//! Single resident card. The GUI never waits for asset decoding or IPC.
use crate::{
    animation::PreparedAnimation,
    config::Config,
    notification::overlay::{CardText, Surface},
    resident::Shared,
};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Instant,
};
use windows::{
    Win32::{
        Foundation::*,
        Graphics::Gdi::*,
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};
type RenderResult = (u64, Option<PreparedAnimation>);
struct Ui {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    config: Config,
    scale: f64,
    jobs: Option<mpsc::SyncSender<(u64, crate::events::Event)>>,
    results: mpsc::Receiver<RenderResult>,
    surfaces: Vec<Surface>,
    durations: Vec<u64>,
    looping: bool,
    selection: Option<u64>,
    layout_count: usize,
    art: Option<PreparedAnimation>,
    start: Instant,
    animation_start: u64,
    pending: bool,
    audio: Vec<std::thread::JoinHandle<()>>,
    sounded: std::collections::HashSet<u64>,
    text_left: i32,
    width: i32,
    height: i32,
    position: POINT,
}
impl Ui {
    fn now(&self) -> u64 {
        self.start.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    fn draw(&mut self, hwnd: HWND) -> Result<(), String> {
        if self.stop.load(Ordering::Acquire) {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return Ok(());
        }
        let now = self.now();
        let (entry, index, count, paused) = {
            let mut state = self
                .shared
                .lock()
                .map_err(|_| "resident state unavailable")?;
            let paused = state.paused;
            state.queue.tick(now, paused);
            (
                state.queue.selected().cloned(),
                state.queue.selected_index(),
                state.queue.items().len(),
                paused,
            )
        };
        let Some(entry) = entry.filter(|_| !paused) else {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            self.selection = None;
            return Ok(());
        };
        if self.sounded.insert(entry.id)
            && let Some(audio) = crate::audio::start(&entry.event, &self.config)
        {
            self.audio.push(audio);
        }
        self.audio.retain(|job| !job.is_finished());
        self.sounded.retain(|id| {
            self.shared
                .lock()
                .ok()
                .is_some_and(|state| state.queue.items().iter().any(|e| e.id == *id))
        });
        if !self.config.notifications.enabled {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            self.selection = None;
            return Ok(());
        }
        let changed = self.selection != Some(entry.id);
        if changed {
            self.selection = Some(entry.id);
            self.art = None;
            self.pending = true;
            self.animation_start = now;
        }
        if self.pending
            && self
                .jobs
                .as_ref()
                .is_some_and(|tx| tx.try_send((entry.id, entry.event.clone())).is_ok())
        {
            self.pending = false;
        }
        let mut loaded = false;
        while let Ok((id, art)) = self.results.try_recv() {
            if Some(id) == self.selection {
                self.art = art;
                loaded = true;
            }
        }
        if changed || loaded || self.layout_count != count {
            self.layout_count = count;
            let pad = (18.0 * self.scale) as i32;
            let aw = self
                .art
                .as_ref()
                .and_then(|a| a.frames.first())
                .map(|f| f.width as i32)
                .unwrap_or(0);
            let ah = self
                .art
                .as_ref()
                .and_then(|a| a.frames.first())
                .map(|f| f.height as i32)
                .unwrap_or(0);
            self.width = (320.0 * self.scale) as i32 + if aw > 0 { aw + pad } else { 0 };
            self.height = ((110.0 * self.scale) as i32).max(ah + if ah > 0 { pad * 2 } else { 0 });
            self.text_left = if aw > 0 {
                aw + pad * 2
            } else {
                (22.0 * self.scale) as i32
            };
            let source = format!("{} [{}/{}]", entry.event.source.label(), index + 1, count);
            let card = Surface::card(
                self.width,
                self.height,
                self.scale,
                self.text_left,
                CardText {
                    source: &source,
                    label: entry.event.kind.label(),
                    navigation: true,
                },
            )?;
            self.durations = vec![1000];
            self.looping = false;
            let bytes = self.width as u64
                * self.height as u64
                * 4
                * self.art.as_ref().map(|a| a.frames.len()).unwrap_or(1) as u64;
            let composed = self
                .art
                .as_ref()
                .filter(|_| bytes <= crate::animation::MAX_BYTES)
                .map(|a| {
                    a.frames
                        .iter()
                        .map(|f| card.with_artwork(f, pad, (self.height - f.height as i32) / 2))
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?;
            self.surfaces = if let Some(frames) = composed.filter(|f| !f.is_empty()) {
                if let Some(a) = &self.art {
                    self.durations = a.durations_ms.clone();
                    self.looping = a.looping;
                }
                frames
            } else {
                vec![card]
            };
            let title: Vec<u16> = format!(
                "{} - {} [{}/{}]",
                entry.event.source.label(),
                entry.event.kind.label(),
                index + 1,
                count
            )
            .encode_utf16()
            .chain(Some(0))
            .collect();
            unsafe {
                SetWindowTextW(hwnd, PCWSTR(title.as_ptr()))
                    .map_err(|_| "cannot update event title")?;
            }
            let mut cursor = POINT::default();
            unsafe {
                let _ = GetCursorPos(&mut cursor);
            }
            let point = if self.config.notifications.position == "near_cursor" {
                cursor
            } else {
                POINT::default()
            };
            let mut info = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };
            unsafe {
                if !GetMonitorInfoW(MonitorFromPoint(point, MONITOR_DEFAULTTOPRIMARY), &mut info)
                    .as_bool()
                {
                    return Err("cannot locate monitor".into());
                }
            }
            let a = info.rcWork;
            let (x, y) = match self.config.notifications.position.as_str() {
                "top_center" => ((a.left + a.right - self.width) / 2, a.top + 24),
                "bottom_right" => (a.right - self.width - 24, a.bottom - self.height - 24),
                "near_cursor" => (cursor.x + 16, cursor.y + 16),
                _ => (
                    (a.left + a.right - self.width) / 2,
                    (a.top + a.bottom - self.height) / 2,
                ),
            };
            self.position = POINT {
                x: x.clamp(a.left, (a.right - self.width).max(a.left)),
                y: y.clamp(a.top, (a.bottom - self.height).max(a.top)),
            };
        }
        let frame = crate::animation::frame_index(
            &self.durations,
            self.looping,
            now.saturating_sub(self.animation_start),
        );
        let alpha = crate::notification::presentation(
            entry.visible_ms,
            self.config.events[entry.event.kind.name()].duration_ms,
            self.config.notifications.fade_in_ms,
            self.config.notifications.fade_out_ms,
            None,
        )
        .0;
        self.surfaces[frame].present(hwnd, self.position, alpha)?;
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        Ok(())
    }
    fn input(&mut self, message: u32, wp: WPARAM, lp: LPARAM) {
        let mut state = match self.shared.lock() {
            Ok(s) => s,
            Err(_) => return,
        };
        let paused = state.paused;
        state.queue.tick(self.now(), paused);
        if message == WM_KEYDOWN {
            match wp.0 {
                0x25 => state.queue.next(-1),
                0x27 => state.queue.next(1),
                0x1b | 0x2e => state.queue.dismiss(),
                _ => {}
            }
        } else {
            let x = (lp.0 as u16) as i16 as i32;
            let y = ((lp.0 >> 16) as u16) as i16 as i32;
            if y >= self.height - (36.0 * self.scale) as i32 && x >= self.text_left {
                let span = (self.width - (18.0 * self.scale) as i32 - self.text_left) / 3;
                match (x - self.text_left) / span.max(1) {
                    0 => state.queue.next(-1),
                    1 => state.queue.next(1),
                    _ => state.queue.dismiss(),
                }
            } else if self.config.notifications.click_to_dismiss {
                state.queue.dismiss();
            }
        }
    }
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: Ui is boxed for the window's lifetime, and accessed only by its GUI thread.
    unsafe {
        if msg == WM_NCCREATE {
            let create = &*(lp.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        match msg {
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_DESTROY => {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            WM_CLOSE => {
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_TIMER | WM_LBUTTONUP | WM_KEYDOWN => {
                let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui;
                if !p.is_null() {
                    let ui = &mut *p;
                    if msg != WM_TIMER {
                        ui.input(msg, wp, lp);
                    }
                    if ui.draw(hwnd).is_err() {
                        ui.stop.store(true, Ordering::Release);
                        let _ = DestroyWindow(hwnd);
                    }
                    return LRESULT(0);
                }
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}
pub fn run(
    config: Config,
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    start: Instant,
) -> Result<(), String> {
    let scale = unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        config.notifications.scale * GetDpiForSystem().max(96) as f64 / 96.0
    };
    let (jobs_tx, jobs_rx) = mpsc::sync_channel::<(u64, crate::events::Event)>(1);
    let (results_tx, results_rx) = mpsc::sync_channel(1);
    let assets = config.clone();
    let done = stop.clone();
    let worker = std::thread::spawn(move || {
        while !done.load(Ordering::Acquire) {
            match jobs_rx.recv_timeout(std::time::Duration::from_millis(30)) {
                Ok((id, event)) => {
                    let art = crate::pet::for_event(&assets, &event)
                        .ok()
                        .flatten()
                        .and_then(|a| {
                            a.prepare(scale, assets.notifications.interpolation == "bilinear")
                                .ok()
                        });
                    let mut result = (id, art);
                    loop {
                        match results_tx.try_send(result) {
                            Ok(()) | Err(mpsc::TrySendError::Disconnected(_)) => break,
                            Err(mpsc::TrySendError::Full(value)) => {
                                result = value;
                                if done.load(Ordering::Acquire) {
                                    return;
                                }
                                std::thread::park_timeout(std::time::Duration::from_millis(2));
                            }
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
        }
    });
    let mut ui = Box::new(Ui {
        shared,
        stop: stop.clone(),
        config,
        scale,
        jobs: Some(jobs_tx),
        results: results_rx,
        surfaces: Vec::new(),
        durations: vec![1000],
        looping: false,
        selection: None,
        layout_count: 0,
        art: None,
        start,
        animation_start: 0,
        pending: false,
        audio: Vec::new(),
        sounded: std::collections::HashSet::new(),
        text_left: 22,
        width: 320,
        height: 110,
        position: POINT::default(),
    });
    let result = (|| unsafe {
        let module = GetModuleHandleW(None).map_err(|_| "cannot find module")?;
        let class = w!("aHerald.Resident");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(proc),
            hInstance: module.into(),
            lpszClassName: class,
            hCursor: LoadCursorW(None, IDC_ARROW).map_err(|_| "cannot load cursor")?,
            ..Default::default()
        };
        if RegisterClassW(&wc) == 0 {
            return Err("cannot register resident window".into());
        }
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class,
            w!("aHerald resident"),
            WS_POPUP,
            0,
            0,
            320,
            110,
            None,
            None,
            Some(module.into()),
            Some(&mut *ui as *mut Ui as *const std::ffi::c_void),
        )
        .map_err(|_| "cannot create resident window")?;
        if SetTimer(Some(hwnd), 1, 16, None) == 0 {
            let _ = DestroyWindow(hwnd);
            return Err("cannot start resident timer".into());
        }
        let mut msg = MSG::default();
        loop {
            let code = GetMessageW(&mut msg, None, 0, 0).0;
            if code <= 0 {
                if IsWindow(Some(hwnd)).as_bool() {
                    let _ = DestroyWindow(hwnd);
                }
                if code < 0 {
                    return Err("resident message loop failed".into());
                }
                break;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        Ok(())
    })();
    stop.store(true, Ordering::Release);
    ui.jobs.take();
    let _ = worker.join();
    for job in ui.audio.drain(..) {
        let _ = job.join();
    }
    result
}
