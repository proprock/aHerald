//! Native notification engine and its independently testable domain logic.

pub mod cli;
pub mod config;

pub const NAME: &str = "aHerald";

pub fn help() -> &'static str {
    "aHerald — native Windows notification companion\nUsage: aherald [--hook | --event EVENT | --status | --quit] [--resident | --transient] [--config PATH]\nResident, status and quit: Milestone 2; Codex monitoring and usage: later milestones.\n"
}
pub mod animation;
pub mod audio;
pub mod events;
pub mod notification;
pub mod pet;
pub mod process;

#[cfg(windows)]
pub mod local;
