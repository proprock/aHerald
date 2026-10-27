//! Native notification-area resources. Menu actions are runtime-only.
use windows::{
    Win32::{
        Foundation::*,
        UI::{Shell::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};
pub const CALLBACK: u32 = WM_APP + 9;
pub const SUMMARY: usize = 101;
pub const RESTART: usize = 206;
pub const SOUND: usize = 201;
pub const VISUAL: usize = 202;
pub const PAUSE: usize = 203;
pub const CONFIG: usize = 204;
pub const EXIT: usize = 205;

pub struct Icon {
    data: NOTIFYICONDATAW,
    pub taskbar_created: u32,
}
impl Icon {
    pub fn new(hwnd: HWND) -> Result<Self, String> {
        // SAFETY: the shared system icon needs no destruction; the window outlives this icon.
        unsafe {
            let mut data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                // Version 4 suppresses standard tooltips unless SHOWTIP is explicit.
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
                uCallbackMessage: CALLBACK,
                hIcon: LoadIconW(None, IDI_APPLICATION).map_err(|_| "cannot load tray icon")?,
                ..Default::default()
            };
            for (to, from) in data
                .szTip
                .iter_mut()
                .zip("aHerald - notifications".encode_utf16())
            {
                *to = from;
            }
            let taskbar_created = RegisterWindowMessageW(w!("TaskbarCreated"));
            if taskbar_created == 0 {
                return Err("cannot register taskbar notification".into());
            }
            let mut icon = Self {
                data,
                taskbar_created,
            };
            // A missing Explorer must not disable notification delivery. Timer retries restore it.
            icon.restore();
            Ok(icon)
        }
    }
    pub fn restore(&mut self) -> bool {
        // SAFETY: NOTIFYICONDATA and the shared icon/window handles remain valid.
        unsafe {
            if !Shell_NotifyIconW(NIM_ADD, &self.data).as_bool() {
                return Shell_NotifyIconW(NIM_MODIFY, &self.data).as_bool();
            }
            self.data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            Shell_NotifyIconW(NIM_SETVERSION, &self.data).as_bool()
        }
    }
    pub fn card_placement(&self, size: (i32, i32)) -> Result<(i32, i32, i32, i32), String> {
        use windows::Win32::{Graphics::Gdi::*, UI::HiDpi::*};
        // SAFETY: borrowed tray identity and monitor handles are used only during this query.
        unsafe {
            let identity = NOTIFYICONIDENTIFIER {
                cbSize: std::mem::size_of::<NOTIFYICONIDENTIFIER>() as u32,
                hWnd: self.data.hWnd,
                uID: self.data.uID,
                ..Default::default()
            };
            let anchor = Shell_NotifyIconGetRect(&identity).unwrap_or_else(|_| {
                let mut p = POINT::default();
                let _ = GetCursorPos(&mut p);
                RECT {
                    left: p.x,
                    top: p.y,
                    right: p.x,
                    bottom: p.y,
                }
            });
            let monitor = MonitorFromRect(&anchor, MONITOR_DEFAULTTONEAREST);
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            if !GetMonitorInfoW(monitor, &mut info).as_bool() {
                return Err("cannot locate tray work area".into());
            }
            let (mut dx, mut dy) = (96, 96);
            let _ = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dx, &mut dy);
            let dpi = dx.max(96) as f64 / 96.0;
            let a = info.rcWork;
            Ok(crate::placement::layout(
                crate::placement::Area {
                    left: a.left,
                    top: a.top,
                    right: a.right,
                    bottom: a.bottom,
                },
                size,
                dpi,
                "near_cursor",
                (
                    anchor.right - (size.0 as f64 * dpi) as i32 - (16.0 * dpi) as i32,
                    anchor.top - (size.1 as f64 * dpi) as i32 - (28.0 * dpi) as i32,
                ),
            ))
        }
    }
    pub fn focus(&self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_SETFOCUS, &self.data);
        }
    }
    pub fn tooltip(&mut self, text: &str) {
        self.data.szTip.fill(0);
        let mut position = 0;
        for c in text.chars() {
            let mut units = [0; 2];
            let units = c.encode_utf16(&mut units);
            if position + units.len() >= self.data.szTip.len() {
                break;
            }
            self.data.szTip[position..position + units.len()].copy_from_slice(units);
            position += units.len();
        }
        // SAFETY: owned NOTIFYICONDATA remains valid, including its terminated tooltip.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &self.data);
        }
    }
}
impl Drop for Icon {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.data);
        }
    }
}
pub struct Menu(HMENU);
impl Drop for Menu {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyMenu(self.0);
        }
    }
}
impl Menu {
    pub fn new(sound: bool, visual: bool, paused: bool, summary: &str) -> Result<Self, String> {
        let menu = Self(unsafe { CreatePopupMenu() }.map_err(|_| "cannot create tray menu")?);
        for (id, label, checked) in [
            (SUMMARY, summary, false),
            (SOUND, "Sound", sound),
            (VISUAL, "Visual notifications", visual),
            (PAUSE, "Pause notifications", paused),
            (CONFIG, "Edit configuration", false),
            (RESTART, "Restart", false),
            (EXIT, "Exit", false),
        ] {
            let wide: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
            unsafe {
                AppendMenuW(
                    menu.0,
                    MF_STRING | if checked { MF_CHECKED } else { MF_UNCHECKED },
                    id,
                    PCWSTR(wide.as_ptr()),
                )
            }
            .map_err(|_| "cannot populate tray menu")?;
        }
        Ok(menu)
    }
    pub fn select(&self, hwnd: HWND) -> usize {
        // SAFETY: native menu is owned for the complete synchronous popup interaction.
        unsafe {
            let mut point = POINT::default();
            let _ = GetCursorPos(&mut point);
            let _ = SetForegroundWindow(hwnd);
            let command = TrackPopupMenu(
                self.0,
                TPM_RETURNCMD | TPM_NONOTIFY | TPM_RIGHTBUTTON,
                point.x,
                point.y,
                None,
                hwnd,
                None,
            );
            let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
            command.0 as usize
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_menu_exposes_all_actions_and_current_checkmarks() {
        let menu = Menu::new(false, true, true, "Codex usage (unavailable)").unwrap();
        unsafe {
            assert_eq!(GetMenuItemCount(Some(menu.0)), 7);
            assert_eq!(
                GetMenuState(menu.0, SOUND as u32, MF_BYCOMMAND) & MF_CHECKED.0,
                0
            );
            assert_ne!(
                GetMenuState(menu.0, VISUAL as u32, MF_BYCOMMAND) & MF_CHECKED.0,
                0
            );
            assert_ne!(
                GetMenuState(menu.0, PAUSE as u32, MF_BYCOMMAND) & MF_CHECKED.0,
                0
            );
            for (id, expected) in [
                (SUMMARY, "unavailable"),
                (CONFIG, "Edit configuration"),
                (RESTART, "Restart"),
            ] {
                let mut label = [0u16; 128];
                let n = GetMenuStringW(menu.0, id as u32, Some(&mut label), MF_BYCOMMAND);
                assert!(String::from_utf16_lossy(&label[..n as usize]).contains(expected));
            }
        }
    }
}
