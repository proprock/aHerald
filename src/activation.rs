//! Notification click policy; external operations are isolated from event data.
use crate::events::{Event, Source};
pub trait Agent {
    fn open_thread(&mut self, url: &str) -> Result<(), String>;
    fn activate_window(&mut self) -> Result<(), String>;
}
pub fn click(
    event: &Event,
    enabled: bool,
    body: bool,
    agent: &mut impl Agent,
) -> Result<(), String> {
    if !enabled || !body || event.source != Source::CodexHook {
        return Ok(());
    }
    if event.validate().is_ok()
        && let Some(id) = &event.session_id
    {
        let mut url = String::from("codex://threads/");
        for b in id.bytes() {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                url.push(b as char);
            } else {
                use std::fmt::Write;
                let _ = write!(url, "%{b:02X}");
            }
        }
        if agent.open_thread(&url).is_ok() {
            return Ok(());
        }
    }
    agent.activate_window()
}

#[cfg(windows)]
mod native;
#[cfg(windows)]
pub use native::Worker;
