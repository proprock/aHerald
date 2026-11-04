//! `aherald-cli` is the console-subsystem companion (AH-211). It must give `aherald.exe` a
//! console and stdio, and return its exit code, so that command-line output shows in PowerShell
//! and cmd without `| Out-String`.
#![cfg(windows)]

mod support;

use std::io::Write;
use std::process::{Command, Output, Stdio};
use support::TempDir;

fn companion() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aherald-cli"))
}

fn direct() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aherald"))
}

fn run_with_stdin(command: &mut Command, input: &[u8]) -> Output {
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(input)
        .expect("write stdin");
    child.wait_with_output().expect("wait")
}

#[test]
fn version_output_and_exit_code_match_the_resident_binary() {
    let via_companion = support::output(companion().arg("--version"));
    let via_direct = support::output(direct().arg("--version"));

    assert!(!via_direct.stdout.is_empty());
    assert_eq!(via_companion.stdout, via_direct.stdout);
    assert_eq!(via_companion.status.code(), via_direct.status.code());
}

#[test]
fn an_unknown_option_keeps_its_stderr_and_exit_code() {
    let via_companion = support::output(companion().arg("--no-such-option"));
    let via_direct = support::output(direct().arg("--no-such-option"));

    assert_eq!(via_direct.status.code(), Some(2));
    assert!(!via_direct.stderr.is_empty());
    assert_eq!(via_companion.stderr, via_direct.stderr);
    assert_eq!(via_companion.status.code(), Some(2));
}

#[test]
fn stdin_reaches_the_wrapped_command_byte_for_byte() {
    // An invalid payload sends nothing to a running resident; the wrapped command still gets it.
    let payload = "не JSON, 🦀 Проверка\n".as_bytes();
    let arguments = ["--statusline", "--then", "findstr \"^\""];

    let via_companion = run_with_stdin(companion().args(arguments), payload);
    let via_direct = run_with_stdin(direct().args(arguments), payload);

    assert_eq!(via_companion.status.code(), via_direct.status.code());
    assert_eq!(via_companion.stdout, via_direct.stdout);
    assert!(String::from_utf8_lossy(&via_companion.stdout).contains("🦀 Проверка"));
}

#[test]
fn a_copy_without_the_resident_binary_reports_an_error_without_panicking() {
    let dir = TempDir::new("cli-companion");
    let lone = dir.join("aherald-cli.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_aherald-cli"), &lone).expect("copy companion");

    let output = support::output(Command::new(&lone).arg("--version"));

    assert_ne!(output.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("aherald.exe"), "stderr: {stderr}");
    assert!(!stderr.contains("panicked"), "stderr: {stderr}");
}
