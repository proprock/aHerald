//! User-context installation of fixed hook bindings. No lookup runs on the hook path.
use crate::{
    agent::Agent,
    bridge_setup::Bridge,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    bridges: Vec<BridgeBinding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    statusline: Option<StatusLineBinding>,
}

/// A bridge file and the exact text aHerald wrote, which is what proves the file is still ours.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BridgeBinding {
    harness: String,
    file: PathBuf,
    text: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StatusLineBinding {
    file: PathBuf,
    installed: Value,
    original: Option<Value>,
    /// The relay script written next to the executable when the installation path allows no
    /// bare command (AH-211); its exact text proves the file is still ours.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    script: Option<OwnedFile>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct OwnedFile {
    file: PathBuf,
    text: String,
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
    /// Bridge files to install. `None` leaves installed bridges alone, so setups that do not
    /// know about them (installer pages) never remove one; `Some` replaces the whole set.
    pub bridges: Option<Vec<Bridge>>,
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
        None => {
            crate::paths::config_dir_with(exe, |key| std::env::var_os(key))?.join("config.toml")
        }
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
        .unwrap_or(include_str!("../config.example.toml"));
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

/// The user's own `statusLine` command that setup wrapped, for `--statusline` to run when it
/// has no `--then`. `None` when there is no receipt, no wrapped command, or the receipt
/// belongs to another installation.
pub fn wrapped_statusline_command(exe: &Path) -> Result<Option<String>, String> {
    let file = crate::paths::receipt_dir_with(exe, |key| std::env::var_os(key))?
        .join("aherald-setup.json");
    let Some(text) = read(&file)? else {
        return Ok(None);
    };
    let receipt: Receipt = serde_json::from_str(&text).map_err(
        |_| "invalid aHerald setup receipt; the status line is shown without your own command",
    )?;
    if receipt.version != 1 || receipt.executable != exe {
        return Ok(None);
    }
    Ok(receipt
        .statusline
        .and_then(|binding| binding.original)
        .filter(|original| original["type"] == "command")
        .and_then(|original| original["command"].as_str().map(str::to_owned)))
}

/// Read the receipt without changing it, for an interactive setup's defaults.
pub fn selected_harnesses(exe: &Path) -> Result<Vec<Agent>, String> {
    let file = crate::paths::receipt_dir_with(exe, |key| std::env::var_os(key))?
        .join("aherald-setup.json");
    let Some(text) = read(&file)? else {
        return Ok(Vec::new());
    };
    let receipt: Receipt = serde_json::from_str(&text)
        .map_err(|_| "invalid aHerald setup receipt; nothing was changed")?;
    if receipt.version != 1 || receipt.executable != exe {
        return Err(
            "setup receipt belongs to another installation or has an unsupported version".into(),
        );
    }
    receipt
        .bindings
        .iter()
        .map(|binding| {
            Agent::from_name(&binding.agent)
                .ok_or_else(|| "unsupported harness in setup receipt".to_owned())
        })
        .collect()
}

fn usage_plan(exe: &Path, options: &UsageOptions) -> Result<Option<Change>, String> {
    if !options.has_selections() {
        return Ok(None);
    }
    let file = settings_file(exe, options.file.as_deref())?;
    let (before, config) = settings_text(&file)?;
    let text = before
        .as_deref()
        .unwrap_or(include_str!("../config.example.toml"));
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
            remove: false,
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
    /// Delete the file instead of writing `after`.
    remove: bool,
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

// Recognize native and legacy definitions only for this executable's full path.
// A matching filename in another installation does not establish ownership.
fn generated_handler(hook: &Value, exe: &Path, agent: Agent) -> bool {
    if hook["type"] != "command" {
        return false;
    }
    let Some(command) = hook["command"].as_str() else {
        return false;
    };
    if let Some(args) = hook.get("args") {
        let expected = match agent {
            Agent::Claude => json!(["--hook", "--agent", "claude", "--resident"]),
            Agent::Codex => json!(["--hook", "--resident"]),
        };
        return Path::new(command) == exe && args == &expected;
    }
    [false, true]
        .into_iter()
        .any(|windows| command == hook_setup::command_for(exe, agent, windows))
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
    exe: &Path,
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
            same_execution(h, &binding.handler) || generated_handler(h, exe, agent)
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
                remove: false,
            },
        );
    }
    Ok(())
}

fn apply_change(change: &Change) -> Result<(), String> {
    if !change.remove {
        return hook_setup::write(&change.file, change.before.is_some(), &change.after);
    }
    let fail = |e: std::io::Error| format!("cannot remove {}: {e}", change.file.display());
    let mut backup = change.file.as_os_str().to_os_string();
    backup.push(".aherald-backup");
    std::fs::copy(&change.file, backup).map_err(fail)?;
    std::fs::remove_file(&change.file).map_err(fail)
}

/// Goose's plugin directory holds nothing else once its hook file is gone.
fn remove_empty_plugin_dirs(file: &Path) {
    if file.file_name().is_some_and(|name| name == "hooks.json") {
        let mut dir = file.parent();
        for _ in 0..2 {
            let Some(current) = dir else { break };
            // `remove_dir` refuses a directory that still holds the backup or anything else.
            if std::fs::remove_dir(current).is_err() {
                break;
            }
            dir = current.parent();
        }
    }
}

/// Plans the bridge files and returns the bindings the receipt should hold afterwards.
fn bridge_plan(
    exe: &Path,
    previous: &[BridgeBinding],
    remove: bool,
    options: &UsageOptions,
    changes: &mut Vec<Change>,
    report: &mut String,
) -> Result<Vec<BridgeBinding>, String> {
    let selected: &[Bridge] = match (&options.bridges, remove) {
        (_, true) => &[],
        (Some(list), false) => list,
        (None, false) => {
            // Untouched: the recorded bridges stay as they are.
            return Ok(previous
                .iter()
                .map(|b| BridgeBinding {
                    harness: b.harness.clone(),
                    file: b.file.clone(),
                    text: b.text.clone(),
                })
                .collect());
        }
    };
    let get = |key: &str| std::env::var_os(key);
    let mut wanted = Vec::new();
    for &bridge in selected {
        let file = bridge.file(get)?;
        if !file.is_absolute() {
            return Err(format!("{} directory must be absolute", bridge.label()));
        }
        wanted.push((bridge, file));
    }
    let mut kept = Vec::new();
    for old in previous {
        if wanted
            .iter()
            .any(|(bridge, file)| bridge.key() == old.harness && *file == old.file)
        {
            continue;
        }
        let label = Bridge::from_name(&old.harness).map_or("Harness", Bridge::label);
        report.push_str(&format!("{label} bridge: {}\n", old.file.display()));
        match read(&old.file)? {
            None => report.push_str("  Already gone.\n"),
            Some(text) if text == old.text => {
                report.push_str("  Remove the file.\n");
                changes.push(Change {
                    file: old.file.clone(),
                    before: Some(text),
                    after: String::new(),
                    remove: true,
                });
            }
            Some(_) => {
                report.push_str("  The file was changed after aHerald wrote it; left in place.\n")
            }
        }
    }
    for (bridge, file) in wanted {
        let text = bridge.text(exe);
        report.push_str(&format!("{} bridge: {}\n", bridge.label(), file.display()));
        let before = read(&file)?;
        let owned = previous
            .iter()
            .find(|b| b.harness == bridge.key() && b.file == file)
            .map(|b| b.text.as_str());
        match before.as_deref() {
            Some(existing) if existing == text => report.push_str("  Nothing to change.\n"),
            Some(existing) if Some(existing) == owned => {
                report.push_str("  Update the file.\n");
                changes.push(Change {
                    file: file.clone(),
                    before,
                    after: text.clone(),
                    remove: false,
                });
            }
            Some(_) => {
                return Err(format!(
                    "{} already exists and is not aHerald's; nothing was changed",
                    file.display()
                ));
            }
            None => {
                report.push_str("  Create the file.\n");
                changes.push(Change {
                    file: file.clone(),
                    before: None,
                    after: text.clone(),
                    remove: false,
                });
            }
        }
        kept.push(BridgeBinding {
            harness: bridge.key().into(),
            file,
            text,
        });
    }
    Ok(kept)
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
    let old_script = previous.as_ref().and_then(|old| old.script.clone());
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
                        remove: false,
                    },
                );
                report.push_str("Claude: restore the original statusLine.\n");
            }
        }
    }
    let Some(file) = selected_file else {
        script_plan(None, old_script.as_ref(), changes, report)?;
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
                .is_some_and(crate::statusline_launch::is_wrapper)
        });
        let own_command = statusline_command(exe, None)?.0;
        if current.is_some()
            && original.is_none()
            && !current
                .as_ref()
                .is_some_and(|v| v["command"] == own_command)
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
    let (wrapped, script) = statusline_command(exe, command)?;
    if wrapped.encode_utf16().count() > 30000 {
        return Err("Claude statusLine command exceeds the Windows command-line limit".into());
    }
    let script = script_plan(script, old_script.as_ref(), changes, report)?;
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
                remove: false,
            },
        );
        report.push_str("Claude: configure the context statusLine wrapper.\n");
    }
    Ok(Some(StatusLineBinding {
        file: file.to_path_buf(),
        installed,
        original,
        script,
    }))
}

/// Plans the relay script next to the executable and returns what the receipt should hold:
/// the file is created or updated when wanted, and removed when it is no longer wanted, but
/// only while its text is still exactly what aHerald wrote.
fn script_plan(
    wanted: Option<(PathBuf, String)>,
    previous: Option<&OwnedFile>,
    changes: &mut Vec<Change>,
    report: &mut String,
) -> Result<Option<OwnedFile>, String> {
    if let Some(old) = previous
        && wanted.as_ref().is_none_or(|(file, _)| *file != old.file)
    {
        match read(&old.file)? {
            None => {}
            Some(text) if text == old.text => {
                report.push_str(&format!(
                    "Claude statusLine script: remove {}\n",
                    old.file.display()
                ));
                changes.push(Change {
                    file: old.file.clone(),
                    before: Some(text),
                    after: String::new(),
                    remove: true,
                });
            }
            Some(_) => report.push_str(&format!(
                "Claude statusLine script: {} was changed after aHerald wrote it; left in place.\n",
                old.file.display()
            )),
        }
    }
    let Some((file, text)) = wanted else {
        return Ok(None);
    };
    let before = read(&file)?;
    let owned = previous
        .filter(|old| old.file == file)
        .map(|old| old.text.as_str());
    let change = match before.as_deref() {
        Some(existing) if existing == text => None,
        Some(existing) if Some(existing) == owned => Some("update"),
        Some(_) => {
            return Err(format!(
                "{} already exists and is not aHerald's; nothing was changed",
                file.display()
            ));
        }
        None => Some("create"),
    };
    if let Some(verb) = change {
        report.push_str(&format!(
            "Claude statusLine script: {verb} {}\n",
            file.display()
        ));
        changes.push(Change {
            file: file.clone(),
            before,
            after: text.clone(),
            remove: false,
        });
    }
    Ok(Some(OwnedFile { file, text }))
}

/// The command for Claude Code's `statusLine`, and the script it needs written, if any.
fn statusline_command(
    exe: &Path,
    original: Option<&str>,
) -> Result<(String, Option<(PathBuf, String)>), String> {
    #[cfg(windows)]
    {
        use crate::statusline_launch as launch;
        // The readable forms carry no wrapped command: `aherald --statusline` reads it from
        // the receipt, which keeps the line short and free of a second quoting layer.
        let companion = exe.with_file_name(launch::COMPANION_NAME);
        if companion.is_file()
            && let Some(command) = launch::companion_command(&companion)
        {
            return Ok((command, None));
        }
        let script = exe.with_file_name(launch::SCRIPT_NAME);
        if let Some(command) = launch::script_command(&script) {
            return Ok((command, Some((script, launch::SCRIPT_TEXT.to_owned()))));
        }
        Ok((encoded_statusline_command(exe, original), None))
    }
    #[cfg(not(windows))]
    {
        let base = hook_setup::statusline_command(exe);
        Ok((
            match original {
                Some(command) => format!("{base} --then '{}'", command.replace('\'', "'\\''")),
                None => base,
            },
            None,
        ))
    }
}

/// The command of versions before AH-211, kept for the paths that neither readable form can
/// carry (a `$`, a backtick or a double quote in the installation path).
#[cfg(windows)]
fn encoded_statusline_command(exe: &Path, original: Option<&str>) -> String {
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
    let receipt_file = crate::paths::receipt_dir_with(exe, |key| std::env::var_os(key))?
        .join("aherald-setup.json");
    let receipt_before = read(&receipt_file)?;
    let previous: Receipt = match receipt_before.as_deref() {
        Some(text) => serde_json::from_str(text)
            .map_err(|_| "invalid aHerald setup receipt; nothing was changed")?,
        None => Receipt {
            version: 1,
            executable: exe.to_path_buf(),
            bindings: Vec::new(),
            bridges: Vec::new(),
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
        if harnesses.is_empty() && options.bridges.as_ref().is_none_or(Vec::is_empty) {
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
        if !bindings.iter().any(|b| {
            b.agent == old.agent && b.file == old.file && same_execution(&b.handler, &old.handler)
        }) {
            add_plan(old, exe, Action::Uninstall, &mut changes, &mut report)?;
        }
    }
    for binding in &bindings {
        add_plan(binding, exe, Action::Install, &mut changes, &mut report)?;
    }
    let bridges = bridge_plan(
        exe,
        &previous.bridges,
        remove,
        options,
        &mut changes,
        &mut report,
    )?;
    let claude_file = bindings
        .iter()
        .find(|b| b.agent == "claude")
        .map(|b| b.file.clone());
    // The wrapper serves the context warning and the configured status line text.
    let wrapper_wanted = if claude_file.is_some() {
        let file = settings_file(exe, options.file.as_deref())?;
        let config = if let Some(change) = changes.iter().find(|c| c.file == file) {
            crate::config::Config::parse(
                &change.after,
                file.parent().ok_or("configuration has no directory")?,
            )?
        } else {
            settings_text(&file)?.1
        };
        config.claude.warnings.context_threshold().is_some()
            || !config.claude.statusline.format.is_empty()
    } else {
        false
    };
    let statusline = statusline_plan(
        exe,
        claude_file.as_deref().filter(|_| wrapper_wanted),
        previous.statusline,
        &mut changes,
        &mut report,
    )?;
    let receipt = serde_json::to_string_pretty(&Receipt {
        version: 1,
        executable: exe.to_path_buf(),
        bindings,
        bridges,
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
            remove: false,
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
        if let Err(error) = apply_change(change) {
            let mut failures = Vec::new();
            for done in changes[..index].iter().rev() {
                let result = match &done.before {
                    Some(before) => {
                        hook_setup::write_inner(&done.file, done.file.exists(), before, false)
                    }
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
    for change in changes.iter().filter(|c| c.remove) {
        remove_empty_plugin_dirs(&change.file);
    }
    for bridge in options.bridges.iter().flatten().filter(|_| !remove) {
        report.push_str(bridge.restart_hint());
        report.push('\n');
    }
    if !remove && harnesses.contains(&Agent::Codex) {
        report.push_str("Start a new Codex session, open /hooks and trust the aHerald hooks.\n");
    }
    Ok(report)
}
