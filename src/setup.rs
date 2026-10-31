//! User-context installation of fixed hook bindings. No lookup runs on the hook path.
use crate::{
    agent::Agent,
    hook_setup::{self, Action, Plan},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Binding {
    agent: String,
    file: PathBuf,
    handler: Value,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    version: u32,
    executable: PathBuf,
    bindings: Vec<Binding>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct UsageOptions {
    pub file: Option<PathBuf>,
    pub codex: Option<bool>,
    pub claude: Option<bool>,
}

fn settings_file(exe: &Path, selected: Option<&Path>) -> Result<PathBuf, String> {
    let file = match selected {
        Some(file) => file.to_path_buf(),
        None => crate::paths::data_dir_with(exe, |key| std::env::var_os(key))?.join("config.toml"),
    };
    if !file.is_absolute() {
        return Err("settings file must be an absolute path".into());
    }
    Ok(file)
}

fn settings_text(file: &Path) -> Result<(Option<String>, crate::config::Config), String> {
    let before = read(file)?;
    let text = before
        .as_deref()
        .unwrap_or(include_str!("../aHerald.example.toml"));
    if text.len() > 65_536 {
        return Err("configuration exceeds 64 KiB".into());
    }
    let config =
        crate::config::Config::parse(text, file.parent().ok_or("configuration has no directory")?)?;
    Ok((before, config))
}

/// Read effective values without creating a default or starting runtime services.
pub fn settings_state(exe: &Path, selected: Option<&Path>) -> Result<String, String> {
    let (_, config) = settings_text(&settings_file(exe, selected)?)?;
    Ok(format!(
        "codex_usage={}\nclaude_usage={}\n",
        config.codex.usage.enabled, config.claude.usage.enabled
    ))
}

fn usage_plan(exe: &Path, options: &UsageOptions) -> Result<Option<Change>, String> {
    if options.codex.is_none() && options.claude.is_none() {
        return Ok(None);
    }
    let file = settings_file(exe, options.file.as_deref())?;
    let (before, config) = settings_text(&file)?;
    let text = before
        .as_deref()
        .unwrap_or(include_str!("../aHerald.example.toml"));
    let mut document = text
        .parse::<toml_edit::DocumentMut>()
        .map_err(|_| "cannot edit configuration TOML")?;
    for (agent, selected, current) in [
        ("codex", options.codex, config.codex.usage.enabled),
        ("claude", options.claude, config.claude.usage.enabled),
    ] {
        if let Some(selected) = selected
            && selected != current
        {
            let item = &mut document[agent]["usage"]["enabled"];
            // Replacing only the scalar must retain the user's surrounding comment.
            let decor = item.as_value().map(|value| value.decor().clone());
            let mut value = toml_edit::Value::from(selected);
            if let Some(decor) = decor {
                *value.decor_mut() = decor;
            }
            *item = toml_edit::Item::Value(value);
        }
    }
    let after = if before.is_some()
        && options
            .codex
            .is_none_or(|value| value == config.codex.usage.enabled)
        && options
            .claude
            .is_none_or(|value| value == config.claude.usage.enabled)
    {
        text.to_owned()
    } else {
        document.to_string()
    };
    if before.as_deref() == Some(&after) {
        Ok(None)
    } else {
        Ok(Some(Change {
            file,
            before,
            after,
        }))
    }
}

struct Change {
    file: PathBuf,
    before: Option<String>,
    after: String,
}

/// Claude Code's exec form bypasses shells. Codex currently accepts a shell command
/// only; use its existing Windows PowerShell runtime directly.
fn handler(exe: &Path, agent: Agent) -> Value {
    match agent {
        Agent::Claude => json!({"type":"command", "command":exe,
            "args":["--hook", "--agent", "claude", "--resident"], "timeout":3}),
        Agent::Codex => codex_handler(exe),
    }
}

fn codex_handler(exe: &Path) -> Value {
    let value = json!({"type":"command", "command":hook_setup::command_for(exe, Agent::Codex, false), "timeout":3});
    #[cfg(windows)]
    {
        let mut value = value;
        // Codex selects PowerShell on Windows (pwsh, then inbox PowerShell).
        // Call within that shell; do not start a second PowerShell process.
        let exe = exe.display().to_string().replace('\'', "''");
        value["commandWindows"] = format!("& '{exe}' --hook --resident | Out-String").into();
        value
    }
    #[cfg(not(windows))]
    {
        value
    }
}

// Recognize only the native and legacy definitions aHerald itself generates.
// A foreign command mentioning "aherald --hook" is not an installation binding.
fn generated_handler(hook: &Value, agent: Agent) -> bool {
    if hook["type"] != "command" {
        return false;
    }
    let Some(command) = hook["command"].as_str() else {
        return false;
    };
    let executable_name = |path: &str| {
        path.rsplit(['/', '\\']).next().is_some_and(|name| {
            name.eq_ignore_ascii_case("aherald") || name.eq_ignore_ascii_case("aherald.exe")
        })
    };
    if let Some(args) = hook.get("args") {
        let expected = match agent {
            Agent::Claude => json!(["--hook", "--agent", "claude", "--resident"]),
            Agent::Codex => json!(["--hook", "--resident"]),
        };
        return executable_name(command) && args == &expected;
    }
    let flag = if agent == Agent::Claude {
        " --agent claude"
    } else {
        ""
    };
    let windows_suffix = format!("' --hook{flag} --resident | Out-String\"");
    let unix_suffix = format!("' --hook{flag} --resident");
    command
        .strip_prefix("powershell.exe -NoProfile -NonInteractive -Command \"& '")
        .and_then(|s| s.strip_suffix(&windows_suffix))
        .or_else(|| {
            command
                .strip_prefix('\'')
                .and_then(|s| s.strip_suffix(&unix_suffix))
        })
        .is_some_and(executable_name)
}

fn same_execution(a: &Value, b: &Value) -> bool {
    ["type", "command", "args", "commandWindows"]
        .iter()
        .all(|key| a.get(*key) == b.get(*key))
}

fn file_for(agent: Agent) -> Result<PathBuf, String> {
    let get = |key: &str| std::env::var_os(key).filter(|v| !v.is_empty());
    match agent {
        Agent::Claude => crate::paths::claude_config_dir_with(get).map(|p| p.join("settings.json")),
        Agent::Codex => get("CODEX_HOME")
            .map(PathBuf::from)
            .or_else(|| crate::paths::home_dir_with(get).map(|p| p.join(".codex")))
            .map(|p| p.join("hooks.json")),
    }
    .ok_or_else(|| format!("cannot locate {} configuration directory", agent.label()))
}

fn read(file: &Path) -> Result<Option<String>, String> {
    match std::fs::read_to_string(file) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {}: {e}", file.display())),
    }
}

fn add_plan(
    binding: &Binding,
    action: Action,
    changes: &mut Vec<Change>,
    report: &mut String,
) -> Result<(), String> {
    let agent = Agent::from_name(&binding.agent).ok_or("unsupported harness in setup receipt")?;
    if !binding.file.is_absolute() {
        return Err("setup receipt contains a relative hook path".into());
    }
    let before = read(&binding.file)?;
    let Plan {
        text,
        changes: descriptions,
    } = hook_setup::plan_handler(before.as_deref(), agent, &binding.handler, action, |h| {
        if action == Action::Install {
            same_execution(h, &binding.handler) || generated_handler(h, agent)
        } else {
            same_execution(h, &binding.handler)
        }
    })
    .map_err(|e| format!("{}: {e}", binding.file.display()))?;
    report.push_str(&format!("{}: {}\n", agent.label(), binding.file.display()));
    if descriptions.is_empty() {
        report.push_str("  Nothing to change.\n");
    } else {
        for description in descriptions {
            report.push_str(&format!("  {description}\n"));
        }
        changes.push(Change {
            file: binding.file.clone(),
            before,
            after: text,
        });
    }
    Ok(())
}

/// Preflight every selected file before writing. The receipt records paths and exact
/// execution definitions so uninstall does not depend on today's environment or PATH.
pub fn run(exe: &Path, harnesses: &[Agent], remove: bool, apply: bool) -> Result<String, String> {
    run_with_options(exe, harnesses, remove, apply, &UsageOptions::default())
}

pub fn run_with_options(
    exe: &Path,
    harnesses: &[Agent],
    remove: bool,
    apply: bool,
    options: &UsageOptions,
) -> Result<String, String> {
    let receipt_file =
        crate::paths::data_dir_with(exe, |key| std::env::var_os(key))?.join("aherald-setup.json");
    let receipt_before = read(&receipt_file)?;
    let previous: Receipt = match receipt_before.as_deref() {
        Some(text) => serde_json::from_str(text)
            .map_err(|_| "invalid aHerald setup receipt; nothing was changed")?,
        None => Receipt {
            version: 1,
            executable: exe.to_path_buf(),
            bindings: Vec::new(),
        },
    };
    if previous.version != 1 {
        return Err("unsupported aHerald setup receipt version".into());
    }
    if remove && previous.executable != exe {
        return Err(
            "another executable owns this setup receipt; run --unsetup from that installation"
                .into(),
        );
    }
    let mut bindings = Vec::new();
    if !remove {
        if harnesses.is_empty() {
            return Err("setup requires at least one harness".into());
        }
        for &agent in harnesses {
            let file = file_for(agent)?;
            if !file.is_absolute() {
                return Err("harness configuration directory must be absolute".into());
            }
            bindings.push(Binding {
                agent: agent.key().into(),
                file,
                handler: handler(exe, agent),
            });
        }
    }
    let mut changes = Vec::new();
    if let Some(change) = usage_plan(exe, options)? {
        changes.push(change);
    }
    let mut report = String::new();
    for old in &previous.bindings {
        if !bindings
            .iter()
            .any(|b| b.agent == old.agent && b.file == old.file)
        {
            add_plan(old, Action::Uninstall, &mut changes, &mut report)?;
        }
    }
    for binding in &bindings {
        add_plan(binding, Action::Install, &mut changes, &mut report)?;
    }
    let receipt = serde_json::to_string_pretty(&Receipt {
        version: 1,
        executable: exe.to_path_buf(),
        bindings,
    })
    .map_err(|_| "cannot encode setup receipt")?
        + "\n";
    // No receipt is created for an uninstall that has never been installed.
    if (!remove || receipt_before.is_some()) && receipt_before.as_deref() != Some(&receipt) {
        changes.push(Change {
            file: receipt_file,
            before: receipt_before,
            after: receipt,
        });
    }
    if !apply {
        report
            .push_str("Dry run: nothing was written. Run again with --apply to write the files.\n");
        return Ok(report);
    }
    // Recheck the snapshot before the first write; do not overwrite edits made while planning.
    for change in &changes {
        if read(&change.file)? != change.before {
            return Err("configuration changed during setup; run setup again".into());
        }
    }
    for (index, change) in changes.iter().enumerate() {
        if let Err(error) = hook_setup::write(&change.file, change.before.is_some(), &change.after)
        {
            let mut failures = Vec::new();
            for done in changes[..index].iter().rev() {
                let result = match &done.before {
                    Some(before) => hook_setup::write_inner(&done.file, true, before, false),
                    None => std::fs::remove_file(&done.file).map_err(|e| e.to_string()),
                };
                if let Err(e) = result {
                    failures.push(format!("{}: {e}", done.file.display()));
                }
            }
            return Err(if failures.is_empty() {
                format!("{error}; earlier writes were restored")
            } else {
                format!("{error}; could not restore: {}", failures.join("; "))
            });
        }
    }
    report.push_str("Setup complete. Changed existing files were backed up as *.aherald-backup.\n");
    if !remove && harnesses.contains(&Agent::Codex) {
        report.push_str("Start a new Codex session, open /hooks and trust the aHerald hooks.\n");
    }
    Ok(report)
}
