#![cfg(windows)]
//! Render failures must never end the resident. Faults are injected through
//! `aherald.fault` beside the configuration, which only debug builds honor.
use aherald::{
    events::Event,
    ipc::{Request, Response, pipe},
    local::Scope,
    resident,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};

struct Cleanup(Scope, PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = pipe::request(&self.0, &Request::Quit, Duration::from_millis(200));
        let _ = std::fs::remove_dir_all(&self.1);
    }
}

fn visible_window(pid: u32) -> Option<RECT> {
    struct Found {
        pid: u32,
        bounds: Option<RECT>,
    }
    unsafe extern "system" fn find(h: HWND, p: LPARAM) -> windows::core::BOOL {
        unsafe {
            let state = &mut *(p.0 as *mut Found);
            let mut pid = 0;
            GetWindowThreadProcessId(h, Some(&mut pid));
            if pid == state.pid && IsWindowVisible(h).as_bool() {
                let mut bounds = RECT::default();
                if GetWindowRect(h, &mut bounds).is_ok() {
                    state.bounds = Some(bounds);
                }
            }
            true.into()
        }
    }
    let mut found = Found { pid, bounds: None };
    unsafe {
        let _ = EnumWindows(Some(find), LPARAM(&mut found as *mut Found as isize));
    }
    found.bounds
}

fn wait_for_window(pid: u32) -> RECT {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(bounds) = visible_window(pid) {
            return bounds;
        }
        assert!(Instant::now() < deadline, "resident showed no card");
        std::thread::yield_now();
    }
}

fn start(fault: &str) -> (Scope, Cleanup, u32, PathBuf) {
    let scope = Scope::current(&format!("resident-fault-{fault}-{}", std::process::id())).unwrap();
    let root = std::env::temp_dir().join(&scope.namespace);
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("config.toml");
    std::fs::write(
        &config,
        "[sounds]\nenabled=false\n[notifications]\nfade_in_ms=0\nfade_out_ms=0",
    )
    .unwrap();
    std::fs::write(root.join("aherald.fault"), fault).unwrap();
    let cleanup = Cleanup(scope.clone(), root.clone());
    let pid = match resident::ensure(
        &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
        &config,
        &scope,
    )
    .unwrap()
    {
        Response::Status { pid, .. } => pid,
        _ => panic!("unexpected status response"),
    };
    (scope, cleanup, pid, root)
}

fn send(scope: &Scope, kind: &str, session: &str) {
    let mut event = Event::demo(kind).unwrap();
    event.session_id = Some(session.into());
    pipe::request(scope, &Request::Event { event }, Duration::from_millis(200)).unwrap();
}

fn status(scope: &Scope) -> (u32, usize) {
    match pipe::request(scope, &Request::Status, Duration::from_millis(500)).unwrap() {
        Response::Status { pid, queued, .. } => (pid, queued),
        _ => panic!("unexpected status response"),
    }
}

#[test]
fn composition_failure_shows_a_text_card_and_the_resident_keeps_serving() {
    let (scope, _cleanup, pid, _root) = start("compose");
    send(&scope, "permission_required", "one");
    let bounds = wait_for_window(pid);
    // Artwork cards are wider than three heights; the text card is not.
    assert!(bounds.right - bounds.left <= (bounds.bottom - bounds.top) * 3);
    send(&scope, "input_required", "two");
    assert_eq!(status(&scope), (pid, 2));
}

#[test]
fn placement_failure_keeps_the_resident_accepting_events_and_recovers() {
    let (scope, _cleanup, pid, root) = start("placement");
    send(&scope, "permission_required", "one");
    let until = Instant::now() + Duration::from_millis(500);
    while Instant::now() < until {
        assert!(
            visible_window(pid).is_none(),
            "card shown without placement"
        );
        assert_eq!(status(&scope).0, pid, "resident exited or restarted");
        std::thread::sleep(Duration::from_millis(50));
    }
    send(&scope, "input_required", "two");
    assert_eq!(status(&scope), (pid, 2));
    std::fs::remove_file(root.join("aherald.fault")).unwrap();
    wait_for_window(pid);
    assert_eq!(status(&scope).0, pid);
}
