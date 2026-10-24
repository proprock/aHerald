#![cfg(windows)]
use aherald::{events::Event, process::deliver};
use std::{
    path::PathBuf,
    process::Child,
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
    hwnd: HWND,
}
unsafe extern "system" fn find(hwnd: HWND, p: LPARAM) -> windows::core::BOOL {
    // SAFETY: EnumWindows calls synchronously with this live Search pointer.
    unsafe {
        let s = &mut *(p.0 as *mut Search);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == s.pid && IsWindowVisible(hwnd).as_bool() {
            s.hwnd = hwnd;
            return false.into();
        }
        true.into()
    }
}
#[test]
fn native_window_keeps_card_beside_external_frames_and_recovers_to_text() {
    let root = std::env::temp_dir().join(format!("aherald-resource-native-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    struct Cleanup(PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let _cleanup = Cleanup(root.clone());
    let corrupt = root.join("corrupt");
    std::fs::create_dir(&corrupt).unwrap();
    std::fs::write(
        corrupt.join("animation.toml"),
        "type='spritesheet'\nimage='sheet.png'\nframe_width=64\nframe_height=64\nframes=1",
    )
    .unwrap();
    std::fs::write(corrupt.join("sheet.png"), b"broken PNG").unwrap();
    let valid = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets/turn_complete");
    for (path, is_image) in [
        (valid, true),
        (corrupt, false),
        (root.join("missing"), false),
    ] {
        let config = root.join("config.toml");
        std::fs::write(&config,format!("[sounds]\nenabled=false\n[notifications]\nfade_in_ms=0\nfade_out_ms=0\n[events.turn_complete]\nduration_ms=0\nanimation='{}'",path.display())).unwrap();
        let mut child = Owned(
            deliver(
                &PathBuf::from(env!("CARGO_BIN_EXE_aherald")),
                &config,
                &Event::demo("turn_complete").unwrap(),
            )
            .unwrap(),
        );
        let mut search = Search {
            pid: child.0.id(),
            hwnd: HWND::default(),
        };
        let deadline = Instant::now() + Duration::from_secs(2);
        while search.hwnd.is_invalid() && Instant::now() < deadline {
            // SAFETY: the callback receives a live stack object for synchronous enumeration.
            unsafe {
                let _ = EnumWindows(Some(find), LPARAM(&mut search as *mut Search as isize));
            }
            std::thread::yield_now();
        }
        assert!(
            !search.hwnd.is_invalid(),
            "resource failure prevented fallback window"
        );
        // SAFETY: only the live window of this test's owned child is inspected/dismissed.
        unsafe {
            let mut rect = RECT::default();
            GetWindowRect(search.hwnd, &mut rect).unwrap();
            let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
            if is_image {
                // A 64x64 decoration joins the 320x110 card with an 18px gap.
                assert!(
                    width > height * 3,
                    "text space must remain beside the frame"
                );
            } else {
                assert!(width > height * 2);
            }
            PostMessageW(Some(search.hwnd), WM_LBUTTONUP, WPARAM(0), LPARAM(0)).unwrap();
        }
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                Instant::now() < deadline,
                "resource window failed to dismiss"
            );
            std::thread::yield_now();
        }
    }
}
