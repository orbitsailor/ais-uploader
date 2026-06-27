use std::collections::VecDeque;

use serde::Deserialize;

use super::*;

// see https://manpages.ubuntu.com/manpages/noble/man5/gpsd_json.5.html#core-protocol-responses
// for the protcol types

#[derive(PartialEq, Eq)]
enum State {
    Initial,
    ReadyToRequestWatch,
    ReceivingData,
}

pub struct Protocol {
    incomplete_data: Vec<u8>,
    messages: VecDeque<GpsMessage>,
    state: State,
}

impl TryFrom<TPVMessage> for GpsMessage {
    type Error = ();

    fn try_from(value: TPVMessage) -> Result<Self, Self::Error> {
        // let time = value.time.ok_or(())?;
        // let mode = match value.mode {
        //     2 => FixMode::Fix2D,
        //     3 => FixMode::Fix3D,
        //     _ => return Err(()),
        // };
        let lat = value.lat.ok_or(())?;
        let lon = value.lon.ok_or(())?;

        let error_estimate = value.epx.and_then(|epx| {
            value.exy.map(|epy| ErrorEstimate {
                longitude_estimate_meters: epx,
                latitude_estimate_meters: epy,
            })
        });

        Ok(GpsMessage {
            // status: value.status,
            // mode,
            // unix_time: time.unix_timestamp(),
            lat,
            lon,
            error_estimate,
        })
    }
}

#[derive(Deserialize)]
struct Typefinder<'src> {
    class: &'src str,
}

#[derive(Deserialize, Debug)]
pub(super) struct VersionMessage {
    proto_major: u32,
    proto_minor: u32,
}

/// A GPS position fix message. Not all fields are mapped.
#[derive(Deserialize)]
struct TPVMessage {
    // status: Option<u16>,
    // mode: u16,
    // time: Option<UtcDateTime>,
    /// latitude in degrees. -90 to +90
    lat: Option<f32>,
    /// longitude in degrees, -180 to +180
    lon: Option<f32>,
    /// longitude error estimate in meters
    epx: Option<f32>,
    /// latitude error estimate in meters
    exy: Option<f32>,
}

#[derive(thiserror::Error, Debug)]
pub enum GpsdProtoError {
    #[error("Error parsing json")]
    Json(#[from] serde_json::Error),
    #[error("Unsupported protocol version: {0:?}")]
    UnsupportedProtocolVersion(VersionMessage),
}

impl Protocol {
    pub fn new() -> Self {
        Self {
            incomplete_data: Default::default(),
            messages: Default::default(),
            state: State::Initial,
        }
    }

    pub fn push_data(&mut self, data: &[u8]) -> Result<(), GpsdProtoError> {
        let mut rest = data;

        if !self.incomplete_data.is_empty() {
            if let (Some(first_line), next_rest) = split_at_newline(data) {
                rest = next_rest;
                // extend the buffered data and process that.
                self.incomplete_data.extend_from_slice(first_line);
                // temporarily grab the vec out because of ownership rules
                let mut vec = core::mem::take(&mut self.incomplete_data);
                self.process_proto_line(&vec)?;
                vec.clear();
                self.incomplete_data = vec;
            }
        }

        while let (Some(current_line), next_rest) = split_at_newline(rest) {
            self.process_proto_line(current_line)?;
            rest = next_rest;
        }

        // not enough data anymore to continue parsing
        self.incomplete_data.extend_from_slice(rest);

        Ok(())
    }

    fn process_proto_line(&mut self, line: &[u8]) -> Result<(), GpsdProtoError> {
        match serde_json::from_slice::<Typefinder>(line)?.class {
            "VERSION" => self.process_version_msg(line),
            "TPV" => self.process_tpv(line),
            _ => Ok(()), // ignore unimplemented classes
        }
    }

    pub fn poll_write(&mut self) -> Option<&[u8]> {
        if self.state == State::ReadyToRequestWatch {
            self.state = State::ReceivingData;
            Some(concat!(r#"?WATCH={"enable":true,"json":true}"#, "\n").as_bytes())
        } else {
            None
        }
    }

    pub fn poll_output(&mut self) -> Option<GpsMessage> {
        self.messages.pop_front()
    }

    fn process_version_msg(&mut self, line: &[u8]) -> Result<(), GpsdProtoError> {
        let version: VersionMessage = serde_json::from_slice(line)?;
        if version.proto_major != 3 || version.proto_minor < 2 {
            return Err(GpsdProtoError::UnsupportedProtocolVersion(version));
        }

        self.state = State::ReadyToRequestWatch;

        Ok(())
    }

    fn process_tpv(&mut self, line: &[u8]) -> Result<(), GpsdProtoError> {
        let msg: TPVMessage = serde_json::from_slice(line)?;
        if let Some(gps) = msg.try_into().ok() {
            self.messages.push_back(gps);
        }
        Ok(())
    }
}

fn split_at_newline(input: &[u8]) -> (Option<&[u8]>, &[u8]) {
    let idx = input.windows(2).enumerate().find_map(|(idx, w)| {
        if w == "\r\n".as_bytes() {
            Some(idx)
        } else {
            None
        }
    });

    if let Some(idx) = idx {
        (Some(&input[..idx]), &input[(idx + 2)..])
    } else {
        // nothing found.
        (None, input)
    }
}
