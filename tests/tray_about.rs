#![cfg(windows)]
use aherald::{
    ipc::{Request, Response, pipe},
    local::Scope,
};
use std::time::{Duration, Instant};
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};

fn resident_window(pid: u32) -> HWND {
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

fn wait_until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(3);
    while !condition() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::yield_now();
    }
}

#[test]
fn the_about_item_shows_a_card_even_with_event_visuals_off_and_a_second_choice_closes_it() {
    let scope = Scope::current(&format!("about-{}", std::process::id())).unwrap();
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
    let Response::Status { pid, .. } = aherald::resident::ensure(
        std::path::Path::new(env!("CARGO_BIN_EXE_aherald")),
        &config,
        &scope,
    )
    .unwrap() else {
        panic!("unexpected status response")
    };
    let mut hwnd = HWND::default();
    wait_until("the resident window", || {
        hwnd = resident_window(pid);
        !hwnd.is_invalid()
    });
    let choose_about = || unsafe {
        PostMessageW(
            Some(hwnd),
            WM_COMMAND,
            WPARAM(aherald::tray::ABOUT),
            LPARAM(0),
        )
        .unwrap();
    };
    assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
    choose_about();
    wait_until("the About card", || {
        unsafe { IsWindowVisible(hwnd) }.as_bool()
    });
    let mut bounds = RECT::default();
    unsafe { GetWindowRect(hwnd, &mut bounds).unwrap() };
    // A usage-style card is wide and tall enough for its body.
    assert!(bounds.right - bounds.left > 500, "{bounds:?}");
    choose_about();
    wait_until("the About card to close", || {
        !unsafe { IsWindowVisible(hwnd) }.as_bool()
    });
}
