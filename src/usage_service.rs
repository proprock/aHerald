//! Background quota polling is independent of the notification queue and GUI.
use crate::{
    app_server::{Connection, Error, StdioTransport, Transport},
    usage::Snapshot,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct View {
    pub snapshot: Option<Snapshot>,
    pub error: Option<String>,
    pub updated: Option<Duration>,
    pub interval: Duration,
    pub enabled: bool,
}
impl View {
    pub fn new(enabled: bool, interval: Duration) -> Self {
        Self {
            snapshot: None,
            error: None,
            updated: None,
            interval,
            enabled,
        }
    }
    fn status(&self, now: Duration) -> &'static str {
        if !self.enabled {
            "disabled"
        } else if self.snapshot.is_some()
            && (self.error.is_some()
                || self
                    .updated
                    .is_some_and(|t| now.saturating_sub(t) >= self.interval))
        {
            "stale"
        } else if self.error.is_some()
            || self.snapshot.as_ref().is_some_and(|s| s.buckets.is_empty())
        {
            "unavailable"
        } else if self.snapshot.is_none() {
            "loading"
        } else {
            "current"
        }
    }
    pub fn summary(&self, now: Duration) -> String {
        let mut text = format!("Codex usage ({})", self.status(now));
        if let Some(snapshot) = &self.snapshot
            && let Some(bucket) = snapshot
                .buckets
                .get("codex")
                .or_else(|| snapshot.buckets.values().next())
        {
            for (label, window) in [
                ("Primary", &bucket.primary),
                ("Secondary", &bucket.secondary),
            ] {
                text.push_str(&format!("\r\n{label}: {}", brief(window.as_ref())));
            }
        }
        text
    }
    pub fn details(&self, now: Duration) -> String {
        let mut text = format!("Codex usage: {}\r\n", self.status(now));
        if !self.enabled {
            text.push_str("Set [usage] enabled = true and restart the resident.\r\n");
        }
        if let Some(error) = &self.error {
            text.push_str(&format!("{error}\r\n"));
        }
        if let Some(updated) = self.updated {
            text.push_str(&format!(
                "Last successful update: {} seconds ago\r\n",
                now.saturating_sub(updated).as_secs()
            ));
        }
        if let Some(snapshot) = &self.snapshot {
            for (id, bucket) in &snapshot.buckets {
                text.push_str(&format!(
                    "\r\n{} [{}] — plan: {}\r\n",
                    clean(bucket.limit_name.as_deref().unwrap_or(id)),
                    clean(id),
                    bucket
                        .plan_type
                        .as_deref()
                        .map(clean)
                        .unwrap_or_else(|| "unavailable".into())
                ));
                for (label, window) in [
                    ("Primary", &bucket.primary),
                    ("Secondary", &bucket.secondary),
                ] {
                    let reset = window
                        .as_ref()
                        .and_then(|w| w.resets_at)
                        .and_then(reset_text)
                        .unwrap_or_else(|| "unavailable".into());
                    text.push_str(&format!(
                        "{label}: {}; reset: {reset}\r\n",
                        brief(window.as_ref())
                    ));
                }
                if let Some(c) = &bucket.credits {
                    text.push_str(&format!(
                        "Workspace credits: available={}, unlimited={}, balance={}\r\n",
                        flag(c.has_credits),
                        flag(c.unlimited),
                        c.balance
                            .as_deref()
                            .map(clean)
                            .unwrap_or_else(|| "unavailable".into())
                    ));
                } else {
                    text.push_str("Workspace credits: unavailable\r\n");
                }
                if let Some(reached) = &bucket.rate_limit_reached_type {
                    text.push_str(&format!("Limit state: {}\r\n", clean(reached)));
                }
            }
            text.push_str(&format!(
                "\r\nOrdinary usage allowed: {}\r\n",
                flag(snapshot.ordinary_usage_allowed)
            ));
            if let Some(c) = &snapshot.reset_credits {
                text.push_str(&format!(
                    "Available reset credits: {} (read-only)\r\n",
                    c.available_count
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "unavailable".into())
                ));
                if let Some(rows) = &c.credits {
                    for row in rows {
                        text.push_str(&format!(
                            "  {}: {}; granted {}; expires {}\r\n",
                            clean(row.title.as_deref().unwrap_or("Reset credit")),
                            clean(row.status.as_deref().unwrap_or("unavailable")),
                            row.granted_at
                                .and_then(reset_text)
                                .unwrap_or_else(|| "unavailable".into()),
                            row.expires_at
                                .and_then(reset_text)
                                .unwrap_or_else(|| "unavailable".into())
                        ));
                    }
                } else {
                    text.push_str("Reset credit details: unavailable\r\n");
                }
            } else {
                text.push_str("Reset credits: unavailable\r\n");
            }
        }
        text.push_str("\r\nReset times use the Windows local timezone.\r\nNotifications run independently of usage. No account mutations are available.");
        text
    }
}
fn clean(value: &str) -> String {
    value
        .chars()
        .filter(|c| {
            !c.is_control() && !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
        })
        .take(120)
        .collect()
}
fn flag(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unavailable",
    }
}
fn brief(window: Option<&crate::usage::Window>) -> String {
    let Some(window) = window else {
        return "unavailable".into();
    };
    let percent = window
        .used_percent
        .map(|v| format!("{v}% used"))
        .unwrap_or_else(|| "unavailable".into());
    format!(
        "{percent} / {}",
        window
            .window_duration_mins
            .map(|m| format!("{m} min"))
            .unwrap_or_else(|| "window unavailable".into())
    )
}
fn reset_text(timestamp: i64) -> Option<String> {
    #[cfg(windows)]
    {
        crate::usage::local_reset(timestamp)
    }
    #[cfg(not(windows))]
    {
        Some(format!("{timestamp} (Unix UTC)"))
    }
}
pub struct Driver<T, F> {
    connection: Connection<T, F>,
    pub view: View,
    next: Duration,
    allowed: Duration,
    backoff: Duration,
}
impl<T: Transport, F: FnMut() -> Result<T, Error>> Driver<T, F> {
    pub fn new(interval: Duration, factory: F) -> Self {
        Self {
            connection: Connection::new(factory),
            view: View::new(true, interval),
            next: Duration::ZERO,
            allowed: Duration::ZERO,
            backoff: Duration::from_secs(10),
        }
    }
    fn failure(&mut self, now: Duration, error: String) {
        self.view.error = Some(error);
        self.next = now.saturating_add(self.backoff);
        self.allowed = self.next;
        self.backoff = (self.backoff * 2).min(Duration::from_secs(300));
    }
    pub fn step(&mut self, now: Duration, manual: bool) -> bool {
        if now >= self.allowed && (manual || now >= self.next) {
            self.allowed = now.saturating_add(Duration::from_secs(5));
            self.next = now.saturating_add(self.view.interval);
            match self.connection.read(now) {
                Ok(value) => match Snapshot::parse(value) {
                    Ok(snapshot) => {
                        self.view.snapshot = Some(snapshot);
                        self.view.updated = Some(now);
                        self.view.error = None;
                        self.backoff = Duration::from_secs(10);
                    }
                    Err(error) => self.failure(now, error.into()),
                },
                Err(error) => self.failure(now, error.to_string()),
            }
            self.updates(now);
            return true;
        }
        if let Some(client) = &mut self.connection.client {
            match client.poll(Duration::ZERO) {
                Ok(()) => return self.updates(now),
                Err(Error::Timeout) => (),
                Err(error) => {
                    self.connection.failed(now);
                    self.failure(now, error.to_string());
                    return true;
                }
            }
        }
        false
    }
    fn updates(&mut self, now: Duration) -> bool {
        let updates = self
            .connection
            .client
            .as_mut()
            .map(|c| c.take_updates())
            .unwrap_or_default();
        let changed = !updates.is_empty();
        for update in updates {
            if update["method"] == "account/updated" {
                self.view.snapshot = None;
                self.view.updated = None;
                self.view.error = None;
                self.next = now;
            } else if let Some(snapshot) = &mut self.view.snapshot {
                match snapshot.update(update["params"].clone()) {
                    Ok(true) => (),
                    _ => self.next = now,
                }
            } else {
                self.next = now;
            }
        }
        changed
    }
}

pub struct Service {
    state: Arc<Mutex<View>>,
    refresh: mpsc::SyncSender<()>,
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
    origin: Instant,
}
impl Service {
    pub fn codex(enabled: bool, interval: Duration) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let canceled = stop.clone();
        Self::spawn(enabled, interval, stop, move || {
            StdioTransport::codex_controlled(canceled.clone())
        })
    }
    pub fn spawn<T: Transport + Send + 'static, F: FnMut() -> Result<T, Error> + Send + 'static>(
        enabled: bool,
        interval: Duration,
        stop: Arc<AtomicBool>,
        factory: F,
    ) -> Self {
        let state = Arc::new(Mutex::new(View::new(enabled, interval)));
        let origin = Instant::now();
        let (refresh, rx) = mpsc::sync_channel(1);
        let shared = state.clone();
        let done = stop.clone();
        let worker = enabled.then(|| {
            std::thread::spawn(move || {
                let mut driver = Driver::new(interval, factory);
                while !done.load(Ordering::Acquire) {
                    let manual = rx.try_recv().is_ok();
                    if driver.step(origin.elapsed(), manual)
                        && let Ok(mut state) = shared.lock()
                    {
                        *state = driver.view.clone();
                    }
                    // Wake immediately on a refresh, with no busy waiting and a bounded mailbox.
                    match rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(()) => {
                            if driver.step(origin.elapsed(), true)
                                && let Ok(mut state) = shared.lock()
                            {
                                *state = driver.view.clone();
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(_) => (),
                    }
                }
            })
        });
        Self {
            state,
            refresh,
            stop,
            worker,
            origin,
        }
    }
    pub fn refresh(&self) {
        let _ = self.refresh.try_send(());
    }
    pub fn view(&self) -> View {
        self.state.lock().map(|v| v.clone()).unwrap_or_else(|_| {
            let mut view = View::new(true, Duration::from_secs(300));
            view.error = Some("Usage state unavailable".into());
            view
        })
    }
    pub fn summary(&self) -> String {
        self.view().summary(self.origin.elapsed())
    }
    pub fn details(&self) -> String {
        self.view().details(self.origin.elapsed())
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.refresh.try_send(());
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
