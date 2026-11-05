//! The OpenCode and Pi bridges in `examples/harnesses` are TypeScript run by the harnesses,
//! so their contract tests run under Node. Without Node 22+ the test reports that and passes,
//! unless `AHERALD_REQUIRE_NODE=1` is set (CI does), which turns the missing Node into a failure.
use std::process::Command;

fn node() -> Option<String> {
    ["node", "/opt/homebrew/opt/node@22/bin/node"]
        .into_iter()
        .find(|node| {
            Command::new(node)
                .arg("--version")
                .output()
                .ok()
                .and_then(|out| {
                    let text = String::from_utf8_lossy(&out.stdout).into_owned();
                    text.trim_start_matches('v')
                        .split('.')
                        .next()?
                        .parse::<u32>()
                        .ok()
                })
                .is_some_and(|major| major >= 22)
        })
        .map(str::to_owned)
}

fn require_node() -> bool {
    std::env::var_os("AHERALD_REQUIRE_NODE").is_some_and(|value| value == "1")
}

#[test]
fn bridges_turn_harness_events_into_bounded_aherald_calls() {
    let Some(node) = node() else {
        assert!(
            !require_node(),
            "AHERALD_REQUIRE_NODE is set but Node.js 22+ is not installed"
        );
        eprintln!("skipped: Node.js 22+ is not installed");
        return;
    };
    let output = Command::new(node)
        .args(["--test", "tests/bridges/bridges.test.mjs"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
}
