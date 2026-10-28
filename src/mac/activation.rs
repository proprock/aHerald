//! Click-to-activate on macOS: raises the agent's app or terminal, or opens its thread.
//!
//! The policy (what a click on which event does) is the platform-neutral `activation::click`;
//! this module supplies the macOS agents; `activation::Worker` keeps the GUI from waiting for
//! LaunchServices or the session files.
use super::terminal;
use crate::{
    activation::{Agent, claude_desktop, click},
    events::Event,
};

const CODEX_BUNDLE: &str = "com.openai.codex";
const CLAUDE_BUNDLE: &str = "com.anthropic.claudefordesktop";

fn open_link(url: &str) -> Result<(), String> {
    crate::shell::open(std::ffi::OsStr::new(url)).map_err(|_| "agent_link_failed".into())
}

struct Codex;
impl Agent for Codex {
    fn open_thread(&mut self, url: &str) -> Result<(), String> {
        open_link(url)
    }
    fn activate_window(&mut self) -> Result<(), String> {
        terminal::open_bundle(CODEX_BUNDLE)
    }
}

struct Claude;
impl Agent for Claude {
    fn open_thread(&mut self, url: &str) -> Result<(), String> {
        open_link(url)
    }
    fn activate_window(&mut self) -> Result<(), String> {
        terminal::open_bundle(CLAUDE_BUNDLE)
    }
    fn desktop_session(&mut self, cli_session_id: &str) -> Option<String> {
        let home = std::env::var_os("HOME").map(std::path::PathBuf::from)?;
        terminal::claude_session_roots(&home)
            .iter()
            .find_map(|root| claude_desktop::local_session_id(root, cli_session_id))
    }
    fn focus_window(&mut self, window: u64) -> Result<(), String> {
        terminal::raise(window)
    }
}

/// What a click on a card of `event` does with the real agents.
pub fn run(event: &Event) -> Result<(), String> {
    match event.source.agent() {
        Some(crate::agent::Agent::Codex) => click(event, true, true, &mut Codex),
        Some(crate::agent::Agent::Claude) => click(event, true, true, &mut Claude),
        None => Ok(()),
    }
}
