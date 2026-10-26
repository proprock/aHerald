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
    /// Agent shown in texts; its lowercase form is the preferred bucket key.
    pub label: &'static str,
}
impl View {
    pub fn new(enabled: bool, interval: Duration) -> Self {
        Self::labeled("Codex", enabled, interval)
    }
    pub fn labeled(label: &'static str, enabled: bool, interval: Duration) -> Self {
        Self {
            snapshot: None,
            error: None,
            updated: None,
            interval,
            enabled,
            label,
        }
    }
    fn key(&self) -> String {
        self.label.to_ascii_lowercase()
    }
    /// Bucket of this agent, or the first one when the id is unknown.
    pub fn bucket(&self) -> Option<&crate::usage::Bucket> {
        let snapshot = self.snapshot.as_ref()?;
        snapshot
            .buckets
            .get(&self.key())
            .or_else(|| snapshot.buckets.values().next())
    }
    fn status(&self, _now: Duration) -> &'static str {
        if !self.enabled {
            "disabled"
        } else if self
            .snapshot
            .as_ref()
            .is_some_and(|s| !s.buckets.is_empty())
        {
            "current"
        } else if self.error.is_some() || self.snapshot.is_some() {
            "unavailable"
        } else {
            "loading"
        }
    }
    pub fn summary(&self, now: Duration) -> String {
        let mut text = format!("{} usage ({})", self.label, self.status(now));
        if let Some(bucket) = self.bucket() {
            for (label, window) in [
                ("Primary", &bucket.primary),
                ("Secondary", &bucket.secondary),
            ] {
                text.push_str(&format!("\r\n{label}: {}", brief(window.as_ref())));
            }
        }
        text
    }
    pub fn compact(
        &self,
        now: Duration,
        unix: i64,
        display: crate::config::UsageDisplay,
    ) -> String {
        let status = self.status(now);
        let display_name = match display {
            crate::config::UsageDisplay::Remain => "remain",
            crate::config::UsageDisplay::Used => "used",
        };
        let Some(bucket) = self.bucket() else {
            return format!("{} {display_name}: {status}", self.label);
        };
        let value = |window: Option<&crate::usage::Window>| {
            let percent = window
                .and_then(|w| match display {
                    crate::config::UsageDisplay::Used => w.used_percent,
                    crate::config::UsageDisplay::Remain => w.remaining_percent(),
                })
                .map(|v| format!("{v:.0}%"))
                .unwrap_or_else(|| "—".into());
            format!(
                "{percent} / {}",
                countdown(window.and_then(|w| w.resets_at), unix)
            )
        };
        let suffix = if status == "current" {
            String::new()
        } else {
            format!(" ({status})")
        };
        format!(
            "{} {display_name}: 5h {}, 7d {}{suffix}",
            self.label,
            value(bucket.primary.as_ref()),
            value(bucket.secondary.as_ref())
        )
    }
    /// Two-window card for agents without Codex's credit and reset-credit details.
    fn simple_card_text(&self, now: Duration, unix: i64) -> String {
        let plan = self
            .bucket()
            .and_then(|b| b.plan_type.as_deref())
            .map(clean);
        let mut text = match plan {
            Some(plan) => format!("{} plan: {plan}\n", self.label),
            None => format!("{} usage\n", self.label),
        };
        if let Some(bucket) = self.bucket() {
            for (label, window) in [("5h", &bucket.primary), ("Week", &bucket.secondary)] {
                let percent = window
                    .as_ref()
                    .and_then(|w| w.used_percent)
                    .map(|v| format!("{v:.0}% used"))
                    .unwrap_or_else(|| "—".into());
                let reset = window.as_ref().and_then(|w| w.resets_at);
                text.push_str(&format!(
                    "{label}: {percent} / {}; reset: {}\n",
                    countdown(reset, unix),
                    reset
                        .and_then(short_reset_text)
                        .unwrap_or_else(|| "—".into())
                ));
            }
            for line in bucket.details.iter().flatten() {
                text.push_str(&clean(line));
                text.push('\n');
            }
        }
        let status = self.status(now);
        if status != "current" {
            text.push_str(&format!("Status: {status}\n"));
        }
        if let Some(error) = &self.error {
            text.push_str(&format!("Last refresh: {}\n", clean(error)));
        }
        if let Some(updated) = self.updated {
            text.push_str(&format!(
                "Updated {}s ago",
                now.saturating_sub(updated).as_secs()
            ));
        }
        text
    }
    pub fn card_text(&self, now: Duration, unix: i64) -> String {
        if self.label != "Codex" {
            return self.simple_card_text(now, unix);
        }
        let mut text = String::new();
        if let Some(snapshot) = &self.snapshot {
            for (id, bucket) in &snapshot.buckets {
                let name = if id == "codex" {
                    "Codex".into()
                } else {
                    clean(bucket.limit_name.as_deref().unwrap_or(id))
                };
                text.push_str(&format!(
                    "{name} plan: {}\n",
                    bucket
                        .plan_type
                        .as_deref()
                        .map(clean)
                        .unwrap_or_else(|| "—".into())
                ));
                for (label, window) in [("5h", &bucket.primary), ("Week", &bucket.secondary)] {
                    let percent = window
                        .as_ref()
                        .and_then(|w| w.used_percent)
                        .map(|v| format!("{v:.0}% used"))
                        .unwrap_or_else(|| "—".into());
                    let reset = window.as_ref().and_then(|w| w.resets_at);
                    text.push_str(&format!(
                        "{label}: {percent} / {}; reset: {}\n",
                        countdown(reset, unix),
                        reset
                            .and_then(short_reset_text)
                            .unwrap_or_else(|| "—".into())
                    ));
                }
                if let Some(c) = &bucket.credits {
                    text.push_str(&format!(
                        "Workspace credits: available={}, unlimited={}, balance={}\n",
                        flag(c.has_credits),
                        flag(c.unlimited),
                        c.balance
                            .as_deref()
                            .map(clean)
                            .unwrap_or_else(|| "—".into())
                    ));
                } else {
                    text.push_str("Workspace credits: —\n");
                }
                if let Some(reached) = &bucket.rate_limit_reached_type {
                    text.push_str(&format!("Limit state: {}\n", clean(reached)));
                }
                text.push('\n');
            }
            text.push_str(&format!(
                "Ordinary usage allowed: {}\n",
                flag(snapshot.ordinary_usage_allowed)
            ));
            if let Some(credits) = &snapshot.reset_credits {
                text.push_str(&format!(
                    "Available reset credits: {} (read-only)\n",
                    credits
                        .available_count
                        .map(|v| v.to_string())
                        .unwrap_or_else(|| "—".into())
                ));
                if let Some(rows) = &credits.credits {
                    for row in rows {
                        text.push_str(&format!(
                            "  {}: {}; expires {}\n",
                            clean(row.title.as_deref().unwrap_or("Reset credit")),
                            clean(row.status.as_deref().unwrap_or("unavailable")),
                            row.expires_at
                                .and_then(short_reset_text)
                                .unwrap_or_else(|| "—".into())
                        ));
                    }
                }
            } else {
                text.push_str("Available reset credits: —\n");
            }
        }
        let status = self.status(now);
        if status != "current" {
            text.push_str(&format!("Status: {status}\n"));
        }
        if let Some(error) = &self.error {
            text.push_str(&format!("Last refresh: {}\n", clean(error)));
        }
        if let Some(updated) = self.updated {
            text.push_str(&format!(
                "Updated {}s ago",
                now.saturating_sub(updated).as_secs()
            ));
        }
        if !self.enabled {
            text.push_str("Set [codex.usage] enabled = true and restart.");
        }
        text
    }
    pub fn details(&self, now: Duration) -> String {
        let mut text = format!("{} usage: {}\r\n", self.label, self.status(now));
        if !self.enabled {
            text.push_str("Set [codex.usage] enabled = true and restart the resident.\r\n");
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
                                .and_then(short_reset_text)
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
/// Preserve all text while fitting a bounded notification card; arrows page through it.
pub fn card_pages(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for line in text.lines() {
        let chars: Vec<char> = line.chars().collect();
        if chars.is_empty() {
            lines.push(String::new());
        }
        for part in chars.chunks(72) {
            lines.push(part.iter().collect::<String>());
        }
    }
    if lines.is_empty() {
        lines.push("Usage unavailable".into());
    }
    lines.chunks(12).map(|part| part.join("\n")).collect()
}
pub fn countdown(reset: Option<i64>, now: i64) -> String {
    let Some(reset) = reset else {
        return "—".into();
    };
    let seconds = reset.saturating_sub(now).max(0) as u64;
    if seconds >= 86400 {
        format!("{}d {}h", seconds / 86400, seconds % 86400 / 3600)
    } else if seconds >= 3600 {
        let minutes = seconds.div_ceil(60);
        format!("{}:{:02}", minutes / 60, minutes % 60)
    } else {
        format!("{} min", seconds.div_ceil(60))
    }
}
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().min(i64::MAX as u64) as i64)
        .unwrap_or(0)
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
fn short_reset_text(timestamp: i64) -> Option<String> {
    reset_text(timestamp).map(|s| s.chars().take(16).collect())
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
const LIVE_POLL: Duration = Duration::from_millis(500);
pub struct Driver<T, F> {
    connection: Connection<T, F>,
    pub view: View,
    next: Duration,
    allowed: Duration,
    backoff: Duration,
}
impl<T: Transport, F: FnMut() -> Result<T, Error>> Driver<T, F> {
    pub fn new(interval: Duration, factory: F) -> Self {
        Self::labeled("Codex", interval, factory)
    }
    pub fn labeled(label: &'static str, interval: Duration, factory: F) -> Self {
        Self {
            connection: Connection::new(factory),
            view: View::labeled(label, true, interval),
            next: Duration::ZERO,
            allowed: Duration::ZERO,
            backoff: Duration::from_secs(10),
        }
    }
    /// How long the worker may sleep before `step` can have work. A live app-server
    /// connection is also polled for pushed updates, so it wakes at least that often.
    pub fn wait(&self, now: Duration) -> Duration {
        let due = self.next.max(self.allowed).saturating_sub(now);
        let wait = if self.connection.client.is_some() {
            due.min(LIVE_POLL)
        } else {
            due
        };
        wait.max(Duration::from_millis(1))
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
    /// Opt-in Claude quota poller; see `claude_usage` for the data source and its limits.
    #[cfg(windows)]
    pub fn claude(enabled: bool, interval: Duration) -> Self {
        Self::spawn_labeled(
            "Claude",
            enabled,
            interval,
            Arc::new(AtomicBool::new(false)),
            || {
                Ok(crate::claude_usage::ClaudeTransport::new(
                    crate::claude_usage::fetch_live,
                ))
            },
        )
    }
    pub fn spawn<T: Transport + Send + 'static, F: FnMut() -> Result<T, Error> + Send + 'static>(
        enabled: bool,
        interval: Duration,
        stop: Arc<AtomicBool>,
        factory: F,
    ) -> Self {
        Self::spawn_labeled("Codex", enabled, interval, stop, factory)
    }
    pub fn spawn_labeled<
        T: Transport + Send + 'static,
        F: FnMut() -> Result<T, Error> + Send + 'static,
    >(
        label: &'static str,
        enabled: bool,
        interval: Duration,
        stop: Arc<AtomicBool>,
        factory: F,
    ) -> Self {
        let state = Arc::new(Mutex::new(View::labeled(label, enabled, interval)));
        let origin = Instant::now();
        let (refresh, rx) = mpsc::sync_channel(1);
        let shared = state.clone();
        let done = stop.clone();
        let worker = enabled.then(|| {
            std::thread::spawn(move || {
                let mut driver = Driver::labeled(label, interval, factory);
                while !done.load(Ordering::Acquire) {
                    let manual = rx.try_recv().is_ok();
                    if driver.step(origin.elapsed(), manual)
                        && let Ok(mut state) = shared.lock()
                    {
                        *state = driver.view.clone();
                    }
                    // Wake immediately on a refresh, with no busy waiting and a bounded mailbox.
                    match rx.recv_timeout(driver.wait(origin.elapsed())) {
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
    pub fn compact(&self, display: crate::config::UsageDisplay) -> String {
        self.view()
            .compact(self.origin.elapsed(), unix_now(), display)
    }
    pub fn card_text(&self) -> String {
        self.view().card_text(self.origin.elapsed(), unix_now())
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
