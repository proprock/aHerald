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
