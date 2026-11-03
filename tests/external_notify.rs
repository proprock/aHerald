//! `aherald --notify <harness> --kind <kind>`: the universal input for other harnesses.
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
fn a_notification_never_answers_the_harness_and_never_echoes_its_payload() {
    for kind in [
        "turn_complete",
        "permission_required",
        "input_required",
        "error",
        "interrupted",
    ] {
        let result = invoke(
            &["--notify", "opencode", "--kind", kind],
            br#"{"session_id":"s1","cwd":"/work/app","prompt":"DO_NOT_LOG","tool_input":{"command":"DO_NOT_LOG"}}"#,
        );
        assert!(result.status.success(), "{kind}");
        assert!(
            result.stdout.is_empty(),
            "{kind}: stdout belongs to the harness"
        );
        assert!(!String::from_utf8_lossy(&result.stderr).contains("DO_NOT_LOG"));
    }
}

#[test]
fn hostile_missing_or_oversized_input_still_succeeds() {
    let oversized = vec![b'x'; 200_000];
    for input in [
        &b""[..],
        b"not json",
        b"\xff\xfe",
        br#"{"session_id":"\u0007"}"#,
        &oversized,
    ] {
        let result = invoke(&["--notify", "pi", "--kind", "error"], input);
        assert!(result.status.success(), "{} bytes", input.len());
        assert!(result.stdout.is_empty());
    }
    let activity = invoke(
        &["--notify", "pi", "--kind", "turn_complete", "--activity"],
        b"{}",
    );
    assert!(activity.status.success());
}

#[test]
fn a_misused_command_line_reports_the_problem_without_failing_the_harness() {
    for args in [
        vec!["--notify", "pi"],
        vec!["--notify", "claude", "--kind", "error"],
        vec!["--notify", "pi", "--kind", "context_warning"],
    ] {
        let result = invoke(&args, b"{}");
        assert!(
            result.status.success(),
            "{args:?}: a hook-like command never fails its harness"
        );
        assert!(result.stdout.is_empty());
        assert!(
            String::from_utf8_lossy(&result.stderr).starts_with("aHerald:"),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}
