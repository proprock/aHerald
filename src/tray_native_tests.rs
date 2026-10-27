use super::*;
use crate::{
    config::{TrayGraph, UsageDisplay},
    tray_usage::Levels,
    usage::Snapshot,
    usage_service::View,
};
use std::time::Duration;
use windows::Win32::Graphics::Gdi::*;

fn levels(used: f64) -> Levels {
    let mut view = View::new(true, Duration::from_secs(300));
    view.snapshot=Some(Snapshot::parse(serde_json::json!({"rateLimits":{"primary":{"usedPercent":used},"secondary":{"usedPercent":90}}})).unwrap());
    Levels::from_view(&view, UsageDisplay::Remain)
}

fn graph_levels(graph: TrayGraph, display: UsageDisplay) -> Levels {
    let mut codex = View::new(true, Duration::from_secs(300));
    codex.snapshot = Some(
        Snapshot::parse(serde_json::json!({"rateLimitsByLimitId":{"codex":{
            "primary":{"usedPercent":80},"secondary":{"usedPercent":0}
        }}}))
        .unwrap(),
    );
    let mut claude = View::new(true, Duration::from_secs(300));
    claude.snapshot = Some(
        Snapshot::parse(serde_json::json!({"rateLimitsByLimitId":{"claude":{
            "primary":{"usedPercent":0},"secondary":{"usedPercent":50}
        }}}))
        .unwrap(),
    );
    Levels::from_views(&codex, Some(&claude), graph, display)
}

#[test]
fn split_native_icon_preserves_four_colors_and_reuses_and_restores_handle() {
    let mut icon = Icon::new(HWND::default()).unwrap();
    let levels = graph_levels(TrayGraph::Both, UsageDisplay::Remain);
    assert!(icon.set_levels(levels.clone(), 16).unwrap());
    let current = icon.data.hIcon;
    assert!(!icon.set_levels(levels, 16).unwrap());
    assert_eq!(icon.data.hIcon, current);
    let mut info = ICONINFO::default();
    // SAFETY: the icon stays owned; bitmap copies and the compatible DC are released below.
    unsafe {
        GetIconInfo(current, &mut info).unwrap();
        let dc = CreateCompatibleDC(None);
        assert!(!dc.is_invalid());
        let old = SelectObject(dc, info.hbmColor.into());
        for (x, color) in [(1, 0x55b422), (4, 0x00a5d8), (9, 0x4444e0), (12, 0x55b422)] {
            assert_eq!(GetPixel(dc, x, 14), COLORREF(color));
        }
        SelectObject(dc, old);
        let _ = DeleteDC(dc);
        let _ = DeleteObject(info.hbmColor.into());
        let _ = DeleteObject(info.hbmMask.into());
    }
    icon.restore();
    assert_eq!(icon.data.hIcon, current);
    assert!(
        icon.set_levels(graph_levels(TrayGraph::Claude, UsageDisplay::Remain), 16)
            .unwrap()
    );
}

#[test]
fn dynamic_native_icon_renders_updates_and_reuses_unchanged_pixels() {
    let mut icon = Icon::new(HWND::default()).unwrap();
    assert!(icon.set_levels(levels(25.), 16).unwrap());
    assert!(
        !icon.set_levels(levels(24.9), 16).unwrap(),
        "unchanged quantized pixels must reuse the native handle"
    );
    let mut info = ICONINFO::default();
    // SAFETY: the icon remains owned; GetIconInfo returns bitmap copies we delete below.
    unsafe {
        GetIconInfo(icon.data.hIcon, &mut info).unwrap();
        let mut bitmap = BITMAP::default();
        assert_ne!(
            GetObjectW(
                info.hbmColor.into(),
                std::mem::size_of::<BITMAP>() as i32,
                Some(&mut bitmap as *mut _ as *mut _)
            ),
            0
        );
        assert_eq!((bitmap.bmWidth, bitmap.bmHeight), (16, 16));
        let dc = CreateCompatibleDC(None);
        assert!(!dc.is_invalid());
        let old = SelectObject(dc, info.hbmColor.into());
        assert_eq!(GetPixel(dc, 2, 14), COLORREF(0x55b422));
        assert_eq!(GetPixel(dc, 10, 14), COLORREF(0x4444e0));
        SelectObject(dc, old);
        let _ = DeleteDC(dc);
        let _ = DeleteObject(info.hbmColor.into());
        let _ = DeleteObject(info.hbmMask.into());
    }
    assert!(icon.set_levels(levels(90.), 24).unwrap());
    // No Explorer is required here: restore retains the same current native image.
    let current = icon.data.hIcon;
    icon.restore();
    assert_eq!(icon.data.hIcon, current);
}

#[test]
fn invalid_render_size_preserves_current_native_image() {
    let mut icon = Icon::new(HWND::default()).unwrap();
    let current = icon.data.hIcon;
    assert!(icon.set_levels(levels(20.), 0).is_err());
    assert_eq!(icon.data.hIcon, current);
}

#[test]
#[ignore = "native visual preview; opens a dedicated window and temporary tray icon"]
fn manual_dynamic_tray_preview() {
    unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        if msg == WM_DESTROY {
            unsafe { PostQuitMessage(0) };
            return LRESULT(0);
        }
        unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
    }
    // SAFETY: this test owns the window, icon and all temporary GDI objects on one thread.
    unsafe {
        use windows::Win32::UI::HiDpi::*;
        struct Dpi(DPI_AWARENESS_CONTEXT);
        impl Drop for Dpi {
            fn drop(&mut self) {
                unsafe {
                    SetThreadDpiAwarenessContext(self.0);
                }
            }
        }
        let previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        assert!(!previous.is_invalid());
        let _dpi = Dpi(previous);
        let class = w!("aHerald.AH034Preview");
        let module = windows::Win32::System::LibraryLoader::GetModuleHandleW(None).unwrap();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: module.into(),
            lpszClassName: class,
            ..Default::default()
        };
        assert_ne!(RegisterClassW(&wc), 0);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            w!("Tray graph: auto / both / claude / codex; 16 / 20 / 24 / 32 px; remain + used"),
            WS_OVERLAPPEDWINDOW | WS_VISIBLE,
            120,
            120,
            760,
            440,
            None,
            None,
            Some(module.into()),
            None,
        )
        .unwrap();
        let mut icon = Icon::new(hwnd).unwrap();
        icon.usage(levels(25.)).unwrap();
        let dc = GetDC(Some(hwnd));
        let brushes = [
            CreateSolidBrush(COLORREF(0xf3f3f3)),
            CreateSolidBrush(COLORREF(0x202020)),
        ];
        let mut images = Vec::new();
        for (row, graph) in [
            TrayGraph::Auto,
            TrayGraph::Both,
            TrayGraph::Claude,
            TrayGraph::Codex,
        ]
        .into_iter()
        .enumerate()
        {
            for (column, size) in [16, 20, 24, 32].into_iter().enumerate() {
                for (mode, display) in [UsageDisplay::Remain, UsageDisplay::Used]
                    .into_iter()
                    .enumerate()
                {
                    let image =
                        NativeIcon::new(&graph_levels(graph, display).render(size).unwrap(), size)
                            .unwrap();
                    let x = 20 + column as i32 * 170 + mode as i32 * 70;
                    let y = 30 + row as i32 * 76;
                    for (background, brush) in brushes.iter().enumerate() {
                        let y = y + background as i32 * 34;
                        let rect = RECT {
                            left: x - 5,
                            top: y - 3,
                            right: x + 45,
                            bottom: y + 32,
                        };
                        FillRect(dc, &rect, *brush);
                        DrawIconEx(
                            dc,
                            x,
                            y,
                            image.0,
                            size as i32,
                            size as i32,
                            0,
                            None,
                            DI_NORMAL,
                        )
                        .unwrap();
                    }
                    images.push(image);
                }
            }
        }
        ReleaseDC(Some(hwnd), dc);
        let end = std::time::Instant::now() + Duration::from_secs(180);
        let mut message = MSG::default();
        while std::time::Instant::now() < end && IsWindow(Some(hwnd)).as_bool() {
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        drop(icon);
        if IsWindow(Some(hwnd)).as_bool() {
            DestroyWindow(hwnd).unwrap();
        }
        for brush in brushes {
            let _ = DeleteObject(brush.into());
        }
        UnregisterClassW(class, Some(module.into())).unwrap();
    }
}

#[test]
#[ignore = "requires an interactive Explorer notification area"]
fn explorer_restore_keeps_current_dynamic_image() {
    struct Window(HWND);
    impl Drop for Window {
        fn drop(&mut self) {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }
    // SAFETY: this test owns its hidden window and tray identity; no existing icon is touched.
    unsafe {
        let window = Window(
            CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                w!("STATIC"),
                w!("AH034 isolated restore"),
                WS_POPUP,
                0,
                0,
                10,
                10,
                None,
                None,
                None,
                None,
            )
            .unwrap(),
        );
        let mut icon = Icon::new(window.0).unwrap();
        icon.set_levels(graph_levels(TrayGraph::Both, UsageDisplay::Remain), 16)
            .unwrap();
        let current = icon.data.hIcon;
        let raster = icon.raster.clone();
        let identity = NOTIFYICONIDENTIFIER {
            cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
            hWnd: window.0,
            uID: 1,
            ..Default::default()
        };
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while Shell_NotifyIconGetRect(&identity).is_err() {
            assert!(
                std::time::Instant::now() < deadline,
                "Explorer did not register the dynamic icon"
            );
            std::thread::yield_now();
        }
        assert!(Shell_NotifyIconW(NIM_DELETE, &icon.data).as_bool());
        assert!(Shell_NotifyIconGetRect(&identity).is_err());
        assert!(icon.restore());
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while Shell_NotifyIconGetRect(&identity).is_err() {
            assert!(
                std::time::Instant::now() < deadline,
                "Explorer did not register the dynamic icon"
            );
            std::thread::yield_now();
        }
        assert_eq!(icon.data.hIcon, current);
        assert_eq!(icon.raster, raster);
        drop(icon);
        assert!(Shell_NotifyIconGetRect(&identity).is_err());
    }
}
