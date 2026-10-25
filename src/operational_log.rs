//! Fixed operational codes only: this API cannot accept payloads or external errors.
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
}
impl Record {
    fn message(&self) -> (u8, &'static str) {
        match self {
            Self::Started => (3, "info resident_started"),
            Self::Stopped => (3, "info resident_stopped"),
            Self::IpcFailure => (2, "warn ipc_failure"),
            Self::QueueRejected => (2, "warn queue_rejected"),
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
    pub fn new(directory: &Path, level: &str) -> Self {
        Self {
            directory: directory.to_owned(),
            level: match level {
                "error" => 1,
                "warn" => 2,
                "info" => 3,
                "debug" | "trace" => 4,
                _ => 0,
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
