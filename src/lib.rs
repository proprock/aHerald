//! Native notification engine and its independently testable domain logic.

pub mod activation;
pub mod agent;
pub mod app_server;
pub mod bridge_setup;
pub mod card;
pub mod card_paint;
pub mod card_surface;
pub mod claude_usage;
pub mod cli;
mod codex_install;
pub mod command_sink;
pub mod config;
mod default_config;
pub mod demo;
pub mod focus;
pub mod forward;
mod http_common;
#[cfg(windows)]
pub mod https;
#[cfg(unix)]
#[path = "unix/https.rs"]
pub mod https;
pub mod session_source;
pub mod setup;
pub mod setup_selection;
pub mod sink;
pub mod statusline;
pub mod statusline_format;
pub mod statusline_launch;
pub(crate) mod time;
pub mod usage;
pub mod usage_service;
pub mod webhook;
#[cfg(unix)]
#[path = "unix/zone.rs"]
pub(crate) mod zone;

pub const NAME: &str = "aHerald";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Unix milliseconds when this binary was built (stamped by `build.rs`).
fn build_unix_ms() -> i64 {
    env!("AHERALD_BUILD_UNIX")
        .parse::<i64>()
        .unwrap_or(0)
        .saturating_mul(1000)
}

/// `2026-11-03 22:52 UTC`: the build time for `--version`.
fn build_stamp(built_ms: i64) -> String {
    let iso = operational_log::format_timestamp(built_ms, 0);
    format!("{} {} UTC", &iso[..10], &iso[11..16])
}

/// `Built 2d 3:05 ago`: how old this binary is, for the About card. A clock set before the
/// build reads as `just now`.
pub fn build_age_text() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            elapsed.as_millis().min(i64::MAX as u128) as i64
        });
    format!(
        "Built {}",
        card::age_text(now.saturating_sub(build_unix_ms()).max(0) as u64)
    )
}

/// The `--version` line.
pub fn version_line() -> String {
    format!("{NAME} {VERSION} (built {})", build_stamp(build_unix_ms()))
}

/// Text of the tray's About card: heading line, then the body in the usage-card format.
pub fn about_card_text() -> String {
    format!(
        "{NAME}\nVersion {VERSION}\n{}\nNotification companion for coding agents.\nMIT license.",
        build_age_text()
    )
}

#[cfg(test)]
mod build_stamp_tests {
    use super::*;

    #[test]
    fn the_build_time_is_utc_to_the_minute() {
        assert_eq!(build_stamp(0), "1970-01-01 00:00 UTC");
        assert_eq!(build_stamp(1_793_746_355_000), "2026-11-03 22:52 UTC");
    }

    #[test]
    fn the_version_line_and_about_text_carry_the_build_time() {
        assert!(version_line().starts_with(&format!("{NAME} {VERSION} (built 20")));
        assert!(about_card_text().contains("\nBuilt "));
        assert!(build_age_text().ends_with(" ago") || build_age_text().ends_with("just now"));
    }
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
pub(crate) mod quote;
pub mod reentry;
pub mod remote;
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
