//! Off Windows the binary builds and parses input, but the notification service is a
//! stand-in until the Unix implementations land. It must say so instead of crashing.
#![cfg(not(windows))]
use std::process::{Command, Stdio};

fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_aherald"))
        .args(args)
        .args(["--config", "tests/fixtures/quiet.toml"])
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn status_reports_that_no_service_is_running() {
    let output = run(&["--status"]);
    assert!(output.status.success());
    assert_eq!(output.stdout, b"{\"running\":false}\n");
}

#[test]
#[cfg(not(target_os = "macos"))]
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
