//! Stand-ins for the service layer that has no Unix implementation yet, so the crate builds
//! on macOS and Linux.
//!
//! Every function keeps the signature of its Windows twin and reports `UNSUPPORTED`
//! (or "nothing found") instead of panicking. IPC, process spawning, HTTPS and the opener
//! already have Unix implementations (`src/unix/`, AH-061); the resident service has a macOS
//! one (`src/mac/`, AH-062) and arrives on Linux with AH-068.

#[cfg(not(target_os = "macos"))]
pub const UNSUPPORTED: &str = "this platform is not supported yet (Windows only)";

#[cfg(not(target_os = "macos"))]
pub mod resident {
    use super::UNSUPPORTED;
    use crate::{config::Config, local::Scope};
    use std::path::Path;

    pub use crate::resident_client::{ensure, forward, forward_if_running};

    pub fn run_demo(_: Config, _: &Path, _: u64) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
    pub fn run(_: Scope, _: Config, _: &Path) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
}

#[cfg(not(target_os = "macos"))]
pub mod console_window {
    /// No console window is recorded off Windows.
    pub fn current() -> Option<u64> {
        None
    }
}
