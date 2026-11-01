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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    statusline: Option<StatusLineBinding>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusLineBinding {
    file: PathBuf,
    installed: Value,
    original: Option<Value>,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct UsageOptions {
    pub file: Option<PathBuf>,
    pub executable: Option<PathBuf>,
    pub codex: Option<bool>,
    pub claude: Option<bool>,
    pub codex_context: Option<bool>,
    pub claude_context: Option<bool>,
    pub codex_idle: Option<bool>,
    pub claude_idle: Option<bool>,
}

impl UsageOptions {
    pub fn has_selections(&self) -> bool {
        [
            self.codex,
            self.claude,
            self.codex_context,
            self.claude_context,
            self.codex_idle,
            self.claude_idle,
        ]
        .iter()
        .any(Option::is_some)
    }
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
        "codex_usage={}\nclaude_usage={}\ncodex_context={}\nclaude_context={}\ncodex_idle={}\nclaude_idle={}\n",
        config.codex.usage.enabled,
        config.claude.usage.enabled,
        config.codex.warnings.context_threshold().is_some(),
        config.claude.warnings.context_threshold().is_some(),
        config.codex.warnings.idle_threshold().is_some(),
        config.claude.warnings.idle_threshold().is_some()
    ))
}

fn usage_plan(exe: &Path, options: &UsageOptions) -> Result<Option<Change>, String> {
    if !options.has_selections() {
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
    let mut changed = false;
    for (agent, selected, current) in [
        ("codex", options.codex, config.codex.usage.enabled),
        ("claude", options.claude, config.claude.usage.enabled),
    ] {
        if let Some(selected) = selected
            && selected != current
        {
            replace_setting(&mut document[agent]["usage"]["enabled"], selected.into());
            changed = true;
        }
    }
    for (agent, context, idle, current, default_context, default_idle) in [
        (
            "codex",
            options.codex_context,
            options.codex_idle,
            &config.codex.warnings,
            "250k",
            25_i64,
        ),
        (
            "claude",
            options.claude_context,
            options.claude_idle,
            &config.claude.warnings,
            "400k",
            55_i64,
        ),
    ] {
        for (selected, key, enabled, threshold, missing, default) in [
            (
                context,
                "context_enabled",
                current.context_enabled,
                "context_tokens",
                current.context_tokens.is_none(),
                toml_edit::Value::from(default_context),
            ),
            (
                idle,
                "idle_enabled",
                current.idle_enabled,
                "idle_minutes",
                current.idle_minutes.is_none(),
                toml_edit::Value::from(default_idle),
            ),
        ] {
            if let Some(selected) = selected
                && (selected != enabled || (selected && missing))
            {
                if selected != enabled {
                    replace_setting(&mut document[agent]["warnings"][key], selected.into());
                }
                if selected && missing {
                    replace_setting(&mut document[agent]["warnings"][threshold], default);
                }
                changed = true;
            }
        }
    }
    let after = if before.is_some() && !changed {
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

fn replace_setting(item: &mut toml_edit::Item, mut value: toml_edit::Value) {
    // Replacing only the scalar retains the user's surrounding comment.
    if let Some(old) = item.as_value() {
        *value.decor_mut() = old.decor().clone();
    }
    *item = toml_edit::Item::Value(value);
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
    let existing = changes
        .iter()
        .find(|c| c.file == binding.file)
        .map(|c| c.after.as_str())
        .or(before.as_deref());
    let Plan {
        text,
        changes: descriptions,
    } = hook_setup::plan_handler(existing, agent, &binding.handler, action, |h| {
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
        merge_change(
            changes,
            Change {
                file: binding.file.clone(),
                before,
                after: text,
            },
        );
    }
    Ok(())
}

fn merge_change(changes: &mut Vec<Change>, change: Change) {
    if let Some(pending) = changes.iter_mut().find(|c| c.file == change.file) {
        pending.after = change.after;
    } else {
        changes.push(change);
    }
}

fn statusline_plan(
    exe: &Path,
    selected_file: Option<&Path>,
    previous: Option<StatusLineBinding>,
    changes: &mut Vec<Change>,
    report: &mut String,
) -> Result<Option<StatusLineBinding>, String> {
    let mut original = None;
    let mut owned = false;
    if let Some(old) = previous {
        if !old.file.is_absolute() {
            return Err("setup receipt contains a relative statusLine path".into());
        }
        let before = read(&old.file)?;
        let text = changes
            .iter()
            .find(|c| c.file == old.file)
            .map(|c| c.after.as_str())
            .or(before.as_deref())
            .unwrap_or("{}");
        let mut root: Value =
            serde_json::from_str(text).map_err(|_| "invalid Claude settings JSON")?;
        if root.get("statusLine") == Some(&old.installed) {
            if selected_file == Some(old.file.as_path()) {
                original = old.original;
                owned = true;
            } else {
                if let Some(value) = old.original {
                    root["statusLine"] = value;
                } else {
                    root.as_object_mut()
                        .ok_or("Claude settings must be an object")?
                        .remove("statusLine");
                }
                merge_change(
                    changes,
                    Change {
                        file: old.file,
                        before,
                        after: serde_json::to_string_pretty(&root)
                            .map_err(|_| "cannot encode Claude settings")?
                            + "\n",
                    },
                );
                report.push_str("Claude: restore the original statusLine.\n");
            }
        }
    }
    let Some(file) = selected_file else {
        return Ok(None);
    };
    let before = read(file)?;
    let text = changes
        .iter()
        .find(|c| c.file == file)
        .map(|c| c.after.as_str())
        .or(before.as_deref())
        .unwrap_or("{}");
    let mut root: Value = serde_json::from_str(text).map_err(|_| "invalid Claude settings JSON")?;
    let object = root
        .as_object_mut()
        .ok_or("Claude settings must be an object")?;
    let current = object.get("statusLine").cloned();
    // A retained binding supplies the original command; never nest our wrapper.
    if !owned {
        original = current.clone().filter(|value| {
            !value["command"]
                .as_str()
                .is_some_and(|s| s.contains("aherald") && s.contains(" --statusline"))
        });
        if current.is_some()
            && original.is_none()
            && !current
                .as_ref()
                .is_some_and(|v| v["command"] == statusline_command(exe, None))
        {
            return Err("a different aHerald statusLine is set; uninstall it first".into());
        }
    }
    let command = match original.as_ref() {
        None => None,
        Some(value) if value["type"] == "command" => Some(
            value["command"]
                .as_str()
                .ok_or("Claude statusLine has no command")?,
        ),
        Some(_) => return Err("Claude statusLine is not a command".into()),
    };
    let mut installed = original
        .clone()
        .unwrap_or_else(|| json!({"type":"command"}));
    let wrapped = statusline_command(exe, command);
    if wrapped.encode_utf16().count() > 30000 {
        return Err("Claude statusLine command exceeds the Windows command-line limit".into());
    }
    installed["command"] = wrapped.into();
    if current.as_ref() != Some(&installed) {
        object.insert("statusLine".into(), installed.clone());
        merge_change(
            changes,
            Change {
                file: file.to_path_buf(),
                before,
                after: serde_json::to_string_pretty(&root)
                    .map_err(|_| "cannot encode Claude settings")?
                    + "\n",
            },
        );
        report.push_str("Claude: configure the context statusLine wrapper.\n");
    }
    Ok(Some(StatusLineBinding {
        file: file.to_path_buf(),
        installed,
        original,
    }))
}

fn statusline_command(exe: &Path, original: Option<&str>) -> String {
    #[cfg(windows)]
    {
        // Numeric UTF-16 literals avoid expansion by either Git Bash or PowerShell.
        let literal = |text: &str| {
            format!(
                "(-join [char[]]({}))",
                text.encode_utf16()
                    .map(|c| c.to_string())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        };
        let then = original
            .map(|s| format!(" --then {}", quote_windows_argument(s)))
            .unwrap_or_default();
        // Process.Start initializes a text writer even though we use its byte stream.
        // Use BOM-free input encoding so that initialization cannot prefix the payload.
        let script = format!(
            "$ErrorActionPreference='Stop'; [Console]::InputEncoding=[Text.UTF8Encoding]::new($false); $s=[Diagnostics.ProcessStartInfo]::new(); $s.FileName='{}'; $s.Arguments='{}'; $s.UseShellExecute=$false; $s.CreateNoWindow=$true; $s.RedirectStandardInput=$true; $s.RedirectStandardOutput=$true; $p=[Diagnostics.Process]::Start($s); [Console]::OpenStandardInput().CopyTo($p.StandardInput.BaseStream); $p.StandardInput.BaseStream.Close(); $p.StandardOutput.BaseStream.CopyTo([Console]::OpenStandardOutput()); $p.WaitForExit(); exit $p.ExitCode",
            exe.display().to_string().replace('\'', "''"),
            format!("--statusline{then}").replace('\'', "''")
        );
        format!(
            "powershell.exe -NoProfile -NonInteractive -Command \"& ([scriptblock]::Create({})); # aherald --statusline\"",
            literal(&script)
        )
    }
    #[cfg(not(windows))]
    {
        let base = hook_setup::statusline_command(exe);
        match original {
            Some(command) => format!("{base} --then '{}'", command.replace('\'', "'\\''")),
            None => base,
        }
    }
}

#[cfg(windows)]
fn quote_windows_argument(text: &str) -> String {
    let mut quoted = String::from("\"");
    let mut slashes = 0;
    for ch in text.chars() {
        if ch == '\\' {
            slashes += 1;
            continue;
        }
        quoted.extend(std::iter::repeat_n(
            '\\',
            if ch == '"' { slashes * 2 + 1 } else { slashes },
        ));
        quoted.push(ch);
        slashes = 0;
    }
    quoted.extend(std::iter::repeat_n('\\', slashes * 2));
    quoted.push('"');
    quoted
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
    let exe = options.executable.as_deref().unwrap_or(exe);
    if !exe.is_absolute() {
        return Err("setup executable path must be absolute".into());
    }
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
            statusline: None,
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
    let claude_file = bindings
        .iter()
        .find(|b| b.agent == "claude")
        .map(|b| b.file.clone());
    let context_enabled = if claude_file.is_some() {
        let file = settings_file(exe, options.file.as_deref())?;
        if let Some(change) = changes.iter().find(|c| c.file == file) {
            crate::config::Config::parse(
                &change.after,
                file.parent().ok_or("configuration has no directory")?,
            )?
            .claude
            .warnings
            .context_threshold()
            .is_some()
        } else {
            settings_text(&file)?
                .1
                .claude
                .warnings
                .context_threshold()
                .is_some()
        }
    } else {
        false
    };
    let statusline = statusline_plan(
        exe,
        claude_file.as_deref().filter(|_| context_enabled),
        previous.statusline,
        &mut changes,
        &mut report,
    )?;
    let receipt = serde_json::to_string_pretty(&Receipt {
        version: 1,
        executable: exe.to_path_buf(),
        bindings,
        statusline,
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
