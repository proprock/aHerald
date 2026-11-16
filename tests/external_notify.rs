//! `aherald --notify <harness> --kind <kind>`: the universal input for other harnesses.
mod support;
use std::process::{Command, Stdio};

fn invoke(args: &[&str], input: &[u8]) -> std::process::Output {
    support::run_bounded_with_input(
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(args)
            // `--notify` uses the resident unless told otherwise: never reach the user's one.
            .args(["--transient", "--config", "tests/fixtures/quiet.toml"])
            .stdin(Stdio::piped()),
        input,
        support::START_BOUND,
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

/// `--notify` without a mode flag goes through a resident, which it starts. The runtime directory
/// is private to each test, so the user's resident is never reached.
#[cfg(unix)]
mod resident {
    use super::support;
    use std::{
        path::Path,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };

    const SESSION: &[u8] = br#"{"session_id":"ah310-session"}"#;

    fn aherald(runtime: &Path, args: &[&str], input: &[u8]) -> std::process::Output {
        support::run_bounded_with_input(
            Command::new(env!("CARGO_BIN_EXE_aherald"))
                .args(args)
                .args(["--config", "tests/fixtures/quiet.toml"])
                .env("XDG_RUNTIME_DIR", runtime)
                .stdin(Stdio::piped()),
            input,
            support::START_BOUND,
        )
        .unwrap()
    }

    /// The queue length of the resident in `runtime`, or `None` when none answers.
    fn queued(runtime: &Path) -> Option<u64> {
        let output = aherald(runtime, &["--status"], b"");
        let status: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
        status.get("queued")?.as_u64()
    }

    fn wait_for_queued(runtime: &Path, expected: u64) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while queued(runtime) != Some(expected) {
            assert!(
                Instant::now() < deadline,
                "expected {expected} queued cards, found {:?}",
                queued(runtime)
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// Stops the resident a test started, also when an assertion fails.
    struct Stop<'a>(&'a Path);
    impl Drop for Stop<'_> {
        fn drop(&mut self) {
            let deadline = Instant::now() + Duration::from_secs(3);
            while Instant::now() < deadline {
                let output = aherald(self.0, &["--quit"], b"");
                if String::from_utf8_lossy(&output.stdout).contains("bye")
                    || queued(self.0).is_none()
                {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }

    #[test]
    fn a_wait_starts_a_resident_and_the_answer_clears_its_card() {
        // The Linux resident opens an X display first; CI runs this file again under xvfb-run.
        if cfg!(target_os = "linux") && std::env::var_os("DISPLAY").is_none() {
            eprintln!("skipped: no X display (run under xvfb-run)");
            return;
        }
        let runtime = support::TempDir::new("n310");
        let _stop = Stop(&runtime);
        let wait = aherald(
            &runtime,
            &["--notify", "opencode", "--kind", "permission_required"],
            SESSION,
        );
        assert!(wait.status.success());
        wait_for_queued(&runtime, 1);
        let answer = aherald(
            &runtime,
            &[
                "--notify",
                "opencode",
                "--kind",
                "permission_required",
                "--activity",
            ],
            SESSION,
        );
        assert!(answer.status.success());
        wait_for_queued(&runtime, 0);
    }

    #[test]
    fn transient_notify_starts_no_resident() {
        let runtime = support::TempDir::new("t310");
        let _stop = Stop(&runtime);
        let output = aherald(
            &runtime,
            &[
                "--notify",
                "opencode",
                "--kind",
                "permission_required",
                "--transient",
            ],
            SESSION,
        );
        assert!(output.status.success());
        std::thread::sleep(Duration::from_millis(500));
        assert_eq!(
            queued(&runtime),
            None,
            "--transient must not start a resident"
        );
    }
}
