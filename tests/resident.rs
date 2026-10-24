#![cfg(windows)]
use aherald::{
    ipc::{Request, Response, pipe},
    local::Scope,
    resident,
};
use std::{path::PathBuf, time::Duration};
#[test]
fn resident_startup_races_status_forwarding_and_quit() {
    let root = std::env::temp_dir().join(format!("aherald-resident-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("config.toml");
    std::fs::write(
        &config,
        "[notifications]\nenabled=false\n[sounds]\nenabled=false",
    )
    .unwrap();
    let scope = Scope::current(&format!("resident-test-{}", std::process::id())).unwrap();
    struct Cleanup(Scope, PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = pipe::request(&self.0, &Request::Quit, Duration::from_millis(100));
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }
    let _cleanup = Cleanup(scope.clone(), root);
    let mut jobs = Vec::new();
    for _ in 0..4 {
        let config = config.clone();
        let scope = scope.clone();
        jobs.push(std::thread::spawn(move || {
            resident::ensure(
                &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
                &config,
                &scope,
            )
            .unwrap()
        }));
    }
    let pids: Vec<_> = jobs
        .into_iter()
        .map(|j| match j.join().unwrap() {
            Response::Status { pid, .. } => pid,
            _ => panic!("expected status"),
        })
        .collect();
    assert!(pids.iter().all(|p| *p == pids[0]));
    assert!(matches!(
        pipe::request(
            &scope,
            &Request::Event {
                event: aherald::events::Event::demo("permission_required").unwrap()
            },
            Duration::from_millis(300)
        )
        .unwrap(),
        Response::Accepted
    ));
    assert!(matches!(
        pipe::request(&scope, &Request::Quit, Duration::from_millis(300)).unwrap(),
        Response::Bye
    ));
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while scope.acquire().unwrap().is_none() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    }
}
