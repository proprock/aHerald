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

pub(crate) struct CardText<'a> {
    pub source: &'a str,
    pub label: &'a str,
    pub navigation: bool,
}
pub(crate) struct Surface {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    pub(crate) width: i32,
    pub(crate) height: i32,
    pixels: *mut u8,
    resized: std::cell::RefCell<Option<Box<Surface>>>,
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
                pixels: bits as *mut u8,
                resized: std::cell::RefCell::new(None),
            };
            if bits.is_null() || previous.is_invalid() {
                return Err("cannot select overlay bitmap".into());
            }
            Ok((surface, bits as *mut u8))
        }
    }
    pub(crate) fn with_artwork(
        &self,
        frame: &crate::animation::PreparedFrame,
        left: i32,
        top: i32,
    ) -> Result<Self, String> {
        if left < 0
            || top < 0
            || left as u32 + frame.width > self.width as u32
            || top as u32 + frame.height > self.height as u32
            || frame.bgra.len() != frame.width as usize * frame.height as usize * 4
        {
            return Err("artwork does not fit notification card".into());
        }
        let (surface, bits) = Self::allocate(self.width, self.height)?;
        // SAFETY: both DIBs own width * height * 4 bytes; checked artwork bounds keep every
        // access inside the destination. Frames and card pixels are premultiplied BGRA.
        unsafe {
            let len = self.width as usize * self.height as usize * 4;
            std::ptr::copy_nonoverlapping(self.pixels, bits, len);
            let pixels = std::slice::from_raw_parts_mut(bits, len);
            for y in 0..frame.height as usize {
                for x in 0..frame.width as usize {
                    let src = &frame.bgra[(y * frame.width as usize + x) * 4..][..4];
                    let offset = ((y + top as usize) * self.width as usize + x + left as usize) * 4;
                    for c in 0..4 {
                        pixels[offset + c] = (src[c] as u16
                            + (pixels[offset + c] as u16 * (255 - src[3] as u16) + 127) / 255)
                            .min(255) as u8;
                    }
                }
            }
        }
        Ok(surface)
    }
    fn text(
        width: i32,
        height: i32,
        scale: f64,
        source: &str,
        label: &str,
        text_left: i32,
    ) -> Result<Self, String> {
        Self::card(
            width,
            height,
            scale,
            text_left,
            CardText {
                source,
                label,
                navigation: false,
            },
        )
    }
    pub(crate) fn card(
        width: i32,
        height: i32,
        scale: f64,
        text_left: i32,
        content: CardText<'_>,
    ) -> Result<Self, String> {
        let CardText {
            source,
            label,
            navigation,
        } = content;
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
            let mut text: Vec<u16> = format!("{source}\n{label}").encode_utf16().collect();
            let mut rect = RECT {
                left: text_left,
                top: ((height - (64.0 * scale) as i32) / 2).max(0),
                right: width - (18.0 * scale) as i32,
                bottom: height,
            };
            DrawTextW(
                dc,
                &mut text,
                &mut rect,
                DT_LEFT | DT_NOPREFIX | DT_WORDBREAK,
            );
            if navigation {
                let span = (width - (18.0 * scale) as i32 - text_left) / 3;
                for (index, label) in ["Prev", "Next", "Dismiss"].iter().enumerate() {
                    let mut footer: Vec<u16> = label.encode_utf16().collect();
                    let mut area = RECT {
                        left: text_left + span * index as i32,
                        top: height - (34.0 * scale) as i32,
                        right: text_left + span * (index as i32 + 1),
                        bottom: height - (10.0 * scale) as i32,
                    };
                    DrawTextW(
                        dc,
                        &mut footer,
                        &mut area,
                        DT_CENTER | DT_NOPREFIX | DT_SINGLELINE,
                    );
                }
            }
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
    pub(crate) fn present_placed(
        &self,
        window: HWND,
        placement: (i32, i32, i32, i32),
        alpha: u8,
    ) -> Result<(i32, i32), String> {
        let (x, y, width, height) = placement;
        let point = POINT { x, y };
        if (width, height) == (self.width, self.height) {
            self.present(window, point, alpha)?;
        } else {
            let mut cached = self.resized.borrow_mut();
            if cached
                .as_ref()
                .is_none_or(|s| (s.width, s.height) != (width, height))
            {
                let (surface, bits) = Self::allocate(width, height)?;
                // SAFETY: both DIB buffers have their checked dimensions * four bytes. Copy
                // premultiplied pixels directly so scaling preserves transparent edges.
                unsafe {
                    for y in 0..height as usize {
                        for x in 0..width as usize {
                            let src = ((y * self.height as usize / height as usize)
                                * self.width as usize
                                + x * self.width as usize / width as usize)
                                * 4;
                            std::ptr::copy_nonoverlapping(
                                self.pixels.add(src),
                                bits.add((y * width as usize + x) * 4),
                                4,
                            );
                        }
                    }
                }
                *cached = Some(Box::new(surface));
            }
            if let Some(surface) = cached.as_ref() {
                surface.present(window, point, alpha)?;
            }
        }
        Ok((width, height))
    }
    pub(crate) fn present(&self, window: HWND, position: POINT, alpha: u8) -> Result<(), String> {
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
    notifications: crate::config::Notifications,
    placement: (i32, i32, i32, i32),
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
            WM_TIMER | WM_LBUTTONUP | WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE => {
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
                if !ptr.is_null() {
                    let state = &mut *ptr;
                    if matches!(message, WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE)
                        && let Ok(p) = crate::placement::current(
                            &state.notifications,
                            (state.surfaces[0].width, state.surfaces[0].height),
                            state.surfaces.len(),
                        )
                    {
                        state.placement = p;
                    }
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
                    .present_placed(hwnd, state.placement, alpha)
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
        let scale = config.notifications.scale;
        let (fallback_width, fallback_height) = ((320.0 * scale) as i32, (110.0 * scale) as i32);
        let label = event.kind.label();
        let source = event.source.label();
        let assets = config.clone();
        let asset_event = event.clone();
        let bilinear = config.notifications.interpolation == "bilinear";
        let prepared = std::thread::spawn(move || {
            crate::pet::for_event(&assets, &asset_event)?
                .map(|animation| animation.prepare(scale, bilinear))
                .transpose()
        })
        .join()
        .ok()
        .and_then(Result::ok)
        .flatten();
        let mut surfaces = Vec::new();
        let mut frame_durations = vec![1000];
        let mut looping = false;
        if let Some(animation) = prepared {
            let padding = (18.0 * scale) as i32;
            let art_width = animation.frames.iter().map(|f| f.width).max().unwrap_or(0) as i32;
            let art_height = animation.frames.iter().map(|f| f.height).max().unwrap_or(0) as i32;
            let width = fallback_width + art_width + padding;
            let height = fallback_height.max(art_height + padding * 2);
            let bytes = width as u64 * height as u64 * 4 * animation.frames.len() as u64;
            if !animation.frames.is_empty() && bytes <= crate::animation::MAX_BYTES {
                let card =
                    Surface::text(width, height, scale, source, label, art_width + padding * 2)?;
                let composed = animation
                    .frames
                    .iter()
                    .map(|frame| {
                        card.with_artwork(frame, padding, (height - frame.height as i32) / 2)
                    })
                    .collect::<Result<Vec<_>, _>>();
                if let Ok(frames) = composed {
                    surfaces = frames;
                    frame_durations = animation.durations_ms;
                    looping = animation.looping;
                }
            }
        }
        if surfaces.is_empty() {
            surfaces.push(Surface::text(
                fallback_width,
                fallback_height,
                scale,
                source,
                label,
                (22.0 * scale) as i32,
            )?);
        }
        let (width, height) = (surfaces[0].width, surfaces[0].height);
        let position = POINT::default();
        let placement =
            crate::placement::current(&config.notifications, (width, height), surfaces.len())?;
        let mut state = Box::new(State {
            surfaces,
            frame_durations,
            looping,
            notifications: config.notifications.clone(),
            placement,
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
        let title: Vec<u16> = format!("{source} - {label}")
            .encode_utf16()
            .chain(Some(0))
            .collect();
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            windows::core::PCWSTR(title.as_ptr()),
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
        state.surfaces[0].present_placed(
            hwnd,
            state.placement,
            if state.fade_in == 0 { 255 } else { 0 },
        )?;
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

#[cfg(test)]
mod card_tests {
    use super::Surface;
    use crate::animation::PreparedFrame;
    #[test]
    fn artwork_blends_inside_the_card_without_replacing_source_or_signal_text() {
        let card = Surface::text(530, 244, 1.0, "Codex", "Permission required", 228).unwrap();
        let frame = PreparedFrame {
            width: 192,
            height: 208,
            bgra: [0, 0, 128, 128].repeat(192 * 208),
        };
        let composed = card.with_artwork(&frame, 18, 18).unwrap();
        let other = Surface::text(530, 244, 1.0, "aHerald demo", "Turn complete", 228).unwrap();
        // SAFETY: each owned DIB has exactly width * height pixels and remains live here.
        unsafe {
            let base = std::slice::from_raw_parts(card.pixels, 530 * 244 * 4);
            let result = std::slice::from_raw_parts(composed.pixels, base.len());
            let different_text = std::slice::from_raw_parts(other.pixels, base.len());
            let offset = (20 * 530 + 20) * 4;
            assert_eq!(&result[offset..offset + 4], &[21, 16, 141, 249]);
            let mut text_changes = 0;
            for y in 0..244 {
                let region = (y * 530 + 228) * 4..(y * 530 + 510) * 4;
                assert_eq!(&base[region.clone()], &result[region.clone()]);
                text_changes += base[region.clone()]
                    .iter()
                    .zip(&different_text[region])
                    .filter(|(a, b)| a != b)
                    .count();
            }
            assert!(
                text_changes > 100,
                "source and signal must render visible text pixels"
            );
            assert_eq!(&result[..4], &[0, 0, 0, 0]);
        }
        assert!(card.with_artwork(&frame, 400, 18).is_err());
    }
}
