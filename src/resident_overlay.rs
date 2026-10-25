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
        System::LibraryLoader::GetModuleHandleW,
        UI::{HiDpi::*, WindowsAndMessaging::*},
    },
    core::{PCWSTR, w},
};
type RenderResult = (u64, Option<Arc<PreparedAnimation>>);
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
    art: Option<Arc<PreparedAnimation>>,
    start: Instant,
    animation_start: u64,
    pending: bool,
    audio: Vec<std::thread::JoinHandle<()>>,
    sounded: std::collections::HashSet<u64>,
    text_left: i32,
    width: i32,
    height: i32,
    display_size: (i32, i32),
    placement: Option<(i32, i32, i32, i32)>,
    tray: Option<crate::tray::Icon>,
    tray_retry: u64,
    config_path: std::path::PathBuf,
    information: HWND,
    usage: crate::usage_service::Service,
    usage_next: u64,
    usage_window: bool,
    usage_text: String,
    audio_cancel: Arc<AtomicBool>,
    error: Option<String>,
    demo: Option<crate::demo::Demo>,
    countdown: Option<(usize, u64, Surface)>,
}
impl Ui {
    fn now(&self) -> u64 {
        self.start.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    fn draw(&mut self, hwnd: HWND) -> Result<(), String> {
        if self.stop.load(Ordering::Acquire) {
            self.tray.take();
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            return Ok(());
        }
        let now = self.now();
        if now >= self.usage_next {
            self.usage_next = now.saturating_add(1000);
            if let Some(tray) = &mut self.tray {
                tray.tooltip(&self.usage.summary());
            }
            if self.usage_window {
                let text = self.usage.details();
                if text != self.usage_text {
                    crate::tray::update_information(self.information, &text);
                    self.usage_text = text;
                }
            }
        }
        let (entry, index, count, paused) = {
            let mut state = self
                .shared
                .lock()
                .map_err(|_| "resident state unavailable")?;
            let paused = state.paused;
            if let Some(demo) = &mut self.demo {
                demo.tick(now, paused, &mut state.queue);
            }
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
        self.audio.retain(|job| !job.is_finished());
        if self.sounded.insert(entry.id)
            && self.audio.len() < 4
            && let Some(audio) = crate::audio::start_controlled(
                &entry.event,
                &self.config,
                self.audio_cancel.clone(),
            )
        {
            self.audio.push(audio);
        }
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
        let controls = crate::card::Card::new(
            entry.event.source.label(),
            index,
            count,
            self.config.notifications.click_to_dismiss,
        );
        if changed || loaded || self.layout_count != count {
            self.countdown = None;
            self.layout_count = count;
            self.placement = None;
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
            let source = &controls.heading;
            let card = Surface::card(
                self.width,
                self.height,
                self.scale,
                self.text_left,
                CardText {
                    source,
                    label: entry.event.kind.label(),
                    navigation: controls.navigation,
                    dismiss_button: controls.dismiss_button,
                    color: self.config.event_color(entry.event.kind),
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
            let title: Vec<u16> = format!("{} - {}", source, entry.event.kind.label(),)
                .encode_utf16()
                .chain(Some(0))
                .collect();
            unsafe {
                SetWindowTextW(hwnd, PCWSTR(title.as_ptr()))
                    .map_err(|_| "cannot update event title")?;
            }
        }
        let frame = crate::animation::frame_index(
            &self.durations,
            self.looping,
            now.saturating_sub(self.animation_start),
        );
        let display_duration = entry.display_duration(self.config.notifications.fade_out_ms);
        let alpha = crate::notification::presentation(
            entry.visible_ms,
            display_duration,
            self.config.notifications.fade_in_ms,
            self.config.notifications.fade_out_ms,
            None,
        )
        .0;
        let placement = match self.placement {
            Some(p) => p,
            None => {
                let p = crate::placement::current(
                    &self.config.notifications,
                    (self.width, self.height),
                    self.surfaces.len(),
                )?;
                self.placement = Some(p);
                p
            }
        };
        let surface = if let Some(seconds) =
            crate::card::remaining_seconds(display_duration, entry.visible_ms)
        {
            if self
                .countdown
                .as_ref()
                .is_none_or(|(f, s, _)| *f != frame || *s != seconds)
            {
                self.countdown = Some((
                    frame,
                    seconds,
                    self.surfaces[frame].with_countdown(seconds, self.scale)?,
                ));
            }
            &self
                .countdown
                .as_ref()
                .ok_or("missing countdown surface")?
                .2
        } else {
            &self.surfaces[frame]
        };
        self.display_size = surface.present_placed(hwnd, placement, alpha)?;
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
            let x = (lp.0 as u16) as i16 as i32 * self.width / self.display_size.0.max(1);
            let y = ((lp.0 >> 16) as u16) as i16 as i32 * self.height / self.display_size.1.max(1);
            let controls = crate::card::Card::new(
                "",
                state.queue.selected_index(),
                state.queue.items().len(),
                self.config.notifications.click_to_dismiss,
            );
            match controls.hit_test(self.width, self.height, self.scale, x, y) {
                Some(crate::card::Action::Previous) => state.queue.next(-1),
                Some(crate::card::Action::Next) => state.queue.next(1),
                Some(crate::card::Action::Dismiss) => state.queue.dismiss(),
                None => {}
            }
        }
    }
    fn command(&mut self, hwnd: HWND, command: usize) {
        use crate::tray::*;
        match command {
            SOUND | VISUAL | PAUSE => {
                if let Ok(mut state) = self.shared.lock() {
                    let paused = state.paused;
                    state.queue.tick(self.now(), paused);
                    match command {
                        SOUND => state.sounds = !state.sounds,
                        VISUAL => state.visuals = !state.visuals,
                        PAUSE => state.paused = !state.paused,
                        _ => {}
                    }
                    self.config.sounds.enabled = state.sounds;
                    self.config.notifications.enabled = state.visuals;
                    if !state.sounds || state.paused {
                        self.audio_cancel.store(true, Ordering::Release);
                    } else if self.audio_cancel.load(Ordering::Acquire) {
                        self.audio_cancel = Arc::new(AtomicBool::new(false));
                    }
                }
            }
            REFRESH => self.usage.refresh(),
            SUMMARY | DETAILS | CONFIG => {
                unsafe {
                    if IsWindow(Some(self.information)).as_bool() {
                        let _ = DestroyWindow(self.information);
                    }
                }
                let message = if command == CONFIG {
                    format!(
                        "Configuration location:\r\n{}\r\n\r\nRuntime toggles are not saved. Restart to reload edited settings.",
                        self.config_path.display()
                    )
                } else {
                    self.usage_window = command == DETAILS;
                    if command == DETAILS {
                        self.usage.details()
                    } else {
                        self.usage.summary()
                    }
                };
                if command == CONFIG {
                    self.usage_window = false;
                }
                self.usage_text = message.clone();
                match information(
                    hwnd,
                    if command == CONFIG {
                        "aHerald configuration"
                    } else {
                        "aHerald usage"
                    },
                    &message,
                ) {
                    Ok(window) => self.information = window,
                    Err(error) => self.error = Some(error),
                }
            }
            EXIT => self.stop.store(true, Ordering::Release),
            _ => {}
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
        let p = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Ui;
        if !p.is_null() {
            if (*p)
                .tray
                .as_ref()
                .is_some_and(|tray| msg == tray.taskbar_created)
            {
                if let Some(tray) = &mut (*p).tray {
                    tray.restore();
                }
                return LRESULT(0);
            }
            if msg == crate::tray::CALLBACK {
                let event = (lp.0 & 0xffff) as u32;
                use windows::Win32::UI::Shell::{NIN_SELECT, NINF_KEY};
                if [
                    WM_CONTEXTMENU,
                    WM_RBUTTONUP,
                    WM_LBUTTONUP,
                    NIN_SELECT,
                    NIN_SELECT | NINF_KEY,
                ]
                .contains(&event)
                {
                    let menu = {
                        let ui = &*p;
                        crate::tray::Menu::new(
                            ui.config.sounds.enabled,
                            ui.config.notifications.enabled,
                            ui.shared.lock().ok().is_some_and(|s| s.paused),
                            &ui.usage.summary().replace("\r\n", "; "),
                        )
                    };
                    if let Ok(menu) = menu {
                        // TrackPopupMenu runs a nested message loop. Do not borrow Ui across it.
                        let command = menu.select(hwnd);
                        let ui = &mut *p;
                        ui.command(hwnd, command);
                        if let Some(tray) = &ui.tray {
                            tray.focus();
                        }
                        if let Err(error) = ui.draw(hwnd) {
                            ui.error = Some(error);
                            ui.stop.store(true, Ordering::Release);
                        }
                    }
                }
                return LRESULT(0);
            }
            if msg == WM_COMMAND {
                let ui = &mut *p;
                ui.command(hwnd, wp.0 & 0xffff);
                if let Err(error) = ui.draw(hwnd) {
                    ui.error = Some(error);
                    ui.stop.store(true, Ordering::Release);
                }
                return LRESULT(0);
            }
        }
        match msg {
            WM_MOUSEACTIVATE => return LRESULT(MA_NOACTIVATE as isize),
            WM_DESTROY => {
                PostQuitMessage(0);
                return LRESULT(0);
            }
            WM_CLOSE => {
                if !p.is_null() {
                    (*p).tray.take();
                }
                let _ = DestroyWindow(hwnd);
                return LRESULT(0);
            }
            WM_TIMER | WM_LBUTTONUP | WM_KEYDOWN | WM_DISPLAYCHANGE | WM_DPICHANGED
            | WM_SETTINGCHANGE
                if !p.is_null() =>
            {
                let ui = &mut *p;
                if matches!(msg, WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE) {
                    ui.placement = None;
                }
                if ui.now() >= ui.tray_retry {
                    ui.tray_retry = ui.now().saturating_add(5000);
                    if let Some(tray) = &mut ui.tray {
                        tray.restore();
                    }
                }
                if matches!(msg, WM_LBUTTONUP | WM_KEYDOWN) {
                    ui.input(msg, wp, lp);
                }
                if let Err(error) = ui.draw(hwnd) {
                    ui.error = Some(error);
                    ui.stop.store(true, Ordering::Release);
                    ui.tray.take();
                    let _ = DestroyWindow(hwnd);
                }
                return LRESULT(0);
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
    config_path: std::path::PathBuf,
    usage: crate::usage_service::Service,
) -> Result<(), String> {
    run_preview(config, shared, stop, start, config_path, usage, None)
}

pub(super) fn run_preview(
    config: Config,
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    start: Instant,
    config_path: std::path::PathBuf,
    usage: crate::usage_service::Service,
    demo: Option<crate::demo::Demo>,
) -> Result<(), String> {
    let scale = unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        config.notifications.scale
    };
    let (jobs_tx, jobs_rx) = mpsc::sync_channel::<(u64, crate::events::Event)>(1);
    let (results_tx, results_rx) = mpsc::sync_channel(1);
    let assets = config.clone();
    let done = stop.clone();
    let worker = std::thread::spawn(move || {
        let mut cache =
            crate::resource_cache::AssetCache::new(16, crate::animation::MAX_BYTES as usize);
        while !done.load(Ordering::Acquire) {
            match jobs_rx.recv_timeout(std::time::Duration::from_millis(30)) {
                Ok((id, event)) => {
                    let art =
                        cache.get_or_load((event.source.agent_key(), event.kind.name()), || {
                            crate::artwork::prepared_for_event(&assets, &event)
                                .ok()
                                .flatten()
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
        display_size: (320, 110),
        placement: None,
        tray: None,
        tray_retry: 0,
        config_path,
        information: HWND::default(),
        usage,
        usage_next: 0,
        usage_window: false,
        usage_text: String::new(),
        audio_cancel: Arc::new(AtomicBool::new(false)),
        error: None,
        demo,
        countdown: None,
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
            if ui.demo.is_some() {
                w!("aHerald demo")
            } else {
                w!("aHerald resident")
            },
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
        match crate::tray::Icon::new(hwnd) {
            Ok(tray) => ui.tray = Some(tray),
            Err(error) => {
                let _ = DestroyWindow(hwnd);
                return Err(error);
            }
        }
        if SetTimer(Some(hwnd), 1, 16, None) == 0 {
            ui.tray.take();
            let _ = DestroyWindow(hwnd);
            return Err("cannot start resident timer".into());
        }
        let mut msg = MSG::default();
        loop {
            let code = GetMessageW(&mut msg, None, 0, 0).0;
            if code <= 0 {
                ui.tray.take();
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
    ui.audio_cancel.store(true, Ordering::Release);
    ui.tray.take();
    ui.jobs.take();
    let _ = worker.join();
    for job in ui.audio.drain(..) {
        let _ = job.join();
    }
    result.and_then(|()| ui.error.take().map_or(Ok(()), Err))
}
