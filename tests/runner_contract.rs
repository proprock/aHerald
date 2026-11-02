//! Opt-in runner scheduling and forced-termination checks; no installed accounts.
#![cfg(windows)]
mod support;
use std::{fs, io::Write, path::PathBuf, time::Duration};
fn root() -> PathBuf {
    PathBuf::from(
        std::env::var_os("AH_RUNNER_PROBE_ROOT").expect("run through tests/runner_probe.ps1"),
    )
}
fn exclusive_slot(resource: &str) {
    let root = root();
    let path = root.join(resource);
    let slot = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .expect("runner scheduled two desktop tests concurrently");
    // Deliberate resource occupancy, so a missing inter-process group is observable.
    std::thread::sleep(Duration::from_millis(300));
    drop(slot);
    fs::remove_file(path).unwrap();
}
#[test]
#[ignore = "runner scheduling probe; tests/runner_probe.ps1 supplies an isolated root"]
fn desktop_slot_first() {
    exclusive_slot("desktop-slot");
}
#[test]
#[ignore = "runner scheduling probe; tests/runner_probe.ps1 supplies an isolated root"]
fn desktop_slot_second() {
    exclusive_slot("desktop-slot");
}

#[test]
#[ignore = "runner exclusivity probe; tests/runner_probe.ps1 supplies an isolated root"]
fn performance_slot() {
    exclusive_slot("performance-slot");
}
#[test]
#[ignore = "runner exclusivity probe; tests/runner_probe.ps1 supplies an isolated root"]
fn background_slot() {
    exclusive_slot("performance-slot");
}

#[test]
#[ignore = "intentional hang; only execute through the bounded runner probe"]
fn timeout_owns_a_detached_resident() {
    use aherald::{ipc::Response, resident};
    let root = root();
    let scope =
        aherald::local::Scope::current(root.file_name().unwrap().to_str().unwrap()).unwrap();
    let config = root.join("config.toml");
    fs::write(
        &config,
        "[notifications]\nenabled=false\n[sounds]\nenabled=false\n[codex.usage]\nenabled=false",
    )
    .unwrap();
    let Response::Status { pid, .. } = resident::ensure(
        std::path::Path::new(env!("CARGO_BIN_EXE_aherald")),
        &config,
        &scope,
    )
    .unwrap() else {
        panic!("expected resident status");
    };
    // Windows releases this exact lock and the resident's IPC when nextest kills
    // its job. Rust Drop cannot run under forced test termination.
    use std::os::windows::fs::OpenOptionsExt;
    let mut lock = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(root.join("owned-lock"))
        .unwrap();
    lock.write_all(b"held").unwrap();
    let record =
        serde_json::json!({"pid":pid,"test_pid":std::process::id(),"namespace":scope.namespace});
    fs::write(root.join("ready.tmp"), record.to_string()).unwrap();
    fs::rename(root.join("ready.tmp"), root.join("ready.json")).unwrap();
    loop {
        std::thread::park();
    }
}

#[test]
#[ignore = "post-timeout native resource check; requires runner probe evidence"]
fn timeout_released_native_resources() {
    use aherald::{
        ipc::{Request, pipe},
        local::Scope,
    };
    let root = root();
    let record: serde_json::Value =
        serde_json::from_slice(&fs::read(root.join("ready.json")).unwrap()).unwrap();
    let scope = Scope::current(record["namespace"].as_str().unwrap()).unwrap();
    assert!(
        scope.acquire().unwrap().is_some(),
        "killed resident retained its mutex"
    );
    assert!(
        pipe::request(&scope, &Request::Status, Duration::from_millis(100)).is_err(),
        "killed resident retained its pipe"
    );
    use std::os::windows::fs::OpenOptionsExt;
    let lock = fs::OpenOptions::new()
        .write(true)
        .share_mode(0)
        .open(root.join("owned-lock"))
        .unwrap();
    drop(lock);
    fs::remove_file(root.join("owned-lock")).unwrap();
}
