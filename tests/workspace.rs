//! The sender crate ships with the program and must carry its version: a release bump edits
//! `Cargo.toml`, and the packaging scripts read the first `version =` line of it.
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

#[test]
fn the_packaging_scripts_still_read_the_program_version_first() {
    // The same sed the scripts use; the workspace table must not put another version before it.
    let text =
        fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let first = text
        .lines()
        .find(|line| line.starts_with("version"))
        .unwrap();
    assert!(first.contains(env!("CARGO_PKG_VERSION")), "{first}");
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
