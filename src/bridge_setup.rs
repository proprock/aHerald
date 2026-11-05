//! The files aHerald installs for harnesses that load a plugin or hook file (OpenCode, Pi,
//! Goose). Each file is wholly aHerald's: it is rewritten only while it still holds the text
//! aHerald recorded, so a file the user wrote or edited is never touched. Crush is configured by
//! hand because its `crushrc` is the user's own script.
use serde_json::Value;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bridge {
    OpenCode,
    Pi,
    Goose,
}

const OPENCODE: &str = include_str!("../examples/harnesses/opencode-aherald.ts");
const PI: &str = include_str!("../examples/harnesses/pi-aherald.ts");
const GOOSE: &str = include_str!("../examples/harnesses/goose-hooks.json");
/// The only place the bridges name the program; setup replaces it with the full path.
const TS_DEFAULT: &str = r#"process.env.AHERALD_EXE || "aherald""#;

impl Bridge {
    pub const ALL: [Self; 3] = [Self::OpenCode, Self::Pi, Self::Goose];
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "opencode" => Some(Self::OpenCode),
            "pi" => Some(Self::Pi),
            "goose" => Some(Self::Goose),
            _ => None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::OpenCode => "opencode",
            Self::Pi => "pi",
            Self::Goose => "goose",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenCode => "OpenCode",
            Self::Pi => "Pi",
            Self::Goose => "Goose",
        }
    }
    pub fn restart_hint(self) -> &'static str {
        match self {
            Self::OpenCode => "Restart OpenCode to load the plugin.",
            Self::Pi => "Restart Pi (or run /reload) to load the extension.",
            Self::Goose => "Start a new Goose session to load the hooks.",
        }
    }

    /// Where the harness loads the file from, honouring the harness's own overrides.
    pub fn file(self, get: impl Fn(&str) -> Option<OsString>) -> Result<PathBuf, String> {
        let get = |key: &str| get(key).filter(|value| !value.is_empty());
        let home = || {
            crate::paths::home_dir_with(get)
                .ok_or_else(|| format!("cannot locate the home directory for {}", self.label()))
        };
        Ok(match self {
            Self::OpenCode => get("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .map_or_else(|| home().map(|h| h.join(".config")), Ok)?
                .join("opencode/plugins/aherald.ts"),
            Self::Pi => get("PI_CODING_AGENT_DIR")
                .map(PathBuf::from)
                .map_or_else(|| home().map(|h| h.join(".pi/agent")), Ok)?
                .join("extensions/aherald.ts"),
            Self::Goose => home()?.join(".agents/plugins/aherald/hooks/hooks.json"),
        })
    }

    /// The exact text installed for `exe`.
    pub fn text(self, exe: &Path) -> String {
        let exe = exe.display().to_string();
        match self {
            Self::OpenCode | Self::Pi => {
                let source = if self == Self::Pi { PI } else { OPENCODE };
                let quoted = serde_json::to_string(&exe).unwrap_or_default();
                source.replace(TS_DEFAULT, &format!("process.env.AHERALD_EXE || {quoted}"))
            }
            Self::Goose => {
                let mut value: Value = serde_json::from_str(GOOSE).unwrap_or_default();
                let quoted = format!("'{}'", exe.replace('\'', "'\\''"));
                stamp(&mut value, &quoted);
                serde_json::to_string_pretty(&value).unwrap_or_default() + "\n"
            }
        }
    }
}

fn stamp(value: &mut Value, quoted: &str) {
    match value {
        Value::String(text) => {
            if let Some(rest) = text.strip_prefix("aherald ") {
                *text = format!("{quoted} {rest}");
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| stamp(item, quoted)),
        Value::Object(map) => map.values_mut().for_each(|item| stamp(item, quoted)),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_typescript_bridge_names_the_program_in_exactly_the_stamped_place() {
        for (bridge, source) in [(Bridge::OpenCode, OPENCODE), (Bridge::Pi, PI)] {
            assert_eq!(source.matches(TS_DEFAULT).count(), 1, "{}", bridge.key());
            let text = bridge.text(Path::new(r"C:\Program Files\aHerald\aherald.exe"));
            assert!(
                text.contains(r#"|| "C:\\Program Files\\aHerald\\aherald.exe""#),
                "{text}"
            );
            assert!(!text.contains(TS_DEFAULT));
        }
    }

    #[test]
    fn the_goose_hooks_run_the_quoted_executable_and_stay_valid_json() {
        let text = Bridge::Goose.text(Path::new("/opt/it's/aherald"));
        let value: Value = serde_json::from_str(&text).unwrap();
        let command = value["hooks"]["SessionEnd"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert_eq!(
            command,
            r"'/opt/it'\''s/aherald' --notify goose --kind turn_complete --ended"
        );
    }

    #[test]
    fn files_follow_the_harness_overrides_and_need_a_home() {
        let env = |pairs: &'static [(&str, &str)]| {
            move |key: &str| {
                pairs
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map(|(_, v)| OsString::from(v))
            }
        };
        let home = env(&[("HOME", "/h"), ("USERPROFILE", "/h")]);
        assert_eq!(
            Bridge::OpenCode.file(home).unwrap(),
            Path::new("/h/.config/opencode/plugins/aherald.ts")
        );
        let xdg = env(&[
            ("HOME", "/h"),
            ("USERPROFILE", "/h"),
            ("XDG_CONFIG_HOME", "/x"),
        ]);
        assert_eq!(
            Bridge::OpenCode.file(xdg).unwrap(),
            Path::new("/x/opencode/plugins/aherald.ts")
        );
        let pi = env(&[("PI_CODING_AGENT_DIR", "/p")]);
        assert_eq!(
            Bridge::Pi.file(pi).unwrap(),
            Path::new("/p/extensions/aherald.ts")
        );
        assert!(Bridge::Goose.file(env(&[])).is_err());
    }
}
