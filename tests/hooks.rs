mod support;
use rstest::rstest;
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn invoke(input: &[u8]) -> std::process::Output {
    support::run_bounded_with_input(
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(["--hook", "--config", "tests/fixtures/quiet.toml"])
            .stdin(Stdio::piped()),
        input,
        Duration::from_secs(3),
    )
    .unwrap()
}

#[test]
fn stop_returns_empty_json_without_a_decision() {
    let result = invoke(include_bytes!("fixtures/stop.json"));
    assert!(result.status.success());
    assert_eq!(result.stdout, b"{}\n");
}
#[rstest]
#[case::malformed_json(b"{".as_slice())]
#[case::unknown_event(br#"{"hook_event_name":"Unknown"}"#.as_slice())]
#[case::permission_request_without_fields(br#"{"hook_event_name":"PermissionRequest"}"#.as_slice())]
fn malformed_and_unknown_hooks_are_silent_success(#[case] bytes: &[u8]) {
    let result = invoke(bytes);
    assert!(result.status.success());
    assert!(result.stdout.is_empty());
}
#[test]
fn model_request_observer_is_silent_and_independent_of_resident_lifetime() {
    let started = Instant::now();
    let result = invoke(br#"{"hook_event_name":"PostModelRequest","session_id":"s","turn_id":"t","status":"completed","token_usage":{"input_tokens":200}}"#);
    assert!(result.status.success());
    assert!(result.stdout.is_empty());
    assert!(started.elapsed() < Duration::from_secs(3));
}
#[test]
fn open_stdin_does_not_hold_hook_process() {
    let child = Command::new(env!("CARGO_BIN_EXE_aherald"))
        .args(["--hook", "--config", "tests/fixtures/quiet.toml"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = support::OwnedChild(child);
    let start = Instant::now();
    let deadline = start + Duration::from_secs(3);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            break;
        }
        if Instant::now() >= deadline {
            child.0.kill().unwrap();
            child.0.wait().unwrap();
            panic!("hook did not return with stdin open");
        }
        std::thread::yield_now();
    }
}

#[rstest]
#[case::permission_request("PermissionRequest")]
#[case::interrupt("Interrupt")]
#[case::user_prompt_submit("UserPromptSubmit")]
#[case::pre_tool_use("PreToolUse")]
#[case::post_tool_use("PostToolUse")]
#[case::session_end("SessionEnd")]
fn permission_and_interrupt_do_not_emit_decisions_or_payloads(#[case] name: &str) {
    let result = invoke(
        format!(r#"{{"hook_event_name":"{name}","session_id":"s","turn_id":"t","tool_input":{{"command":"DO_NOT_LOG"}}}}"#)
            .as_bytes(),
    );
    assert!(result.status.success());
    assert!(result.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&result.stderr).contains("DO_NOT_LOG"));
}

#[test]
fn stop_response_survives_invalid_configuration() {
    let path =
        std::env::temp_dir().join(format!("aherald-invalid-hook-{}.toml", std::process::id()));
    std::fs::write(&path, "invalid = [").unwrap();
    let output = support::run_bounded_with_input(
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(["--hook", "--config"])
            .arg(&path)
            .stdin(Stdio::piped()),
        include_bytes!("fixtures/stop.json"),
        Duration::from_secs(3),
    )
    .unwrap();
    std::fs::remove_file(path).unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"{}\n");
}

#[cfg(windows)]
#[test]
fn powershell_gui_invocation_collects_hook_responses_with_quoted_paths() {
    let root = std::env::temp_dir().join(format!("aherald shell's paths {}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("quiet config.toml");
    std::fs::copy("tests/fixtures/quiet.toml", &config).unwrap();
    let executable = root.join("aherald.exe");
    std::fs::copy(env!("CARGO_BIN_EXE_aherald"), &executable).unwrap();
    let quote = |path: &std::path::Path| path.to_str().unwrap().replace('\'', "''");
    for (input, expected) in [
        (include_bytes!("fixtures/stop.json").as_slice(), "{}"),
        (
            include_bytes!("fixtures/permission-request.json").as_slice(),
            "",
        ),
        (include_bytes!("fixtures/interrupt.json").as_slice(), ""),
    ] {
        let command = format!(
            "& '{}' --hook --transient --config '{}' | Out-String",
            quote(&executable),
            quote(&config)
        );
        let output = support::run_bounded_with_input(
            Command::new("powershell.exe")
                .args(["-NoProfile", "-NonInteractive", "-Command", &command])
                .stdin(Stdio::piped()),
            input,
            Duration::from_secs(3),
        )
        .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), expected);
        assert!(output.stderr.is_empty());
    }
    std::fs::remove_dir_all(root).unwrap();
}

/// `--install-hooks` and `--install-statusline` write where the agent reads its settings: its own
/// directory variable first, the home directory only without it (AH-234).
mod hook_file_location {
    use super::support::{self, TempDir};
    use std::{path::Path, process::Command, time::Duration};

    /// Runs aHerald with a scratch home and the given agent variables (others removed).
    fn run(args: &[&str], home: &Path, vars: &[(&str, &Path)]) -> String {
        let mut command = Command::new(env!("CARGO_BIN_EXE_aherald"));
        command
            .args(args)
            .env_remove("CLAUDE_CONFIG_DIR")
            .env_remove("CODEX_HOME")
            .env(aherald::paths::HOME_VAR, home);
        for (key, value) in vars {
            command.env(key, value);
        }
        let output = support::run_bounded(&mut command, Duration::from_secs(10)).unwrap();
        assert!(
            output.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    #[test]
    fn claude_hooks_go_to_claude_config_dir_when_it_is_set() {
        let (home, config) = (TempDir::new("hk-home"), TempDir::new("hk-claude"));
        let report = run(
            &["--install-hooks", "claude", "--apply"],
            &home,
            &[("CLAUDE_CONFIG_DIR", &config)],
        );
        assert!(config.join("settings.json").is_file(), "{report}");
        assert!(!home.join(".claude").exists(), "home must stay untouched");
    }

    #[test]
    fn codex_hooks_go_to_codex_home_when_it_is_set() {
        let (home, codex) = (TempDir::new("hk-home"), TempDir::new("hk-codex"));
        let report = run(
            &["--install-hooks", "codex", "--apply"],
            &home,
            &[("CODEX_HOME", &codex)],
        );
        assert!(codex.join("hooks.json").is_file(), "{report}");
        assert!(!home.join(".codex").exists(), "home must stay untouched");
    }

    #[test]
    fn without_the_variables_the_home_directory_is_used() {
        let home = TempDir::new("hk-home");
        run(&["--install-hooks", "claude", "--apply"], &home, &[]);
        run(&["--install-hooks", "codex", "--apply"], &home, &[]);
        assert!(home.join(".claude/settings.json").is_file());
        assert!(home.join(".codex/hooks.json").is_file());
    }

    #[test]
    fn removal_looks_in_the_same_place_as_installation() {
        let (home, config) = (TempDir::new("hk-home"), TempDir::new("hk-claude"));
        let vars = [("CLAUDE_CONFIG_DIR", &*config)];
        run(&["--install-hooks", "claude", "--apply"], &home, &vars);
        let written = std::fs::read_to_string(config.join("settings.json")).unwrap();
        assert!(written.contains("aherald") || written.contains("aHerald"));
        run(&["--uninstall-hooks", "claude", "--apply"], &home, &vars);
        let after = std::fs::read_to_string(config.join("settings.json")).unwrap();
        assert!(!after.contains("--hook"), "{after}");
    }

    #[cfg(not(windows))]
    #[test]
    fn the_statusline_goes_to_claude_config_dir_too() {
        let (home, config) = (TempDir::new("hk-home"), TempDir::new("hk-claude"));
        run(
            &["--install-statusline", "--apply"],
            &home,
            &[("CLAUDE_CONFIG_DIR", &config)],
        );
        let written = std::fs::read_to_string(config.join("settings.json")).unwrap();
        assert!(written.contains("statusLine"), "{written}");
        assert!(!home.join(".claude").exists(), "home must stay untouched");
    }
}
