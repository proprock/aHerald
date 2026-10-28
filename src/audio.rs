use crate::{config::Config, events::Event};
use std::{fs::File, io::Read, path::Path};
const MAX_BYTES: usize = 2 * 1024 * 1024;
/// Why a sound file cannot be used. Fixed reasons only: nothing about the file's name or content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundError {
    /// Missing, not a file, or not readable.
    Unreadable,
    /// More than 2 MiB.
    TooLarge,
    /// More than five seconds of audio.
    TooLong,
    /// A WAV, but not 16-bit mono or stereo PCM at 8000-48000 Hz.
    Unsupported,
    /// Not a well-formed WAV (or an invalid volume).
    Malformed,
}
impl SoundError {
    /// The word written to the log.
    pub fn code(self) -> &'static str {
        match self {
            Self::Unreadable => "unreadable",
            Self::TooLarge => "too_large",
            Self::TooLong => "too_long",
            Self::Unsupported => "unsupported",
            Self::Malformed => "malformed",
        }
    }
}
impl std::fmt::Display for SoundError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unreadable => "cannot read WAV",
            Self::TooLarge => "WAV exceeds 2 MiB",
            Self::TooLong => "WAV is longer than five seconds",
            Self::Unsupported => "only 16-bit mono/stereo PCM WAV is supported",
            Self::Malformed => "invalid WAV",
        })
    }
}
#[derive(Debug)]
pub struct Wave {
    pub channels: u16,
    pub sample_rate: u32,
    pub samples: Vec<i16>,
}
fn u16_at(b: &[u8], p: usize) -> u16 {
    u16::from_le_bytes([b[p], b[p + 1]])
}
fn u32_at(b: &[u8], p: usize) -> u32 {
    u32::from_le_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]])
}
impl Wave {
    pub fn parse(bytes: &[u8], volume: f64) -> Result<Self, SoundError> {
        if bytes.len() > MAX_BYTES {
            return Err(SoundError::TooLarge);
        }
        if bytes.len() < 12
            || &bytes[..4] != b"RIFF"
            || &bytes[8..12] != b"WAVE"
            || !volume.is_finite()
            || !(0.0..=1.0).contains(&volume)
        {
            return Err(SoundError::Malformed);
        }
        let end = (u32_at(bytes, 4) as usize)
            .checked_add(8)
            .ok_or(SoundError::Malformed)?;
        if end != bytes.len() {
            return Err(SoundError::Malformed);
        }
        let mut format = None;
        let mut data = None;
        let mut p = 12;
        while p < end {
            if end - p < 8 {
                return Err(SoundError::Malformed);
            }
            let length = u32_at(bytes, p + 4) as usize;
            let next = p
                .checked_add(8)
                .and_then(|v| v.checked_add(length))
                .ok_or(SoundError::Malformed)?;
            if next > end {
                return Err(SoundError::Malformed);
            }
            match &bytes[p..p + 4] {
                b"fmt " if format.is_none() && length >= 16 => {
                    let b = &bytes[p + 8..next];
                    let channels = u16_at(b, 2);
                    let rate = u32_at(b, 4);
                    if u16_at(b, 0) != 1
                        || !(1..=2).contains(&channels)
                        || !(8000..=48000).contains(&rate)
                        || u16_at(b, 14) != 16
                        || u16_at(b, 12) != channels * 2
                        || u32_at(b, 8) != rate * u32::from(channels) * 2
                    {
                        return Err(SoundError::Unsupported);
                    }
                    format = Some((channels, rate));
                }
                b"data" if data.is_none() => data = Some(&bytes[p + 8..next]),
                b"fmt " | b"data" => return Err(SoundError::Malformed),
                _ => {}
            }
            p = next.checked_add(length % 2).ok_or(SoundError::Malformed)?;
            if p > end {
                return Err(SoundError::Malformed);
            }
        }
        let (channels, sample_rate) = format.ok_or(SoundError::Malformed)?;
        let data = data.ok_or(SoundError::Malformed)?;
        if data.is_empty() || data.len() % (channels as usize * 2) != 0 {
            return Err(SoundError::Malformed);
        }
        if data.len() as u64 > u64::from(sample_rate) * u64::from(channels) * 2 * 5 {
            return Err(SoundError::TooLong);
        }
        let samples = data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| (f64::from(i16::from_le_bytes(*b)) * volume).round() as i16)
            .collect();
        Ok(Self {
            channels,
            sample_rate,
            samples,
        })
    }
    pub fn load(path: &Path, volume: f64) -> Result<Self, SoundError> {
        let _timer = crate::perf::span(crate::perf::Span::AudioLoad);
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(|_| SoundError::Unreadable)?
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| SoundError::Unreadable)?;
        Self::parse(&bytes, volume)
    }
}
type SoundKey = (std::path::PathBuf, u64, u64, Option<std::time::SystemTime>);
/// Decoded sounds, most recently used last. The key holds the file's size and
/// modification time, so a replaced file is decoded again and a deleted one is an error.
pub struct SoundCache {
    entries: std::collections::VecDeque<(SoundKey, std::sync::Arc<Wave>)>,
    capacity: usize,
}
impl SoundCache {
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Default::default(),
            capacity: capacity.max(1),
        }
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn get(&mut self, path: &Path, volume: f64) -> Result<std::sync::Arc<Wave>, SoundError> {
        let meta = std::fs::metadata(path).map_err(|_| SoundError::Unreadable)?;
        let key = (
            path.to_owned(),
            volume.to_bits(),
            meta.len(),
            meta.modified().ok(),
        );
        if let Some(index) = self.entries.iter().position(|(k, _)| *k == key)
            && let Some(entry) = self.entries.remove(index)
        {
            let wave = entry.1.clone();
            self.entries.push_back(entry);
            return Ok(wave);
        }
        let wave = std::sync::Arc::new(Wave::load(path, volume)?);
        self.entries.push_back((key, wave.clone()));
        while self.entries.len() > self.capacity {
            self.entries.pop_front();
        }
        Ok(wave)
    }
}
/// The sounds of one process: the resident replays the same few files all day.
#[cfg(any(windows, target_os = "macos"))]
fn cached(path: &Path, volume: f64) -> Result<std::sync::Arc<Wave>, SoundError> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<SoundCache>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| std::sync::Mutex::new(SoundCache::new(8)))
        .lock()
        .map_err(|_| SoundError::Unreadable)?
        .get(path, volume)
}

/// Logs why `path` cannot be used, once for each state of that file. The log gets the reason
/// only, never the path.
#[cfg(any(windows, target_os = "macos"))]
fn report_rejection(path: &Path, error: SoundError) {
    crate::operational_log::log_once(
        "sound",
        path,
        crate::operational_log::Record::SoundRejected(error.code()),
    );
}

/// Logs the first failure of a run of failures to play; a success ends the run.
#[cfg(any(windows, target_os = "macos"))]
fn note_playback(played: bool) {
    use std::sync::atomic::{AtomicBool, Ordering};
    static FAILING: AtomicBool = AtomicBool::new(false);
    if played {
        FAILING.store(false, Ordering::Relaxed);
    } else if !FAILING.swap(true, Ordering::Relaxed) {
        crate::operational_log::log(crate::operational_log::Record::SoundFailed);
    }
}
#[cfg(unix)]
#[path = "unix/audio.rs"]
pub mod serial;

pub fn selected<'a>(config: &'a Config, event: &Event) -> Option<&'a Path> {
    let settings = &config.events[event.kind];
    (config.sounds.enabled
        && config.sounds.volume > 0.0
        && settings.enabled
        && settings.sound_enabled)
        .then_some(settings.sound.as_deref())
        .flatten()
}
#[cfg(windows)]
#[path = "audio_windows.rs"]
mod native;

/// One sound through the platform's output, serialized across processes.
#[cfg(any(windows, target_os = "macos"))]
fn play(
    wave: &Wave,
    key: &[u8],
    dedup_ms: u64,
    cancel: &std::sync::atomic::AtomicBool,
) -> Result<(), String> {
    #[cfg(windows)]
    {
        native::play(wave, key, dedup_ms, cancel)
    }
    #[cfg(target_os = "macos")]
    {
        serial::play_with(
            serial::SERIAL_GATE,
            wave,
            key,
            dedup_ms,
            cancel,
            crate::mac::audio::output,
        )
    }
}

#[cfg(any(windows, target_os = "macos"))]
pub fn start(event: &Event, config: &Config) -> Option<std::thread::JoinHandle<()>> {
    start_controlled(
        event,
        config,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
}
#[cfg(any(windows, target_os = "macos"))]
pub fn start_controlled(
    event: &Event,
    config: &Config,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> Option<std::thread::JoinHandle<()>> {
    let path = selected(config, event)?.to_owned();
    let volume = config.sounds.volume;
    let dedup = if config.sounds.suppress_repeats {
        config.general.deduplicate_ms
    } else {
        0
    };
    let key = serde_json::to_vec(event).ok()?;
    Some(std::thread::spawn(move || match cached(&path, volume) {
        Ok(wave) => note_playback(play(&wave, &key, dedup, &cancel).is_ok()),
        Err(error) => report_rejection(&path, error),
    }))
}
