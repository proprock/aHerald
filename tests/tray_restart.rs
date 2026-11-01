#![cfg(windows)]
use aherald::{
    ipc::{Request, Response, pipe},
    local::Scope,
};
use std::time::{Duration, Instant};
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
fn window(pid: u32) -> HWND {
    struct Find {
        pid: u32,
        hwnd: HWND,
    }
    unsafe extern "system" fn find(h: HWND, p: LPARAM) -> windows::core::BOOL {
        unsafe {
            let f = &mut *(p.0 as *mut Find);
            let mut pid = 0;
            GetWindowThreadProcessId(h, Some(&mut pid));
            let mut class = [0u16; 64];
            let n = GetClassNameW(h, &mut class);
            if pid == f.pid && String::from_utf16_lossy(&class[..n as usize]) == "aHerald.Resident"
            {
                f.hwnd = h;
            }
        }
        true.into()
    }
    let mut f = Find {
        pid,
        hwnd: HWND::default(),
    };
    unsafe {
        EnumWindows(Some(find), LPARAM(&mut f as *mut _ as isize)).unwrap();
    }
    f.hwnd
}
#[test]
fn restart_changes_pid_reloads_config_and_preserves_isolated_namespace() {
    apply_restarts(false, false);
}

#[test]
fn apply_and_test_restarts_into_a_carousel_inside_the_replacement_resident() {
    apply_restarts(true, false);
}

#[test]
fn real_notification_ends_the_test_without_entering_the_test_list() {
    apply_restarts(true, true);
}

#[test]
#[ignore = "interactive carousel inspection; exposes only its owned window for 90 seconds"]
fn preview_applied_configuration() {
    apply_restarts_preview(true, false, true);
}

fn apply_restarts(test: bool, real: bool) {
    apply_restarts_preview(test, real, false);
}

fn apply_restarts_preview(test: bool, real: bool, preview: bool) {
    let scope = Scope::current(&format!("restart-{test}-{real}-{}", std::process::id())).unwrap();
    let root = std::env::temp_dir().join(&scope.namespace);
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("config with spaces.toml");
    std::fs::write(
        &config,
        "[notifications]\nenabled=false\n[sounds]\nenabled=false",
    )
    .unwrap();
    struct Cleanup(Scope, std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = pipe::request(&self.0, &Request::Quit, Duration::from_millis(200));
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }
    let _cleanup = Cleanup(scope.clone(), root);
    let Response::Status { pid: old, .. } = aherald::resident::ensure(
        std::path::Path::new(env!("CARGO_BIN_EXE_aherald")),
        &config,
        &scope,
    )
    .unwrap() else {
        panic!()
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let hwnd = loop {
        let h = window(old);
        if !h.is_invalid() {
            break h;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    };
    std::fs::write(
        &config,
        format!("[notifications]\nenabled={real}\n[sounds]\nenabled=true"),
    )
    .unwrap();
    unsafe {
        PostMessageW(
            Some(hwnd),
            WM_COMMAND,
            WPARAM(if test {
                aherald::tray::TEST_CONFIG
            } else {
                aherald::tray::RESTART
            }),
            LPARAM(0),
        )
        .unwrap();
    }
    let new = loop {
        if let Ok(Response::Status {
            pid,
            sounds,
            queued,
            paused,
            ..
        }) = pipe::request(&scope, &Request::Status, Duration::from_millis(100))
            && pid != old
        {
            assert!(sounds);
            assert_eq!(queued, 0);
            assert!(!paused);
            break pid;
        }
        assert!(
            Instant::now() < deadline,
            "restart did not publish a replacement"
        );
        std::thread::yield_now();
    };
    assert_ne!(new, old);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !window(old).is_invalid() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(
        scope.acquire().unwrap().is_none(),
        "replacement must own the singleton"
    );
    if test {
        let deadline = Instant::now() + Duration::from_secs(8);
        let hwnd = loop {
            let h = window(new);
            let mut text = [0u16; 200];
            let n = unsafe { GetWindowTextW(h, &mut text) };
            let text = String::from_utf16_lossy(&text[..n as usize]);
            if text.contains("[Test]")
                && text.ends_with("[1/7]")
                && unsafe { IsWindowVisible(h) }.as_bool()
            {
                break h;
            }
            assert!(
                Instant::now() < deadline,
                "resident did not show its carousel: {text}"
            );
            std::thread::yield_now();
        };
        unsafe {
            PostMessageW(Some(hwnd), WM_KEYDOWN, WPARAM(0x25), LPARAM(0)).unwrap();
        }
        loop {
            let mut text = [0u16; 200];
            let n = unsafe { GetWindowTextW(hwnd, &mut text) };
            if String::from_utf16_lossy(&text[..n as usize]).ends_with("[7/7]") {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "carousel did not navigate backwards"
            );
            std::thread::yield_now();
        }
        // Clicking the body advances, while the explicit Dismiss button closes.
        unsafe {
            PostMessageW(Some(hwnd), WM_LBUTTONUP, WPARAM(0), LPARAM((20 << 16) | 20)).unwrap();
        }
        loop {
            let mut text = [0u16; 200];
            let n = unsafe { GetWindowTextW(hwnd, &mut text) };
            if String::from_utf16_lossy(&text[..n as usize]).ends_with("[1/7]") {
                break;
            }
            assert!(Instant::now() < deadline, "body click did not advance");
            std::thread::yield_now();
        }
        if preview {
            // Expose only this owned fixture window to the desktop capture inventory.
            unsafe {
                let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                SetWindowLongPtrW(
                    hwnd,
                    GWL_EXSTYLE,
                    ((style & !WS_EX_TOOLWINDOW.0) | WS_EX_APPWINDOW.0) as isize,
                );
                let _ = ShowWindow(hwnd, SW_HIDE);
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            let until = Instant::now() + Duration::from_secs(90);
            while Instant::now() < until && unsafe { IsWindowVisible(hwnd) }.as_bool() {
                std::thread::park_timeout(Duration::from_millis(100));
            }
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        if real {
            let mut event = aherald::events::Event::demo("permission_required").unwrap();
            event.folder = Some("Real notification".into());
            assert!(matches!(
                pipe::request(&scope, &Request::Event { event }, Duration::from_secs(1)).unwrap(),
                Response::Accepted
            ));
            loop {
                let mut text = [0u16; 200];
                let n = unsafe { GetWindowTextW(hwnd, &mut text) };
                let text = String::from_utf16_lossy(&text[..n as usize]);
                if text.contains("Real notification") && !text.contains("[Test]") {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "real notification did not end test: {text}"
                );
                std::thread::yield_now();
            }
            assert!(
                matches!(pipe::request(&scope, &Request::Status, Duration::from_secs(1)).unwrap(), Response::Status { pid, queued:1, ..} if pid == new)
            );
            return;
        }
        if unsafe { IsWindowVisible(hwnd) }.as_bool() {
            unsafe {
                PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)).unwrap();
            }
        }
        while unsafe { IsWindowVisible(hwnd) }.as_bool() {
            assert!(
                Instant::now() < deadline,
                "closing test did not hide its card"
            );
            std::thread::yield_now();
        }
        assert!(
            matches!(pipe::request(&scope, &Request::Status, Duration::from_secs(1)).unwrap(), Response::Status {pid, queued:0, ..} if pid == new)
        );
        // Ordinary Apply after Apply + Test must not inherit the one-shot test flag.
        unsafe {
            PostMessageW(
                Some(hwnd),
                WM_COMMAND,
                WPARAM(aherald::tray::RESTART),
                LPARAM(0),
            )
            .unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(8);
        let newest = loop {
            if let Ok(Response::Status { pid, .. }) =
                pipe::request(&scope, &Request::Status, Duration::from_millis(100))
                && pid != new
            {
                break pid;
            }
            assert!(Instant::now() < deadline, "second Apply did not restart");
        };
        loop {
            let h = window(newest);
            if !h.is_invalid() {
                assert!(
                    !unsafe { IsWindowVisible(h) }.as_bool(),
                    "ordinary Apply retained test mode"
                );
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
    }
}

#[test]
fn restart_with_invalid_configuration_keeps_the_resident_serving_and_reports_the_error() {
    invalid_apply_keeps_running(false);
}

#[test]
fn apply_and_test_with_invalid_configuration_keeps_the_resident_serving() {
    invalid_apply_keeps_running(true);
}

fn invalid_apply_keeps_running(test: bool) {
    let scope = Scope::current(&format!("restart-invalid-{test}-{}", std::process::id())).unwrap();
    let root = std::env::temp_dir().join(&scope.namespace);
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("config.toml");
    std::fs::write(
        &config,
        "[notifications]\nenabled=false\n[sounds]\nenabled=false",
    )
    .unwrap();
    struct Cleanup(Scope, std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = pipe::request(&self.0, &Request::Quit, Duration::from_millis(200));
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }
    let _cleanup = Cleanup(scope.clone(), root);
    let Response::Status { pid: old, .. } = aherald::resident::ensure(
        std::path::Path::new(env!("CARGO_BIN_EXE_aherald")),
        &config,
        &scope,
    )
    .unwrap() else {
        panic!()
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    let hwnd = loop {
        let h = window(old);
        if !h.is_invalid() {
            break h;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    };
    let restart = || unsafe {
        PostMessageW(
            Some(hwnd),
            WM_COMMAND,
            WPARAM(if test {
                aherald::tray::TEST_CONFIG
            } else {
                aherald::tray::RESTART
            }),
            LPARAM(0),
        )
        .unwrap();
    };
    std::fs::write(&config, "[notifications]\nscale = 99").unwrap();
    restart();
    // The error card is shown even though event visuals are disabled.
    let deadline = Instant::now() + Duration::from_secs(2);
    while !unsafe { IsWindowVisible(hwnd) }.as_bool() {
        assert!(Instant::now() < deadline, "restart error was not shown");
        std::thread::yield_now();
    }
    std::thread::sleep(Duration::from_millis(1500));
    assert!(matches!(
        pipe::request(&scope, &Request::Status, Duration::from_millis(500)).unwrap(),
        Response::Status { pid, .. } if pid == old
    ));
    assert!(!window(old).is_invalid());
    // Once the file is fixed, Restart works again.
    std::fs::write(&config, "[sounds]\nenabled=false").unwrap();
    restart();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(Response::Status { pid, .. }) =
            pipe::request(&scope, &Request::Status, Duration::from_millis(100))
            && pid != old
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "restart did not publish a replacement"
        );
        std::thread::yield_now();
    }
}
