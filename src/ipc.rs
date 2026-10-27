//! Bounded, versioned local wire frames. No command or arbitrary payload execution.
use serde::{Deserialize, Serialize, de::DeserializeOwned};
pub const MAX_FRAME: usize = 4096;
const VERSION: u16 = 1;
pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(value).map_err(|_| "cannot encode IPC message")?;
    if bytes.is_empty() || bytes.len() > MAX_FRAME {
        return Err("IPC message exceeds limit".into());
    }
    let mut out = Vec::with_capacity(bytes.len() + 6);
    out.extend(VERSION.to_le_bytes());
    out.extend((bytes.len() as u32).to_le_bytes());
    out.extend(bytes);
    Ok(out)
}
pub fn frame_length(header: &[u8]) -> Result<usize, String> {
    if header.len() != 6 || u16::from_le_bytes([header[0], header[1]]) != VERSION {
        return Err("incompatible IPC version".into());
    }
    let length = u32::from_le_bytes([header[2], header[3], header[4], header[5]]) as usize;
    if length == 0 || length > MAX_FRAME {
        return Err("invalid IPC length".into());
    }
    Ok(length)
}
pub fn decode<T: DeserializeOwned>(frame: &[u8]) -> Result<T, String> {
    let header = frame.get(..6).ok_or("truncated IPC header")?;
    let length = frame_length(header)?;
    if frame.len() != length + 6 {
        return Err("invalid IPC frame length".into());
    }
    serde_json::from_slice(&frame[6..]).map_err(|_| "invalid IPC message".into())
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    Event { event: crate::events::Event },
    Status,
    Quit,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case", deny_unknown_fields)]
pub enum Response {
    Accepted,
    Status {
        pid: u32,
        queued: usize,
        paused: bool,
        sounds: bool,
        visuals: bool,
    },
    Bye,
    Rejected,
}
#[cfg(windows)]
#[path = "pipe.rs"]
pub mod pipe;
#[cfg(not(windows))]
pub use crate::unsupported::pipe;
