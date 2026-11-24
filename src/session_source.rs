//! Bounded, read-only title lookup. Hooks and GUI never wait for Codex metadata.
use crate::{agent::Agent, card::Heading, events::Event};
use std::{
    collections::HashMap,
    sync::{
        Arc, Mutex,
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

/// Both dedicated and shared app-server connections retain the harness-only heading
/// on lookup errors. An unnamed thread is a successful lookup, not a failure.
#[derive(Default)]
pub(crate) struct LookupFailures(bool);
impl LookupFailures {
    pub(crate) fn observe(&mut self, failed: bool) {
        if failed && !self.0 {
            crate::operational_log::log(crate::operational_log::Record::SessionLookupFailure);
        }
        self.0 = failed;
    }
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
    pub fn disabled() -> Self {
        let (_, results) = mpsc::sync_channel(1);
        Self {
            entries: HashMap::new(),
            jobs: None,
            results,
            stop: Arc::new(AtomicBool::new(false)),
            worker: None,
            start: Instant::now(),
        }
    }
    pub fn for_resident(
        config: &crate::config::Config,
        shared: Option<crate::usage_service::NameLookup>,
    ) -> Self {
        Self::for_mode(config, crate::config::Mode::Resident, shared)
    }
    pub fn for_mode(
        config: &crate::config::Config,
        mode: crate::config::Mode,
        shared: Option<crate::usage_service::NameLookup>,
    ) -> Self {
        Self::for_mode_with(
            config,
            mode,
            shared,
            crate::app_server::StdioTransport::codex_controlled,
        )
    }
    pub fn for_mode_with<T: crate::app_server::Transport + Send + 'static>(
        config: &crate::config::Config,
        mode: crate::config::Mode,
        shared: Option<crate::usage_service::NameLookup>,
        connect: impl FnMut(Arc<AtomicBool>) -> Result<T, crate::app_server::Error> + Send + 'static,
    ) -> Self {
        if !config.codex.session_names
            || (mode == crate::config::Mode::Transient && !config.codex.transient_names)
        {
            Self::disabled()
        } else if mode == crate::config::Mode::Resident
            && let Some(lookup) = shared
        {
            Self::shared(lookup)
        } else {
            Self::connect(connect)
        }
    }
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
        Self::disabled()
    }
    /// The Codex-backed transient lookup.
    pub fn transient_codex(enabled: bool) -> Self {
        Self::transient(enabled, crate::app_server::StdioTransport::codex_controlled)
    }
    /// Resolves names over a connection opened on the first lookup. `connect` receives the
    /// flag that cancels a pending read when the service is dropped.
    fn connect<T: crate::app_server::Transport + Send + 'static>(
        connect: impl FnMut(Arc<AtomicBool>) -> Result<T, crate::app_server::Error> + Send + 'static,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        Self::start(codex_lookup(connect, stop.clone()), stop)
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
        let (agent, id) = name_target(event)?;
        if agent == Agent::Claude {
            return claude_name(id);
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

pub(crate) fn name_target(event: &Event) -> Option<(Agent, &str)> {
    let agent = event.source.agent()?;
    let id = event.session_id.as_deref()?;
    event.validate().ok()?;
    Some((agent, id))
}

/// Claude names arrive with statusLine reports and are already sanitized.
pub(crate) fn claude_name(id: &str) -> Option<String> {
    crate::statusline::with_store(|store| store.name(id).map(str::to_owned))
}

/// Creates a lazy read-only lookup for use on a worker thread.
fn codex_lookup<T: crate::app_server::Transport + Send>(
    mut connect: impl FnMut(Arc<AtomicBool>) -> Result<T, crate::app_server::Error> + Send,
    stop: Arc<AtomicBool>,
) -> impl FnMut(&str) -> Option<String> + Send {
    use crate::app_server::{Client, Error};
    let mut client = None;
    let mut failures = LookupFailures::default();
    move |id| {
        let result = (|| {
            if client.is_none() {
                client = Some(Client::initialize(
                    connect(stop.clone())?,
                    Duration::from_millis(750),
                )?);
            }
            let name = client
                .as_mut()
                .ok_or(Error::Disconnected)?
                .read_thread_name(id);
            match name {
                // An unknown thread still leaves a usable connection for the next session.
                Err(Error::Remote(_)) => Ok(None),
                result => result,
            }
        })();
        failures.observe(result.is_err());
        match result {
            Ok(name) => name,
            Err(_) => {
                client = None;
                None
            }
        }
    }
}

type CodexLookup = dyn FnMut(&str) -> Option<String> + Send;

/// One lazy connection shared by the messenger's sink workers.
pub(crate) struct CodexNames {
    lookup: Mutex<Box<CodexLookup>>,
    stop: Arc<AtomicBool>,
}

impl CodexNames {
    pub(crate) fn new<T: crate::app_server::Transport + Send + 'static>(
        connect: impl FnMut(Arc<AtomicBool>) -> Result<T, crate::app_server::Error> + Send + 'static,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        Self {
            lookup: Mutex::new(Box::new(codex_lookup(connect, stop.clone()))),
            stop,
        }
    }

    pub(crate) fn name(&self, id: &str) -> Option<String> {
        let mut lookup = self.lookup.lock().ok()?;
        if self.stop.load(Ordering::Acquire) {
            return None;
        }
        lookup(id).and_then(|name| clean(&name))
    }

    pub(crate) fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
}

pub(crate) fn clean(name: &str) -> Option<String> {
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
