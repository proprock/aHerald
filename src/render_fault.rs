//! What the three residents share when a card cannot be drawn. A card that cannot be drawn
//! never ends the resident: it hides the window, keeps the queue and retries on the next wake.
use std::path::Path;

/// Debug builds only: lets native tests inject a render failure by writing its name to
/// `aherald.fault` in the diagnostic directory (isolated per resident instance).
#[cfg(debug_assertions)]
pub fn injected(directory: &Path, name: &str) -> bool {
    std::fs::read_to_string(directory.join("aherald.fault")).is_ok_and(|v| v.trim() == name)
}
#[cfg(not(debug_assertions))]
pub fn injected(_: &Path, _: &str) -> bool {
    false
}
