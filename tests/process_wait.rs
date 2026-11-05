//! Executed child fixtures exercise native pipes and process lifetime.
#![cfg(windows)]
#![allow(clippy::undocumented_unsafe_blocks)] // TODO(AH-249): write the SAFETY comments of these Windows test helpers.
mod support;
use std::{
    io::Write,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
fn command(mode: &str, root: &std::path::Path) -> Command {
    let mut cmd = Command::new(std::env::current_exe().unwrap());
    cmd.args(["--exact", "process_child", "--nocapture"])
        .env("AH_PROCESS_MODE", mode)
        .env("AH_PROCESS_ROOT", root);
    cmd
}
#[test]
fn exited_child_with_inherited_output_cannot_extend_the_deadline() {
    let root = support::TempDir::new("inherited-pipes");
    let scope =
        aherald::local::Scope::current(root.file_name().unwrap().to_str().unwrap()).unwrap();
    let mut owner = support::detached::Residents::new(scope);
    let mut child = command("inherit", &root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    owner.retain(child.id()).unwrap();
    support::wait_until(
        Duration::from_secs(3),
        Duration::from_millis(5),
        "exited parent and inherited output holder ready",
        || root.join("pid").exists() && child.try_wait().unwrap().is_some(),
    )
    .unwrap();
    let start = Instant::now();
    let result = support::capture_bounded(child, &[], Duration::from_millis(300));
    let elapsed = start.elapsed();
    // The finite descendant exits independently, including during unwinding on red.
    let pid: u32 = std::fs::read_to_string(root.join("pid"))
        .unwrap()
        .parse()
        .unwrap();
    #[cfg(windows)]
    unsafe {
        use windows::Win32::{Foundation::*, System::Threading::*};
        if let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            let state = WaitForSingleObject(handle, 3000);
            let _ = CloseHandle(handle);
            assert_eq!(state, WAIT_OBJECT_0);
        }
    }
    #[cfg(not(windows))]
    let _ = pid;
    assert!(
        elapsed < Duration::from_secs(1),
        "pipe capture exceeded deadline: {elapsed:?}"
    );
    assert!(
        result.is_err(),
        "inherited output exceeded deadline but returned success after {:?}",
        start.elapsed()
    );
}
#[test]
fn delayed_large_output_is_drained_from_both_pipes() {
    let root = support::TempDir::new("large-output");
    let out = support::run_bounded(&mut command("large", &root), Duration::from_secs(3)).unwrap();
    assert!(out.status.success());
    assert!(
        out.stdout
            .windows(300_000)
            .any(|v| v.iter().all(|b| *b == b'o'))
    );
    assert!(
        out.stderr
            .windows(300_000)
            .any(|v| v.iter().all(|b| *b == b'e'))
    );
}
#[test]
fn hung_child_with_unread_stdin_is_killed_without_leaking_payloads() {
    let root = support::TempDir::new("hung-input");
    let scope =
        aherald::local::Scope::current(root.file_name().unwrap().to_str().unwrap()).unwrap();
    let mut owner = support::detached::Residents::new(scope);
    let child = command("hang", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    owner.retain(pid).unwrap();
    support::wait_until(
        Duration::from_secs(3),
        Duration::from_millis(5),
        "hung input fixture ready",
        || root.join("pid").exists(),
    )
    .unwrap();
    let start = Instant::now();
    let error = support::capture_bounded(child, &vec![b'x'; 2_000_000], Duration::from_secs(1))
        .unwrap_err();
    assert!(start.elapsed() < Duration::from_secs(3));
    assert!(error.contains("killed"), "{error}");
    assert!(!error.contains("PRIVATE_PAYLOAD"));
    #[cfg(windows)]
    unsafe {
        use windows::Win32::{Foundation::*, System::Threading::*};
        if let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            assert_eq!(WaitForSingleObject(handle, 0), WAIT_OBJECT_0);
            let _ = CloseHandle(handle);
        }
    }
    #[cfg(not(windows))]
    let _ = pid;
}
#[test]
// This Windows-only scenario deliberately outlives its direct parent; the outer
// test observes its finite exit through a native process handle.
#[allow(clippy::zombie_processes)]
fn process_child() {
    let Ok(mode) = std::env::var("AH_PROCESS_MODE") else {
        return;
    };
    let root = std::path::PathBuf::from(std::env::var_os("AH_PROCESS_ROOT").unwrap());
    match mode.as_str() {
        "inherit" => {
            let child = command("finite", &root).spawn().unwrap();
            std::fs::write(root.join("pid"), child.id().to_string()).unwrap();
        }
        "finite" => std::thread::sleep(Duration::from_millis(1500)),
        "large" => {
            std::thread::sleep(Duration::from_millis(80));
            std::io::stdout().write_all(&vec![b'o'; 300_000]).unwrap();
            std::io::stderr().write_all(&vec![b'e'; 300_000]).unwrap();
        }
        "hang" => {
            std::fs::write(root.join("pid"), std::process::id().to_string()).unwrap();
            eprintln!("PRIVATE_PAYLOAD");
            loop {
                std::thread::park();
            }
        }
        _ => panic!("unknown fixture mode"),
    }
}

#[test]
fn detached_guard_rejects_unowned_processes_and_cleans_a_hung_child_after_panic() {
    use windows::Win32::{Foundation::*, System::Threading::*};
    let root = support::TempDir::new("detached-panic");
    let scope =
        aherald::local::Scope::current(root.file_name().unwrap().to_str().unwrap()).unwrap();
    let mut guard = support::detached::Residents::new(scope);
    assert!(guard.retain(std::process::id()).is_err());
    let child = command("hang", &root)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let pid = child.id();
    let mut owned = support::OwnedChild(child);
    guard.retain(pid).unwrap();
    // Retain a separate observation handle; only the guard now owns termination.
    let observation = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid).unwrap() };

    let result = std::panic::catch_unwind(move || {
        let _guard = guard;
        panic!("exercise detached guard cleanup");
    });
    assert!(result.is_err());
    assert!(owned.0.try_wait().unwrap().is_some());
    unsafe {
        let state = WaitForSingleObject(observation, 0);
        let _ = CloseHandle(observation);
        assert_eq!(
            state, WAIT_OBJECT_0,
            "hung owned child survived panic cleanup"
        );
    }
}

#[test]
fn detached_guard_shuts_down_its_resident_without_stopping_an_independent_one() {
    use aherald::{
        ipc::{Request, Response, pipe},
        resident,
    };
    let configuration = "[notifications]\nenabled=false\n[sounds]\nenabled=false";
    let mut sibling = support::ResidentFixture::new("detached-sibling", configuration);
    let sibling_pid = sibling.start().unwrap();
    let fixture = support::ResidentFixture::new("detached-owned", configuration);
    let Response::Status { pid, .. } = resident::ensure(
        std::path::Path::new(env!("CARGO_BIN_EXE_aherald")),
        &fixture.config,
        &fixture.scope,
    )
    .unwrap() else {
        panic!("expected resident status");
    };
    let mut guard = support::detached::Residents::new(fixture.scope.clone());
    guard.retain(pid).unwrap();
    guard.shutdown().unwrap();
    assert!(
        fixture.scope.acquire().unwrap().is_some(),
        "owned resident kept its IPC mutex"
    );
    assert!(pipe::request(&fixture.scope, &Request::Status, Duration::from_millis(100)).is_err());
    assert!(
        matches!(pipe::request(&sibling.scope, &Request::Status, Duration::from_millis(300)), Ok(Response::Status { pid, .. }) if pid == sibling_pid)
    );
}
