//! Off Windows the binary builds and parses input, but the notification service is a
//! stand-in until the Unix implementations land. It must say so instead of crashing.
#![cfg(not(windows))]
mod support;
use std::{
    process::{Command, Stdio},
    time::Duration,
};

/// Runs with a private runtime directory, so a resident of the real user is never reached.
fn run(args: &[&str]) -> std::process::Output {
    let runtime = std::env::temp_dir().join(format!(
        "ah-unsupported-{}-{}",
        std::process::id(),
        args.join("_").replace('-', "")
    ));
    let _ = std::fs::remove_dir_all(&runtime);
    std::fs::create_dir_all(&runtime).unwrap();
    let output = support::run_bounded(
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(args)
            .args(["--config", "tests/fixtures/quiet.toml"])
            .env("XDG_RUNTIME_DIR", &runtime)
            .stdin(Stdio::null()),
        Duration::from_secs(20),
    )
    .unwrap_or_else(|error| panic!("{error}"));
    let _ = std::fs::remove_dir_all(&runtime);
    output
}

#[test]
fn status_reports_that_no_service_is_running() {
    let output = run(&["--status"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"{\"running\":false}\n");
}

#[test]
#[cfg(all(not(target_os = "macos"), not(target_os = "linux")))]
fn starting_the_resident_fails_with_a_clear_message() {
    let output = run(&["--resident"]);
    assert_eq!(output.status.code(), Some(2));
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("not supported"), "{error}");
}

#[test]
fn help_is_available() {
    let output = run(&["--help"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: aherald"));
}
