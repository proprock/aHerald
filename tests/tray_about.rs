#![cfg(windows)]
mod support;
use aherald::{ipc::Response, local::Scope};
use std::time::{Duration, Instant};
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};

fn resident_window(pid: u32) -> HWND {
    struct Find {
        pid: u32,
        hwnd: HWND,
    }
    unsafe extern "system" fn find(h: HWND, p: LPARAM) -> windows::core::BOOL {
        // SAFETY: EnumWindows passes a valid `h`; `p` is the address of the `Find` that the caller
        // keeps alive for the whole synchronous enumeration.
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
    // SAFETY: `find` matches the callback signature and `f` outlives the synchronous enumeration
    // whose address is passed as `LPARAM`.
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

    let mut cleanup = support::detached::Cleanup::new(scope.clone(), root);
    let Response::Status { pid, .. } = aherald::resident::ensure(
        std::path::Path::new(env!("CARGO_BIN_EXE_aherald")),
        &config,
        &scope,
    )
    .unwrap() else {
        panic!("unexpected status response")
    };
    cleanup.0.retain(pid).unwrap();
    let mut hwnd = HWND::default();
    wait_until("the resident window", || {
        hwnd = resident_window(pid);
        !hwnd.is_invalid()
    });
    // SAFETY: `hwnd` is the resident window found above; posting a message passes plain values.
    let choose_about = || unsafe {
        PostMessageW(
            Some(hwnd),
            WM_COMMAND,
            WPARAM(aherald::tray::ABOUT),
            LPARAM(0),
        )
        .unwrap();
    };
    // SAFETY: `hwnd` is the resident window found above.
    assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
    choose_about();
    wait_until("the About card", || {
        // SAFETY: `hwnd` is the resident window found above.
        unsafe { IsWindowVisible(hwnd) }.as_bool()
    });
    let mut bounds = RECT::default();
    // SAFETY: `hwnd` is the resident window found above and `bounds` is a live RECT.
    unsafe { GetWindowRect(hwnd, &mut bounds).unwrap() };
    // A usage-style card is wide and tall enough for its body.
    assert!(bounds.right - bounds.left > 500, "{bounds:?}");
    choose_about();
    wait_until("the About card to close", || {
        // SAFETY: `hwnd` is the resident window found above.
        !unsafe { IsWindowVisible(hwnd) }.as_bool()
    });
}
