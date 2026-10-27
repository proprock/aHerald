#![cfg(windows)]
use aherald::{tray, usage::Snapshot, usage_service::View};
use serde_json::json;
use std::time::Duration;
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};

fn fixture() -> String {
    let mut view = View::new(true, Duration::from_secs(300));
    view.updated = Some(Duration::ZERO);
    view.snapshot = Some(Snapshot::parse(json!({"rateLimitsByLimitId":{
        "codex":{"limitName":"Codex","planType":"pro","primary":{"usedPercent":25,"windowDurationMins":300,"resetsAt":1792890000},"secondary":{"usedPercent":71,"windowDurationMins":10080,"resetsAt":1793494800},"credits":{"hasCredits":true,"unlimited":false,"balance":"12.50"}},
        "other":{"limitName":"Additional bucket","primary":{"usedPercent":0,"windowDurationMins":60,"resetsAt":1792889940}}
    },"rateLimitResetCredits":{"availableCount":2,"credits":null}})).unwrap());
    view.details(Duration::from_secs(12))
}
#[test]
fn native_details_control_contains_all_buckets_updates_and_scrolls() {
    let text = fixture();
    let window = tray::information(HWND::default(), "aHerald usage test", &text).unwrap();
    struct Window(HWND);
    impl Drop for Window {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }
    let _owned = Window(window);
    unsafe {
        let child = GetDlgItem(Some(window), 1).unwrap();
        let mut actual = vec![0u16; text.encode_utf16().count() + 1];
        let count = GetWindowTextW(child, &mut actual);
        let actual = String::from_utf16_lossy(&actual[..count as usize]);
        assert!(
            actual.contains("Additional bucket")
                && actual.contains("25%")
                && actual.contains("71%")
                && actual.contains("0%")
        );
        assert_ne!(GetWindowLongW(child, GWL_STYLE) as u32 & WS_VSCROLL.0, 0);
        tray::update_information(
            window,
            "Codex usage: stale\r\nLast successful snapshot retained",
        );
        let mut updated = [0u16; 256];
        let count = GetWindowTextW(child, &mut updated);
        assert!(String::from_utf16_lossy(&updated[..count as usize]).contains("stale"));
        SendMessageW(child, WM_CHAR, Some(WPARAM('X' as usize)), Some(LPARAM(0)));
        let after = GetWindowTextW(child, &mut updated);
        assert_eq!(after, count, "usage details must remain read-only");
        let long = (0..150)
            .map(|n| format!("Additional bucket {n}\r\n"))
            .collect::<String>();
        tray::update_information(window, &long);
        SendMessageW(
            child,
            WM_VSCROLL,
            Some(WPARAM(SB_BOTTOM.0 as usize)),
            Some(LPARAM(0)),
        );
        assert!(
            GetScrollPos(child, SB_VERT) > 0,
            "all buckets must remain reachable by scrolling"
        );
        tray::update_information(window, &format!("{long}Refreshed\r\n"));
        assert!(
            GetScrollPos(child, SB_VERT) > 0,
            "background refresh must preserve the user's scroll position"
        );
    }
}

#[test]
#[ignore = "interactive visual preview; close the window to finish"]
fn preview_usage_details() {
    let window = tray::information(
        HWND::default(),
        "aHerald usage preview — synthetic data",
        &fixture(),
    )
    .unwrap();
    unsafe {
        // Expose this unowned test preview in desktop inspection tools. Production
        // information windows remain owned by the resident notification window.
        SetWindowLongW(window, GWL_EXSTYLE, WS_EX_APPWINDOW.0 as i32);
        let _ = ShowWindow(window, SW_HIDE);
        let _ = ShowWindow(window, SW_SHOWNORMAL);
        let mut message = MSG::default();
        while IsWindow(Some(window)).as_bool() {
            if PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            } else {
                MsgWaitForMultipleObjects(None, false, 100, QS_ALLINPUT);
            }
        }
    }
}
