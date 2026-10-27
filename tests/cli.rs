use std::process::Command;

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
