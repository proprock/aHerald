use crate::{config::Config, events::Event};
use std::{
    mem::size_of,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::w,
};

/// Everything drawn on a card's base surface.
pub(crate) struct CardText<'a> {
    pub heading: &'a crate::card::Heading,
    /// The event kind, drawn large on one row; with `body` the usage text instead.
    pub label: &'a str,
    /// `label` is several lines of body text rather than the event kind.
    pub body: bool,
    pub index: usize,
    pub count: usize,
    pub dismiss_button: bool,
    /// Coarse age of the event ("2 min ago"); absent for cards that live seconds.
    pub age: Option<&'a str>,
    pub color: crate::config::Color,
    pub background_color: crate::config::Color,
    /// Outline of the card and frame of its buttons; absent draws no card outline.
    pub border: Option<crate::config::Color>,
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
impl Font {
    /// Segoe UI medium of `size` logical pixels; `None` when GDI cannot create it.
    fn segoe(size: f64) -> Option<Self> {
        // SAFETY: plain GDI call with constant arguments; `Font` owns and deletes the handle.
        let font = unsafe {
            CreateFontW(
                -size as i32,
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
            )
        };
        (!font.is_invalid()).then_some(Self(font))
    }
}
/// Draws `text` into `rect` on a memory DC owned by a live `Surface`.
fn draw_text(dc: HDC, text: &str, rect: &mut RECT, format: DRAW_TEXT_FORMAT) {
    let mut text: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: `dc` is a live memory DC and both buffers outlive this synchronous call.
    unsafe { DrawTextW(dc, &mut text, rect, format) };
}
/// GDI text writes RGB but can clear alpha; make every drawn pixel opaque again.
fn restore_alpha(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        if pixel[3] == 0 && pixel[..3].iter().any(|c| *c != 0) {
            pixel[3] = 255;
        }
    }
}
/// Draws a button face: a slightly lighter fill with a one-pixel border and rounded
/// corners, over the card background (BGRA, not yet premultiplied).
fn draw_button_frame(
    pixels: &mut [u8],
    width: i32,
    area: &crate::card::Area,
    background: &[u8; 4],
    border_color: Option<crate::config::Color>,
    scale: f64,
) {
    let lighten =
        |value: u8, amount: u32| (value as u32 + (255 - value as u32) * amount / 100) as u8;
    let fill = [
        lighten(background[0], 8),
        lighten(background[1], 8),
        lighten(background[2], 8),
        255,
    ];
    let border = match border_color {
        Some(c) => [c.0[2], c.0[1], c.0[0], 255],
        None => [
            lighten(background[0], 32),
            lighten(background[1], 32),
            lighten(background[2], 32),
            255,
        ],
    };
    let (w, h) = (area.right - area.left, area.bottom - area.top);
    let radius = ((4.0 * scale) as i32).min(w / 2).min(h / 2);
    let line = (scale.round() as i32).max(1);
    // Distance of a pixel outside the rounded rectangle (negative inside).
    let inside = |x: i32, y: i32, inset: i32| {
        let (left, top) = (inset, inset);
        let (right, bottom) = (w - inset, h - inset);
        let r = (radius - inset).max(0);
        let cx = x.clamp(left + r, right - r - 1);
        let cy = y.clamp(top + r, bottom - r - 1);
        x >= left
            && y >= top
            && x < right
            && y < bottom
            && (x - cx).pow(2) + (y - cy).pow(2) <= r.pow(2)
    };
    for y in 0..h {
        for x in 0..w {
            let colour = if inside(x, y, line) {
                &fill
            } else if inside(x, y, 0) {
                &border
            } else {
                continue;
            };
            let i = (((area.top + y) * width + area.left + x) * 4) as usize;
            if let Some(pixel) = pixels.get_mut(i..i + 4) {
                pixel.copy_from_slice(colour);
            }
        }
    }
}
impl Surface {
    fn byte_len(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }
    fn pixels(&self) -> &[u8] {
        // SAFETY: the DIB section owns `byte_len` bytes until `self` drops, and GDI never
        // draws while a borrow of the surface is alive (drawing needs the DC calls below).
        unsafe { std::slice::from_raw_parts(self.pixels, self.byte_len()) }
    }
    fn pixels_mut(&mut self) -> &mut [u8] {
        // SAFETY: as in `pixels`; `&mut self` makes this the only live view of the bytes.
        unsafe { std::slice::from_raw_parts_mut(self.pixels, self.byte_len()) }
    }
    fn allocate(width: i32, height: i32) -> Result<Self, String> {
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
            Ok(surface)
        }
    }
    /// Replaces this surface's pixels with `base` (same dimensions) and drops the
    /// cached resized copy, which no longer matches.
    fn copy_from(&mut self, base: &Surface) {
        debug_assert_eq!((self.width, self.height), (base.width, base.height));
        self.pixels_mut().copy_from_slice(base.pixels());
        *self.resized.get_mut() = None;
    }
    /// Draws `base` with one artwork frame blended over it into this surface, which
    /// must have the same dimensions as `base`.
    pub(crate) fn blend_artwork(
        &mut self,
        base: &Surface,
        frame: &crate::animation::PreparedFrame,
        left: i32,
        top: i32,
    ) -> Result<(), String> {
        if (self.width, self.height) != (base.width, base.height)
            || left < 0
            || top < 0
            || left as u32 + frame.width > self.width as u32
            || top as u32 + frame.height > self.height as u32
            || frame.bgra.len() != frame.width as usize * frame.height as usize * 4
        {
            return Err("artwork does not fit notification card".into());
        }
        self.copy_from(base);
        // Frames and card pixels are premultiplied BGRA; the bounds were checked above.
        let width = self.width as usize;
        let pixels = self.pixels_mut();
        for y in 0..frame.height as usize {
            for x in 0..frame.width as usize {
                let src = &frame.bgra[(y * frame.width as usize + x) * 4..][..4];
                let offset = ((y + top as usize) * width + x + left as usize) * 4;
                for c in 0..4 {
                    pixels[offset + c] = (src[c] as u16
                        + (pixels[offset + c] as u16 * (255 - src[3] as u16) + 127) / 255)
                        .min(255) as u8;
                }
            }
        }
        Ok(())
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
                heading: &crate::card::Heading::from(source),
                label,
                body: false,
                index: 0,
                count: 1,
                dismiss_button: false,
                age: None,
                color: crate::config::Color([101, 220, 178]),
                background_color: crate::config::Notifications::default().background_color,
                border: None,
            },
        )
    }
    pub(crate) fn body_height(width: i32, scale: f64, label: &str) -> Result<i32, String> {
        let surface = Self::allocate(1, 1)?;
        let font = Font::segoe(18.0 * scale).ok_or("cannot create measurement font")?;
        // SAFETY: the temporary DC and selected font remain alive through measurement.
        let old = unsafe { SelectObject(surface.dc, font.0.into()) };
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: 0,
        };
        draw_text(
            surface.dc,
            label,
            &mut rect,
            DT_LEFT | DT_NOPREFIX | DT_WORDBREAK | DT_CALCRECT,
        );
        // SAFETY: restores the selection made above before the font is deleted.
        unsafe { SelectObject(surface.dc, old) };
        Ok(rect.bottom.max((18.0 * scale) as i32))
    }
    pub(crate) fn card(
        width: i32,
        height: i32,
        scale: f64,
        text_left: i32,
        content: CardText<'_>,
    ) -> Result<Self, String> {
        let CardText {
            heading,
            label,
            body,
            index,
            count,
            dismiss_button,
            age,
            color,
            background_color,
            border,
        } = content;
        let controls = crate::card::Card {
            heading: heading.clone(),
            index,
            count,
            navigation: count > 1,
            dismiss_button,
            text_left: Some(text_left),
        };
        let buttons = controls.buttons(width, height, scale);
        let mut surface = Self::allocate(width, height)?;
        let dc = surface.dc;
        let pixels = surface.pixels_mut();
        let background = [
            background_color.0[2],
            background_color.0[1],
            background_color.0[0],
            242,
        ];
        let radius = (18.0 * scale) as i32;
        let stripe = (8.0 * scale) as i32;
        for y in 0..height {
            for x in 0..width {
                let cx = x.clamp(radius, width - radius - 1);
                let cy = y.clamp(radius, height - radius - 1);
                let inside = (x - cx).pow(2) + (y - cy).pow(2) <= radius.pow(2);
                let i = ((y * width + x) * 4) as usize;
                pixels[i..i + 4].copy_from_slice(if inside { &background } else { &[0, 0, 0, 0] });
                if inside && x < stripe {
                    pixels[i..i + 3].copy_from_slice(&[color.0[2], color.0[1], color.0[0]]);
                }
            }
        }
        if let Some(border) = border {
            let outline = [border.0[2], border.0[1], border.0[0], 255];
            let line = (scale.round() as i32).max(1);
            let within = |x: i32, y: i32, inset: i32| {
                let r = (radius - inset).max(0);
                let cx = x.clamp(inset + r, width - inset - r - 1);
                let cy = y.clamp(inset + r, height - inset - r - 1);
                x >= inset
                    && y >= inset
                    && x < width - inset
                    && y < height - inset
                    && (x - cx).pow(2) + (y - cy).pow(2) <= r.pow(2)
            };
            for y in 0..height {
                for x in 0..width {
                    if within(x, y, 0) && !within(x, y, line) {
                        let i = ((y * width + x) * 4) as usize;
                        pixels[i..i + 4].copy_from_slice(&outline);
                    }
                }
            }
        }
        for (_, area) in &buttons {
            draw_button_frame(pixels, width, area, &background, border, scale);
        }
        let font_at = |size: f64| Font::segoe(size * scale).ok_or("cannot create overlay font");
        let (title_font, name_font, kind_font, row_font) = (
            font_at(18.0)?,
            font_at(16.0)?,
            font_at(22.0)?,
            font_at(15.0)?,
        );
        // SAFETY: the memory DC is owned by `surface`; the original selection is restored
        // below, before the fonts are deleted.
        let old_font = unsafe {
            SetBkMode(dc, TRANSPARENT);
            SelectObject(dc, title_font.0.into())
        };
        let draw = |font: &Font, gray: u32, rect: &mut RECT, text: &str, format| {
            // SAFETY: `dc` stays selected into `surface`, which outlives this closure.
            unsafe {
                SelectObject(dc, font.0.into());
                SetTextColor(dc, COLORREF(gray * 0x0001_0101));
            }
            draw_text(dc, text, rect, format);
        };
        let single = DT_LEFT | DT_NOPREFIX | DT_SINGLELINE | DT_END_ELLIPSIS;
        let px = |v: f64| (v * scale) as i32;
        // Row 1: harness and folder; the right edge keeps clear of the countdown badge.
        let mut rect = RECT {
            left: text_left,
            top: px(14.0),
            right: width - px(82.0),
            bottom: px(38.0),
        };
        draw(&title_font, 0xf5, &mut rect, &heading.title, single);
        // Row 2 is reserved so the card never changes height; empty without a name.
        if let Some(name) = &heading.name {
            let mut rect = RECT {
                left: text_left,
                top: px(38.0),
                right: width - px(18.0),
                bottom: px(60.0),
            };
            draw(&name_font, 0xc4, &mut rect, name, single);
        }
        // Row 3: the event kind, set apart and larger; usage cards show their body instead.
        if !label.is_empty() {
            let (font, top, format, gray) = if body {
                (
                    &title_font,
                    px(66.0),
                    DT_LEFT | DT_NOPREFIX | DT_WORDBREAK,
                    0xf5,
                )
            } else {
                (&kind_font, px(72.0), single, 0xf5)
            };
            let mut rect = RECT {
                left: text_left,
                top,
                right: width - px(18.0),
                bottom: height - px(40.0),
            };
            draw(font, gray, &mut rect, label, format);
        }
        for (action, r) in &buttons {
            let caption = match action {
                crate::card::Action::Previous => "<",
                crate::card::Action::Next => ">",
                crate::card::Action::Dismiss => "Dismiss",
            };
            let mut area = RECT {
                left: r.left,
                top: r.top,
                right: r.right,
                bottom: r.bottom,
            };
            draw(
                &row_font,
                0xf5,
                &mut area,
                caption,
                DT_CENTER | DT_VCENTER | DT_NOPREFIX | DT_SINGLELINE,
            );
        }
        if let Some(counter) = controls.counter() {
            let r = controls.counter_area(width, height, scale);
            let mut area = RECT {
                left: r.left,
                top: r.top,
                right: r.right,
                bottom: r.bottom,
            };
            draw(
                &row_font,
                0xc4,
                &mut area,
                &counter,
                DT_CENTER | DT_VCENTER | DT_NOPREFIX | DT_SINGLELINE,
            );
        }
        if let Some(age) = age {
            let r = controls.age_area(width, height, scale);
            let mut area = RECT {
                left: r.left,
                top: r.top,
                right: r.right,
                bottom: r.bottom,
            };
            draw(
                &row_font,
                0xa0,
                &mut area,
                age,
                DT_LEFT | DT_VCENTER | DT_NOPREFIX | DT_SINGLELINE | DT_END_ELLIPSIS,
            );
        }
        // SAFETY: restores the selection made above; GdiFlush completes pending drawing
        // into the DIB before the pixels are read again.
        unsafe {
            SelectObject(dc, old_font);
            let _ = GdiFlush();
        }
        let pixels = surface.pixels_mut();
        restore_alpha(pixels);
        for pixel in pixels.as_chunks_mut::<4>().0 {
            let alpha = pixel[3] as u16;
            for channel in &mut pixel[..3] {
                *channel = ((*channel as u16 * alpha + 127) / 255) as u8;
            }
        }
        Ok(surface)
    }
    /// Draws the remaining-seconds badge over the pixels already in this surface.
    pub(crate) fn draw_countdown(&mut self, seconds: u64, scale: f64) -> Result<(), String> {
        let surface = self;
        let font = Font::segoe(16.0 * scale).ok_or("cannot create countdown font")?;
        // SAFETY: only the DC owned by `surface` is drawn to; the font selection is
        // restored below, before the font is deleted.
        let old = unsafe {
            let old = SelectObject(surface.dc, font.0.into());
            SetBkMode(surface.dc, TRANSPARENT);
            SetTextColor(surface.dc, COLORREF(0x00f5f5f5));
            old
        };
        let width = surface.width;
        let mut area = RECT {
            left: width - (80.0 * scale) as i32,
            top: (16.0 * scale) as i32,
            right: width - (18.0 * scale) as i32,
            bottom: (40.0 * scale) as i32,
        };
        draw_text(
            surface.dc,
            &format!("{seconds}s"),
            &mut area,
            DT_RIGHT | DT_NOPREFIX | DT_SINGLELINE,
        );
        // SAFETY: restores the selection made above; GdiFlush completes pending drawing
        // into the DIB before the pixels are read again.
        unsafe {
            SelectObject(surface.dc, old);
            let _ = GdiFlush();
        }
        restore_alpha(surface.pixels_mut());
        *surface.resized.get_mut() = None;
        Ok(())
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
                let mut surface = Self::allocate(width, height)?;
                // Copy premultiplied pixels directly so scaling preserves transparent edges.
                let source = self.pixels();
                let target = surface.pixels_mut();
                for y in 0..height as usize {
                    for x in 0..width as usize {
                        let src = ((y * self.height as usize / height as usize)
                            * self.width as usize
                            + x * self.width as usize / width as usize)
                            * 4;
                        let dst = (y * width as usize + x) * 4;
                        target[dst..dst + 4].copy_from_slice(&source[src..src + 4]);
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
        let _timer = crate::perf::span(crate::perf::Span::FramePresent);
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
    composition: Composition,
    /// Configured scale times the DPI scale the card was drawn at.
    render_scale: f64,
    config: Config,
    frame_durations: Vec<u64>,
    looping: bool,
    notifications: crate::config::Notifications,
    placement: (i32, i32, i32, i32),
    start: Instant,
    duration: u64,
    fade_in: u64,
    fade_out: u64,
    controls: crate::card::Card,
    dismissed: Option<u64>,
    failed: bool,
    source_labels: crate::session_source::Service,
    event: Event,
    activation: crate::activation::Worker,
    log_directory: std::path::PathBuf,
    log_level: crate::config::LogLevel,
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
                        let scale = state.render_scale;
                        let mut controls = crate::card::Card::new(
                            source.clone(),
                            0,
                            1,
                            !state.controls.dismiss_button,
                        );
                        controls.text_left = state.controls.text_left;
                        let (config, event) = (&state.config, &state.event);
                        let updated = state.composition.update_card(|width, height, left| {
                            Surface::card(
                                width,
                                height,
                                scale,
                                left,
                                event_card_text(config, event, &controls, None),
                            )
                        });
                        match updated {
                            Ok(()) => {
                                state.controls.heading = source.clone();
                                let title: Vec<u16> = format!(
                                    "{} - {}",
                                    source.window_text(),
                                    state.event.kind.label()
                                )
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
                    if matches!(message, WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE) {
                        // A monitor with another DPI needs the card drawn again, not stretched.
                        let wanted = state.notifications.scale
                            * crate::placement::dpi_scale(&state.notifications);
                        if (wanted - state.render_scale).abs() > 1e-9
                            && let Ok(composition) = build_composition(
                                &state.config,
                                &state.event,
                                &state.controls,
                                wanted,
                            )
                        {
                            state.frame_durations = composition.durations.clone();
                            state.looping = composition.looping;
                            state.composition = composition;
                            state.render_scale = wanted;
                        }
                        if let Ok(p) = crate::placement::current(
                            &state.notifications,
                            state.composition.size(),
                            Some(hwnd.0 as isize),
                        ) {
                            state.placement = p;
                        }
                    }
                    let (card_width, card_height) = state.composition.size();
                    let elapsed = state.start.elapsed().as_millis().min(u64::MAX as u128) as u64;
                    let x = (lp.0 as u16) as i16 as i32 * card_width / state.placement.2.max(1);
                    let y = ((lp.0 >> 16) as u16) as i16 as i32 * card_height
                        / state.placement.3.max(1);
                    if message == WM_LBUTTONUP && state.dismissed.is_none() {
                        state.activation.submit(
                            &state.event,
                            state.notifications.activate_agent_on_click,
                            state.controls.body_hit_test(
                                card_width,
                                card_height,
                                state.render_scale,
                                x,
                                y,
                            ),
                            &state.log_directory,
                            state.log_level,
                        );
                    }
                    if message == WM_LBUTTONUP
                        && state.dismissed.is_none()
                        && state.controls.hit_test(
                            card_width,
                            card_height,
                            state.render_scale,
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
                            let seconds = crate::card::remaining_seconds(state.duration, elapsed);
                            let surface =
                                state
                                    .composition
                                    .render(frame, seconds, state.render_scale)?;
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

/// One card shared by the transient and resident windows: a base card plus the
/// artwork frames, blended into a single working surface only when a frame (or the
/// countdown second) changes. Memory is two card surfaces plus the artwork, however
/// many frames there are.
pub(crate) struct Composition {
    base: Surface,
    work: Surface,
    art: Option<std::sync::Arc<crate::animation::PreparedAnimation>>,
    art_left: i32,
    /// What `work` currently shows: frame and countdown second.
    shown: Option<(usize, Option<u64>)>,
    pub durations: Vec<u64>,
    pub looping: bool,
    pub text_left: i32,
    /// Artwork was available but the text card was used instead.
    pub degraded: bool,
}
impl Composition {
    /// Sizes the card by the largest artwork frame. Any composition failure, or a
    /// card too large to hold twice within the byte budget, falls back to the plain
    /// text card; only a failing text card is an error. The artwork's own size is
    /// already bounded when it is prepared.
    pub(crate) fn compose(
        scale: f64,
        art: Option<std::sync::Arc<crate::animation::PreparedAnimation>>,
        make_card: impl Fn(i32, i32, i32) -> Result<Surface, String>,
    ) -> Result<Self, String> {
        let _timer = crate::perf::span(crate::perf::Span::CompositionBuild);
        crate::perf::count(crate::perf::Counter::CompositionRebuild);
        let (base_width, base_height) = ((320.0 * scale) as i32, crate::card::height(scale));
        let has_art = art.as_ref().is_some_and(|a| !a.frames.is_empty());
        if let Some(art) = art.as_ref().filter(|a| !a.frames.is_empty()) {
            let padding = (18.0 * scale) as i32;
            let art_width = art.frames.iter().map(|f| f.width).max().unwrap_or(0) as i32;
            let art_height = art.frames.iter().map(|f| f.height).max().unwrap_or(0) as i32;
            let width = base_width + art_width + padding;
            let height = base_height.max(art_height + padding * 2);
            let bytes = width as u64 * height as u64 * 4 * 2;
            if bytes <= crate::animation::MAX_BYTES
                && let Ok(base) = make_card(width, height, art_width + padding * 2)
                && let Ok(work) = Surface::allocate(width, height)
            {
                crate::perf::gauge(crate::perf::Gauge::CompositionBytes, bytes);
                return Ok(Self {
                    base,
                    work,
                    art: Some(art.clone()),
                    art_left: padding,
                    shown: None,
                    durations: art.durations_ms.clone(),
                    looping: art.looping,
                    text_left: art_width + padding * 2,
                    degraded: false,
                });
            }
        }
        let text_left = (22.0 * scale) as i32;
        crate::perf::gauge(
            crate::perf::Gauge::CompositionBytes,
            base_width as u64 * base_height as u64 * 4 * 2,
        );
        Ok(Self {
            base: make_card(base_width, base_height, text_left)?,
            work: Surface::allocate(base_width, base_height)?,
            art: None,
            art_left: 0,
            shown: None,
            durations: vec![1000],
            looping: false,
            text_left,
            degraded: has_art,
        })
    }
    pub(crate) fn frame_count(&self) -> usize {
        self.art.as_ref().map_or(1, |a| a.frames.len())
    }
    pub(crate) fn size(&self) -> (i32, i32) {
        (self.base.width, self.base.height)
    }
    /// Draws `frame`, with the countdown badge when `seconds` is given, into the
    /// working surface and returns it. Asking again for what is shown costs nothing.
    pub(crate) fn render(
        &mut self,
        frame: usize,
        seconds: Option<u64>,
        scale: f64,
    ) -> Result<&Surface, String> {
        let frame = frame.min(self.frame_count() - 1);
        if self.shown != Some((frame, seconds)) {
            let _timer = crate::perf::span(crate::perf::Span::FrameRender);
            self.shown = None;
            match &self.art {
                Some(art) => {
                    let f = &art.frames[frame];
                    let top = (self.base.height - f.height as i32) / 2;
                    self.work.blend_artwork(&self.base, f, self.art_left, top)?;
                }
                None => self.work.copy_from(&self.base),
            }
            if let Some(seconds) = seconds {
                self.work.draw_countdown(seconds, scale)?;
            }
            self.shown = Some((frame, seconds));
        }
        Ok(&self.work)
    }
    /// Redraws the base card's text (heading, counter, age) at the same size; the artwork
    /// is blended at render time, so it and its frames are untouched.
    pub(crate) fn update_card(
        &mut self,
        make_card: impl FnOnce(i32, i32, i32) -> Result<Surface, String>,
    ) -> Result<(), String> {
        self.base = make_card(self.base.width, self.base.height, self.text_left)?;
        self.shown = None;
        Ok(())
    }
}

/// Registers the transient card class; an existing registration (a second card in the
/// same process) is reused. Must match `placement::TRANSIENT_CLASS`.
pub(crate) fn register_class(
    instance: windows::Win32::Foundation::HINSTANCE,
) -> Result<windows::core::PCWSTR, String> {
    let class = w!("aHerald.Overlay");
    // SAFETY: plain class registration with a static name and this module's window procedure.
    unsafe {
        let wc = WNDCLASSW {
            lpfnWndProc: Some(window_proc),
            hInstance: instance,
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).map_err(|_| "cannot load cursor")?,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0
            && windows::Win32::Foundation::GetLastError()
                != windows::Win32::Foundation::ERROR_CLASS_ALREADY_EXISTS
        {
            return Err("cannot register overlay class".into());
        }
    }
    Ok(class)
}

/// Serializes card placement across hook processes of this Windows session.
fn placement_guard() -> Option<crate::local::Ownership> {
    let scope = crate::local::Scope::current("overlay-placement").ok()?;
    let deadline = Instant::now() + Duration::from_millis(500);
    loop {
        match scope.acquire() {
            Ok(Some(owner)) => return Some(owner),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => return None,
        }
    }
}

/// What an event card shows: its heading, kind, queue position and (resident) age.
pub(crate) fn event_card_text<'a>(
    config: &Config,
    event: &Event,
    controls: &'a crate::card::Card,
    age: Option<&'a str>,
) -> CardText<'a> {
    CardText {
        heading: &controls.heading,
        label: event.kind.label(),
        body: false,
        index: controls.index,
        count: controls.count,
        dismiss_button: controls.dismiss_button,
        age,
        color: config.event_color(event.kind),
        background_color: config.event_background_color(event.kind),
        border: config.event_border_color(event.kind),
    }
}

/// Draws the card for `event` at `scale` (configured scale times the monitor's DPI
/// scale). Artwork is prepared on a helper thread, as asset decoding stays off the
/// thread that owns the window.
fn build_composition(
    config: &Config,
    event: &Event,
    controls: &crate::card::Card,
    scale: f64,
) -> Result<Composition, String> {
    let make_card = |width, height, text_left| {
        Surface::card(
            width,
            height,
            scale,
            text_left,
            event_card_text(config, event, controls, None),
        )
    };
    let assets = config.clone();
    let asset_event = event.clone();
    let prepared = std::thread::spawn(move || {
        crate::artwork::prepared_for_event(&assets, &asset_event, scale)
    })
    .join()
    .ok()
    .and_then(Result::ok)
    .flatten();
    Composition::compose(scale, prepared.map(std::sync::Arc::new), make_card)
}

pub fn show(event: &Event, config: &Config) -> Result<(), String> {
    show_with_source(
        event,
        config,
        crate::session_source::Service::transient_codex(config.codex.transient_names),
    )
}
fn show_with_source(
    event: &Event,
    config: &Config,
    mut source_labels: crate::session_source::Service,
) -> Result<(), String> {
    // SAFETY: all Win32 objects are created/used/destroyed on this thread; boxed State outlives Window.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        // The card is drawn at the DPI of the monitor it will appear on.
        let scale = config.notifications.scale * crate::placement::dpi_scale(&config.notifications);
        let label = event.kind.label();
        let source = source_labels.label(event);
        let mut controls =
            crate::card::Card::new(source.clone(), 0, 1, config.notifications.click_to_dismiss);
        let composition = build_composition(config, event, &controls, scale)?;
        controls.text_left = Some(composition.text_left);
        let (frame_durations, looping) = (composition.durations.clone(), composition.looping);
        let (width, height) = composition.size();
        let position = POINT::default();
        // Concurrent hook processes pick their spot one at a time; the guard is held
        // until this card is visible so the next process sees it. A timeout only risks
        // an overlap, never a missing card.
        let placing = placement_guard();
        let placement = crate::placement::current(&config.notifications, (width, height), Some(0))?;
        let mut state = Box::new(State {
            composition,
            render_scale: scale,
            config: config.clone(),
            frame_durations,
            looping,
            notifications: config.notifications.clone(),
            placement,
            start: Instant::now(),
            duration: config.events[event.kind].duration_ms,
            fade_in: config.notifications.fade_in_ms,
            fade_out: config.notifications.fade_out_ms,
            controls,
            dismissed: None,
            failed: false,
            source_labels,
            event: event.clone(),
            activation: crate::activation::Worker::default(),
            log_directory: config.diagnostic_directory.clone(),
            log_level: config.general.logging,
        });
        let instance = GetModuleHandleW(None).map_err(|_| "cannot find module handle")?;
        let class = register_class(instance.into())?;
        let title: Vec<u16> = format!("{} - {label}", source.window_text())
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
        let first_alpha = if state.fade_in == 0 { 255 } else { 0 };
        let placement = state.placement;
        state
            .composition
            .render(0, None, scale)?
            .present_placed(hwnd, placement, first_alpha)?;
        if SetTimer(Some(hwnd), 1, 16, None) == 0 {
            return Err("cannot start overlay timer".into());
        }
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        drop(placing);

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
mod class_tests {
    #[test]
    fn a_second_card_in_one_process_reuses_the_registered_class() {
        // SAFETY: the module handle query has no preconditions.
        let instance = unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None) }
            .unwrap()
            .into();
        assert!(super::register_class(instance).is_ok());
        assert!(super::register_class(instance).is_ok());
    }
}

#[cfg(test)]
mod composition_tests {
    use super::{Composition, Surface};
    use crate::animation::{PreparedAnimation, PreparedFrame};
    fn frame(width: u32, height: u32) -> PreparedFrame {
        PreparedFrame {
            width,
            height,
            bgra: vec![255; (width * height * 4) as usize],
        }
    }
    fn art(sizes: &[(u32, u32)]) -> PreparedAnimation {
        PreparedAnimation {
            frames: sizes.iter().map(|&(w, h)| frame(w, h)).collect(),
            durations_ms: vec![40; sizes.len()],
            looping: true,
        }
    }
    fn card(width: i32, height: i32, left: i32) -> Result<Surface, String> {
        Surface::text(width, height, 1.0, "Codex", "Turn complete", left)
    }
    fn shared(art: PreparedAnimation) -> Option<std::sync::Arc<PreparedAnimation>> {
        Some(std::sync::Arc::new(art))
    }
    fn pixel(surface: &Surface, x: usize, y: usize) -> [u8; 4] {
        surface.pixels()[(y * surface.width as usize + x) * 4..][..4]
            .try_into()
            .unwrap()
    }
    /// Frame `index` is an opaque block whose blue channel identifies it.
    fn numbered(count: usize, width: u32, height: u32) -> PreparedAnimation {
        PreparedAnimation {
            frames: (0..count)
                .map(|i| PreparedFrame {
                    width,
                    height,
                    bgra: [i as u8, 0, 0, 255].repeat((width * height) as usize),
                })
                .collect(),
            durations_ms: vec![40; count],
            looping: true,
        }
    }
    #[test]
    fn card_is_sized_by_the_largest_frame() {
        let c = Composition::compose(1.0, shared(art(&[(40, 40), (64, 150)])), card).unwrap();
        assert_eq!(c.frame_count(), 2);
        assert_eq!(c.size(), (402, 186));
        assert_eq!((c.text_left, c.looping), (100, true));
        assert_eq!(c.durations, [40, 40]);
    }
    #[test]
    fn failing_card_or_artwork_falls_back_to_one_text_card() {
        let wide = |w: i32, h: i32, l: i32| {
            if w > 320 {
                Err("injected".into())
            } else {
                card(w, h, l)
            }
        };
        let c = Composition::compose(1.0, shared(art(&[(40, 40)])), wide).unwrap();
        assert_eq!((c.frame_count(), c.size().0, c.text_left), (1, 320, 22));
        assert!(c.degraded, "a fallback with artwork available is reported");
    }
    #[test]
    fn missing_artwork_uses_text_card_and_text_failure_is_an_error() {
        let c = Composition::compose(1.0, None, card).unwrap();
        assert_eq!(c.frame_count(), 1);
        assert!(Composition::compose(1.0, None, |_, _, _| Err("boom".into())).is_err());
    }
    #[test]
    fn an_animation_whose_full_card_frames_exceed_the_budget_keeps_its_artwork() {
        // 80 frames of a 530x244 card would need ~41 MiB as full-card surfaces.
        let count = 80;
        assert!(530 * 244 * 4 * count as u64 > crate::animation::MAX_BYTES);
        let mut c = Composition::compose(1.0, shared(numbered(count, 192, 208)), card).unwrap();
        assert!(!c.degraded);
        assert_eq!((c.frame_count(), c.size()), (count, (530, 244)));
        for index in [0, 41, count - 1] {
            let surface = c.render(index, None, 1.0).unwrap();
            assert_eq!(pixel(surface, 20, 20), [index as u8, 0, 0, 255]);
            assert_eq!(
                pixel(surface, 0, 0),
                [0, 0, 0, 0],
                "rounded corner stays clear"
            );
        }
    }
    #[test]
    fn rendering_a_frame_leaves_no_trace_of_the_previous_one() {
        let mut animation = numbered(2, 40, 40);
        animation.frames[1].bgra = vec![0; 40 * 40 * 4];
        let mut c = Composition::compose(1.0, shared(animation), card).unwrap();
        let behind = card(378, 150, 76).unwrap();
        assert_eq!(
            pixel(c.render(0, None, 1.0).unwrap(), 30, 75),
            [0, 0, 0, 255]
        );
        let transparent = c.render(1, None, 1.0).unwrap();
        // A fully transparent frame shows exactly the card behind it.
        assert_eq!(transparent.pixels(), behind.pixels());
    }
    #[test]
    fn countdown_is_drawn_per_frame_and_removed_without_touching_the_base() {
        let mut c = Composition::compose(1.0, shared(numbered(2, 40, 40)), card).unwrap();
        let plain = c.render(1, None, 1.0).unwrap().pixels().to_vec();
        let timed = c.render(1, Some(3), 1.0).unwrap().pixels().to_vec();
        assert_ne!(plain, timed);
        let width = c.size().0 as usize;
        let row = |data: &[u8], y: usize| data[y * width * 4..(y + 1) * width * 4].to_vec();
        for y in 60..c.size().1 as usize {
            assert_eq!(
                row(&plain, y),
                row(&timed, y),
                "only the badge row band differs"
            );
        }
        assert_eq!(c.render(1, None, 1.0).unwrap().pixels(), plain.as_slice());
    }
    #[test]
    fn a_new_session_name_redraws_one_base_card_and_keeps_every_frame() {
        let mut c = Composition::compose(1.0, shared(numbered(3, 40, 40)), card).unwrap();
        let before = c.render(2, None, 1.0).unwrap().pixels().to_vec();
        c.update_card(|w, h, left| {
            Surface::card(
                w,
                h,
                1.0,
                left,
                super::CardText {
                    heading: &crate::card::Heading::new("Codex", Some("aHerald"), Some("Проверка")),
                    label: "Error",
                    body: false,
                    index: 0,
                    count: 1,
                    dismiss_button: false,
                    age: None,
                    color: crate::config::Color([101, 220, 178]),
                    background_color: crate::config::Notifications::default().background_color,
                    border: None,
                },
            )
        })
        .unwrap();
        let after = c.render(2, None, 1.0).unwrap();
        assert_ne!(before.as_slice(), after.pixels(), "heading text changed");
        assert_eq!(pixel(after, 30, 75), [2, 0, 0, 255], "artwork survives");
        assert_eq!(c.render(0, None, 1.0).unwrap().pixels()[..4], [0, 0, 0, 0]);
    }
}

#[cfg(test)]
mod card_tests {
    use super::Surface;
    fn drawn(
        heading: &crate::card::Heading,
        count: usize,
        dismiss: bool,
        age: Option<&str>,
    ) -> Vec<u8> {
        let surface = Surface::card(
            320,
            150,
            1.0,
            22,
            super::CardText {
                heading,
                label: "Permission required",
                body: false,
                index: 0,
                count,
                dismiss_button: dismiss,
                age,
                color: crate::config::Color([250, 180, 55]),
                background_color: crate::config::Notifications::default().background_color,
                border: None,
            },
        )
        .unwrap();
        surface.pixels().to_vec()
    }
    /// Pixels of the card inside `x`, `y` ranges.
    fn region(data: &[u8], x: std::ops::Range<usize>, y: std::ops::Range<usize>) -> Vec<u8> {
        y.flat_map(|y| data[(y * 320 + x.start) * 4..(y * 320 + x.end) * 4].to_vec())
            .collect()
    }
    #[test]
    fn a_configured_border_outlines_the_card_and_its_buttons() {
        let border = crate::config::Color([255, 0, 0]);
        let make = |border| {
            Surface::card(
                320,
                150,
                1.0,
                22,
                super::CardText {
                    heading: &"Codex".into(),
                    label: "Turn complete",
                    body: false,
                    index: 0,
                    count: 3,
                    dismiss_button: false,
                    age: None,
                    color: crate::config::Color([65, 210, 140]),
                    background_color: crate::config::Notifications::default().background_color,
                    border,
                },
            )
            .unwrap()
        };
        let (plain, framed) = (make(None), make(Some(border)));
        let at = |s: &Surface, x: usize, y: usize| s.pixels()[(y * 320 + x) * 4..][..4].to_vec();
        // Straight edges, away from the rounded corners and the stripe: opaque border colour.
        for (x, y) in [(160, 0), (160, 149), (319, 75)] {
            assert_eq!(at(&framed, x, y), [0, 0, 255, 255], "edge {x},{y}");
            assert_ne!(at(&plain, x, y), at(&framed, x, y));
        }
        // The pixel just inside is untouched card background.
        assert_eq!(at(&framed, 160, 3), at(&plain, 160, 3));
        // Buttons take the same colour for their frame.
        let next = crate::card::Card::new("Codex", 0, 3, true)
            .buttons(320, 150, 1.0)
            .into_iter()
            .find(|(a, _)| *a == crate::card::Action::Next)
            .unwrap()
            .1;
        assert_eq!(
            at(&framed, next.left as usize + 10, next.top as usize),
            [0, 0, 255, 255]
        );
    }
    #[test]
    fn the_stripe_is_wide_and_carries_the_event_colour() {
        let data = drawn(&"Codex".into(), 1, false, None);
        let at = |x: usize, y: usize| data[(y * 320 + x) * 4..][..4].to_vec();
        // Premultiplied BGRA of #FAB437 at the card alpha.
        assert_eq!(
            at(7, 75)[..3],
            at(1, 75)[..3],
            "stripe is at least 8 px wide"
        );
        assert_ne!(
            at(7, 75)[..3],
            at(12, 75)[..3],
            "and ends before the text column"
        );
    }
    #[test]
    fn name_row_is_reserved_and_the_name_changes_only_that_row() {
        let without = drawn(&"Codex".into(), 1, false, None);
        let named = crate::card::Heading::new("Codex", None, Some("Fix the build"));
        let with = drawn(&named, 1, false, None);
        assert_eq!(
            region(&without, 0..320, 0..38),
            region(&with, 0..320, 0..38),
            "title row"
        );
        assert_ne!(
            region(&without, 22..300, 38..60),
            region(&with, 22..300, 38..60)
        );
        assert_eq!(
            region(&without, 0..320, 60..150),
            region(&with, 0..320, 60..150),
            "below"
        );
    }
    #[test]
    fn several_cards_add_framed_arrows_and_a_counter_without_moving_the_text() {
        let one = drawn(&"Codex".into(), 1, false, None);
        let many = drawn(&"Codex".into(), 5, false, None);
        assert_eq!(
            region(&one, 0..320, 0..110),
            region(&many, 0..320, 0..110),
            "text rows"
        );
        let buttons = crate::card::Card::new("Codex", 0, 5, true).buttons(320, 150, 1.0);
        for (_, area) in buttons {
            let (l, t, r, b) = (
                area.left as usize,
                area.top as usize,
                area.right as usize,
                area.bottom as usize,
            );
            let changed = |x: usize, y: usize| {
                one[(y * 320 + x) * 4..][..4] != many[(y * 320 + x) * 4..][..4]
            };
            // The border pixels on all four edges differ from the plain card.
            assert!(
                changed(l + 8, t) && changed(l + 8, b - 1),
                "top and bottom edges"
            );
            assert!(
                changed(l, (t + b) / 2) && changed(r - 1, (t + b) / 2),
                "side edges"
            );
        }
    }
    #[test]
    fn dismiss_button_and_age_share_the_bottom_left() {
        let plain = drawn(&"Codex".into(), 1, false, None);
        let dismiss = drawn(&"Codex".into(), 1, true, None);
        let aged = drawn(&"Codex".into(), 1, true, Some("2 min ago"));
        assert_ne!(
            region(&plain, 22..118, 116..140),
            region(&dismiss, 22..118, 116..140)
        );
        assert_ne!(
            region(&dismiss, 128..220, 116..140),
            region(&aged, 128..220, 116..140)
        );
        assert_eq!(
            region(&dismiss, 0..320, 0..110),
            region(&aged, 0..320, 0..110)
        );
        assert_eq!(
            region(&plain, 250..320, 116..140),
            region(&aged, 250..320, 116..140),
            "right side stays clear"
        );
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
        event.folder = Some("aHerald".into());
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
        assert_eq!(title(), "Codex, aHerald - Permission required");
        release.send(()).unwrap();
        while title() != "Codex, aHerald: Проверка aHerald - Permission required" {
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
    #[ignore = "native background color comparison; closes after 60 seconds"]
    fn manual_background_color_preview() {
        use super::*;
        use std::time::Duration;
        unsafe extern "system" fn preview_proc(
            hwnd: HWND,
            message: u32,
            wp: WPARAM,
            lp: LPARAM,
        ) -> LRESULT {
            unsafe { DefWindowProcW(hwnd, message, wp, lp) }
        }
        let config = Config::parse("[notifications]\nbackground_color='#203C30'\n[events.error]\nbackground_color='#482838'\n[codex.usage]\nbackground_color='#243C58'", std::path::Path::new(".")).unwrap();
        let cards = [
            (
                "Global background",
                config.event_background_color(crate::events::Kind::TurnComplete),
            ),
            (
                "Event override",
                config.event_background_color(crate::events::Kind::Error),
            ),
            ("Usage override", config.usage_background_color()),
        ]
        .into_iter()
        .map(|(source, background_color)| {
            Surface::card(
                320,
                150,
                1.0,
                22,
                CardText {
                    heading: &crate::card::Heading::from(source),
                    label: "Custom card background",
                    body: false,
                    index: 0,
                    count: 1,
                    dismiss_button: false,
                    age: None,
                    color: crate::config::Color([65, 210, 140]),
                    background_color,
                    border: None,
                },
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
        // SAFETY: the window, DC and native card surfaces stay on this test thread.
        unsafe {
            let module = GetModuleHandleW(None).unwrap();
            let class = w!("aHerald.BackgroundPreview");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(preview_proc),
                hInstance: module.into(),
                lpszClassName: class,
                ..Default::default()
            };
            assert_ne!(RegisterClassW(&wc), 0);
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class,
                w!("AH-035: global / event / usage backgrounds"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                100,
                100,
                1040,
                210,
                None,
                None,
                Some(module.into()),
                None,
            )
            .unwrap();
            let window = Window(hwnd);
            let end = Instant::now() + Duration::from_secs(60);
            let mut message = MSG::default();
            while Instant::now() < end && IsWindow(Some(hwnd)).as_bool() {
                while PeekMessageW(&mut message, Some(hwnd), 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
                if !IsWindow(Some(hwnd)).as_bool() {
                    break;
                }
                let dc = GetDC(Some(hwnd));
                for (index, card) in cards.iter().enumerate() {
                    BitBlt(
                        dc,
                        10 + index as i32 * 330,
                        20,
                        320,
                        110,
                        Some(card.dc),
                        0,
                        0,
                        SRCCOPY,
                    )
                    .unwrap();
                }
                ReleaseDC(Some(hwnd), dc);
                std::thread::park_timeout(Duration::from_millis(25));
            }
            drop(window);
            UnregisterClassW(class, Some(module.into())).unwrap();
        }
    }
    #[test]
    fn custom_background_renders_premultiplied_pixels_and_survives_heading_updates() {
        let background_color = crate::config::Color([16, 32, 48]);
        let card = Surface::card(
            320,
            150,
            1.0,
            22,
            super::CardText {
                heading: &crate::card::Heading::from("Codex"),
                label: "Error",
                body: false,
                index: 0,
                count: 1,
                dismiss_button: false,
                age: None,
                color: crate::config::Color([245, 85, 100]),
                background_color,
                border: None,
            },
        )
        .unwrap();
        let updated = Surface::card(
            320,
            150,
            1.0,
            22,
            super::CardText {
                heading: &crate::card::Heading::from("New"),
                label: "Error",
                body: false,
                index: 0,
                count: 1,
                dismiss_button: false,
                age: None,
                color: crate::config::Color([245, 85, 100]),
                background_color,
                border: None,
            },
        )
        .unwrap();
        for surface in [&card, &updated] {
            // SAFETY: each surface owns a live 320x150 BGRA DIB.
            let pixels = unsafe { std::slice::from_raw_parts(surface.pixels, 320 * 150 * 4) };
            assert_eq!(&pixels[(90 * 320 + 100) * 4..][..4], &[46, 30, 15, 242]);
            assert_eq!(&pixels[(25 * 320 + 150) * 4..][..4], &[46, 30, 15, 242]);
            assert_eq!(&pixels[..4], &[0, 0, 0, 0]);
            assert_eq!(&pixels[(55 * 320 + 2) * 4..][..4], &[95, 81, 233, 242]);
        }
    }
    #[test]
    fn controls_accent_and_countdown_render_real_pixels_in_their_regions() {
        let make = |navigation, dismiss_button| {
            Surface::card(
                320,
                150,
                1.0,
                22,
                super::CardText {
                    heading: &crate::card::Heading::from("Codex"),
                    label: "Turn complete",
                    body: false,
                    index: 0,
                    count: if navigation { 3 } else { 1 },
                    dismiss_button,
                    age: None,
                    color: crate::config::Color([16, 32, 48]),
                    background_color: crate::config::Notifications::default().background_color,
                    border: None,
                },
            )
            .unwrap()
        };
        let single = make(false, false);
        let multiple = make(true, false);
        let explicit = make(false, true);
        // SAFETY: every surface owns a 320x150 BGRA DIB and lives through inspection.
        let pixels = |surface: &Surface| unsafe {
            std::slice::from_raw_parts(surface.pixels, 320 * 150 * 4).to_vec()
        };
        let base = pixels(&single);
        let region = |data: &[u8], left: usize, top: usize, right: usize, bottom: usize| {
            (top..bottom)
                .flat_map(|y| (left..right).map(move |x| (y * 320 + x) * 4))
                .filter(|i| data[*i] > 180 && data[*i + 1] > 180 && data[*i + 2] > 180)
                .count()
        };
        assert_eq!(&base[(55 * 320 + 2) * 4..][..4], &[46, 30, 15, 242]);
        assert_eq!(
            region(&base, 22, 116, 303, 140),
            0,
            "no controls on a single card"
        );
        assert!(
            region(&pixels(&multiple), 190, 116, 303, 140) > 10,
            "arrows and counter"
        );
        assert!(
            region(&pixels(&explicit), 22, 116, 118, 140) > 40,
            "dismiss button"
        );
        let mut previous = base.clone();
        for second in [3, 2, 1] {
            let mut drawn = Surface::allocate(320, 150).unwrap();
            drawn.copy_from(&single);
            drawn.draw_countdown(second, 1.0).unwrap();
            let timed = pixels(&drawn);
            assert!(region(&timed, 240, 16, 303, 40) > 20);
            assert_ne!(timed, previous);
            for y in 40..150 {
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
                let mut animated = super::Composition::compose(
                    1.0,
                    Some(std::sync::Arc::new(crate::animation::PreparedAnimation {
                        frames: (0..4)
                            .map(|i| crate::animation::PreparedFrame {
                                width: 8,
                                height: 8,
                                bgra: vec![i; 8 * 8 * 4],
                            })
                            .collect(),
                        durations_ms: vec![40; 4],
                        looping: true,
                    })),
                    |w, h, left| Surface::text(w, h, 1.0, "Codex", "Error", left),
                )
                .unwrap();
                for frame in 0..8 {
                    animated
                        .render(frame, Some(3 - frame as u64 % 3), 1.0)
                        .unwrap();
                }
                animated
                    .update_card(|w, h, left| {
                        Surface::text(w, h, 1.0, "Codex: renamed", "Error", left)
                    })
                    .unwrap();
                let mut shown = Surface::allocate(320, 110).unwrap();
                shown.copy_from(&card);
                shown.draw_countdown(3, 1.0).unwrap();
                let card = shown;
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
        let mut composed = Surface::allocate(530, 244).unwrap();
        composed.blend_artwork(&card, &frame, 18, 18).unwrap();
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
        assert!(composed.blend_artwork(&card, &frame, 400, 18).is_err());
    }
}
