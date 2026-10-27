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
pub const CLAUDE_SUMMARY: usize = 102;
pub const RESTART: usize = 206;
pub const ABOUT: usize = 207;
pub const SOUND: usize = 201;
pub const VISUAL: usize = 202;
pub const PAUSE: usize = 203;
pub const CONFIG: usize = 204;
pub const EXIT: usize = 205;

pub struct Icon {
    data: NOTIFYICONDATAW,
    pub taskbar_created: u32,
    image: NativeIcon,
    raster: Option<(u32, Vec<u8>)>,
}
impl Icon {
    pub fn new(hwnd: HWND) -> Result<Self, String> {
        let size = icon_size(hwnd);
        let levels = crate::tray_usage::Levels::from_view(
            &crate::usage_service::View::new(false, std::time::Duration::ZERO),
            crate::config::UsageDisplay::Remain,
        );
        let pixels = levels.render(size)?;
        let image = NativeIcon::new(&pixels, size)?;
        // SAFETY: the owned icon and notification window outlive the tray registration.
        unsafe {
            let mut data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: 1,
                // Version 4 suppresses standard tooltips unless SHOWTIP is explicit.
                uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP | NIF_SHOWTIP,
                uCallbackMessage: CALLBACK,
                hIcon: image.0,
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
                image,
                raster: Some((size, pixels)),
            };
            // A missing Explorer must not disable notification delivery. Timer retries restore it.
            icon.restore();
            Ok(icon)
        }
    }
    pub fn restore(&mut self) -> bool {
        // SAFETY: NOTIFYICONDATA and the owned icon/borrowed window remain valid.
        unsafe {
            if !Shell_NotifyIconW(NIM_ADD, &self.data).as_bool() {
                return Shell_NotifyIconW(NIM_MODIFY, &self.data).as_bool();
            }
            self.data.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            Shell_NotifyIconW(NIM_SETVERSION, &self.data).as_bool()
        }
    }
    fn set_levels(&mut self, levels: crate::tray_usage::Levels, size: u32) -> Result<bool, String> {
        let pixels = levels.render(size)?;
        if self
            .raster
            .as_ref()
            .is_some_and(|(old_size, old)| *old_size == size && *old == pixels)
        {
            return Ok(false);
        }
        let image = NativeIcon::new(&pixels, size)?;
        self.data.hIcon = image.0;
        // SAFETY: both icon handles remain alive until Shell has copied the replacement.
        // If Explorer is absent, keep the current image for TaskbarCreated/timer restoration.
        unsafe {
            let _ = Shell_NotifyIconW(NIM_MODIFY, &self.data);
        }
        self.image = image;
        self.raster = Some((size, pixels));
        Ok(true)
    }
    pub fn usage(&mut self, levels: crate::tray_usage::Levels) -> Result<(), String> {
        self.set_levels(levels, icon_size(self.data.hWnd))
            .map(|_| ())
    }
    /// DPI scale (1.0 at 96 DPI) of the monitor holding the tray icon, which is where
    /// the usage card is shown; the card is drawn at this scale.
    pub fn card_dpi_scale(&self) -> f64 {
        self.anchor().map_or(1.0, |(_, _, dpi)| dpi)
    }
    /// Placement of a usage card already drawn at `card_dpi_scale`, `size` in physical pixels.
    pub fn card_placement(&self, size: (i32, i32)) -> Result<(i32, i32, i32, i32), String> {
        let (anchor, a, dpi) = self.anchor()?;
        Ok(crate::placement::layout_rendered(
            crate::placement::Area {
                left: a.left,
                top: a.top,
                right: a.right,
                bottom: a.bottom,
            },
            size,
            dpi,
            crate::config::Position::NearCursor,
            (
                anchor.right - size.0 - (16.0 * dpi) as i32,
                anchor.top - size.1 - (28.0 * dpi) as i32,
            ),
        ))
    }
    /// Tray icon rectangle, the work area of its monitor and that monitor's DPI scale.
    fn anchor(&self) -> Result<(RECT, RECT, f64), String> {
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
            Ok((anchor, info.rcWork, dx.max(96) as f64 / 96.0))
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
    /// `codex` and `claude` are the usage lines of the enabled agents; each opens its own details.
    pub fn new(
        sound: bool,
        visual: bool,
        paused: bool,
        codex: Option<&str>,
        claude: Option<&str>,
    ) -> Result<Self, String> {
        let menu = Self(unsafe { CreatePopupMenu() }.map_err(|_| "cannot create tray menu")?);
        // Alphabetical by agent name.
        let usage = [(CLAUDE_SUMMARY, claude), (SUMMARY, codex)]
            .into_iter()
            .filter_map(|(id, text)| text.map(|text| (id, text, false)));
        let version = format!("aHerald v{}", crate::VERSION);
        let items = [
            (ABOUT, version.as_str(), false),
            (SOUND, "Sound", sound),
            (VISUAL, "Pop-up", visual),
            (PAUSE, "Pause notifications", paused),
            (CONFIG, "Edit configuration", false),
            (RESTART, "Restart", false),
            (EXIT, "Exit", false),
        ];
        for (id, label, checked) in usage.chain(items) {
            if id == CONFIG || id == SOUND {
                // SAFETY: menu is owned; separators have no command or label.
                unsafe { AppendMenuW(menu.0, MF_SEPARATOR, 0, PCWSTR::null()) }
                    .map_err(|_| "cannot add tray menu separator")?;
            }
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
        let menu = Menu::new(
            false,
            true,
            true,
            Some("Codex usage (unavailable)"),
            Some("Claude remain: 5h 80%"),
        )
        .unwrap();
        unsafe {
            assert_eq!(GetMenuItemCount(Some(menu.0)), 11);
            assert_ne!(GetMenuState(menu.0, 3, MF_BYPOSITION) & MF_SEPARATOR.0, 0);
            assert_ne!(GetMenuState(menu.0, 7, MF_BYPOSITION) & MF_SEPARATOR.0, 0);
            let mut label = [0u16; 128];
            let n = GetMenuStringW(
                menu.0,
                CLAUDE_SUMMARY as u32,
                Some(&mut label),
                MF_BYCOMMAND,
            );
            assert!(String::from_utf16_lossy(&label[..n as usize]).contains("Claude remain"));
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
            let version = format!("aHerald v{}", crate::VERSION);
            for (id, expected) in [
                (SUMMARY, "unavailable"),
                (VISUAL, "Pop-up"),
                (CONFIG, "Edit configuration"),
                (RESTART, "Restart"),
                (ABOUT, version.as_str()),
            ] {
                let mut label = [0u16; 128];
                let n = GetMenuStringW(menu.0, id as u32, Some(&mut label), MF_BYCOMMAND);
                assert!(String::from_utf16_lossy(&label[..n as usize]).contains(expected));
            }
        }
    }

    #[test]
    fn usage_lines_appear_only_for_enabled_agents() {
        let menu = Menu::new(true, true, false, None, Some("Claude")).unwrap();
        unsafe {
            assert_eq!(GetMenuItemCount(Some(menu.0)), 10);
            assert_eq!(
                GetMenuState(menu.0, SUMMARY as u32, MF_BYCOMMAND),
                u32::MAX,
                "no Codex line when Codex usage is hidden"
            );
        }
        let menu = Menu::new(true, true, false, Some("Codex"), None).unwrap();
        unsafe {
            assert_eq!(GetMenuItemCount(Some(menu.0)), 10);
        }
        let menu = Menu::new(true, true, false, Some("Codex"), Some("Claude")).unwrap();
        unsafe {
            assert_eq!(GetMenuItemID(menu.0, 0), CLAUDE_SUMMARY as u32);
            assert_eq!(GetMenuItemID(menu.0, 1), SUMMARY as u32);
        }
    }

    #[test]
    fn separators_follow_usage_and_pause_without_a_leading_separator() {
        for (codex, claude, usage_count) in [
            (None, None, 0),
            (Some("Codex"), None, 1),
            (None, Some("Claude"), 1),
            (Some("Codex"), Some("Claude"), 2),
        ] {
            let menu = Menu::new(true, true, false, codex, claude).unwrap();
            unsafe {
                let count = GetMenuItemCount(Some(menu.0)) as u32;
                // The version item closes the usage block, then a separator precedes Sound.
                let about_position = usage_count;
                let sound_position = about_position + 2;
                assert_eq!(GetMenuItemID(menu.0, about_position as i32), ABOUT as u32);
                assert_eq!(GetMenuItemID(menu.0, sound_position as i32), SOUND as u32);
                let separators: Vec<_> = (0..count)
                    .filter(|&position| {
                        GetMenuState(menu.0, position, MF_BYPOSITION) & MF_SEPARATOR.0 != 0
                    })
                    .collect();
                assert_eq!(separators, vec![about_position + 1, sound_position + 3]);
            }
        }
    }
}

// Win32 owns a copy of each bitmap after CreateIconIndirect; only HICON survives.
struct Bitmap(windows::Win32::Graphics::Gdi::HBITMAP);
impl Drop for Bitmap {
    fn drop(&mut self) {
        // SAFETY: owned bitmap, never selected into a DC while this guard is dropped.
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::DeleteObject(self.0.into());
        }
    }
}
struct NativeIcon(HICON);
impl Drop for NativeIcon {
    fn drop(&mut self) {
        // SAFETY: dynamically created HICON, removed/replaced in the tray before destruction.
        unsafe {
            let _ = DestroyIcon(self.0);
        }
    }
}
impl NativeIcon {
    fn new(pixels: &[u8], size: u32) -> Result<Self, String> {
        use windows::Win32::Graphics::Gdi::*;
        if !(8..=256).contains(&size) || pixels.len() != (size * size * 4) as usize {
            return Err("invalid tray raster".into());
        }
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size as i32,
                biHeight: -(size as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: bounded raster length matches the 32-bit top-down DIB; bitmap guards own
        // both allocations. CreateIconIndirect copies them, so they may be released afterward.
        unsafe {
            let mut bits = std::ptr::null_mut();
            let color = Bitmap(
                CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)
                    .map_err(|e| format!("cannot create tray bitmap: {e}"))?,
            );
            if bits.is_null() {
                return Err("tray bitmap has no pixel buffer".into());
            }
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), bits.cast::<u8>(), pixels.len());
            let mask_bits = vec![0u8; (size.div_ceil(16) * 2 * size) as usize];
            let mask = Bitmap(CreateBitmap(
                size as i32,
                size as i32,
                1,
                1,
                Some(mask_bits.as_ptr().cast()),
            ));
            if mask.0.is_invalid() {
                return Err("cannot create tray transparency mask".into());
            }
            CreateIconIndirect(&ICONINFO {
                fIcon: true.into(),
                hbmMask: mask.0,
                hbmColor: color.0,
                ..Default::default()
            })
            .map(Self)
            .map_err(|e| format!("cannot create tray icon: {e}"))
        }
    }
}

#[cfg(test)]
#[path = "tray_native_tests.rs"]
mod native_tests;

fn icon_size(owner: HWND) -> u32 {
    use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
    // SAFETY: query borrowed Shell/owner HWNDs without changing their state. Read the
    // taskbar DPI because the non-activating overlay may be on a different monitor.
    unsafe {
        let taskbar = FindWindowW(w!("Shell_TrayWnd"), None).unwrap_or(owner);
        let dpi = GetDpiForWindow(taskbar).max(96);
        GetSystemMetricsForDpi(SM_CXSMICON, dpi).clamp(8, 256) as u32
    }
}
