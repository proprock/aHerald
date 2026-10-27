#![cfg(windows)]
use aherald::{events::Event, process::deliver};
use std::{
    path::PathBuf,
    process::Child,
    time::{Duration, Instant},
};
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn transient_delivery_does_not_wait_for_finite_or_indefinite_lifetime() {
    for duration in [5000, 0] {
        let path = std::env::temp_dir().join(format!(
            "aherald-delivery-{}-{duration}.toml",
            std::process::id()
        ));
        std::fs::write(
            &path,
            format!("[events.turn_complete]\nduration_ms={duration}"),
        )
        .unwrap();
        let start = Instant::now();
        let result = deliver(
            &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
            &path,
            &Event::demo("turn_complete").unwrap(),
        );

        let mut child = OwnedChild(result.unwrap());
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(child.0.try_wait().unwrap().is_none());
        std::fs::remove_file(&path).unwrap();
    }
}
#[test]
fn missing_executable_and_invalid_event_fail_safely() {
    assert!(
        deliver(
            &PathBuf::from("missing-ah-executable.exe"),
            &PathBuf::from("examples/config.toml"),
            &Event::demo("error").unwrap()
        )
        .is_err()
    );
    let mut event = Event::demo("error").unwrap();
    event.session_id = Some("x".repeat(257));
    assert!(
        deliver(
            &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
            &PathBuf::from("examples/config.toml"),
            &event
        )
        .is_err()
    );
}
