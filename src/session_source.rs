//! Bounded, read-only title lookup. Hooks and GUI never wait for Codex metadata.
use crate::events::{Event, Source};
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
        use crate::app_server::{Client, StdioTransport};
        let stop = Arc::new(AtomicBool::new(false));
        let cancel = stop.clone();
        let mut client = None;
        Self::start(
            move |id| {
                if client.is_none() {
                    client = StdioTransport::codex_controlled(cancel.clone())
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
    pub fn label(&mut self, event: &Event) -> String {
        self.label_at(event, self.start.elapsed())
    }
    pub fn label_at(&mut self, event: &Event, now: Duration) -> String {
        while let Ok((id, name)) = self.results.try_recv() {
            if let Some(entry) = self.entries.get_mut(&id) {
                entry.pending = false;
                entry.refresh_at = now + Duration::from_secs(60);
                entry.name = name;
            }
        }
        if event.source != Source::CodexHook {
            return event.source.label().into();
        }
        let Some(id) = event.session_id.as_deref() else {
            return "Codex".into();
        };
        if event.validate().is_err() {
            return "Codex".into();
        }
        if !self.entries.contains_key(id) && self.entries.len() >= 64 {
            // Do not evict in-flight work: a later result must never label another session.
            if let Some(oldest) = self
                .entries
                .iter()
                .filter(|(_, v)| !v.pending)
                .min_by_key(|(_, v)| v.refresh_at)
                .map(|(k, _)| k.clone())
            {
                self.entries.remove(&oldest);
            } else {
                return fallback(id);
            }
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
        entry
            .name
            .as_ref()
            .map(|name| format!("Codex: {name}"))
            .unwrap_or_else(|| fallback(id))
    }
}
fn fallback(id: &str) -> String {
    let suffix: String = id
        .chars()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("Codex: session {suffix}")
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
