//! A hook must come back promptly and must not depend on the resident answering.
#![cfg(unix)]
use std::{
    io::Write,
    os::unix::{fs::DirBuilderExt, net::UnixListener},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// Runs `--hook --resident` with the runtime directory pointed at `runtime` so the
/// resident socket of the real user is never touched.
fn hook(runtime: &std::path::Path) -> (std::process::Output, Duration) {
    let started = Instant::now();
    let mut child = Command::new(env!("CARGO_BIN_EXE_aherald"))
        .args([
            "--hook",
            "--resident",
            "--config",
            "tests/fixtures/quiet.toml",
        ])
        .env("XDG_RUNTIME_DIR", runtime)
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
    let output = child.wait_with_output().unwrap();
    (output, started.elapsed())
}

/// A hook that finds no resident starts one in the background; stop it so it does not outlive
/// the test (on macOS it would keep a status item in the menu bar).
fn stop_residents(runtime: &std::path::Path) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let output = Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(["--quit", "--config", "tests/fixtures/quiet.toml"])
            .env("XDG_RUNTIME_DIR", runtime)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        if String::from_utf8_lossy(&output.stdout).contains("bye") {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn runtime(tag: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("ah-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn the_hook_returns_its_reply_when_no_resident_can_be_reached() {
    let runtime = runtime("none");
    let (output, elapsed) = hook(&runtime);
    assert_eq!(output.stdout, b"{}\n");
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
    stop_residents(&runtime);
    let _ = std::fs::remove_dir_all(runtime);
}

#[test]
fn the_hook_returns_its_reply_when_the_resident_accepts_but_never_answers() {
    let runtime = runtime("stalled");
    let directory = runtime.join("aherald");
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&directory)
        .unwrap();
    let listener = UnixListener::bind(directory.join("resident.sock")).unwrap();
    // Holds every connection open and silent for the length of the test.
    let held = std::thread::spawn(move || {
        let mut clients = Vec::new();
        listener.set_nonblocking(true).unwrap();
        let deadline = Instant::now() + Duration::from_secs(8);
        while Instant::now() < deadline {
            if let Ok((stream, _)) = listener.accept() {
                clients.push(stream);
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    let (output, elapsed) = hook(&runtime);
    assert_eq!(output.stdout, b"{}\n");
    assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
    held.join().unwrap();
    let _ = std::fs::remove_dir_all(runtime);
}
