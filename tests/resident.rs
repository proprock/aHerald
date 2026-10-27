#![cfg(windows)]
use aherald::{
    ipc::{Request, Response, pipe},
    local::Scope,
    resident,
};
use std::{path::PathBuf, time::Duration};

#[test]
fn resident_crash_restarts_and_accepts_fresh_events() {
    use windows::Win32::{Foundation::*, System::Threading::*};
    let scope = Scope::current(&format!("resident-crash-{}", std::process::id())).unwrap();
    let root = std::env::temp_dir().join(&scope.namespace);
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("config.toml");
    std::fs::write(
        &config,
        "[notifications]\nenabled=false\n[sounds]\nenabled=false",
    )
    .unwrap();
    struct Cleanup(Scope, PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = pipe::request(&self.0, &Request::Quit, Duration::from_millis(300));
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            while std::time::Instant::now() < deadline && self.0.acquire().ok().flatten().is_none()
            {
                std::thread::yield_now();
            }
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }
    let _cleanup = Cleanup(scope.clone(), root);
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_aherald"));
    let pid = match resident::ensure(&exe, &config, &scope).unwrap() {
        Response::Status { pid, .. } => pid,
        _ => panic!(),
    };
    // SAFETY: PID was returned by this test's private resident; the handle is
    // closed after a bounded wait. No unrelated process is terminated.
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE | PROCESS_SYNCHRONIZE, false, pid).unwrap();
        TerminateProcess(handle, 1).unwrap();
        let result = WaitForSingleObject(handle, 3000);
        CloseHandle(handle).unwrap();
        assert_eq!(result, WAIT_OBJECT_0);
    }
    resident::forward(
        &exe,
        &config,
        &scope,
        &aherald::events::Event::demo("permission_required").unwrap(),
    )
    .unwrap();
    assert!(matches!(
        pipe::request(&scope, &Request::Status, Duration::from_millis(300)).unwrap(),
        Response::Status { queued: 1, .. }
    ));
}

#[test]
fn disabled_terminal_cards_still_resolve_matching_waits_over_ipc() {
    let root = std::env::temp_dir().join(format!("aherald-resolved-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("config.toml");
    std::fs::write(&config, "[notifications]\nenabled=false\n[sounds]\nenabled=false\n[events.interrupted]\nenabled=false").unwrap();
    let scope = Scope::current(&format!("resolved-test-{}", std::process::id())).unwrap();
    struct Cleanup(Scope, PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = pipe::request(&self.0, &Request::Quit, Duration::from_millis(300));
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            while std::time::Instant::now() < deadline {
                if self.0.acquire().ok().flatten().is_some() {
                    break;
                }
                std::thread::yield_now();
            }
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }
    let _cleanup = Cleanup(scope.clone(), root);
    resident::ensure(
        &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
        &config,
        &scope,
    )
    .unwrap();
    let event = |name: &str| {
        aherald::events::parse_hook(
            format!(r#"{{"hook_event_name":"{name}","session_id":"one","turn_id":"turn-a"}}"#)
                .as_bytes(),
        )
        .unwrap()
        .unwrap()
    };
    for name in [
        "PermissionRequest",
        "PermissionRequest",
        "Interrupt",
        "PermissionRequest",
    ] {
        assert!(matches!(
            pipe::request(
                &scope,
                &Request::Event { event: event(name) },
                Duration::from_millis(500)
            )
            .unwrap(),
            Response::Accepted
        ));
        let expected = if name == "PermissionRequest" {
            None
        } else {
            Some(0)
        };
        if let Some(expected) = expected {
            assert!(
                matches!(pipe::request(&scope, &Request::Status, Duration::from_millis(500)).unwrap(), Response::Status { queued, .. } if queued == expected)
            );
        }
    }
    assert!(matches!(
        pipe::request(&scope, &Request::Status, Duration::from_millis(500)).unwrap(),
        Response::Status { queued: 0, .. }
    ));
}
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
fn sixteen_concurrent_hooks_reach_one_resident_exactly_once() {
    let root = std::env::temp_dir().join(format!("aherald-burst-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("config.toml");
    std::fs::write(
        &config,
        "[notifications]\nenabled=false\n[sounds]\nenabled=false",
    )
    .unwrap();
    let scope = Scope::current(&format!("burst-test-{}", std::process::id())).unwrap();
    struct Cleanup(Scope, PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = pipe::request(&self.0, &Request::Quit, Duration::from_millis(300));
            let deadline = std::time::Instant::now() + Duration::from_secs(3);
            while std::time::Instant::now() < deadline && self.0.acquire().ok().flatten().is_none()
            {
                std::thread::yield_now();
            }
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }
    let _cleanup = Cleanup(scope.clone(), root);
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_aherald"));
    // All hooks start together against a cold resident, as parallel subagent hooks do.
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(16));
    let jobs: Vec<_> = (0..16)
        .map(|n| {
            let (exe, config, scope, barrier) =
                (exe.clone(), config.clone(), scope.clone(), barrier.clone());
            std::thread::spawn(move || {
                let event = aherald::events::parse_hook(
                    format!(
                        r#"{{"hook_event_name":"PermissionRequest","session_id":"burst-{n}","turn_id":"turn-{n}"}}"#
                    )
                    .as_bytes(),
                )
                .unwrap()
                .unwrap();
                barrier.wait();
                resident::forward(&exe, &config, &scope, &event)
            })
        })
        .collect();
    for job in jobs {
        job.join().unwrap().expect("every concurrent hook succeeds");
    }
    assert!(matches!(
        pipe::request(&scope, &Request::Status, Duration::from_millis(500)).unwrap(),
        Response::Status { queued: 16, .. }
    ));
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
    let deadline = Instant::now() + Duration::from_secs(3);
    let bounds = loop {
        let mut bounds = RECT::default();
        unsafe { GetWindowRect(window, &mut bounds).unwrap() };
        if bounds.right - bounds.left > (bounds.bottom - bounds.top) * 3 {
            break bounds;
        }
        assert!(Instant::now() < deadline, "resident artwork did not load");
        std::thread::yield_now();
    };
    let x = bounds.right - bounds.left - 39;
    let y = bounds.bottom - bounds.top - 22;
    unsafe {
        assert_ne!(GetForegroundWindow(), window);
        PostMessageW(
            Some(window),
            WM_LBUTTONUP,
            WPARAM(0),
            LPARAM(((y << 16) | x) as isize),
        )
        .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while !title(window).contains("Input required") {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(title(window).contains("2/2"));
    unsafe {
        PostMessageW(
            Some(window),
            WM_LBUTTONUP,
            WPARAM(0),
            LPARAM((90 << 16) | 100),
        )
        .unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while title(window) != "aHerald demo - Permission required" {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(title(window).contains("Permission required"));
    assert!(matches!(
        pipe::request(&scope, &Request::Status, Duration::from_millis(200)).unwrap(),
        Response::Status { queued: 1, .. }
    ));
    unsafe {
        PostMessageW(Some(window), WM_COMMAND, WPARAM(203), LPARAM(0)).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if matches!(
            pipe::request(&scope, &Request::Status, Duration::from_millis(200)).unwrap(),
            Response::Status { paused: true, .. }
        ) {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "tray pause action did not take effect"
        );
    }
    assert!(!unsafe { IsWindowVisible(window) }.as_bool());
    let mut event = Event::demo("input_required").unwrap();
    event.session_id = Some("while-paused".into());
    pipe::request(
        &scope,
        &Request::Event { event },
        Duration::from_millis(200),
    )
    .unwrap();
    assert!(matches!(
        pipe::request(&scope, &Request::Status, Duration::from_millis(200)).unwrap(),
        Response::Status {
            queued: 2,
            paused: true,
            ..
        }
    ));
    unsafe {
        PostMessageW(Some(window), WM_COMMAND, WPARAM(203), LPARAM(0)).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while !unsafe { IsWindowVisible(window) }.as_bool() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    for command in [201, 202] {
        unsafe {
            SendMessageW(window, WM_COMMAND, Some(WPARAM(command)), Some(LPARAM(0)));
        }
    }
    assert!(matches!(
        pipe::request(&scope, &Request::Status, Duration::from_millis(200)).unwrap(),
        Response::Status {
            sounds: true,
            visuals: false,
            paused: false,
            queued: 2,
            ..
        }
    ));
    assert!(!unsafe { IsWindowVisible(window) }.as_bool());
    unsafe {
        SendMessageW(window, WM_COMMAND, Some(WPARAM(202)), Some(LPARAM(0)));
    }
    assert!(unsafe { IsWindowVisible(window) }.as_bool());
    unsafe {
        SendMessageW(
            window,
            WM_COMMAND,
            Some(WPARAM(aherald::tray::SUMMARY)),
            Some(LPARAM(0)),
        );
    }
    assert!(unsafe { IsWindowVisible(window) }.as_bool());
    let mut found = Found {
        pid,
        windows: Vec::new(),
    };
    unsafe {
        EnumWindows(Some(find), LPARAM(&mut found as *mut _ as isize)).unwrap();
    }
    assert_eq!(
        found.windows,
        vec![window],
        "usage must reuse the notification card"
    );
    assert!(matches!(
        pipe::request(&scope, &Request::Status, Duration::from_millis(200)).unwrap(),
        Response::Status { queued: 3, .. }
    ));
    unsafe {
        SendMessageW(
            window,
            WM_COMMAND,
            Some(WPARAM(aherald::tray::SUMMARY)),
            Some(LPARAM(0)),
        );
    }
    assert!(
        title(window).contains("required"),
        "closing usage restores the selected notification"
    );
    unsafe {
        PostMessageW(Some(window), WM_COMMAND, WPARAM(205), LPARAM(0)).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while scope.acquire().unwrap().is_none() {
        assert!(
            Instant::now() < deadline,
            "tray exit did not release ownership"
        );
        std::thread::yield_now();
    }
}

struct EndCleanup(Scope, PathBuf);
impl Drop for EndCleanup {
    fn drop(&mut self) {
        let _ = pipe::request(&self.0, &Request::Quit, Duration::from_millis(300));
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while std::time::Instant::now() < deadline && self.0.acquire().ok().flatten().is_none() {
            std::thread::yield_now();
        }
        let _ = std::fs::remove_dir_all(&self.1);
    }
}

fn claude_event(kind: aherald::events::Kind, ended: bool) -> aherald::events::Event {
    aherald::events::Event {
        kind,
        source: aherald::events::Source::ClaudeHook,
        session_id: Some("s".into()),
        turn_id: None,
        request_id: None,
        agent_id: None,
        window: None,
        ended,
    }
}

#[test]
fn session_end_clears_a_running_residents_wait_and_never_starts_one() {
    use aherald::events::Kind;
    let scope = Scope::current(&format!("session-end-{}", std::process::id())).unwrap();
    let root = std::env::temp_dir().join(&scope.namespace);
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("config.toml");
    std::fs::write(
        &config,
        "[notifications]\nenabled=false\n[sounds]\nenabled=false",
    )
    .unwrap();
    let _cleanup = EndCleanup(scope.clone(), root);
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_aherald"));
    let end = claude_event(Kind::TurnComplete, true);
    // Nothing is running: there is nothing to clear and no resident is launched for it.
    resident::forward_if_running(&scope, &end).unwrap();
    assert!(pipe::request(&scope, &Request::Status, Duration::from_millis(200)).is_err());
    resident::forward(
        &exe,
        &config,
        &scope,
        &claude_event(Kind::PermissionRequired, false),
    )
    .unwrap();
    let queued = || match pipe::request(&scope, &Request::Status, Duration::from_millis(300)) {
        Ok(Response::Status { queued, .. }) => queued,
        other => panic!("{other:?}"),
    };
    assert_eq!(queued(), 1);
    resident::forward_if_running(&scope, &end).unwrap();
    assert_eq!(queued(), 0);
}
