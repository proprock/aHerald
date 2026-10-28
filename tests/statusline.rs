//! The `--statusline` wrapper as Claude Code runs it: payload on stdin, the wrapped command's
//! output on stdout, and no resident required.
#![cfg(unix)]
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn run(args: &[&str], input: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_aherald"))
        .arg("--statusline")
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

const PAYLOAD: &str =
    r#"{"session_id":"integration-sl","session_name":"N","model":{"display_name":"M"}}"#;

#[test]
fn the_status_line_of_the_wrapped_command_comes_through_unchanged() {
    let out = run(&["--then", "tr a-z A-Z"], PAYLOAD);
    assert!(out.status.success());
    assert_eq!(String::from_utf8_lossy(&out.stdout), PAYLOAD.to_uppercase());
    assert!(out.stderr.is_empty());
}

#[test]
fn alone_it_prints_nothing_and_succeeds_with_or_without_a_running_resident() {
    let out = run(&[], PAYLOAD);
    assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
    let garbage = run(&[], "not json at all");
    assert!(garbage.status.success() && garbage.stdout.is_empty());
}

#[test]
fn the_exit_code_of_the_wrapped_command_is_kept() {
    let out = run(&["--then", "echo bar; exit 2"], PAYLOAD);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "bar\n");
}
