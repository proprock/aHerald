//! macOS implementation of the native presentation layer (AH-062).
pub mod activation;
pub mod audio;
pub mod focus;
pub mod geometry;
pub mod image;
pub mod input;
pub mod menu;
pub mod panel;
pub mod resident;
pub mod setup_window;
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

    /// The terminal device of the hook's session, to tell tabs of one terminal apart.
    pub fn tty() -> Option<String> {
        super::terminal::current_tty()
    }
}
pub mod keychain;
