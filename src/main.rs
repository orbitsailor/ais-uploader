mod ais_reformatter;
mod listener;
use std::{error::Error, net::SocketAddr, process::ExitCode};

use clap::Parser;
use reqwest::Url;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::upload::run_upload;

mod upload;

/// This program listens on a tcp/udp port and forwards received AIS data to the configured
/// endpoint.
#[derive(Parser, Debug)]
#[command(version, about)]
struct Args {
    #[arg(short = 'e', long, env = "UPLOAD_ENDPOINT")]
    upload_endpoint: Url,

    #[arg(short, long, env = "AUTH_TOKEN")]
    auth_token: String,

    #[clap(flatten)]
    ports: ListenPorts,

    /// write all messages to be forwarded to standard out in addition to forwarding
    #[arg(short = 'l', long)]
    write_to_stdout: bool,

    /// prefix received lines with the current unix timestamp
    #[arg(short = 'p', long)]
    prefix_current_time: bool,
}

#[derive(Parser, Debug)]
#[group(required = true, multiple = true)]
struct ListenPorts {
    /// listen on the specified udp port for ais messages.
    /// Expect a single AIS message per packet
    #[arg(short = 'u', long)]
    udp_listener: Option<SocketAddr>,

    /// listen on the specified TCP port for ais messages.
    #[arg(short = 't', long)]
    tcp_listener: Option<SocketAddr>,
}

#[tokio::main]
async fn main() -> Result<ExitCode, Box<dyn Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let args = Args::parse();
    debug!("read args: {args:?}");
    let shutdown_token = register_ctrl_c_listener();
    let msg_rx = intitialize_listeners(&args, &shutdown_token).await?;

    let upload_handle = tokio::task::spawn(async move {
        let res = run_upload(
            shutdown_token,
            msg_rx,
            args.upload_endpoint,
            args.auth_token.into(),
            args.write_to_stdout,
        )
        .await;
        info!("Upload task done");
        res
    });

    if let Err(e) = upload_handle.await.unwrap() {
        error!("Upload failed with result: {e:?}");
        return Ok(ExitCode::FAILURE);
    }

    Ok(ExitCode::SUCCESS)
}

async fn intitialize_listeners(
    args: &Args,
    shutdown_token: &CancellationToken,
) -> Result<mpsc::Receiver<Vec<u8>>, Box<dyn Error>> {
    let (msg_tx, msg_rx) = tokio::sync::mpsc::channel(4096);
    let prefix_time = args.prefix_current_time;

    if let Some(addr) = args.ports.udp_listener {
        let socket = tokio::net::UdpSocket::bind(addr).await?;
        info!("listening on UDP addr {addr}");
        let msg_tx = msg_tx.clone();
        let shutdown_token = shutdown_token.clone();

        tokio::task::spawn(async move {
            let res = shutdown_token
                .run_until_cancelled(listener::run_udp_listener(socket, msg_tx, prefix_time))
                .await;
            info!("Udp listener exited with result: {res:?}");
        });
    }

    if let Some(addr) = args.ports.tcp_listener {
        let socket = tokio::net::TcpListener::bind(addr).await?;
        info!("listening on TCP addr {addr}");

        let shutdown_token = shutdown_token.clone();
        tokio::task::spawn(async move {
            let res = listener::run_tcp_listener(socket, msg_tx, shutdown_token, prefix_time).await;
            info!("tcp listener exited with result {res:?}");
        });
    }

    Ok(msg_rx)
}

fn register_ctrl_c_listener() -> CancellationToken {
    let shutdown_token = CancellationToken::new();
    let cloned_token = shutdown_token.clone();
    _ = tokio::task::spawn(async move {
        info!("Set up ctrl_c handler");
        tokio::signal::ctrl_c()
            .await
            .expect("could not set up exit handler");
        info!("Shutdown requested");
        cloned_token.cancel();
    });

    shutdown_token
}
