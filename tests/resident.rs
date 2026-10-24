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

#[test]
fn resident_uses_one_window_and_switches_and_dismisses_only_selected_event() {
    use aherald::events::Event;
    use std::time::Instant;
    use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
    let scope = Scope::current(&format!("resident-ui-{}", std::process::id())).unwrap();
    let root = std::env::temp_dir().join(&scope.namespace);
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("config.toml");
    std::fs::write(
        &config,
        "[sounds]\nenabled=false\n[notifications]\nfade_in_ms=0\nfade_out_ms=0",
    )
    .unwrap();
    struct Cleanup(Scope, PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = pipe::request(&self.0, &Request::Quit, Duration::from_millis(200));
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }
    let _cleanup = Cleanup(scope.clone(), root);
    let pid = match resident::ensure(
        &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
        &config,
        &scope,
    )
    .unwrap()
    {
        Response::Status { pid, .. } => pid,
        _ => panic!(),
    };
    for (kind, session) in [("permission_required", "one"), ("input_required", "two")] {
        let mut event = Event::demo(kind).unwrap();
        event.session_id = Some(session.into());
        pipe::request(
            &scope,
            &Request::Event { event },
            Duration::from_millis(200),
        )
        .unwrap();
    }
    struct Found {
        pid: u32,
        windows: Vec<HWND>,
    }
    unsafe extern "system" fn find(h: HWND, p: LPARAM) -> windows::core::BOOL {
        unsafe {
            let state = &mut *(p.0 as *mut Found);
            let mut pid = 0;
            GetWindowThreadProcessId(h, Some(&mut pid));
            if pid == state.pid && IsWindowVisible(h).as_bool() {
                state.windows.push(h);
            }
            true.into()
        }
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    let window = loop {
        let mut found = Found {
            pid,
            windows: Vec::new(),
        };
        unsafe {
            let _ = EnumWindows(Some(find), LPARAM(&mut found as *mut Found as isize));
        }
        if !found.windows.is_empty() {
            assert_eq!(found.windows.len(), 1);
            break found.windows[0];
        }
        assert!(
            Instant::now() < deadline,
            "resident did not show its single card"
        );
        std::thread::yield_now();
    };
    fn title(h: HWND) -> String {
        let mut text = [0u16; 256];
        let n = unsafe { GetWindowTextW(h, &mut text) };
        String::from_utf16_lossy(&text[..n as usize])
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while !title(window).contains("1/2") {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    unsafe {
        assert_ne!(GetForegroundWindow(), window);
        PostMessageW(Some(window), WM_KEYDOWN, WPARAM(0x27), LPARAM(0)).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while !title(window).contains("Input required") {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    unsafe {
        PostMessageW(
            Some(window),
            WM_LBUTTONUP,
            WPARAM(0),
            LPARAM((90 << 16) | 220),
        )
        .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while !title(window).contains("1/1") {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(title(window).contains("Permission required"));
    assert!(matches!(
        pipe::request(&scope, &Request::Status, Duration::from_millis(200)).unwrap(),
        Response::Status { queued: 1, .. }
    ));
}
