#[cfg(feature = "gpsd_client")]
mod protocol;

#[derive(Clone)]
pub struct ErrorEstimate {
    pub longitude_estimate_meters: f32,
    pub latitude_estimate_meters: f32,
}
// #[derive(Clone)]
// enum FixMode {
//     Fix2D,
//     Fix3D,
// }

#[derive(Clone)]
struct GpsMessage {
    // status: Option<u16>,
    // mode: FixMode,
    // unix_time: i64,
    /// latitude in degrees. -90 to +90
    pub lat: f32,
    /// longitude in degrees, -180 to +180
    pub lon: f32,

    pub error_estimate: Option<ErrorEstimate>,
}

#[cfg(feature = "gpsd_client")]
pub use driver::run_client;

#[cfg(feature = "gpsd_client")]
mod driver {
    use std::{fmt::Write, net::SocketAddr, sync::Arc, time::Duration};

    use arc_swap::ArcSwap;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        time::Instant,
    };
    use tracing::{info, warn};

    use super::GpsMessage;

    pub async fn run_client(addr: &SocketAddr, output: Arc<ArcSwap<Option<String>>>) {
        loop {
            let next_attempt = Instant::now() + Duration::from_secs(10);

            if let Err(e) = run_single_connection(addr, &output).await {
                warn!("Error processing the GPS: {e}")
            }

            if Instant::now() < next_attempt {
                tokio::time::sleep_until(next_attempt).await;
            }
        }
    }

    async fn run_single_connection(
        addr: &SocketAddr,
        output: &ArcSwap<Option<String>>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        info!("Connecting to GPS at {addr}");
        let mut conn = tokio::net::TcpStream::connect(addr).await?;

        info!("GPS connected, running proto");
        let mut proto = super::protocol::Protocol::new();

        let mut read_buf: Box<[u8]> = vec![0; 4096].into();

        loop {
            if let Some(data) = proto.poll_write() {
                conn.write_all(data).await?;
            }

            let read_bytes = conn.read(&mut read_buf).await?;
            if read_bytes == 0 {
                return Ok(());
            }
            proto.push_data(&read_buf[..read_bytes])?;

            if let Some(mut msg) = proto.poll_output() {
                while let Some(later) = proto.poll_output() {
                    msg = later;
                }
                output.store(Arc::new(Some(format_gps(msg))));
            }
        }
    }

    fn format_gps(msg: GpsMessage) -> String {
        let mut res = format!("l:{};{}", msg.lon, msg.lat);
        if let Some(err) = msg.error_estimate {
            res.write_fmt(format_args!(
                ";err:{}",
                f32::max(err.longitude_estimate_meters, err.latitude_estimate_meters)
            ))
            .expect("writing to string cannot fail");
        }
        res
    }
}
