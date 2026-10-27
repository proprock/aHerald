#![cfg(windows)]
use aherald::{
    ipc::{Request, Response, pipe},
    local::Scope,
    resident,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::w,
};

#[test]
#[ignore = "read-only inspection of the manually started resident"]
fn manual_resident_has_one_notification_window() {
    let scope = Scope::current("resident").unwrap();
    let pid = match pipe::request(&scope, &Request::Status, Duration::from_millis(200)).unwrap() {
        Response::Status { pid, queued, .. } => {
            eprintln!("resident pid={pid}, queued={queued}");
            pid
        }
        _ => panic!("resident is unavailable"),
    };
    struct Found {
        pid: u32,
        cards: usize,
    }
    unsafe extern "system" fn find(h: HWND, p: LPARAM) -> windows::core::BOOL {
        unsafe {
            let state = &mut *(p.0 as *mut Found);
            let mut pid = 0;
            GetWindowThreadProcessId(h, Some(&mut pid));
            if pid == state.pid {
                let mut name = [0u16; 128];
                let n = GetClassNameW(h, &mut name);
                let class = String::from_utf16_lossy(&name[..n as usize]);
                let mut text = [0u16; 256];
                let n = GetWindowTextW(h, &mut text);
                eprintln!(
                    "class={class}, visible={}, title={}",
                    IsWindowVisible(h).as_bool(),
                    String::from_utf16_lossy(&text[..n as usize])
                );
                if class == "aHerald.Resident" {
                    state.cards += 1;
                }
            }
            true.into()
        }
    }
    let mut found = Found { pid, cards: 0 };
    unsafe {
        EnumWindows(Some(find), LPARAM(&mut found as *mut _ as isize)).unwrap();
    }
    assert_eq!(found.cards, 1);
}

#[test]
#[ignore = "requires an interactive Explorer notification area"]
fn explorer_notification_recreates_deleted_icon_and_quit_removes_it() {
    let scope = Scope::current(&format!("tray-test-{}", std::process::id())).unwrap();
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
    struct Found {
        pid: u32,
        window: HWND,
    }
    unsafe extern "system" fn find(h: HWND, p: LPARAM) -> windows::core::BOOL {
        unsafe {
            let state = &mut *(p.0 as *mut Found);
            let mut pid = 0;
            GetWindowThreadProcessId(h, Some(&mut pid));
            let mut name = [0u16; 64];
            let n = GetClassNameW(h, &mut name);
            if pid == state.pid
                && String::from_utf16_lossy(&name[..n as usize]) == "aHerald.Resident"
            {
                state.window = h;
            }
            true.into()
        }
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    let hwnd = loop {
        let mut found = Found {
            pid,
            window: HWND::default(),
        };
        unsafe {
            EnumWindows(Some(find), LPARAM(&mut found as *mut _ as isize)).unwrap();
        }
        if !found.window.is_invalid() {
            break found.window;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    };
    let identity = NOTIFYICONIDENTIFIER {
        cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    while unsafe { Shell_NotifyIconGetRect(&identity) }.is_err() {
        assert!(
            Instant::now() < deadline,
            "Explorer did not register the icon"
        );
        std::thread::yield_now();
    }
    let data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: 1,
        ..Default::default()
    };
    unsafe {
        assert!(Shell_NotifyIconW(NIM_DELETE, &data).as_bool());
        assert!(Shell_NotifyIconGetRect(&identity).is_err());
        let message = RegisterWindowMessageW(w!("TaskbarCreated"));
        SendMessageW(hwnd, message, None, None);
    }
    let deadline = Instant::now() + Duration::from_secs(3);
    while unsafe { Shell_NotifyIconGetRect(&identity) }.is_err() {
        assert!(
            Instant::now() < deadline,
            "TaskbarCreated did not restore the icon"
        );
        std::thread::yield_now();
    }
    pipe::request(&scope, &Request::Quit, Duration::from_millis(200)).unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while scope.acquire().unwrap().is_none() {
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(unsafe { Shell_NotifyIconGetRect(&identity) }.is_err());
}
