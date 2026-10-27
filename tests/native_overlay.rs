#![cfg(windows)]
use aherald::{
    events::Event,
    process::{Child, deliver},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
struct Owned(Child);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Search {
    pid: u32,
    window: HWND,
}

#[test]
#[ignore = "interactive placement preview; click the card to finish"]
fn preview_placement() {
    let path = std::env::temp_dir().join(format!(
        "aherald-placement-preview-{}.toml",
        std::process::id()
    ));
    std::fs::write(&path, "[notifications]\nposition='bottom_right'\nscale=1.5\n[events.permission_required]\nduration_ms=0\n[sounds]\nenabled=false").unwrap();
    let mut child = Owned(
        deliver(
            &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
            &path,
            &Event::demo("permission_required").unwrap(),
        )
        .unwrap(),
    );
    let mut search = Search {
        pid: child.0.id(),
        window: HWND::default(),
    };
    let deadline = Instant::now() + Duration::from_secs(3);
    while search.window.is_invalid() && Instant::now() < deadline {
        unsafe {
            let _ = EnumWindows(Some(find), LPARAM(&mut search as *mut _ as isize));
        }
        std::thread::yield_now();
    }
    assert!(!search.window.is_invalid());
    unsafe {
        // Only the preview gets a taskbar entry so desktop inspection can find it.
        let style = GetWindowLongPtrW(search.window, GWL_EXSTYLE) as u32;
        SetWindowLongPtrW(
            search.window,
            GWL_EXSTYLE,
            ((style & !WS_EX_TOOLWINDOW.0) | WS_EX_APPWINDOW.0) as isize,
        );
        let _ = ShowWindow(search.window, SW_HIDE);
        let _ = ShowWindow(search.window, SW_SHOWNOACTIVATE);
    }
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline {
        if child.0.try_wait().unwrap().is_some() {
            let _ = std::fs::remove_file(path);
            return;
        }
        std::thread::park_timeout(Duration::from_millis(50));
    }
    let _ = std::fs::remove_file(path);
    panic!("preview was not dismissed");
}
unsafe extern "system" fn find(hwnd: HWND, param: LPARAM) -> windows::core::BOOL {
    // SAFETY: EnumWindows runs synchronously; param points to the live Search on its caller stack.
    unsafe {
        let search = &mut *(param.0 as *mut Search);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == search.pid && IsWindowVisible(hwnd).as_bool() {
            search.window = hwnd;
            return false.into();
        }
        true.into()
    }
}
#[test]
fn overlay_is_nonactivating_layered_and_click_dismisses_indefinite_wait() {
    let path = std::env::temp_dir().join(format!("aherald-native-{}.toml", std::process::id()));
    std::fs::write(&path, "[events.permission_required]\nduration_ms=0\n[notifications]\nfade_in_ms=0\nfade_out_ms=0\n[sounds]\nenabled=false").unwrap();
    let mut child = Owned(
        deliver(
            &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
            &path,
            &Event::demo("permission_required").unwrap(),
        )
        .unwrap(),
    );
    std::fs::remove_file(path).unwrap();
    let mut search = Search {
        pid: child.0.id(),
        window: HWND::default(),
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    while search.window.is_invalid() && Instant::now() < deadline {
        // SAFETY: the callback receives a live Search for this synchronous enumeration.
        unsafe {
            let _ = EnumWindows(Some(find), LPARAM(&mut search as *mut Search as isize));
        }
        std::thread::yield_now();
    }
    assert!(!search.window.is_invalid(), "no visible native overlay");
    // SAFETY: only the owned child's window is inspected or sent a synthetic dismissal.
    unsafe {
        let style = GetWindowLongPtrW(search.window, GWL_EXSTYLE) as u32;
        let required = WS_EX_LAYERED | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TOPMOST;
        assert_eq!(style & required.0, required.0);
        assert_ne!(GetForegroundWindow(), search.window);
        let mut before = RECT::default();
        GetWindowRect(search.window, &mut before).unwrap();
        for message in [WM_DISPLAYCHANGE, WM_DPICHANGED, WM_SETTINGCHANGE] {
            SendMessageW(search.window, message, Some(WPARAM(0)), Some(LPARAM(0)));
        }
        let mut after = RECT::default();
        GetWindowRect(search.window, &mut after).unwrap();
        assert_eq!(
            (before.left, before.top, before.right, before.bottom),
            (after.left, after.top, after.right, after.bottom)
        );
        assert_ne!(GetForegroundWindow(), search.window);
        PostMessageW(Some(search.window), WM_LBUTTONUP, WPARAM(0), LPARAM(0)).unwrap();
    }
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            return;
        }
        std::thread::yield_now();
    }
    panic!("dismissal did not terminate owned overlay");
}

#[test]
fn finite_notification_exits_by_its_timer() {
    let path = std::env::temp_dir().join(format!("aherald-timer-{}.toml", std::process::id()));
    std::fs::write(&path, "[events.turn_complete]\nduration_ms=100\n[notifications]\nfade_in_ms=0\nfade_out_ms=50\n[sounds]\nenabled=false").unwrap();
    let mut child = Owned(
        deliver(
            &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
            &path,
            &Event::demo("turn_complete").unwrap(),
        )
        .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Some(status) = child.0.try_wait().unwrap() {
            std::fs::remove_file(&path).unwrap();
            assert!(status.success());
            return;
        }
        std::thread::yield_now();
    }
    std::fs::remove_file(&path).unwrap();
    panic!("finite notification did not exit");
}

#[test]
fn dismissal_during_fade_in_does_not_flash_to_full_opacity() {
    assert_eq!(
        aherald::notification::presentation(50, 0, 100, 200, Some(50)),
        (127, false)
    );
}

#[test]
fn sound_only_missing_resource_exits_without_creating_an_indefinite_overlay() {
    let path = std::env::temp_dir().join(format!("aherald-audio-only-{}.toml", std::process::id()));
    std::fs::write(&path,"[notifications]\nenabled=false\n[events.permission_required]\nduration_ms=0\nsound='does-not-exist.wav'").unwrap();
    let mut child = Owned(
        deliver(
            &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
            &path,
            &Event::demo("permission_required").unwrap(),
        )
        .unwrap(),
    );
    std::fs::remove_file(path).unwrap();
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success());
            return;
        }
        std::thread::yield_now();
    }
    panic!("sound-only child created an indefinite GUI or blocked on missing WAV");
}
