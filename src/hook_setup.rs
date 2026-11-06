//! Hook-file editing for setup: event lists, hook commands and the planner that adds or removes
//! aHerald's own entries; the rest of the file keeps its content and key order.
use crate::agent::Agent;
use serde_json::{Map, Value, json};
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Install,
    Uninstall,
}

/// What a plan would do to the file.
#[derive(Debug, PartialEq, Eq)]
pub struct Plan {
    pub text: String,
    pub changes: Vec<String>,
}

/// The hook events aHerald listens to, per agent.
pub fn events(agent: Agent) -> &'static [&'static str] {
    match agent {
        Agent::Claude => &[
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "Notification",
            "PermissionRequest",
            "Stop",
            "StopFailure",
            "SessionEnd",
        ],
        Agent::Codex => &[
            "PostModelRequest",
            "Stop",
            "PermissionRequest",
            "Interrupt",
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "SessionEnd",
        ],
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
        let exe = crate::quote::powershell(&exe);
        format!(
            "powershell.exe -NoProfile -NonInteractive -Command \"& {exe} --hook{agent_flag} --resident | Out-String\""
        )
    } else {
        format!(
            "{} --hook{agent_flag} --resident",
            crate::quote::posix(&exe)
        )
    }
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

/// Native setup uses a complete execution definition, including exec-form arguments.
/// Removal matches the saved binding exactly; another installation's hooks survive.
pub(crate) fn plan_handler(
    existing: Option<&str>,
    agent: Agent,
    handler: &Value,
    action: Action,
    owns: impl Fn(&Value) -> bool,
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
                        if !owns(hook) {
                            continue;
                        }
                        found = true;
                        if ["command", "args", "commandWindows"]
                            .iter()
                            .any(|key| hook.get(*key) != handler.get(*key))
                        {
                            let object = hook
                                .as_object_mut()
                                .ok_or_else(|| malformed("a handler is not an object"))?;
                            for key in ["command", "args", "commandWindows"] {
                                if let Some(value) = handler.get(key) {
                                    object.insert(key.into(), value.clone());
                                } else {
                                    object.shift_remove(key);
                                }
                            }
                            changes.push(format!("update {event}"));
                        }
                    }
                }
                if !found {
                    list.as_array_mut().into_iter().for_each(|l| {
                        l.push(json!({"hooks": [handler]}));
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
                            entries.retain(|h| !owns(h));
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
        "{} --statusline",
        crate::quote::posix(&exe.display().to_string())
    )
}

/// The wrapped command of a statusLine command this program wrote, if it is one:
/// `Some(None)` for the bare wrapper, `Some(Some(original))` for `--then '<original>'`.
#[cfg(not(windows))]
pub(crate) fn wrapped(command: &str) -> Option<Option<String>> {
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

/// Backs the file up, then replaces it atomically (a symlinked file stays a symlink).
pub(crate) fn write(file: &Path, exists: bool, text: &str) -> Result<(), String> {
    write_inner(file, exists, text, true)
}

pub(crate) fn write_inner(
    file: &Path,
    exists: bool,
    text: &str,
    backup: bool,
) -> Result<(), String> {
    // `exists` is what the caller read; a file that vanished since is simply created.
    crate::atomic_file::write(file, text.as_bytes(), backup && exists)
        .map_err(|e| format!("cannot write {}: {e}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    const CMD: &str = "'/Apps/aherald' --hook --agent claude --resident";

    fn handler() -> Value {
        json!({"type":"command", "command": CMD, "timeout": 3})
    }
    fn plan(existing: Option<&str>, action: Action) -> Result<Plan, String> {
        plan_handler(existing, Agent::Claude, &handler(), action, |h| {
            h["command"] == CMD
        })
    }

    #[test]
    fn install_adds_every_event_keeps_foreign_content_and_is_idempotent() {
        let first = plan(
            Some(r#"{"zeta":1,"hooks":{"Stop":[{"hooks":[{"command":"mine"}]}]}}"#),
            Action::Install,
        )
        .unwrap();
        assert_eq!(first.changes.len(), events(Agent::Claude).len());
        let value: Value = serde_json::from_str(&first.text).unwrap();
        assert_eq!(value["hooks"]["Stop"][0]["hooks"][0]["command"], "mine");
        assert_eq!(value["hooks"]["Stop"][1]["hooks"][0], handler());
        assert!(first.text.find("zeta").unwrap() < first.text.find("hooks").unwrap());
        assert!(
            plan(Some(&first.text), Action::Install)
                .unwrap()
                .changes
                .is_empty()
        );
    }

    #[test]
    fn uninstall_removes_only_owned_entries_and_empty_containers() {
        let installed = plan(
            Some(r#"{"hooks":{"Stop":[{"hooks":[{"command":"mine"}]}]}}"#),
            Action::Install,
        )
        .unwrap();
        let removed = plan(Some(&installed.text), Action::Uninstall).unwrap();
        let value: Value = serde_json::from_str(&removed.text).unwrap();
        assert_eq!(
            value,
            json!({"hooks":{"Stop":[{"hooks":[{"command":"mine"}]}]}})
        );
        let all = plan(None, Action::Install).unwrap();
        let none = plan(Some(&all.text), Action::Uninstall).unwrap();
        assert_eq!(
            serde_json::from_str::<Value>(&none.text).unwrap(),
            json!({})
        );
    }

    #[test]
    fn unexpected_files_are_refused() {
        for text in [
            "{",
            "[]",
            r#"{"hooks":[]}"#,
            r#"{"hooks":{"Stop":{}}}"#,
            r#"{"hooks":{"Stop":[{}]}}"#,
        ] {
            assert!(plan(Some(text), Action::Install).is_err(), "{text}");
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn a_wrapped_status_line_command_yields_the_original() {
        let base = statusline_command(Path::new("/Apps/aHerald/aherald"));
        assert_eq!(wrapped(&base), Some(None));
        assert_eq!(
            wrapped(&format!("{base} --then 'it'\\''s.sh'")),
            Some(Some("it's.sh".into()))
        );
    }
}
