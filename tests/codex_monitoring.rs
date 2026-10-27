use std::process::Command;

#[test]
fn configured_input_monitoring_reports_unsupported_without_blocking_delivery() {
    let path = std::env::temp_dir().join(format!("aherald-monitoring-{}.toml", std::process::id()));
    std::fs::write(
        &path,
        "[codex]\ninput_monitoring=true\n[notifications]\nenabled=false\n[sounds]\nenabled=false",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_aherald"))
        .args(["--event", "turn_complete", "--config"])
        .arg(&path)
        .output()
        .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    let diagnostic = String::from_utf8(output.stderr).unwrap();
    assert!(
        diagnostic.contains("input monitoring unsupported"),
        "{diagnostic}"
    );
    assert!(
        diagnostic.contains("unrelated CLI/app sessions"),
        "{diagnostic}"
    );
}
