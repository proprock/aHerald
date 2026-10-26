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
    let scope = Scope::current(&format!("restart-{}", std::process::id())).unwrap();
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
        "[notifications]\nenabled=false\n[sounds]\nenabled=true",
    )
    .unwrap();
    unsafe {
        PostMessageW(
            Some(hwnd),
            WM_COMMAND,
            WPARAM(aherald::tray::RESTART),
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
}

#[test]
fn restart_with_invalid_configuration_keeps_the_resident_serving_and_reports_the_error() {
    let scope = Scope::current(&format!("restart-invalid-{}", std::process::id())).unwrap();
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
            WPARAM(aherald::tray::RESTART),
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
