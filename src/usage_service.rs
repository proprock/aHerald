//! Background quota polling is independent of the notification queue and GUI.
use crate::{
    agent::Agent,
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
    /// Agent whose usage this is; it selects the texts and the preferred bucket key.
    pub agent: Agent,
}
impl View {
    pub fn new(enabled: bool, interval: Duration) -> Self {
        Self::for_agent(Agent::Codex, enabled, interval)
    }
    pub fn for_agent(agent: Agent, enabled: bool, interval: Duration) -> Self {
        Self {
            snapshot: None,
            error: None,
            updated: None,
            interval,
            enabled,
            agent,
        }
    }
    fn label(&self) -> &'static str {
        self.agent.label()
    }
    fn key(&self) -> &'static str {
        self.agent.key()
    }
    /// Bucket of this agent, or the first one when the id is unknown.
    pub fn bucket(&self) -> Option<&crate::usage::Bucket> {
        let snapshot = self.snapshot.as_ref()?;
        snapshot
            .buckets
            .get(self.key())
            .or_else(|| snapshot.buckets.values().next())
    }
    fn status(&self) -> &'static str {
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
    pub fn compact(&self, unix: i64, display: crate::config::UsageDisplay) -> String {
        let status = self.status();
        let display_name = match display {
            crate::config::UsageDisplay::Remain => "remain",
            crate::config::UsageDisplay::Used => "used",
        };
        let Some(bucket) = self.bucket() else {
            return format!("{} {display_name}: {status}", self.label());
        };
        let suffix = if status == "current" {
            String::new()
        } else {
            format!(" ({status})")
        };
        if let Some(line) = spend_only_line(self.label(), bucket) {
            return format!("{line}{suffix}");
        }
        let value = |window: Option<&crate::usage::Window>| {
            window_text(
                window.and_then(|w| match display {
                    crate::config::UsageDisplay::Used => w.used_percent,
                    crate::config::UsageDisplay::Remain => w.remaining_percent(),
                }),
                window.and_then(|w| w.resets_at),
                unix,
            )
        };
        format!(
            "{} {display_name}: 5h {}, 7d {}{suffix}",
            self.label(),
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
            Some(plan) => format!("{} plan: {plan}\n", self.label()),
            None => format!("{} usage\n", self.label()),
        };
        if let Some(bucket) = self.bucket() {
            let windows = if has_no_windows(bucket) {
                Vec::new()
            } else {
                vec![("5h", &bucket.primary), ("Week", &bucket.secondary)]
            };
            for (label, window) in windows {
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
            for (name, allowance) in [
                ("Extra usage", &bucket.extra_usage),
                ("Spend", &bucket.spend),
            ] {
                if let Some(allowance) = allowance {
                    text.push_str(&allowance_line(name, allowance));
                }
            }
        }
        let status = self.status();
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
        if self.agent != Agent::Codex {
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
        let status = self.status();
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
}
/// Preserve all text while fitting a bounded notification card; arrows page through it.
/// What the usage card shows in place of the quota details until it closes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UsageNotice {
    /// The About text, read afresh each time so its build age keeps counting.
    About,
    Text(String),
}
impl UsageNotice {
    pub fn text(&self) -> String {
        match self {
            Self::About => crate::about_card_text(),
            Self::Text(text) => text.clone(),
        }
    }
}
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
/// `79%`, or `—` when the value is unknown; the percent form of the menu, tooltip and status line.
pub fn percent_text(percent: Option<f64>) -> String {
    percent
        .map(|v| format!("{v:.0}%"))
        .unwrap_or_else(|| "—".into())
}
/// `79% / 3:29`: one window of the menu and tooltip text (`Claude remain: 5h 79% / 3:29, ...`).
pub fn window_text(percent: Option<f64>, resets_at: Option<i64>, now: i64) -> String {
    format!("{} / {}", percent_text(percent), countdown(resets_at, now))
}
pub fn countdown(reset: Option<i64>, now: i64) -> String {
    let Some(reset) = reset else {
        return "—".into();
    };
    // Rounded up: the time left must not promise more than there is.
    span_text(
        reset
            .saturating_sub(now)
            .max(0)
            .cast_unsigned()
            .div_ceil(60),
    )
}
/// `12 min`, `3:05`, `2d 3:05`: the one form of a time span in the tray, cards and status line.
pub fn span_text(minutes: u64) -> String {
    match minutes {
        0..60 => format!("{minutes} min"),
        60..1_440 => format!("{}:{:02}", minutes / 60, minutes % 60),
        _ => format!(
            "{}d {}:{:02}",
            minutes / 1_440,
            minutes % 1_440 / 60,
            minutes % 60
        ),
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
/// A bucket with neither the 5-hour nor the weekly window but a spend or extra-usage figure:
/// an account billed by usage (enterprise).
fn has_no_windows(bucket: &crate::usage::Bucket) -> bool {
    bucket.primary.is_none()
        && bucket.secondary.is_none()
        && (bucket.spend.is_some() || bucket.extra_usage.is_some())
}
/// `Claude enterprise: spend 0.74 / 100.00 USD (0.74%)`: the usage line for such an account.
fn spend_only_line(label: &str, bucket: &crate::usage::Bucket) -> Option<String> {
    if !has_no_windows(bucket) {
        return None;
    }
    let (name, allowance) = match (&bucket.spend, &bucket.extra_usage) {
        (Some(spend), _) => ("spend", spend),
        (None, Some(extra)) => ("extra", extra),
        (None, None) => return None,
    };
    let amount = |value: &Option<String>| clean(value.as_deref().unwrap_or("—"));
    let plan = bucket
        .plan_type
        .as_deref()
        .map_or("enterprise".into(), clean);
    Some(format!(
        "{label} {plan}: {name} {} / {}{}{}",
        amount(&allowance.used),
        amount(&allowance.limit),
        allowance
            .currency
            .as_deref()
            .map(|c| format!(" {}", clean(c)))
            .unwrap_or_default(),
        allowance
            .percent
            .map(|p| format!(" ({}%)", percent_figure(p)))
            .unwrap_or_default()
    ))
}
/// `1`, `0.74`, `12.5`: at most two decimals, none when whole.
fn percent_figure(percent: f64) -> String {
    format!("{percent:.2}")
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_owned()
}
/// `Extra usage: enabled, limit: 12.34/50.00 USD` (the figures are used/limit), then the
/// provider's percentage, state and limit-reached flag when it reports them.
fn allowance_line(name: &str, allowance: &crate::usage::Allowance) -> String {
    let amount = |value: &Option<String>| clean(value.as_deref().unwrap_or("—"));
    // Extra usage reads "limit: used/limit"; spend reads "used/limit" without the label.
    let label = if name == "Extra usage" { "limit: " } else { "" };
    let mut line = format!(
        "{name}: {}, {label}{}/{}{}",
        if allowance.enabled == Some(true) {
            "enabled"
        } else {
            "disabled"
        },
        amount(&allowance.used),
        amount(&allowance.limit),
        allowance
            .currency
            .as_deref()
            .map(|c| format!(" {}", clean(c)))
            .unwrap_or_default()
    );
    if let Some(percent) = allowance.percent {
        line.push_str(&format!(", {}% used", percent_figure(percent)));
    }
    if let Some(severity) = &allowance.severity {
        line.push_str(&format!(", {}", clean(severity)));
    }
    if allowance.reached == Some(true) {
        line.push_str(", limit reached");
    }
    line.push('\n');
    line
}
fn flag(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "yes",
        Some(false) => "no",
        None => "unavailable",
    }
}
fn short_reset_text(timestamp: i64) -> Option<String> {
    reset_text(timestamp).map(|s| s.chars().take(16).collect())
}
fn reset_text(timestamp: i64) -> Option<String> {
    crate::usage::local_reset(timestamp)
}
const LIVE_POLL: Duration = Duration::from_millis(500);
/// What a provider pushed since the last look.
#[derive(Default)]
pub struct Push {
    pub changed: bool,
    /// The account changed: the shown snapshot no longer applies.
    pub reset: bool,
    /// The pushed data cannot be merged; read a fresh snapshot now.
    pub refetch: bool,
}
/// One provider of quota snapshots. Implementations return typed results; they do
/// not need to speak any wire protocol of another provider.
pub trait UsageSource {
    fn read(&mut self, now: Duration) -> Result<Snapshot, Error>;
    /// True while a connection stays open and must be polled for pushed updates.
    fn is_live(&self) -> bool {
        false
    }
    /// Reads pending pushed messages without blocking.
    fn listen(&mut self, _now: Duration) -> Result<(), Error> {
        Ok(())
    }
    /// Applies pushed updates to the shown snapshot.
    fn updates(&mut self, _snapshot: &mut Option<Snapshot>) -> Push {
        Push::default()
    }
    /// A session name read over the source's own connection, if it has one.
    fn thread_name(&mut self, _now: Duration, _id: &str) -> Option<String> {
        None
    }
}
/// Codex app-server: one owned child connection with rolling updates.
pub struct CodexSource<T, F> {
    connection: Connection<T, F>,
    name_failures: crate::session_source::LookupFailures,
}
impl<T: Transport, F: FnMut() -> Result<T, Error>> CodexSource<T, F> {
    pub fn new(factory: F) -> Self {
        Self {
            connection: Connection::new(factory),
            name_failures: Default::default(),
        }
    }
}
impl<T: Transport, F: FnMut() -> Result<T, Error>> UsageSource for CodexSource<T, F> {
    fn read(&mut self, now: Duration) -> Result<Snapshot, Error> {
        let _timer = crate::perf::span(crate::perf::Span::UsagePoll);
        Snapshot::parse(self.connection.read(now)?).map_err(Error::Service)
    }
    fn is_live(&self) -> bool {
        self.connection.client.is_some()
    }
    fn thread_name(&mut self, now: Duration, id: &str) -> Option<String> {
        let result = self.connection.thread_name(now, id);
        self.name_failures.observe(result.is_err());
        result.ok().flatten()
    }
    fn listen(&mut self, now: Duration) -> Result<(), Error> {
        let Some(client) = &mut self.connection.client else {
            return Ok(());
        };
        match client.poll(Duration::ZERO) {
            Ok(()) | Err(Error::Timeout) => Ok(()),
            Err(error) => {
                self.connection.failed(now);
                Err(error)
            }
        }
    }
    fn updates(&mut self, snapshot: &mut Option<Snapshot>) -> Push {
        let updates = self
            .connection
            .client
            .as_mut()
            .map(|c| c.take_updates())
            .unwrap_or_default();
        let mut push = Push {
            changed: !updates.is_empty(),
            ..Push::default()
        };
        for update in updates {
            if update["method"] == "account/updated" {
                *snapshot = None;
                push.reset = true;
                push.refetch = true;
            } else if let Some(current) = snapshot {
                if !matches!(current.update(update["params"].clone()), Ok(true)) {
                    push.refetch = true;
                }
            } else {
                push.refetch = true;
            }
        }
        push
    }
}
/// Longest wait a `Retry-After` can impose; a longer refresh interval still wins.
const RATE_LIMIT_CAP: Duration = Duration::from_secs(900);
pub struct Driver<S> {
    source: S,
    pub view: View,
    next: Duration,
    allowed: Duration,
    backoff: Duration,
}
impl<T: Transport, F: FnMut() -> Result<T, Error>> Driver<CodexSource<T, F>> {
    pub fn new(interval: Duration, factory: F) -> Self {
        Self::with_source(Agent::Codex, interval, CodexSource::new(factory))
    }
}
impl<S: UsageSource> Driver<S> {
    pub fn with_source(agent: Agent, interval: Duration, source: S) -> Self {
        Self {
            source,
            view: View::for_agent(agent, true, interval),
            next: Duration::ZERO,
            allowed: Duration::ZERO,
            backoff: Duration::from_secs(10),
        }
    }
    /// How long the worker may sleep before `step` can have work. A live app-server
    /// connection is also polled for pushed updates, so it wakes at least that often.
    pub fn wait(&self, now: Duration) -> Duration {
        let due = self.next.max(self.allowed).saturating_sub(now);
        let wait = if self.source.is_live() {
            due.min(LIVE_POLL)
        } else {
            due
        };
        wait.max(Duration::from_millis(1))
    }
    /// Runs a request that arrived for the worker; true when it asks for a refresh.
    pub fn command(&mut self, now: Duration, command: Command) -> bool {
        match command {
            Command::Refresh => true,
            Command::Name { id, reply } => {
                let _ = reply.try_send(self.source.thread_name(now, &id));
                false
            }
        }
    }
    fn failure(&mut self, now: Duration, error: Error) {
        if self.view.error.is_none() {
            crate::operational_log::log(crate::operational_log::Record::UsageFailure);
        }
        // A rate-limited service is asked again no sooner than its `Retry-After` (capped)
        // and never sooner than the refresh interval.
        let delay = match error {
            Error::RateLimited(_, retry_after) => self
                .backoff
                .max(retry_after.unwrap_or_default())
                .min(RATE_LIMIT_CAP)
                .max(self.view.interval),
            _ => self.backoff,
        };
        self.view.error = Some(error.to_string());
        self.next = now.saturating_add(delay);
        self.allowed = self.next;
        self.backoff = (self.backoff * 2).min(Duration::from_secs(300));
    }
    pub fn step(&mut self, now: Duration, manual: bool) -> bool {
        if now >= self.allowed && (manual || now >= self.next) {
            self.allowed = now.saturating_add(Duration::from_secs(5));
            self.next = now.saturating_add(self.view.interval);
            match self.source.read(now) {
                Ok(snapshot) => {
                    self.view.snapshot = Some(snapshot);
                    self.view.updated = Some(now);
                    self.view.error = None;
                    self.backoff = Duration::from_secs(10);
                }
                Err(error) => self.failure(now, error),
            }
            self.updates(now);
            return true;
        }
        match self.source.listen(now) {
            Ok(()) => self.updates(now),
            Err(error) => {
                self.failure(now, error);
                true
            }
        }
    }
    fn updates(&mut self, now: Duration) -> bool {
        let push = self.source.updates(&mut self.view.snapshot);
        if push.reset {
            self.view.updated = None;
            self.view.error = None;
        }
        if push.refetch {
            self.next = now;
        }
        push.changed
    }
}

/// A request for the usage worker, which owns the agent's single connection.
pub enum Command {
    Refresh,
    Name {
        id: String,
        reply: mpsc::SyncSender<Option<String>>,
    },
}
/// How long a name lookup waits: the worker may be mid-request when it arrives.
const LOOKUP_WAIT: Duration = Duration::from_secs(12);
/// Reads session names over the usage worker's connection instead of a second child.
#[derive(Clone)]
pub struct NameLookup {
    commands: mpsc::SyncSender<Command>,
}
impl NameLookup {
    /// `None` for an unnamed or unknown thread, a busy or stopped worker, or `cancel`.
    pub fn name(&self, id: &str, cancel: &AtomicBool) -> Option<String> {
        let (reply, answer) = mpsc::sync_channel(1);
        self.commands
            .try_send(Command::Name {
                id: id.into(),
                reply,
            })
            .ok()?;
        let deadline = Instant::now() + LOOKUP_WAIT;
        loop {
            match answer.recv_timeout(Duration::from_millis(20)) {
                Ok(name) => return name,
                Err(mpsc::RecvTimeoutError::Disconnected) => return None,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if cancel.load(Ordering::Acquire) || Instant::now() >= deadline {
                        return None;
                    }
                }
            }
        }
    }
}
type ChangeCallback = Arc<dyn Fn() + Send + Sync>;

pub struct Service {
    changed: Arc<Mutex<Option<ChangeCallback>>>,
    state: Arc<Mutex<View>>,
    refresh: mpsc::SyncSender<Command>,
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
    #[cfg(any(windows, unix))]
    pub fn claude(
        enabled: bool,
        interval: Duration,
        source: crate::config::ClaudeUsageSource,
    ) -> Self {
        Self::spawn_source(
            Agent::Claude,
            enabled,
            interval,
            Arc::new(AtomicBool::new(false)),
            move || {
                crate::claude_usage::ClaudeSource::new(move || {
                    crate::claude_usage::read_with(source, crate::claude_usage::fetch_live, || {
                        crate::statusline::with_store(|store| {
                            store.quota_snapshot(unix_now(), None)
                        })
                    })
                })
            },
        )
    }
    pub fn spawn<T: Transport + Send + 'static, F: FnMut() -> Result<T, Error> + Send + 'static>(
        enabled: bool,
        interval: Duration,
        stop: Arc<AtomicBool>,
        factory: F,
    ) -> Self {
        Self::spawn_source(Agent::Codex, enabled, interval, stop, move || {
            CodexSource::new(factory)
        })
    }
    /// The source is built on the worker thread, so it need not be `Send` itself.
    pub fn spawn_source<S: UsageSource, M: FnOnce() -> S + Send + 'static>(
        agent: Agent,
        enabled: bool,
        interval: Duration,
        stop: Arc<AtomicBool>,
        make: M,
    ) -> Self {
        let state = Arc::new(Mutex::new(View::for_agent(agent, enabled, interval)));
        let origin = Instant::now();
        let (refresh, rx) = mpsc::sync_channel(8);
        let shared = state.clone();
        let changed: Arc<Mutex<Option<ChangeCallback>>> = Arc::new(Mutex::new(None));
        let notify = changed.clone();
        let done = stop.clone();
        let worker = enabled.then(|| {
            std::thread::spawn(move || {
                let publish = |view: &View| {
                    if let Ok(mut state) = shared.lock() {
                        *state = view.clone();
                    } else {
                        return;
                    }
                    let callback = notify.lock().ok().and_then(|c| c.clone());
                    if let Some(callback) = callback {
                        callback();
                    }
                };
                let mut driver = Driver::with_source(agent, interval, make());
                while !done.load(Ordering::Acquire) {
                    let mut manual = false;
                    while let Ok(command) = rx.try_recv() {
                        manual |= driver.command(origin.elapsed(), command);
                    }
                    if driver.step(origin.elapsed(), manual) {
                        publish(&driver.view);
                    }
                    // Wake immediately on a refresh, with no busy waiting and a bounded mailbox.
                    match rx.recv_timeout(driver.wait(origin.elapsed())) {
                        Ok(command) => {
                            let manual = driver.command(origin.elapsed(), command);
                            if driver.step(origin.elapsed(), manual) {
                                publish(&driver.view);
                            }
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(_) => (),
                    }
                }
            })
        });
        Self {
            changed,
            state,
            refresh,
            stop,
            worker,
            origin,
        }
    }
    /// Installs a nonblocking wake callback and replays state published before registration.
    /// Callbacks may read the service: neither its state nor callback lock is held.
    pub fn on_change(&self, callback: impl Fn() + Send + Sync + 'static) {
        let callback: ChangeCallback = Arc::new(callback);
        if let Ok(mut changed) = self.changed.lock() {
            *changed = Some(callback.clone());
        }
        callback();
    }
    pub fn refresh(&self) {
        let _ = self.refresh.try_send(Command::Refresh);
    }
    /// Session names through this service's connection; empty answers when disabled.
    pub fn names(&self) -> NameLookup {
        NameLookup {
            commands: self.refresh.clone(),
        }
    }
    pub fn view(&self) -> View {
        self.state.lock().map(|v| v.clone()).unwrap_or_else(|_| {
            let mut view = View::new(true, Duration::from_secs(300));
            view.error = Some("Usage state unavailable".into());
            view
        })
    }
    pub fn compact(&self, display: crate::config::UsageDisplay) -> String {
        self.view().compact(unix_now(), display)
    }
    pub fn card_text(&self) -> String {
        self.view().card_text(self.origin.elapsed(), unix_now())
    }
}
impl Drop for Service {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = self.refresh.try_send(Command::Refresh);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}
