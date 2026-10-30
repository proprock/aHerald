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
    for name in [
        "PermissionRequest",
        "Interrupt",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
        "SessionEnd",
    ] {
        let result = invoke(
            format!(r#"{{"hook_event_name":"{name}","session_id":"s","turn_id":"t","tool_input":{{"command":"DO_NOT_LOG"}}}}"#)
                .as_bytes(),
        );
        assert!(result.status.success());
        assert!(result.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("DO_NOT_LOG"));
    }
}

#[test]
fn stop_response_survives_invalid_configuration() {
    let path =
        std::env::temp_dir().join(format!("aherald-invalid-hook-{}.toml", std::process::id()));
    std::fs::write(&path, "invalid = [").unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_aherald"))
        .args(["--hook", "--config"])
        .arg(&path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(include_bytes!("fixtures/stop.json"))
        .unwrap();
    let output = child.wait_with_output().unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"{}\n");
}

#[cfg(windows)]
#[test]
fn powershell_gui_invocation_collects_hook_responses_with_quoted_paths() {
    let root = std::env::temp_dir().join(format!("aherald shell's paths {}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("quiet config.toml");
    std::fs::copy("tests/fixtures/quiet.toml", &config).unwrap();
    let executable = root.join("aherald.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_aherald"), &executable).unwrap();
    let quote = |path: &std::path::Path| path.to_str().unwrap().replace('\'', "''");
    for (input, expected) in [
        (include_bytes!("fixtures/stop.json").as_slice(), "{}"),
        (
            include_bytes!("fixtures/permission-request.json").as_slice(),
            "",
        ),
        (include_bytes!("fixtures/interrupt.json").as_slice(), ""),
    ] {
        let command = format!(
            "& '{}' --hook --transient --config '{}' | Out-String",
            quote(&executable),
            quote(&config)
        );
        let mut child = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", &command])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(input).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        while child.try_wait().unwrap().is_none() {
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("PowerShell hook invocation exceeded its deadline");
            }
            std::thread::yield_now();
        }
        let output = child.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), expected);
        assert!(output.stderr.is_empty());
    }
    std::fs::remove_dir_all(root).unwrap();
}
