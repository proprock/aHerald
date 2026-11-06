//! The operations that end in a card: the hooks and `--notify` of the agents, `--event`, the
//! `--demo` preview and the start of the resident itself.
use super::{child, config_path, executable, log_code};
use aherald::{
    cli::{Cli, Operation},
    config::{Config, Mode},
    events::Event,
    operational_log::Record,
};
use std::path::Path;

/// The resident scope and the configuration, loaded in the order a hook needs: a failure of
/// either is a fixed operational code, and an interactive start (no operation) also says why on a card.
fn connect(cli: &Cli, path: &Path) -> Result<(aherald::local::Scope, Config), String> {
    let scope = aherald::local::Scope::current("resident").inspect_err(|_| {
        if matches!(cli.operation, Some(Operation::Hook | Operation::Notify)) {
            log_code(path, Record::HookDeliveryFailed);
        }
    })?;
    let config = Config::load(path, cli.config.is_some()).inspect_err(|error| {
        log_code(path, Record::InvalidConfiguration);
        // An interactive start has no terminal to read stderr in: say why on a card.
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        if cli.operation.is_none()
            && aherald::notification::startup_notice(&aherald::notice::startup_failure_text(error))
                .is_err()
        {
            log_code(path, Record::RenderFailure);
        }
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        let _ = error;
    })?;
    Ok((scope, config))
}

fn warn_about_input_monitoring(config: &Config) {
    if config.codex.input_monitoring {
        eprintln!(
            "aHerald: Codex input monitoring unsupported for unrelated CLI/app sessions: no verified stable observation source; lifecycle hooks remain available"
        );
    }
}

/// No operation: run the resident, or with `--transient` stay silent unless there is an event.
pub fn start(cli: &Cli) -> Result<(), String> {
    let path = config_path(cli)?;
    let (scope, config) = connect(cli, &path)?;
    warn_about_input_monitoring(&config);
    if cli.selected_mode(&config) == Mode::Resident {
        return child::run_resident(scope, config, &path, cli.test_after_apply).inspect_err(
            |error| {
                // No terminal to read stderr in: say why the resident could not run on a card.
                #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
                if aherald::notification::startup_notice(&aherald::notice::run_failure_text(error))
                    .is_err()
                {
                    log_code(&path, Record::RenderFailure);
                }
                #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
                let _ = error;
            },
        );
    }
    Err("specify --event EVENT, --hook or --notify; see --help".into())
}

/// `--demo [SECONDS]`: loop a configuration preview.
pub fn demo(cli: &Cli) -> Result<(), String> {
    let path = config_path(cli)?;
    let (_, config) = connect(cli, &path)?;
    let pause_sec = cli.demo_pause_sec.ok_or("missing demo pause")?;
    aherald::resident::run_demo(config, &path, pause_sec)
}

/// `--event KIND`: send a sample event, to check the configuration.
pub fn event(cli: &Cli) -> Result<(), String> {
    let path = config_path(cli)?;
    let (_, config) = connect(cli, &path)?;
    warn_about_input_monitoring(&config);
    let name = cli
        .event
        .as_deref()
        .ok_or("specify --event EVENT, --hook or --notify; see --help")?;
    dispatch(
        &Event::demo(name)?,
        &config,
        &path,
        cli.selected_mode(&config),
    )
}

/// `--notify HARNESS --kind KIND`: the harness or its bridge names the kind; stdin only adds
/// session and folder.
pub fn notify(cli: &Cli) -> Result<(), String> {
    let path = config_path(cli)?;
    let (Some(harness), Some(kind)) = (cli.notify.as_deref(), cli.kind) else {
        return Err("--notify requires --kind".into());
    };
    let input = aherald::events::read_bounded(std::io::stdin(), aherald::events::INPUT_TIMEOUT)
        .unwrap_or_default();
    let event = aherald::events::external_event(harness, kind, cli.activity, cli.ended, &input);
    let (_, config) = connect(cli, &path)?;
    warn_about_input_monitoring(&config);
    dispatch(&event, &config, &path, cli.selected_mode(&config))
}

/// `--hook [--agent claude|codex]`: one hook payload on stdin.
pub fn hook(cli: &Cli) -> Result<(), String> {
    let path = config_path(cli)?;
    // Respond to the hook contract before any fallible configuration or delivery.
    let agent = cli.hook_agent();
    let mut hook_context = None;
    let mut hook_busy = false;
    let hook_event =
        aherald::events::read_bounded(std::io::stdin(), aherald::events::INPUT_TIMEOUT)
            .and_then(|bytes| {
                if agent == aherald::agent::Agent::Codex {
                    hook_context = aherald::codex_context::parse(&bytes)?;
                }
                let event = agent.parse_hook(&bytes)?;
                hook_busy = event.as_ref().is_some_and(aherald::agent::is_claude_stop)
                    && aherald::agent::claude_has_background_work(&bytes);
                Ok(event)
            })
            .inspect_err(|error| {
                log_code(
                    &path,
                    Record::HookRejected(aherald::events::hook_error_code(error)),
                )
            })?;
    if let Some(reply) = hook_event.as_ref().and_then(|e| agent.hook_reply(e)) {
        println!("{reply}");
    }
    let (scope, config) = connect(cli, &path)?;
    warn_about_input_monitoring(&config);
    let mode = cli.selected_mode(&config);
    if let Some(report) = hook_context {
        if agent.hooks_enabled(&config)
            && config.codex.warnings.context_threshold().is_some()
            && mode == Mode::Resident
        {
            // Observation only: do not start a resident or fail the model request.
            let _ = aherald::ipc::pipe::request_if_running(
                &scope,
                &aherald::ipc::Request::CodexContext { report },
                std::time::Duration::from_millis(150),
            );
        }
        return Ok(());
    }
    if let Some(mut event) = hook_event
        && agent.hooks_enabled(&config)
    {
        // A Stop with background work is no finished turn: it only settles the session's
        // waits (as activity does), with no card and no sound.
        if hook_busy && config.claude.suppress_while_busy {
            event.activity = true;
            return match mode {
                Mode::Resident => aherald::resident::forward_if_running(
                    &aherald::local::Scope::current("resident")?,
                    &event,
                ),
                _ => Ok(()),
            };
        }
        if agent.records_console_window() || cfg!(target_os = "linux") {
            event.window = aherald::console_window::current();
        }
        // One `ps` per hook, so only when tab detection is switched on.
        #[cfg(target_os = "macos")]
        if config.general.detect_terminal_tab && event.window.is_some() {
            event.tty = aherald::mac::console_window::tty();
        }
        dispatch(&event, &config, &path, mode)?;
    }
    Ok(())
}

fn dispatch(event: &Event, config: &Config, path: &Path, mode: Mode) -> Result<(), String> {
    if event.ended || event.activity {
        let idle_enabled = match event.source.agent() {
            Some(aherald::agent::Agent::Claude) => {
                config.claude.warnings.idle_threshold().is_some()
            }
            Some(aherald::agent::Agent::Codex) => config.codex.warnings.idle_threshold().is_some(),
            // An external harness reports its waits' answers as activity: always forward it.
            None => matches!(event.source, aherald::events::Source::External(_)),
        };
        if event.activity && !idle_enabled {
            return Ok(());
        }
        // Only a resident holds waits to clear; a transient card has no later session state.
        return match mode {
            Mode::Resident => aherald::resident::forward_if_running(
                &aherald::local::Scope::current("resident")?,
                event,
            ),
            _ => Ok(()),
        };
    }
    let setting = &config.events[event.kind];
    if !setting.enabled && !(mode == Mode::Resident && event.is_terminal_evidence()) {
        return Ok(());
    }
    let executable = executable()?;
    if mode == Mode::Resident {
        return aherald::local::Scope::current("resident")
            .and_then(|scope| aherald::resident::forward(&executable, path, &scope, event))
            .inspect_err(|_| log_code(path, Record::HookDeliveryFailed));
    }
    if !config.notifications.enabled && (!config.sounds.enabled || !setting.sound_enabled) {
        return Ok(());
    }
    let _child = aherald::process::deliver(&executable, path, event)?;
    Ok(())
}
