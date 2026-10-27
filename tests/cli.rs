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
