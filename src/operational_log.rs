//! Fixed operational codes only: this API cannot accept payloads or external errors.
//! A line is `<local ISO-8601 time> <LEVEL> <component.event> [key=number ...]`.
use crate::config::LogLevel;
use std::{
    borrow::Cow,
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU8, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
pub const MAX_LOG_BYTES: u64 = 64 * 1024;
/// Per-file cap at `debug`, where aggregated performance lines share the log.
pub const MAX_DEBUG_LOG_BYTES: u64 = 512 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Error = 1,
    Warn = 2,
    Info = 3,
    Debug = 4,
}
impl Level {
    fn label(self) -> &'static str {
        match self {
            Self::Error => "ERROR",
            Self::Warn => "WARN ",
            Self::Info => "INFO ",
            Self::Debug => "DEBUG",
        }
    }
}
fn threshold(level: LogLevel) -> u8 {
    match level {
        LogLevel::Off => 0,
        LogLevel::Error => 1,
        LogLevel::Warn => 2,
        LogLevel::Info => 3,
        LogLevel::Debug | LogLevel::Trace => 4,
    }
}
/// Formats Unix milliseconds as local ISO-8601 time with the given UTC offset.
pub fn format_timestamp(unix_millis: i64, offset_minutes: i32) -> String {
    let local = unix_millis + i64::from(offset_minutes) * 60_000;
    let days = local.div_euclid(86_400_000);
    let millis_of_day = local.rem_euclid(86_400_000);
    // Civil date from days since 1970-01-01 (proleptic Gregorian, Howard Hinnant).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let sign = if offset_minutes < 0 { '-' } else { '+' };
    let offset = offset_minutes.unsigned_abs();
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}{sign}{:02}:{:02}",
        millis_of_day / 3_600_000,
        millis_of_day / 60_000 % 60,
        millis_of_day / 1000 % 60,
        millis_of_day % 1000,
        offset / 60,
        offset % 60
    )
}
#[cfg(windows)]
fn local_offset_minutes() -> i32 {
    use windows::Win32::System::Time::{GetTimeZoneInformation, TIME_ZONE_INFORMATION};
    const TIME_ZONE_ID_DAYLIGHT: u32 = 2;
    let mut zone = TIME_ZONE_INFORMATION::default();
    // SAFETY: `zone` is a valid, writable TIME_ZONE_INFORMATION.
    match unsafe { GetTimeZoneInformation(&mut zone) } {
        u32::MAX => 0,
        TIME_ZONE_ID_DAYLIGHT => -(zone.Bias + zone.DaylightBias),
        _ => -(zone.Bias + zone.StandardBias),
    }
}
#[cfg(not(windows))]
fn local_offset_minutes() -> i32 {
    0
}
fn now_timestamp() -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    format_timestamp(millis, local_offset_minutes())
}
pub enum Record {
    Started,
    Stopped,
    IpcFailure,
    QueueRejected,
    AgentActivationFailure,
    /// Click-to-window latency; a duration is not payload data.
    AgentActivated {
        millis: u64,
    },
    RenderFailure,
    RenderFallback,
    InvalidConfiguration,
    ResidentStartupFailure,
    UsageFailure,
    /// Aggregated timings of one span over a window, in microseconds. Names are static
    /// and values numeric, so no event data can reach the log through these.
    Perf {
        span: &'static str,
        n: u64,
        avg: u64,
        sd: u64,
        min: u64,
        p95: u64,
        max: u64,
    },
    /// One operation that took longer than its span's threshold.
    Slow {
        span: &'static str,
        micros: u64,
        suppressed: u64,
    },
    Count {
        name: &'static str,
        n: u64,
    },
    Gauge {
        name: &'static str,
        value: u64,
    },
    /// Process-level counters, to see growth over a long-lived resident.
    Resources {
        working_set_kb: u64,
        gdi: u32,
        user: u32,
        threads: u32,
    },
}
/// Microseconds as milliseconds with two decimals (`12.34`), without floating point.
pub fn format_millis(micros: u64) -> String {
    format!("{}.{:02}", micros / 1000, micros % 1000 / 10)
}
fn ms(micros: u64) -> String {
    format!("{}ms", format_millis(micros))
}
impl Record {
    pub fn level(&self) -> Level {
        self.describe().0
    }
    /// Level and message text, as written after the timestamp.
    pub fn describe(&self) -> (Level, Cow<'static, str>) {
        match self {
            Self::Perf {
                span,
                n,
                avg,
                sd,
                min,
                p95,
                max,
            } => {
                let text = format!(
                    "perf {span} n={n} avg={} sd={} min={} p95={} max={}",
                    ms(*avg),
                    ms(*sd),
                    ms(*min),
                    ms(*p95),
                    ms(*max)
                );
                (Level::Debug, text.into())
            }
            Self::Slow {
                span,
                micros,
                suppressed,
            } => {
                let mut text = format!("slow {span} ms={}", format_millis(*micros));
                if *suppressed > 0 {
                    text.push_str(&format!(" suppressed={suppressed}"));
                }
                (Level::Warn, text.into())
            }
            Self::Count { name, n } => (Level::Debug, format!("count {name} n={n}").into()),
            Self::Gauge { name, value } => {
                (Level::Debug, format!("gauge {name} value={value}").into())
            }
            Self::Resources {
                working_set_kb,
                gdi,
                user,
                threads,
            } => (
                Level::Debug,
                format!(
                    "resources working_set_kb={working_set_kb} gdi={gdi} user={user} threads={threads}"
                )
                .into(),
            ),
            Self::Started => (Level::Info, "resident.started".into()),
            Self::Stopped => (Level::Info, "resident.stopped".into()),
            Self::IpcFailure => (Level::Warn, "ipc.failure".into()),
            Self::AgentActivationFailure => (Level::Warn, "agent.activation_failed".into()),
            Self::AgentActivated { millis } => {
                (Level::Debug, format!("agent.activated ms={millis}").into())
            }
            Self::QueueRejected => (Level::Warn, "ipc.queue_rejected".into()),
            Self::RenderFailure => (Level::Warn, "render.failed".into()),
            Self::RenderFallback => (Level::Warn, "render.fallback".into()),
            Self::InvalidConfiguration => (Level::Warn, "config.invalid".into()),
            Self::ResidentStartupFailure => (Level::Error, "resident.startup_failed".into()),
            Self::UsageFailure => (Level::Warn, "usage.failed".into()),
        }
    }
}
pub struct Log {
    directory: PathBuf,
    level: u8,
    file: Option<File>,
    bytes: u64,
}
impl Log {
    pub fn new(directory: &Path, level: LogLevel) -> Self {
        Self {
            directory: directory.to_owned(),
            level: threshold(level),
            file: None,
            bytes: 0,
        }
    }
    fn cap(&self) -> u64 {
        if self.level >= Level::Debug as u8 {
            MAX_DEBUG_LOG_BYTES
        } else {
            MAX_LOG_BYTES
        }
    }
    pub fn record(&mut self, record: Record) -> io::Result<()> {
        let (level, message) = record.describe();
        if self.level < level as u8 {
            return Ok(());
        }
        let line = format!("{} {} {message}\n", now_timestamp(), level.label());
        let path = self.directory.join("aherald.log");
        if self.file.is_none() {
            std::fs::create_dir_all(&self.directory)?;
            let file = OpenOptions::new().create(true).append(true).open(&path)?;
            self.bytes = file.metadata()?.len();
            self.file = Some(file);
        }
        let cap = self.cap();
        if self.bytes.saturating_add(line.len() as u64) > cap {
            self.file.take();
            let previous = self.directory.join("aherald.previous.log");
            match std::fs::remove_file(&previous) {
                Ok(()) => (),
                Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                Err(e) => return Err(e),
            }
            // Truncate an oversized pre-existing log before retaining it as the backup.
            OpenOptions::new()
                .write(true)
                .open(&path)?
                .set_len(self.bytes.min(cap))?;
            std::fs::rename(&path, previous)?;
            self.file = Some(OpenOptions::new().create(true).append(true).open(path)?);
            self.bytes = 0;
        }
        if let Some(file) = &mut self.file {
            file.write_all(line.as_bytes())?;
            self.bytes += line.len() as u64;
        }
        Ok(())
    }
}

/// Process-wide logger: one open file shared by every thread instead of a `Log` per call.
struct Shared {
    directory: PathBuf,
    level: LogLevel,
    log: Log,
}
static SHARED: Mutex<Option<Shared>> = Mutex::new(None);
/// Mirror of the configured threshold so a disabled record costs one atomic load.
static GATE: AtomicU8 = AtomicU8::new(0);
/// Points the shared logger at a directory and level. Repeating the current settings
/// keeps the open file; changing them starts a fresh handle.
pub fn init(directory: &Path, level: LogLevel) {
    let mut shared = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    if shared
        .as_ref()
        .is_some_and(|s| s.directory == directory && s.level == level)
    {
        return;
    }
    *shared = Some(Shared {
        directory: directory.to_owned(),
        level,
        log: Log::new(directory, level),
    });
    GATE.store(threshold(level), Ordering::Release);
}
pub fn enabled(level: Level) -> bool {
    GATE.load(Ordering::Acquire) >= level as u8
}
/// Writes through the shared logger; dropped before `init` or when the level is off.
/// Logging failures never reach the caller.
pub fn log(record: Record) {
    if !enabled(record.level()) {
        return;
    }
    let mut shared = SHARED.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(shared) = shared.as_mut() {
        let _ = shared.log.record(record);
    }
}
