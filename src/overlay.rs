use crate::{config::Config, events::Event};
use std::{mem::size_of, time::Instant};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::w,
};

struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    width: i32,
    height: i32,
}
impl Drop for Surface {
    fn drop(&mut self) {
        // SAFETY: all handles are owned, the old selection is restored before deleting the bitmap.
        unsafe {
            SelectObject(self.dc, self.previous);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}
struct Font(HFONT);
impl Drop for Font {
    fn drop(&mut self) {
        unsafe {
            let _ = DeleteObject(self.0.into());
        }
    }
}
impl Surface {
    fn allocate(width: i32, height: i32) -> Result<(Self, *mut u8), String> {
        // SAFETY: dimensions come from bounded frame preparation or bounded fallback sizing.
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.is_invalid() {
                return Err("cannot create overlay DC".into());
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let mut bits = std::ptr::null_mut();
            let bitmap = match CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0)
            {
                Ok(bitmap) => bitmap,
                Err(_) => {
                    let _ = DeleteDC(dc);
                    return Err("cannot allocate overlay bitmap".into());
                }
            };
            let previous = SelectObject(dc, bitmap.into());
            let surface = Self {
                dc,
                bitmap,
                previous,
                width,
                height,
            };
            if bits.is_null() || previous.is_invalid() {
                return Err("cannot select overlay bitmap".into());
            }
            Ok((surface, bits as *mut u8))
        }
    }
    fn image(frame: crate::animation::PreparedFrame) -> Result<Self, String> {
        let (surface, bits) = Self::allocate(frame.width as i32, frame.height as i32)?;
        // SAFETY: preparation produces exactly width * height * 4 bytes and the DIB owns that allocation.
        unsafe {
            std::ptr::copy_nonoverlapping(frame.bgra.as_ptr(), bits, frame.bgra.len());
        }
        Ok(surface)
    }
    fn text(width: i32, height: i32, scale: f64, label: &str) -> Result<Self, String> {
        // SAFETY: the DIB is top-down BGRA and sized for all pixel accesses. The memory DC and
        // bitmap remain owned until Surface drops, and GDI selections are restored before cleanup.
        unsafe {
            let (surface, bits) = Self::allocate(width, height)?;
            let dc = surface.dc;
            let pixels = std::slice::from_raw_parts_mut(bits, (width * height * 4) as usize);
            let radius = (18.0 * scale) as i32;
            for y in 0..height {
                for x in 0..width {
                    let cx = x.clamp(radius, width - radius - 1);
                    let cy = y.clamp(radius, height - radius - 1);
                    let inside = (x - cx).pow(2) + (y - cy).pow(2) <= radius.pow(2);
                    let i = ((y * width + x) * 4) as usize;
                    pixels[i..i + 4].copy_from_slice(if inside {
                        &[45, 35, 28, 242]
                    } else {
                        &[0, 0, 0, 0]
                    });
                    if inside && x < (5.0 * scale) as i32 {
                        pixels[i..i + 3].copy_from_slice(&[178, 220, 101]);
                    }
                }
            }
            let font = Font(CreateFontW(
                -(18.0 * scale) as i32,
                0,
                0,
                0,
                500,
                0,
                0,
                0,
                DEFAULT_CHARSET,
                OUT_DEFAULT_PRECIS,
                CLIP_DEFAULT_PRECIS,
                ANTIALIASED_QUALITY,
                DEFAULT_PITCH.0 as u32,
                w!("Segoe UI"),
            ));
            if font.0.is_invalid() {
                return Err("cannot create overlay font".into());
            }
            let old_font = SelectObject(dc, font.0.into());
            SetBkMode(dc, TRANSPARENT);
            SetTextColor(dc, COLORREF(0x00f5f5f5));
            let mut text: Vec<u16> = format!("aHerald\n{label}").encode_utf16().collect();
            let mut rect = RECT {
                left: (22.0 * scale) as i32,
                top: (18.0 * scale) as i32,
                right: width - (18.0 * scale) as i32,
                bottom: height,
            };
            DrawTextW(
                dc,
                &mut text,
                &mut rect,
                DT_LEFT | DT_NOPREFIX | DT_WORDBREAK,
            );
            SelectObject(dc, old_font);
            let _ = GdiFlush();
            // GDI text writes RGB, but can clear alpha. Restore coverage, then premultiply.
            for pixel in pixels.as_chunks_mut::<4>().0 {
                if pixel[3] == 0 && pixel[..3].iter().any(|c| *c != 0) {
                    pixel[3] = 255;
                }
                let alpha = pixel[3] as u16;
                for channel in &mut pixel[..3] {
                    *channel = ((*channel as u16 * alpha + 127) / 255) as u8;
                }
            }
            Ok(surface)
        }
    }
    fn present(&self, window: HWND, position: POINT, alpha: u8) -> Result<(), String> {
        let size = SIZE {
            cx: self.width,
            cy: self.height,
        };
        let origin = POINT::default();
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: alpha,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: the selected 32-bit premultiplied DIB and all argument pointers are live for this call.
        unsafe {
            UpdateLayeredWindow(
                window,
                None,
                Some(&position),
                Some(&size),
                Some(self.dc),
                Some(&origin),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        }
        .map_err(|_| "cannot update layered overlay".into())
    }
}
struct State {
    surfaces: Vec<Surface>,
    frame_durations: Vec<u64>,
    looping: bool,
    position: POINT,
    start: Instant,
    duration: u64,
    fade_in: u64,
    fade_out: u64,
    click: bool,
    dismissed: Option<u64>,
    failed: bool,
}
struct Window(HWND);
impl Drop for Window {
    fn drop(&mut self) {
        unsafe {
            if IsWindow(Some(self.0)).as_bool() {
                let _ = DestroyWindow(self.0);
            }
        }
    }
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: State is boxed by show(), lives until the window is destroyed, and is accessed only
    // on this GUI thread. Destruction messages do not borrow it, preventing reentrant aliasing.
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lp.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        match message {
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_ERASEBKGND => return LRESULT(1),
            WM_DESTROY => {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_TIMER | WM_LBUTTONUP => {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
                if !ptr.is_null() {
                    let state = &mut *ptr;
                    let elapsed = state.start.elapsed().as_millis().min(u64::MAX as u128) as u64;
                    if message == WM_LBUTTONUP && state.click && state.dismissed.is_none() {
                        state.dismissed = Some(elapsed);
                    }
                    let (alpha, done) = super::presentation(
                        elapsed,
                        state.duration,
                        state.fade_in,
                        state.fade_out,
                        state.dismissed,
                    );
                    if done {
                        let _ = DestroyWindow(hwnd);
                    } else if state.surfaces[crate::animation::frame_index(
                        &state.frame_durations,
                        state.looping,
                        elapsed,
                    )]
                    .present(hwnd, state.position, alpha)
                    .is_err()
                    {
                        state.failed = true;
                        let _ = DestroyWindow(hwnd);
                    }
                    return LRESULT(0);
                }
            }
            _ => {}
        }
        DefWindowProcW(hwnd, message, wp, lp)
    }
}

pub fn show(event: &Event, config: &Config) -> Result<(), String> {
    // SAFETY: all Win32 objects are created/used/destroyed on this thread; boxed State outlives Window.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let scale = config.notifications.scale * GetDpiForSystem().max(96) as f64 / 96.0;
        let (fallback_width, fallback_height) = ((320.0 * scale) as i32, (110.0 * scale) as i32);
        let label = match event.kind {
            crate::events::Kind::TurnComplete => "Turn complete",
            crate::events::Kind::PermissionRequired => "Permission required",
            crate::events::Kind::InputRequired => "Input required",
            crate::events::Kind::Error => "Agent error",
            crate::events::Kind::Interrupted => "Turn interrupted",
        };
        let path = config.events[event.kind.name()].animation.clone();
        let bilinear = config.notifications.interpolation == "bilinear";
        let prepared = path.and_then(|path| {
            std::thread::spawn(move || {
                crate::animation::Animation::load(&path)?.prepare(scale, bilinear)
            })
            .join()
            .ok()
            .and_then(Result::ok)
        });
        let (surfaces, frame_durations, looping) = if let Some(animation) = prepared {
            let surfaces = animation
                .frames
                .into_iter()
                .map(Surface::image)
                .collect::<Result<Vec<_>, _>>();
            match surfaces {
                Ok(frames) if !frames.is_empty() => {
                    (frames, animation.durations_ms, animation.looping)
                }
                _ => (
                    vec![Surface::text(
                        fallback_width,
                        fallback_height,
                        scale,
                        label,
                    )?],
                    vec![1000],
                    false,
                ),
            }
        } else {
            (
                vec![Surface::text(
                    fallback_width,
                    fallback_height,
                    scale,
                    label,
                )?],
                vec![1000],
                false,
            )
        };
        let (width, height) = (surfaces[0].width, surfaces[0].height);
        let mut cursor = POINT::default();
        let _ = GetCursorPos(&mut cursor);
        let point = if config.notifications.position == "near_cursor" {
            cursor
        } else {
            POINT::default()
        };
        let monitor = MonitorFromPoint(point, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return Err("cannot determine monitor work area".into());
        }
        let area = info.rcWork;
        let (x, y) = match config.notifications.position.as_str() {
            "top_center" => ((area.left + area.right - width) / 2, area.top + 24),
            "bottom_right" => (area.right - width - 24, area.bottom - height - 24),
            "near_cursor" => (cursor.x + 16, cursor.y + 16),
            _ => (
                (area.left + area.right - width) / 2,
                (area.top + area.bottom - height) / 2,
            ),
        };
        let position = POINT {
            x: x.clamp(area.left, (area.right - width).max(area.left)),
            y: y.clamp(area.top, (area.bottom - height).max(area.top)),
        };
        let mut state = Box::new(State {
            surfaces,
            frame_durations,
            looping,
            position,
            start: Instant::now(),
            duration: config.events[event.kind.name()].duration_ms,
            fade_in: config.notifications.fade_in_ms,
            fade_out: config.notifications.fade_out_ms,
            click: config.notifications.click_to_dismiss,
            dismissed: None,
            failed: false,
        });
        let instance = GetModuleHandleW(None).map_err(|_| "cannot find module handle")?;
        let class = w!("aHerald.Overlay");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).map_err(|_| "cannot load cursor")?,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err("cannot register overlay class".into());
        }
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("aHerald notification"),
            WS_POPUP,
            position.x,
            position.y,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            Some(&mut *state as *mut State as *const std::ffi::c_void),
        )
        .map_err(|_| "cannot create overlay window")?;
        let window = Window(hwnd);
        state.surfaces[0].present(hwnd, position, if state.fade_in == 0 { 255 } else { 0 })?;
        if SetTimer(Some(hwnd), 1, 16, None) == 0 {
            return Err("cannot start overlay timer".into());
        }
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

        let mut message = MSG::default();
        loop {
            let status = GetMessageW(&mut message, None, 0, 0).0;
            if status == -1 {
                return Err("overlay message loop failed".into());
            }
            if status == 0 {
                break;
            }
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
        drop(window);
        if state.failed {
            Err("overlay rendering failed".into())
        } else {
            Ok(())
        }
    }
}
