//! The Windows `statusLine` is a readable command that Git Bash and PowerShell read alike
//! (AH-211). Each test runs the generated command as Claude Code does, not its text.
#![cfg(windows)]

mod support;

use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::{Command, Output, Stdio},
    time::Duration,
};

const WRAPPED: &str = "powershell.exe -NoProfile -NonInteractive -Command \"[Console]::OpenStandardInput().CopyTo([Console]::OpenStandardOutput()); exit 7\"";
/// Not JSON, so nothing is sent to a running resident; the wrapped command still gets it.
const PAYLOAD: &str = "не JSON 🦀 Проверка \u{1b}[31mred\u{1b}[0m\n";
const SETUP: [&str; 6] = [
    "--setup",
    "--harness",
    "claude",
    "--claude-context",
    "true",
    "--apply",
];

struct Install {
    _base: support::TempDir,
    root: PathBuf,
    exe: PathBuf,
}

impl Install {
    fn new(dir_name: &str, companion: bool) -> Self {
        let base = support::TempDir::new("statusline-windows");
        let root = base.join(dir_name);
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join("aherald.exe");
        std::fs::copy(env!("CARGO_BIN_EXE_aherald"), &exe).unwrap();
        if companion {
            std::fs::copy(
                env!("CARGO_BIN_EXE_aherald-cli"),
                root.join("aherald-cli.exe"),
            )
            .unwrap();
        }
        Self {
            _base: base,
            root,
            exe,
        }
    }

    fn settings_file(&self) -> PathBuf {
        self.root.join("claude/settings.json")
    }

    fn run(&self, args: &[&str]) -> Output {
        let output = support::output(
            Command::new(&self.exe)
                .args(args)
                .env("CODEX_HOME", self.root.join("codex"))
                .env("CLAUDE_CONFIG_DIR", self.root.join("claude")),
        );
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }

    fn settings(&self) -> Value {
        serde_json::from_slice(&std::fs::read(self.settings_file()).unwrap()).unwrap()
    }

    fn command(&self) -> String {
        self.settings()["statusLine"]["command"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// Gives Claude Code a status line of the user's own, which setup must keep and restore.
    fn user_statusline(&self) -> Value {
        let original =
            json!({"theme":"dark","statusLine":{"type":"command","command":WRAPPED,"padding":2}});
        std::fs::create_dir_all(self.settings_file().parent().unwrap()).unwrap();
        std::fs::write(self.settings_file(), original.to_string()).unwrap();
        original
    }

    fn receipt(&self) -> PathBuf {
        self.root.join("aherald-setup.json")
    }
}

/// Runs `command` the way Claude Code does: through Git Bash, or else PowerShell.
fn run_command(command: &str, bash: bool) -> Output {
    let mut launcher = if bash {
        let mut cmd = Command::new(
            PathBuf::from(std::env::var_os("ProgramFiles").unwrap()).join("Git/bin/bash.exe"),
        );
        cmd.args(["-c", command]);
        cmd
    } else {
        let mut cmd = Command::new("powershell.exe");
        // PowerShell maps any nonzero native exit code to 1 unless it is passed on, as
        // `statusline::statusline_shell` does; that remapping is outside aHerald's control.
        let script = format!("{command}\nif ($null -ne $LASTEXITCODE) {{ exit $LASTEXITCODE }}");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        cmd
    };
    let child = launcher
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Shell startup, the companion, aHerald and the user's PowerShell all start here.
    support::capture_bounded(child, PAYLOAD.as_bytes(), Duration::from_secs(30)).unwrap()
}

fn assert_relayed(output: &Output) {
    assert_eq!(
        output.stdout,
        PAYLOAD.as_bytes(),
        "stderr={:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(7));
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn check_plain_path(bash: bool) {
    let app = Install::new("aherald-plain", true);
    let original = app.user_statusline();
    app.run(&SETUP);
    let command = app.command();
    let expected = format!(
        "{}/aherald-cli.exe --statusline",
        app.root.display().to_string().replace('\\', "/")
    );
    assert_eq!(command, expected);
    assert!(!app.root.join("statusline.ps1").exists());
    assert_relayed(&run_command(&command, bash));
    app.run(&["--unsetup", "--apply"]);
    assert_eq!(app.settings(), original);
}

#[test]
fn a_plain_path_gets_a_bare_companion_command_that_relays_in_powershell() {
    check_plain_path(false);
}

#[test]
#[ignore = "requires installed Git Bash"]
fn a_plain_path_gets_a_bare_companion_command_that_relays_in_git_bash() {
    check_plain_path(true);
}

fn check_spaced_path(bash: bool) {
    let app = Install::new("aherald spaced 'path' & Путь", true);
    let original = app.user_statusline();
    app.run(&SETUP);
    let command = app.command();
    assert!(
        command
            .starts_with("powershell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File \""),
        "{command}"
    );
    assert!(command.ends_with("/statusline.ps1\""), "{command}");
    assert!(!command.contains("--then"), "{command}");
    let script = app.root.join("statusline.ps1");
    assert_eq!(
        std::fs::read_to_string(&script).unwrap(),
        aherald::statusline_launch::SCRIPT_TEXT
    );
    assert_relayed(&run_command(&command, bash));
    let after_setup = app.settings();
    app.run(&SETUP);
    assert_eq!(app.settings(), after_setup);
    app.run(&["--unsetup", "--apply"]);
    assert_eq!(app.settings(), original);
    assert!(!script.exists(), "the script outlived the uninstall");
}

#[test]
fn a_path_with_spaces_gets_a_readable_script_that_relays_in_powershell() {
    check_spaced_path(false);
}

#[test]
#[ignore = "requires installed Git Bash"]
fn a_path_with_spaces_gets_a_readable_script_that_relays_in_git_bash() {
    check_spaced_path(true);
}

#[test]
fn the_previous_encoded_command_is_replaced_and_the_original_is_restored_exactly() {
    let app = Install::new("aherald-upgrade", true);
    let original = app.user_statusline();
    app.run(&SETUP);
    // Rewrite the install into what an earlier version left: its command in both the
    // settings file and the receipt.
    let old = "powershell.exe -NoProfile -NonInteractive -Command \"& ([scriptblock]::Create((-join [char[]](36,115)))); # aherald --statusline\"";
    let mut settings = app.settings();
    settings["statusLine"]["command"] = old.into();
    std::fs::write(app.settings_file(), settings.to_string()).unwrap();
    let mut receipt: Value =
        serde_json::from_slice(&std::fs::read(app.receipt()).unwrap()).unwrap();
    receipt["statusline"]["installed"]["command"] = old.into();
    std::fs::write(app.receipt(), receipt.to_string()).unwrap();

    app.run(&SETUP);
    let command = app.command();
    assert!(
        command.ends_with("/aherald-cli.exe --statusline"),
        "{command}"
    );
    assert_eq!(app.settings()["statusLine"]["padding"], 2);
    assert_relayed(&run_command(&command, false));
    app.run(&["--unsetup", "--apply"]);
    assert_eq!(app.settings(), original);
}

#[test]
fn a_hostile_path_keeps_the_encoded_command_and_still_relays() {
    let app = Install::new("aherald $HOME `tick`", false);
    app.user_statusline();
    app.run(&SETUP);
    let command = app.command();
    assert!(
        command.starts_with("powershell.exe -NoProfile"),
        "{command}"
    );
    assert!(!app.root.join("statusline.ps1").exists());
    assert_relayed(&run_command(&command, false));
}

#[test]
fn without_then_the_wrapped_command_comes_from_the_setup_receipt() {
    let app = Install::new("aherald-receipt", true);
    app.user_statusline();
    app.run(&SETUP);
    let run = || {
        let child = Command::new(&app.exe)
            .arg("--statusline")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        support::capture_bounded(child, PAYLOAD.as_bytes(), Duration::from_secs(30)).unwrap()
    };
    assert_relayed(&run());

    // A corrupt receipt must not fail the status line: a plain line and a note on stderr.
    let good = std::fs::read(app.receipt()).unwrap();
    std::fs::write(app.receipt(), "not json").unwrap();
    let output = run();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(!output.stderr.is_empty());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("panicked"));

    std::fs::remove_file(app.receipt()).unwrap();
    let output = run();
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());

    std::fs::write(app.receipt(), good).unwrap();
    assert_relayed(&run());
}

fn check_install_statusline(dir_name: &str, script: bool) {
    let app = Install::new(dir_name, true);
    let original = app.user_statusline();
    let before = std::fs::read(app.settings_file()).unwrap();

    let dry = app.run(&["--install-statusline"]);
    assert!(String::from_utf8_lossy(&dry.stdout).contains("Dry run"));
    assert_eq!(std::fs::read(app.settings_file()).unwrap(), before);
    assert!(!app.receipt().exists());

    app.run(&["--install-statusline", "--apply"]);
    let command = app.command();
    assert_eq!(command.starts_with("powershell "), script, "{command}");
    assert_eq!(app.root.join("statusline.ps1").exists(), script);
    assert_eq!(app.settings()["statusLine"]["padding"], 2);
    assert_relayed(&run_command(&command, false));
    let installed = std::fs::read(app.settings_file()).unwrap();
    app.run(&["--install-statusline", "--apply"]);
    assert_eq!(std::fs::read(app.settings_file()).unwrap(), installed);

    let dry = app.run(&["--uninstall-statusline"]);
    assert!(String::from_utf8_lossy(&dry.stdout).contains("Dry run"));
    assert_eq!(std::fs::read(app.settings_file()).unwrap(), installed);
    app.run(&["--uninstall-statusline", "--apply"]);
    assert_eq!(app.settings(), original);
    assert!(!app.root.join("statusline.ps1").exists());
}

#[test]
fn install_statusline_wraps_the_users_command_and_uninstall_restores_it_exactly() {
    check_install_statusline("aherald-install-plain", false);
}

#[test]
fn install_statusline_in_a_path_with_spaces_writes_and_removes_the_script() {
    check_install_statusline("aherald install spaced", true);
}

#[test]
fn install_statusline_without_a_user_command_adds_and_removes_the_entry() {
    let app = Install::new("aherald-install-bare", true);
    app.run(&["--install-statusline", "--apply"]);
    let command = app.command();
    assert!(
        command.ends_with("/aherald-cli.exe --statusline"),
        "{command}"
    );
    // Nothing of the user's to run: the plain line prints nothing for a payload that is no JSON.
    let output = run_command(&command, false);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    app.run(&["--uninstall-statusline", "--apply"]);
    assert!(app.settings().get("statusLine").is_none());
}

#[test]
fn install_statusline_replaces_the_previous_encoded_command_and_keeps_the_original() {
    let app = Install::new("aherald-install-upgrade", true);
    let original = app.user_statusline();
    app.run(&SETUP);
    let old = "powershell.exe -NoProfile -NonInteractive -Command \"& ([scriptblock]::Create((-join [char[]](36,115)))); # aherald --statusline\"";
    let mut settings = app.settings();
    settings["statusLine"]["command"] = old.into();
    std::fs::write(app.settings_file(), settings.to_string()).unwrap();
    let mut receipt: Value =
        serde_json::from_slice(&std::fs::read(app.receipt()).unwrap()).unwrap();
    receipt["statusline"]["installed"]["command"] = old.into();
    std::fs::write(app.receipt(), receipt.to_string()).unwrap();

    app.run(&["--install-statusline", "--apply"]);
    assert!(app.command().ends_with("/aherald-cli.exe --statusline"));
    assert_relayed(&run_command(&app.command(), false));
    app.run(&["--uninstall-statusline", "--apply"]);
    assert_eq!(app.settings()["statusLine"], original["statusLine"]);
}

#[test]
fn install_statusline_leaves_the_hooks_of_an_earlier_setup_alone() {
    let app = Install::new("aherald-install-hooks", true);
    app.user_statusline();
    app.run(&["--setup", "--harness", "claude", "--apply"]);
    let hooks = app.settings()["hooks"].clone();
    assert!(hooks.is_object());
    app.run(&["--install-statusline", "--apply"]);
    assert_eq!(app.settings()["hooks"], hooks);
    app.run(&["--uninstall-statusline", "--apply"]);
    assert_eq!(app.settings()["hooks"], hooks);
    assert_eq!(app.settings()["statusLine"]["command"], WRAPPED);
}
