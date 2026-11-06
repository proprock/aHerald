//! The sender crate ships with the program and must carry its version: a release bump edits
//! `Cargo.toml`, and the packaging scripts read the program's version with `cargo metadata`.
use std::{fs, path::Path};

fn version(manifest: &str) -> String {
    let text = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(manifest)).unwrap();
    text.lines()
        .find_map(|line| {
            line.strip_prefix("version")
                .and_then(|rest| rest.trim_start().strip_prefix('='))
                .map(|value| value.trim().trim_matches('"').to_owned())
        })
        .unwrap_or_else(|| panic!("no version in {manifest}"))
}

#[test]
fn aherald_ping_has_the_version_of_aherald() {
    assert_eq!(
        version("crates/aherald-ping/Cargo.toml"),
        version("Cargo.toml")
    );
    assert_eq!(version("Cargo.toml"), env!("CARGO_PKG_VERSION"));
}

/// Runs `body` in a POSIX shell at the repository root with `scripts/version.sh` loaded.
#[cfg(unix)]
fn shell(body: &str) -> std::process::Output {
    std::process::Command::new("sh")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .arg("-c")
        .arg(format!(". scripts/version.sh; {body}"))
        .output()
        .expect("sh runs")
}

#[cfg(unix)]
#[test]
fn the_shell_scripts_read_the_program_version_from_cargo() {
    let output = shell("crate_version");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        env!("CARGO_PKG_VERSION")
    );
}

#[cfg(unix)]
#[test]
fn a_tag_must_be_v_plus_the_crate_version() {
    let version = env!("CARGO_PKG_VERSION");
    let good = shell(&format!("require_tag_matches v{version} {version}"));
    assert!(good.status.success(), "{good:?}");
    for tag in [version.to_owned(), format!("v{version}.1"), String::new()] {
        let bad = shell(&format!("require_tag_matches '{tag}' {version}"));
        assert!(!bad.status.success(), "tag {tag:?} was accepted");
        let message = String::from_utf8_lossy(&bad.stderr);
        assert!(message.contains(version), "{message}");
    }
}

#[cfg(windows)]
#[test]
fn the_powershell_scripts_read_the_program_version_from_cargo() {
    let run = |body: &str| {
        std::process::Command::new("powershell")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .args(["-NoProfile", "-NonInteractive", "-Command"])
            .arg(format!(". ./scripts/Version.ps1; {body}"))
            .output()
            .expect("powershell runs")
    };
    let version = env!("CARGO_PKG_VERSION");
    let output = run("Get-CrateVersion");
    assert!(output.status.success(), "{output:?}");
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), version);
    assert!(
        run(&format!("Assert-TagMatches v{version} {version}"))
            .status
            .success()
    );
    for tag in [version.to_owned(), format!("v{version}.1")] {
        let bad = run(&format!("Assert-TagMatches {tag} {version}"));
        assert!(!bad.status.success(), "tag {tag:?} was accepted");
    }
}

#[test]
fn the_claude_plugin_has_the_version_of_aherald() {
    let text = fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("plugins/aherald/.claude-plugin/plugin.json"),
    )
    .unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(manifest["version"], env!("CARGO_PKG_VERSION"));
}
