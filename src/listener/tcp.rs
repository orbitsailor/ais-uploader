use std::error::Error;

use tokio::{io::AsyncReadExt, net::TcpStream, sync::mpsc::Sender};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

use crate::ais_reformatter::Formatter;

pub async fn run_tcp_listener(
    socket: tokio::net::TcpListener,
    msg_tx: Sender<Vec<u8>>,
    shutdown_token: CancellationToken,
    formatter: Formatter,
) -> Result<(), Box<dyn Error>> {
    loop {
        tokio::select! {
            _ = shutdown_token.cancelled() => {
                break;
            },
            accept_res = socket.accept() => {
                let (conn, peer_addr) = accept_res?;
                info!("accepted TCP connection for peer {peer_addr}");
                let shutdown_token = shutdown_token.clone();
                let msg_tx = msg_tx.clone();
                let formatter = formatter.clone();
                tokio::task::spawn(async move {
                    let res = shutdown_token.run_until_cancelled(process_tcp_stream(conn, msg_tx, formatter)).await;
                    info!("connection to peer {peer_addr} terminated with result {res:?}");
                });
            }
        }
    }

    Ok(())
}

async fn process_tcp_stream(
    mut conn: TcpStream,
    msg_tx: Sender<Vec<u8>>,
    mut formatter: Formatter,
) -> Result<(), Box<dyn Error>> {
    let mut buf = vec![0u8; 4096].into_boxed_slice();
    let mut previously_filled_bytes = 0;
    let mut line_buf = Vec::new();

    loop {
        let free_space = &mut buf[previously_filled_bytes..];
        if free_space.is_empty() {
            error!("Too large line received, receive buffer full. Aborting connection.");
            break;
        }

        let bytes_read = conn.read(free_space).await?;
        if bytes_read == 0 {
            break;
        }

        let valid_part = &buf[..(previously_filled_bytes + bytes_read)];
        if let Some(last_newline_idx) = valid_part.iter().rposition(|e| *e == b'\n') {
            let completed_part = &valid_part[..last_newline_idx];
            formatter.process_complete_chunk(completed_part, &mut line_buf);
            for line in line_buf.drain(..) {
                msg_tx
                    .send(line)
                    .await
                    .expect("channel closed unexpectedly");
            }

            let incomplete_part = (completed_part.len() + 1)..valid_part.len();
            previously_filled_bytes = incomplete_part.len();
            buf.copy_within(incomplete_part, 0);
        } else {
            // no newlines found. Buffer whatever we read too.
            previously_filled_bytes += bytes_read;
        }
    }

    Ok(())
}
