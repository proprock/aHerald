use std::{
    io::Write,
    process::{Command, Stdio},
};

fn invoke(args: &[&str], input: &[u8]) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_aherald"))
        .args(args)
        .args(["--config", "tests/fixtures/quiet.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn claude_hooks_never_write_a_decision_to_stdout() {
    for name in ["Stop", "StopFailure", "PermissionRequest", "Notification"] {
        let result = invoke(
            &["--hook", "--agent", "claude"],
            format!(
                r#"{{"hook_event_name":"{name}","session_id":"s","notification_type":"permission_prompt","tool_input":{{"command":"DO_NOT_LOG"}}}}"#
            )
            .as_bytes(),
        );
        assert!(result.status.success(), "{name}");
        assert!(
            result.stdout.is_empty(),
            "{name} must not answer Claude Code"
        );
        assert!(!String::from_utf8_lossy(&result.stderr).contains("DO_NOT_LOG"));
    }
}

#[test]
fn claude_hook_ignores_malformed_and_unknown_input_silently() {
    for bytes in [b"{".as_slice(), br#"{"hook_event_name":"PostToolUse"}"#] {
        let result = invoke(&["--hook", "--agent", "claude"], bytes);
        assert!(result.status.success());
        assert!(result.stdout.is_empty());
    }
}

#[test]
fn codex_stop_reply_is_unchanged_without_agent_flag() {
    let result = invoke(&["--hook"], br#"{"hook_event_name":"Stop"}"#);
    assert_eq!(result.stdout, b"{}\n");
}

#[test]
fn invalid_agent_flag_degrades_safely_in_hook_mode() {
    // Hook processes must never fail Claude Code: report on stderr, stay silent on stdout.
    for args in [
        &["--hook", "--agent", "gemini"][..],
        &["--hook", "--agent", "claude", "--agent", "codex"],
    ] {
        let result = invoke(args, br#"{"hook_event_name":"Stop"}"#);
        assert!(result.status.success(), "{args:?}");
        assert!(result.stdout.is_empty(), "{args:?}");
        assert!(!result.stderr.is_empty(), "{args:?}");
    }
}

#[test]
fn agent_flag_requires_hook_operation() {
    let result = invoke(&["--agent", "claude", "--event", "turn_complete"], b"");
    assert!(!result.status.success());
}
