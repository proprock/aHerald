//! Stand-ins for the resident and console window on Unix systems other than macOS and Linux,
//! so the crate still builds there.
//!
//! Every function keeps the signature of its Windows twin and reports `UNSUPPORTED`
//! (or "nothing found") instead of panicking. Windows, macOS (`src/mac/`) and Linux
//! (`src/linux/`) have real implementations; IPC, process spawning, HTTPS and the opener are
//! shared by every Unix system (`src/unix/`).

#[cfg(all(not(target_os = "macos"), not(target_os = "linux")))]
pub const UNSUPPORTED: &str = "this platform is not supported yet (Windows only)";

#[cfg(all(not(target_os = "macos"), not(target_os = "linux")))]
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

#[cfg(all(not(target_os = "macos"), not(target_os = "linux")))]
pub mod console_window {
    /// No console window is recorded off Windows.
    pub fn current() -> Option<u64> {
        None
    }
}
