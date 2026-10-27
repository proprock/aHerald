//! Fixed operational codes only: this API cannot accept payloads or external errors.
use crate::config::LogLevel;
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
pub const MAX_LOG_BYTES: u64 = 64 * 1024;
pub enum Record {
    Started,
    Stopped,
    IpcFailure,
    QueueRejected,
    AgentActivationFailure,
    RenderFailure,
    RenderFallback,
    InvalidConfiguration,
    ResidentStartupFailure,
    UsageFailure,
}
impl Record {
    fn message(&self) -> (u8, &'static str) {
        match self {
            Self::Started => (3, "info resident_started"),
            Self::Stopped => (3, "info resident_stopped"),
            Self::IpcFailure => (2, "warn ipc_failure"),
            Self::AgentActivationFailure => (2, "warn agent_activation_failed"),
            Self::QueueRejected => (2, "warn queue_rejected"),
            Self::RenderFailure => (2, "warn render_failed"),
            Self::RenderFallback => (2, "warn render_fallback"),
            Self::InvalidConfiguration => (2, "warn invalid_configuration"),
            Self::ResidentStartupFailure => (1, "error resident_startup_failed"),
            Self::UsageFailure => (2, "warn usage_failed"),
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
            level: match level {
                LogLevel::Off => 0,
                LogLevel::Error => 1,
                LogLevel::Warn => 2,
                LogLevel::Info => 3,
                LogLevel::Debug | LogLevel::Trace => 4,
            },
            file: None,
            bytes: 0,
        }
    }
    pub fn record(&mut self, record: Record) -> io::Result<()> {
        let (level, message) = record.message();
        if self.level < level {
            return Ok(());
        }
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let line = format!("{seconds} {message}\n");
        let path = self.directory.join("aherald.log");
        if self.file.is_none() {
            std::fs::create_dir_all(&self.directory)?;
            let file = OpenOptions::new().create(true).append(true).open(&path)?;
            self.bytes = file.metadata()?.len();
            self.file = Some(file);
        }
        if self.bytes.saturating_add(line.len() as u64) > MAX_LOG_BYTES {
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
                .set_len(self.bytes.min(MAX_LOG_BYTES))?;
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
