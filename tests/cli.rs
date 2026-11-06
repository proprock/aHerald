mod support;
use std::process::Command;

#[test]
fn raw_output_names_a_source_and_for_usage_exactly_one_harness() {
    use aherald::{cli::Cli, ipc::RawSource};
    let parse = |args: &[&str]| Cli::parse(args.iter().map(|s| s.to_string()));
    let source = |args: &[&str]| parse(args).unwrap().raw_source;
    assert_eq!(
        source(&["--raw-output", "usage", "--harness", "claude"]),
        Some(RawSource::ClaudeUsage)
    );
    assert_eq!(
        source(&["--harness", "codex", "--raw-output", "usage"]),
        Some(RawSource::CodexUsage)
    );
    assert_eq!(
        source(&["--raw-output", "statusline"]),
        Some(RawSource::ClaudeStatusLine)
    );
    assert_eq!(
        source(&["--raw-output", "statusline", "--harness", "claude"]),
        Some(RawSource::ClaudeStatusLine)
    );
    for bad in [
        vec!["--raw-output"],
        vec!["--raw-output", "events"],
        vec!["--raw-output", "usage"],
        vec!["--raw-output", "usage", "--harness", "opencode"],
        vec![
            "--raw-output",
            "usage",
            "--harness",
            "codex",
            "--harness",
            "claude",
        ],
        vec!["--raw-output", "statusline", "--harness", "codex"],
        vec!["--raw-output", "usage", "--harness", "claude", "--apply"],
        vec!["--raw-output", "usage", "--harness", "claude", "--status"],
        vec!["--status", "--harness", "claude"],
    ] {
        assert!(parse(&bad).is_err(), "{bad:?}");
    }
}

#[test]
fn demo_accepts_default_and_custom_pause_and_rejects_conflicts() {
    use aherald::cli::{Cli, Operation};
    let parse = |args: &[&str]| Cli::parse(args.iter().map(|s| s.to_string()));
    assert_eq!(parse(&["--demo"]).unwrap().operation, Some(Operation::Demo));
    assert!(parse(&["--demo", "3", "--config", "example.toml"]).is_ok());
    assert_eq!(parse(&["--demo"]).unwrap().demo_pause_sec, Some(3));
    assert_eq!(parse(&["--demo", "7"]).unwrap().demo_pause_sec, Some(7));
    for args in [
        vec!["--demo", "0"],
        vec!["--demo", "-1"],
        vec!["--demo", "NaN"],
        vec!["--demo", "1.5"],
        vec!["--demo", "18446744073709551615"],
        vec!["--demo", "--hook"],
        vec!["--demo", "--event", "error"],
        vec!["--demo", "--demo"],
        vec!["--demo", "--transient"],
    ] {
        assert!(parse(&args).is_err(), "{args:?}");
    }
}

#[test]
fn help_identifies_application() {
    let output = support::output(Command::new(env!("CARGO_BIN_EXE_aherald")).arg("--help"));
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("aHerald"));
}

#[test]
fn version_prints_the_crate_version_and_needs_no_configuration() {
    let output = support::output(Command::new(env!("CARGO_BIN_EXE_aherald")).args([
        "--version",
        "--config",
        "does-not-exist.toml",
    ]));
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    let prefix = format!("aHerald {} (built 20", env!("CARGO_PKG_VERSION"));
    assert!(text.starts_with(&prefix), "{text}");
    assert!(text.ends_with(" UTC)\n"), "{text}");
}

#[test]
fn version_conflicts_with_other_operations_and_modes() {
    use aherald::cli::{Cli, Operation};
    let parse = |args: &[&str]| Cli::parse(args.iter().map(|s| s.to_string()));
    assert_eq!(
        parse(&["--version"]).unwrap().operation,
        Some(Operation::Version)
    );
    for args in [
        vec!["--version", "--help"],
        vec!["--version", "--hook"],
        vec!["--version", "--resident"],
    ] {
        assert!(parse(&args).is_err(), "{args:?}");
    }
}

#[test]
fn the_about_card_names_the_application_and_its_version() {
    let text = aherald::about_card_text();
    let (heading, body) = text.split_once('\n').unwrap();
    assert_eq!(heading, "aHerald");
    assert!(body.contains(&format!("Version {}", env!("CARGO_PKG_VERSION"))));
}

#[test]
fn status_does_not_require_loading_the_notification_configuration() {
    let path = std::env::temp_dir().join(format!("aherald-control-{}.toml", std::process::id()));
    std::fs::write(&path, "[invalid TOML").unwrap();
    let output = support::output(
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(["--status", "--config"])
            .arg(&path),
    );
    let _ = std::fs::remove_file(&path);
    assert!(
        output.status.success(),
        "status tried to parse notification settings"
    );
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(value.get("running").is_some() || value["result"] == "status");
}

#[test]
fn perf_report_prints_the_ranked_table_from_the_log_beside_the_configuration() {
    let root = std::env::temp_dir().join(format!("aherald-perf-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("config.toml");
    std::fs::write(&config, "[invalid TOML").unwrap(); // not needed, so never parsed
    let run = || {
        support::output(
            Command::new(env!("CARGO_BIN_EXE_aherald"))
                .args(["--perf-report", "--config"])
                .arg(&config),
        )
    };
    let missing = run();
    assert!(!missing.status.success(), "no log means nothing to report");
    std::fs::write(
        root.join("aherald.log"),
        "2026-10-27T14:00:00.000+00:00 DEBUG perf frame.render n=4 avg=2.00ms sd=0.00ms min=2.00ms p95=2.00ms max=2.00ms\n",
    )
    .unwrap();
    let output = run();
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    let row: Vec<_> = text.lines().nth(1).unwrap().split_whitespace().collect();
    assert_eq!(
        row,
        ["frame.render", "4", "8.00", "2.00", "2.00", "2.00", "0"]
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn setup_window_stands_alone_and_rejects_other_options() {
    use aherald::cli::{Cli, Operation};
    let parse = |args: &[&str]| Cli::parse(args.iter().map(|s| s.to_string()));
    assert_eq!(
        parse(&["--setup-window"]).unwrap().operation,
        Some(Operation::SetupWindow)
    );
    for args in [
        vec!["--setup-window", "--apply"],
        vec!["--setup-window", "--resident"],
        vec!["--setup-window", "--codex-usage", "true"],
        vec!["--setup-window", "--console-setup"],
    ] {
        assert!(parse(&args).is_err(), "{args:?} must be rejected");
    }
}

#[test]
fn notify_names_a_harness_and_an_existing_event_kind() {
    use aherald::cli::{Cli, Operation};
    use aherald::events::Kind;
    let parse = |args: &[&str]| Cli::parse(args.iter().map(|s| s.to_string()));
    let cli = parse(&["--notify", "opencode", "--kind", "turn_complete"]).unwrap();
    assert_eq!(cli.operation, Some(Operation::Notify));
    assert_eq!(cli.notify.as_deref(), Some("opencode"));
    assert_eq!(cli.kind, Some(Kind::TurnComplete));
    assert!(!cli.activity && !cli.ended);
    // The kind may come first, and one lifecycle flag is allowed.
    let cli = parse(&[
        "--kind",
        "permission_required",
        "--notify",
        "pi",
        "--activity",
    ])
    .unwrap();
    assert!(cli.activity && cli.notify.as_deref() == Some("pi"));
    assert!(
        parse(&["--notify", "my-tool_2", "--kind", "error", "--ended"])
            .unwrap()
            .ended
    );
    assert!(parse(&["--notify", "goose", "--kind", "interrupted", "--resident"]).is_ok());
    assert!(
        parse(&[
            "--notify",
            "goose",
            "--kind",
            "input_required",
            "--config",
            "c.toml"
        ])
        .is_ok()
    );
}

#[test]
fn notify_rejects_unknown_kinds_reserved_keys_and_misplaced_options() {
    use aherald::cli::Cli;
    let parse = |args: &[&str]| Cli::parse(args.iter().map(|s| s.to_string()));
    for args in [
        vec!["--notify", "opencode"],
        vec!["--notify", "opencode", "--kind", "nope"],
        vec!["--notify", "opencode", "--kind", "context_warning"],
        vec!["--notify", "opencode", "--kind", "idle_warning"],
        vec!["--notify", "OpenCode", "--kind", "error"],
        vec!["--notify", "codex", "--kind", "error"],
        vec!["--notify", "claude", "--kind", "error"],
        vec!["--notify", "default", "--kind", "error"],
        vec!["--notify", "", "--kind", "error"],
        vec!["--notify", "pi", "--kind", "error", "--activity", "--ended"],
        vec!["--notify", "pi", "--kind", "error", "--kind", "error"],
        vec!["--notify", "pi", "--kind", "error", "--notify", "goose"],
        vec!["--notify", "pi", "--kind", "error", "--hook"],
        vec!["--notify", "pi", "--kind", "error", "--event", "error"],
        vec!["--notify", "pi", "--kind", "error", "--agent", "claude"],
        vec!["--kind", "error"],
        vec!["--activity"],
        vec!["--hook", "--ended"],
        vec!["--notify"],
    ] {
        assert!(parse(&args).is_err(), "{args:?} must be rejected");
    }
}

#[test]
fn the_internal_child_modes_are_parsed_by_the_cli() {
    use aherald::cli::{Cli, Operation};
    let parse = |args: &[&str]| Cli::parse(args.iter().map(|s| s.to_string()));
    let cli = parse(&["--resident-child", "resident", "--config", "c.toml"]).unwrap();
    assert_eq!(cli.operation, Some(Operation::ResidentChild));
    assert_eq!(cli.child_scope.as_deref(), Some("resident"));
    assert_eq!(cli.config, Some("c.toml".into()));
    let cli = parse(&["--display-child", "--config", "c.toml"]).unwrap();
    assert_eq!(cli.operation, Some(Operation::DisplayChild));
    assert_eq!(cli.config, Some("c.toml".into()));
    // A restart waits for its parent, then continues as the command that follows.
    let cli = parse(&["--restart-child", "42", "--test-after-apply", "--resident"]).unwrap();
    assert_eq!((cli.restart_parent, cli.test_after_apply), (Some(42), true));
    assert_eq!(cli.operation, None);
    assert!(!cli.no_arguments);
    let cli = parse(&["--restart-child", "42"]).unwrap();
    assert_eq!(
        (cli.restart_parent, cli.test_after_apply),
        (Some(42), false)
    );
    assert!(cli.no_arguments, "nothing follows the restart prefix");
    assert!(parse(&[]).unwrap().no_arguments);
    for bad in [
        vec!["--resident-child"],
        vec!["--resident-child", "resident"],
        vec!["--resident-child", "resident", "--config"],
        vec!["--resident-child", "resident", "--other", "x"],
        vec![
            "--resident-child",
            "resident",
            "--config",
            "c.toml",
            "--resident",
        ],
        vec!["--display-child"],
        vec!["--display-child", "--other", "x"],
        vec!["--display-child", "--config", "c.toml", "extra"],
        vec!["--restart-child"],
        vec!["--restart-child", "not-a-pid"],
        // Only the first argument can open a child mode.
        vec![
            "--hook",
            "--resident-child",
            "resident",
            "--config",
            "c.toml",
        ],
        vec!["--hook", "--restart-child", "42"],
        vec!["--test-after-apply"],
    ] {
        assert!(parse(&bad).is_err(), "{bad:?}");
    }
}
