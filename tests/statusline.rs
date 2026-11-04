//! The `--statusline` wrapper as Claude Code runs it: payload on stdin, the wrapped command's
//! output on stdout, and no resident required.
#![cfg(unix)]
mod support;
use std::process::{Command, Stdio};

fn run(args: &[&str], input: &str) -> std::process::Output {
    support::run_bounded_with_input(
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .arg("--statusline")
            .args(args)
            .stdin(Stdio::piped()),
        input.as_bytes(),
        std::time::Duration::from_secs(3),
    )
    .unwrap()
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
    // No configuration at all (an explicit path nobody wrote), so no machine's own file counts.
    let none = std::env::temp_dir().join(format!("aherald-sl-{}-none", std::process::id()));
    let none = none.join("config.toml");
    let none = none.to_str().unwrap();
    let out = run(&["--config", none], PAYLOAD);
    assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
    let garbage = run(&["--config", none], "not json at all");
    assert!(garbage.status.success() && garbage.stdout.is_empty());
}

#[test]
fn the_exit_code_of_the_wrapped_command_is_kept() {
    let out = run(&["--then", "echo bar; exit 2"], PAYLOAD);
    assert_eq!(out.status.code(), Some(2));
    assert_eq!(String::from_utf8_lossy(&out.stdout), "bar\n");
}

fn config(name: &str, text: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("aherald-sl-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("config.toml");
    std::fs::write(&path, text).unwrap();
    path
}

const FORMAT_PAYLOAD: &str = r#"{"session_id":"integration-fmt","model":{"display_name":"Opus"},"cost":{"total_lines_added":4},"context_window":{"used_percentage":12.5}}"#;
const FORMAT: &str = "[claude.statusline]\nformat = \"{model.display_name}{[·{worktree.branch}]}·+{cost.total_lines_added}·{context_window.used_percentage}·{context_window}\"\ncolor = false\n";

#[test]
fn a_configured_format_is_printed_when_the_user_has_no_command() {
    let path = config("format", FORMAT);
    let out = run(&["--config", path.to_str().unwrap()], FORMAT_PAYLOAD);
    assert!(out.status.success() && out.stderr.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "Opus·+4·12%·ctx 12% used\n"
    );
}

#[test]
fn the_users_own_command_wins_over_the_format() {
    let path = config("then", FORMAT);
    let out = run(
        &["--config", path.to_str().unwrap(), "--then", "echo mine"],
        FORMAT_PAYLOAD,
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout), "mine\n");
}

#[test]
fn a_missing_configuration_leaves_the_line_empty_and_a_broken_one_is_named() {
    let missing = std::env::temp_dir()
        .join(format!("aherald-sl-{}-missing", std::process::id()))
        .join("config.toml");
    let out = run(&["--config", missing.to_str().unwrap()], FORMAT_PAYLOAD);
    assert!(out.status.success() && out.stdout.is_empty());
    assert!(!missing.exists() && !missing.parent().unwrap().exists());
    for (name, text) in [
        ("garbage", "this is = not [toml"),
        ("syntax", "[claude.statusline]\nformat = \"{oops\"\n"),
        ("unknown", "[claude.statusline]\nformt = \"x\"\n"),
    ] {
        let path = config(name, text);
        let out = run(&["--config", path.to_str().unwrap()], FORMAT_PAYLOAD);
        let text = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{name}");
        assert!(
            text.starts_with("Invalid configuration: "),
            "{name}: {text}"
        );
    }
}

#[test]
fn an_invalid_payload_prints_nothing_even_with_a_format() {
    let path = config("payload", FORMAT);
    let out = run(&["--config", path.to_str().unwrap()], "not json");
    assert!(out.status.success() && out.stdout.is_empty());
}

#[test]
fn an_unreadable_configuration_is_named_on_the_status_line_instead_of_hiding_the_format() {
    let bad = config("bad", "[claude.statusline]\nformt = \"x\"\n");
    let out = run(&["--config", bad.to_str().unwrap()], FORMAT_PAYLOAD);
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.starts_with("Invalid configuration: "), "{text}");
    // No file is just no configuration; the user's own command is never touched.
    let missing = run(&["--config", "/no/such/config.toml"], FORMAT_PAYLOAD);
    assert_eq!(String::from_utf8_lossy(&missing.stdout), "");
    let then = run(
        &["--config", bad.to_str().unwrap(), "--then", "echo mine"],
        FORMAT_PAYLOAD,
    );
    assert_eq!(String::from_utf8_lossy(&then.stdout), "mine\n");
}
