#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // A hook-like command never fails its harness: its problems go to stderr only.
    let hook = args.iter().any(|arg| arg == "--hook" || arg == "--notify");
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
    let package_directory = std::env::current_exe().ok().and_then(|exe| {
        aherald::paths::diagnostic_dir_with(&exe, path, |name| std::env::var_os(name)).ok()
    });
    aherald::operational_log::init(
        package_directory.as_deref().unwrap_or(directory),
        aherald::config::LogLevel::Warn,
    );
    aherald::operational_log::log(record);
}

fn run(mut args: Vec<String>) -> Result<(), String> {
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    let mut test_after_apply = false;
    if args.first().map(String::as_str) == Some("--restart-child") {
        let pid = args
            .get(1)
            .ok_or("missing restart parent")?
            .parse::<u32>()
            .map_err(|_| "invalid restart parent")?;
        aherald::restart::wait_for_parent(pid)?;
        args.drain(..2);
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
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
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        let result = aherald::resident::run_after_apply(
            aherald::local::Scope::current(&args[1])?,
            config,
            path,
            test_after_apply,
        );
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
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
    #[cfg(target_os = "macos")]
    if args.is_empty()
        && let Ok(exe) = std::env::current_exe()
        && aherald::setup_selection::launch_opens_setup(&args, &exe)
    {
        return aherald::mac::setup_window::run();
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
    if cli.operation.as_deref() == Some("console-setup") {
        return console_setup();
    }
    if cli.operation.as_deref() == Some("setup-window") {
        #[cfg(target_os = "macos")]
        return aherald::mac::setup_window::run();
        #[cfg(not(target_os = "macos"))]
        return Err("--setup-window is available on macOS only".into());
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
        return statusline(cli.then.as_deref(), cli.config.as_deref());
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
        let exe = std::env::current_exe().map_err(|_| "cannot locate aHerald executable")?;
        let directory =
            aherald::paths::diagnostic_dir_with(&exe, &path, |key| std::env::var_os(key))?;
        print!("{}", aherald::perf_report::report(&directory)?);
        return Ok(());
    }
    if matches!(cli.operation.as_deref(), Some("status" | "quit")) {
        let scope = aherald::local::Scope::current("resident")?;
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
    // `--notify`: the harness or its bridge names the kind; stdin only adds session and folder.
    let notify_event = match (cli.operation.as_deref(), cli.notify.as_deref(), cli.kind) {
        (Some("notify"), Some(harness), Some(kind)) => {
            let input =
                aherald::events::read_bounded(std::io::stdin(), aherald::events::INPUT_TIMEOUT)
                    .unwrap_or_default();
            Some(aherald::events::external_event(
                harness,
                kind,
                cli.activity,
                cli.ended,
                &input,
            ))
        }
        _ => None,
    };
    let scope = aherald::local::Scope::current("resident").inspect_err(|_| {
        if matches!(cli.operation.as_deref(), Some("hook" | "notify")) {
            log_code(&path, aherald::operational_log::Record::HookDeliveryFailed);
        }
    })?;
    let config =
        aherald::config::Config::load(&path, cli.config.is_some()).inspect_err(|error| {
            log_code(
                &path,
                aherald::operational_log::Record::InvalidConfiguration,
            );
            // An interactive start has no terminal to read stderr in: say why on a card.
            #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
            if cli.operation.is_none()
                && aherald::notification::configuration_notice(error).is_err()
            {
                log_code(&path, aherald::operational_log::Record::RenderFailure);
            }
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
        #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
        return aherald::resident::run_after_apply(scope, config, &path, test_after_apply);
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        return aherald::resident::run(scope, config, &path);
    }
    if cli.operation.as_deref() == Some("hook") {
        if let Some(report) = hook_context {
            if cli.hook_agent().hooks_enabled(&config)
                && config.codex.warnings.context_threshold().is_some()
                && cli.selected_mode(&config) == aherald::config::Mode::Resident
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
            && cli.hook_agent().hooks_enabled(&config)
        {
            if cli.hook_agent().records_console_window() || cfg!(target_os = "linux") {
                event.window = aherald::console_window::current();
            }
            // One `ps` per hook, so only when tab detection is switched on.
            #[cfg(target_os = "macos")]
            if config.general.detect_terminal_tab && event.window.is_some() {
                event.tty = aherald::mac::console_window::tty();
            }
            dispatch(&event, &config, &path, cli.selected_mode(&config))?;
        }
        return Ok(());
    }
    if let Some(event) = notify_event {
        return dispatch(&event, &config, &path, cli.selected_mode(&config));
    }
    let name = cli
        .event
        .as_deref()
        .ok_or("specify --event EVENT, --hook or --notify; see --help")?;
    dispatch(
        &aherald::events::Event::demo(name)?,
        &config,
        &path,
        cli.selected_mode(&config),
    )
}

fn console_setup() -> Result<(), String> {
    use std::io::{IsTerminal, Write};
    if !cfg!(target_os = "linux") {
        return Err("--console-setup is available on Linux only".into());
    }
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err("--console-setup requires an interactive terminal".into());
    }
    fn choice(prompt: &str, default: bool) -> Result<bool, String> {
        loop {
            print!("{prompt} {} ", if default { "[Y/n]" } else { "[y/N]" });
            std::io::stdout().flush().map_err(|e| e.to_string())?;
            let mut line = String::new();
            if std::io::stdin()
                .read_line(&mut line)
                .map_err(|e| e.to_string())?
                == 0
            {
                return Err("setup cancelled: input closed".into());
            }
            match line.trim().to_ascii_lowercase().as_str() {
                "" => return Ok(default),
                "y" | "yes" => return Ok(true),
                "n" | "no" => return Ok(false),
                _ => eprintln!("Enter y or n."),
            }
        }
    }
    let exe = std::env::current_exe().map_err(|_| "cannot locate aHerald executable")?;
    let previous = aherald::setup::selected_harnesses(&exe)?;
    let settings = aherald::setup::settings_state(&exe, None)?;
    let defaults = aherald::setup_selection::Selection::initial(&previous, &settings);
    println!("Select harness integrations. Existing user settings and assets are preserved.");
    let mut selection = defaults;
    selection.codex = choice("Configure Codex hooks?", defaults.codex)?;
    selection.claude = choice("Configure Claude Code hooks?", defaults.claude)?;
    println!(
        "Usage monitoring is optional. Claude auto tries the login token, then the last statusLine report; existing source settings are preserved."
    );
    selection.codex_usage = choice("Enable Codex usage monitoring?", defaults.codex_usage)?;
    selection.claude_usage = choice("Enable Claude usage monitoring?", defaults.claude_usage)?;
    // Warning choices apply to a selected agent only; an unselected agent keeps its settings.
    println!(
        "Session warnings are optional. A context warning needs a resident and, for Claude Code, the statusLine wrapper: an existing status line is kept and still shown, and without one Claude shows the [claude.statusline] format (empty by default). Existing thresholds are preserved."
    );
    if selection.codex {
        selection.codex_context = choice("Enable Codex context warning?", defaults.codex_context)?;
        selection.codex_idle = choice("Enable Codex idle warning?", defaults.codex_idle)?;
    }
    if selection.claude {
        selection.claude_context =
            choice("Enable Claude context warning?", defaults.claude_context)?;
        selection.claude_idle = choice("Enable Claude idle warning?", defaults.claude_idle)?;
    }
    let harnesses = selection.harnesses();
    let options = selection.options();
    let remove = selection.remove();
    print!(
        "{}",
        aherald::setup::run_with_options(&exe, &harnesses, remove, false, &options)?
    );
    if !choice("Apply these changes?", false)? {
        println!("No changes applied.");
        return Ok(());
    }
    print!(
        "{}",
        aherald::setup::run_with_options(&exe, &harnesses, remove, true, &options)?
    );
    if choice("Launch aHerald now?", true)? {
        std::process::Command::new(exe)
            .arg("--resident")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("cannot launch aHerald: {e}"))?;
    }
    Ok(())
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
            // An external harness reports its waits' answers as activity: always forward it.
            None => matches!(event.source, aherald::events::Source::External(_)),
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
/// Without `--then` it prints `[claude.statusline] format`, if one is configured.
/// It never starts a resident and never fails the status line.
fn statusline(then: Option<&str>, config: Option<&std::path::Path>) -> Result<(), String> {
    use std::io::Write;
    let input =
        aherald::events::read_bounded(std::io::stdin(), std::time::Duration::from_millis(500))
            .unwrap_or_default();
    // The configuration is read only when there is no command of the user's to run, so wrapping
    // costs nothing extra. It is never created here, and any problem leaves the line empty.
    let format = match then {
        Some(_) => None,
        None => config
            .map(|path| Ok(path.to_path_buf()))
            .unwrap_or_else(aherald::config::Config::default_path)
            .ok()
            .and_then(|path| aherald::config::Config::load(&path, true).ok())
            .filter(|config| !config.claude.statusline.format.is_empty())
            .map(|config| {
                (
                    config.claude.statusline.format.clone(),
                    aherald::statusline_format::Options {
                        display_ctx: config.claude.statusline.display_ctx,
                        display_usage: config.claude.statusline.display_usage,
                        color: config.claude.statusline.color,
                    },
                )
            }),
    };
    let outcome = aherald::statusline::run_with_format(
        &input,
        then,
        format
            .as_ref()
            .map(|(text, options)| (text.as_str(), *options)),
        |report| {
            if let Ok(scope) = aherald::local::Scope::current("resident") {
                let _ = aherald::ipc::pipe::request_if_running(
                    &scope,
                    &aherald::ipc::Request::StatusLine { report },
                    std::time::Duration::from_millis(150),
                );
            }
        },
    );
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(&outcome.stdout);
    let _ = stdout.flush();
    std::process::exit(outcome.code);
}
