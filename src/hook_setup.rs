//! Explicit, user-invoked setup of the aHerald hooks in an agent's own hook file.
//!
//! Nothing here runs unless the user asks (`--install-hooks` / `--uninstall-hooks`); the
//! default is a dry run and a write needs `--apply`. Only aHerald's own entries are touched,
//! the rest of the file keeps its content and key order.
use crate::agent::Agent;
use serde_json::{Map, Value, json};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Install,
    Uninstall,
}

/// What a plan would do to the file, one human-readable line per change.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub text: String,
    pub changes: Vec<String>,
}

/// The hook events aHerald listens to, per agent.
pub fn events(agent: Agent) -> &'static [&'static str] {
    match agent {
        Agent::Claude => &[
            "Notification",
            "PermissionRequest",
            "Stop",
            "StopFailure",
            "SessionEnd",
        ],
        Agent::Codex => &["Stop", "PermissionRequest", "Interrupt"],
    }
}

/// The hook command line for the running program. Unix agents run it through `sh`; on Windows
/// the agents' default shell is PowerShell, which also needs `Out-String` to wait for the
/// GUI-subsystem program.
pub fn command_for(exe: &Path, agent: Agent, windows: bool) -> String {
    let exe = exe.display().to_string();
    let agent_flag = match agent {
        Agent::Claude => " --agent claude",
        Agent::Codex => "",
    };
    if windows {
        let exe = exe.replace('\'', "''");
        format!(
            "powershell.exe -NoProfile -NonInteractive -Command \"& '{exe}' --hook{agent_flag} --resident | Out-String\""
        )
    } else {
        let exe = exe.replace('\'', "'\\''");
        format!("'{exe}' --hook{agent_flag} --resident")
    }
}

/// The agent's hook file under `home`.
pub fn default_file(agent: Agent, home: &Path) -> PathBuf {
    match agent {
        Agent::Claude => home.join(".claude/settings.json"),
        Agent::Codex => home.join(".codex/hooks.json"),
    }
}

/// Hook commands are the user's; only ones that are clearly aHerald's are ours.
fn is_ours(hook: &Value) -> bool {
    hook["command"].as_str().is_some_and(|command| {
        let command = command.to_ascii_lowercase();
        command.contains("aherald") && command.contains("--hook")
    })
}

fn malformed(what: &str) -> String {
    format!("the hook file has an unexpected shape ({what}); nothing was changed")
}

/// The groups of one event, each checked to hold a `hooks` array.
fn groups(event: &mut Value) -> Result<&mut Vec<Value>, String> {
    let groups = event
        .as_array_mut()
        .ok_or_else(|| malformed("an event is not a list"))?;
    for group in groups.iter() {
        if !group.get("hooks").is_some_and(Value::is_array) {
            return Err(malformed("a group has no hooks list"));
        }
    }
    Ok(groups)
}

pub fn plan(
    existing: Option<&str>,
    agent: Agent,
    command: &str,
    action: Action,
) -> Result<Plan, String> {
    let mut root: Value = match existing {
        Some(text) => serde_json::from_str(text)
            .map_err(|_| "the hook file is not valid JSON; nothing was changed")?,
        None => Value::Object(Map::new()),
    };
    let object = root
        .as_object_mut()
        .ok_or_else(|| malformed("not an object"))?;
    let mut changes = Vec::new();
    match action {
        Action::Install => {
            let hooks = object
                .entry("hooks")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .ok_or_else(|| malformed("`hooks` is not an object"))?;
            for event in events(agent) {
                let list = hooks
                    .entry(*event)
                    .or_insert_with(|| Value::Array(Vec::new()));
                let mut found = false;
                for group in groups(list)? {
                    for hook in group["hooks"].as_array_mut().into_iter().flatten() {
                        if !is_ours(hook) {
                            continue;
                        }
                        found = true;
                        if hook["command"] != command {
                            hook["command"] = command.into();
                            changes.push(format!("update {event}"));
                        }
                    }
                }
                if !found {
                    list.as_array_mut().into_iter().for_each(|l| {
                        l.push(json!({"hooks": [{"type": "command", "command": command, "timeout": 3}]}));
                    });
                    changes.push(format!("add {event}"));
                }
            }
        }
        Action::Uninstall => {
            if let Some(hooks) = object.get_mut("hooks") {
                let hooks = hooks
                    .as_object_mut()
                    .ok_or_else(|| malformed("`hooks` is not an object"))?;
                let names: Vec<String> = hooks.keys().cloned().collect();
                for name in names {
                    let list = groups(&mut hooks[name.as_str()])?;
                    let mut removed = false;
                    for group in list.iter_mut() {
                        if let Some(entries) = group["hooks"].as_array_mut() {
                            let n = entries.len();
                            entries.retain(|h| !is_ours(h));
                            removed |= entries.len() != n;
                        }
                    }
                    if removed {
                        list.retain(|g| !g["hooks"].as_array().is_some_and(Vec::is_empty));
                        changes.push(format!("remove {name}"));
                    }
                    if removed && list.is_empty() {
                        hooks.shift_remove(name.as_str());
                    }
                }
                if !changes.is_empty() && hooks.is_empty() {
                    object.shift_remove("hooks");
                }
            }
        }
    }
    let mut text =
        serde_json::to_string_pretty(&root).map_err(|_| "cannot encode the hook file")?;
    text.push('\n');
    Ok(Plan { text, changes })
}

/// `'<exe>' --statusline`, quoted for `sh`.
pub fn statusline_command(exe: &Path) -> String {
    format!(
        "'{}' --statusline",
        exe.display().to_string().replace('\'', "'\\''")
    )
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// The wrapped command of a statusLine command this program wrote, if it is one:
/// `Some(None)` for the bare wrapper, `Some(Some(original))` for `--then '<original>'`.
fn wrapped(command: &str) -> Option<Option<String>> {
    let lower = command.to_ascii_lowercase();
    if !lower.contains("aherald") || !command.contains(" --statusline") {
        return None;
    }
    let Some((_, rest)) = command.split_once(" --statusline --then ") else {
        return Some(None);
    };
    let inner = rest.strip_prefix('\'')?.strip_suffix('\'')?;
    Some(Some(inner.replace("'\\''", "'")))
}

/// Plans putting aHerald's `statusLine` wrapper into the settings text, keeping the user's own
/// status line command as `--then`, or taking it out again.
pub fn plan_statusline(
    existing: Option<&str>,
    exe_command: &str,
    action: Action,
) -> Result<Plan, String> {
    let mut root: Value = match existing {
        Some(text) => serde_json::from_str(text)
            .map_err(|_| "the settings file is not valid JSON; nothing was changed")?,
        None => Value::Object(Map::new()),
    };
    let object = root
        .as_object_mut()
        .ok_or_else(|| malformed("not an object"))?;
    let mut changes = Vec::new();
    let current = object.get("statusLine").cloned();
    let current_command = match &current {
        None => None,
        Some(value) if value["type"] == "command" => Some(
            value["command"]
                .as_str()
                .ok_or_else(|| malformed("statusLine has no command"))?
                .to_owned(),
        ),
        Some(_) => return Err(malformed("statusLine is not a command")),
    };
    match action {
        Action::Install => match current_command {
            Some(command) if wrapped(&command).is_some() => {
                let wanted = match wrapped(&command).flatten() {
                    Some(original) => format!("{exe_command} --then {}", quote(&original)),
                    None => exe_command.to_owned(),
                };
                if command != wanted {
                    return Err("a different aHerald statusLine is set; uninstall it first".into());
                }
            }
            Some(command) => {
                object["statusLine"]["command"] =
                    format!("{exe_command} --then {}", quote(&command)).into();
                changes.push("wrap your statusLine command".into());
            }
            None => {
                object.insert(
                    "statusLine".into(),
                    json!({"type": "command", "command": exe_command}),
                );
                changes.push("add statusLine".into());
            }
        },
        Action::Uninstall => {
            if let Some(command) = current_command
                && let Some(original) = wrapped(&command)
            {
                match original {
                    Some(original) => {
                        object["statusLine"]["command"] = original.into();
                        changes.push("restore your statusLine command".into());
                    }
                    None => {
                        object.shift_remove("statusLine");
                        changes.push("remove statusLine".into());
                    }
                }
            }
        }
    }
    let mut text =
        serde_json::to_string_pretty(&root).map_err(|_| "cannot encode the settings file")?;
    text.push('\n');
    Ok(Plan { text, changes })
}

/// Plans the change to `file` and, with `apply`, writes it. Returns the report to print.
/// A dry run (`apply == false`) never touches the disk.
pub fn run(
    agent: Agent,
    action: Action,
    file: &Path,
    command: &str,
    apply: bool,
) -> Result<String, String> {
    let reminder = (matches!(action, Action::Install) && agent == Agent::Codex)
        .then_some("Start a new Codex session, open /hooks and trust the aHerald hooks.");
    run_with(
        &format!("{} hook file", agent.label()),
        file,
        apply,
        |existing| plan(existing, agent, command, action),
        reminder,
    )
}

/// As [`run`] for Claude Code's `statusLine` in the settings file.
pub fn run_statusline(
    action: Action,
    file: &Path,
    exe_command: &str,
    apply: bool,
) -> Result<String, String> {
    run_with(
        "Claude settings file",
        file,
        apply,
        |existing| plan_statusline(existing, exe_command, action),
        None,
    )
}

fn run_with(
    label: &str,
    file: &Path,
    apply: bool,
    plan: impl FnOnce(Option<&str>) -> Result<Plan, String>,
    reminder: Option<&str>,
) -> Result<String, String> {
    let existing = match std::fs::read_to_string(file) {
        Ok(text) => Some(text),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(format!("cannot read {}: {e}", file.display())),
    };
    let plan = plan(existing.as_deref())?;
    let mut report = format!("{label}: {}\n", file.display());
    if plan.changes.is_empty() {
        report.push_str("Nothing to change.\n");
        return Ok(report);
    }
    for change in &plan.changes {
        report.push_str(&format!("  {change}\n"));
    }
    if !apply {
        report
            .push_str("Dry run: nothing was written. Run again with --apply to write the file.\n");
        return Ok(report);
    }
    write(file, existing.is_some(), &plan.text)?;
    report.push_str("Written.");
    if existing.is_some() {
        report.push_str(" The previous file was kept next to it as *.aherald-backup.");
    }
    report.push('\n');
    if let Some(reminder) = reminder {
        report.push_str(reminder);
        report.push('\n');
    }
    Ok(report)
}

/// Backs the file up, then replaces it atomically (a symlinked file stays a symlink).
fn write(file: &Path, exists: bool, text: &str) -> Result<(), String> {
    let fail = |e: std::io::Error| format!("cannot write {}: {e}", file.display());
    let target = if exists {
        std::fs::canonicalize(file).map_err(fail)?
    } else {
        if let Some(parent) = file.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(fail)?;
        }
        file.to_path_buf()
    };
    let name = target
        .file_name()
        .ok_or("the hook file has no name")?
        .to_os_string();
    let sibling = |suffix: String| {
        let mut n = name.clone();
        n.push(suffix);
        target.with_file_name(n)
    };
    if exists {
        std::fs::copy(&target, sibling(".aherald-backup".into())).map_err(fail)?;
    }
    let temporary = sibling(format!(".aherald-{}.tmp", std::process::id()));
    std::fs::write(&temporary, text).map_err(fail)?;
    if exists && let Ok(meta) = std::fs::metadata(&target) {
        let _ = std::fs::set_permissions(&temporary, meta.permissions());
    }
    std::fs::rename(&temporary, &target).map_err(|e| {
        let _ = std::fs::remove_file(&temporary);
        fail(e)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const CMD: &str =
        "'/Applications/aHerald.app/Contents/MacOS/aherald' --hook --agent claude --resident";

    fn parse(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    fn commands(value: &Value, event: &str) -> Vec<String> {
        value["hooks"][event]
            .as_array()
            .map(|groups| {
                groups
                    .iter()
                    .flat_map(|g| g["hooks"].as_array().unwrap())
                    .map(|h| h["command"].as_str().unwrap().to_owned())
                    .collect()
            })
            .unwrap_or_default()
    }

    #[test]
    fn install_into_a_missing_file_adds_every_event_with_a_short_timeout() {
        let plan = plan(None, Agent::Claude, CMD, Action::Install).unwrap();
        let value = parse(&plan.text);
        for event in events(Agent::Claude) {
            assert_eq!(commands(&value, event), [CMD], "{event}");
            assert_eq!(value["hooks"][event][0]["hooks"][0]["timeout"], 3);
            assert_eq!(value["hooks"][event][0]["hooks"][0]["type"], "command");
        }
        assert_eq!(plan.changes.len(), events(Agent::Claude).len());
    }

    #[test]
    fn install_keeps_foreign_settings_hooks_and_key_order() {
        let before = r#"{"zeta":1,"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done"}]}],"PreToolUse":[]},"alpha":{"b":2,"a":1}}"#;
        let plan = plan(Some(before), Agent::Claude, CMD, Action::Install).unwrap();
        let value = parse(&plan.text);
        assert_eq!(commands(&value, "Stop"), ["say done", CMD]);
        assert_eq!(value["hooks"]["PreToolUse"], json!([]));
        let keys: Vec<_> = value.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["zeta", "hooks", "alpha"]);
        let inner: Vec<_> = value["alpha"].as_object().unwrap().keys().collect();
        assert_eq!(inner, ["b", "a"]);
    }

    #[test]
    fn installing_twice_changes_nothing() {
        let once = plan(None, Agent::Claude, CMD, Action::Install).unwrap();
        let twice = plan(Some(&once.text), Agent::Claude, CMD, Action::Install).unwrap();
        assert!(twice.changes.is_empty());
        assert_eq!(parse(&twice.text), parse(&once.text));
    }

    #[test]
    fn an_older_aherald_command_is_replaced_not_duplicated() {
        let old =
            "powershell.exe -NoProfile -Command \"& 'C:\\\\old\\\\aherald.exe' --hook --resident\"";
        let before = json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": old, "timeout": 3}]}]}});
        let plan = plan(
            Some(&before.to_string()),
            Agent::Claude,
            CMD,
            Action::Install,
        )
        .unwrap();
        assert_eq!(commands(&parse(&plan.text), "Stop"), [CMD]);
        assert!(plan.changes.iter().any(|c| c.contains("Stop")));
    }

    #[test]
    fn uninstall_removes_only_aherald_entries_and_empty_containers() {
        let installed = plan(
            Some(r#"{"hooks":{"Stop":[{"hooks":[{"type":"command","command":"say done"}]}]},"theme":"dark"}"#),
            Agent::Claude,
            CMD,
            Action::Install,
        )
        .unwrap();
        let removed = plan(Some(&installed.text), Agent::Claude, CMD, Action::Uninstall).unwrap();
        let value = parse(&removed.text);
        assert_eq!(commands(&value, "Stop"), ["say done"]);
        assert!(value["hooks"].get("Notification").is_none());
        assert_eq!(value["theme"], "dark");
        let alone = plan(None, Agent::Claude, CMD, Action::Install).unwrap();
        let empty = plan(Some(&alone.text), Agent::Claude, CMD, Action::Uninstall).unwrap();
        assert!(parse(&empty.text).get("hooks").is_none());
    }

    #[test]
    fn uninstall_without_hooks_is_a_no_op() {
        let plan = plan(
            Some(r#"{"theme":"dark"}"#),
            Agent::Claude,
            CMD,
            Action::Uninstall,
        )
        .unwrap();
        assert!(plan.changes.is_empty());
        assert_eq!(parse(&plan.text), json!({"theme": "dark"}));
    }

    #[test]
    fn invalid_or_unexpected_files_are_refused_untouched() {
        for bad in [
            "{not json",
            "[]",
            r#"{"hooks":[]}"#,
            r#"{"hooks":{"Stop":{}}}"#,
        ] {
            assert!(
                plan(Some(bad), Agent::Claude, CMD, Action::Install).is_err(),
                "{bad}"
            );
        }
    }

    #[test]
    fn the_command_quotes_the_path_and_selects_the_agent() {
        let exe = Path::new("/Applications/aHerald.app/Contents/MacOS/aherald");
        assert_eq!(command_for(exe, Agent::Claude, false), CMD);
        assert_eq!(
            command_for(exe, Agent::Codex, false),
            "'/Applications/aHerald.app/Contents/MacOS/aherald' --hook --resident"
        );
        let spaced = Path::new("/Users/me/My Apps/it's/aherald");
        assert_eq!(
            command_for(spaced, Agent::Codex, false),
            "'/Users/me/My Apps/it'\\''s/aherald' --hook --resident"
        );
        assert_eq!(
            command_for(Path::new("C:\\Tools\\aherald.exe"), Agent::Claude, true),
            "powershell.exe -NoProfile -NonInteractive -Command \"& 'C:\\Tools\\aherald.exe' --hook --agent claude --resident | Out-String\""
        );
    }

    #[test]
    fn each_agent_has_its_own_default_file() {
        let home = Path::new("/Users/me");
        assert_eq!(
            default_file(Agent::Claude, home),
            Path::new("/Users/me/.claude/settings.json")
        );
        assert_eq!(
            default_file(Agent::Codex, home),
            Path::new("/Users/me/.codex/hooks.json")
        );
    }

    const EXE: &str = "'/Applications/aHerald.app/Contents/MacOS/aherald' --statusline";

    #[test]
    fn a_missing_status_line_gets_the_bare_wrapper_and_other_settings_stay() {
        let plan =
            plan_statusline(Some(r#"{"zeta":1,"theme":"dark"}"#), EXE, Action::Install).unwrap();
        let value = parse(&plan.text);
        assert_eq!(
            value["statusLine"],
            json!({"type": "command", "command": EXE})
        );
        let keys: Vec<_> = value.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["zeta", "theme", "statusLine"]);
        assert_eq!(plan.changes.len(), 1);
    }

    #[test]
    fn an_existing_status_line_is_wrapped_keeping_its_other_fields_and_restored_exactly() {
        let own = "~/bin/bar.sh --who 'me' | tr a b";
        let before = json!({"statusLine": {"type": "command", "command": own, "padding": 2, "refreshInterval": 5}}).to_string();
        let wrapped = plan_statusline(Some(&before), EXE, Action::Install).unwrap();
        let value = parse(&wrapped.text);
        let command = value["statusLine"]["command"].as_str().unwrap();
        assert!(command.starts_with(&format!("{EXE} --then '")), "{command}");
        assert_eq!(
            (
                &value["statusLine"]["padding"],
                &value["statusLine"]["refreshInterval"]
            ),
            (&json!(2), &json!(5))
        );
        // Installing again is a no-op; uninstalling gives the original command back.
        let again = plan_statusline(Some(&wrapped.text), EXE, Action::Install).unwrap();
        assert!(again.changes.is_empty());
        let restored = plan_statusline(Some(&wrapped.text), EXE, Action::Uninstall).unwrap();
        assert_eq!(parse(&restored.text), parse(&before));
    }

    #[test]
    fn uninstalling_the_bare_wrapper_removes_the_status_line_and_foreign_ones_are_left_alone() {
        let installed = plan_statusline(None, EXE, Action::Install).unwrap();
        let removed = plan_statusline(Some(&installed.text), EXE, Action::Uninstall).unwrap();
        assert!(parse(&removed.text).get("statusLine").is_none());
        let foreign = r#"{"statusLine":{"type":"command","command":"my-bar"}}"#;
        let untouched = plan_statusline(Some(foreign), EXE, Action::Uninstall).unwrap();
        assert!(untouched.changes.is_empty());
        assert_eq!(parse(&untouched.text), parse(foreign));
    }

    #[test]
    fn unsupported_or_conflicting_status_lines_are_refused() {
        for bad in [
            r#"{"statusLine":"my-bar"}"#,
            r#"{"statusLine":{"type":"text"}}"#,
            r#"{"statusLine":{"type":"command"}}"#,
            "{broken",
            "[]",
        ] {
            assert!(
                plan_statusline(Some(bad), EXE, Action::Install).is_err(),
                "{bad}"
            );
        }
        let other = r#"{"statusLine":{"type":"command","command":"'/old/aherald' --statusline"}}"#;
        assert!(plan_statusline(Some(other), EXE, Action::Install).is_err());
    }

    #[test]
    fn the_wrapper_command_quotes_the_path_for_the_shell() {
        assert_eq!(
            statusline_command(Path::new(
                "/Applications/aHerald.app/Contents/MacOS/aherald"
            )),
            EXE
        );
        assert_eq!(
            statusline_command(Path::new("/Users/me/it's/aherald")),
            "'/Users/me/it'\\''s/aherald' --statusline"
        );
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aherald-hooks-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_dry_run_reports_the_changes_and_writes_nothing() {
        let dir = scratch("dry");
        let file = dir.join("nested/settings.json");
        let report = run(Agent::Claude, Action::Install, &file, CMD, false).unwrap();
        assert!(
            report.contains("add Stop") && report.contains("--apply"),
            "{report}"
        );
        assert!(!dir.join("nested").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn apply_backs_the_old_file_up_keeps_its_mode_and_follows_a_symlink() {
        use std::os::unix::fs::{MetadataExt, PermissionsExt, symlink};
        let dir = scratch("apply");
        let real = dir.join("dotfiles-settings.json");
        std::fs::write(&real, r#"{"theme":"dark"}"#).unwrap();
        std::fs::set_permissions(&real, std::fs::Permissions::from_mode(0o600)).unwrap();
        let link = dir.join("settings.json");
        symlink(&real, &link).unwrap();
        let report = run(Agent::Claude, Action::Install, &link, CMD, true).unwrap();
        assert!(report.contains("Written"), "{report}");
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let written = parse(&std::fs::read_to_string(&real).unwrap());
        assert_eq!(written["theme"], "dark");
        assert_eq!(commands(&written, "Stop"), [CMD]);
        assert_eq!(std::fs::metadata(&real).unwrap().mode() & 0o777, 0o600);
        let backup =
            std::fs::read_to_string(dir.join("dotfiles-settings.json.aherald-backup")).unwrap();
        assert_eq!(backup, r#"{"theme":"dark"}"#);
        // Removing restores a file without our hooks.
        run(Agent::Claude, Action::Uninstall, &link, CMD, true).unwrap();
        assert_eq!(
            parse(&std::fs::read_to_string(&real).unwrap()),
            json!({"theme": "dark"})
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_file_that_cannot_be_planned_is_left_untouched() {
        let dir = scratch("bad");
        let file = dir.join("settings.json");
        std::fs::write(&file, "{broken").unwrap();
        assert!(run(Agent::Claude, Action::Install, &file, CMD, true).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "{broken");
        assert!(!dir.join("settings.json.aherald-backup").exists());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
