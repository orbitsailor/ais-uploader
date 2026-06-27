use arc_swap::ArcSwap;
use std::{fmt::Write, sync::Arc, time::UNIX_EPOCH};

#[derive(Clone)]
pub struct Formatter {
    latest_gps: Arc<ArcSwap<Option<String>>>,
    add_time_prefix: bool,
}

impl Formatter {
    pub fn new(latest_gps: Arc<ArcSwap<Option<String>>>, add_time_prefix: bool) -> Self {
        Self {
            latest_gps,
            add_time_prefix,
        }
    }

    pub fn process_complete_chunk(&self, chunk: &[u8], output_buf: &mut Vec<Vec<u8>>) {
        let mut prefix = String::new();
        if self.add_time_prefix {
            let current_time = std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("unix epoch to be earlier")
                .as_secs();

            prefix
                .write_fmt(format_args!("{current_time},"))
                .expect("Writing to string cannot fail");
        }

        if let Some(gps_dat) = self.latest_gps.load().as_ref() {
            prefix.push_str(&gps_dat);
            prefix.push(',');
        }

        let prefix = prefix.as_bytes();

        let lines = chunk.split(|c| *c == b'\n').filter_map(|mut line| {
            if let Some(b'\r') = line.last() {
                line = &line[..(line.len() - 1)];
            }
            if line.is_empty() {
                None
            } else {
                Some([prefix, line, b"\n"].concat())
            }
        });

        output_buf.extend(lines);
    }
}
