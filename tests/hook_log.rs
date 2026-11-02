//! A hook input that cannot be understood leaves one log line with a fixed code and nothing
//! of the input, while the hook keeps its contract: silent stdout, exit 0.
mod support;
use std::{
    path::Path,
    process::{Command, Stdio},
};

fn directory(test: &str) -> support::TempDir {
    let directory = support::TempDir::new(&format!("hook-log-{test}"));
    std::fs::write(
        directory.join("config.toml"),
        "[notifications]\nenabled=false\n[sounds]\nenabled=false\n",
    )
    .unwrap();
    directory
}

fn invoke(directory: &Path, agent: Option<&str>, input: &[u8]) -> std::process::Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_aherald"));
    command.arg("--hook");
    if let Some(agent) = agent {
        command.args(["--agent", agent]);
    }
    support::run_bounded_with_input(
        command
            .arg("--config")
            .arg(directory.join("config.toml"))
            .stdin(Stdio::piped()),
        input,
        std::time::Duration::from_secs(3),
    )
    .unwrap()
}

fn log(directory: &Path) -> String {
    std::fs::read_to_string(directory.join("aherald.log")).unwrap_or_default()
}

#[test]
fn a_hook_input_that_is_not_json_or_is_too_large_is_logged_by_code_only() {
    let directory = directory("refused");
    for agent in [None, Some("claude")] {
        let result = invoke(&directory, agent, br#"{"secret-prompt": "#);
        assert!(result.status.success());
        assert!(result.stdout.is_empty());
    }
    let result = invoke(&directory, None, &vec![b' '; 70 * 1024]);
    assert!(result.status.success());
    assert!(result.stdout.is_empty());

    let log = log(&directory);
    assert_eq!(log.matches("hook.rejected invalid").count(), 2, "{log}");
    assert_eq!(log.matches("hook.rejected too_large").count(), 1, "{log}");
    assert!(!log.contains("secret-prompt"), "{log}");
}

#[test]
fn an_event_the_agent_does_not_report_on_is_not_a_failure() {
    let directory = directory("ignored");
    let result = invoke(&directory, None, br#"{"hook_event_name":"Unknown"}"#);
    assert!(result.status.success());
    assert!(!log(&directory).contains("hook.rejected"));
}

/// A resident that cannot be reached or started leaves the hook silent on stdout, so the log is
/// the only place the failure shows up.
#[cfg(unix)]
#[test]
fn a_hook_that_cannot_reach_or_start_the_resident_logs_a_fixed_code() {
    let directory = directory("unreachable");
    // The runtime directory is a plain file, so no resident socket can exist or be created.
    let runtime = directory.join("not-a-directory");
    std::fs::write(&runtime, b"").unwrap();
    let result = support::run_bounded_with_input(
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(["--hook", "--resident", "--config"])
            .arg(directory.join("config.toml"))
            .env("XDG_RUNTIME_DIR", &runtime)
            .stdin(Stdio::piped()),
        include_bytes!("fixtures/stop.json"),
        std::time::Duration::from_secs(3),
    )
    .unwrap();
    assert!(result.status.success());
    assert_eq!(result.stdout, b"{}\n");

    let log = log(&directory);
    assert_eq!(log.matches("hook.delivery_failed").count(), 1, "{log}");
    assert!(!log.contains("not-a-directory"), "{log}");
}
