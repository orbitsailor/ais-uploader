use tokio::sync::mpsc::Sender;

use crate::ais_reformatter::Formatter;

pub async fn run_udp_listener(
    socket: tokio::net::UdpSocket,
    msg_tx: Sender<Vec<u8>>,
    mut formatter: Formatter,
) -> Result<(), std::io::Error> {
    let mut buf = vec![0u8; 65_535].into_boxed_slice();

    let mut line_buf = Vec::new();

    loop {
        let num_bytes = socket.recv(&mut buf).await?;
        formatter.process_complete_chunk(&buf[..num_bytes], &mut line_buf);
        for line in line_buf.drain(..) {
            msg_tx
                .send(line)
                .await
                .expect("channel closed unexpectedly");
        }
    }
}
