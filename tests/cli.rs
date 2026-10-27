use std::process::Command;

#[test]
fn demo_accepts_default_and_custom_pause_and_rejects_conflicts() {
    use aherald::cli::Cli;
    let parse = |args: &[&str]| Cli::parse(args.iter().map(|s| s.to_string()));
    assert_eq!(
        parse(&["--demo"]).unwrap().operation.as_deref(),
        Some("demo")
    );
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
    let output = Command::new(env!("CARGO_BIN_EXE_aherald"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("aHerald"));
}

#[test]
fn status_does_not_require_loading_the_notification_configuration() {
    let path = std::env::temp_dir().join(format!("aherald-control-{}.toml", std::process::id()));
    std::fs::write(&path, "[invalid TOML").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_aherald"))
        .args(["--status", "--config"])
        .arg(&path)
        .output()
        .unwrap();
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
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(["--perf-report", "--config"])
            .arg(&config)
            .output()
            .unwrap()
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
