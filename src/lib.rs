//! Native notification engine and its independently testable domain logic.

pub mod app_server;
pub mod cli;
pub mod config;
pub mod usage;
pub mod usage_service;

pub const NAME: &str = "aHerald";

pub fn help() -> &'static str {
    "aHerald — native Windows notification companion\nUsage: aherald [--hook | --event EVENT | --status | --quit] [--resident | --transient] [--config PATH]\nUse --resident to run the local notification service; --status and --quit control it. Codex input monitoring: unsupported for unrelated sessions. Enable usage in resident configuration for read-only Codex quotas.\n"
}
pub mod animation;
pub mod audio;
#[cfg(windows)]
mod detached;
pub mod events;
pub mod notification;
pub mod pet;
pub mod placement;
pub mod process;

#[cfg(windows)]
pub mod local;

pub mod ipc;

#[cfg(windows)]
pub mod resident;

pub mod queue;

#[cfg(windows)]
pub mod tray;
