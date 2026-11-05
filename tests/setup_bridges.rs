//! `--setup --harness opencode|pi|goose`: aHerald-owned bridge files, installed and removed
//! through the setup receipt. Crush is configured by hand (its `crushrc` is the user's script).
mod support;
use std::{
    path::{Path, PathBuf},
    process::{Command, Output},
};

struct Home {
    root: PathBuf,
    exe: PathBuf,
}
impl Home {
    fn new(name: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let root = std::env::temp_dir().join(format!(
            "aherald bridges '{name}' {}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(root.join("home")).unwrap();
        let exe = root.join(if cfg!(windows) {
            "aherald.exe"
        } else {
            "aherald"
        });
        std::fs::copy(env!("CARGO_BIN_EXE_aherald"), &exe).unwrap();
        Self { root, exe }
    }
    fn run(&self, args: &[&str]) -> Output {
        let home = self.root.join("home");
        support::run_bounded(
            Command::new(&self.exe)
                .args(args)
                .env("HOME", &home)
                .env("USERPROFILE", &home)
                .env("CODEX_HOME", self.root.join("codex"))
                .env("CLAUDE_CONFIG_DIR", self.root.join("claude"))
                .env_remove("XDG_CONFIG_HOME")
                .env_remove("PI_CODING_AGENT_DIR"),
            std::time::Duration::from_secs(20),
        )
        .unwrap()
    }
    fn ok(&self, args: &[&str]) -> String {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }
    fn opencode(&self) -> PathBuf {
        self.root.join("home/.config/opencode/plugins/aherald.ts")
    }
    fn pi(&self) -> PathBuf {
        self.root.join("home/.pi/agent/extensions/aherald.ts")
    }
    fn goose(&self) -> PathBuf {
        self.root
            .join("home/.agents/plugins/aherald/hooks/hooks.json")
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn backup(file: &Path) -> PathBuf {
    let mut name = file.file_name().unwrap().to_os_string();
    name.push(".aherald-backup");
    file.with_file_name(name)
}

#[test]
fn a_dry_run_writes_nothing_and_apply_installs_files_that_point_at_this_executable() {
    let app = Home::new("install");
    let preview = app.ok(&[
        "--setup",
        "--harness",
        "opencode",
        "--harness",
        "pi",
        "--harness",
        "goose",
    ]);
    assert!(preview.contains("Dry run"), "{preview}");
    assert!(!app.opencode().exists() && !app.pi().exists() && !app.goose().exists());
    app.ok(&[
        "--setup",
        "--harness",
        "opencode",
        "--harness",
        "pi",
        "--harness",
        "goose",
        "--apply",
    ]);
    let exe = serde_json::to_string(app.exe.to_str().unwrap()).unwrap();
    for file in [app.opencode(), app.pi()] {
        let text = std::fs::read_to_string(file).unwrap();
        assert!(
            text.contains(&format!("process.env.AHERALD_EXE || {exe}")),
            "{text}"
        );
    }
    let goose: serde_json::Value =
        serde_json::from_slice(&std::fs::read(app.goose()).unwrap()).unwrap();
    let command = goose["hooks"]["Stop"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    let quoted = app.exe.to_str().unwrap().replace('\'', "'\\''");
    assert_eq!(
        command,
        format!("'{quoted}' --notify goose --kind turn_complete")
    );
    // Repeating changes nothing and makes no backup.
    let before = std::fs::read(app.opencode()).unwrap();
    app.ok(&[
        "--setup",
        "--harness",
        "opencode",
        "--harness",
        "pi",
        "--harness",
        "goose",
        "--apply",
    ]);
    assert_eq!(std::fs::read(app.opencode()).unwrap(), before);
    assert!(!backup(&app.opencode()).exists());
}

#[test]
fn a_file_that_is_not_ours_is_never_overwritten_and_blocks_the_whole_setup() {
    let app = Home::new("foreign");
    std::fs::create_dir_all(app.pi().parent().unwrap()).unwrap();
    std::fs::write(app.pi(), "// my own extension\n").unwrap();
    let output = app.run(&[
        "--setup",
        "--harness",
        "opencode",
        "--harness",
        "pi",
        "--apply",
    ]);
    assert!(!output.status.success());
    assert_eq!(
        std::fs::read_to_string(app.pi()).unwrap(),
        "// my own extension\n"
    );
    assert!(
        !app.opencode().exists(),
        "nothing is written when one file is refused"
    );
}

#[test]
fn deselecting_removes_only_that_bridge_and_unsetup_removes_the_rest() {
    let app = Home::new("remove");
    app.ok(&[
        "--setup",
        "--harness",
        "opencode",
        "--harness",
        "pi",
        "--apply",
    ]);
    // A selection of bridges replaces the previous one.
    app.ok(&["--setup", "--harness", "pi", "--apply"]);
    assert!(!app.opencode().exists() && app.pi().exists());
    assert!(
        backup(&app.opencode()).exists(),
        "a removed file stays recoverable"
    );
    // A setup naming no bridge leaves installed bridges alone.
    app.ok(&["--setup", "--harness", "claude", "--apply"]);
    assert!(app.pi().exists());
    // ...unless it states that its selection is complete, as the installer pages do.
    app.ok(&[
        "--setup",
        "--harness",
        "claude",
        "--replace-bridges",
        "--apply",
    ]);
    assert!(
        !app.pi().exists(),
        "an empty complete selection removes the bridge"
    );
    app.ok(&["--setup", "--harness", "pi", "--apply"]);
    assert!(app.pi().exists());
    app.ok(&["--unsetup", "--apply"]);
    assert!(!app.pi().exists());
}

#[test]
fn a_bridge_the_user_edited_is_left_in_place_on_removal() {
    let app = Home::new("edited");
    app.ok(&["--setup", "--harness", "goose", "--apply"]);
    let mut text = std::fs::read_to_string(app.goose()).unwrap();
    text.push('\n');
    std::fs::write(app.goose(), &text).unwrap();
    let report = app.ok(&["--unsetup", "--apply"]);
    assert_eq!(std::fs::read_to_string(app.goose()).unwrap(), text);
    assert!(report.contains("left in place"), "{report}");
}

#[test]
fn a_failed_write_restores_the_earlier_ones() {
    let app = Home::new("rollback");
    // `.agents` is a file, so the Goose plugin directory cannot be created.
    std::fs::write(app.root.join("home/.agents"), "x").unwrap();
    let output = app.run(&[
        "--setup",
        "--harness",
        "opencode",
        "--harness",
        "goose",
        "--apply",
    ]);
    assert!(!output.status.success());
    assert!(
        !app.opencode().exists(),
        "the earlier file is removed again"
    );
}

#[test]
fn setup_after_removal_by_unsetup_can_reinstall_over_the_recorded_file() {
    let app = Home::new("update");
    app.ok(&["--setup", "--harness", "opencode", "--apply"]);
    // An older aHerald left a different text in the receipt: the recorded file is still ours.
    let receipt = app.root.join("aherald-setup.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&receipt).unwrap()).unwrap();
    json["bridges"][0]["text"] = "// older bridge\n".into();
    std::fs::write(&receipt, json.to_string()).unwrap();
    std::fs::write(app.opencode(), "// older bridge\n").unwrap();
    app.ok(&["--setup", "--harness", "opencode", "--apply"]);
    assert!(
        std::fs::read_to_string(app.opencode())
            .unwrap()
            .contains("AHERALD_EXE")
    );
    assert_eq!(
        std::fs::read_to_string(backup(&app.opencode())).unwrap(),
        "// older bridge\n"
    );
}

#[test]
fn bridge_selections_are_only_valid_with_setup_and_never_twice() {
    let app = Home::new("args");
    for args in [
        &["--unsetup", "--harness", "pi"][..],
        &["--setup", "--harness", "pi", "--harness", "pi"],
        &["--setup", "--harness", "crush"],
    ] {
        let output = app.run(args);
        assert!(!output.status.success(), "{args:?}");
    }
}

#[test]
fn the_settings_report_lists_which_bridges_are_installed() {
    let app = Home::new("report");
    let report = app.ok(&["--setup-settings"]);
    for line in [
        "opencode_bridge=false",
        "pi_bridge=false",
        "goose_bridge=false",
    ] {
        assert!(report.lines().any(|l| l == line), "{report}");
    }
    app.ok(&[
        "--setup",
        "--harness",
        "opencode",
        "--harness",
        "goose",
        "--apply",
    ]);
    let report = app.ok(&["--setup-settings"]);
    for line in [
        "opencode_bridge=true",
        "pi_bridge=false",
        "goose_bridge=true",
    ] {
        assert!(report.lines().any(|l| l == line), "{report}");
    }
}

/// The Windows installer asks from a temporary copy of the program, naming the installed
/// configuration: the receipt that sits beside that configuration is the one to read.
#[test]
fn the_report_finds_the_receipt_beside_a_named_configuration_from_another_copy() {
    let app = Home::new("elsewhere");
    app.ok(&["--setup", "--harness", "pi", "--apply"]);
    let temporary = app.root.join("tmp");
    std::fs::create_dir_all(&temporary).unwrap();
    let copy = temporary.join(app.exe.file_name().unwrap());
    std::fs::copy(&app.exe, &copy).unwrap();
    let config = app.root.join("config.toml");
    let home = app.root.join("home");
    let output = support::run_bounded(
        Command::new(&copy)
            .args(["--setup-settings", "--settings-file"])
            .arg(&config)
            .env("HOME", &home)
            .env("USERPROFILE", &home),
        std::time::Duration::from_secs(20),
    )
    .unwrap();
    let report = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{report}");
    assert!(report.lines().any(|l| l == "pi_bridge=true"), "{report}");
    assert!(
        report.lines().any(|l| l == "opencode_bridge=false"),
        "{report}"
    );
}
