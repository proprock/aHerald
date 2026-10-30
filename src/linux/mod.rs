//! Native Linux presentation support.
pub mod activation;
pub mod audio;
pub use activation as console_window;
#[path = "../mac/input.rs"]
pub mod input;
pub mod resident;
mod surface;
mod tray;
pub mod x11;
