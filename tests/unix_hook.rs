//! A hook must come back promptly and must not depend on the resident answering.
#![cfg(unix)]
mod support;
use std::{
    os::unix::{fs::DirBuilderExt, net::UnixListener},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Runs `--hook --resident` with the runtime directory pointed at `runtime` so the
/// resident socket of the real user is never touched.
fn hook(runtime: &std::path::Path) -> (std::process::Output, Duration) {
    let started = Instant::now();
    // Bounded well above the 5 s the tests assert, so a hang fails with a diagnostic and the
    // hook process is killed instead of blocking the suite.
    let mut command = Command::new(env!("CARGO_BIN_EXE_aherald"));
    command
        .args([
            "--hook",
            "--resident",
            "--config",
            "tests/fixtures/quiet.toml",
        ])
        .env("XDG_RUNTIME_DIR", runtime)
        .stdin(Stdio::piped());
    let output = support::run_bounded_with_input(
        &mut command,
        include_bytes!("fixtures/stop.json"),
        Duration::from_secs(20),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    (output, started.elapsed())
}

/// A hook that finds no resident starts one in the background; stop it so it does not outlive
/// the test (on macOS it would keep a status item in the menu bar).
fn stop_residents(runtime: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let output = support::run_bounded(
            Command::new(env!("CARGO_BIN_EXE_aherald"))
                .args(["--quit", "--config", "tests/fixtures/quiet.toml"])
                .env("XDG_RUNTIME_DIR", runtime)
                .stdin(Stdio::null()),
            Duration::from_secs(5),
        )
        .unwrap_or_else(|error| panic!("{error}"));
        if String::from_utf8_lossy(&output.stdout).contains("bye") {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn runtime(tag: &str) -> support::TempDir {
    support::TempDir::new(tag)
}

#[test]
fn the_hook_returns_its_reply_when_no_resident_can_be_reached() {
    let runtime = runtime("none");
    let (output, elapsed) = hook(&runtime);
    assert_eq!(output.stdout, b"{}\n");
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
    stop_residents(&runtime);
}

#[test]
fn the_hook_returns_its_reply_when_the_resident_accepts_but_never_answers() {
    let runtime = runtime("stalled");
    let directory = runtime.join("aherald");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let listener = UnixListener::bind(directory.join("resident.sock")).unwrap();
    // Holds every connection open and silent for the length of the test.
    let held = std::thread::spawn(move || {
        let mut clients = Vec::new();
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            if let Ok((stream, _)) = listener.accept() {
                clients.push(stream);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    let (output, elapsed) = hook(&runtime);
    assert_eq!(output.stdout, b"{}\n");
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
    held.join().unwrap();
}

/// Runs one `--hook --agent <agent> --resident` with the default warnings (idle and context
/// off) against a fake resident in a private runtime directory, and returns every request the
/// fake received as JSON. The fake answers `accepted`, as a running resident does.
fn forwarded(agent: &str, payload: &str) -> Vec<serde_json::Value> {
    use aherald::ipc::{Response, encode, frame_length};
    use std::io::{Read, Write};
    let runtime = runtime(&format!("forward-{agent}"));
    let directory = runtime.join("aherald");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let listener = UnixListener::bind(directory.join("resident.sock")).unwrap();
    listener.set_nonblocking(true).unwrap();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let fake = std::thread::spawn({
        let stop = stop.clone();
        move || {
            let mut seen = Vec::new();
            while !stop.load(std::sync::atomic::Ordering::SeqCst) {
                let Ok((mut stream, _)) = listener.accept() else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut header = [0; 6];
                stream.read_exact(&mut header).unwrap();
                let mut body = vec![0; frame_length(&header).unwrap()];
                stream.read_exact(&mut body).unwrap();
                seen.push(serde_json::from_slice::<serde_json::Value>(&body).unwrap());
                stream
                    .write_all(&encode(&Response::Accepted).unwrap())
                    .unwrap();
            }
            seen
        }
    });
    let output = support::run_bounded_with_input(
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(["--hook", "--agent", agent, "--resident"])
            .args(["--config", "tests/fixtures/quiet.toml"])
            .env("XDG_RUNTIME_DIR", &*runtime)
            .stdin(Stdio::piped()),
        payload.as_bytes(),
        Duration::from_secs(20),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    assert!(output.status.success());
    stop.store(true, std::sync::atomic::Ordering::SeqCst);
    fake.join().unwrap()
}

#[test]
fn tool_activity_reaches_a_running_resident_with_the_idle_warning_off() {
    for agent in ["codex", "claude"] {
        for name in ["PreToolUse", "PostToolUse", "UserPromptSubmit"] {
            let payload = format!(
                r#"{{"hook_event_name":"{name}","session_id":"s","turn_id":"t","tool_use_id":"call-1","tool_input":{{"command":"SECRET"}}}}"#
            );
            let seen = forwarded(agent, &payload);
            let [request] = seen.as_slice() else {
                panic!("{agent} {name}: {seen:?}");
            };
            assert_eq!(request["operation"], "event", "{agent} {name}");
            assert_eq!(request["event"]["activity"], true, "{agent} {name}");
            assert_eq!(request["event"]["session_id"], "s", "{agent} {name}");
            assert!(!request.to_string().contains("SECRET"), "{agent} {name}");
        }
    }
}

#[test]
fn a_completed_codex_model_request_reaches_the_resident_with_its_turn() {
    let seen = forwarded(
        "codex",
        r#"{"hook_event_name":"PostModelRequest","session_id":"s","turn_id":"t","status":"completed","token_usage":{"input_tokens":5}}"#,
    );
    let [request] = seen.as_slice() else {
        panic!("{seen:?}");
    };
    assert_eq!(request["operation"], "codex_context");
    assert_eq!(request["report"]["session_id"], "s");
    assert_eq!(request["report"]["turn_id"], "t");
    assert_eq!(request["report"]["tokens"], 5);
}

#[test]
fn a_completed_subagent_model_request_reaches_the_resident_as_progress() {
    let seen = forwarded(
        "codex",
        r#"{"hook_event_name":"PostModelRequest","session_id":"s","turn_id":"t","agent_id":"child","status":"completed","token_usage":{"input_tokens":5}}"#,
    );
    let [request] = seen.as_slice() else {
        panic!("{seen:?}");
    };
    assert_eq!(
        request,
        &serde_json::json!({"operation":"codex_progress","session_id":"s","turn_id":"t","agent_id":"child"})
    );
}

#[test]
fn an_unfinished_codex_model_request_reaches_nothing() {
    for status in ["failed", "cancelled"] {
        let payload = format!(
            r#"{{"hook_event_name":"PostModelRequest","session_id":"s","turn_id":"t","status":"{status}"}}"#
        );
        assert!(forwarded("codex", &payload).is_empty(), "{status}");
    }
}
