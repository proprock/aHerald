//! Native notification engine and its independently testable domain logic.

pub mod activation;
pub mod agent;
pub mod app_server;
pub mod card;
pub mod card_paint;
pub mod card_surface;
pub mod claude_usage;
pub mod cli;
mod codex_install;
pub mod config;
mod default_config;
pub mod demo;
mod http_common;
#[cfg(windows)]
pub mod https;
#[cfg(unix)]
#[path = "unix/https.rs"]
pub mod https;
pub mod session_source;
pub mod setup;
pub mod statusline;
pub mod usage;
pub mod usage_service;
#[cfg(unix)]
#[path = "unix/zone.rs"]
pub(crate) mod zone;

pub const NAME: &str = "aHerald";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The `--version` line.
pub fn version_line() -> String {
    format!("{NAME} {VERSION}")
}

/// Text of the tray's About card: heading line, then the body in the usage-card format.
pub fn about_card_text() -> String {
    format!(
        "{NAME}\nVersion {VERSION}\nNotification companion for Codex and Claude Code.\nMIT license."
    )
}

pub fn help() -> &'static str {
    "aHerald — native notification companion (Windows, macOS, Linux X11/XWayland)\nUsage: aherald [--hook | --event EVENT | --demo [SECONDS] | --status | --quit | --perf-report | --setup --harness AGENT [--harness AGENT] | --unsetup | --install-hooks AGENT | --uninstall-hooks AGENT | --statusline [--then COMMAND] | --install-statusline | --uninstall-statusline | --version] [--resident | --transient] [--config PATH] [--apply] [--hooks-file PATH]\nWithout --config, use config.toml beside aherald.exe; create it with defaults if absent. Relative resources use the selected configuration directory.\nUse --demo for a looping configuration preview (3-second pauses); exit through its tray. Demo does not accept mode flags. Use --resident to run the local notification service; --status and --quit control it. --perf-report ranks the timings that a resident with general.logging = \"debug\" wrote to aherald.log. --setup --harness codex|claude configures the selected harnesses for this executable; --unsetup removes the saved bindings. Both are dry runs unless --apply. Native setup also accepts --codex-usage true|false and --claude-usage true|false to change only those flags, preserving other configuration. --setup-settings reports current usage flags without writing; --settings-file PATH selects an absolute target configuration for native setup. --install-hooks / --uninstall-hooks codex|claude add or remove only aHerald's entries in that agent's hook file; a dry run unless --apply, with a backup. --statusline is for Claude Code's statusLine setting: it tells a running resident the session name, quota and input context count and runs --then COMMAND unchanged. Optional per-agent [claude.warnings] / [codex.warnings] context_tokens and idle_minutes enable session warnings in resident mode; Codex context requires PostModelRequest (verified CLI 0.166.0-alpha.5.1). Codex input monitoring: unsupported for unrelated sessions. Enable usage in resident configuration for read-only Codex quotas.\n"
}
pub mod animation;
pub mod artwork;
pub mod audio;
#[cfg(windows)]
pub mod console_window;
#[cfg(windows)]
mod detached;
#[cfg(unix)]
#[path = "unix/detached.rs"]
mod detached;
pub mod events;
pub mod hook_setup;
pub mod notice;
pub mod notification;
pub mod operational_log;
pub mod paths;
pub mod perf;
pub mod perf_report;
pub mod pet;
pub mod placement;
pub mod presenter;
pub mod process;
#[cfg(windows)]
pub mod process_stats;
pub mod reentry;
pub mod resource_cache;

#[cfg(windows)]
pub mod local;
#[cfg(unix)]
#[path = "unix/local.rs"]
pub mod local;

pub mod ipc;

#[cfg(target_os = "macos")]
pub mod mac;

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(windows)]
pub mod resident;

pub mod queue;
mod resident_client;
pub mod resident_request;

#[cfg(windows)]
pub mod restart;
#[cfg(unix)]
#[path = "unix/restart.rs"]
pub mod restart;
#[cfg(windows)]
pub mod shell;
#[cfg(unix)]
#[path = "unix/shell.rs"]
pub mod shell;
#[cfg(windows)]
pub mod tray;

pub mod tray_usage;

#[cfg(not(windows))]
mod unsupported;
#[cfg(target_os = "linux")]
pub use linux::console_window;
#[cfg(target_os = "linux")]
pub use linux::resident;
#[cfg(target_os = "macos")]
pub use mac::console_window;
#[cfg(target_os = "macos")]
pub use mac::resident;
#[cfg(all(not(windows), not(target_os = "macos"), not(target_os = "linux")))]
pub use unsupported::console_window;
#[cfg(all(not(windows), not(target_os = "macos"), not(target_os = "linux")))]
pub use unsupported::resident;
pub mod codex_context;
pub mod session_warnings;
