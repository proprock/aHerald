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
    let config = aherald::config::Config::load(&path, cli.config.is_some())?;
    if matches!(cli.operation.as_deref(), Some("status" | "quit"))
        || cli.selected_mode(&config) == "resident"
    {
        return Err("resident operations are not implemented until Milestone 2".into());
    }
    if config.codex.input_monitoring || config.usage.enabled {
        return Err("Codex monitoring and usage are not implemented until later milestones".into());
    }
    Err("notification delivery is not implemented yet".into())
}
