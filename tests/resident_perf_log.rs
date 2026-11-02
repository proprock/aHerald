#![cfg(windows)]
//! A resident at `logging = "debug"` explains where its time goes: aggregated span lines
//! for the work it did, and nothing derived from the events themselves.
use aherald::{
    events::Event,
    ipc::{Request, Response, pipe},
    local::Scope,
};
use std::time::Duration;

mod support;
use support::{ResidentFixture, wait_until};

fn start(level: &str) -> ResidentFixture {
    // Short cards let the animation timer stop and flush its statistics.
    let mut fixture = ResidentFixture::new(
        "resident-perf",
        &format!(
            "[general]\nlogging='{level}'\n[sounds]\nenabled=false\n[notifications]\nfade_in_ms=0\nfade_out_ms=0\n[events.permission_required]\nduration_sec=1\n"
        ),
    );
    fixture.start().unwrap();
    fixture
}

fn send(scope: &Scope, session: &str) {
    let mut event = Event::demo("permission_required").unwrap();
    event.session_id = Some(session.into());
    pipe::request(scope, &Request::Event { event }, Duration::from_millis(500)).unwrap();
}

fn wait_for(root: &std::path::Path, needle: &str) -> String {
    let mut text = String::new();
    wait_until(
        Duration::from_secs(5),
        Duration::from_millis(50),
        "resident performance log",
        || {
            text = std::fs::read_to_string(root.join("aherald.log")).unwrap_or_default();
            text.contains(needle)
        },
    )
    .unwrap_or_else(|error| panic!("{error}: missing `{needle}`\n{text}"));
    text
}

#[test]
fn a_debug_resident_logs_span_statistics_for_the_work_it_did() {
    let mut fixture = start("debug");
    let root = fixture.root.to_path_buf();
    send(&fixture.scope, "perf-session-one");
    send(&fixture.scope, "perf-session-two");
    // The listener thread flushes IPC statistics when it stops; the GUI thread flushes
    // its render statistics once the cards have gone and its timer stops.
    wait_for(&root, " perf frame.render n=");
    fixture.shutdown().unwrap();
    let text = wait_for(&root, " perf ipc.handle n=");
    for line in text.lines() {
        // `<timestamp> <LEVEL> <message>`: every line is well formed and carries only
        // fixed names and numbers.
        let message = line.splitn(3, ' ').nth(2).unwrap_or_default();
        assert!(
            !line.contains("perf-session") && !line.contains(&root.to_string_lossy().to_string()),
            "event data leaked into the log: {line}"
        );
        if let Some(rest) = message.strip_prefix("perf ") {
            let (name, fields) = rest.split_once(' ').unwrap();
            assert!(name.contains('.'), "{line}");
            for field in fields.split(' ') {
                let (key, value) = field.split_once('=').unwrap();
                assert!(
                    ["n", "avg", "sd", "min", "p95", "max"].contains(&key),
                    "{line}"
                );
                assert!(value.chars().next().unwrap().is_ascii_digit(), "{line}");
            }
        }
    }
    assert!(text.contains(" DEBUG perf placement.compute n="), "{text}");
}

#[test]
fn a_default_resident_writes_no_performance_lines() {
    let mut fixture = start("warn");
    let root = fixture.root.to_path_buf();
    send(&fixture.scope, "quiet-session");
    wait_until(
        Duration::from_secs(5),
        Duration::from_millis(25),
        "quiet resident queue drained",
        || {
            matches!(
                pipe::request(&fixture.scope, &Request::Status, Duration::from_millis(200)),
                Ok(Response::Status { queued: 0, .. })
            )
        },
    )
    .unwrap();
    fixture.shutdown().unwrap();
    let text = std::fs::read_to_string(root.join("aherald.log")).unwrap_or_default();
    assert!(!text.contains(" perf "), "{text}");
    assert!(!text.contains("DEBUG"), "{text}");
}
