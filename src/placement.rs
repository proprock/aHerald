#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Fit the entire card, retaining aspect ratio, into physical work-area pixels.
pub fn layout(
    a: Area,
    size: (i32, i32),
    dpi_scale: f64,
    position: &str,
    cursor: (i32, i32),
) -> (i32, i32, i32, i32) {
    let aw = (i64::from(a.right) - i64::from(a.left)).max(1) as f64;
    let ah = (i64::from(a.bottom) - i64::from(a.top)).max(1) as f64;
    let factor = dpi_scale
        .min(aw / size.0.max(1) as f64)
        .min(ah / size.1.max(1) as f64);
    let w = (size.0 as f64 * factor).floor().max(1.0) as i32;
    let h = (size.1 as f64 * factor).floor().max(1.0) as i32;
    let margin = (24.0 * dpi_scale) as i64;
    let cx = (i64::from(a.left) + i64::from(a.right) - i64::from(w)) / 2;
    let cy = (i64::from(a.top) + i64::from(a.bottom) - i64::from(h)) / 2;
    let (x, y) = match position {
        "top_center" => (cx, i64::from(a.top) + margin),
        "bottom_right" => (
            i64::from(a.right) - i64::from(w) - margin,
            i64::from(a.bottom) - i64::from(h) - margin,
        ),
        "near_cursor" => (
            i64::from(cursor.0) + (16.0 * dpi_scale) as i64,
            i64::from(cursor.1) + (16.0 * dpi_scale) as i64,
        ),
        _ => (cx, cy),
    };
    (
        x.clamp(
            a.left as i64,
            (i64::from(a.right) - i64::from(w)).max(a.left as i64),
        ) as i32,
        y.clamp(
            a.top as i64,
            (i64::from(a.bottom) - i64::from(h)).max(a.top as i64),
        ) as i32,
        w,
        h,
    )
}

#[cfg(windows)]
pub(crate) fn current(
    config: &crate::config::Notifications,
    size: (i32, i32),
    frames: usize,
) -> Result<(i32, i32, i32, i32), String> {
    use windows::Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        UI::{HiDpi::*, WindowsAndMessaging::*},
    };
    unsafe extern "system" fn collect(
        monitor: HMONITOR,
        _: HDC,
        _: *mut RECT,
        data: LPARAM,
    ) -> windows::core::BOOL {
        // SAFETY: synchronous enumeration owns the live Vec for the duration of the callback.
        unsafe {
            (*(data.0 as *mut Vec<HMONITOR>)).push(monitor);
        }
        true.into()
    }
    // SAFETY: monitor handles are borrowed snapshots; all output buffers live across each call.
    unsafe {
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let mut monitors = Vec::<HMONITOR>::new();
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect),
            LPARAM(&mut monitors as *mut _ as isize),
        );
        let primary = MonitorFromPoint(POINT::default(), MONITOR_DEFAULTTOPRIMARY);
        monitors.sort_by_key(|m| {
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            let _ = GetMonitorInfoW(*m, &mut info.monitorInfo);
            (*m != primary, info.szDevice)
        });
        let selected = config
            .monitor
            .parse::<usize>()
            .ok()
            .and_then(|i| monitors.get(i).copied());
        let monitor = selected.unwrap_or_else(|| {
            if config.position == "near_cursor" && config.monitor == "primary" {
                MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST)
            } else {
                primary
            }
        });
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return Err("cannot determine monitor work area".into());
        }
        let (mut dx, mut dy) = (96, 96);
        let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
        let a = info.rcWork;
        Ok(layout(
            Area {
                left: a.left,
                top: a.top,
                right: a.right,
                bottom: a.bottom,
            },
            size,
            (dx.max(96) as f64 / 96.0).min(
                (crate::animation::MAX_BYTES as f64
                    / (size.0 as f64 * size.1 as f64 * 4.0 * frames.max(1) as f64))
                    .sqrt(),
            ),
            &config.position,
            (cursor.x, cursor.y),
        ))
    }
}
