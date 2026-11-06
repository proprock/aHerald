//! `--statusline`: Claude Code's `statusLine` command.
use aherald::cli::Cli;

/// Resolves the command to keep (`--then`, or on Windows the one in the setup receipt) and runs
/// the status line.
pub fn run(cli: &Cli) -> Result<(), String> {
    // The Windows wrapper's command line carries no `--then`: the user's own command is in
    // the setup receipt (AH-211). A missing receipt means there is none; an unreadable one
    // must not fail the status line, so say so on stderr and print the plain line.
    #[cfg(windows)]
    let from_receipt = match cli.then {
        Some(_) => None,
        None => super::executable()
            .and_then(|exe| aherald::setup::wrapped_statusline_command(&exe))
            .unwrap_or_else(|error| {
                eprintln!("aHerald: {error}");
                None
            }),
    };
    #[cfg(not(windows))]
    let from_receipt: Option<String> = None;
    print_line(
        cli.then.as_deref().or(from_receipt.as_deref()),
        cli.config.as_deref(),
    )
}

/// Claude Code's `statusLine` command: tells a running resident the session name and quota, and
/// passes the payload to the user's own command (`--then`) so their status line is unchanged.
/// Without `--then` it prints `[claude.statusline] format`, if one is configured.
/// It never starts a resident and never fails the status line.
fn print_line(then: Option<&str>, config: Option<&std::path::Path>) -> Result<(), String> {
    use std::io::Write;
    let input =
        aherald::events::read_bounded(std::io::stdin(), std::time::Duration::from_millis(500))
            .unwrap_or_default();
    // The configuration is read only when there is no command of the user's to run, so wrapping
    // costs nothing extra. It is never created here; a missing file leaves the line empty and an
    // unreadable one is named on it.
    let mut failure = None;
    let format = match then {
        Some(_) => None,
        None => {
            let path = config
                .map(|path| Ok(path.to_path_buf()))
                .unwrap_or_else(aherald::config::Config::default_path)
                .ok()
                .filter(|path| path.exists());
            match path.map(|path| aherald::config::Config::load(&path, true)) {
                Some(Ok(config)) if !config.claude.statusline.format.is_empty() => Some((
                    config.claude.statusline.format.clone(),
                    aherald::statusline_format::Options {
                        display_ctx: config.claude.statusline.display_ctx,
                        display_usage: config.claude.statusline.display_usage,
                        color: config.claude.statusline.color,
                    },
                )),
                // The status line is where the user is looking: say what is wrong there.
                Some(Err(error)) => {
                    failure = Some(aherald::notice::startup_failure_text(&error));
                    None
                }
                _ => None,
            }
        }
    };
    // The payload itself goes along for `aherald --raw-output statusline`; the resident keeps
    // the latest one in memory only.
    let raw = aherald::usage_service::bounded_raw(&input);
    let mut outcome = aherald::statusline::run_with_format(
        &input,
        then,
        format
            .as_ref()
            .map(|(text, options)| (text.as_str(), *options)),
        |report| {
            if let Ok(scope) = aherald::local::Scope::current("resident") {
                let _ = aherald::ipc::pipe::request_if_running(
                    &scope,
                    &aherald::ipc::Request::StatusLine {
                        report,
                        raw: Some(raw),
                    },
                    std::time::Duration::from_millis(150),
                );
            }
        },
    );
    if let (true, Some(text)) = (outcome.stdout.is_empty(), failure) {
        outcome.stdout = format!("{text}\n").into_bytes();
    }
    let mut stdout = std::io::stdout();
    let _ = stdout.write_all(&outcome.stdout);
    let _ = stdout.flush();
    std::process::exit(outcome.code);
}
