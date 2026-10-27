//! Per-OS file-name and directory conventions, so callers never hard-code them.
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Environment variable that holds the user's home directory.
pub const HOME_VAR: &str = if cfg!(windows) { "USERPROFILE" } else { "HOME" };

/// File name of an executable: `codex` is `codex.exe` on Windows and `codex` elsewhere.
pub fn executable_name(stem: &str) -> String {
    if cfg!(windows) {
        format!("{stem}.exe")
    } else {
        stem.into()
    }
}

/// The user's home directory from `get` (an environment lookup); empty values are unset.
pub fn home_dir_with(get: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    non_empty(get(HOME_VAR)).map(PathBuf::from)
}

/// Claude Code's configuration directory: `CLAUDE_CONFIG_DIR`, else `<home>/.claude`.
pub fn claude_config_dir_with(get: impl Fn(&str) -> Option<OsString>) -> Option<PathBuf> {
    non_empty(get("CLAUDE_CONFIG_DIR"))
        .map(PathBuf::from)
        .or_else(|| home_dir_with(&get).map(|home| home.join(".claude")))
}

pub fn claude_config_dir() -> Option<PathBuf> {
    claude_config_dir_with(|name| std::env::var_os(name))
}

fn non_empty(value: Option<OsString>) -> Option<OsString> {
    value.filter(|v| !Path::new(v).as_os_str().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<OsString> + 'a {
        move |name| {
            pairs
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, value)| OsString::from(value))
        }
    }

    #[test]
    fn executable_name_adds_the_extension_only_on_windows() {
        let expected = if cfg!(windows) { "codex.exe" } else { "codex" };
        assert_eq!(executable_name("codex"), expected);
    }

    #[test]
    fn home_comes_from_the_platform_variable_and_ignores_empty_values() {
        assert_eq!(
            home_dir_with(env(&[(HOME_VAR, "/users/a")])),
            Some(PathBuf::from("/users/a"))
        );
        assert_eq!(home_dir_with(env(&[(HOME_VAR, "")])), None);
        assert_eq!(home_dir_with(env(&[("OTHER", "/x")])), None);
    }

    #[test]
    fn claude_config_dir_prefers_the_override_then_home() {
        assert_eq!(
            claude_config_dir_with(env(&[("CLAUDE_CONFIG_DIR", "/cfg"), (HOME_VAR, "/h")])),
            Some(PathBuf::from("/cfg"))
        );
        assert_eq!(
            claude_config_dir_with(env(&[("CLAUDE_CONFIG_DIR", ""), (HOME_VAR, "/h")])),
            Some(Path::new("/h").join(".claude"))
        );
        assert_eq!(claude_config_dir_with(env(&[])), None);
    }
}
