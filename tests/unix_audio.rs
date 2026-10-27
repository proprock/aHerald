//! Playback is serialized across processes: a sound waits for another process's sound, and a
//! process that dies mid-sound does not keep the gate.
#![cfg(unix)]
use aherald::audio::{Wave, serial::play_with};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};

fn wave() -> Wave {
    Wave {
        channels: 1,
        sample_rate: 8000,
        samples: vec![0; 800],
    }
}

fn directory(tag: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("ah-audio-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

/// Not a test of its own: the body of the process that holds the gate for a while. Without
/// `AHERALD_TEST_GATE` it returns at once.
#[test]
fn gate_holder_helper() {
    let Ok(gate) = std::env::var("AHERALD_TEST_GATE") else {
        return;
    };
    let dir = PathBuf::from(std::env::var("AHERALD_TEST_DIR").unwrap());
    let hold = Duration::from_millis(
        std::env::var("AHERALD_TEST_HOLD_MS")
            .unwrap()
            .parse()
            .unwrap(),
    );
    play_with(
        &gate,
        &wave(),
        b"holder",
        0,
        &AtomicBool::new(false),
        |_, _| {
            std::fs::write(dir.join("started"), b"1").unwrap();
            std::thread::sleep(hold);
            std::fs::write(dir.join("ended"), b"1").unwrap();
            Ok(())
        },
    )
    .unwrap();
}

fn spawn_holder(gate: &str, dir: &Path, hold_ms: u64) -> Child {
    let child = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "gate_holder_helper", "--nocapture"])
        .env("AHERALD_TEST_GATE", gate)
        .env("AHERALD_TEST_DIR", dir)
        .env("AHERALD_TEST_HOLD_MS", hold_ms.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !dir.join("started").exists() {
        assert!(Instant::now() < deadline, "the holder never started");
        std::thread::sleep(Duration::from_millis(10));
    }
    child
}

#[test]
fn a_sound_waits_for_the_sound_of_another_process() {
    let gate = format!("audio-proc-{}", std::process::id());
    let dir = directory("wait");
    let mut holder = spawn_holder(&gate, &dir, 700);
    let mut finished_before_ours = None;
    play_with(
        &gate,
        &wave(),
        b"ours",
        0,
        &AtomicBool::new(false),
        |_, _| {
            finished_before_ours = Some(dir.join("ended").exists());
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(finished_before_ours, Some(true), "the sounds overlapped");
    holder.wait().unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_process_killed_mid_sound_releases_the_gate() {
    let gate = format!("audio-kill-{}", std::process::id());
    let dir = directory("kill");
    let mut holder = spawn_holder(&gate, &dir, 20_000);
    holder.kill().unwrap();
    holder.wait().unwrap();
    let started = Instant::now();
    let mut played = false;
    play_with(
        &gate,
        &wave(),
        b"ours",
        0,
        &AtomicBool::new(false),
        |_, _| {
            played = true;
            Ok(())
        },
    )
    .unwrap();
    assert!(played);
    assert!(started.elapsed() < Duration::from_secs(2));
    let _ = std::fs::remove_dir_all(dir);
}
