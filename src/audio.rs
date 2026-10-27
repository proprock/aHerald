use crate::{config::Config, events::Event};
use std::{fs::File, io::Read, path::Path};
const MAX_BYTES: usize = 2 * 1024 * 1024;
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
    pub fn parse(bytes: &[u8], volume: f64) -> Result<Self, String> {
        if bytes.len() < 12
            || bytes.len() > MAX_BYTES
            || &bytes[..4] != b"RIFF"
            || &bytes[8..12] != b"WAVE"
            || !volume.is_finite()
            || !(0.0..=1.0).contains(&volume)
        {
            return Err("invalid WAV or volume".into());
        }
        let end = (u32_at(bytes, 4) as usize)
            .checked_add(8)
            .ok_or("invalid RIFF length")?;
        if end != bytes.len() {
            return Err("invalid RIFF length".into());
        }
        let mut format = None;
        let mut data = None;
        let mut p = 12;
        while p < end {
            if end - p < 8 {
                return Err("truncated WAV chunk".into());
            }
            let length = u32_at(bytes, p + 4) as usize;
            let next = p
                .checked_add(8)
                .and_then(|v| v.checked_add(length))
                .ok_or("invalid WAV chunk")?;
            if next > end {
                return Err("truncated WAV chunk".into());
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
                        return Err("only 16-bit mono/stereo PCM WAV is supported".into());
                    }
                    format = Some((channels, rate));
                }
                b"data" if data.is_none() => data = Some(&bytes[p + 8..next]),
                b"fmt " | b"data" => return Err("duplicate or invalid WAV chunk".into()),
                _ => {}
            }
            p = next.checked_add(length % 2).ok_or("invalid WAV padding")?;
            if p > end {
                return Err("missing WAV padding".into());
            }
        }
        let (channels, sample_rate) = format.ok_or("missing WAV format")?;
        let data = data.ok_or("missing WAV data")?;
        if data.is_empty()
            || data.len() % (channels as usize * 2) != 0
            || data.len() as u64 > u64::from(sample_rate) * u64::from(channels) * 2 * 5
        {
            return Err("invalid WAV duration or sample alignment".into());
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
    pub fn load(path: &Path, volume: f64) -> Result<Self, String> {
        let mut bytes = Vec::new();
        File::open(path)
            .map_err(|_| "cannot open WAV")?
            .take(MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "cannot read WAV")?;
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
    pub fn get(&mut self, path: &Path, volume: f64) -> Result<std::sync::Arc<Wave>, String> {
        let meta = std::fs::metadata(path).map_err(|_| "cannot open WAV")?;
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
#[cfg(windows)]
fn cached(path: &Path, volume: f64) -> Result<std::sync::Arc<Wave>, String> {
    static CACHE: std::sync::OnceLock<std::sync::Mutex<SoundCache>> = std::sync::OnceLock::new();
    CACHE
        .get_or_init(|| std::sync::Mutex::new(SoundCache::new(8)))
        .lock()
        .map_err(|_| "sound cache unavailable")?
        .get(path, volume)
}
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
#[cfg(windows)]
pub fn start(event: &Event, config: &Config) -> Option<std::thread::JoinHandle<()>> {
    start_controlled(
        event,
        config,
        std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
    )
}
#[cfg(windows)]
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
    Some(std::thread::spawn(move || {
        if let Ok(wave) = cached(&path, volume) {
            let _ = native::play(&wave, &key, dedup, &cancel);
        }
    }))
}
