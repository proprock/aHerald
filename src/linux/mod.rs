//! Native Linux presentation support.
pub mod audio;
#[path = "../mac/input.rs"]
pub mod input;
pub mod resident;
mod surface;
mod tray;
pub mod x11;
