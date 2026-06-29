use arc_swap::ArcSwap;
use core::iter::Iterator;
use std::{fmt::Write, sync::Arc, time::UNIX_EPOCH};

#[derive(Clone)]
pub struct Formatter {
    latest_gps: Arc<ArcSwap<Option<String>>>,
    add_time_prefix: bool,
    prefix_buffer: String,
}

impl Formatter {
    pub fn new(latest_gps: Arc<ArcSwap<Option<String>>>, add_time_prefix: bool) -> Self {
        Self {
            latest_gps,
            add_time_prefix,
            prefix_buffer: String::new(),
        }
    }

    pub fn process_complete_chunk(&mut self, chunk: &[u8], output_buf: &mut Vec<Vec<u8>>) {
        let prefix = build_prefix(
            &mut self.prefix_buffer,
            self.add_time_prefix,
            self.latest_gps.load().as_deref(),
        )
        .as_bytes();

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

fn build_prefix<'b>(
    prefix: &'b mut String,
    add_time_prefix: bool,
    gps_pos: Option<&str>,
) -> &'b str {
    prefix.clear();
    prefix.push('\\');

    if add_time_prefix {
        let current_time = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("unix epoch to be earlier")
            .as_secs();

        prefix
            .write_fmt(format_args!("c:{current_time}"))
            .expect("Writing to string cannot fail");
    }

    if let Some(gps_pos) = gps_pos {
        prefix.push(',');
        prefix.push_str(gps_pos);
    }

    if prefix.len() > 1 {
        let checksum = calculate_checksum(&prefix[1..].as_bytes());
        prefix
            .write_fmt(format_args!("*{:02X}\\", checksum))
            .expect("write to string cannot fail");

        prefix
    } else {
        ""
    }
}

fn calculate_checksum(data: &[u8]) -> u8 {
    data.iter().fold(0, |cs, b| cs ^ b)
}
