//! Stand-ins for the Windows-only service layer so the crate builds on macOS and Linux.
//!
//! Every function keeps the signature of its Windows twin and reports `UNSUPPORTED`
//! (or "nothing found") instead of panicking. The Unix implementations replace these
//! modules one at a time (AH-061 and later); hook parsing, configuration and the
//! notification domain logic already run on every platform.

pub const UNSUPPORTED: &str = "this platform is not supported yet (Windows only)";

pub mod local {
    /// Names the per-user service instance. Nothing is created off Windows.
    #[derive(Clone, Debug)]
    pub struct Scope {
        pub namespace: String,
    }
    impl Scope {
        pub fn current(namespace: &str) -> Result<Self, String> {
            Ok(Self {
                namespace: namespace.into(),
            })
        }
    }
}

pub mod pipe {
    use super::{UNSUPPORTED, local::Scope};
    use crate::ipc::{Request, Response};
    use std::time::Duration;

    pub fn request(_: &Scope, _: &Request, _: Duration) -> Result<Response, String> {
        Err(UNSUPPORTED.into())
    }
}

pub mod resident {
    use super::{UNSUPPORTED, local::Scope};
    use crate::{config::Config, events::Event};
    use std::path::Path;

    pub fn forward(_: &Path, _: &Path, _: &Scope, _: &Event) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
    pub fn forward_if_running(_: &Scope, _: &Event) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
    pub fn run_demo(_: Config, _: &Path, _: u64) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
    pub fn run(_: Scope, _: Config, _: &Path) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
}

pub mod restart {
    use super::UNSUPPORTED;

    pub fn wait_for_parent(_: u32) -> Result<(), String> {
        Err(UNSUPPORTED.into())
    }
}

pub mod console_window {
    /// No console window is recorded off Windows.
    pub fn current() -> Option<u64> {
        None
    }
}
