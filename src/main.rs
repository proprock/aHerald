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

fn run(args: Vec<String>) -> Result<(), String> {
    if args.first().map(String::as_str) == Some("--resident-child") {
        if args.len() != 4 || args[2] != "--config" {
            return Err("invalid resident invocation".into());
        }
        let path = std::path::Path::new(&args[3]);
        return aherald::resident::run(
            aherald::local::Scope::current(&args[1])?,
            aherald::config::Config::load(path, true)?,
            path,
        );
    }
    if args.first().map(String::as_str) == Some("--display-child") {
        if args.len() != 3 || args[1] != "--config" {
            return Err("invalid internal invocation".into());
        }
        let config = aherald::config::Config::load(std::path::Path::new(&args[2]), false)?;
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
    let config = aherald::config::Config::load(&path, cli.config.is_some())?;
    if cli.selected_mode(&config) == "resident" && cli.operation.is_none() {
        return aherald::resident::run(scope, config, &path);
    }
    if config.codex.input_monitoring || config.usage.enabled {
        eprintln!("aHerald: Codex monitoring and usage are not implemented until later milestones");
    }
    if cli.operation.as_deref() == Some("hook") {
        let bytes =
            aherald::events::read_bounded(std::io::stdin(), aherald::events::INPUT_TIMEOUT)?;
        if let Some(event) = aherald::events::parse_hook(&bytes)? {
            if event.kind == aherald::events::Kind::TurnComplete {
                println!("{{}}");
            }
            if config.codex.hooks_enabled {
                dispatch(&event, &config, &path, cli.selected_mode(&config))?;
            }
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
    mode: &str,
) -> Result<(), String> {
    let setting = &config.events[event.kind.name()];
    if !setting.enabled {
        return Ok(());
    }
    let executable = std::env::current_exe().map_err(|_| "cannot locate aHerald executable")?;
    if mode == "resident" {
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
