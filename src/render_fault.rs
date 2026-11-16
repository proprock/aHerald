//! What the three residents share when a card cannot be drawn. A card that cannot be drawn
//! never ends the resident: it hides the window, keeps the queue and retries on the next wake.
use crate::{config::Config, operational_log::Record};
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

/// One failure episode: the failed draws since the last one that succeeded.
#[derive(Debug, Default)]
pub struct Episode {
    failing: bool,
}

impl Episode {
    /// Records the outcome of one draw. True when this failure starts an episode, which is
    /// the one time it is logged; a successful draw ends the episode.
    pub fn record(&mut self, drawn: bool) -> bool {
        let was_failing = std::mem::replace(&mut self.failing, !drawn);
        !drawn && !was_failing
    }

    /// Whether the last draw failed.
    pub fn failing(&self) -> bool {
        self.failing
    }
}

/// Writes a fixed operational record to the resident's log; never payloads or error text.
pub fn log(config: &Config, record: Record) {
    crate::operational_log::init(&config.diagnostic_directory, config.general.logging);
    crate::operational_log::log(record);
}

#[cfg(test)]
mod tests {
    use super::Episode;

    #[test]
    fn a_failure_episode_is_reported_once_and_a_success_ends_it() {
        let mut episode = Episode::default();
        let outcomes: Vec<_> = [false, false, false, true, true, false, false]
            .into_iter()
            .map(|drawn| (episode.record(drawn), episode.failing()))
            .collect();
        assert_eq!(
            outcomes,
            [
                (true, true),
                (false, true),
                (false, true),
                (false, false),
                (false, false),
                (true, true),
                (false, true),
            ]
        );
    }
}
