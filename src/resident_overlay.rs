//! Single resident card. The GUI never waits for asset decoding or IPC.
use crate::{
    animation::PreparedAnimation,
    config::Config,
    notification::overlay::{CardText, Composition, Surface},
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
/// Debug builds only: lets native tests inject a render failure by writing its name
/// to `aherald.fault` beside the configuration (isolated per resident instance).
#[cfg(debug_assertions)]
fn fault(directory: &std::path::Path, name: &str) -> bool {
    std::fs::read_to_string(directory.join("aherald.fault")).is_ok_and(|v| v.trim() == name)
}
#[cfg(not(debug_assertions))]
fn fault(_: &std::path::Path, _: &str) -> bool {
    false
}
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
    text_left: i32,
    width: i32,
    height: i32,
    display_size: (i32, i32),
    placement: Option<(i32, i32, i32, i32)>,
    tray: Option<crate::tray::Icon>,
    tray_retry: u64,
    config_path: std::path::PathBuf,
    usage: crate::usage_service::Service,
    claude: Option<crate::usage_service::Service>,
    usage_next: u64,
    usage_window: bool,
    activation: crate::activation::Worker,
    usage_text: String,
    usage_page: usize,
    /// Which agent the usage card describes: Claude when true, otherwise Codex.
    usage_claude: bool,
    usage_surface: Option<Surface>,
    audio_cancel: Arc<AtomicBool>,
    error: Option<String>,
    render_failed: bool,
    timer_ms: Option<u32>,
    tooltip: String,
    /// Replaces the usage card text, e.g. a refused Restart; cleared when the card closes.
    notice: Option<String>,
    usage_failing: [bool; 2],
    demo: Option<crate::demo::Demo>,
    countdown: Option<(usize, u64, Surface)>,
    source_labels: crate::session_source::Service,
    source_heading: String,
}
impl Ui {
    /// Codex usage is shown unless only Claude usage is enabled.
    fn codex_shown(&self) -> bool {
        self.config.codex.usage.enabled || self.claude.is_none()
    }
    fn compact(&self, separator: &str) -> String {
        let display = self.config.codex.usage.display;
        let mut parts = Vec::new();
        // Alphabetical by agent name, matching the tray menu.
        if let Some(claude) = &self.claude {
            parts.push(claude.compact(display));
        }
        if self.codex_shown() {
            parts.push(self.usage.compact(display));
        }
        parts.join(separator)
    }
    /// Tray menu lines; each agent's line opens only that agent's details.
    fn menu_lines(&self) -> (Option<String>, Option<String>) {
        let display = self.config.codex.usage.display;
        (
            self.codex_shown().then(|| self.usage.compact(display)),
            self.claude.as_ref().map(|c| c.compact(display)),
        )
    }
    /// Details of the agent chosen from the menu.
    fn card_text(&self) -> String {
        if let Some(notice) = &self.notice {
            return notice.clone();
        }
        match &self.claude {
            Some(claude) if self.usage_claude => claude.card_text(),
            _ => self.usage.card_text(),
        }
    }
    /// Details are opened on demand, so ask the chosen agent for fresh data.
    fn refresh_selected(&self) {
        match &self.claude {
            Some(claude) if self.usage_claude => claude.refresh(),
            _ => self.usage.refresh(),
        }
    }
    fn now(&self) -> u64 {
        self.start.elapsed().as_millis().min(u64::MAX as u128) as u64
    }
    /// Fixed operational codes only; never payloads or external error text.
    fn log(&self, record: crate::operational_log::Record) {
        let _ = crate::operational_log::Log::new(
            &self.config.diagnostic_directory,
            &self.config.general.logging,
        )
        .record(record);
    }
    /// A card that cannot be drawn must not end the resident: hide the window, keep
    /// the queue and retry from scratch on the next wake.
    fn draw(&mut self, hwnd: HWND) {
        if self.draw_now(hwnd).is_err() {
            self.selection = None;
            self.placement = None;
            self.surfaces.clear();
            // SAFETY: hiding this thread's own window.
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            if !std::mem::replace(&mut self.render_failed, true) {
                self.log(crate::operational_log::Record::RenderFailure);
            }
        } else {
            self.render_failed = false;
        }
        self.schedule(hwnd);
    }
    /// The 16 ms timer runs only while a card is visible or a timed card is counting
    /// down; otherwise IPC posts `WAKE`. Enabled usage polling keeps a slow housekeeping
    /// timer for the tray tooltip and icon.
    fn schedule(&mut self, hwnd: HWND) {
        let counting = self
            .shared
            .lock()
            .is_ok_and(|s| !s.paused && s.queue.selected().is_some_and(|e| e.duration > 0));
        let wanted =
            if self.demo.is_some() || counting || unsafe { IsWindowVisible(hwnd) }.as_bool() {
                Some(16)
            } else if self.config.codex.usage.enabled || self.claude.is_some() {
                Some(15_000)
            } else {
                None
            };
        if wanted == self.timer_ms {
            return;
        }
        // SAFETY: the timer belongs to this GUI thread's window.
        unsafe {
            match wanted {
                Some(ms) => {
                    let _ = SetTimer(Some(hwnd), 1, ms, None);
                }
                None => {
                    let _ = KillTimer(Some(hwnd), 1);
                }
            }
        }
        self.timer_ms = wanted;
    }
    fn draw_now(&mut self, hwnd: HWND) -> Result<(), String> {
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
            // One log line per failing episode, never per poll.
            let failing = [
                self.codex_shown() && self.usage.view().error.is_some(),
                self.claude
                    .as_ref()
                    .is_some_and(|c| c.view().error.is_some()),
            ];
            for (was, now) in self.usage_failing.iter_mut().zip(failing) {
                if now && !*was {
                    let _ = crate::operational_log::Log::new(
                        &self.config.diagnostic_directory,
                        &self.config.general.logging,
                    )
                    .record(crate::operational_log::Record::UsageFailure);
                }
                *was = now;
            }
            let tooltip = self.compact("\n");
            let levels = crate::tray_usage::Levels::from_views(
                &self.usage.view(),
                self.claude.as_ref().map(|c| c.view()).as_ref(),
                self.config.tray.graph,
                self.config.codex.usage.display,
            );
            if let Some(tray) = &mut self.tray {
                if tooltip != self.tooltip {
                    tray.tooltip(&tooltip);
                    self.tooltip = tooltip;
                }
                if let Err(error) = tray.usage(levels) {
                    eprintln!("aHerald: {error}");
                }
            }
            if self.usage_window {
                let text = self.card_text();
                if text != self.usage_text {
                    self.usage_text = text;
                    self.usage_surface = None;
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
        self.usage_window = entry.as_ref().is_some_and(|e| e.is_usage());
        if !self.usage_window {
            self.notice = None;
        }
        if self.usage_window {
            if self.selection != entry.as_ref().map(|e| e.id) {
                self.usage_surface = None;
                self.usage_page = 0;
            }
            self.selection = entry.as_ref().map(|e| e.id);
            return self.draw_usage(hwnd, index, count);
        }
        let Some(entry) = entry.filter(|_| !paused) else {
            unsafe {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            self.selection = None;
            return Ok(());
        };
        self.audio.retain(|job| !job.is_finished());
        // Every new event sounds once, even behind a selected persistent card.
        let room = 4usize.saturating_sub(self.audio.len());
        let fresh = self
            .shared
            .lock()
            .map(|mut state| state.queue.take_unannounced(room))
            .unwrap_or_default();
        for fresh in fresh {
            if let Some(audio) = crate::audio::start_controlled(
                &fresh.event,
                &self.config,
                self.audio_cancel.clone(),
            ) {
                self.audio.push(audio);
            }
        }
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
        let source = self.source_labels.label(&entry.event);
        let controls = crate::card::Card::new(
            &source,
            index,
            count,
            self.config.notifications.click_to_dismiss,
        );
        if changed
            || loaded
            || self.layout_count != count
            || self.source_heading != controls.heading
        {
            self.source_heading = controls.heading.clone();
            self.countdown = None;
            self.layout_count = count;
            self.placement = None;
            let source = &controls.heading;
            let scale = self.scale;
            let config = &self.config;
            let kind = entry.event.kind;
            let composed = Composition::compose(scale, self.art.as_deref(), |w, h, left| {
                // Debug-build fault injection: artwork-sized cards fail to build.
                if fault(&config.diagnostic_directory, "compose") && left != (22.0 * scale) as i32 {
                    return Err("injected composition failure".into());
                }
                Surface::card(
                    w,
                    h,
                    scale,
                    left,
                    CardText {
                        source,
                        label: kind.label(),
                        navigation: controls.navigation,
                        dismiss_button: controls.dismiss_button,
                        color: config.event_color(kind),
                        background_color: config.event_background_color(kind),
                    },
                )
            })?;
            self.width = composed.surfaces[0].width;
            self.height = composed.surfaces[0].height;
            self.text_left = composed.text_left;
            if composed.degraded {
                self.log(crate::operational_log::Record::RenderFallback);
            }
            self.durations = composed.durations;
            self.looping = composed.looping;
            self.surfaces = composed.surfaces;
            let title: Vec<u16> = format!("{} - {}", source, entry.event.kind.label(),)
                .encode_utf16()
                .chain(Some(0))
                .collect();
            // The title is informational; a failure must not stop the card.
            unsafe {
                let _ = SetWindowTextW(hwnd, PCWSTR(title.as_ptr()));
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
                if fault(&self.config.diagnostic_directory, "placement") {
                    return Err("injected placement failure".into());
                }
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
    fn draw_usage(&mut self, hwnd: HWND, index: usize, count: usize) -> Result<(), String> {
        let (heading, body) = self
            .usage_text
            .split_once('\n')
            .unwrap_or((&self.usage_text, ""));
        let pages = crate::usage_service::card_pages(body);
        self.usage_page = self.usage_page.min(pages.len() - 1);
        let controls = crate::card::Card::new(heading, index, count, true);
        if self.layout_count != count {
            self.layout_count = count;
            self.usage_surface = None;
        }
        self.width = (760.0 * self.scale) as i32;
        self.height = Surface::body_height(
            self.width - (40.0 * self.scale) as i32,
            self.scale,
            &pages[self.usage_page],
        )? + (94.0 * self.scale) as i32;
        if self.usage_surface.is_none() {
            self.usage_surface = Some(Surface::card(
                self.width,
                self.height,
                self.scale,
                (22.0 * self.scale) as i32,
                CardText {
                    source: &controls.heading,
                    label: &pages[self.usage_page],
                    navigation: controls.navigation,
                    dismiss_button: false,
                    color: crate::config::Color([80, 165, 250]),
                    background_color: self.config.usage_background_color(),
                },
            )?);
        }
        let placement = self
            .tray
            .as_ref()
            .ok_or("tray unavailable")?
            .card_placement((self.width, self.height))?;
        self.display_size = self
            .usage_surface
            .as_ref()
            .ok_or("usage surface unavailable")?
            .present_placed(hwnd, placement, 255)?;
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
                self.usage_window || self.config.notifications.click_to_dismiss,
            );
            if let Some(entry) = state.queue.selected() {
                self.activation.submit(
                    &entry.event,
                    self.config.notifications.activate_agent_on_click,
                    controls.body_hit_test(self.width, self.height, self.scale, x, y),
                    self.config_path
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new(".")),
                    &self.config.general.logging,
                );
            }
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
            CONFIG => {
                let path = self.config_path.clone();
                std::thread::spawn(move || {
                    if let Err(error) = crate::shell::edit_configuration(&path) {
                        eprintln!("aHerald: {error}");
                    }
                });
            }
            RESTART => {
                // A replacement that cannot load its configuration would exit silently
                // and leave no resident: validate first and keep this one running.
                if let Err(error) = Config::load(&self.config_path, true) {
                    self.log(crate::operational_log::Record::InvalidConfiguration);
                    self.notice = Some(format!(
                        "aHerald\nRestart cancelled; the running configuration stays active.\n{error}"
                    ));
                    if let Ok(mut state) = self.shared.lock() {
                        state.queue.show_usage();
                    }
                    self.usage_text = self.card_text();
                    self.usage_page = 0;
                    self.usage_surface = None;
                    self.selection = None;
                    self.placement = None;
                    return;
                }
                let stop = self.stop.clone();
                let window = hwnd.0 as isize;
                std::thread::spawn(move || match crate::restart::launch() {
                    Ok(()) => {
                        stop.store(true, Ordering::Release);
                        // No timer runs while idle: wake the window so it sees the stop.
                        crate::resident::wake(window);
                    }
                    Err(error) => eprintln!("aHerald: {error}"),
                });
            }
            SUMMARY | CLAUDE_SUMMARY => {
                self.notice = None;
                let claude = command == CLAUDE_SUMMARY;
                if let Ok(mut state) = self.shared.lock() {
                    // The same line closes its open card; the other line switches agents.
                    if state.queue.selected().is_some_and(|e| e.is_usage())
                        && self.usage_claude == claude
                    {
                        state.queue.dismiss();
                    } else {
                        state.queue.show_usage();
                        self.usage_claude = claude;
                        self.refresh_selected();
                    }
                }
                self.usage_page = 0;
                self.usage_text = self.card_text();
                self.usage_surface = None;
                self.selection = None;
                self.placement = None;
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
                        let (codex, claude) = ui.menu_lines();
                        crate::tray::Menu::new(
                            ui.config.sounds.enabled,
                            ui.config.notifications.enabled,
                            ui.shared.lock().ok().is_some_and(|s| s.paused),
                            codex.as_deref(),
                            claude.as_deref(),
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
                        ui.draw(hwnd);
                    }
                }
                return LRESULT(0);
            }
            if msg == WM_MOUSEWHEEL && (*p).usage_window {
                let ui = &mut *p;
                let body = ui
                    .usage_text
                    .split_once('\n')
                    .map(|(_, body)| body)
                    .unwrap_or("");
                let pages = crate::usage_service::card_pages(body).len();
                let delta = ((wp.0 >> 16) as u16) as i16;
                if delta != 0 {
                    ui.usage_page = (ui.usage_page as isize - delta.signum() as isize)
                        .rem_euclid(pages as isize) as usize;
                }
                ui.usage_surface = None;
                ui.draw(hwnd);
                return LRESULT(0);
            }
            if msg == WM_COMMAND {
                let ui = &mut *p;
                ui.command(hwnd, wp.0 & 0xffff);
                ui.draw(hwnd);
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
            WM_TIMER
            | crate::resident::WAKE
            | WM_LBUTTONUP
            | WM_KEYDOWN
            | WM_DISPLAYCHANGE
            | WM_DPICHANGED
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
                ui.draw(hwnd);
                return LRESULT(0);
            }
            _ => {}
        }
        DefWindowProcW(hwnd, msg, wp, lp)
    }
}
/// Claude quota polling is opt-in via `[claude.usage] enabled`.
fn claude_usage(config: &Config) -> Option<crate::usage_service::Service> {
    config.claude.usage.enabled.then(|| {
        crate::usage_service::Service::claude(
            true,
            std::time::Duration::from_secs(config.claude.usage.refresh_interval_sec),
        )
    })
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
    let claude = claude_usage(&config);
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
        text_left: 22,
        width: 320,
        height: 110,
        display_size: (320, 110),
        placement: None,
        tray: None,
        tray_retry: 0,
        config_path,
        claude,
        usage,
        usage_next: 0,
        usage_window: false,
        activation: crate::activation::Worker::default(),
        usage_text: String::new(),
        usage_page: 0,
        usage_claude: false,
        usage_surface: None,
        audio_cancel: Arc::new(AtomicBool::new(false)),
        error: None,
        render_failed: false,
        timer_ms: None,
        tooltip: String::new(),
        notice: None,
        usage_failing: [false; 2],
        demo,
        countdown: None,
        source_labels: crate::session_source::Service::codex(),
        source_heading: String::new(),
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
        // A second run in one process (in-process tests) finds the class registered.
        if RegisterClassW(&wc) == 0 && GetLastError() != ERROR_CLASS_ALREADY_EXISTS {
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
        if let Ok(mut state) = ui.shared.lock() {
            state.window = hwnd.0 as isize;
        }
        // The first draw also arms or skips the timer; events queued before the window
        // existed are picked up here.
        let _ = PostMessageW(Some(hwnd), crate::resident::WAKE, WPARAM(0), LPARAM(0));
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
