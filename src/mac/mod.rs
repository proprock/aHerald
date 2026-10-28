//! macOS implementation of the native presentation layer (AH-062).
pub mod activation;
pub mod audio;
pub mod geometry;
pub mod image;
pub mod input;
pub mod menu;
pub mod panel;
pub mod resident;
pub mod surface;
pub mod terminal;
pub mod text;
pub mod tray;

/// The hook's terminal for click-to-activate, as on Windows (`console_window`).
pub mod console_window {
    /// The process id of the terminal application the hook runs under.
    pub fn current() -> Option<u64> {
        super::terminal::current()
    }
}
