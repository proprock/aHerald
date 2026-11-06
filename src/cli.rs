use crate::{agent::Agent, config::Mode};
use std::path::PathBuf;

/// What the command line asks for. No operation means "run": start the resident or, with
/// `--transient`, show one card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Demo,
    SetupSettings,
    ConsoleSetup,
    SetupWindow,
    Setup,
    Unsetup,
    Hook,
    Status,
    RemoteInfo,
    Quit,
    Help,
    Version,
    PerfReport,
    Statusline,
    InstallStatusline,
    UninstallStatusline,
    Notify,
    Event,
    InstallHooks,
    UninstallHooks,
    RawOutput,
    /// Internal: the resident process that a launcher started (`--resident-child SCOPE --config PATH`).
    ResidentChild,
    /// Internal: the process that shows one card (`--display-child --config PATH`, event on stdin).
    DisplayChild,
}

impl Operation {
    /// The operation a plain `--flag` names; the flags that carry values are parsed in `Cli::parse`.
    fn from_flag(flag: &str) -> Option<Self> {
        Some(match flag {
            "--setup-settings" => Self::SetupSettings,
            "--console-setup" => Self::ConsoleSetup,
            "--setup-window" => Self::SetupWindow,
            "--setup" => Self::Setup,
            "--unsetup" => Self::Unsetup,
            "--hook" => Self::Hook,
            "--status" => Self::Status,
            "--remote-info" => Self::RemoteInfo,
            "--quit" => Self::Quit,
            "--help" => Self::Help,
            "--version" => Self::Version,
            "--perf-report" => Self::PerfReport,
            "--statusline" => Self::Statusline,
            "--install-statusline" => Self::InstallStatusline,
            "--uninstall-statusline" => Self::UninstallStatusline,
            _ => return None,
        })
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cli {
    pub operation: Option<Operation>,
    /// Internal: the process that started this one is to be waited for first (`--restart-child PID`).
    pub restart_parent: Option<u32>,
    /// Internal: the restarted resident shows its test cards (`--test-after-apply`).
    pub test_after_apply: bool,
    /// No argument followed the restart prefix, if there was one: a bare launch.
    pub no_arguments: bool,
    /// Internal: the instance scope that `--resident-child` serves.
    pub child_scope: Option<String>,
    pub mode: Option<Mode>,
    pub event: Option<String>,
    pub config: Option<PathBuf>,
    pub demo_pause_sec: Option<u64>,
    /// Agent that produced a `--hook` payload; Codex when omitted.
    pub agent: Option<Agent>,
    /// Agent whose hook file `--install-hooks` / `--uninstall-hooks` changes.
    pub hooks_agent: Option<Agent>,
    /// Write the change; without it hook setup is a dry run.
    pub apply: bool,
    /// Harnesses explicitly selected by the user or installer.
    pub harnesses: Vec<Agent>,
    /// Explicit installer preferences; omitted values are preserved.
    pub usage: crate::setup::UsageOptions,
    /// Hook file to use instead of the agent's default one.
    pub hooks_file: Option<PathBuf>,
    /// Command `--statusline` runs with the same input, to keep a status line the user has.
    pub then: Option<String>,
    /// Harness key of a `--notify` event (see `events::is_harness_key`).
    pub notify: Option<String>,
    /// The existing event kind a `--notify` event reports.
    pub kind: Option<crate::events::Kind>,
    /// `--notify` evidence only: the session resumed work, so its waits are over.
    pub activity: bool,
    /// `--notify` evidence only: the whole session ended.
    pub ended: bool,
    /// What `--raw-output` asks the resident for.
    pub raw_source: Option<crate::ipc::RawSource>,
}

/// The text of `--help`, grouped by purpose. Every public flag appears here; the internal
/// `--*-child` and `--test-after-apply` re-entry flags do not. A test holds the list to `Cli::parse`.
pub fn help() -> &'static str {
    concat!(
        "aHerald — native notification companion (Windows, macOS, Linux X11/XWayland)\n",
        "Usage: aherald [OPERATION] [--resident | --transient] [--config PATH]\n",
        "\n",
        "Hooks and events (what an agent calls)\n",
        "  --hook [--agent claude|codex]   Read one hook payload from stdin; Codex unless --agent claude.\n",
        "  --notify HARNESS --kind KIND [--activity | --ended]\n",
        "                                  Report an event of a harness without hooks (OpenCode, Pi, Goose,\n",
        "                                  Crush). KIND: turn_complete, permission_required, input_required,\n",
        "                                  error, interrupted. --activity: work resumed; --ended: the session\n",
        "                                  ended (evidence only, never shown).\n",
        "  --event EVENT                   Send a sample event of that kind, to check the configuration.\n",
        "  --statusline [--then COMMAND]   Claude Code statusLine: tell a running resident the session name,\n",
        "                                  quota and input context, and run COMMAND unchanged; without --then\n",
        "                                  print the [claude.statusline] format, if set.\n",
        "\n",
        "Resident control\n",
        "  --resident | --transient        Run the local notification service, or show one card and exit.\n",
        "  --status | --quit               Ask the running resident for its state, or stop it.\n",
        "  --remote-info                   Print the endpoint and token an agent in a container or VM needs;\n",
        "                                  the resident listens only with [remote] enabled = true.\n",
        "  --demo [SECONDS]                Loop a configuration preview (3-second pauses); exit through its\n",
        "                                  tray. Takes no mode flag.\n",
        "\n",
        "Setup (dry runs unless --apply)\n",
        "  --setup --harness NAME [--harness NAME] [--replace-bridges]\n",
        "                                  Configure the selected harnesses for this executable. NAME: codex,\n",
        "                                  claude, opencode, pi or goose. --replace-bridges makes the named\n",
        "                                  bridge files the whole set (empty if none).\n",
        "  --unsetup                       Remove the saved bindings.\n",
        "  --console-setup                 Interactive setup (Linux).\n",
        "  --setup-window                  The setup window (macOS).\n",
        "  --codex-usage | --claude-usage | --codex-context | --claude-context | --codex-idle |\n",
        "  --claude-idle  true|false       Change one usage or warning setting with --setup; custom\n",
        "                                  thresholds are kept.\n",
        "                                  First context enablement uses Codex 250k / Claude 400k when no\n",
        "                                  threshold exists.\n",
        "  --setup-executable PATH         Bind the hooks to this executable instead of the running one.\n",
        "  --settings-file PATH            Absolute path of the configuration that setup edits or reports.\n",
        "  --setup-settings                Report the current usage and warning flags without writing.\n",
        "  --install-hooks AGENT | --uninstall-hooks AGENT\n",
        "                                  Legacy: add or remove only aHerald's entries in the hook file of\n",
        "                                  codex or claude, with a backup. --hooks-file PATH picks the file.\n",
        "  --install-statusline | --uninstall-statusline\n",
        "                                  Wrap or restore Claude Code's statusLine command.\n",
        "  --apply                         Write the change instead of showing it.\n",
        "\n",
        "Usage and diagnostics\n",
        "  --raw-output usage --harness codex|claude | --raw-output statusline\n",
        "                                  Print the last unparsed usage answer, or the last Claude Code\n",
        "                                  statusLine payload, kept in the running resident's memory only;\n",
        "                                  the source and age go to stderr.\n",
        "  --perf-report                   Rank the timings that a resident with general.logging = \"debug\"\n",
        "                                  wrote to aherald.log.\n",
        "  --version | --help              Print the version and build time, or this text.\n",
        "\n",
        "Configuration\n",
        "  Without --config, the installation's default config.toml is used and created with defaults if\n",
        "  absent. Linux system packages use XDG directories; portable installations use config.toml beside\n",
        "  the executable. Relative resources use the selected configuration directory.\n",
        "  Optional per-agent [claude.warnings] / [codex.warnings] context_tokens and idle_minutes enable\n",
        "  session warnings in resident mode; Codex context requires PostModelRequest (verified CLI\n",
        "  0.166.0-alpha.5.1). Enable usage in the resident configuration for read-only Codex quotas.\n",
        "  Codex input monitoring: unsupported for unrelated sessions.\n",
    )
}

impl Cli {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut cli = Self::default();
        let mut raw_kind = None;
        let mut args = args.into_iter().peekable();
        // The re-entry flags of the program's own child processes: only ever the first argument
        // (a restart prefix, then at most one more), never part of a user's command line.
        if args.peek().map(String::as_str) == Some("--restart-child") {
            args.next();
            let pid = args.next().ok_or("missing restart parent")?;
            cli.restart_parent = Some(pid.parse().map_err(|_| "invalid restart parent")?);
            if args.peek().map(String::as_str) == Some("--test-after-apply") {
                args.next();
                cli.test_after_apply = true;
            }
        }
        cli.no_arguments = args.peek().is_none();
        match args.peek().map(String::as_str) {
            Some("--resident-child") => {
                let rest: Vec<String> = args.collect();
                let [_, scope, flag, path] = rest.as_slice() else {
                    return Err("invalid resident invocation".into());
                };
                if flag != "--config" {
                    return Err("invalid resident invocation".into());
                }
                cli.operation = Some(Operation::ResidentChild);
                cli.child_scope = Some(scope.clone());
                cli.config = Some(path.into());
                return Ok(cli);
            }
            Some("--display-child") => {
                let rest: Vec<String> = args.collect();
                let [_, flag, path] = rest.as_slice() else {
                    return Err("invalid internal invocation".into());
                };
                if flag != "--config" {
                    return Err("invalid internal invocation".into());
                }
                cli.operation = Some(Operation::DisplayChild);
                cli.config = Some(path.into());
                return Ok(cli);
            }
            _ => {}
        }
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--demo" => {
                    if cli.operation.replace(Operation::Demo).is_some() {
                        return Err("conflicting explicit operations".into());
                    }
                    let seconds = if args.peek().is_some_and(|s| !s.starts_with("--")) {
                        args.next()
                            .ok_or("missing demo pause")?
                            .parse::<u64>()
                            .map_err(|_| "demo pause must be a positive whole number of seconds")?
                    } else {
                        3
                    };
                    if !(1..=86_400).contains(&seconds) {
                        return Err("demo pause must be 1..86400 seconds".into());
                    }
                    cli.demo_pause_sec = Some(seconds);
                }
                "--setup-settings"
                | "--console-setup"
                | "--setup-window"
                | "--setup"
                | "--unsetup"
                | "--hook"
                | "--status"
                | "--remote-info"
                | "--quit"
                | "--help"
                | "--version"
                | "--perf-report"
                | "--statusline"
                | "--install-statusline"
                | "--uninstall-statusline" => {
                    let operation = Operation::from_flag(&arg).expect("a plain operation flag");
                    if cli.operation.replace(operation).is_some() {
                        return Err("conflicting explicit operations".into());
                    }
                }
                "--resident" | "--transient" => {
                    let mode = if arg == "--resident" {
                        Mode::Resident
                    } else {
                        Mode::Transient
                    };
                    if cli.mode.replace(mode).is_some() {
                        return Err("conflicting explicit modes".into());
                    }
                }
                "--notify" => {
                    if cli.operation.replace(Operation::Notify).is_some() {
                        return Err("conflicting explicit operations".into());
                    }
                    let key = args.next().ok_or("--notify requires a harness key")?;
                    if !crate::events::is_harness_key(&key) {
                        return Err("invalid or reserved harness key".into());
                    }
                    cli.notify = Some(key);
                }
                "--kind" => {
                    if cli.kind.is_some() {
                        return Err("duplicate --kind".into());
                    }
                    let name = args.next().ok_or("--kind requires a value")?;
                    // The two warnings are generated by aHerald itself, never reported.
                    cli.kind = Some(
                        crate::events::Kind::from_name(&name)
                            .filter(|kind| {
                                !matches!(
                                    kind,
                                    crate::events::Kind::ContextWarning
                                        | crate::events::Kind::IdleWarning
                                )
                            })
                            .ok_or("unknown event")?,
                    );
                }
                "--activity" | "--ended" => {
                    let flag = if arg == "--activity" {
                        &mut cli.activity
                    } else {
                        &mut cli.ended
                    };
                    if std::mem::replace(flag, true) {
                        return Err(format!("duplicate {arg}"));
                    }
                }
                "--event" => {
                    if cli.operation.replace(Operation::Event).is_some() {
                        return Err("conflicting explicit operations".into());
                    }
                    let event = args.next().ok_or("--event requires a value")?;
                    if crate::events::Kind::from_name(&event).is_none() {
                        return Err("unknown event".into());
                    }
                    cli.event = Some(event);
                }
                "--install-hooks" | "--uninstall-hooks" => {
                    let operation = if arg == "--install-hooks" {
                        Operation::InstallHooks
                    } else {
                        Operation::UninstallHooks
                    };
                    if cli.operation.replace(operation).is_some() {
                        return Err("conflicting explicit operations".into());
                    }
                    let name = args.next().ok_or("--install-hooks requires an agent")?;
                    cli.hooks_agent = Some(Agent::from_name(&name).ok_or("unknown agent")?);
                }
                "--raw-output" => {
                    if cli.operation.replace(Operation::RawOutput).is_some() {
                        return Err("conflicting explicit operations".into());
                    }
                    raw_kind = Some(
                        args.next()
                            .ok_or("--raw-output requires usage or statusline")?,
                    );
                }
                "--harness" => {
                    let name = args.next().ok_or("--harness requires a value")?;
                    if let Some(bridge) = crate::bridge_setup::Bridge::from_name(&name) {
                        let bridges = cli.usage.bridges.get_or_insert_with(Vec::new);
                        if bridges.contains(&bridge) {
                            return Err("duplicate --harness".into());
                        }
                        bridges.push(bridge);
                        continue;
                    }
                    let agent = Agent::from_name(&name).ok_or("unknown harness")?;
                    if cli.harnesses.contains(&agent) {
                        return Err("duplicate --harness".into());
                    }
                    cli.harnesses.push(agent);
                }
                "--codex-usage" | "--claude-usage" | "--codex-context" | "--claude-context"
                | "--codex-idle" | "--claude-idle" => {
                    let selected = match args.next().as_deref() {
                        Some("true") => true,
                        Some("false") => false,
                        _ => return Err("setting selection must be true or false".into()),
                    };
                    let slot = match arg.as_str() {
                        "--codex-usage" => &mut cli.usage.codex,
                        "--claude-usage" => &mut cli.usage.claude,
                        "--codex-context" => &mut cli.usage.codex_context,
                        "--claude-context" => &mut cli.usage.claude_context,
                        "--codex-idle" => &mut cli.usage.codex_idle,
                        _ => &mut cli.usage.claude_idle,
                    };
                    if slot.replace(selected).is_some() {
                        return Err("duplicate setting selection".into());
                    }
                }
                "--setup-executable" => {
                    if cli.usage.executable.is_some() {
                        return Err("duplicate --setup-executable".into());
                    }
                    cli.usage.executable = Some(
                        args.next()
                            .ok_or("--setup-executable requires a path")?
                            .into(),
                    );
                }
                "--settings-file" => {
                    if cli.usage.file.is_some() {
                        return Err("duplicate --settings-file".into());
                    }
                    cli.usage.file =
                        Some(args.next().ok_or("--settings-file requires a path")?.into());
                }
                "--apply" => cli.apply = true,
                // A setup page states its whole selection: the bridges named with --harness are
                // the complete set, empty when none is named, so a cleared box removes its file.
                "--replace-bridges" => {
                    cli.usage.bridges.get_or_insert_with(Vec::new);
                }
                "--then" => {
                    if cli.then.is_some() {
                        return Err("duplicate --then".into());
                    }
                    cli.then = Some(args.next().ok_or("--then requires a command")?);
                }
                "--hooks-file" => {
                    if cli.hooks_file.is_some() {
                        return Err("duplicate --hooks-file".into());
                    }
                    cli.hooks_file =
                        Some(args.next().ok_or("--hooks-file requires a path")?.into());
                }
                "--agent" => {
                    if cli.agent.is_some() {
                        return Err("duplicate --agent".into());
                    }
                    let name = args.next().ok_or("--agent requires a value")?;
                    cli.agent = Some(Agent::from_name(&name).ok_or("unknown agent")?);
                }
                "--config" => {
                    if cli.config.is_some() {
                        return Err("duplicate --config".into());
                    }
                    cli.config = Some(args.next().ok_or("--config requires a path")?.into());
                }
                _ => return Err("unknown argument".into()),
            }
        }
        if matches!(
            cli.operation,
            Some(
                Operation::Status
                    | Operation::Quit
                    | Operation::Help
                    | Operation::Version
                    | Operation::Demo
                    | Operation::RemoteInfo
            )
        ) && cli.mode.is_some()
        {
            return Err("operation does not accept a mode".into());
        }
        let setup = matches!(
            cli.operation,
            Some(
                Operation::Setup
                    | Operation::Unsetup
                    | Operation::InstallHooks
                    | Operation::UninstallHooks
                    | Operation::InstallStatusline
                    | Operation::UninstallStatusline
            )
        );
        let native_setup = matches!(cli.operation, Some(Operation::Setup | Operation::Unsetup));
        let bridges_named = cli.usage.bridges.is_some();
        if cli.operation == Some(Operation::Setup) && cli.harnesses.is_empty() && !bridges_named {
            return Err(
                "--setup requires at least one --harness codex|claude|opencode|pi|goose".into(),
            );
        }
        let raw = cli.operation == Some(Operation::RawOutput);
        if (bridges_named && cli.operation != Some(Operation::Setup))
            || (!cli.harnesses.is_empty()
                && !matches!(cli.operation, Some(Operation::Setup | Operation::RawOutput)))
        {
            return Err("--harness is only valid with --setup or --raw-output".into());
        }
        if raw
            && (cli.mode.is_some()
                || cli.agent.is_some()
                || cli.hooks_file.is_some()
                || cli.then.is_some()
                || cli.apply)
        {
            return Err("--raw-output only accepts --harness".into());
        }
        if let Some(kind) = raw_kind {
            use crate::ipc::RawSource;
            cli.raw_source = Some(match (kind.as_str(), cli.harnesses.as_slice()) {
                ("usage", [Agent::Codex]) => RawSource::CodexUsage,
                ("usage", [Agent::Claude]) => RawSource::ClaudeUsage,
                ("usage", _) => {
                    return Err("--raw-output usage requires one --harness codex|claude".into());
                }
                ("statusline", [] | [Agent::Claude]) => RawSource::ClaudeStatusLine,
                ("statusline", _) => {
                    return Err("--raw-output statusline is for --harness claude only".into());
                }
                _ => return Err("--raw-output expects usage or statusline".into()),
            });
        }
        let settings_read = cli.operation == Some(Operation::SetupSettings);
        if matches!(
            cli.operation,
            Some(Operation::ConsoleSetup | Operation::SetupWindow)
        ) && (cli.mode.is_some()
            || cli.agent.is_some()
            || cli.config.is_some()
            || cli.hooks_file.is_some()
            || cli.then.is_some()
            || cli.apply
            || cli.usage.file.is_some()
            || cli.usage.executable.is_some()
            || cli.usage.has_selections())
        {
            return Err("--console-setup and --setup-window do not accept other options".into());
        }
        if cli.usage.executable.is_some() && !native_setup {
            return Err("--setup-executable is only valid with --setup or --unsetup".into());
        }
        if cli.usage.has_selections() && !native_setup {
            return Err("setting selections are only valid with --setup or --unsetup".into());
        }
        if cli.usage.file.is_some() && !native_setup && !settings_read {
            return Err("--settings-file is only valid with native setup".into());
        }
        if settings_read
            && (cli.apply
                || cli.mode.is_some()
                || cli.agent.is_some()
                || cli.config.is_some()
                || cli.hooks_file.is_some()
                || cli.then.is_some())
        {
            return Err("--setup-settings only accepts --settings-file".into());
        }
        if native_setup && (cli.hooks_file.is_some() || cli.config.is_some()) {
            return Err("--setup / --unsetup do not accept --hooks-file or --config".into());
        }
        if setup && (cli.mode.is_some() || cli.agent.is_some()) {
            return Err("hook setup does not accept a mode or --agent".into());
        }
        if cli.then.is_some() && cli.operation != Some(Operation::Statusline) {
            return Err("--then is only valid with --statusline".into());
        }
        if cli.operation == Some(Operation::Statusline)
            && (cli.mode.is_some() || cli.agent.is_some())
        {
            return Err("--statusline does not accept a mode or --agent".into());
        }
        if (cli.apply || cli.hooks_file.is_some()) && !setup {
            return Err(
                "--apply and --hooks-file are only valid with --install-hooks or --uninstall-hooks"
                    .into(),
            );
        }
        let notify = cli.operation == Some(Operation::Notify);
        if notify && cli.kind.is_none() {
            return Err("--notify requires --kind".into());
        }
        if !notify && (cli.kind.is_some() || cli.activity || cli.ended) {
            return Err("--kind, --activity and --ended are only valid with --notify".into());
        }
        if cli.activity && cli.ended {
            return Err("--activity and --ended exclude each other".into());
        }
        if cli.agent.is_some() && cli.operation != Some(Operation::Hook) {
            return Err("--agent is only valid with --hook".into());
        }
        Ok(cli)
    }

    pub fn hook_agent(&self) -> Agent {
        self.agent.unwrap_or(Agent::Codex)
    }

    pub fn selected_mode(&self, config: &crate::config::Config) -> Mode {
        self.mode.unwrap_or(config.general.mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every public flag with the smallest valid command line that uses it.
    const PUBLIC: &[(&str, &[&str])] = &[
        (
            "--activity",
            &[
                "--notify",
                "opencode",
                "--kind",
                "turn_complete",
                "--activity",
            ],
        ),
        ("--agent", &["--hook", "--agent", "claude"]),
        ("--apply", &["--install-hooks", "claude", "--apply"]),
        (
            "--claude-context",
            &["--setup", "--harness", "claude", "--claude-context", "true"],
        ),
        (
            "--claude-idle",
            &["--setup", "--harness", "claude", "--claude-idle", "false"],
        ),
        (
            "--claude-usage",
            &["--setup", "--harness", "claude", "--claude-usage", "true"],
        ),
        (
            "--codex-context",
            &["--setup", "--harness", "codex", "--codex-context", "true"],
        ),
        (
            "--codex-idle",
            &["--setup", "--harness", "codex", "--codex-idle", "false"],
        ),
        (
            "--codex-usage",
            &["--setup", "--harness", "codex", "--codex-usage", "true"],
        ),
        ("--config", &["--config", "config.toml"]),
        ("--console-setup", &["--console-setup"]),
        ("--demo", &["--demo", "5"]),
        (
            "--ended",
            &["--notify", "opencode", "--kind", "turn_complete", "--ended"],
        ),
        ("--event", &["--event", "turn_complete"]),
        ("--harness", &["--setup", "--harness", "codex"]),
        ("--help", &["--help"]),
        ("--hook", &["--hook"]),
        (
            "--hooks-file",
            &["--install-hooks", "claude", "--hooks-file", "settings.json"],
        ),
        ("--install-hooks", &["--install-hooks", "codex"]),
        ("--install-statusline", &["--install-statusline"]),
        ("--kind", &["--notify", "pi", "--kind", "error"]),
        ("--notify", &["--notify", "goose", "--kind", "interrupted"]),
        ("--perf-report", &["--perf-report"]),
        ("--quit", &["--quit"]),
        ("--raw-output", &["--raw-output", "statusline"]),
        ("--remote-info", &["--remote-info"]),
        (
            "--replace-bridges",
            &["--setup", "--harness", "pi", "--replace-bridges"],
        ),
        ("--resident", &["--resident"]),
        ("--setup", &["--setup", "--harness", "claude"]),
        (
            "--setup-executable",
            &[
                "--setup",
                "--harness",
                "codex",
                "--setup-executable",
                "/bin/aherald",
            ],
        ),
        ("--setup-settings", &["--setup-settings"]),
        ("--setup-window", &["--setup-window"]),
        (
            "--settings-file",
            &["--setup-settings", "--settings-file", "/tmp/config.toml"],
        ),
        ("--status", &["--status"]),
        ("--statusline", &["--statusline"]),
        ("--then", &["--statusline", "--then", "other-command"]),
        ("--transient", &["--transient"]),
        ("--uninstall-hooks", &["--uninstall-hooks", "claude"]),
        ("--uninstall-statusline", &["--uninstall-statusline"]),
        ("--unsetup", &["--unsetup"]),
        ("--version", &["--version"]),
    ];

    fn flags_in(text: &str) -> Vec<String> {
        let mut found = Vec::new();
        for word in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
            if word.len() > 2 && word.starts_with("--") && !found.iter().any(|f| f == word) {
                found.push(word.to_owned());
            }
        }
        found
    }

    #[test]
    fn every_public_flag_is_accepted_and_described_in_help() {
        let help = help();
        for (flag, line) in PUBLIC {
            let args = line.iter().map(|arg| (*arg).to_owned());
            assert!(Cli::parse(args).is_ok(), "{flag}: {line:?} is not accepted");
            assert!(
                flags_in(help).iter().any(|f| f == flag),
                "{flag} is missing from --help"
            );
        }
    }

    #[test]
    fn help_names_no_flag_that_is_not_public() {
        for flag in flags_in(help()) {
            assert!(
                PUBLIC.iter().any(|(known, _)| *known == flag),
                "{flag} appears in --help but is not a public flag"
            );
        }
        assert!(!help().contains("-child") && !help().contains("test-after-apply"));
    }

    #[test]
    fn help_fits_a_hundred_columns() {
        for line in help().lines() {
            assert!(
                line.chars().count() <= 100,
                "{} columns: {line}",
                line.chars().count()
            );
        }
    }
}
