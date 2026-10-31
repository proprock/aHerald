use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::{Command, Output},
};

struct Install {
    root: PathBuf,
    exe: PathBuf,
}
impl Install {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "aherald setup Путь $HOME `tick & apostrophe'-{name}-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join(if cfg!(windows) {
            "aherald.exe"
        } else {
            "aherald"
        });
        std::fs::copy(env!("CARGO_BIN_EXE_aherald"), &exe).unwrap();
        Self { root, exe }
    }
    fn run(&self, args: &[&str]) -> Output {
        Command::new(&self.exe)
            .args(args)
            .env("CODEX_HOME", self.root.join("codex"))
            .env("CLAUDE_CONFIG_DIR", self.root.join("claude"))
            .bounded_output()
    }
    fn file(&self, agent: &str) -> PathBuf {
        self.root.join(if agent == "claude" {
            "claude/settings.json"
        } else {
            "codex/hooks.json"
        })
    }
    fn read(&self, agent: &str) -> Value {
        serde_json::from_slice(&std::fs::read(self.file(agent)).unwrap()).unwrap()
    }
    fn ok(&self, args: &[&str]) -> Output {
        let output = self.run(args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}
impl Drop for Install {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn setup_previews_then_installs_only_selected_harness_and_is_idempotent() {
    let app = Install::new("selected");
    let args = ["--setup", "--harness", "claude"];
    let preview = app.ok(&args);
    assert!(String::from_utf8_lossy(&preview.stdout).contains("Dry run"));
    assert!(!app.file("claude").exists());
    assert!(!app.root.join("aherald-setup.json").exists());
    app.ok(&["--setup", "--harness", "claude", "--apply"]);
    assert!(!app.file("codex").exists());
    let settings = app.read("claude");
    let hook = &settings["hooks"]["Stop"][0]["hooks"][0];
    assert_eq!(hook["command"], app.exe.to_str().unwrap());
    assert_eq!(
        hook["args"],
        json!(["--hook", "--agent", "claude", "--resident"])
    );
    assert_eq!(hook["timeout"], 3);
    let before = std::fs::read(app.file("claude")).unwrap();
    app.ok(&["--setup", "--harness", "claude", "--apply"]);
    assert_eq!(std::fs::read(app.file("claude")).unwrap(), before);
    assert!(
        !app.file("claude")
            .with_file_name("settings.json.aherald-backup")
            .exists()
    );
}

#[test]
fn malformed_second_harness_prevents_all_writes() {
    let app = Install::new("preflight");
    std::fs::create_dir_all(app.file("claude").parent().unwrap()).unwrap();
    std::fs::write(app.file("claude"), "{invalid").unwrap();
    let out = app.run(&[
        "--setup",
        "--harness",
        "codex",
        "--harness",
        "claude",
        "--apply",
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("not valid JSON"));
    assert!(!app.file("codex").exists());
    assert_eq!(
        std::fs::read_to_string(app.file("claude")).unwrap(),
        "{invalid"
    );
    assert!(!app.root.join("aherald-setup.json").exists());
}

#[test]
fn removal_uses_saved_paths_and_preserves_foreign_hooks_and_settings() {
    let app = Install::new("remove");
    let original = json!({"theme":"dark", "hooks":{"Stop":[{"hooks":[{"type":"command","command":"my-notifier"}, {"type":"command", "command":"echo aherald --hook"}]}]}});
    std::fs::create_dir_all(app.file("claude").parent().unwrap()).unwrap();
    std::fs::write(app.file("claude"), original.to_string()).unwrap();
    app.ok(&["--setup", "--harness", "claude", "--apply"]);
    let mut configured = app.read("claude");
    configured["hooks"]["Stop"].as_array_mut().unwrap().push(json!({"hooks":[{
        "type":"command", "command":"C:/other/aherald.exe", "args":["--hook","--agent","claude","--resident"]
    }]}));
    std::fs::write(app.file("claude"), configured.to_string()).unwrap();
    // The installer can be launched later without the original harness environment.
    let output = Command::new(&app.exe)
        .args(["--unsetup", "--apply"])
        .env("CLAUDE_CONFIG_DIR", app.root.join("elsewhere"))
        .bounded_output();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after = app.read("claude");
    assert_eq!(after["theme"], "dark");
    let hooks = after["hooks"].as_object().unwrap();
    assert_eq!(hooks.len(), 1);
    let stop = hooks["Stop"].as_array().unwrap();
    assert_eq!(stop.len(), 2);
    assert_eq!(stop[0]["hooks"][0]["command"], "my-notifier");
    assert_eq!(stop[0]["hooks"][1]["command"], "echo aherald --hook");
    assert_eq!(stop[1]["hooks"][0]["command"], "C:/other/aherald.exe");
    app.ok(&["--unsetup", "--apply"]);
}

#[test]
fn setup_rejects_missing_unknown_duplicate_harness_and_runtime_options() {
    let app = Install::new("invalid");
    for args in [
        vec!["--setup"],
        vec!["--setup", "--harness", "other"],
        vec!["--setup", "--harness", "claude", "--harness", "claude"],
        vec!["--harness", "claude"],
        vec!["--setup", "--harness", "claude", "--resident"],
        vec!["--unsetup", "--harness", "claude"],
    ] {
        assert!(!app.run(&args).status.success(), "{args:?}");
    }
}

#[test]
fn changing_selection_removes_deselected_bindings_and_keeps_backups() {
    let app = Install::new("selection-change");
    app.ok(&[
        "--setup",
        "--harness",
        "codex",
        "--harness",
        "claude",
        "--apply",
    ]);
    let original = std::fs::read(app.file("codex")).unwrap();
    app.ok(&["--setup", "--harness", "claude", "--apply"]);
    assert!(app.read("codex").get("hooks").is_none());
    assert_eq!(
        std::fs::read(
            app.file("codex")
                .with_file_name("hooks.json.aherald-backup")
        )
        .unwrap(),
        original
    );
    assert!(app.read("claude")["hooks"]["Stop"].is_array());
    app.ok(&["--unsetup", "--apply"]);
    assert!(app.read("claude").get("hooks").is_none());
}
#[cfg(windows)]
#[test]
fn generated_codex_command_for_a_special_character_path_returns_the_hook_response() {
    use std::{
        io::Write,
        process::Stdio,
        time::{Duration, Instant},
    };
    let app = Install::new("codex-execution");
    app.ok(&["--setup", "--harness", "codex", "--apply"]);
    std::fs::write(
        app.root.join("config.toml"),
        "[codex]\nhooks_enabled=false\n",
    )
    .unwrap();
    let settings = app.read("codex");
    let command = settings["hooks"]["Stop"][0]["hooks"][0]["commandWindows"]
        .as_str()
        .unwrap();
    let shell = "powershell.exe";
    let mut child = Command::new(shell)
        .args(["-NoProfile", "-NonInteractive", "-Command", command])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(include_bytes!("fixtures/stop.json"))
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("{shell} exceeded deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let out = finish(child);
    assert!(
        out.status.success(),
        "{shell}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        out.stderr.is_empty(),
        "{shell}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "{}", "{shell}");
}

#[test]
fn generated_claude_exec_binding_completes_without_a_shell_or_resident() {
    use std::{io::Write, process::Stdio};
    let app = Install::new("claude-execution");
    app.ok(&["--setup", "--harness", "claude", "--apply"]);
    std::fs::write(
        app.root.join("config.toml"),
        "[claude]\nhooks_enabled=false\n",
    )
    .unwrap();
    let settings = app.read("claude");
    let hook = &settings["hooks"]["Stop"][0]["hooks"][0];
    let mut child = Command::new(hook["command"].as_str().unwrap())
        .args(
            hook["args"]
                .as_array()
                .unwrap()
                .iter()
                .map(|a| a.as_str().unwrap()),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"hook_event_name":"Stop","session_id":"fixture"}"#)
        .unwrap();
    let out = finish(child);
    assert!(out.status.success());
    assert!(out.stdout.is_empty());
    assert!(
        out.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[cfg(windows)]
#[test]
fn receipt_write_failure_rolls_back_hook_files() {
    use std::os::windows::fs::OpenOptionsExt;
    let app = Install::new("rollback");
    app.ok(&["--setup", "--harness", "claude", "--apply"]);
    let config_path = app.root.join("config.toml");
    let config_before = "# preserve on transaction rollback\n[codex.usage]\nenabled = false\n";
    std::fs::write(&config_path, config_before).unwrap();
    let before = std::fs::read(app.file("claude")).unwrap();
    let receipt = app.root.join("aherald-setup.json");
    let state = std::fs::read(&receipt).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&receipt)
        .unwrap();
    let out = app.run(&[
        "--setup",
        "--harness",
        "claude",
        "--harness",
        "codex",
        "--codex-usage",
        "true",
        "--apply",
    ]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("earlier writes were restored"));
    assert_eq!(std::fs::read(app.file("claude")).unwrap(), before);
    assert_eq!(std::fs::read(&receipt).unwrap(), state);
    assert_eq!(
        std::fs::read_to_string(&config_path).unwrap(),
        config_before
    );
    assert!(!app.file("codex").exists());
    drop(lock);
}
#[test]
fn app_bundle_setup_keeps_the_receipt_outside_signed_contents() {
    let app = Install::new("bundle");
    let bundle_exe = app
        .root
        .join("aHerald.app/Contents/MacOS")
        .join(app.exe.file_name().unwrap());
    std::fs::create_dir_all(bundle_exe.parent().unwrap()).unwrap();
    std::fs::copy(&app.exe, &bundle_exe).unwrap();
    let out = Command::new(&bundle_exe)
        .args(["--setup", "--harness", "claude", "--apply"])
        .env("HOME", &app.root)
        .env("USERPROFILE", &app.root)
        .env("CLAUDE_CONFIG_DIR", app.root.join("claude"))
        .bounded_output();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        app.root
            .join("Library/Application Support/aHerald/aherald-setup.json")
            .exists()
    );
    assert!(
        !bundle_exe
            .parent()
            .unwrap()
            .join("aherald-setup.json")
            .exists()
    );
}
#[test]
fn an_older_app_bundle_cannot_uninstall_another_installations_saved_bindings() {
    let app = Install::new("bundle-owner");
    let mut executables = Vec::new();
    for name in ["First.app", "Second.app"] {
        let exe = app
            .root
            .join(name)
            .join("Contents/MacOS")
            .join(app.exe.file_name().unwrap());
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::copy(&app.exe, &exe).unwrap();
        let out = Command::new(&exe)
            .args(["--setup", "--harness", "claude", "--apply"])
            .env("HOME", &app.root)
            .env("USERPROFILE", &app.root)
            .env("CLAUDE_CONFIG_DIR", app.root.join("claude"))
            .bounded_output();
        assert!(out.status.success());
        executables.push(exe);
    }
    let before = std::fs::read(app.file("claude")).unwrap();
    let out = Command::new(&executables[0])
        .args(["--unsetup", "--apply"])
        .env("HOME", &app.root)
        .env("USERPROFILE", &app.root)
        .bounded_output();
    assert!(
        !out.status.success(),
        "another executable owns the saved installation"
    );
    assert_eq!(std::fs::read(app.file("claude")).unwrap(), before);
}

trait BoundedOutput {
    fn bounded_output(&mut self) -> Output;
}
impl BoundedOutput for Command {
    fn bounded_output(&mut self) -> Output {
        use std::process::Stdio;
        finish(
            self.stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        )
    }
}
fn finish(mut child: std::process::Child) -> Output {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(5);
    while child.try_wait().unwrap().is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("native setup/hook exceeded deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    child.wait_with_output().unwrap()
}

#[test]
fn installer_reads_usage_defaults_without_creating_config() {
    let app = Install::new("usage-read");
    let out = app.ok(&["--setup-settings"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "codex_usage=false\nclaude_usage=false"
    );
    assert!(!app.root.join("config.toml").exists());
}

#[test]
fn installer_changes_only_selected_usage_and_preserves_comments_and_sources() {
    let app = Install::new("usage-edit");
    let path = app.root.join("config.toml");
    let before = "# personal profile\n[notifications]\nscale = 1.5\n[codex.usage]\nenabled = false # keep this note\nrefresh_interval_sec = 60\n[claude.usage]\nenabled = true\nsource = 'statusline'\n";
    std::fs::write(&path, before).unwrap();
    let out = app.ok(&["--setup-settings"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "codex_usage=false\nclaude_usage=true"
    );
    let args = ["--setup", "--harness", "codex", "--codex-usage", "true"];
    app.ok(&args);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    app.ok(&[
        "--setup",
        "--harness",
        "codex",
        "--codex-usage",
        "true",
        "--apply",
    ]);
    let after = std::fs::read_to_string(&path).unwrap();
    assert_eq!(
        after,
        before.replacen("enabled = false", "enabled = true", 1)
    );
    assert_eq!(
        std::fs::read_to_string(path.with_file_name("config.toml.aherald-backup")).unwrap(),
        before
    );
    app.ok(&["--unsetup", "--apply"]);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), after);
}

#[test]
fn installer_invalid_config_prevents_hook_writes_and_usage_changes() {
    let app = Install::new("usage-invalid");
    let path = app.root.join("config.toml");
    std::fs::write(&path, "[codex.usage]\nenabled = 'bad'\n").unwrap();
    let output = app.run(&[
        "--setup",
        "--harness",
        "claude",
        "--codex-usage",
        "true",
        "--apply",
    ]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid configuration"));
    assert!(!app.file("claude").exists());
    assert!(!app.root.join("aherald-setup.json").exists());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "[codex.usage]\nenabled = 'bad'\n"
    );
}

#[test]
fn installer_usage_selection_without_harness_creates_defaults_and_is_idempotent() {
    let app = Install::new("usage-create");
    let args = [
        "--unsetup",
        "--codex-usage",
        "true",
        "--claude-usage",
        "false",
        "--apply",
    ];
    app.ok(&args);
    let path = app.root.join("config.toml");
    let before = std::fs::read_to_string(&path).unwrap();
    let config = aherald::config::Config::parse(&before, &app.root).unwrap();
    assert!(config.codex.usage.enabled);
    assert!(!config.claude.usage.enabled);
    app.ok(&args);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
    assert!(!path.with_file_name("config.toml.aherald-backup").exists());
}

#[test]
fn installer_settings_preview_uses_explicit_target_file_and_rejects_invalid_flags() {
    let app = Install::new("usage-preview-target");
    let path = app.root.join("elsewhere/config.toml");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "[codex.usage]\nenabled = true\n").unwrap();
    let output = app.ok(&[
        "--setup-settings",
        "--settings-file",
        path.to_str().unwrap(),
    ]);
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "codex_usage=true\nclaude_usage=false"
    );
    for args in [
        vec!["--codex-usage", "true"],
        vec!["--setup", "--harness", "codex", "--claude-usage", "yes"],
        vec![
            "--setup",
            "--harness",
            "codex",
            "--codex-usage",
            "true",
            "--codex-usage",
            "false",
        ],
        vec!["--setup-settings", "--apply"],
        vec!["--setup-settings", "--claude-usage", "true"],
    ] {
        assert!(!app.run(&args).status.success(), "{args:?}");
    }
}
