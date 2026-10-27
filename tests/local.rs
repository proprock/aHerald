#![cfg(windows)]
use aherald::local::Scope;
#[test]
fn scoped_resources_are_unique_and_recover_after_owner_exit() {
    let scope = Scope::current(&format!("test-{}", std::process::id())).unwrap();
    assert!(scope.mutex_name.contains("S-1-5-5-"));
    let guard = scope.acquire().unwrap().unwrap();
    let pipe = scope.create_pipe().unwrap();
    assert!(scope.create_pipe().is_err());
    let name = scope.clone();
    assert!(
        !std::thread::spawn(move || name.acquire().unwrap().is_some())
            .join()
            .unwrap()
    );
    drop(pipe);
    drop(guard);
    assert!(scope.acquire().unwrap().is_some());
}

#[test]
fn process_owner_helper() {
    let Ok(namespace) = std::env::var("AHERALD_TEST_OWNER") else {
        return;
    };
    let scope = Scope::current(&namespace).unwrap();
    let _owner = scope.acquire().unwrap().unwrap();
    let _pipe = scope.create_pipe().unwrap();
    std::fs::write(std::env::var("AHERALD_TEST_READY").unwrap(), b"ready").unwrap();
    loop {
        std::thread::park();
    }
}
#[test]
fn process_termination_releases_mutex_and_pipe() {
    use std::time::{Duration, Instant};
    let namespace = format!("crash-{}", std::process::id());
    let ready = std::env::temp_dir().join(&namespace);
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "process_owner_helper"])
        .env("AHERALD_TEST_OWNER", &namespace)
        .env("AHERALD_TEST_READY", &ready)
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap();
    struct Cleanup<'a>(&'a mut std::process::Child, std::path::PathBuf);
    impl Drop for Cleanup<'_> {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
            let _ = std::fs::remove_file(&self.1);
        }
    }
    let owned = Cleanup(&mut child, ready.clone());
    let deadline = Instant::now() + Duration::from_secs(3);
    while !ready.exists() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let scope = Scope::current(&namespace).unwrap();
    assert!(scope.acquire().unwrap().is_none());
    owned.0.kill().unwrap();
    owned.0.wait().unwrap();
    let _guard = scope.acquire().unwrap().unwrap();
    let _pipe = scope.create_pipe().unwrap();
}
