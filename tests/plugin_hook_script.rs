//! The plugin launcher finds the executable and hands it the hook payload; failures stay silent.
#![cfg(unix)]
use std::{
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const SCRIPT: &str = "plugins/aherald/hooks/aherald-hook.sh";

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aherald-plugin-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// An app bundle whose program records its arguments and the payload it received.
fn fake_app(root: &Path, record: &Path) -> PathBuf {
    let app = root.join("aHerald.app");
    let exe = app.join("Contents/MacOS/aherald");
    fs::create_dir_all(exe.parent().unwrap()).unwrap();
    fs::write(
        &exe,
        format!(
            "#!/bin/sh\necho \"$@\" > '{0}'\ncat >> '{0}'\nexit 0\n",
            record.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    app
}

fn run(agent: &str, payload: &str, env: &[(&str, &Path)]) -> std::process::Output {
    let mut command = Command::new(Path::new(env!("CARGO_MANIFEST_DIR")).join(SCRIPT));
    command
        .arg(agent)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("AHERALD_MDFIND", "/usr/bin/false")
        .env("AHERALD_SYSTEM_APPLICATIONS", "/nonexistent")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in env {
        command.env(key, value);
    }
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn the_payload_reaches_the_app_named_by_the_override() {
    let dir = scratch("override");
    let record = dir.join("record");
    let app = fake_app(&dir, &record);
    let out = run(
        "claude",
        r#"{"hook_event_name":"Stop"}"#,
        &[("AHERALD_APP", &app)],
    );
    assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
    let seen = fs::read_to_string(&record).unwrap();
    assert!(
        seen.starts_with("--hook --agent claude --resident"),
        "{seen}"
    );
    assert!(seen.contains(r#"{"hook_event_name":"Stop"}"#), "{seen}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_app_in_the_users_applications_folder_is_found_and_codex_omits_the_agent_flag() {
    let dir = scratch("home");
    let record = dir.join("record");
    fake_app(&dir.join("Applications"), &record);
    let out = run("codex", "{}", &[("HOME", &dir)]);
    assert!(out.status.success());
    let seen = fs::read_to_string(&record).unwrap();
    assert!(seen.starts_with("--hook --resident"), "{seen}");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn without_an_app_the_script_drains_the_payload_and_stays_silent() {
    let dir = scratch("none");
    let out = run("claude", &"x".repeat(200_000), &[("HOME", &dir)]);
    assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_unknown_agent_is_ignored_without_running_anything() {
    let dir = scratch("agent");
    let record = dir.join("record");
    let app = fake_app(&dir, &record);
    let out = run("gemini", "{}", &[("AHERALD_APP", &app)]);
    assert!(out.status.success() && out.stdout.is_empty());
    assert!(!record.exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn linux_override_passes_payload_and_suppresses_a_failing_program() {
    let dir = scratch("linux-override");
    let record = dir.join("record");
    let exe = dir.join("path with spaces/aherald");
    fs::create_dir_all(exe.parent().unwrap()).unwrap();
    fs::write(
        &exe,
        format!(
            "#!/bin/sh\necho \"$@\" > '{}'\ncat >> '{}'\necho noisy\necho error >&2\nexit 7\n",
            record.display(),
            record.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    let out = run(
        "claude",
        "payload",
        &[("AHERALD_PATH", &exe), ("HOME", &dir)],
    );
    assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
    assert_eq!(
        fs::read_to_string(&record).unwrap(),
        "--hook --agent claude --resident\npayload"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn linux_portable_location_is_discovered() {
    let dir = scratch("linux-portable");
    let record = dir.join("record");
    let exe = dir.join(".local/share/aHerald/aherald");
    fs::create_dir_all(exe.parent().unwrap()).unwrap();
    fs::write(
        &exe,
        format!(
            "#!/bin/sh\necho \"$@\" > '{}'\ncat >> '{}'\n",
            record.display(),
            record.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    let out = run("codex", "payload", &[("HOME", &dir)]);
    assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
    assert_eq!(
        fs::read_to_string(&record).unwrap(),
        "--hook --resident\npayload"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_missing_explicit_binary_does_not_run_a_different_installation() {
    let dir = scratch("missing-override");
    let record = dir.join("record");
    let app = fake_app(&dir, &record);
    let missing = dir.join("missing");
    let out = run(
        "claude",
        "payload",
        &[
            ("HOME", &dir),
            ("AHERALD_PATH", &missing),
            ("AHERALD_APP", &app),
        ],
    );
    assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
    assert!(!record.exists());
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn codex_preserves_only_the_valid_stop_acknowledgement() {
    let dir = scratch("codex-reply");
    let exe = dir.join("aherald");
    fs::write(&exe, "#!/bin/sh\ncat >/dev/null\nprintf '{}\\n'\n").unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    let out = run("codex", "payload", &[("AHERALD_PATH", &exe)]);
    assert!(out.status.success() && out.stderr.is_empty());
    assert_eq!(out.stdout, b"{}\n");
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn linux_path_is_used_when_no_override_or_portable_binary_exists() {
    let dir = scratch("linux-path");
    let record = dir.join("record");
    let bin = dir.join("bin");
    fs::create_dir_all(&bin).unwrap();
    let exe = bin.join("aherald");
    fs::write(
        &exe,
        format!(
            "#!/bin/sh\necho \"$@\" > '{}'\ncat >> '{}'\n",
            record.display(),
            record.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).unwrap();
    let path = PathBuf::from(format!("{}:/usr/bin:/bin", bin.display()));
    let out = run("claude", "payload", &[("HOME", &dir), ("PATH", &path)]);
    assert!(out.status.success() && out.stdout.is_empty() && out.stderr.is_empty());
    assert_eq!(
        fs::read_to_string(&record).unwrap(),
        "--hook --agent claude --resident\npayload"
    );
    fs::remove_dir_all(dir).unwrap();
}
