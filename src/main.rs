#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let hook = args.iter().any(|arg| arg == "--hook");
    if let Err(error) = run(args) {
        eprintln!("aHerald: {error}");
        if !hook {
            std::process::exit(2);
        }
    }
}

/// Fixed operational code beside the configuration; the configuration itself may be
/// unreadable, so its log level cannot be consulted.
fn log_code(path: &std::path::Path, record: aherald::operational_log::Record) {
    let directory = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    aherald::operational_log::init(directory, aherald::config::LogLevel::Warn);
    aherald::operational_log::log(record);
}

fn run(mut args: Vec<String>) -> Result<(), String> {
    #[cfg(windows)]
    let mut test_after_apply = false;
    if args.first().map(String::as_str) == Some("--restart-child") {
        let pid = args
            .get(1)
            .ok_or("missing restart parent")?
            .parse::<u32>()
            .map_err(|_| "invalid restart parent")?;
        aherald::restart::wait_for_parent(pid)?;
        args.drain(..2);
        #[cfg(windows)]
        if args.first().is_some_and(|a| a == "--test-after-apply") {
            test_after_apply = true;
            args.remove(0);
        }
    }
    if args.first().map(String::as_str) == Some("--resident-child") {
        if args.len() != 4 || args[2] != "--config" {
            return Err("invalid resident invocation".into());
        }
        use aherald::operational_log::Record;
        let path = std::path::Path::new(&args[3]);
        let config = aherald::config::Config::load(path, true)
            .inspect_err(|_| log_code(path, Record::InvalidConfiguration))?;
        #[cfg(windows)]
        let result = aherald::resident::run_after_apply(
            aherald::local::Scope::current(&args[1])?,
            config,
            path,
            test_after_apply,
        );
        #[cfg(not(windows))]
        let result =
            aherald::resident::run(aherald::local::Scope::current(&args[1])?, config, path);
        return result.inspect_err(|_| log_code(path, Record::ResidentStartupFailure));
    }
    if args.first().map(String::as_str) == Some("--display-child") {
        if args.len() != 3 || args[1] != "--config" {
            return Err("invalid internal invocation".into());
        }
        let config = aherald::config::Config::load(std::path::Path::new(&args[2]), true)?;
        let bytes =
            aherald::events::read_bounded(std::io::stdin(), aherald::events::INPUT_TIMEOUT)?;
        if bytes.len() > aherald::process::TRANSFER_LIMIT {
            return Err("notification transfer exceeds limit".into());
        }
        let event: aherald::events::Event =
            serde_json::from_slice(&bytes).map_err(|_| "invalid normalized notification")?;
        event.validate()?;
        return aherald::notification::show(&event, &config);
    }
    let cli = aherald::cli::Cli::parse(args)?;
    if cli.operation.as_deref() == Some("help") {
        println!("{}", aherald::help());
        return Ok(());
    }
    if cli.operation.as_deref() == Some("version") {
        println!("{}", aherald::version_line());
        return Ok(());
    }
    if cli.operation.as_deref() == Some("setup-settings") {
        let exe = std::env::current_exe().map_err(|_| "cannot locate aHerald executable")?;
        print!(
            "{}",
            aherald::setup::settings_state(&exe, cli.usage.file.as_deref())?
        );
        return Ok(());
    }
    if matches!(cli.operation.as_deref(), Some("setup" | "unsetup")) {
        let exe = std::env::current_exe().map_err(|_| "cannot locate aHerald executable")?;
        print!(
            "{}",
            aherald::setup::run_with_options(
                &exe,
                &cli.harnesses,
                cli.operation.as_deref() == Some("unsetup"),
                cli.apply,
                &cli.usage
            )?
        );
        return Ok(());
    }
    if cli.operation.as_deref() == Some("statusline") {
        return statusline(cli.then.as_deref());
    }
    if let Some(action) = match cli.operation.as_deref() {
        Some("install-statusline") => Some(aherald::hook_setup::Action::Install),
        Some("uninstall-statusline") => Some(aherald::hook_setup::Action::Uninstall),
        _ => None,
    } {
        if cfg!(windows) {
            return Err("--install-statusline is not supported on Windows yet".into());
        }
        let exe = std::env::current_exe().map_err(|_| "cannot locate aHerald executable")?;
        let file = match cli.hooks_file.clone() {
            Some(file) => file,
            None => aherald::hook_setup::default_file(
                aherald::agent::Agent::Claude,
                &aherald::paths::home_dir_with(|name| std::env::var_os(name))
                    .ok_or("cannot locate the home directory; use --hooks-file")?,
            ),
        };
        print!(
            "{}",
            aherald::hook_setup::run_statusline(
                action,
                &file,
                &aherald::hook_setup::statusline_command(&exe),
                cli.apply,
            )?
        );
        return Ok(());
    }
    if let Some(action) = match cli.operation.as_deref() {
        Some("install-hooks") => Some(aherald::hook_setup::Action::Install),
        Some("uninstall-hooks") => Some(aherald::hook_setup::Action::Uninstall),
        _ => None,
    } {
        let agent = cli.hooks_agent.ok_or("hook setup needs an agent")?;
        let exe = std::env::current_exe().map_err(|_| "cannot locate aHerald executable")?;
        let file = match cli.hooks_file.clone() {
            Some(file) => file,
            None => aherald::hook_setup::default_file(
                agent,
                &aherald::paths::home_dir_with(|name| std::env::var_os(name))
                    .ok_or("cannot locate the home directory; use --hooks-file")?,
            ),
        };
        let command = aherald::hook_setup::command_for(&exe, agent, cfg!(windows));
        print!(
            "{}",
            aherald::hook_setup::run(agent, action, &file, &command, cli.apply)?
        );
        return Ok(());
    }
    let path = cli
        .config
        .clone()
        .map(Ok)
        .unwrap_or_else(aherald::config::Config::default_path)?;
    if cli.operation.as_deref() == Some("perf-report") {
        let directory = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(std::path::Path::new("."));
        print!("{}", aherald::perf_report::report(directory)?);
        return Ok(());
    }
    let scope = aherald::local::Scope::current("resident").inspect_err(|_| {
        if cli.operation.as_deref() == Some("hook") {
            log_code(&path, aherald::operational_log::Record::HookDeliveryFailed);
        }
    })?;
    if matches!(cli.operation.as_deref(), Some("status" | "quit")) {
        let request = if cli.operation.as_deref() == Some("quit") {
            aherald::ipc::Request::Quit
        } else {
            aherald::ipc::Request::Status
        };
        match aherald::ipc::pipe::request(&scope, &request, std::time::Duration::from_millis(100)) {
            Ok(response) => println!(
                "{}",
                serde_json::to_string(&response).map_err(|_| "cannot encode status")?
            ),
            Err(_) => println!("{{\"running\":false}}"),
        }
        return Ok(());
    }
    // Respond to the hook contract before any fallible configuration or delivery.
    let mut hook_context = None;
    let hook_event = if cli.operation.as_deref() == Some("hook") {
        let event = aherald::events::read_bounded(std::io::stdin(), aherald::events::INPUT_TIMEOUT)
            .and_then(|bytes| {
                if cli.hook_agent() == aherald::agent::Agent::Codex {
                    hook_context = aherald::codex_context::parse(&bytes)?;
                }
                cli.hook_agent().parse_hook(&bytes)
            })
            .inspect_err(|error| {
                log_code(
                    &path,
                    aherald::operational_log::Record::HookRejected(
                        aherald::events::hook_error_code(error),
                    ),
                )
            })?;
        if let Some(reply) = event.as_ref().and_then(|e| cli.hook_agent().hook_reply(e)) {
            println!("{reply}");
        }
        event
    } else {
        None
    };
    let config = aherald::config::Config::load(&path, cli.config.is_some()).inspect_err(|_| {
        log_code(
            &path,
            aherald::operational_log::Record::InvalidConfiguration,
        )
    })?;
    if let Some(pause_sec) = cli.demo_pause_sec {
        return aherald::resident::run_demo(config, &path, pause_sec);
    }
    if config.codex.input_monitoring {
        eprintln!(
            "aHerald: Codex input monitoring unsupported for unrelated CLI/app sessions: no verified stable observation source; lifecycle hooks remain available"
        );
    }
    if cli.selected_mode(&config) == aherald::config::Mode::Resident && cli.operation.is_none() {
        #[cfg(windows)]
        return aherald::resident::run_after_apply(scope, config, &path, test_after_apply);
        #[cfg(not(windows))]
        return aherald::resident::run(scope, config, &path);
    }
    if cli.operation.as_deref() == Some("hook") {
        if let Some(report) = hook_context {
            if cli.hook_agent().hooks_enabled(&config)
                && config.codex.warnings.context_threshold().is_some()
                && cli.selected_mode(&config) == aherald::config::Mode::Resident
            {
                // Observation only: do not start a resident or fail the model request.
                let _ = aherald::ipc::pipe::request(
                    &scope,
                    &aherald::ipc::Request::CodexContext { report },
                    std::time::Duration::from_millis(150),
                );
            }
            return Ok(());
        }
        if let Some(mut event) = hook_event
            && cli.hook_agent().hooks_enabled(&config)
        {
            if cli.hook_agent().records_console_window() || cfg!(target_os = "linux") {
                event.window = aherald::console_window::current();
            }
            dispatch(&event, &config, &path, cli.selected_mode(&config))?;
        }
        return Ok(());
    }
    let name = cli
        .event
        .as_deref()
        .ok_or("specify --event EVENT or --hook; see --help")?;
    dispatch(
        &aherald::events::Event::demo(name)?,
        &config,
        &path,
        cli.selected_mode(&config),
    )
}

fn dispatch(
    event: &aherald::events::Event,
    config: &aherald::config::Config,
    path: &std::path::Path,
    mode: aherald::config::Mode,
) -> Result<(), String> {
    if event.ended || event.activity {
        let idle_enabled = match event.source.agent() {
            Some(aherald::agent::Agent::Claude) => {
                config.claude.warnings.idle_threshold().is_some()
            }
            Some(aherald::agent::Agent::Codex) => config.codex.warnings.idle_threshold().is_some(),
            None => false,
        };
        if event.activity && !idle_enabled {
            return Ok(());
        }
        // Only a resident holds waits to clear; a transient card has no later session state.
        return match mode {
            aherald::config::Mode::Resident => aherald::resident::forward_if_running(
                &aherald::local::Scope::current("resident")?,
                event,
            ),
            _ => Ok(()),
        };
    }
    let setting = &config.events[event.kind];
    if !setting.enabled
        && !(mode == aherald::config::Mode::Resident && event.is_terminal_evidence())
    {
        return Ok(());
    }
    let executable = std::env::current_exe().map_err(|_| "cannot locate aHerald executable")?;
    if mode == aherald::config::Mode::Resident {
        return aherald::local::Scope::current("resident")
            .and_then(|scope| aherald::resident::forward(&executable, path, &scope, event))
            .inspect_err(|_| log_code(path, aherald::operational_log::Record::HookDeliveryFailed));
    }
    if !config.notifications.enabled && (!config.sounds.enabled || !setting.sound_enabled) {
        return Ok(());
    }
    let _child = aherald::process::deliver(&executable, path, event)?;
    Ok(())
}

/// Claude Code's `statusLine` command: tells a running resident the session name and quota, and
/// passes the payload to the user's own command (`--then`) so their status line is unchanged.
/// It never starts a resident and never fails the status line.
fn statusline(then: Option<&str>) -> Result<(), String> {
    use std::io::Write;
    let input =
        aherald::events::read_bounded(std::io::stdin(), std::time::Duration::from_millis(500))
            .unwrap_or_default();
    let outcome = aherald::statusline::run(&input, then, |report| {
        if let Ok(scope) = aherald::local::Scope::current("resident") {
            let _ = aherald::ipc::pipe::request(
                &scope,
                &aherald::ipc::Request::StatusLine { report },
                std::time::Duration::from_millis(150),
            );
        }
    });
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(&outcome.stdout);
    let _ = stdout.flush();
    std::process::exit(outcome.code);
}
