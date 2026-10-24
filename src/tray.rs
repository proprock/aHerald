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
            (DETAILS, "Usage details", false),
            (REFRESH, "Refresh usage", false),
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
    if msg == WM_SIZE {
        // SAFETY: the child is owned by this window; bounds are clamped for minimization.
        unsafe {
            if let Ok(child) = GetDlgItem(Some(hwnd), 1) {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let _ = MoveWindow(
                    child,
                    12,
                    12,
                    (rect.right - 24).max(0),
                    (rect.bottom - 24).max(0),
                    true,
                );
            }
        }
    }
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
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            760,
            520,
            Some(owner),
            None,
            Some(module.into()),
            None,
        )
        .map_err(|_| "cannot create information window")?;
        let editor = match CreateWindowExW(
            WS_EX_CLIENTEDGE,
            w!("EDIT"),
            PCWSTR(text.as_ptr()),
            WS_CHILD
                | WS_VISIBLE
                | WS_VSCROLL
                | WS_TABSTOP
                | WINDOW_STYLE((ES_MULTILINE | ES_READONLY | ES_AUTOVSCROLL) as u32),
            12,
            12,
            720,
            450,
            Some(window),
            // For a child window this slot encodes an integer control ID, never a pointer.
            Some(HMENU(std::ptr::without_provenance_mut(1))),
            Some(module.into()),
            None,
        ) {
            Ok(editor) => editor,
            Err(_) => {
                let _ = DestroyWindow(window);
                return Err("cannot create information text".into());
            }
        };
        // The shared system GUI font is borrowed and must not be deleted.
        let font = windows::Win32::Graphics::Gdi::GetStockObject(
            windows::Win32::Graphics::Gdi::DEFAULT_GUI_FONT,
        );
        SendMessageW(
            editor,
            WM_SETFONT,
            Some(WPARAM(font.0 as usize)),
            Some(LPARAM(1)),
        );
        let _ = ShowWindow(window, SW_SHOWNORMAL);
        Ok(window)
    }
}

pub fn update_information(window: HWND, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    // SAFETY: IsWindow/GetDlgItem validate the owned modeless window before updating it.
    unsafe {
        if IsWindow(Some(window)).as_bool()
            && let Ok(child) = GetDlgItem(Some(window), 1)
        {
            let position = GetScrollPos(child, SB_VERT).clamp(0, u16::MAX as i32);
            let _ = SetWindowTextW(child, PCWSTR(wide.as_ptr()));
            // Replacing an edit control's text resets its viewport. Restore the
            // thumb position so background usage updates do not interrupt reading.
            SendMessageW(
                child,
                WM_VSCROLL,
                Some(WPARAM(
                    (position as usize) << 16 | SB_THUMBPOSITION.0 as usize,
                )),
                Some(LPARAM(0)),
            );
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
            for (id, expected) in [
                (SUMMARY, "unavailable"),
                (DETAILS, "Usage details"),
                (REFRESH, "Refresh usage"),
            ] {
                let mut label = [0u16; 128];
                let n = GetMenuStringW(menu.0, id as u32, Some(&mut label), MF_BYCOMMAND);
                assert!(String::from_utf16_lossy(&label[..n as usize]).contains(expected));
            }
        }
    }
}
