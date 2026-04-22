use tokio::sync::mpsc::Sender;

use crate::ais_reformatter::process_complete_chunk;

pub async fn run_udp_listener(
    socket: tokio::net::UdpSocket,
    msg_tx: Sender<Vec<u8>>,
    add_time_prefix: bool,
) -> Result<(), std::io::Error> {
    let mut buf = vec![0u8; 65_535].into_boxed_slice();

    loop {
        let num_bytes = socket.recv(&mut buf).await?;
        let chunk = process_complete_chunk(&buf[..num_bytes], add_time_prefix);
        msg_tx
            .send(chunk)
            .await
            .expect("channel closed unexpectedly");
    }
}
