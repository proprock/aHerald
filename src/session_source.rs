//! Bounded, read-only title lookup. Hooks and GUI never wait for Codex metadata.
use crate::{card::Heading, events::Event};
use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

struct Entry {
    name: Option<String>,
    refresh_at: Duration,
    pending: bool,
}
pub struct Service {
    entries: HashMap<String, Entry>,
    jobs: Option<mpsc::SyncSender<String>>,
    results: mpsc::Receiver<(String, Option<String>)>,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    start: Instant,
}
impl Service {
    pub fn new(resolve: impl FnMut(&str) -> Option<String> + Send + 'static) -> Self {
        Self::start(resolve, Arc::new(AtomicBool::new(false)))
    }
    fn start(
        mut resolve: impl FnMut(&str) -> Option<String> + Send + 'static,
        stop: Arc<AtomicBool>,
    ) -> Self {
        let (jobs, incoming) = mpsc::sync_channel::<String>(32);
        let (outgoing, results) = mpsc::sync_channel(32);
        let done = stop.clone();
        let worker = std::thread::spawn(move || {
            while !done.load(Ordering::Acquire) {
                let Ok(id) = incoming.recv() else { break };
                if done.load(Ordering::Acquire) {
                    break;
                }
                let name = resolve(&id).and_then(|s| clean(&s));
                // Bounded output backpressure is released by GUI polling or Drop.
                if outgoing.send((id, name)).is_err() {
                    break;
                }
            }
        });
        Self {
            entries: HashMap::new(),
            jobs: Some(jobs),
            results,
            stop,
            worker: Some(worker),
            start: Instant::now(),
        }
    }
    pub fn codex() -> Self {
        Self::connect(crate::app_server::StdioTransport::codex_controlled)
    }
    /// Names for a process that shows a single card. Looking one up starts a `codex
    /// app-server` child, so unless `enabled` the card keeps the session-ID label and no
    /// child or worker thread is created.
    pub fn transient<T: crate::app_server::Transport + Send + 'static>(
        enabled: bool,
        connect: impl FnMut(Arc<AtomicBool>) -> Result<T, crate::app_server::Error> + Send + 'static,
    ) -> Self {
        if enabled {
            return Self::connect(connect);
        }
        let mut service = Self::new(|_| None);
        // Without a job sender no lookup is queued; the worker sees a closed channel and ends.
        service.jobs.take();
        service
    }
    /// The Codex-backed `transient` used by the overlay.
    pub fn transient_codex(enabled: bool) -> Self {
        Self::transient(enabled, crate::app_server::StdioTransport::codex_controlled)
    }
    /// Resolves names over a connection opened on the first lookup. `connect` receives the
    /// flag that cancels a pending read when the service is dropped.
    fn connect<T: crate::app_server::Transport + Send + 'static>(
        mut connect: impl FnMut(Arc<AtomicBool>) -> Result<T, crate::app_server::Error> + Send + 'static,
    ) -> Self {
        use crate::app_server::Client;
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = stop.clone();
        let mut client = None;
        Self::start(
            move |id| {
                if client.is_none() {
                    client = connect(cancel.clone())
                        .ok()
                        .and_then(|t| Client::initialize(t, Duration::from_millis(750)).ok());
                }
                match client.as_mut()?.read_thread_name(id) {
                    Ok(name) => name,
                    Err(_) => {
                        client = None;
                        None
                    }
                }
            },
            stop,
        )
    }
    /// Names come from the usage worker's `codex app-server` connection, so no second
    /// child process runs. Only useful while Codex usage polling is enabled.
    pub fn shared(lookup: crate::usage_service::NameLookup) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = stop.clone();
        Self::start(move |id| lookup.name(id, &cancel), stop)
    }
    pub fn label(&mut self, event: &Event) -> Heading {
        self.label_at(event, self.start.elapsed())
    }
    /// The card heading: `<harness>[, <folder>]` and the stored session name once known.
    pub fn label_at(&mut self, event: &Event, now: Duration) -> Heading {
        // An aHerald notice has no session: its message takes the place of the session name.
        if event.source == crate::events::Source::App {
            return Heading::notice(event.message.as_deref().unwrap_or_default());
        }
        let name = self.name_at(event, now);
        Heading::new(
            event.source.label(),
            event.folder.as_deref(),
            name.as_deref(),
        )
    }
    fn name_at(&mut self, event: &Event, now: Duration) -> Option<String> {
        while let Ok((id, name)) = self.results.try_recv() {
            if let Some(entry) = self.entries.get_mut(&id) {
                entry.pending = false;
                entry.refresh_at = now + Duration::from_secs(60);
                entry.name = name;
            }
        }
        if !event
            .source
            .agent()
            .is_some_and(|agent| agent.names_sessions())
        {
            return None;
        }
        let id = event.session_id.as_deref()?;
        if event.validate().is_err() {
            return None;
        }
        if !self.entries.contains_key(id) && self.entries.len() >= 64 {
            // Do not evict in-flight work: a later result must never label another session.
            let oldest = self
                .entries
                .iter()
                .filter(|(_, v)| !v.pending)
                .min_by_key(|(_, v)| v.refresh_at)
                .map(|(k, _)| k.clone())?;
            self.entries.remove(&oldest);
        }
        let entry = self.entries.entry(id.into()).or_insert(Entry {
            name: None,
            refresh_at: Duration::ZERO,
            pending: false,
        });
        if !entry.pending
            && now >= entry.refresh_at
            && self
                .jobs
                .as_ref()
                .is_some_and(|tx| tx.try_send(id.into()).is_ok())
        {
            entry.pending = true;
        }
        entry.name.clone()
    }
}
fn clean(name: &str) -> Option<String> {
    let text: String = name
        .chars()
        .filter(|c| !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let text: String = text.chars().take(120).collect();
    (!text.is_empty()).then_some(text)
}
impl Drop for Service {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.jobs.take();
        // Unblock a result producer before joining; controlled transport cancellation
        // also releases a pending metadata read without touching other Codex processes.
        while let Some(worker) = &self.worker {
            if worker.is_finished() {
                break;
            }
            let _ = self.results.recv_timeout(Duration::from_millis(10));
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
