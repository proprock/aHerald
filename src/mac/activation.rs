//! Click-to-activate on macOS: raises the agent's app or terminal, or opens its thread.
//!
//! The policy (what a click on which event does) is the platform-neutral `activation::click`;
//! this module supplies the macOS agents and the worker that keeps the GUI from waiting for
//! LaunchServices or the session files.
use super::terminal;
use crate::{
    activation::{Agent, claude_desktop, click},
    events::Event,
    operational_log::{Record, log},
};
use std::{path::Path, sync::mpsc, thread::JoinHandle, time::Instant};

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
fn run(event: &Event) -> Result<(), String> {
    match event.source.agent() {
        Some(crate::agent::Agent::Codex) => click(event, true, true, &mut Codex),
        Some(crate::agent::Agent::Claude) => click(event, true, true, &mut Claude),
        None => Ok(()),
    }
}

/// One owned worker and one pending click; the GUI never waits for LaunchServices or the disk.
pub struct Worker {
    runner: fn(&Event) -> Result<(), String>,
    sender: Option<mpsc::SyncSender<(Event, Instant)>>,
    handle: Option<JoinHandle<()>>,
}

impl Default for Worker {
    fn default() -> Self {
        Self::with(run)
    }
}

impl Worker {
    /// A worker that performs clicks with `runner`; tests pass a fake.
    pub fn with(runner: fn(&Event) -> Result<(), String>) -> Self {
        Self {
            runner,
            sender: None,
            handle: None,
        }
    }

    /// Queues the click for the worker. Nothing happens unless activation is `enabled`, the
    /// click hit the card's `body` and the event comes from a supported agent; a click that
    /// arrives while another is still pending is dropped.
    pub fn submit(
        &mut self,
        event: &Event,
        enabled: bool,
        body: bool,
        directory: &Path,
        level: crate::config::LogLevel,
    ) {
        if !enabled || !body || event.source.agent().is_none() {
            return;
        }
        if self.sender.is_none() {
            let (tx, rx) = mpsc::sync_channel::<(Event, Instant)>(1);
            crate::operational_log::init(directory, level);
            let runner = self.runner;
            match std::thread::Builder::new()
                .name("agent-activation".into())
                .spawn(move || {
                    for (event, clicked) in rx {
                        log(if runner(&event).is_err() {
                            Record::AgentActivationFailure
                        } else {
                            Record::AgentActivated {
                                millis: clicked.elapsed().as_millis() as u64,
                            }
                        });
                    }
                }) {
                Ok(handle) => {
                    self.sender = Some(tx);
                    self.handle = Some(handle);
                }
                Err(_) => log(Record::AgentActivationFailure),
            }
        }
        if let Some(tx) = &self.sender {
            let _ = tx.try_send((event.clone(), Instant::now()));
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.sender.take();
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::LogLevel, events::Source};
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };

    static CLICKS: AtomicUsize = AtomicUsize::new(0);
    static SEEN: Mutex<Vec<String>> = Mutex::new(Vec::new());

    fn count(event: &Event) -> Result<(), String> {
        SEEN.lock().unwrap().push(event.kind.name().to_owned());
        CLICKS.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
    fn event(source: Source) -> Event {
        let mut event = Event::demo("turn_complete").unwrap();
        event.source = source;
        event
    }

    /// One test only: the runner is a plain function, so its counter is shared.
    #[test]
    fn a_click_reaches_the_runner_only_for_an_enabled_body_click_on_an_agent_event() {
        let directory = std::env::temp_dir();
        let mut worker = Worker::with(count);
        worker.submit(
            &event(Source::CodexHook),
            false,
            true,
            &directory,
            LogLevel::Off,
        );
        worker.submit(
            &event(Source::CodexHook),
            true,
            false,
            &directory,
            LogLevel::Off,
        );
        worker.submit(&event(Source::Demo), true, true, &directory, LogLevel::Off);
        worker.submit(&event(Source::Usage), true, true, &directory, LogLevel::Off);
        drop(worker);
        assert_eq!(CLICKS.load(Ordering::SeqCst), 0);

        let mut worker = Worker::with(count);
        worker.submit(
            &event(Source::ClaudeHook),
            true,
            true,
            &directory,
            LogLevel::Off,
        );
        // Dropping the worker lets the pending click finish.
        drop(worker);
        assert_eq!(CLICKS.load(Ordering::SeqCst), 1);
        assert_eq!(SEEN.lock().unwrap().as_slice(), ["turn_complete"]);
    }
}
