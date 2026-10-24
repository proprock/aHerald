use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};
pub const EVENT_NAMES: [&str; 5] = [
    "turn_complete",
    "permission_required",
    "input_required",
    "error",
    "interrupted",
];

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: General,
    pub resident: Resident,
    pub notifications: Notifications,
    pub sounds: Sounds,
    pub events: BTreeMap<String, EventConfig>,
    pub agents: BTreeMap<String, AgentAssets>,
    pub codex: Codex,
    pub usage: Usage,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            general: General::default(),
            resident: Resident::default(),
            notifications: Notifications::default(),
            sounds: Sounds::default(),
            codex: Codex::default(),
            usage: Usage::default(),
            agents: BTreeMap::new(),
            events: EVENT_NAMES
                .into_iter()
                .map(|name| {
                    (
                        name.into(),
                        EventConfig {
                            duration_ms: if matches!(name, "permission_required" | "input_required")
                            {
                                0
                            } else {
                                3000
                            },
                            ..EventConfig::default()
                        },
                    )
                })
                .collect(),
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentAssets {
    pub pet: Option<PathBuf>,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    pub mode: String,
    pub logging: String,
    pub deduplicate_ms: u64,
}
impl Default for General {
    fn default() -> Self {
        Self {
            mode: "transient".into(),
            logging: "warn".into(),
            deduplicate_ms: 1500,
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Resident {
    pub tray_enabled: bool,
    pub auto_start: bool,
}
impl Default for Resident {
    fn default() -> Self {
        Self {
            tray_enabled: true,
            auto_start: false,
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Notifications {
    pub pet: Option<PathBuf>,
    pub enabled: bool,
    pub position: String,
    pub monitor: String,
    pub scale: f64,
    pub fade_in_ms: u64,
    pub fade_out_ms: u64,
    pub click_to_dismiss: bool,
    pub interpolation: String,
}
impl Default for Notifications {
    fn default() -> Self {
        Self {
            enabled: true,
            position: "center".into(),
            monitor: "primary".into(),
            scale: 1.0,
            fade_in_ms: 150,
            fade_out_ms: 300,
            click_to_dismiss: true,
            interpolation: "nearest".into(),
            pet: None,
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Sounds {
    pub enabled: bool,
    pub volume: f64,
    pub suppress_repeats: bool,
}
impl Default for Sounds {
    fn default() -> Self {
        Self {
            enabled: true,
            volume: 0.75,
            suppress_repeats: true,
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct EventConfig {
    pub enabled: bool,
    pub animation: Option<PathBuf>,
    pub sound: Option<PathBuf>,
    pub sound_enabled: bool,
    pub duration_ms: u64,
}
impl Default for EventConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            animation: None,
            sound: None,
            sound_enabled: true,
            duration_ms: 3000,
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Codex {
    pub hooks_enabled: bool,
    pub input_monitoring: bool,
}
impl Default for Codex {
    fn default() -> Self {
        Self {
            hooks_enabled: true,
            input_monitoring: false,
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Usage {
    pub enabled: bool,
    pub refresh_interval_sec: u64,
}
impl Default for Usage {
    fn default() -> Self {
        Self {
            enabled: false,
            refresh_interval_sec: 300,
        }
    }
}
impl Config {
    pub fn default_path() -> Result<PathBuf, String> {
        std::env::var_os("APPDATA")
            .map(|base| PathBuf::from(base).join("aHerald/config.toml"))
            .ok_or("APPDATA is unavailable; use --config".into())
    }
    pub fn parse(text: &str, directory: &Path) -> Result<Self, String> {
        let value: toml::Value = toml::from_str(text).map_err(|_| "invalid TOML configuration")?;
        let mut config: Self = value
            .clone()
            .try_into()
            .map_err(|_| "invalid configuration fields")?;
        config.validate()?;
        for (name, default) in Self::default().events {
            let has_duration = value
                .get("events")
                .and_then(|events| events.get(&name))
                .and_then(|event| event.get("duration_ms"))
                .is_some();
            let event = config.events.entry(name).or_insert_with(|| default.clone());
            if !has_duration {
                event.duration_ms = default.duration_ms;
            }
        }
        for event in config.events.values_mut() {
            for path in [&mut event.animation, &mut event.sound]
                .into_iter()
                .flatten()
            {
                if path.is_relative() {
                    *path = directory.join(&*path);
                }
            }
        }
        for path in std::iter::once(&mut config.notifications.pet)
            .chain(config.agents.values_mut().map(|agent| &mut agent.pet))
            .flatten()
        {
            if path.is_relative() {
                *path = directory.join(&*path);
            }
        }
        Ok(config)
    }
    pub fn load(path: &Path, explicit: bool) -> Result<Self, String> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if !explicit && error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(_) => return Err("cannot open configuration file".into()),
        };
        let mut text = String::new();
        file.take(65_537)
            .read_to_string(&mut text)
            .map_err(|_| "cannot read configuration file")?;
        if text.len() > 65_536 {
            return Err("configuration exceeds 64 KiB".into());
        }
        let absolute =
            std::path::absolute(path).map_err(|_| "cannot resolve configuration path")?;
        Self::parse(
            &text,
            absolute.parent().ok_or("configuration has no directory")?,
        )
    }
    fn validate(&self) -> Result<(), String> {
        let n = &self.notifications;
        if !["resident", "transient"].contains(&self.general.mode.as_str())
            || !["off", "error", "warn", "info", "debug", "trace"]
                .contains(&self.general.logging.as_str())
            || self.general.deduplicate_ms > 60_000
            || !["center", "top_center", "bottom_right", "near_cursor"]
                .contains(&n.position.as_str())
            || !(n.monitor == "primary" || n.monitor.parse::<u32>().is_ok())
            || !n.scale.is_finite()
            || !(0.25..=4.0).contains(&n.scale)
            || n.fade_in_ms > 10_000
            || n.fade_out_ms > 10_000
            || !["nearest", "bilinear"].contains(&n.interpolation.as_str())
            || !self.sounds.volume.is_finite()
            || !(0.0..=1.0).contains(&self.sounds.volume)
            || !(10..=86_400).contains(&self.usage.refresh_interval_sec)
            || self.events.iter().any(|(name, event)| {
                !EVENT_NAMES.contains(&name.as_str()) || event.duration_ms > 86_400_000
            })
            || self.resident.auto_start
        {
            return Err(
                "unsupported or out-of-range configuration (auto_start must be false)".into(),
            );
        }
        Ok(())
    }
}
