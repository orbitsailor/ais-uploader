use std::{convert::Infallible, io::Write, pin::pin, sync::Arc, time::Duration};

use reqwest::{Body, Client, Method, Request, RequestBuilder, Url};
use tokio::{sync::mpsc, time::Instant};
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

pub async fn run_upload(
    shutdown_token: CancellationToken,
    mut message_rx: mpsc::Receiver<Vec<u8>>,
    url: Url,
    auth_token: Arc<str>,
    write_to_stdout: bool,
) -> Result<(), Box<dyn std::error::Error + Send>> {
    let mut recovered_msg = None;
    let client = Client::new();

    let mut final_attempt_at_shutdown_done = false;

    loop {
        if final_attempt_at_shutdown_done {
            info!("Now respecting shutdown request over upload retry");
            return Ok(());
        } else if shutdown_token.is_cancelled() {
            info!("shutdown source triggered, making a final upload attempt");
            final_attempt_at_shutdown_done = true;
        }

        let (upload_tx, upload_rx) = tokio::sync::mpsc::channel(1);
        let mut upload_task = pin!(tokio::task::spawn(run_single_request(
            client.clone(),
            upload_rx,
            url.clone(),
            auth_token.clone()
        )));
        let upload_start_instant = Instant::now();

        let mut recycle_timeout = pin!(tokio::time::sleep(Duration::from_secs(55)));

        if let Some(msg) = recovered_msg.take() {
            upload_tx
                .send(msg)
                .await
                .expect("new channel should have space for at least 1 msg");
        }

        loop {
            tokio::select! {
                msg_result = message_rx.recv() => {
                    if let Some(msg) = msg_result {
                        if write_to_stdout {
                            _ = std::io::stdout().write_all(&msg);
                        }

                        if let Err(er) = upload_tx.send(Ok(msg)).await {
                            info!("channel broken.");
                            recovered_msg = Some(er.0);
                            // the channel was broken.
                            if let Err(e) = upload_task.await.box_err() {
                                error!("error uploading: {e}");
                                wait_for_next_retry(upload_start_instant, &shutdown_token).await;
                            }

                            break;
                        }
                    }
                    else {
                        info!("upload stream source shut down, stopping upload");
                        drop(upload_tx);
                        return upload_task.await.box_err()?;
                    }
                },
                _ = &mut recycle_timeout => {
                    info!("recycling connection");
                    drop(upload_tx);
                    if let Err(e) = upload_task.await.box_err()? {
                        error!("error uploading: {e}");
                        wait_for_next_retry(upload_start_instant, &shutdown_token).await;
                    }
                    break;
                }
                upload_result = &mut upload_task => {
                    debug!("{upload_result:?}");
                    if let Err(e) = upload_result.box_err()? {
                        error!("error uploading: {e}");
                        wait_for_next_retry(upload_start_instant, &shutdown_token).await;
                    }
                    break;
                }
            }
        }
    }
}

async fn wait_for_next_retry(op_start_time: Instant, cancel_token: &CancellationToken) {
    let wait_deadline = op_start_time + Duration::from_secs(15);
    if wait_deadline > Instant::now() {
        info!("waiting until {wait_deadline:?} before retrying");
        cancel_token
            .run_until_cancelled(tokio::time::sleep_until(wait_deadline))
            .await;
    }
}

async fn run_single_request(
    client: Client,
    message_rx: mpsc::Receiver<Result<Vec<u8>, Infallible>>,
    url: Url,
    auth_token: Arc<str>,
) -> Result<(), Box<dyn std::error::Error + Send + 'static>> {
    let (client, req_r) = RequestBuilder::from_parts(client, Request::new(Method::POST, url))
        .bearer_auth(auth_token)
        .body(Body::wrap_stream(ReceiverStream::new(message_rx)))
        .build_split();
    let req = req_r.expect("Failed to build request");

    info!("launching upload connection");

    let res = tokio::time::timeout(Duration::from_secs(90), client.execute(req))
        .await
        .box_err()
        .inspect_err(|_| warn!("Request deadline expired!"))?
        .and_then(|r| r.error_for_status())
        .box_err()?;

    info!("upload finished cleanly with status {res:?}");

    Ok(())
}

trait BoxErr<T, R> {
    fn box_err(self: Self) -> Result<T, Box<dyn std::error::Error + Send + 'static>>;
}

impl<T, R> BoxErr<T, R> for Result<T, R>
where
    R: std::error::Error + Send + 'static,
{
    fn box_err(self: Self) -> Result<T, Box<dyn std::error::Error + Send + 'static>> {
        self.map_err(|e| {
            let v: Box<dyn std::error::Error + Send + 'static> = Box::new(e);
            v
        })
    }
}
