//! Small bounded waits shared by integration tests.
use std::time::{Duration, Instant};

/// Poll a boolean state, retaining a useful timeout diagnostic without printing payloads.
pub fn wait_until(
    timeout: Duration,
    interval: Duration,
    what: &str,
    mut ready: impl FnMut() -> bool,
) -> Result<(), String> {
    let start = Instant::now();
    loop {
        if ready() {
            return Ok(());
        }
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            return Err(format!(
                "timed out waiting for {what} after {elapsed:?}; last observation: false"
            ));
        }
        std::thread::sleep(interval.min(timeout.saturating_sub(elapsed)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_wait_accepts_a_delayed_change_and_reports_a_missing_effect() {
        let start = Instant::now();
        assert!(
            wait_until(
                Duration::from_secs(1),
                Duration::from_millis(5),
                "delayed state",
                || start.elapsed() >= Duration::from_millis(25)
            )
            .is_ok()
        );
        let error = wait_until(
            Duration::from_millis(25),
            Duration::from_millis(5),
            "missing state",
            || false,
        )
        .unwrap_err();
        assert!(
            error.contains("missing state") && error.contains("last observation: false"),
            "{error}"
        );
    }
}
