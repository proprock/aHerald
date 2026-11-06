//! The operations of the `aherald` executable, one module per kind. `main.rs` parses the command
//! line and dispatches; nothing here parses arguments.
pub mod child;
pub mod control;
pub mod deliver;
pub mod setup;
pub mod statusline;

use std::path::{Path, PathBuf};

/// The running executable, which hooks and setup bind to.
pub fn executable() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|_| "cannot locate aHerald executable".into())
}

/// `--config`, else the default configuration beside the executable.
pub fn config_path(cli: &aherald::cli::Cli) -> Result<PathBuf, String> {
    cli.config
        .clone()
        .map(Ok)
        .unwrap_or_else(aherald::config::Config::default_path)
}

/// Fixed operational code beside the configuration; the configuration itself may be
/// unreadable, so its log level cannot be consulted.
pub fn log_code(path: &Path, record: aherald::operational_log::Record) {
    let directory = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let package_directory = std::env::current_exe().ok().and_then(|exe| {
        aherald::paths::diagnostic_dir_with(&exe, path, |name| std::env::var_os(name)).ok()
    });
    aherald::operational_log::init(
        package_directory.as_deref().unwrap_or(directory),
        aherald::config::LogLevel::Warn,
    );
    aherald::operational_log::log(record);
}
