use crate::{agent::Agent, config::Mode};
use std::path::PathBuf;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Cli {
    pub operation: Option<String>,
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
    /// Hook file to use instead of the agent's default one.
    pub hooks_file: Option<PathBuf>,
}

impl Cli {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut cli = Self::default();
        let mut args = args.into_iter().peekable();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--demo" => {
                    if cli.operation.replace("demo".into()).is_some() {
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
                "--hook" | "--status" | "--quit" | "--help" | "--version" | "--perf-report" => {
                    if cli.operation.replace(arg[2..].to_owned()).is_some() {
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
                "--event" => {
                    if cli.operation.replace("event".into()).is_some() {
                        return Err("conflicting explicit operations".into());
                    }
                    let event = args.next().ok_or("--event requires a value")?;
                    if crate::events::Kind::from_name(&event).is_none() {
                        return Err("unknown event".into());
                    }
                    cli.event = Some(event);
                }
                "--install-hooks" | "--uninstall-hooks" => {
                    if cli.operation.replace(arg[2..].to_owned()).is_some() {
                        return Err("conflicting explicit operations".into());
                    }
                    let name = args.next().ok_or("--install-hooks requires an agent")?;
                    cli.hooks_agent = Some(Agent::from_name(&name).ok_or("unknown agent")?);
                }
                "--apply" => cli.apply = true,
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
            cli.operation.as_deref(),
            Some("status" | "quit" | "help" | "version" | "demo")
        ) && cli.mode.is_some()
        {
            return Err("operation does not accept a mode".into());
        }
        let setup = matches!(
            cli.operation.as_deref(),
            Some("install-hooks" | "uninstall-hooks")
        );
        if setup && (cli.mode.is_some() || cli.agent.is_some()) {
            return Err("hook setup does not accept a mode or --agent".into());
        }
        if (cli.apply || cli.hooks_file.is_some()) && !setup {
            return Err(
                "--apply and --hooks-file are only valid with --install-hooks or --uninstall-hooks"
                    .into(),
            );
        }
        if cli.agent.is_some() && cli.operation.as_deref() != Some("hook") {
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
