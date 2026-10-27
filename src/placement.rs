#[cfg(windows)]
use crate::config::Monitor;
use crate::config::Position;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Area {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// Fit the entire card, retaining aspect ratio, into physical work-area pixels. The
/// card is drawn at 96 DPI and enlarged by `dpi_scale`.
pub fn layout(
    a: Area,
    size: (i32, i32),
    dpi_scale: f64,
    position: Position,
    cursor: (i32, i32),
) -> (i32, i32, i32, i32) {
    place(a, size, dpi_scale, dpi_scale, position, cursor)
}

/// Like `layout`, for a card already drawn at the monitor's DPI: its pixels are
/// physical, so it is only ever shrunk (to fit), and `dpi_scale` scales the margins.
pub fn layout_rendered(
    a: Area,
    size: (i32, i32),
    dpi_scale: f64,
    position: Position,
    cursor: (i32, i32),
) -> (i32, i32, i32, i32) {
    place(a, size, 1.0, dpi_scale, position, cursor)
}

fn place(
    a: Area,
    size: (i32, i32),
    zoom: f64,
    dpi_scale: f64,
    position: Position,
    cursor: (i32, i32),
) -> (i32, i32, i32, i32) {
    let aw = (i64::from(a.right) - i64::from(a.left)).max(1) as f64;
    let ah = (i64::from(a.bottom) - i64::from(a.top)).max(1) as f64;
    let factor = zoom
        .min(aw / size.0.max(1) as f64)
        .min(ah / size.1.max(1) as f64);
    let w = (size.0 as f64 * factor).floor().max(1.0) as i32;
    let h = (size.1 as f64 * factor).floor().max(1.0) as i32;
    let margin = (24.0 * dpi_scale) as i64;
    let cx = (i64::from(a.left) + i64::from(a.right) - i64::from(w)) / 2;
    let cy = (i64::from(a.top) + i64::from(a.bottom) - i64::from(h)) / 2;
    let (x, y) = match position {
        Position::TopCenter => (cx, i64::from(a.top) + margin),
        Position::BottomRight => (
            i64::from(a.right) - i64::from(w) - margin,
            i64::from(a.bottom) - i64::from(h) - margin,
        ),
        Position::NearCursor => (
            i64::from(cursor.0) + (16.0 * dpi_scale) as i64,
            i64::from(cursor.1) + (16.0 * dpi_scale) as i64,
        ),
        Position::Center => (cx, cy),
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

/// Move a card `(x, y, width, height)` past already shown cards so none overlap. Cards
/// anchored at the bottom stack upward, all others downward. When the work area has no
/// room left, the base spot is kept rather than pushing a card off screen.
pub fn stack(
    card: (i32, i32, i32, i32),
    area: Area,
    occupied: &[Area],
    position: Position,
    gap: i32,
) -> (i32, i32) {
    let (x, base_y, w, h) = card;
    let up = position == Position::BottomRight;
    let mut y = base_y;
    for _ in 0..=occupied.len() {
        let Some(hit) = occupied
            .iter()
            .find(|o| x < o.right && o.left < x + w && y < o.bottom && o.top < y + h)
        else {
            return (x, y);
        };
        y = if up {
            hit.top - gap - h
        } else {
            hit.bottom + gap
        };
        if y < area.top || y + h > area.bottom {
            break;
        }
    }
    (x, base_y)
}

/// Window class shared with `overlay::show`; other processes' cards are found by it.
#[cfg(windows)]
pub(crate) const TRANSIENT_CLASS: &str = "aHerald.Overlay";

/// Visible transient cards of any process, excluding `exclude` (a raw `HWND`, or 0).
#[cfg(windows)]
fn transient_cards(exclude: isize) -> Vec<Area> {
    use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
    struct Search {
        exclude: isize,
        found: Vec<Area>,
    }
    unsafe extern "system" fn collect(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
        // SAFETY: synchronous enumeration owns the live Search for the callback's duration.
        unsafe {
            let search = &mut *(data.0 as *mut Search);
            let mut class = [0u16; 32];
            let length = GetClassNameW(hwnd, &mut class).max(0) as usize;
            let mut rect = RECT::default();
            if hwnd.0 as isize != search.exclude
                && String::from_utf16_lossy(&class[..length]) == TRANSIENT_CLASS
                && IsWindowVisible(hwnd).as_bool()
                && GetWindowRect(hwnd, &mut rect).is_ok()
            {
                search.found.push(Area {
                    left: rect.left,
                    top: rect.top,
                    right: rect.right,
                    bottom: rect.bottom,
                });
            }
        }
        true.into()
    }
    let mut search = Search {
        exclude,
        found: Vec::new(),
    };
    // SAFETY: `search` outlives the synchronous enumeration.
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut search as *mut Search as isize));
    }
    search.found
}

/// Work area, DPI scale (1.0 at 96 DPI) and cursor position of the monitor that would
/// show a card now.
#[cfg(windows)]
struct Target {
    area: Area,
    dpi_scale: f64,
    cursor: (i32, i32),
}

/// DPI scale of the monitor a card would be shown on, to draw the card at that DPI.
#[cfg(windows)]
pub(crate) fn dpi_scale(config: &crate::config::Notifications) -> f64 {
    target(config).map_or(1.0, |t| t.dpi_scale)
}

/// Placement for a card drawn at `dpi_scale(config)`, so its size is in physical pixels.
/// `avoid_cards` is `Some(own_window)` (0 before the window exists) to stack clear of
/// other transient cards; the resident's single window passes `None`.
#[cfg(windows)]
pub(crate) fn current(
    config: &crate::config::Notifications,
    size: (i32, i32),
    avoid_cards: Option<isize>,
) -> Result<(i32, i32, i32, i32), String> {
    let _timer = crate::perf::span(crate::perf::Span::Placement);
    let Target {
        area,
        dpi_scale,
        cursor,
    } = target(config)?;
    let (x, y, w, h) = layout_rendered(area, size, dpi_scale, config.position, cursor);
    let (x, y) = match avoid_cards {
        Some(own) => stack(
            (x, y, w, h),
            area,
            &transient_cards(own),
            config.position,
            8,
        ),
        None => (x, y),
    };
    Ok((x, y, w, h))
}

#[cfg(windows)]
fn target(config: &crate::config::Notifications) -> Result<Target, String> {
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
        let selected = match config.monitor {
            Monitor::Index(i) => monitors.get(i).copied(),
            Monitor::Primary => None,
        };
        let monitor = selected.unwrap_or_else(|| {
            if config.position == Position::NearCursor && config.monitor == Monitor::Primary {
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
        let area = Area {
            left: a.left,
            top: a.top,
            right: a.right,
            bottom: a.bottom,
        };
        Ok(Target {
            area,
            dpi_scale: dx.max(96) as f64 / 96.0,
            cursor: (cursor.x, cursor.y),
        })
    }
}
