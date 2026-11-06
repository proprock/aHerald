//! The program's own child processes: the resident that a launcher started and the process that
//! shows one card. Their command lines are internal and parsed by `Cli::parse`.
use super::log_code;
use aherald::{cli::Cli, operational_log::Record};
use std::path::Path;

/// `--resident-child SCOPE --config PATH`: run the resident for that scope.
pub fn resident(cli: &Cli) -> Result<(), String> {
    let scope_name = cli
        .child_scope
        .as_deref()
        .ok_or("invalid resident invocation")?;
    let path = cli.config.as_deref().ok_or("invalid resident invocation")?;
    let config = aherald::config::Config::load(path, true)
        .inspect_err(|_| log_code(path, Record::InvalidConfiguration))?;
    let scope = aherald::local::Scope::current(scope_name)?;
    run_resident(scope, config, path, cli.test_after_apply)
        .inspect_err(|_| log_code(path, Record::ResidentStartupFailure))
}

/// Runs the resident in this process; `test` shows the test cards after a restart.
pub fn run_resident(
    scope: aherald::local::Scope,
    config: aherald::config::Config,
    path: &Path,
    test: bool,
) -> Result<(), String> {
    #[cfg(any(windows, target_os = "linux", target_os = "macos"))]
    return aherald::resident::run_after_apply(scope, config, path, test);
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = test;
        aherald::resident::run(scope, config, path)
    }
}

/// `--display-child --config PATH`: show the normalized event on stdin as one card.
pub fn display(cli: &Cli) -> Result<(), String> {
    let path = cli.config.as_deref().ok_or("invalid internal invocation")?;
    let config = aherald::config::Config::load(path, true)?;
    let bytes = aherald::events::read_bounded(std::io::stdin(), aherald::events::INPUT_TIMEOUT)?;
    if bytes.len() > aherald::process::TRANSFER_LIMIT {
        return Err("notification transfer exceeds limit".into());
    }
    let event: aherald::events::Event =
        serde_json::from_slice(&bytes).map_err(|_| "invalid normalized notification")?;
    event.validate()?;
    aherald::notification::show(&event, &config)
}
