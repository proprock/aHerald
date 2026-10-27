//! Native notification engine and its independently testable domain logic.

pub mod activation;
pub mod agent;
pub mod app_server;
pub mod card;
pub mod claude_usage;
pub mod cli;
mod codex_install;
pub mod config;
mod default_config;
pub mod demo;
#[cfg(windows)]
pub mod https;
pub mod session_source;
pub mod usage;
pub mod usage_service;

pub const NAME: &str = "aHerald";

pub fn help() -> &'static str {
    "aHerald — native Windows notification companion\nUsage: aherald [--hook | --event EVENT | --demo [SECONDS] | --status | --quit] [--resident | --transient] [--config PATH]\nWithout --config, use config.toml beside aherald.exe; create it with defaults if absent. Relative resources use the selected configuration directory.\nUse --demo for a looping configuration preview (3-second pauses); exit through its tray. Demo does not accept mode flags. Use --resident to run the local notification service; --status and --quit control it. Codex input monitoring: unsupported for unrelated sessions. Enable usage in resident configuration for read-only Codex quotas.\n"
}
pub mod animation;
pub mod artwork;
pub mod audio;
#[cfg(windows)]
pub mod console_window;
#[cfg(windows)]
mod detached;
pub mod events;
pub mod notification;
pub mod operational_log;
pub mod pet;
pub mod placement;
pub mod process;
pub mod resource_cache;

#[cfg(windows)]
pub mod local;

pub mod ipc;

#[cfg(windows)]
pub mod resident;

pub mod queue;

#[cfg(windows)]
pub mod restart;
#[cfg(windows)]
pub mod shell;
#[cfg(windows)]
pub mod tray;

pub mod tray_usage;
