//! Single resident card. The GUI never waits for asset decoding or IPC.
use crate::reentry::{Deferred, Gate};
use crate::{
    animation::PreparedAnimation,
    config::Config,
    notification::overlay::{CardText, Composition, Surface, event_card_text},
    resident::Shared,
};
use std::{
    cell::{Cell, UnsafeCell},
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
/// How long a newly selected card waits for its artwork before showing the text card.
const ART_WAIT_MS: u64 = 300;
/// A payload-free wake: quota data stays in the service under its own mutex.
const USAGE_CHANGED: u32 = WM_APP + 2;
/// Event id, the render scale the artwork was prepared at, and the artwork.
type RenderResult = (u64, f64, Option<Arc<PreparedAnimation>>);
struct Ui {
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    config: Config,
    scale: f64,
    jobs: Option<mpsc::SyncSender<(u64, crate::events::Event, f64)>>,
    results: mpsc::Receiver<RenderResult>,
    composition: Option<Composition>,
    durations: Vec<u64>,
    looping: bool,
    selection: Option<u64>,
    layout_count: usize,
    art: Option<Arc<PreparedAnimation>>,
    start: Instant,
    animation_start: u64,
    pending: bool,
    /// Deadline (GUI clock, ms) until which a newly selected card waits for its artwork
    /// instead of being drawn as a text card first.
    art_wait: Option<u64>,
    /// The selected card still has to be composed (selection changed, artwork arrived).
    compose_due: bool,
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
    source_labels: crate::session_source::Service,
    /// Heading, queue position and age text the composed event card currently shows.
    shown_card: (crate::card::Heading, usize, usize, String),
    presenter: crate::presenter::Presenter,
    pacer: crate::perf::FramePacer,
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
        crate::operational_log::init(
            &self.config.diagnostic_directory,
            self.config.general.logging,
        );
        crate::operational_log::log(record);
    }
    /// Redraws everything at the DPI of the monitor the next card will appear on. Cards
    /// are drawn at that DPI rather than stretched, so a monitor change or a DPI change
    /// invalidates the composed card, its artwork and the usage card.
    fn refresh_scale(&mut self, dpi_scale: f64) {
        let wanted = self.config.notifications.scale * dpi_scale;
        if (wanted - self.scale).abs() > 1e-9 {
            self.scale = wanted;
            self.art = None;
            self.pending = true;
            self.composition = None;
            self.usage_surface = None;
            self.relayout();
        }
    }
    /// The window position is stale: place it again and present the next frame.
    fn relayout(&mut self) {
        self.placement = None;
        self.presenter.invalidate();
    }
    /// The usage card text, page or size changed: rebuild its surface and layout.
    fn invalidate_usage(&mut self) {
        self.usage_surface = None;
        self.relayout();
    }
    fn hide(&mut self, hwnd: HWND) {
        // SAFETY: hiding this thread's own window.
        unsafe {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }
        self.selection = None;
        self.presenter.invalidate();
    }
    /// A card that cannot be drawn must not end the resident: hide the window, keep
    /// the queue and retry from scratch on the next wake.
    fn draw(&mut self, hwnd: HWND) {
        if self.draw_now(hwnd).is_err() {
            self.placement = None;
            self.composition = None;
            self.hide(hwnd);
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
        let wanted = if self.demo.is_some()
            || counting
            || self.art_wait.is_some()
            || unsafe { IsWindowVisible(hwnd) }.as_bool()
        {
            Some(16)
        } else if self.shared.lock().is_ok_and(|s| s.warnings.has_deadline()) {
            Some(1000)
        } else if self.config.codex.usage.enabled || self.claude.is_some() {
            Some(15_000)
        } else {
            None
        };
        if wanted == self.timer_ms {
            return;
        }
        if wanted != Some(16) {
            // The animation burst is over; close its performance window now rather than
            // at the next card.
            crate::perf::flush();
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
            let mut started = 0;
            for (was, now) in self.usage_failing.iter_mut().zip(failing) {
                started += usize::from(now && !*was);
                *was = now;
            }
            for _ in 0..started {
                self.log(crate::operational_log::Record::UsageFailure);
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
                    self.invalidate_usage();
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
            crate::resident_request::tick(&mut *state, &self.config, now);
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
                self.invalidate_usage();
                self.usage_page = 0;
            }
            self.selection = entry.as_ref().map(|e| e.id);
            return self.draw_usage(hwnd, index, count);
        }
        let Some(entry) = entry.filter(|_| !paused) else {
            self.hide(hwnd);
            return Ok(());
        };
        self.audio.retain(|job| !job.is_finished());
        // Every new event sounds once, even behind a selected persistent card.
        let room = crate::presenter::sound_slots(self.audio.len());
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
            self.hide(hwnd);
            return Ok(());
        }
        let changed = self.selection != Some(entry.id);
        if changed || self.placement.is_none() {
            self.refresh_scale(crate::placement::dpi_scale(&self.config.notifications));
        }
        if changed {
            self.selection = Some(entry.id);
            self.art = None;
            self.pending = true;
            self.animation_start = now;
            self.compose_due = true;
            // Artwork usually takes 0-130 ms; a stalled or missing worker must not hide
            // the notification, so the text card follows after the limit.
            self.art_wait = self.jobs.as_ref().map(|_| now + ART_WAIT_MS);
        }
        if self.pending
            && self.jobs.as_ref().is_some_and(|tx| {
                tx.try_send((entry.id, entry.event.clone(), self.scale))
                    .is_ok()
            })
        {
            self.pending = false;
        }
        let mut loaded = false;
        while let Ok((id, scale, art)) = self.results.try_recv() {
            if Some(id) != self.selection {
                continue;
            }
            if scale == self.scale {
                self.art = art;
                loaded = true;
            } else {
                // Prepared for a monitor the card has since left: ask again.
                self.pending = true;
            }
        }
        if loaded {
            self.compose_due = true;
            self.art_wait = None;
        }
        if self.art_wait.is_some_and(|deadline| now >= deadline) {
            self.art_wait = None;
        } else if self.art_wait.is_some() {
            // Keep the previous picture (if any); the 16 ms timer polls for the artwork.
            return Ok(());
        }
        let heading = self.source_labels.label(&entry.event);
        let age = crate::card::age_text(self.now().saturating_sub(entry.arrived_ms));
        let controls = crate::card::Card::new(
            heading,
            index,
            count,
            self.config.notifications.click_to_dismiss,
        );
        // The card keeps one size whatever the queue holds, so a change of heading, queue
        // position or age redraws only the text.
        let shown = (controls.heading.clone(), index, count, age);
        let text_changed = self.shown_card != shown;
        if !(self.compose_due || self.composition.is_none())
            && text_changed
            && let Some(composition) = self.composition.as_mut()
            && composition
                .update_card(|w, h, left| {
                    Surface::card(
                        w,
                        h,
                        self.scale,
                        left,
                        event_card_text(&self.config, &entry.event, &controls, Some(&shown.3)),
                    )
                })
                .is_ok()
        {
            self.shown_card = shown;
            self.layout_count = count;
            self.presenter.invalidate();
        } else if self.compose_due || self.composition.is_none() || text_changed {
            self.compose_due = false;
            self.layout_count = count;
            self.relayout();
            let scale = self.scale;
            let config = &self.config;
            let composed = Composition::compose(scale, self.art.clone(), |w, h, left| {
                // Debug-build fault injection: artwork-sized cards fail to build.
                if fault(&config.diagnostic_directory, "compose") && left != (22.0 * scale) as i32 {
                    return Err("injected composition failure".into());
                }
                Surface::card(
                    w,
                    h,
                    scale,
                    left,
                    event_card_text(config, &entry.event, &controls, Some(&shown.3)),
                )
            })?;
            self.shown_card = shown;
            (self.width, self.height) = composed.size();
            self.text_left = composed.text_left;
            if composed.degraded {
                self.log(crate::operational_log::Record::RenderFallback);
            }
            self.durations = composed.durations.clone();
            self.looping = composed.looping;
            self.composition = Some(composed);
            // The counter is drawn on the card; the title repeats it for assistive tools.
            let position = if controls.count > 1 {
                format!(" [{}/{}]", controls.index + 1, controls.count)
            } else {
                String::new()
            };
            let title: Vec<u16> = format!(
                "{} - {}{position}",
                controls.heading.window_text(),
                entry.event.title()
            )
            .encode_utf16()
            .chain(Some(0))
            .collect();
            // The title is informational; a failure must not stop the card.
            unsafe {
                let _ = SetWindowTextW(hwnd, PCWSTR(title.as_ptr()));
                // Debug-build fault injection: a message sent while this draw runs.
                if fault(&self.config.diagnostic_directory, "reenter") {
                    SendMessageW(hwnd, WM_DISPLAYCHANGE, None, None);
                }
            }
        }
        let Some(shown) = self.presenter.next(&crate::presenter::Input {
            entry: &entry,
            durations: &self.durations,
            looping: self.looping,
            animation_ms: now.saturating_sub(self.animation_start),
            fade_in_ms: self.config.notifications.fade_in_ms,
            fade_out_ms: self.config.notifications.fade_out_ms,
        }) else {
            return Ok(());
        };
        let (frame, alpha) = (shown.frame, shown.alpha);
        let placement = match self.placement {
            Some(p) => p,
            None => {
                if fault(&self.config.diagnostic_directory, "placement") {
                    return Err("injected placement failure".into());
                }
                let p = crate::placement::current(
                    &self.config.notifications,
                    (self.width, self.height),
                    None,
                )?;
                self.placement = Some(p);
                p
            }
        };
        let composition = self.composition.as_mut().ok_or("missing composed card")?;
        crate::perf::count(crate::perf::Counter::FramePresented);
        crate::perf::add(
            crate::perf::Counter::FramesSkipped,
            self.pacer
                .present(entry.id, frame, composition.frame_count()),
        );
        let surface = composition.render(frame, shown.seconds, self.scale)?;
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
        let controls = crate::card::Card::new(
            crate::card::Heading::from_summary(heading),
            index,
            count,
            true,
        );
        if self.layout_count != count {
            self.layout_count = count;
            self.invalidate_usage();
        }
        if self.usage_surface.is_none() || self.placement.is_none() {
            let dpi = self.tray.as_ref().map_or(1.0, |t| t.card_dpi_scale());
            self.refresh_scale(dpi);
        }
        // Measuring needs a DC, DIB and font, so layout is cached with the surface.
        if self.usage_surface.is_none() {
            self.width = (760.0 * self.scale) as i32;
            self.height = Surface::body_height(
                self.width - (40.0 * self.scale) as i32,
                self.scale,
                &pages[self.usage_page],
            )? + (114.0 * self.scale) as i32;
            self.usage_surface = Some(Surface::card(
                self.width,
                self.height,
                self.scale,
                (22.0 * self.scale) as i32,
                CardText {
                    heading: &controls.heading,
                    label: &pages[self.usage_page],
                    body: true,
                    index,
                    count,
                    dismiss_button: false,
                    age: None,
                    color: crate::config::Color([80, 165, 250]),
                    background_color: self.config.usage_background_color(),
                    border: self.config.usage_border_color(),
                },
            )?);
        }
        if self.presenter.next_static().is_none() {
            return Ok(());
        }
        // Asking Explorer for the icon rectangle is an RPC: do it once per layout.
        let placement = match self.placement {
            Some(p) => p,
            None => {
                let p = self
                    .tray
                    .as_ref()
                    .ok_or("tray unavailable")?
                    .card_placement((self.width, self.height))?;
                self.placement = Some(p);
                p
            }
        };
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
            let mut controls = crate::card::Card::new(
                "",
                state.queue.selected_index(),
                state.queue.items().len(),
                self.usage_window || self.config.notifications.click_to_dismiss,
            );
            controls.text_left = Some(self.text_left);
            if let Some(entry) = state.queue.selected() {
                self.activation.submit(
                    &entry.event,
                    self.config.notifications.activate_agent_on_click,
                    controls.body_hit_test(self.width, self.height, self.scale, x, y),
                    self.config_path
                        .parent()
                        .unwrap_or_else(|| std::path::Path::new(".")),
                    self.config.general.logging,
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
                    self.invalidate_usage();
                    self.selection = None;
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
                self.invalidate_usage();
                self.selection = None;
            }
            ABOUT => {
                let about = crate::about_card_text();
                if let Ok(mut state) = self.shared.lock() {
                    // The same item closes its open card, like a usage line.
                    if state.queue.selected().is_some_and(|e| e.is_usage())
                        && self.notice.as_deref() == Some(about.as_str())
                    {
                        state.queue.dismiss();
                        self.notice = None;
                    } else {
                        self.notice = Some(about);
                        state.queue.show_usage();
                    }
                }
                self.usage_text = self.card_text();
                self.usage_page = 0;
                self.invalidate_usage();
                self.selection = None;
            }
            EXIT => self.stop.store(true, Ordering::Release),
            _ => {}
        }
    }
}
/// Everything behind the window's `GWLP_USERDATA`.
///
/// `ShowWindow` and `SetWindowTextW` send messages to this window synchronously, so the
/// procedure is entered again while `Ui::draw` runs. `Ui` therefore sits behind the
/// `gate`: a handler borrows it only when no other handler is active, and a message that
/// arrives meanwhile is kept and handled once the active one is done. The wake target
/// has its own lock so destruction can detach it even during reentry.
struct Window {
    usage_target: Arc<Mutex<Option<isize>>>,
    gate: Gate,
    taskbar_created: Cell<u32>,
    ui: UnsafeCell<Ui>,
}
impl Window {
    /// Runs `f` on the `Ui`, or keeps `message` for later when a handler is active.
    ///
    /// # Safety
    /// GUI thread only; the gate guarantees that at most one `&mut Ui` is alive.
    unsafe fn with_ui<R>(&self, message: Deferred, f: impl FnOnce(&mut Ui) -> R) -> Option<R> {
        if !self.gate.enter() {
            self.gate.defer(message);
            return None;
        }
        // SAFETY: the gate is held, so no other reference to the Ui exists.
        let result = f(unsafe { &mut *self.ui.get() });
        self.gate.leave();
        Some(result)
    }
}
/// Handles a tray icon notification. The popup menu runs a nested message loop, so the
/// `Ui` is borrowed before and after it, never across it.
///
/// # Safety
/// GUI thread only.
unsafe fn tray_callback(w: &Window, hwnd: HWND, message: Deferred) {
    use windows::Win32::UI::Shell::{NIN_SELECT, NINF_KEY};
    let event = (message.lparam & 0xffff) as u32;
    if ![
        WM_CONTEXTMENU,
        WM_RBUTTONUP,
        WM_LBUTTONUP,
        NIN_SELECT,
        NIN_SELECT | NINF_KEY,
    ]
    .contains(&event)
    {
        return;
    }
    // SAFETY: GUI thread, per this function's contract.
    unsafe {
        let Some(menu) = w.with_ui(message, |ui| {
            let (codex, claude) = ui.menu_lines();
            crate::tray::Menu::new(
                ui.config.sounds.enabled,
                ui.config.notifications.enabled,
                ui.shared.lock().ok().is_some_and(|s| s.paused),
                codex.as_deref(),
                claude.as_deref(),
            )
        }) else {
            return;
        };
        if let Ok(menu) = menu {
            let command = menu.select(hwnd);
            let chosen = Deferred {
                msg: WM_COMMAND,
                wparam: command,
                lparam: 0,
            };
            w.with_ui(chosen, |ui| {
                ui.command(hwnd, command);
                if let Some(tray) = &ui.tray {
                    tray.focus();
                }
                ui.draw(hwnd);
            });
        }
    }
}
/// Returns false for a message the window leaves to `DefWindowProcW`.
///
/// # Safety
/// GUI thread only.
unsafe fn handle(w: &Window, hwnd: HWND, message: Deferred) -> bool {
    let (msg, wp, lp) = (message.msg, WPARAM(message.wparam), LPARAM(message.lparam));
    // SAFETY: GUI thread, per this function's contract.
    unsafe {
        if msg != 0 && msg == w.taskbar_created.get() {
            w.with_ui(message, |ui| {
                if let Some(tray) = &mut ui.tray {
                    tray.restore();
                }
                // The icon moved with the taskbar: a cached placement is stale.
                ui.relayout();
            });
            return true;
        }
        match msg {
            crate::tray::CALLBACK => tray_callback(w, hwnd, message),
            WM_MOUSEWHEEL => {
                // Only the usage card scrolls; otherwise the default procedure decides.
                return w
                    .with_ui(message, |ui| {
                        if !ui.usage_window {
                            return false;
                        }
                        let body = ui
                            .usage_text
                            .split_once('\n')
                            .map(|(_, body)| body)
                            .unwrap_or("");
                        let pages = crate::usage_service::card_pages(body).len();
                        let delta = ((wp.0 >> 16) as u16) as i16;
                        if delta != 0 {
                            ui.usage_page = (ui.usage_page as isize - delta.signum() as isize)
                                .rem_euclid(pages as isize)
                                as usize;
                        }
                        ui.invalidate_usage();
                        ui.draw(hwnd);
                        true
                    })
                    .unwrap_or(true);
            }
            WM_COMMAND => {
                w.with_ui(message, |ui| {
                    ui.command(hwnd, wp.0 & 0xffff);
                    ui.draw(hwnd);
                });
            }
            WM_CLOSE => {
                if w.with_ui(message, |ui| ui.tray.take()).is_some() {
                    let _ = DestroyWindow(hwnd);
                }
            }
            WM_TIMER
            | crate::resident::WAKE
            | USAGE_CHANGED
            | WM_LBUTTONUP
            | WM_KEYDOWN
            | WM_DISPLAYCHANGE
            | WM_DPICHANGED
            | WM_SETTINGCHANGE => {
                w.with_ui(message, |ui| {
                    if msg == USAGE_CHANGED {
                        ui.usage_next = 0;
                    }
                    if matches!(msg, WM_DISPLAYCHANGE | WM_DPICHANGED | WM_SETTINGCHANGE) {
                        ui.relayout();
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
                });
            }
            _ => return false,
        }
    }
    true
}
unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    // SAFETY: the Window is boxed for the window's lifetime and used only by its GUI thread.
    unsafe {
        if msg == WM_NCCREATE {
            let create = &*(lp.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        if let Some(w) = (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Window).as_ref() {
            if msg == WM_NCDESTROY {
                // Serialize detachment with posting, before Windows can reuse the HWND.
                if let Ok(mut target) = w.usage_target.lock() {
                    *target = None;
                }
            }
            // Anything over 50 ms here is a UI stall, reported with the message class.
            let _timer = crate::perf::span(match msg {
                WM_TIMER => crate::perf::Span::GuiTimer,
                crate::resident::WAKE | USAGE_CHANGED => crate::perf::Span::GuiWake,
                WM_LBUTTONDOWN | WM_LBUTTONUP | WM_MOUSEMOVE | WM_KEYDOWN => {
                    crate::perf::Span::GuiInput
                }
                _ => crate::perf::Span::GuiOther,
            });
            let message = Deferred {
                msg,
                wparam: wp.0,
                lparam: lp.0,
            };
            if handle(w, hwnd, message) {
                // Run what arrived while a handler was active, once none is.
                while !w.gate.busy() {
                    let Some(next) = w.gate.next() else { break };
                    handle(w, hwnd, next);
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
            config.claude.usage.source,
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
    let (jobs_tx, jobs_rx) = mpsc::sync_channel::<(u64, crate::events::Event, f64)>(1);
    let (results_tx, results_rx) = mpsc::sync_channel(1);
    let assets = config.clone();
    let done = stop.clone();
    let worker = std::thread::spawn(move || {
        let new_cache =
            || crate::resource_cache::AssetCache::new(16, crate::animation::MAX_BYTES as usize);
        let mut cache = new_cache();
        // Artwork is prepared for one render scale at a time; another monitor's DPI
        // starts a fresh cache rather than mixing scales.
        let mut cached_scale = 0.0;
        while !done.load(Ordering::Acquire) {
            match jobs_rx.recv_timeout(std::time::Duration::from_millis(30)) {
                Ok((id, event, scale)) => {
                    if scale != cached_scale {
                        cache = new_cache();
                        cached_scale = scale;
                    }
                    // Debug-build fault injection: artwork preparation stalls.
                    if fault(&assets.diagnostic_directory, "slowart") {
                        std::thread::sleep(std::time::Duration::from_millis(800));
                    }
                    let art =
                        cache.get_or_load((event.source.agent_key(), event.kind.name()), || {
                            crate::artwork::prepared_for_event(&assets, &event, scale)
                                .ok()
                                .flatten()
                        });
                    let mut result = (id, scale, art);
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
    // With Codex usage polling on, session names share its app-server child.
    let source_labels = if config.codex.usage.enabled {
        crate::session_source::Service::shared(usage.names())
    } else {
        crate::session_source::Service::codex()
    };
    let window = Box::new(Window {
        usage_target: Arc::new(Mutex::new(None)),
        gate: Gate::default(),
        taskbar_created: Cell::new(0),
        ui: UnsafeCell::new(Ui {
            shared,
            stop: stop.clone(),
            config,
            scale,
            jobs: Some(jobs_tx),
            results: results_rx,
            composition: None,
            durations: vec![1000],
            looping: false,
            selection: None,
            layout_count: 0,
            art: None,
            start,
            animation_start: 0,
            pending: false,
            art_wait: None,
            compose_due: false,
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
            source_labels,
            shown_card: Default::default(),
            presenter: crate::presenter::Presenter::default(),
            pacer: crate::perf::FramePacer::default(),
        }),
    });
    // A short-lived borrow per use: none may be alive while messages are dispatched,
    // because the window procedure borrows the Ui too.
    let ui = || unsafe { &mut *window.ui.get() };
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
            if ui().demo.is_some() {
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
            Some(&*window as *const Window as *const std::ffi::c_void),
        )
        .map_err(|_| "cannot create resident window")?;
        match crate::tray::Icon::new(hwnd) {
            Ok(tray) => {
                window.taskbar_created.set(tray.taskbar_created);
                ui().tray = Some(tray);
            }
            Err(error) => {
                let _ = DestroyWindow(hwnd);
                return Err(error);
            }
        }
        if let Ok(mut state) = ui().shared.lock() {
            state.window = hwnd.0 as isize;
        }
        if let Ok(mut target) = window.usage_target.lock() {
            *target = Some(hwnd.0 as isize);
        }
        let target = window.usage_target.clone();
        let notify = move || {
            if let Ok(target) = target.lock()
                && let Some(raw) = *target
            {
                // SAFETY: WM_NCDESTROY clears the target under this same lock;
                // no pointers cross threads and PostMessage does not wait for the GUI.
                let _ = PostMessageW(
                    Some(HWND(raw as *mut std::ffi::c_void)),
                    USAGE_CHANGED,
                    WPARAM(0),
                    LPARAM(0),
                );
            }
        };
        ui().usage.on_change(notify.clone());
        if let Some(claude) = &ui().claude {
            claude.on_change(notify);
        }
        // The first draw also arms or skips the timer; events queued before the window
        // existed are picked up here.
        let _ = PostMessageW(Some(hwnd), crate::resident::WAKE, WPARAM(0), LPARAM(0));
        let mut msg = MSG::default();
        loop {
            let code = GetMessageW(&mut msg, None, 0, 0).0;
            if code <= 0 {
                ui().tray.take();
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
    let ui = ui();
    ui.audio_cancel.store(true, Ordering::Release);
    ui.tray.take();
    ui.jobs.take();
    let _ = worker.join();
    for job in ui.audio.drain(..) {
        let _ = job.join();
    }
    result.and_then(|()| ui.error.take().map_or(Ok(()), Err))
}

#[cfg(test)]
mod warning_deadline_tests {
    use super::*;
    use crate::events::{Event, Kind, Source};
    use std::time::Duration;

    #[test]
    fn hidden_resident_delivers_idle_warning_from_an_empty_queue() {
        let directory =
            std::env::temp_dir().join(format!("aherald-warning-deadline-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let config = Config::parse(
            "[notifications]\nenabled=false\n[sounds]\nenabled=false\n\
             [claude.warnings]\nidle_minutes=1\n[codex.warnings]\nidle_minutes=1\n\
             [events.turn_complete]\nenabled=false\n[events.idle_warning]\nduration_ms=0",
            &directory,
        )
        .unwrap();
        let mut state = Shared {
            queue: crate::queue::Queue::new(64, 0),
            warnings: Default::default(),
            paused: false,
            sounds: false,
            visuals: false,
            window: 0,
        };
        // Same identity across agents must still represent independent sessions.
        let mut event = Event::demo("turn_complete").unwrap();
        event.session_id = Some("shared-id".into());
        for source in [Source::ClaudeHook, Source::CodexHook] {
            event.source = source;
            crate::resident_request::respond(
                crate::ipc::Request::Event {
                    event: event.clone(),
                },
                &mut state,
                &config,
                0,
            );
        }
        event.source = Source::ClaudeHook;
        event.activity = true;
        crate::resident_request::respond(
            crate::ipc::Request::Event { event },
            &mut state,
            &config,
            1,
        );
        assert!(state.queue.selected().is_none());
        let shared = Arc::new(Mutex::new(state));
        let stop = Arc::new(AtomicBool::new(false));
        let state = shared.clone();
        let done = stop.clone();
        let path = directory.join("config.toml");
        // Seed the native clock near the deadline; retain the real Win32 timer.
        let start = Instant::now() - Duration::from_secs(59);
        let service = crate::usage_service::Service::codex(false, Duration::from_secs(300));
        let worker = std::thread::spawn(move || run(config, state, done, start, path, service));
        struct Cleanup {
            shared: Arc<Mutex<Shared>>,
            stop: Arc<AtomicBool>,
            worker: Option<std::thread::JoinHandle<Result<(), String>>>,
            directory: std::path::PathBuf,
        }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                self.stop.store(true, Ordering::Release);
                crate::resident::wake(self.shared.lock().unwrap().window);
                if let Some(worker) = self.worker.take() {
                    worker.join().unwrap().unwrap();
                }
                let _ = std::fs::remove_dir_all(&self.directory);
            }
        }
        let _cleanup = Cleanup {
            shared: shared.clone(),
            stop,
            worker: Some(worker),
            directory,
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let state = shared.lock().unwrap();
            if let Some(entry) = state.queue.selected() {
                assert_eq!(entry.event.kind, Kind::IdleWarning);
                assert_eq!(entry.event.source, Source::CodexHook);
                assert_eq!(entry.event.session_id.as_deref(), Some("shared-id"));
                assert!(!state.warnings.has_deadline());
                let hwnd = HWND(state.window as *mut std::ffi::c_void);
                // SAFETY: querying visibility does not mutate or own the window.
                assert!(!unsafe { IsWindowVisible(hwnd) }.as_bool());
                break;
            }
            drop(state);
            assert!(
                Instant::now() < deadline,
                "native timer did not deliver idle warning"
            );
            std::thread::yield_now();
        }
    }
}

#[cfg(test)]
mod usage_wake_tests {
    use super::*;
    use crate::{
        app_server::Error,
        usage::Snapshot,
        usage_service::{Service, UsageSource},
    };
    use std::time::Duration;

    #[test]
    fn hidden_resident_observes_usage_without_waiting_for_housekeeping() {
        struct Controlled(mpsc::Receiver<()>);
        impl UsageSource for Controlled {
            fn read(&mut self, _: Duration) -> Result<Snapshot, Error> {
                let _ = self.0.recv_timeout(Duration::from_secs(5));
                Err(Error::Unavailable)
            }
        }
        let directory =
            std::env::temp_dir().join(format!("aherald-usage-wake-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let log = directory.join("aherald.log");
        let _ = std::fs::remove_file(&log);
        let mut config = Config::default();
        config.notifications.enabled = false;
        config.sounds.enabled = false;
        config.codex.usage.enabled = true;
        config.general.logging = crate::config::LogLevel::Debug;
        config.diagnostic_directory = directory.clone();
        let shared = Arc::new(Mutex::new(Shared {
            queue: crate::queue::Queue::new(64, 0),
            warnings: Default::default(),
            paused: false,
            sounds: false,
            visuals: false,
            window: 0,
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let (release, input) = mpsc::sync_channel(1);
        let service = Service::spawn_source(
            crate::agent::Agent::Codex,
            true,
            Duration::from_secs(300),
            Arc::new(AtomicBool::new(false)),
            move || Controlled(input),
        );
        let state = shared.clone();
        let done = stop.clone();
        let path = directory.join("config.toml");
        let worker =
            std::thread::spawn(move || run(config, state, done, Instant::now(), path, service));
        struct Cleanup(
            Arc<Mutex<Shared>>,
            Arc<AtomicBool>,
            Option<std::thread::JoinHandle<Result<(), String>>>,
        );
        impl Drop for Cleanup {
            fn drop(&mut self) {
                self.1.store(true, Ordering::Release);
                crate::resident::wake(self.0.lock().unwrap().window);
                if let Some(worker) = self.2.take() {
                    worker.join().unwrap().unwrap();
                }
            }
        }
        let _cleanup = Cleanup(shared.clone(), stop, Some(worker));
        let deadline = Instant::now() + Duration::from_secs(3);
        let hwnd = loop {
            let window = shared.lock().unwrap().window;
            if window != 0 {
                break HWND(window as *mut std::ffi::c_void);
            }
            assert!(Instant::now() < deadline, "resident window did not start");
            std::thread::yield_now();
        };
        // A posted tray command establishes that earlier startup wakes were processed.
        unsafe {
            PostMessageW(
                Some(hwnd),
                WM_COMMAND,
                WPARAM(crate::tray::PAUSE),
                LPARAM(0),
            )
            .unwrap();
        }
        while !shared.lock().unwrap().paused {
            assert!(
                Instant::now() < deadline,
                "GUI startup command was not processed"
            );
            std::thread::yield_now();
        }
        // Force a fresh usage deadline. A wake must bypass the one-second throttle too.
        unsafe {
            SendMessageW(hwnd, WM_TIMER, Some(WPARAM(1)), Some(LPARAM(0)));
        }
        assert!(!unsafe { IsWindowVisible(hwnd).as_bool() });
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !std::fs::read_to_string(&log)
            .unwrap_or_default()
            .contains("usage.failed")
        {
            assert!(
                Instant::now() < deadline,
                "hidden GUI did not observe published usage before its 15-second timer"
            );
            std::thread::yield_now();
        }
        assert!(
            !unsafe { IsWindowVisible(hwnd).as_bool() },
            "usage update must not open a card"
        );
        drop(_cleanup);
        let _ = std::fs::remove_file(log);
        let _ = std::fs::remove_dir(directory);
    }
}
