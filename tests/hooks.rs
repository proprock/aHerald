use std::{
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn invoke(input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_aherald"))
        .args(["--hook", "--config", "tests/fixtures/quiet.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}
#[test]
fn stop_returns_empty_json_without_a_decision() {
    let result = invoke(include_bytes!("fixtures/stop.json"));
    assert!(result.status.success());
    assert_eq!(result.stdout, b"{}\n");
}
#[test]
fn malformed_and_unknown_hooks_are_silent_success() {
    for bytes in [
        b"{".as_slice(),
        br#"{"hook_event_name":"Unknown"}"#,
        br#"{"hook_event_name":"PermissionRequest"}"#,
    ] {
        let result = invoke(bytes);
        assert!(result.status.success());
        assert!(result.stdout.is_empty());
    }
}
#[test]
fn open_stdin_does_not_hold_hook_process() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_aherald"))
        .args(["--hook", "--config", "tests/fixtures/quiet.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let start = Instant::now();
    let deadline = start + Duration::from_secs(3);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("hook did not return with stdin open");
        }
        std::thread::yield_now();
    }
}

#[test]
fn permission_and_interrupt_do_not_emit_decisions_or_payloads() {
    for name in ["PermissionRequest", "Interrupt"] {
        let result = invoke(
            format!(r#"{{"hook_event_name":"{name}","tool_input":{{"command":"DO_NOT_LOG"}}}}"#)
                .as_bytes(),
        );
        assert!(result.status.success());
        assert!(result.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("DO_NOT_LOG"));
    }
}
