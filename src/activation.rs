//! Notification click policy; external operations are isolated from event data.
use crate::events::{Event, Source};
pub mod claude_desktop;

pub trait Agent {
    fn open_thread(&mut self, url: &str) -> Result<(), String>;
    fn activate_window(&mut self) -> Result<(), String>;
    /// Claude Desktop's own session id (`local_<uuid>`) for a hook `session_id`, if it hosts one.
    fn desktop_session(&mut self, _cli_session_id: &str) -> Option<String> {
        None
    }
    /// Raises a terminal window recorded by the hook process.
    fn focus_window(&mut self, _window: u64) -> Result<(), String> {
        Err("agent_window_unavailable".into())
    }
}
pub fn click(
    event: &Event,
    enabled: bool,
    body: bool,
    agent: &mut impl Agent,
) -> Result<(), String> {
    if !enabled || !body {
        return Ok(());
    }
    match event.source {
        Source::CodexHook => click_codex(event, agent),
        Source::ClaudeHook => click_claude(event, agent),
        Source::Usage | Source::Demo => Ok(()),
    }
}
fn valid_session(event: &Event) -> Option<&str> {
    event.validate().ok().and(event.session_id.as_deref())
}
fn click_codex(event: &Event, agent: &mut impl Agent) -> Result<(), String> {
    if let Some(id) = valid_session(event) {
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
/// Desktop sessions have an app link; terminal sessions only have the window their hook ran in.
/// Without either, nothing is guessed: raising an unrelated Claude window would mislead.
fn click_claude(event: &Event, agent: &mut impl Agent) -> Result<(), String> {
    let desktop = valid_session(event)
        .and_then(|id| agent.desktop_session(id))
        .filter(|local| claude_desktop::is_local_id(local));
    if let Some(local) = desktop {
        return match agent.open_thread(&format!("claude://claude.ai/epitaxy/{local}")) {
            Ok(()) => Ok(()),
            Err(_) => agent.activate_window(),
        };
    }
    match event.window {
        Some(window) => agent.focus_window(window),
        None => Err("agent_window_unavailable".into()),
    }
}

#[cfg(windows)]
mod native;
#[cfg(windows)]
pub use native::Worker;
