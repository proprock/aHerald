use crate::events::Kind;
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    #[serde(skip)]
    pub diagnostic_directory: PathBuf,
    pub general: General,
    pub resident: Resident,
    pub tray: Tray,
    pub notifications: Notifications,
    pub sounds: Sounds,
    pub events: Events,
    pub agents: BTreeMap<String, AgentAssets>,
    pub codex: Codex,
    pub claude: Claude,
}

/// Per-kind event settings; every kind always has an entry, so indexing cannot fail.
#[derive(Clone, Debug)]
pub struct Events([EventConfig; 5]);
impl Events {
    pub fn iter(&self) -> impl Iterator<Item = (Kind, &EventConfig)> {
        Kind::ALL.into_iter().zip(self.0.iter())
    }
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (Kind, &mut EventConfig)> {
        Kind::ALL.into_iter().zip(self.0.iter_mut())
    }
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut EventConfig> {
        self.0.iter_mut()
    }
    fn slot(kind: Kind) -> usize {
        Kind::ALL
            .iter()
            .position(|candidate| *candidate == kind)
            .unwrap_or_default()
    }
}
impl Default for Events {
    fn default() -> Self {
        Self(Kind::ALL.map(|kind| EventConfig {
            // Waits for the user stay visible until answered or dismissed.
            duration_ms: if matches!(kind, Kind::PermissionRequired | Kind::InputRequired) {
                0
            } else {
                3000
            },
            ..EventConfig::default()
        }))
    }
}
impl std::ops::Index<Kind> for Events {
    type Output = EventConfig;
    fn index(&self, kind: Kind) -> &EventConfig {
        &self.0[Self::slot(kind)]
    }
}
impl std::ops::IndexMut<Kind> for Events {
    fn index_mut(&mut self, kind: Kind) -> &mut EventConfig {
        &mut self.0[Self::slot(kind)]
    }
}
impl<'de> Deserialize<'de> for Events {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let table = BTreeMap::<String, EventConfig>::deserialize(deserializer)?;
        let mut events = Self::default();
        for (name, event) in table {
            // The message never includes the key; users may type private text there.
            let kind = Kind::from_name(&name)
                .ok_or_else(|| serde::de::Error::custom("unknown event kind"))?;
            events[kind] = event;
        }
        Ok(events)
    }
}
impl Default for Config {
    fn default() -> Self {
        Self {
            diagnostic_directory: PathBuf::from("."),
            general: General::default(),
            resident: Resident::default(),
            tray: Tray::default(),
            notifications: Notifications::default(),
            sounds: Sounds::default(),
            codex: Codex::default(),
            claude: Claude::default(),
            agents: BTreeMap::new(),
            events: Events::default(),
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Tray {
    pub graph: TrayGraph,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TrayGraph {
    #[default]
    Auto,
    Both,
    Claude,
    Codex,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AgentAssets {
    pub pet: Option<PathBuf>,
    pub pet_animations: BTreeMap<String, PetAnimation>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PetAnimation {
    Idle,
    #[serde(rename = "running-right")]
    RunningRight,
    #[serde(rename = "running-left")]
    RunningLeft,
    Waving,
    Jumping,
    Waiting,
    Failed,
    Running,
    Review,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(try_from = "String")]
pub struct Color(pub [u8; 3]);
impl TryFrom<String> for Color {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let bytes = value.as_bytes();
        if bytes.len() != 7 || bytes[0] != b'#' || !bytes[1..].iter().all(u8::is_ascii_hexdigit) {
            return Err("color must be #RRGGBB".into());
        }
        let mut rgb = [0; 3];
        for (index, channel) in rgb.iter_mut().enumerate() {
            *channel = u8::from_str_radix(&value[1 + index * 2..3 + index * 2], 16)
                .map_err(|_| "invalid color")?;
        }
        Ok(Self(rgb))
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Resident,
    #[default]
    Transient,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Off,
    Error,
    #[default]
    Warn,
    Info,
    Debug,
    Trace,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Position {
    #[default]
    Center,
    TopCenter,
    BottomRight,
    NearCursor,
}

/// `"primary"` or a zero-based monitor number written as a string.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(try_from = "String")]
pub enum Monitor {
    #[default]
    Primary,
    Index(usize),
}
impl TryFrom<String> for Monitor {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value == "primary" {
            return Ok(Self::Primary);
        }
        value
            .parse::<u32>()
            .map(|index| Self::Index(index as usize))
            .map_err(|_| "monitor must be \"primary\" or a monitor number".into())
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Interpolation {
    #[default]
    Nearest,
    Bilinear,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct General {
    pub mode: Mode,
    pub logging: LogLevel,
    pub deduplicate_ms: u64,
}
impl Default for General {
    fn default() -> Self {
        Self {
            mode: Mode::default(),
            logging: LogLevel::default(),
            deduplicate_ms: 1500,
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Resident {
    pub auto_start: bool,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Notifications {
    pub background_color: Color,
    pub visual: Visual,
    pub pet: Option<PathBuf>,
    pub pet_animations: BTreeMap<String, PetAnimation>,
    pub enabled: bool,
    pub position: Position,
    pub monitor: Monitor,
    pub scale: f64,
    pub fade_in_ms: u64,
    pub fade_out_ms: u64,
    pub click_to_dismiss: bool,
    pub activate_agent_on_click: bool,
    pub interpolation: Interpolation,
}
impl Default for Notifications {
    fn default() -> Self {
        Self {
            background_color: Color([28, 35, 45]),
            visual: Visual::Auto,
            enabled: true,
            position: Position::default(),
            monitor: Monitor::default(),
            scale: 1.0,
            fade_in_ms: 150,
            fade_out_ms: 300,
            click_to_dismiss: true,
            activate_agent_on_click: true,
            interpolation: Interpolation::default(),
            pet: None,
            pet_animations: BTreeMap::new(),
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
    pub background_color: Option<Color>,
    pub visual: EventVisual,
    pub enabled: bool,
    pub animation: Option<PathBuf>,
    pub sound: Option<PathBuf>,
    pub sound_enabled: bool,
    pub duration_ms: u64,
    pub color: Option<Color>,
}
impl Default for EventConfig {
    fn default() -> Self {
        Self {
            background_color: None,
            visual: EventVisual::Inherit,
            enabled: true,
            animation: None,
            sound: None,
            sound_enabled: true,
            duration_ms: 3000,
            color: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Visual {
    #[default]
    Auto,
    Builtin,
    Pet,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventVisual {
    #[default]
    Inherit,
    Builtin,
    Pet,
    Custom,
}
/// Opt-in Claude Code quota polling; see `claude_usage` for the token-handling exception.
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ClaudeUsage {
    pub enabled: bool,
    pub refresh_interval_sec: u64,
}
impl Default for ClaudeUsage {
    fn default() -> Self {
        Self {
            enabled: false,
            refresh_interval_sec: 300,
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Claude {
    pub hooks_enabled: bool,
    pub usage: ClaudeUsage,
}
impl Default for Claude {
    fn default() -> Self {
        Self {
            hooks_enabled: true,
            usage: ClaudeUsage::default(),
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Codex {
    pub hooks_enabled: bool,
    pub input_monitoring: bool,
    pub usage: Usage,
}
impl Default for Codex {
    fn default() -> Self {
        Self {
            hooks_enabled: true,
            input_monitoring: false,
            usage: Usage::default(),
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Usage {
    pub background_color: Option<Color>,
    pub enabled: bool,
    pub refresh_interval_sec: u64,
    pub display: UsageDisplay,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum UsageDisplay {
    Used,
    #[default]
    Remain,
}
impl Default for Usage {
    fn default() -> Self {
        Self {
            background_color: None,
            enabled: false,
            refresh_interval_sec: 300,
            display: UsageDisplay::Remain,
        }
    }
}
impl Config {
    pub fn event_background_color(&self, kind: crate::events::Kind) -> Color {
        self.events[kind]
            .background_color
            .unwrap_or(self.notifications.background_color)
    }

    pub fn usage_background_color(&self) -> Color {
        self.codex
            .usage
            .background_color
            .unwrap_or(self.notifications.background_color)
    }

    pub fn event_color(&self, kind: crate::events::Kind) -> Color {
        self.events[kind].color.unwrap_or(Color(match kind {
            Kind::TurnComplete => [65, 210, 140],
            Kind::PermissionRequired => [250, 180, 55],
            Kind::InputRequired => [80, 165, 250],
            Kind::Error => [245, 85, 100],
            Kind::Interrupted => [180, 130, 240],
        }))
    }
    pub fn default_path() -> Result<PathBuf, String> {
        let executable =
            std::env::current_exe().map_err(|e| format!("cannot locate executable: {e}"))?;
        executable
            .parent()
            .map(|directory| directory.join("config.toml"))
            .ok_or_else(|| "executable has no parent directory; use --config".into())
    }
    pub fn parse(text: &str, directory: &Path) -> Result<Self, String> {
        let value: toml::Value = toml::from_str(text).map_err(|error| {
            let line = error.span().map_or(0, |s| {
                text[..s.start.min(text.len())].matches('\n').count() + 1
            });
            match line {
                0 => "invalid TOML syntax".to_owned(),
                line => format!("invalid TOML syntax at line {line}"),
            }
        })?;
        // Only the key path and a fixed reason are reported; values may be private.
        let mut config: Self = toml::from_str(text).map_err(|error| {
            let reason = match error.message() {
                m if m.starts_with("unknown field") => "is not a supported setting",
                m if m.starts_with("invalid type") => "has the wrong type",
                m if m.starts_with("missing field") => "is missing",
                m if m.starts_with("unknown event kind") => "is not a supported event",
                _ => "has an unsupported value",
            };
            match error.span().and_then(|s| field_path(text, s.start)) {
                Some(path) => format!("invalid configuration: {path} {reason}"),
                None => format!("invalid configuration: a setting {reason}"),
            }
        })?;
        config.validate()?;
        config.diagnostic_directory = directory.to_owned();
        // A table that omits `duration_ms` keeps the kind's own default, not the struct's.
        let defaults = Events::default();
        for (kind, event) in config.events.iter_mut() {
            let has_duration = value
                .get("events")
                .and_then(|events| events.get(kind.name()))
                .and_then(|event| event.get("duration_ms"))
                .is_some();
            if !has_duration {
                event.duration_ms = defaults[kind].duration_ms;
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
                crate::default_config::create(path)?;
                File::open(path)
                    .map_err(|e| format!("cannot open configuration {}: {e}", path.display()))?
            }
            Err(e) => return Err(format!("cannot open configuration {}: {e}", path.display())),
        };
        let mut text = String::new();
        file.take(65_537)
            .read_to_string(&mut text)
            .map_err(|e| format!("cannot read configuration {}: {e}", path.display()))?;
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
        // (valid, setting, requirement); messages never include the offending value.
        let checks = [
            (
                self.general.deduplicate_ms <= 60_000,
                "general.deduplicate_ms",
                "must be at most 60000",
            ),
            (
                n.scale.is_finite() && (0.25..=4.0).contains(&n.scale),
                "notifications.scale",
                "must be between 0.25 and 4",
            ),
            (
                n.fade_in_ms <= 10_000,
                "notifications.fade_in_ms",
                "must be at most 10000",
            ),
            (
                n.fade_out_ms <= 10_000,
                "notifications.fade_out_ms",
                "must be at most 10000",
            ),
            (
                self.sounds.volume.is_finite() && (0.0..=1.0).contains(&self.sounds.volume),
                "sounds.volume",
                "must be between 0 and 1",
            ),
            (
                (10..=86_400).contains(&self.codex.usage.refresh_interval_sec),
                "codex.usage.refresh_interval_sec",
                "must be between 10 and 86400",
            ),
            // The endpoint is undocumented; stay polite (rate-tray also spaces requests >= 300 s).
            (
                (300..=86_400).contains(&self.claude.usage.refresh_interval_sec),
                "claude.usage.refresh_interval_sec",
                "must be between 300 and 86400",
            ),
            (
                !self.resident.auto_start,
                "resident.auto_start",
                "must be false (autostart is configured manually)",
            ),
        ];
        if let Some((_, setting, requirement)) = checks.iter().find(|(ok, ..)| !ok) {
            return Err(format!("invalid configuration: {setting} {requirement}"));
        }
        for (kind, event) in self.events.iter() {
            let name = kind.name();
            if event.duration_ms > 86_400_000 {
                return Err(format!(
                    "invalid configuration: events.{name}.duration_ms must be at most 86400000"
                ));
            }
            if event.visual == EventVisual::Custom && event.animation.is_none() {
                return Err(format!(
                    "invalid configuration: events.{name}.animation is required when visual is \"custom\""
                ));
            }
        }
        for (owner, mapping) in std::iter::once(("notifications", &n.pet_animations)).chain(
            self.agents
                .iter()
                .map(|(agent, a)| (agent.as_str(), &a.pet_animations)),
        ) {
            if let Some(name) = mapping.keys().find(|name| Kind::from_name(name).is_none()) {
                return Err(format!(
                    "invalid configuration: {}.pet_animations.{} is not a supported event",
                    safe_key(owner),
                    safe_key(name)
                ));
            }
        }
        Ok(())
    }
}

/// A user-written key is echoed only when it is a plain identifier of sane length.
fn safe_key(key: &str) -> &str {
    if key.len() <= 40
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        key
    } else {
        "<invalid>"
    }
}

/// Dotted key path (`table.key`) of the TOML line containing `offset`, if plain.
fn field_path(text: &str, offset: usize) -> Option<String> {
    let offset = offset.min(text.len());
    let start = text[..offset].rfind('\n').map_or(0, |i| i + 1);
    let end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    let line = text[start..end].trim();
    let header = |l: &str| {
        let l = l.trim();
        l.strip_prefix('[')
            .and_then(|l| l.strip_suffix(']'))
            .map(|l| l.trim_matches(['[', ']']).trim().to_owned())
    };
    let table = text[..start].lines().rev().find_map(header);
    let path = match header(line) {
        Some(table) => table,
        None => {
            let key = line.split('=').next()?.trim();
            match table {
                Some(table) if !table.is_empty() => format!("{table}.{key}"),
                _ => key.to_owned(),
            }
        }
    };
    let plain = !path.is_empty()
        && path.len() <= 80
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    plain.then_some(path)
}
