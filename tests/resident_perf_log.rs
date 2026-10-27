#![cfg(windows)]
//! A resident at `logging = "debug"` explains where its time goes: aggregated span lines
//! for the work it did, and nothing derived from the events themselves.
use aherald::{
    events::Event,
    ipc::{Request, Response, pipe},
    local::Scope,
    resident,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

struct Cleanup(Scope, PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = pipe::request(&self.0, &Request::Quit, Duration::from_millis(200));
        let _ = std::fs::remove_dir_all(&self.1);
    }
}

fn start(level: &str) -> (Scope, Cleanup, PathBuf) {
    let scope = Scope::current(&format!("resident-perf-{level}-{}", std::process::id())).unwrap();
    let root = std::env::temp_dir().join(&scope.namespace);
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("config.toml");
    // Short cards, so the animation timer stops and the resident closes its window.
    std::fs::write(
        &config,
        format!(
            "[general]\nlogging='{level}'\n[sounds]\nenabled=false\n[notifications]\nfade_in_ms=0\nfade_out_ms=0\n[events.permission_required]\nduration_ms=300\n"
        ),
    )
    .unwrap();
    let cleanup = Cleanup(scope.clone(), root.clone());
    match resident::ensure(
        &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
        &config,
        &scope,
    )
    .unwrap()
    {
        Response::Status { .. } => (scope, cleanup, root),
        _ => panic!("unexpected status response"),
    }
}

fn send(scope: &Scope, session: &str) {
    let mut event = Event::demo("permission_required").unwrap();
    event.session_id = Some(session.into());
    pipe::request(scope, &Request::Event { event }, Duration::from_millis(500)).unwrap();
}

fn wait_for(root: &std::path::Path, needle: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let text = std::fs::read_to_string(root.join("aherald.log")).unwrap_or_default();
        if text.contains(needle) {
            return text;
        }
        assert!(
            Instant::now() < deadline,
            "no `{needle}` in the log:\n{text}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_debug_resident_logs_span_statistics_for_the_work_it_did() {
    let (scope, _cleanup, root) = start("debug");
    send(&scope, "perf-session-one");
    send(&scope, "perf-session-two");
    // The listener thread flushes IPC statistics when it stops; the GUI thread flushes
    // its render statistics once the cards have gone and its timer stops.
    wait_for(&root, " perf frame.render n=");
    pipe::request(&scope, &Request::Quit, Duration::from_millis(500)).unwrap();
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
    let (scope, _cleanup, root) = start("warn");
    send(&scope, "quiet-session");
    std::thread::sleep(Duration::from_millis(700));
    pipe::request(&scope, &Request::Quit, Duration::from_millis(500)).unwrap();
    std::thread::sleep(Duration::from_millis(300));
    let text = std::fs::read_to_string(root.join("aherald.log")).unwrap_or_default();
    assert!(!text.contains(" perf "), "{text}");
    assert!(!text.contains("DEBUG"), "{text}");
}
