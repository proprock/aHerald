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
pub const DETAILS: usize = 102;
pub const REFRESH: usize = 103;
pub const SOUND: usize = 201;
pub const VISUAL: usize = 202;
pub const PAUSE: usize = 203;
pub const CONFIG: usize = 204;
pub const EXIT: usize = 205;
pub const UNAVAILABLE: &str = "Usage unavailable.\r\n\r\nThe official usage connection is planned for Milestone 4.\r\nNotifications continue to work independently.";

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
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
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
    pub fn focus(&self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_SETFOCUS, &self.data);
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
    pub fn new(sound: bool, visual: bool, paused: bool) -> Result<Self, String> {
        let menu = Self(unsafe { CreatePopupMenu() }.map_err(|_| "cannot create tray menu")?);
        for (id, label, checked) in [
            (SUMMARY, "Usage summary (unavailable)", false),
            (DETAILS, "Usage details (unavailable)", false),
            (REFRESH, "Refresh usage (unavailable)", false),
            (SOUND, "Sound", sound),
            (VISUAL, "Visual notifications", visual),
            (PAUSE, "Pause notifications", paused),
            (CONFIG, "Configuration location", false),
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

unsafe extern "system" fn information_proc(
    hwnd: HWND,
    msg: u32,
    wp: WPARAM,
    lp: LPARAM,
) -> LRESULT {
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}
pub fn information(owner: HWND, title: &str, text: &str) -> Result<HWND, String> {
    // A modeless owned window keeps Quit responsive while details are open.
    unsafe {
        let module = windows::Win32::System::LibraryLoader::GetModuleHandleW(None)
            .map_err(|_| "cannot find module")?;
        let class = w!("aHerald.Information");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(information_proc),
            hInstance: module.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).map_err(|_| "cannot load cursor")?,
            ..Default::default()
        };
        let _ = RegisterClassW(&wc);
        let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();
        let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        let window = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class,
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            600,
            210,
            Some(owner),
            None,
            Some(module.into()),
            None,
        )
        .map_err(|_| "cannot create information window")?;
        if CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            PCWSTR(text.as_ptr()),
            WS_CHILD | WS_VISIBLE,
            18,
            18,
            550,
            150,
            Some(window),
            None,
            Some(module.into()),
            None,
        )
        .is_err()
        {
            let _ = DestroyWindow(window);
            return Err("cannot create information text".into());
        }
        let _ = ShowWindow(window, SW_SHOWNORMAL);
        Ok(window)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_menu_exposes_all_actions_and_current_checkmarks() {
        let menu = Menu::new(false, true, true).unwrap();
        unsafe {
            assert_eq!(GetMenuItemCount(Some(menu.0)), 8);
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
            for id in [SUMMARY, DETAILS, REFRESH] {
                let mut label = [0u16; 128];
                let n = GetMenuStringW(menu.0, id as u32, Some(&mut label), MF_BYCOMMAND);
                assert!(String::from_utf16_lossy(&label[..n as usize]).contains("unavailable"));
            }
        }
    }
}
