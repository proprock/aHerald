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
    pub dismiss_button: bool,
    pub color: crate::config::Color,
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
    #[cfg(test)]
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
                dismiss_button: false,
                color: crate::config::Color([101, 220, 178]),
            },
        )
    }
    pub(crate) fn body_height(width: i32, scale: f64, label: &str) -> Result<i32, String> {
        let (surface, _) = Self::allocate(1, 1)?;
        // SAFETY: the temporary DC and selected font remain alive through measurement.
        unsafe {
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
                return Err("cannot create measurement font".into());
            }
            let old = SelectObject(surface.dc, font.0.into());
            let mut text: Vec<u16> = label.encode_utf16().collect();
            let mut rect = RECT {
                left: 0,
                top: 0,
                right: width,
                bottom: 0,
            };
            DrawTextW(
                surface.dc,
                &mut text,
                &mut rect,
                DT_LEFT | DT_NOPREFIX | DT_WORDBREAK | DT_CALCRECT,
            );
            SelectObject(surface.dc, old);
            Ok(rect.bottom.max((18.0 * scale) as i32))
        }
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
            dismiss_button,
            color,
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
                        pixels[i..i + 3].copy_from_slice(&[color.0[2], color.0[1], color.0[0]]);
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
            let mut text: Vec<u16> = source.encode_utf16().collect();
            let mut rect = RECT {
                left: text_left,
                top: if label.contains('\n') {
                    (18.0 * scale) as i32
                } else {
                    ((height - (64.0 * scale) as i32) / 2).max(0)
                },
                right: width - (82.0 * scale) as i32,
                bottom: height,
            };
            if !text.is_empty() {
                DrawTextW(
                    dc,
                    &mut text,
                    &mut rect,
                    DT_LEFT | DT_NOPREFIX | DT_SINGLELINE | DT_END_ELLIPSIS,
                );
            }
            rect.top += (26.0 * scale) as i32;
            rect.right = width - (18.0 * scale) as i32;
            rect.bottom = height
                - (if navigation || dismiss_button {
                    36.0
                } else {
                    10.0
                } * scale) as i32;
            let mut text: Vec<u16> = label.encode_utf16().collect();
            if !text.is_empty() {
                DrawTextW(
                    dc,
                    &mut text,
                    &mut rect,
                    DT_LEFT | DT_NOPREFIX | DT_WORDBREAK,
                );
            }
            let controls = crate::card::Card {
                heading: source.into(),
                navigation,
                dismiss_button,
            };
            for (action, r) in controls.buttons(width, height, scale) {
                let label = match action {
                    crate::card::Action::Previous => "<",
                    crate::card::Action::Next => ">",
                    crate::card::Action::Dismiss => "Dismiss",
                };
                let mut footer: Vec<u16> = label.encode_utf16().collect();
                let mut area = RECT {
                    left: r.left,
                    top: r.top,
                    right: r.right,
                    bottom: r.bottom,
                };
                DrawTextW(
                    dc,
                    &mut footer,
                    &mut area,
                    DT_CENTER | DT_NOPREFIX | DT_SINGLELINE,
                );
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
    pub(crate) fn with_heading(
        &self,
        source: &str,
        text_left: i32,
        scale: f64,
    ) -> Result<Self, String> {
        let heading = Self::card(
            self.width,
            self.height,
            scale,
            text_left,
            CardText {
                source,
                label: "",
                navigation: false,
                dismiss_button: false,
                color: crate::config::Color([0, 0, 0]),
            },
        )?;
        let (surface, bits) = Self::allocate(self.width, self.height)?;
        // SAFETY: all three owned DIBs have identical dimensions. Copy only the
        // heading's bounded rectangle, preserving artwork, controls and event text.
        unsafe {
            let len = self.width as usize * self.height as usize * 4;
            std::ptr::copy_nonoverlapping(self.pixels, bits, len);
            let top = ((self.height - (64.0 * scale) as i32) / 2).max(0);
            let right = (self.width - (82.0 * scale) as i32).clamp(0, self.width);
            for y in top..(top + (26.0 * scale) as i32).min(self.height) {
                let left = text_left.clamp(0, right);
                let offset = ((y * self.width + left) * 4) as usize;
                std::ptr::copy_nonoverlapping(
                    heading.pixels.add(offset),
                    bits.add(offset),
                    ((right - left) * 4) as usize,
                );
            }
        }
        Ok(surface)
    }
    pub(crate) fn with_countdown(&self, seconds: u64, scale: f64) -> Result<Self, String> {
        let (surface, bits) = Self::allocate(self.width, self.height)?;
        // SAFETY: both DIBs own exactly width * height * four bytes; only this owned DC
        // is drawn to, and the font selection is restored before its RAII destruction.
        unsafe {
            let len = self.width as usize * self.height as usize * 4;
            std::ptr::copy_nonoverlapping(self.pixels, bits, len);
            let font = Font(CreateFontW(
                -(16.0 * scale) as i32,
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
                return Err("cannot create countdown font".into());
            }
            let old = SelectObject(surface.dc, font.0.into());
            SetBkMode(surface.dc, TRANSPARENT);
            SetTextColor(surface.dc, COLORREF(0x00f5f5f5));
            let mut text: Vec<u16> = format!("{seconds}s").encode_utf16().collect();
            let mut area = RECT {
                left: self.width - (80.0 * scale) as i32,
                top: (16.0 * scale) as i32,
                right: self.width - (18.0 * scale) as i32,
                bottom: (40.0 * scale) as i32,
            };
            DrawTextW(
                surface.dc,
                &mut text,
                &mut area,
                DT_RIGHT | DT_NOPREFIX | DT_SINGLELINE,
            );
            SelectObject(surface.dc, old);
            let _ = GdiFlush();
            for pixel in std::slice::from_raw_parts_mut(bits, len)
                .as_chunks_mut::<4>()
                .0
            {
                if pixel[3] == 0 && pixel[..3].iter().any(|c| *c != 0) {
                    pixel[3] = 255;
                }
            }
        }
        Ok(surface)
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
    controls: crate::card::Card,
    countdown: Option<(usize, u64, Surface)>,
    dismissed: Option<u64>,
    failed: bool,
    source_labels: crate::session_source::Service,
    event: Event,
    text_left: i32,
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
                    let source = state.source_labels.label(&state.event);
                    if source != state.controls.heading {
                        let updated = state
                            .surfaces
                            .iter()
                            .map(|surface| {
                                surface.with_heading(
                                    &source,
                                    state.text_left,
                                    state.notifications.scale,
                                )
                            })
                            .collect::<Result<Vec<_>, _>>();
                        match updated {
                            Ok(surfaces) => {
                                state.surfaces = surfaces;
                                state.controls.heading = source.clone();
                                state.countdown = None;
                                let title: Vec<u16> =
                                    format!("{source} - {}", state.event.kind.label())
                                        .encode_utf16()
                                        .chain(Some(0))
                                        .collect();
                                let _ = SetWindowTextW(hwnd, windows::core::PCWSTR(title.as_ptr()));
                            }
                            Err(_) => {
                                state.failed = true;
                                let _ = DestroyWindow(hwnd);
                                return LRESULT(0);
                            }
                        }
                    }
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
                    let x = (lp.0 as u16) as i16 as i32 * state.surfaces[0].width
                        / state.placement.2.max(1);
                    let y = ((lp.0 >> 16) as u16) as i16 as i32 * state.surfaces[0].height
                        / state.placement.3.max(1);
                    if message == WM_LBUTTONUP
                        && state.dismissed.is_none()
                        && state.controls.hit_test(
                            state.surfaces[0].width,
                            state.surfaces[0].height,
                            state.notifications.scale,
                            x,
                            y,
                        ) == Some(crate::card::Action::Dismiss)
                    {
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
                    } else {
                        let frame = crate::animation::frame_index(
                            &state.frame_durations,
                            state.looping,
                            elapsed,
                        );
                        let result = (|| {
                            let surface = if let Some(seconds) =
                                crate::card::remaining_seconds(state.duration, elapsed)
                            {
                                if state
                                    .countdown
                                    .as_ref()
                                    .is_none_or(|(f, s, _)| *f != frame || *s != seconds)
                                {
                                    state.countdown = Some((
                                        frame,
                                        seconds,
                                        state.surfaces[frame]
                                            .with_countdown(seconds, state.notifications.scale)?,
                                    ));
                                }
                                &state
                                    .countdown
                                    .as_ref()
                                    .ok_or("missing countdown surface")?
                                    .2
                            } else {
                                &state.surfaces[frame]
                            };
                            surface.present_placed(hwnd, state.placement, alpha)
                        })();
                        if result.is_err() {
                            state.failed = true;
                            let _ = DestroyWindow(hwnd);
                        }
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
    show_with_source(event, config, crate::session_source::Service::codex())
}
fn show_with_source(
    event: &Event,
    config: &Config,
    mut source_labels: crate::session_source::Service,
) -> Result<(), String> {
    // SAFETY: all Win32 objects are created/used/destroyed on this thread; boxed State outlives Window.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let scale = config.notifications.scale;
        let (fallback_width, fallback_height) = ((320.0 * scale) as i32, (110.0 * scale) as i32);
        let label = event.kind.label();
        let source = source_labels.label(event);
        let controls = crate::card::Card::new(&source, 0, 1, config.notifications.click_to_dismiss);
        let make_card = |width, height, text_left| {
            Surface::card(
                width,
                height,
                scale,
                text_left,
                CardText {
                    source: &source,
                    label,
                    navigation: false,
                    dismiss_button: controls.dismiss_button,
                    color: config.event_color(event.kind),
                },
            )
        };
        let assets = config.clone();
        let asset_event = event.clone();
        let prepared =
            std::thread::spawn(move || crate::artwork::prepared_for_event(&assets, &asset_event))
                .join()
                .ok()
                .and_then(Result::ok)
                .flatten();
        let mut surfaces = Vec::new();
        let mut frame_durations = vec![1000];
        let mut looping = false;
        let mut text_left = (22.0 * scale) as i32;
        if let Some(animation) = prepared {
            let padding = (18.0 * scale) as i32;
            let art_width = animation.frames.iter().map(|f| f.width).max().unwrap_or(0) as i32;
            let art_height = animation.frames.iter().map(|f| f.height).max().unwrap_or(0) as i32;
            let width = fallback_width + art_width + padding;
            let height = fallback_height.max(art_height + padding * 2);
            let bytes = width as u64 * height as u64 * 4 * animation.frames.len() as u64;
            if !animation.frames.is_empty() && bytes <= crate::animation::MAX_BYTES {
                let card = make_card(width, height, art_width + padding * 2)?;
                let composed = animation
                    .frames
                    .iter()
                    .map(|frame| {
                        card.with_artwork(frame, padding, (height - frame.height as i32) / 2)
                    })
                    .collect::<Result<Vec<_>, _>>();
                if let Ok(frames) = composed {
                    text_left = art_width + padding * 2;
                    surfaces = frames;
                    frame_durations = animation.durations_ms;
                    looping = animation.looping;
                }
            }
        }
        if surfaces.is_empty() {
            surfaces.push(make_card(
                fallback_width,
                fallback_height,
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
            controls,
            countdown: None,
            dismissed: None,
            failed: false,
            source_labels,
            event: event.clone(),
            text_left,
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
    #[test]
    fn metadata_heading_preserves_artwork_signal_and_controls() {
        let card = Surface::text(
            530,
            244,
            1.0,
            "Codex: session 12345678",
            "Permission required",
            228,
        )
        .unwrap();
        let updated = card
            .with_heading("Codex: Проверка aHerald", 228, 1.0)
            .unwrap();
        // SAFETY: both owned DIBs contain width * height BGRA pixels.
        unsafe {
            let old = std::slice::from_raw_parts(card.pixels, 530 * 244 * 4);
            let new = std::slice::from_raw_parts(updated.pixels, old.len());
            let mut changed = 0;
            for y in 0..244 {
                for x in 0..530 {
                    let i = (y * 530 + x) * 4;
                    if (90..116).contains(&y) && (228..448).contains(&x) {
                        changed += usize::from(old[i..i + 4] != new[i..i + 4]);
                    } else {
                        assert_eq!(old[i..i + 4], new[i..i + 4]);
                    }
                }
            }
            assert!(changed > 100);
        }
    }

    #[test]
    fn background_title_reaches_native_window() {
        source_window(false);
    }
    #[test]
    #[ignore = "one native visual inspection of the resolved session heading"]
    fn preview_session_heading() {
        source_window(true);
    }
    fn source_window(preview: bool) {
        use super::*;
        use std::{sync::mpsc, time::Duration};
        use windows::core::BOOL;
        let (release, gate) = mpsc::channel();
        let mut event = Event::demo("permission_required").unwrap();
        event.source = crate::events::Source::CodexHook;
        event.session_id = Some("019a0000-0000-0000-0000-000012345678".into());
        let mut config = Config::default();
        config.sounds.enabled = false;
        let job = std::thread::spawn(move || {
            show_with_source(
                &event,
                &config,
                crate::session_source::Service::new(move |_| {
                    gate.recv().ok()?;
                    Some("Проверка aHerald".into())
                }),
            )
        });
        unsafe extern "system" fn find(hwnd: HWND, param: LPARAM) -> BOOL {
            unsafe {
                let mut pid = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                let mut class = [0u16; 80];
                let len = GetClassNameW(hwnd, &mut class);
                if pid == std::process::id()
                    && String::from_utf16_lossy(&class[..len as usize]) == "aHerald.Overlay"
                {
                    *(param.0 as *mut HWND) = hwnd;
                    return BOOL(0);
                }
                BOOL(1)
            }
        }
        let mut hwnd = HWND::default();
        let deadline = Instant::now() + Duration::from_secs(5);
        while hwnd.is_invalid() {
            unsafe {
                let _ = EnumWindows(Some(find), LPARAM(&mut hwnd as *mut HWND as isize));
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        struct Close(HWND);
        impl Drop for Close {
            fn drop(&mut self) {
                unsafe {
                    let _ = PostMessageW(Some(self.0), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
        }
        let close = Close(hwnd);
        let title = || {
            let mut text = [0u16; 256];
            let len = unsafe { GetWindowTextW(hwnd, &mut text) };
            String::from_utf16_lossy(&text[..len as usize])
        };
        assert_eq!(title(), "Codex: session 12345678 - Permission required");
        release.send(()).unwrap();
        while title() != "Codex: Проверка aHerald - Permission required" {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_ne!(unsafe { GetForegroundWindow() }, hwnd);
        if preview {
            // Expose only this owned fixture window to desktop capture.
            unsafe {
                let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                SetWindowLongPtrW(
                    hwnd,
                    GWL_EXSTYLE,
                    ((style & !WS_EX_TOOLWINDOW.0) | WS_EX_APPWINDOW.0) as isize,
                );
                let _ = ShowWindow(hwnd, SW_HIDE);
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
            }
            std::thread::park_timeout(Duration::from_secs(30));
        }
        drop(close);
        job.join().unwrap().unwrap();
    }
    use crate::animation::PreparedFrame;
    #[test]
    fn controls_accent_and_countdown_render_real_pixels_in_their_regions() {
        let make = |navigation, dismiss_button| {
            Surface::card(
                320,
                110,
                1.0,
                22,
                super::CardText {
                    source: "Codex",
                    label: "Turn complete",
                    navigation,
                    dismiss_button,
                    color: crate::config::Color([16, 32, 48]),
                },
            )
            .unwrap()
        };
        let single = make(false, false);
        let multiple = make(true, false);
        let explicit = make(false, true);
        // SAFETY: every surface owns a 320x110 BGRA DIB and lives through inspection.
        let pixels = |surface: &Surface| unsafe {
            std::slice::from_raw_parts(surface.pixels, 320 * 110 * 4).to_vec()
        };
        let base = pixels(&single);
        let region = |data: &[u8], left: usize, top: usize, right: usize, bottom: usize| {
            (top..bottom)
                .flat_map(|y| (left..right).map(move |x| (y * 320 + x) * 4))
                .filter(|i| data[*i] > 180 && data[*i + 1] > 180 && data[*i + 2] > 180)
                .count()
        };
        assert_eq!(&base[(55 * 320 + 2) * 4..][..4], &[46, 30, 15, 242]);
        assert_eq!(region(&base, 215, 78, 303, 101), 0);
        assert!(region(&pixels(&multiple), 215, 78, 303, 101) > 10);
        assert!(region(&pixels(&explicit), 205, 78, 303, 101) > 40);
        let mut previous = base.clone();
        for second in [3, 2, 1] {
            let timed = pixels(&single.with_countdown(second, 1.0).unwrap());
            assert!(region(&timed, 240, 16, 303, 40) > 20);
            assert_ne!(timed, previous);
            for y in 40..110 {
                assert_eq!(
                    &timed[y * 320 * 4..(y + 1) * 320 * 4],
                    &base[y * 320 * 4..(y + 1) * 320 * 4]
                );
            }
            assert!(
                timed
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| p[..3].iter().all(|c| *c <= p[3]))
            );
            previous = timed;
        }
        assert_eq!(
            pixels(&single),
            base,
            "badges must not modify the cached base card"
        );
    }
    #[test]
    fn repeated_card_rendering_and_dpi_resize_release_native_resources() {
        if std::env::var_os("AHERALD_RESOURCE_HELPER").is_none() {
            let child = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "notification::overlay::card_tests::repeated_card_rendering_and_dpi_resize_release_native_resources"])
                .env("AHERALD_RESOURCE_HELPER", "1").spawn().unwrap();
            struct Owned(std::process::Child);
            impl Drop for Owned {
                fn drop(&mut self) {
                    let _ = self.0.kill();
                    let _ = self.0.wait();
                }
            }
            let mut child = Owned(child);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            loop {
                if let Some(status) = child.0.try_wait().unwrap() {
                    assert!(status.success());
                    break;
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "resource cycle helper timed out"
                );
                std::thread::yield_now();
            }
            return;
        }
        use windows::{
            Win32::{Graphics::Gdi::GdiFlush, System::Threading::*, UI::WindowsAndMessaging::*},
            core::w,
        };
        // SAFETY: this isolated test owns the window and every DIB and destroys them
        // on the same thread. Sampling excludes other test processes.
        unsafe {
            let window = super::Window(
                CreateWindowExW(
                    WS_EX_LAYERED | WS_EX_NOACTIVATE,
                    w!("STATIC"),
                    w!("resource test"),
                    WS_POPUP,
                    0,
                    0,
                    1,
                    1,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap(),
            );
            let cycle = || {
                let card = Surface::text(320, 110, 1.0, "Codex", "Error", 22).unwrap();
                let card = card.with_countdown(3, 1.0).unwrap();
                for width in [320, 480, 320] {
                    card.present_placed(window.0, (0, 0, width, 110), 255)
                        .unwrap();
                }
            };
            cycle();
            let _ = GdiFlush();
            let before = GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS);
            let mut handles = 0;
            GetProcessHandleCount(GetCurrentProcess(), &mut handles).unwrap();
            let memory = || {
                let mut info = APP_MEMORY_INFORMATION::default();
                GetProcessInformation(
                    GetCurrentProcess(),
                    ProcessAppMemoryInfo,
                    &mut info as *mut _ as *mut _,
                    std::mem::size_of_val(&info) as u32,
                )
                .unwrap();
                info.PrivateCommitUsage
            };
            let threads = || {
                use windows::Win32::System::Diagnostics::ToolHelp::*;
                let snapshot =
                    crate::local::Handle(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).unwrap());
                let mut entry = PROCESSENTRY32W {
                    dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
                    ..Default::default()
                };
                Process32FirstW(snapshot.0, &mut entry).unwrap();
                loop {
                    if entry.th32ProcessID == GetCurrentProcessId() {
                        break entry.cntThreads;
                    }
                    Process32NextW(snapshot.0, &mut entry).unwrap();
                }
            };
            let before_memory = memory();
            let before_threads = threads();
            for _ in 0..200 {
                cycle();
            }
            let _ = GdiFlush();
            assert_eq!(GetGuiResources(GetCurrentProcess(), GR_GDIOBJECTS), before);
            let mut after = 0;
            GetProcessHandleCount(GetCurrentProcess(), &mut after).unwrap();
            assert_eq!(after, handles);
            assert_eq!(threads(), before_threads);
            assert!(
                memory() <= before_memory + 8 * 1024 * 1024,
                "repeated rendering grew private commit by more than 8 MiB"
            );
        }
    }
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
