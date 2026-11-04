mod support;
use std::process::{Command, Stdio};

fn invoke(args: &[&str], input: &[u8]) -> std::process::Output {
    support::run_bounded_with_input(
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(args)
            .args(["--config", "tests/fixtures/quiet.toml"])
            .stdin(Stdio::piped()),
        input,
        std::time::Duration::from_secs(3),
    )
    .unwrap()
}

#[test]
fn claude_hooks_never_write_a_decision_to_stdout() {
    for name in [
        "Stop",
        "StopFailure",
        "PermissionRequest",
        "Notification",
        "UserPromptSubmit",
        "PreToolUse",
        "PostToolUse",
    ] {
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
fn session_end_becomes_an_evidence_only_event_of_its_session() {
    use aherald::{agent::Agent, events::Source};
    for reason in ["clear", "logout", "prompt_input_exit", "other"] {
        let hook = format!(
            r#"{{"hook_event_name":"SessionEnd","session_id":"s","reason":"{reason}","transcript_path":"DO_NOT_KEEP"}}"#
        );
        let event = Agent::Claude
            .parse_hook(hook.as_bytes())
            .unwrap()
            .unwrap_or_else(|| panic!("{reason}"));
        assert_eq!(event.source, Source::ClaudeHook);
        assert_eq!(event.session_id.as_deref(), Some("s"));
        assert!(event.ended, "{reason}");
        assert!(event.is_terminal_evidence(), "{reason}");
    }
}

#[test]
fn session_end_without_a_session_and_subagent_stop_are_ignored() {
    use aherald::agent::Agent;
    for hook in [
        r#"{"hook_event_name":"SessionEnd","reason":"other"}"#,
        r#"{"hook_event_name":"SubagentStop","session_id":"s","agent_id":"x","agent_type":"Explore"}"#,
    ] {
        assert_eq!(
            Agent::Claude.parse_hook(hook.as_bytes()),
            Ok(None),
            "{hook}"
        );
    }
}

#[test]
fn session_end_hook_process_stays_silent_and_succeeds() {
    for name in ["SessionEnd", "SubagentStop"] {
        let result = invoke(
            &["--hook", "--agent", "claude"],
            format!(r#"{{"hook_event_name":"{name}","session_id":"s","reason":"logout"}}"#)
                .as_bytes(),
        );
        assert!(result.status.success(), "{name}");
        assert!(result.stdout.is_empty(), "{name}");
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

#[test]
fn only_a_shown_stop_can_be_held_back_for_background_work() {
    use aherald::agent::{Agent, is_claude_stop};
    let event = |name: &str| {
        Agent::Claude
            .parse_hook(format!(r#"{{"hook_event_name":"{name}","session_id":"s"}}"#).as_bytes())
            .unwrap()
            .unwrap()
    };
    assert!(is_claude_stop(&event("Stop")));
    for name in [
        "StopFailure",
        "PermissionRequest",
        "SessionEnd",
        "PreToolUse",
    ] {
        assert!(!is_claude_stop(&event(name)), "{name}");
    }
}

#[test]
fn a_stop_with_background_work_succeeds_silently_without_a_resident() {
    let result = invoke(
        &["--hook", "--agent", "claude"],
        br#"{"hook_event_name":"Stop","session_id":"s","background_tasks":[{"id":"1","type":"shell","status":"running","command":"DO_NOT_LOG"}],"session_crons":[]}"#,
    );
    assert!(result.status.success());
    assert!(result.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&result.stderr).contains("DO_NOT_LOG"));
}
