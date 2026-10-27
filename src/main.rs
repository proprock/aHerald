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
    let _ = aherald::operational_log::Log::new(directory, aherald::config::LogLevel::Warn)
        .record(record);
}

fn run(mut args: Vec<String>) -> Result<(), String> {
    if args.first().map(String::as_str) == Some("--restart-child") {
        let pid = args
            .get(1)
            .ok_or("missing restart parent")?
            .parse::<u32>()
            .map_err(|_| "invalid restart parent")?;
        aherald::restart::wait_for_parent(pid)?;
        args.drain(..2);
    }
    if args.first().map(String::as_str) == Some("--resident-child") {
        if args.len() != 4 || args[2] != "--config" {
            return Err("invalid resident invocation".into());
        }
        use aherald::operational_log::Record;
        let path = std::path::Path::new(&args[3]);
        let config = aherald::config::Config::load(path, true)
            .inspect_err(|_| log_code(path, Record::InvalidConfiguration))?;
        return aherald::resident::run(aherald::local::Scope::current(&args[1])?, config, path)
            .inspect_err(|_| log_code(path, Record::ResidentStartupFailure));
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
    let path = cli
        .config
        .clone()
        .map(Ok)
        .unwrap_or_else(aherald::config::Config::default_path)?;
    let scope = aherald::local::Scope::current("resident")?;
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
    let hook_event = if cli.operation.as_deref() == Some("hook") {
        let bytes =
            aherald::events::read_bounded(std::io::stdin(), aherald::events::INPUT_TIMEOUT)?;
        let event = cli.hook_agent().parse_hook(&bytes)?;
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
        return aherald::resident::run(scope, config, &path);
    }
    if cli.operation.as_deref() == Some("hook") {
        if let Some(mut event) = hook_event
            && cli.hook_agent().hooks_enabled(&config)
        {
            if cli.hook_agent().records_console_window() {
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
    if event.ended {
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
        return aherald::resident::forward(
            &executable,
            path,
            &aherald::local::Scope::current("resident")?,
            event,
        );
    }
    if !config.notifications.enabled && (!config.sounds.enabled || !setting.sound_enabled) {
        return Ok(());
    }
    let _child = aherald::process::deliver(&executable, path, event)?;
    Ok(())
}
