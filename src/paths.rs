//! Per-OS file-name and directory conventions, so callers never hard-code them.
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Environment variable that holds the user's home directory.
pub const HOME_VAR: &str = if cfg!(windows) { "USERPROFILE" } else { "HOME" };

/// Per-user locations for the system Linux package. Portable installations keep
/// their existing layout beside the executable.
#[derive(Debug, PartialEq, Eq)]
pub struct LinuxUserDirs {
    pub config: PathBuf,
    pub data: PathBuf,
    pub state: PathBuf,
}

/// The package executable has a fixed path; copied portable binaries retain
/// portable semantics regardless of the machine on which they run.
pub fn is_linux_system_executable(executable: &Path) -> bool {
    cfg!(target_os = "linux") && executable == Path::new("/usr/lib/aherald/aherald")
}

pub fn config_dir_with(
    executable: &Path,
    get: impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, String> {
    if is_linux_system_executable(executable) {
        return xdg_dir_with(&get, "XDG_CONFIG_HOME", ".config");
    }
    data_dir_with(executable, get)
}

pub fn receipt_dir_with(
    executable: &Path,
    get: impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, String> {
    if is_linux_system_executable(executable) {
        return xdg_dir_with(&get, "XDG_DATA_HOME", ".local/share");
    }
    data_dir_with(executable, get)
}

/// Keep explicitly chosen configuration paths and portable logs together.
pub fn diagnostic_dir_with(
    executable: &Path,
    config: &Path,
    get: impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, String> {
    if is_linux_system_executable(executable)
        && let Ok(config_dir) = xdg_dir_with(&get, "XDG_CONFIG_HOME", ".config")
        && config == config_dir.join("config.toml")
    {
        return xdg_dir_with(&get, "XDG_STATE_HOME", ".local/state");
    }
    config
        .parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| "configuration has no directory".into())
}

pub fn linux_user_dirs_with(
    get: impl Fn(&str) -> Option<OsString>,
) -> Result<LinuxUserDirs, String> {
    Ok(LinuxUserDirs {
        config: xdg_dir_with(&get, "XDG_CONFIG_HOME", ".config")?,
        data: xdg_dir_with(&get, "XDG_DATA_HOME", ".local/share")?,
        state: xdg_dir_with(&get, "XDG_STATE_HOME", ".local/state")?,
    })
}

fn xdg_dir_with(
    get: &impl Fn(&str) -> Option<OsString>,
    key: &str,
    fallback: &str,
) -> Result<PathBuf, String> {
    if let Some(path) = non_empty(get(key))
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
    {
        return Ok(path.join("aherald"));
    }
    home_dir_with(get)
        .filter(|p| p.is_absolute())
        .map(|home| home.join(fallback).join("aherald"))
        .ok_or_else(|| "cannot locate an absolute home directory".into())
}

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

/// The file that holds an agent's hooks (and, for Claude Code, its statusLine): Claude's
/// `settings.json` in `CLAUDE_CONFIG_DIR` or `<home>/.claude`; Codex's `hooks.json` in `CODEX_HOME`
/// or `<home>/.codex`. The one lookup that `--setup`, `--install-hooks` and the statusLine commands
/// share, so they never write to different files.
pub fn agent_hook_file_with(
    agent: crate::agent::Agent,
    get: impl Fn(&str) -> Option<OsString>,
) -> Option<PathBuf> {
    match agent {
        crate::agent::Agent::Claude => {
            claude_config_dir_with(get).map(|dir| dir.join("settings.json"))
        }
        crate::agent::Agent::Codex => non_empty(get("CODEX_HOME"))
            .map(PathBuf::from)
            .or_else(|| home_dir_with(&get).map(|home| home.join(".codex")))
            .map(|dir| dir.join("hooks.json")),
    }
}

pub fn agent_hook_file(agent: crate::agent::Agent) -> Option<PathBuf> {
    agent_hook_file_with(agent, |name| std::env::var_os(name))
}

/// The `Contents/Resources` directory when `executable` is the main program of a macOS
/// application bundle (`X.app/Contents/MacOS/<name>`); `None` for a bare executable.
pub fn bundle_resources(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let bundle = contents.parent()?;
    let is_bundle = macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && bundle.extension()? == "app";
    is_bundle.then(|| contents.join("Resources"))
}

/// Where aHerald keeps its configuration and log: beside the executable, except inside an
/// application bundle, whose signed contents must stay unmodified and may sit in a read-only
/// folder. There it is `<home>/Library/Application Support/aHerald`.
pub fn data_dir_with(
    executable: &Path,
    get: impl Fn(&str) -> Option<OsString>,
) -> Result<PathBuf, String> {
    if bundle_resources(executable).is_none() {
        return executable
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| "executable has no parent directory; use --config".into());
    }
    home_dir_with(get)
        .map(|home| home.join("Library/Application Support/aHerald"))
        .ok_or_else(|| "cannot locate the home directory; use --config".into())
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
    fn an_agents_hook_file_follows_its_own_directory_variable_then_home() {
        use crate::agent::Agent;
        let home = env(&[(HOME_VAR, "/Users/me")]);
        assert_eq!(
            agent_hook_file_with(Agent::Claude, &home),
            Some(PathBuf::from("/Users/me/.claude/settings.json"))
        );
        assert_eq!(
            agent_hook_file_with(Agent::Codex, &home),
            Some(PathBuf::from("/Users/me/.codex/hooks.json"))
        );
        let both = env(&[
            (HOME_VAR, "/Users/me"),
            ("CLAUDE_CONFIG_DIR", "/cfg/claude"),
            ("CODEX_HOME", "/cfg/codex"),
        ]);
        assert_eq!(
            agent_hook_file_with(Agent::Claude, &both),
            Some(PathBuf::from("/cfg/claude/settings.json"))
        );
        assert_eq!(
            agent_hook_file_with(Agent::Codex, &both),
            Some(PathBuf::from("/cfg/codex/hooks.json"))
        );
        // An empty variable is unset, and without a home there is no answer.
        let empty = env(&[
            (HOME_VAR, "/Users/me"),
            ("CODEX_HOME", ""),
            ("CLAUDE_CONFIG_DIR", ""),
        ]);
        assert_eq!(
            agent_hook_file_with(Agent::Codex, &empty),
            Some(PathBuf::from("/Users/me/.codex/hooks.json"))
        );
        assert_eq!(
            agent_hook_file_with(Agent::Claude, &empty),
            Some(PathBuf::from("/Users/me/.claude/settings.json"))
        );
        assert_eq!(agent_hook_file_with(Agent::Codex, env(&[])), None);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_user_directories_follow_xdg_and_ignore_relative_overrides() {
        let dirs = linux_user_dirs_with(env(&[
            (HOME_VAR, "/home/alice"),
            ("XDG_CONFIG_HOME", "/custom/config"),
            ("XDG_DATA_HOME", "relative"),
            ("XDG_STATE_HOME", ""),
        ]))
        .unwrap();
        assert_eq!(dirs.config, PathBuf::from("/custom/config/aherald"));
        assert_eq!(dirs.data, PathBuf::from("/home/alice/.local/share/aherald"));
        assert_eq!(
            dirs.state,
            PathBuf::from("/home/alice/.local/state/aherald")
        );
        assert!(linux_user_dirs_with(env(&[(HOME_VAR, "relative")])).is_err());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn package_paths_do_not_change_portable_paths() {
        let vars = env(&[(HOME_VAR, "/home/alice")]);
        let package = Path::new("/usr/lib/aherald/aherald");
        let portable = Path::new("/tmp/portable/aherald");
        assert_eq!(
            config_dir_with(portable, &vars).unwrap(),
            PathBuf::from("/tmp/portable")
        );
        if cfg!(target_os = "linux") {
            assert_eq!(
                config_dir_with(package, &vars).unwrap(),
                PathBuf::from("/home/alice/.config/aherald")
            );
            assert_eq!(
                receipt_dir_with(package, &vars).unwrap(),
                PathBuf::from("/home/alice/.local/share/aherald")
            );
            assert_eq!(
                diagnostic_dir_with(
                    package,
                    Path::new("/home/alice/.config/aherald/config.toml"),
                    &vars
                )
                .unwrap(),
                PathBuf::from("/home/alice/.local/state/aherald")
            );
            assert_eq!(
                diagnostic_dir_with(package, Path::new("/custom/config.toml"), &vars).unwrap(),
                PathBuf::from("/custom")
            );
        }
    }

    #[test]
    fn a_bundle_executable_has_its_resources_next_to_macos() {
        let exe = Path::new("/Applications/aHerald.app/Contents/MacOS/aherald");
        assert_eq!(
            bundle_resources(exe),
            Some(PathBuf::from(
                "/Applications/aHerald.app/Contents/Resources"
            ))
        );
        for bare in [
            "/usr/local/bin/aherald",
            "/tmp/MacOS/aherald",
            "/x/Contents/MacOS/aherald",
            "/x/aHerald.app/Contents/Other/aherald",
            "aherald",
        ] {
            assert_eq!(bundle_resources(Path::new(bare)), None, "{bare}");
        }
    }

    #[test]
    fn data_lives_beside_a_bare_executable_and_in_application_support_for_a_bundle() {
        let home = env(&[(HOME_VAR, "/Users/me")]);
        assert_eq!(
            data_dir_with(Path::new("/opt/aherald/aherald"), &home),
            Ok(PathBuf::from("/opt/aherald"))
        );
        assert_eq!(
            data_dir_with(
                Path::new("/Applications/aHerald.app/Contents/MacOS/aherald"),
                &home
            ),
            Ok(PathBuf::from(
                "/Users/me/Library/Application Support/aHerald"
            ))
        );
        assert!(
            data_dir_with(Path::new("/A/aHerald.app/Contents/MacOS/aherald"), env(&[])).is_err()
        );
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
