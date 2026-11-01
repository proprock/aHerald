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
        "stdout={:?}, stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
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
    let mut child = spawn_fixture(|| {
        Command::new(hook["command"].as_str().unwrap())
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
    })
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
        "--claude-context",
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
            spawn_fixture(|| {
                self.stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
            })
            .unwrap(),
        )
    }
}
fn spawn_fixture(
    spawn: impl FnMut() -> std::io::Result<std::process::Child>,
) -> std::io::Result<std::process::Child> {
    spawn_fixture_until(
        std::time::Instant::now() + std::time::Duration::from_secs(1),
        spawn,
    )
}

fn spawn_fixture_until(
    deadline: std::time::Instant,
    mut spawn: impl FnMut() -> std::io::Result<std::process::Child>,
) -> std::io::Result<std::process::Child> {
    loop {
        match spawn() {
            // Linux rejects exec while any writable handle remains open. Parallel
            // fixture copies and process launches can briefly overlap those handles.
            Err(error)
                if cfg!(target_os = "linux")
                    && error.raw_os_error() == Some(26)
                    && std::time::Instant::now() < deadline =>
            {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            result => return result,
        }
    }
}

fn finish(child: std::process::Child) -> Output {
    finish_with_timeout(child, std::time::Duration::from_secs(5))
}

fn finish_with_timeout(mut child: std::process::Child, timeout: std::time::Duration) -> Output {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + timeout;
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
#[cfg(windows)]
fn generated_statusline_preserves_unicode_input_output_and_exit_code() {
    check_statusline_execution(false);
}

#[test]
#[cfg(windows)]
#[ignore = "requires installed Git Bash"]
fn generated_statusline_runs_through_git_bash() {
    check_statusline_execution(true);
}

#[cfg(windows)]
fn check_statusline_execution(bash: bool) {
    use std::{io::Write, process::Stdio};
    let app = Install::new("statusline-execution");
    let original = "powershell.exe -NoProfile -NonInteractive -Command \"[Console]::OpenStandardInput().CopyTo([Console]::OpenStandardOutput()); exit 7\"";
    std::fs::create_dir_all(app.file("claude").parent().unwrap()).unwrap();
    std::fs::write(
        app.file("claude"),
        json!({"statusLine":{"type":"command","command":original}}).to_string(),
    )
    .unwrap();
    app.ok(&[
        "--setup",
        "--harness",
        "claude",
        "--claude-context",
        "true",
        "--apply",
    ]);
    let command = app.read("claude")["statusLine"]["command"]
        .as_str()
        .unwrap()
        .to_owned();
    let payload = "{\"session_id\":\"fixture\",\"session_name\":\"Проверка 🦀 $HOME `tick\",\"context_window\":{\"current_usage\":{\"input_tokens\":400001}}}";
    // Run the generated native command directly: an extra PowerShell shell
    // would transcode its stdout and remap nonzero native exits to 1.
    let script = command
        .strip_prefix("powershell.exe -NoProfile -NonInteractive -Command \"")
        .unwrap()
        .strip_suffix('"')
        .unwrap();
    let mut launcher = if bash {
        let mut cmd = Command::new(
            PathBuf::from(std::env::var_os("ProgramFiles").unwrap()).join("Git/bin/bash.exe"),
        );
        cmd.args(["-c", &command]);
        cmd
    } else {
        let mut cmd = Command::new("powershell.exe");
        // UTF-8 input with a preamble reproduces Windows CI's console encoding.
        let script = format!("[Console]::InputEncoding=[Text.UTF8Encoding]::new($true); {script}");
        cmd.args(["-NoProfile", "-NonInteractive", "-Command", &script]);
        cmd
    };
    let mut child = launcher
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    // This fixture starts PowerShell, aHerald, Git Bash and the user's PowerShell.
    // Give shell startup a bounded budget independent of the native hook deadline.
    let output = finish_with_timeout(child, std::time::Duration::from_secs(20));
    assert_eq!(
        output.status.code(),
        Some(7),
        "stdout={:?}, stderr={:?}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, payload.as_bytes());
    assert!(
        output.stderr.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn installer_preflight_uses_the_destination_installation_receipt() {
    let app = Install::new("destination");
    app.ok(&[
        "--setup",
        "--harness",
        "claude",
        "--claude-context",
        "true",
        "--apply",
    ]);
    let before = std::fs::read(app.file("claude")).unwrap();
    let preview = Install::new("preflight");
    let out = Command::new(&preview.exe)
        .args([
            "--setup",
            "--harness",
            "claude",
            "--claude-context",
            "true",
            "--setup-executable",
        ])
        .arg(&app.exe)
        .env("CLAUDE_CONFIG_DIR", app.root.join("claude"))
        .bounded_output();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(std::fs::read(app.file("claude")).unwrap(), before);
    assert!(!preview.root.join("aherald-setup.json").exists());
}

#[test]
fn invalid_statusline_prevents_all_setup_writes() {
    let app = Install::new("invalid-statusline");
    std::fs::create_dir_all(app.file("claude").parent().unwrap()).unwrap();
    let before = json!({"theme":"dark", "statusLine":{"type":"command","command":123}}).to_string();
    std::fs::write(app.file("claude"), &before).unwrap();
    let out = app.run(&[
        "--setup",
        "--harness",
        "codex",
        "--harness",
        "claude",
        "--claude-context",
        "true",
        "--apply",
    ]);
    assert!(!out.status.success());
    assert_eq!(std::fs::read_to_string(app.file("claude")).unwrap(), before);
    assert!(!app.file("codex").exists());
    assert!(!app.root.join("config.toml").exists());
    assert!(!app.root.join("aherald-setup.json").exists());
}

#[test]
fn context_setup_installs_statusline_once_and_restores_the_original() {
    let app = Install::new("context-statusline");
    let original = json!({"theme":"dark", "statusLine":{"type":"command", "command":"echo original", "padding":2}});
    std::fs::create_dir_all(app.file("claude").parent().unwrap()).unwrap();
    std::fs::write(app.file("claude"), original.to_string()).unwrap();
    let args = ["--setup", "--harness", "claude", "--claude-context", "true"];
    app.ok(&args);
    assert_eq!(app.read("claude"), original);
    let mut apply = args.to_vec();
    apply.push("--apply");
    app.ok(&apply);
    let installed = app.read("claude");
    assert_ne!(
        installed["statusLine"]["command"],
        original["statusLine"]["command"]
    );
    assert_eq!(installed["statusLine"]["padding"], 2);
    assert_eq!(installed["theme"], "dark");
    app.ok(&apply);
    assert_eq!(app.read("claude"), installed);
    app.ok(&["--unsetup", "--apply"]);
    assert_eq!(app.read("claude"), original);
}

#[test]
fn context_setup_removes_only_its_unchanged_wrapper_on_disable() {
    let app = Install::new("context-statusline-disable");
    app.ok(&[
        "--setup",
        "--harness",
        "claude",
        "--claude-context",
        "true",
        "--apply",
    ]);
    assert!(app.read("claude")["statusLine"].is_object());
    app.ok(&[
        "--setup",
        "--harness",
        "claude",
        "--claude-context",
        "false",
        "--apply",
    ]);
    assert!(app.read("claude").get("statusLine").is_none());
    app.ok(&[
        "--setup",
        "--harness",
        "claude",
        "--claude-context",
        "true",
        "--apply",
    ]);
    let mut changed = app.read("claude");
    changed["statusLine"] = json!({"type":"command","command":"echo new-user-bar"});
    std::fs::write(app.file("claude"), changed.to_string()).unwrap();
    app.ok(&["--unsetup", "--apply"]);
    assert_eq!(app.read("claude")["statusLine"], changed["statusLine"]);
}

#[test]
fn installer_warning_selections_preserve_thresholds_and_use_agent_defaults() {
    let app = Install::new("warning-settings");
    let path = app.root.join("config.toml");
    let enable = [
        "--unsetup",
        "--codex-context",
        "true",
        "--claude-context",
        "true",
        "--codex-idle",
        "true",
        "--claude-idle",
        "true",
        "--apply",
    ];
    app.ok(&enable);
    let first = std::fs::read_to_string(&path).unwrap();
    let config = aherald::config::Config::parse(&first, &app.root).unwrap();
    assert_eq!(config.codex.warnings.context_tokens.unwrap().0, 250_000);
    assert_eq!(config.claude.warnings.context_tokens.unwrap().0, 400_000);
    assert_eq!(config.codex.warnings.idle_minutes, Some(25));
    assert_eq!(config.claude.warnings.idle_minutes, Some(55));
    let custom = "# personal thresholds\n[codex.warnings]\ncontext_tokens = '233k' # keep this\nidle_minutes = 17\n[claude.warnings]\ncontext_tokens = 390000\nidle_minutes = 42\n";
    std::fs::write(&path, custom).unwrap();
    let disable = [
        "--unsetup",
        "--codex-context",
        "false",
        "--claude-context",
        "false",
        "--codex-idle",
        "false",
        "--claude-idle",
        "false",
    ];
    app.ok(&disable);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), custom);
    let mut apply = disable.to_vec();
    apply.push("--apply");
    app.ok(&apply);
    let disabled = std::fs::read_to_string(&path).unwrap();
    assert!(disabled.contains("context_tokens = '233k' # keep this"));
    assert!(disabled.contains("idle_minutes = 17"));
    assert!(disabled.contains("idle_minutes = 42"));
    let report = app.ok(&["--setup-settings"]);
    let report = String::from_utf8_lossy(&report.stdout);
    for key in [
        "codex_context",
        "claude_context",
        "codex_idle",
        "claude_idle",
    ] {
        assert!(report.contains(&format!("{key}=false")), "{report}");
    }
    app.ok(&apply);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), disabled);
    app.ok(&enable);
    let enabled = std::fs::read_to_string(&path).unwrap();
    let config = aherald::config::Config::parse(&enabled, &app.root).unwrap();
    assert_eq!(config.codex.warnings.context_tokens.unwrap().0, 233_000);
    assert_eq!(config.claude.warnings.context_tokens.unwrap().0, 390_000);
    assert_eq!(config.codex.warnings.idle_minutes, Some(17));
    assert_eq!(config.claude.warnings.idle_minutes, Some(42));
    let report = app.ok(&["--setup-settings"]);
    let report = String::from_utf8_lossy(&report.stdout);
    for key in [
        "codex_context",
        "claude_context",
        "codex_idle",
        "claude_idle",
    ] {
        assert!(report.contains(&format!("{key}=true")), "{report}");
    }
}

#[test]
fn installer_warning_selections_are_independent_and_restore_zero_idle() {
    let app = Install::new("warning-independent");
    let path = app.root.join("config.toml");
    std::fs::write(&path, "[codex.warnings]\nidle_minutes = 0 # disabled before\n[claude.warnings]\ncontext_tokens = '399k'\nidle_minutes = 19\n").unwrap();
    app.ok(&[
        "--unsetup",
        "--codex-context",
        "true",
        "--codex-idle",
        "true",
        "--apply",
    ]);
    let text = std::fs::read_to_string(&path).unwrap();
    let config = aherald::config::Config::parse(&text, &app.root).unwrap();
    assert_eq!(config.codex.warnings.idle_threshold(), Some(25));
    assert_eq!(
        config.codex.warnings.context_threshold().unwrap().0,
        250_000
    );
    assert_eq!(
        config.claude.warnings.context_threshold().unwrap().0,
        399_000
    );
    assert_eq!(config.claude.warnings.idle_threshold(), Some(19));
    assert!(text.contains("idle_minutes = 25 # disabled before"));
    for flag in [
        "--codex-context",
        "--claude-context",
        "--codex-idle",
        "--claude-idle",
    ] {
        for args in [
            vec![flag, "true"],
            vec!["--unsetup", flag, "yes"],
            vec!["--unsetup", flag, "true", flag, "false"],
            vec!["--setup-settings", flag, "true"],
        ] {
            assert!(!app.run(&args).status.success(), "{args:?}");
        }
    }
}

#[test]
fn installer_reads_usage_defaults_without_creating_config() {
    let app = Install::new("usage-read");
    let out = app.ok(&["--setup-settings"]);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "codex_usage=false\nclaude_usage=false\ncodex_context=false\nclaude_context=false\ncodex_idle=true\nclaude_idle=true"
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
        "codex_usage=false\nclaude_usage=true\ncodex_context=false\nclaude_context=false\ncodex_idle=true\nclaude_idle=true"
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
        "codex_usage=true\nclaude_usage=false\ncodex_context=false\nclaude_context=false\ncodex_idle=true\nclaude_idle=true"
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

#[cfg(target_os = "linux")]
#[test]
fn a_transient_executable_write_handle_does_not_fail_fixture_launch() {
    let app = Install::new("busy-launch");
    let mut writer = Some(
        std::fs::OpenOptions::new()
            .write(true)
            .open(&app.exe)
            .unwrap(),
    );
    let mut attempts = 0;
    let child = spawn_fixture(|| {
        attempts += 1;
        let result = Command::new(&app.exe)
            .arg("--version")
            .stdout(std::process::Stdio::piped())
            .spawn();
        if attempts == 1 {
            assert_eq!(
                result.as_ref().err().and_then(|e| e.raw_os_error()),
                Some(26)
            );
            // Release the real writer after observing the kernel error.
            drop(writer.take());
        }
        result
    })
    .expect("a transient ETXTBSY must not abort the fixture launch");
    let output = finish(child);
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        aherald::version_line()
    );
    assert_eq!(attempts, 2);
}

#[test]
fn fixture_launch_does_not_retry_unrelated_errors() {
    let mut attempts = 0;
    let error = spawn_fixture(|| {
        attempts += 1;
        Err(std::io::Error::from(std::io::ErrorKind::NotFound))
    })
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert_eq!(attempts, 1);
}

#[cfg(target_os = "linux")]
#[test]
fn fixture_launch_returns_busy_error_when_the_retry_deadline_has_expired() {
    let app = Install::new("busy-expired");
    let _writer = std::fs::OpenOptions::new()
        .write(true)
        .open(&app.exe)
        .unwrap();
    let mut attempts = 0;
    let error = spawn_fixture_until(std::time::Instant::now(), || {
        attempts += 1;
        Command::new(&app.exe).arg("--version").spawn()
    })
    .unwrap_err();
    assert_eq!(error.raw_os_error(), Some(26));
    assert_eq!(attempts, 1);
}
