//! Notification click policy; external operations are isolated from event data.
use crate::events::Event;
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
    match event.source.agent() {
        Some(crate::agent::Agent::Codex) => click_codex(event, agent),
        Some(crate::agent::Agent::Claude) => click_claude(event, agent),
        None => Ok(()),
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
/// A recorded window goes first: the Desktop lookup scans every session file and a terminal
/// session never matches, so it cost seconds on a cold disk cache.
fn click_claude(event: &Event, agent: &mut impl Agent) -> Result<(), String> {
    let focus = event.window.map(|window| agent.focus_window(window));
    if matches!(focus, Some(Ok(()))) {
        return Ok(());
    }
    let desktop = valid_session(event)
        .and_then(|id| agent.desktop_session(id))
        .filter(|local| claude_desktop::is_local_id(local));
    if let Some(local) = desktop {
        return match agent.open_thread(&format!("claude://claude.ai/epitaxy/{local}")) {
            Ok(()) => Ok(()),
            Err(_) => agent.activate_window(),
        };
    }
    focus.unwrap_or_else(|| Err("agent_window_unavailable".into()))
}

#[cfg(windows)]
mod native;

#[cfg(target_os = "linux")]
use crate::linux::activation::run;
#[cfg(target_os = "macos")]
use crate::mac::activation::run;
/// What a click on a card of `event` does with the platform's real agents.
#[cfg(windows)]
use native::run;

/// One owned worker and one pending click; the GUI never waits for shell or process queries.
pub struct Worker {
    runner: fn(&Event) -> Result<(), String>,
    sender: Option<std::sync::mpsc::SyncSender<(Event, std::time::Instant)>>,
    handle: Option<std::thread::JoinHandle<()>>,
}

#[cfg(any(windows, target_os = "macos", target_os = "linux"))]
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
        directory: &std::path::Path,
        level: crate::config::LogLevel,
    ) {
        use crate::operational_log::{Record, log};
        if !enabled || !body || event.source.agent().is_none() {
            return;
        }
        if self.sender.is_none() {
            let (tx, rx) = std::sync::mpsc::sync_channel::<(Event, std::time::Instant)>(1);
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
            let _ = tx.try_send((event.clone(), std::time::Instant::now()));
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
mod worker_tests {
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
