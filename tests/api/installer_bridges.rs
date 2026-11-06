//! The Windows installer lists the bridge files by hand (a task, the `--harness` argument and the
//! settings-report line each). Adding a `Bridge` variant must fail here until the installer
//! knows it, because Inno Setup cannot iterate the Rust enum.
use aherald::bridge_setup::Bridge;
use std::{fs, path::Path};

fn installer() -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("installer/windows.iss")).unwrap()
}

#[test]
fn the_windows_installer_offers_every_bridge() {
    let script = installer();
    for bridge in Bridge::ALL {
        let key = bridge.key();
        for (what, needle) in [
            ("a task", format!("Name: {key};")),
            ("the setup argument", format!("--harness {key}'")),
            (
                "the settings report line",
                format!("'{key}_bridge', '{key}'"),
            ),
        ] {
            assert!(
                script.contains(&needle),
                "installer/windows.iss has no {what} for the {} bridge (`{needle}`)",
                bridge.label()
            );
        }
        assert!(
            script.contains(bridge.label()),
            "the installer task for {} does not show its name",
            bridge.label()
        );
    }
}
