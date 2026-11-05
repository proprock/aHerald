#![cfg(windows)]
//! Render failures must never end the resident. Faults are injected through
//! `aherald.fault` beside the configuration, which only debug builds honor.
#![allow(clippy::undocumented_unsafe_blocks)] // TODO(AH-249): write the SAFETY comments of these Windows test helpers.
use aherald::{
    events::Event,
    ipc::{Request, Response, pipe},
    local::Scope,
};
use std::time::{Duration, Instant};
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};

mod support;
use support::{ResidentFixture, wait_until};

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
    let mut bounds = None;
    wait_until(
        Duration::from_secs(3),
        Duration::from_millis(5),
        "resident card visibility",
        || {
            bounds = visible_window(pid);
            bounds.is_some()
        },
    )
    .unwrap();
    bounds.unwrap()
}

fn start(fault: &str) -> (ResidentFixture, u32) {
    let mut fixture = ResidentFixture::new(
        "render-fault",
        "[sounds]\nenabled=false\n[notifications]\nfade_in_ms=0\nfade_out_ms=0",
    );
    std::fs::write(fixture.root.join("aherald.fault"), fault).unwrap();
    let pid = fixture.start().unwrap();
    (fixture, pid)
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
    let (fixture, pid) = start("compose");
    let scope = &fixture.scope;
    send(scope, "permission_required", "one");
    let bounds = wait_for_window(pid);
    // Artwork cards are wider than 2.5 heights; the text card is not.
    assert!((bounds.right - bounds.left) * 2 <= (bounds.bottom - bounds.top) * 5);
    send(scope, "input_required", "two");
    assert_eq!(status(scope), (pid, 2));
}

#[test]
fn a_card_waits_for_its_artwork_and_first_appears_with_it() {
    let (fixture, pid) = start("none");
    let scope = &fixture.scope;
    send(scope, "permission_required", "one");
    let bounds = wait_for_window(pid);
    // Artwork cards are wider than 2.5 heights; the text card is not.
    assert!(
        (bounds.right - bounds.left) * 2 > (bounds.bottom - bounds.top) * 5,
        "first shown card was the text card"
    );
}

#[test]
fn slow_artwork_delays_the_card_only_until_the_wait_limit() {
    let (fixture, pid) = start("slowart");
    let scope = &fixture.scope;
    let sent = Instant::now();
    send(scope, "permission_required", "one");
    let bounds = wait_for_window(pid);
    let waited = sent.elapsed();
    assert!(
        waited >= Duration::from_millis(250),
        "card did not wait for artwork: {waited:?}"
    );
    assert!(
        waited < Duration::from_millis(700),
        "card waited for the stalled worker: {waited:?}"
    );
    assert!(
        (bounds.right - bounds.left) * 2 <= (bounds.bottom - bounds.top) * 5,
        "expected the text card after the wait limit"
    );
    assert_eq!(status(scope).0, pid);
}

#[test]
fn placement_failure_keeps_the_resident_accepting_events_and_recovers() {
    let (fixture, pid) = start("placement");
    let scope = &fixture.scope;
    send(scope, "permission_required", "one");
    let until = Instant::now() + Duration::from_millis(500);
    while Instant::now() < until {
        assert!(
            visible_window(pid).is_none(),
            "card shown without placement"
        );
        assert_eq!(status(scope).0, pid, "resident exited or restarted");
        std::thread::sleep(Duration::from_millis(50));
    }
    send(scope, "input_required", "two");
    assert_eq!(status(scope), (pid, 2));
    std::fs::remove_file(fixture.root.join("aherald.fault")).unwrap();
    // A failed card is retried from scratch on the next wake, not on a timer, so the
    // recovery needs an event; without it the outcome raced the failing draw.
    send(scope, "input_required", "three");
    wait_for_window(pid);
    assert_eq!(status(scope).0, pid);
}

#[test]
fn a_message_sent_during_draw_is_handled_after_it_and_the_resident_keeps_serving() {
    let (fixture, pid) = start("reenter");
    let scope = &fixture.scope;
    send(scope, "permission_required", "one");
    wait_for_window(pid);
    send(scope, "input_required", "two");
    assert_eq!(status(scope), (pid, 2));
    // The deferred display change re-placed the card instead of dropping or hiding it.
    let until = Instant::now() + Duration::from_millis(300);
    while Instant::now() < until {
        assert!(visible_window(pid).is_some(), "card vanished");
        std::thread::sleep(Duration::from_millis(30));
    }
    std::fs::remove_file(fixture.root.join("aherald.fault")).unwrap();
    assert_eq!(status(scope).0, pid);
}
