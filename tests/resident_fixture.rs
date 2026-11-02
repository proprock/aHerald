#![cfg(windows)]
mod support;
use aherald::ipc::{Request, Response, pipe};
use std::{process::Command, time::Duration};
use support::ResidentFixture;

const QUIET: &str =
    "[notifications]\nenabled=false\n[sounds]\nenabled=false\n[codex.usage]\nenabled=false\n";

#[test]
fn independent_residents_have_unique_scopes_and_cleanup_only_their_own_process() {
    let mut first = ResidentFixture::new("fixture", QUIET);
    let mut second = ResidentFixture::new("fixture", QUIET);
    assert_ne!(first.scope.pipe_name, second.scope.pipe_name);
    let first_pid = first.start().unwrap();
    let second_pid = second.start().unwrap();
    assert_ne!(first_pid, second_pid);
    let scope = first.scope.clone();
    let root = first.root.to_path_buf();
    drop(first);
    assert!(!root.exists());
    assert!(pipe::request(&scope, &Request::Status, Duration::from_millis(100)).is_err());
    assert!(
        matches!(pipe::request(&second.scope, &Request::Status, Duration::from_millis(500)), Ok(Response::Status { pid, .. }) if pid == second_pid)
    );
    second.shutdown().unwrap();
}

#[test]
fn startup_failure_cleans_files_without_stopping_a_sibling() {
    let mut sibling = ResidentFixture::new("startup", QUIET);
    let pid = sibling.start().unwrap();
    let root;
    {
        let mut failed = ResidentFixture::new("startup", "not valid toml = [");
        root = failed.root.to_path_buf();
        assert!(failed.start().unwrap_err().contains("exited"));
    }
    assert!(!root.exists());
    assert!(
        matches!(pipe::request(&sibling.scope, &Request::Status, Duration::from_millis(500)), Ok(Response::Status { pid: actual, .. }) if actual == pid)
    );
}

#[test]
fn panic_cleanup_waits_for_process_exit_before_removing_files() {
    let mut fixture = ResidentFixture::new("panic", QUIET);
    fixture.start().unwrap();
    let scope = fixture.scope.clone();
    let root = fixture.root.to_path_buf();
    let result = std::panic::catch_unwind(move || {
        let _fixture = fixture;
        panic!("exercise cleanup");
    });
    assert!(result.is_err());
    assert!(!root.exists());
    assert!(
        scope.acquire().unwrap().is_some(),
        "resident still owns its mutex"
    );
}

#[test]
fn startup_timeout_kills_and_reaps_the_owned_child() {
    let mut fixture = ResidentFixture::new("timeout", QUIET);
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", "fixture_hanging_child", "--nocapture"])
        .env("AH_FIXTURE_CHILD_ROOT", fixture.root.as_os_str());
    let error = fixture
        .start_command(&mut command, Duration::from_secs(2))
        .unwrap_err();
    assert!(error.contains("timed out"), "{error}");
    let pid: u32 = std::fs::read_to_string(fixture.root.join("child.pid"))
        .unwrap()
        .parse()
        .unwrap();
    use windows::Win32::{Foundation::*, System::Threading::*};
    // SAFETY: query only the child PID recorded by this test; close any opened handle.
    unsafe {
        if let Ok(handle) = OpenProcess(PROCESS_SYNCHRONIZE, false, pid) {
            let state = WaitForSingleObject(handle, 0);
            let _ = CloseHandle(handle);
            assert_eq!(state, WAIT_OBJECT_0, "timed-out child survived");
        }
    }
    let root = fixture.root.to_path_buf();
    drop(fixture);
    assert!(!root.exists());
}

#[test]
fn fixture_hanging_child() {
    let Some(root) = std::env::var_os("AH_FIXTURE_CHILD_ROOT") else {
        return;
    };
    std::fs::write(
        std::path::Path::new(&root).join("child.pid"),
        std::process::id().to_string(),
    )
    .unwrap();
    loop {
        std::thread::park();
    }
}
