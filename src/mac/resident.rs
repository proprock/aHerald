//! The macOS resident: IPC on a worker thread, one floating card on the main thread.
//!
//! This is the slice of the Windows resident (`resident.rs`, `resident_overlay.rs`) that the
//! macOS overlay needs: queue, selection, artwork, fades and countdown. Usage, tray, audio
//! and click handling arrive with their own tasks (AH-063..AH-065).
use super::{
    geometry::{self, Frame},
    input,
    menu::{self, Command},
    panel::{self, Panel},
    surface::{Composition, Surface},
    tray::Tray,
};
use crate::{
    animation::PreparedAnimation,
    card::Card,
    card_paint::{CardText, event_card_text},
    config::Config,
    events::Event,
    ipc::{Response, pipe},
    local::Scope,
    queue::Queue,
};
use objc2::{MainThreadMarker, rc::Retained};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSEvent, NSEventMask, NSEventType,
};
use objc2_foundation::{NSDate, NSDefaultRunLoopMode, NSPoint};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub use crate::resident_client::{ensure, forward, forward_if_running};

/// How long a newly selected card waits for its artwork before showing the text card.
const ART_WAIT_MS: u64 = 300;
/// Frame interval while a card is on screen.
const FRAME_MS: u64 = 16;

struct Shared {
    queue: Queue,
    paused: bool,
    sounds: bool,
    visuals: bool,
}

impl crate::resident_request::Controls for Shared {
    fn queue(&mut self) -> &mut Queue {
        &mut self.queue
    }
    fn paused(&self) -> bool {
        self.paused
    }
    fn sounds(&self) -> bool {
        self.sounds
    }
    fn visuals(&self) -> bool {
        self.visuals
    }
}

/// Wakes the AppKit event loop from another thread.
struct Waker(Mutex<Option<Retained<NSApplication>>>);

// SAFETY: the only use of the retained application off the main thread is posting an
// application-defined event, which AppKit documents as callable from any thread.
unsafe impl Send for Waker {}
unsafe impl Sync for Waker {}

impl Waker {
    fn set(&self, app: Retained<NSApplication>) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = Some(app);
        }
    }
    fn wake(&self) {
        let Ok(slot) = self.0.lock() else { return };
        let Some(app) = slot.as_ref() else { return };
        if let Some(event) =
            NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
                NSEventType::ApplicationDefined,
                NSPoint::new(0.0, 0.0),
                objc2_app_kit::NSEventModifierFlags::empty(),
                0.0,
                0,
                None,
                0,
                0,
                0,
            )
        {
            app.postEvent_atStart(&event, true);
        }
    }
}

pub fn run_demo(mut config: Config, path: &Path, pause_sec: u64) -> Result<(), String> {
    let demo = crate::demo::Demo::new(&config, pause_sec)?;
    // Demo cards remain clearly labelled while using the supported agent's artwork.
    if !config.agents.contains_key("demo")
        && let Some(assets) = config.agents.get("codex").cloned()
    {
        config.agents.insert("demo".into(), assets);
    }
    let shared = new_shared(&config);
    let stop = Arc::new(AtomicBool::new(false));
    present(
        config,
        shared,
        stop,
        Instant::now(),
        Arc::new(Waker(Mutex::new(None))),
        path,
        Some(demo),
    )
}

pub fn run(scope: Scope, config: Config, path: &Path) -> Result<(), String> {
    let Some(_owner) = scope.acquire()? else {
        return Ok(());
    };
    let stop = Arc::new(AtomicBool::new(false));
    let start = Instant::now();
    let shared = new_shared(&config);
    let waker = Arc::new(Waker(Mutex::new(None)));
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let stop_event = Arc::new(pipe::StopEvent::new()?);
    let listener = {
        let (state, shutdown, waker) = (shared.clone(), stop.clone(), waker.clone());
        let (listener_stop, settings) = (stop_event.clone(), config.clone());
        let log_directory = path.parent().unwrap_or_else(|| Path::new(".")).to_owned();
        std::thread::spawn(move || {
            use crate::operational_log::{Record, init, log};
            init(&log_directory, settings.general.logging);
            let server = match scope.create_pipe() {
                Ok(s) => s,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));
            log(Record::Started);
            while !shutdown.load(Ordering::Acquire) {
                let result = server.serve_until(&listener_stop, |request| {
                    let Ok(mut state) = state.lock() else {
                        return Response::Rejected;
                    };
                    let now = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
                    let (response, effects) =
                        crate::resident_request::respond(request, &mut *state, &settings, now);
                    drop(state);
                    if effects.quit {
                        shutdown.store(true, Ordering::Release);
                    }
                    if effects.wake {
                        waker.wake();
                    }
                    if effects.rejected {
                        log(Record::QueueRejected);
                    }
                    response
                });
                if result.is_err() {
                    log(Record::IpcFailure);
                }
            }
            log(Record::Stopped);
        })
    };
    let startup = ready_rx
        .recv_timeout(Duration::from_secs(1))
        .unwrap_or_else(|_| Err("resident listener startup deadline exceeded".into()));
    if let Err(error) = startup {
        stop.store(true, Ordering::Release);
        stop_event.signal();
        let _ = listener.join();
        return Err(error);
    }
    let result = present(config, shared, stop.clone(), start, waker, path, None);
    stop.store(true, Ordering::Release);
    stop_event.signal();
    let _ = listener.join();
    result
}

fn new_shared(config: &Config) -> Arc<Mutex<Shared>> {
    Arc::new(Mutex::new(Shared {
        queue: Queue::new(64, config.general.deduplicate_ms),
        paused: false,
        sounds: config.sounds.enabled,
        visuals: config.notifications.enabled,
    }))
}

/// Event id, the render scale the artwork was prepared at, and the artwork.
type RenderResult = (u64, f64, Option<Arc<PreparedAnimation>>);

struct Ui {
    shared: Arc<Mutex<Shared>>,
    config: Config,
    start: Instant,
    panel: Panel,
    mtm: MainThreadMarker,
    jobs: Option<mpsc::SyncSender<(u64, Event, f64)>>,
    results: mpsc::Receiver<RenderResult>,
    scale: f64,
    composition: Option<Composition>,
    selection: Option<u64>,
    art: Option<Arc<PreparedAnimation>>,
    pending: bool,
    art_wait: Option<u64>,
    compose_due: bool,
    animation_start: u64,
    frame: Option<Frame>,
    screen_watch: geometry::ScreenWatch,
    shown_card: (crate::card::Heading, usize, usize, String),
    presenter: crate::presenter::Presenter,
    source_labels: crate::session_source::Service,
    demo: Option<crate::demo::Demo>,
    tray: Tray,
    usage: crate::usage_service::Service,
    claude: Option<crate::usage_service::Service>,
    /// Which agent the usage card describes: Claude when true, otherwise Codex.
    usage_claude: bool,
    usage_next: u64,
    /// What the menu was last built from, so it is rebuilt only when that changes.
    menu_key: Option<(bool, bool, bool, String)>,
    tooltip: String,
    stop: Arc<AtomicBool>,
    waker: Arc<Waker>,
    config_path: std::path::PathBuf,
    /// Replaces the usage card text, e.g. the about text or a refused Restart; cleared when
    /// the card closes.
    notice: Option<String>,
    usage_text: String,
    usage_text_next: u64,
    usage_page: usize,
    usage_window: bool,
    usage_surface: Option<Surface>,
    layout_count: usize,
    audio: Vec<std::thread::JoinHandle<()>>,
    audio_cancel: Arc<AtomicBool>,
    activation: crate::activation::Worker,
}

impl Ui {
    fn now(&self) -> u64 {
        self.start.elapsed().as_millis().min(u64::MAX as u128) as u64
    }

    fn hide(&mut self) {
        self.panel.hide();
        self.selection = None;
        self.presenter.invalidate();
    }

    /// Keeps the status item's icon, tooltip and menu in step with usage and the toggles.
    fn refresh_tray(&mut self) {
        let now = self.now();
        let (sound, visual, paused) = self
            .shared
            .lock()
            .map(|s| (s.sounds, s.visuals, s.paused))
            .unwrap_or((false, false, false));
        let display = self.config.codex.usage.display;
        let codex = self.codex_shown().then(|| self.usage.compact(display));
        let claude = self.claude.as_ref().map(|c| c.compact(display));
        // Alphabetical by agent name, matching the menu.
        let tooltip: Vec<&str> = [claude.as_deref(), codex.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        let tooltip = tooltip.join("\n");
        let key = (
            sound,
            visual,
            paused,
            format!(
                "{}\n{}",
                codex.as_deref().unwrap_or(""),
                claude.as_deref().unwrap_or("")
            ),
        );
        if self.menu_key.as_ref() != Some(&key) {
            self.tray.set_menu(&menu::items(
                sound,
                visual,
                paused,
                codex.as_deref(),
                claude.as_deref(),
            ));
            self.menu_key = Some(key);
        }
        if now >= self.usage_next {
            self.usage_next = now.saturating_add(1000);
            if tooltip != self.tooltip {
                self.tray.set_tooltip(&tooltip);
                self.tooltip = tooltip;
            }
            let levels = crate::tray_usage::Levels::from_views(
                &self.usage.view(),
                self.claude.as_ref().map(|c| c.view()).as_ref(),
                self.config.tray.graph,
                display,
            );
            if let Err(error) = self.tray.set_levels(levels) {
                eprintln!("aHerald: {error}");
            }
        }
    }

    /// Codex is listed when its usage is on, or when it is the only agent there is.
    fn codex_shown(&self) -> bool {
        self.config.codex.usage.enabled || self.claude.is_none()
    }

    /// Asks the agent whose details are open for fresh data.
    fn refresh_selected(&self) {
        match &self.claude {
            Some(claude) if self.usage_claude => claude.refresh(),
            _ => self.usage.refresh(),
        }
    }

    /// Opens the usage card with `text` instead of the quota details.
    fn show_notice(&mut self, text: String) {
        self.notice = Some(text);
        if let Ok(mut state) = self.shared.lock() {
            state.queue.show_usage();
        }
        self.reset_usage_card();
    }

    /// The usage card's text or page changed: rebuild it from the first page.
    fn reset_usage_card(&mut self) {
        self.usage_text = self.usage_card_text();
        self.usage_page = 0;
        self.invalidate_usage();
        self.selection = None;
    }

    fn usage_card_text(&self) -> String {
        self.notice.clone().unwrap_or_else(|| match &self.claude {
            Some(claude) if self.usage_claude => claude.card_text(),
            _ => self.usage.card_text(),
        })
    }

    /// The usage card's text, page, size or place changed: redraw it from scratch.
    fn invalidate_usage(&mut self) {
        self.usage_surface = None;
        self.frame = None;
        self.presenter.invalidate();
    }

    /// Pages through a long usage card with the scroll wheel.
    fn handle_scrolls(&mut self) {
        let scrolls = self.panel.take_scrolls();
        if !self.usage_window {
            return;
        }
        let body = self.usage_text.split_once('\n').map_or("", |(_, b)| b);
        let last = crate::usage_service::card_pages(body).len() - 1;
        for delta in scrolls.into_iter().filter(|d| *d != 0.0) {
            let page = if delta < 0.0 {
                (self.usage_page + 1).min(last)
            } else {
                self.usage_page.saturating_sub(1)
            };
            if page != self.usage_page {
                self.usage_page = page;
                self.invalidate_usage();
            }
        }
    }

    /// Runs what the menu chose. Returns whether the loop must stop.
    fn handle_commands(&mut self) {
        for command in self.tray.take_commands() {
            match command {
                Command::Sound | Command::Visual | Command::Pause => {
                    let now = self.now();
                    if let Ok(mut state) = self.shared.lock() {
                        let paused = state.paused;
                        state.queue.tick(now, paused);
                        match command {
                            Command::Sound => state.sounds = !state.sounds,
                            Command::Visual => state.visuals = !state.visuals,
                            _ => state.paused = !state.paused,
                        }
                        self.config.sounds.enabled = state.sounds;
                        self.config.notifications.enabled = state.visuals;
                        // Muting or pausing silences what is playing; the next sound
                        // starts with a fresh flag.
                        if !state.sounds || state.paused {
                            self.audio_cancel.store(true, Ordering::Release);
                        } else if self.audio_cancel.load(Ordering::Acquire) {
                            self.audio_cancel = Arc::new(AtomicBool::new(false));
                        }
                    }
                }
                Command::Config => {
                    let path = self.config_path.clone();
                    std::thread::spawn(move || {
                        if let Err(error) = crate::shell::edit_configuration(&path) {
                            eprintln!("aHerald: {error}");
                        }
                    });
                }
                Command::Restart => {
                    // A replacement that cannot load its configuration would exit silently
                    // and leave no resident: validate first and keep this one running.
                    if let Err(error) = Config::load(&self.config_path, true) {
                        crate::operational_log::log(
                            crate::operational_log::Record::InvalidConfiguration,
                        );
                        self.show_notice(format!(
                            "aHerald\nRestart cancelled; the running configuration stays active.\n{error}"
                        ));
                        continue;
                    }
                    let (stop, waker) = (self.stop.clone(), self.waker.clone());
                    std::thread::spawn(move || match crate::restart::launch() {
                        Ok(()) => {
                            stop.store(true, Ordering::Release);
                            waker.wake();
                        }
                        Err(error) => eprintln!("aHerald: {error}"),
                    });
                }
                Command::Exit => {
                    self.stop.store(true, Ordering::Release);
                }
                Command::CodexSummary | Command::ClaudeSummary => {
                    let claude = command == Command::ClaudeSummary;
                    self.notice = None;
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
                    self.reset_usage_card();
                }
                Command::About => {
                    let about = crate::about_card_text();
                    // The same item closes its open card, like a usage line.
                    if self.usage_window && self.notice.as_deref() == Some(about.as_str()) {
                        if let Ok(mut state) = self.shared.lock() {
                            state.queue.dismiss();
                        }
                        self.notice = None;
                        self.reset_usage_card();
                    } else {
                        self.show_notice(about);
                    }
                }
            }
        }
    }

    /// The displays now. When the set changed since the last look, the card is placed again,
    /// whichever display it was on.
    fn current_screens(&mut self) -> Result<Vec<geometry::Screen>, String> {
        let screens = panel::screens(self.mtm);
        if screens.is_empty() {
            return Err("no display".into());
        }
        if self.screen_watch.changed(&screens) {
            self.frame = None;
            self.presenter.invalidate();
        }
        Ok(screens)
    }

    /// Applies the clicks the panel collected to the queue; the next step redraws.
    fn handle_clicks(&mut self) {
        let clicks = self.panel.take_clicks();
        if clicks.is_empty() {
            return;
        }
        let Some(frame) = self.frame else {
            return;
        };
        let (width, height, text_left) = if self.usage_window {
            let Some(surface) = &self.usage_surface else {
                return;
            };
            (surface.width, surface.height, (22.0 * self.scale) as i32)
        } else {
            let Some(composition) = &self.composition else {
                return;
            };
            let (width, height) = composition.size();
            (width, height, composition.text_left)
        };
        let layout = input::Layout {
            width,
            height,
            scale: self.scale,
            text_left,
        };
        let window = (frame.rect.width, frame.rect.height);
        let now = self.now();
        let Ok(mut state) = self.shared.lock() else {
            return;
        };
        let paused = state.paused;
        state.queue.tick(now, paused);
        for point in clicks {
            let pixels = input::to_pixels(point, window, &layout);
            // A click on the card's body raises the agent; the buttons keep their own meaning.
            if !self.usage_window
                && input::on_body(
                    &state.queue,
                    self.config.notifications.click_to_dismiss,
                    &layout,
                    pixels,
                )
                && let Some(entry) = state.queue.selected()
            {
                self.activation.submit(
                    &entry.event,
                    self.config.notifications.activate_agent_on_click,
                    true,
                    self.config_path.parent().unwrap_or_else(|| Path::new(".")),
                    self.config.general.logging,
                );
            }
            input::click(
                &mut state.queue,
                self.usage_window || self.config.notifications.click_to_dismiss,
                &layout,
                input::to_pixels(point, window, &layout),
            );
        }
    }

    /// Whether the next wake should come from the clock rather than from IPC.
    fn animating(&self) -> bool {
        self.demo.is_some()
            || self.art_wait.is_some()
            || self.panel.is_visible()
            || self
                .shared
                .lock()
                .is_ok_and(|s| !s.paused && s.queue.selected().is_some_and(|e| e.duration > 0))
    }

    /// Redraws one step. A card that cannot be drawn must not end the resident: hide it,
    /// keep the queue and retry from scratch on the next wake.
    fn step(&mut self) {
        self.refresh_tray();
        if self.draw().is_err() {
            self.frame = None;
            self.composition = None;
            self.hide();
            self.compose_due = true;
        }
    }

    fn draw_usage(&mut self, index: usize, count: usize) -> Result<(), String> {
        let (heading, body) = self
            .usage_text
            .split_once('\n')
            .unwrap_or((&self.usage_text, ""));
        let pages = crate::usage_service::card_pages(body);
        self.usage_page = self.usage_page.min(pages.len() - 1);
        let controls = Card::new(
            crate::card::Heading::from_summary(heading),
            index,
            count,
            true,
        );
        if self.layout_count != count {
            self.layout_count = count;
            self.invalidate_usage();
        }
        let screens = self.current_screens()?;
        let anchor = self.tray.frame();
        let centre = anchor.map_or((0.0, 0.0), |a| (a.x + a.width / 2.0, a.y + a.height / 2.0));
        let screen = geometry::choose(
            &screens,
            self.config.notifications.monitor,
            crate::config::Position::NearCursor,
            centre,
        );
        let wanted = self.config.notifications.scale * screens[screen].scale;
        if (wanted - self.scale).abs() > 1e-9 {
            self.scale = wanted;
            self.composition = None;
            self.art = None;
            self.pending = true;
            self.compose_due = true;
            self.invalidate_usage();
        }
        let scale = self.scale;
        if self.usage_surface.is_none() {
            let width = (760.0 * scale) as i32;
            let height = Surface::body_height(
                width - (40.0 * scale) as i32,
                scale,
                &pages[self.usage_page],
            )? + (114.0 * scale) as i32;
            self.usage_surface = Some(Surface::card(
                width,
                height,
                scale,
                (22.0 * scale) as i32,
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
        let surface = self
            .usage_surface
            .as_ref()
            .ok_or("usage surface unavailable")?;
        let size = (surface.width, surface.height);
        let frame = match self.frame {
            Some(frame) => frame,
            None => {
                let frame = match anchor {
                    Some(anchor) => geometry::place_below(&screens[screen], anchor, size),
                    None => geometry::place(
                        &screens[screen],
                        size,
                        crate::config::Position::TopCenter,
                        centre,
                    ),
                };
                self.frame = Some(frame);
                frame
            }
        };
        self.panel.show(surface, &frame, 255)
    }

    fn draw(&mut self) -> Result<(), String> {
        let now = self.now();
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
            self.usage_surface = None;
        }
        if self.usage_window {
            if now >= self.usage_text_next {
                self.usage_text_next = now.saturating_add(1000);
                let text = self.usage_card_text();
                if text != self.usage_text {
                    self.usage_text = text;
                    self.invalidate_usage();
                }
            }
            let id = entry.as_ref().map(|e| e.id);
            if self.selection != id {
                self.usage_page = 0;
                self.invalidate_usage();
            }
            self.selection = id;
            return self.draw_usage(index, count);
        }
        let Some(entry) = entry.filter(|_| !paused) else {
            self.hide();
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
            if let Some(job) = crate::audio::start_controlled(
                &fresh.event,
                &self.config,
                self.audio_cancel.clone(),
            ) {
                self.audio.push(job);
            }
        }
        if !self.config.notifications.enabled {
            self.hide();
            return Ok(());
        }
        let screens = self.current_screens()?;
        let cursor = panel::cursor();
        let changed = self.selection != Some(entry.id);
        let screen = geometry::choose(
            &screens,
            self.config.notifications.monitor,
            self.config.notifications.position,
            cursor,
        );
        if changed || self.frame.is_none() {
            // Cards are drawn at the backing scale of the display they appear on.
            let wanted = self.config.notifications.scale * screens[screen].scale;
            if (wanted - self.scale).abs() > 1e-9 {
                self.scale = wanted;
                self.art = None;
                self.pending = true;
                self.composition = None;
                self.compose_due = true;
            }
            self.frame = None;
            self.presenter.invalidate();
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
                // Prepared for a display the card has since left: ask again.
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
            return Ok(());
        }
        let heading = self.source_labels.label(&entry.event);
        let age = crate::card::age_text(now.saturating_sub(entry.arrived_ms));
        let controls = Card::new(
            heading,
            index,
            count,
            self.config.notifications.click_to_dismiss,
        );
        let shown = (controls.heading.clone(), index, count, age);
        let text_changed = self.shown_card != shown;
        let scale = self.scale;
        if !(self.compose_due || self.composition.is_none())
            && text_changed
            && let Some(composition) = self.composition.as_mut()
            && composition
                .update_card(|w, h, left| {
                    Surface::card(
                        w,
                        h,
                        scale,
                        left,
                        event_card_text(&self.config, &entry.event, &controls, Some(&shown.3)),
                    )
                })
                .is_ok()
        {
            self.shown_card = shown;
            self.presenter.invalidate();
        } else if self.compose_due || self.composition.is_none() || text_changed {
            self.compose_due = false;
            self.frame = None;
            self.presenter.invalidate();
            let config = &self.config;
            let composed = Composition::compose(scale, self.art.clone(), |w, h, left| {
                Surface::card(
                    w,
                    h,
                    scale,
                    left,
                    event_card_text(config, &entry.event, &controls, Some(&shown.3)),
                )
            })?;
            self.shown_card = shown;
            self.composition = Some(composed);
        }
        let composition = self.composition.as_mut().ok_or("missing composed card")?;
        let Some(shown) = self.presenter.next(&crate::presenter::Input {
            entry: &entry,
            durations: &composition.durations,
            looping: composition.looping,
            animation_ms: now.saturating_sub(self.animation_start),
            fade_in_ms: self.config.notifications.fade_in_ms,
            fade_out_ms: self.config.notifications.fade_out_ms,
        }) else {
            return Ok(());
        };
        let frame = match self.frame {
            Some(frame) => frame,
            None => {
                let frame = geometry::place(
                    &screens[screen],
                    composition.size(),
                    self.config.notifications.position,
                    cursor,
                );
                self.frame = Some(frame);
                frame
            }
        };
        let surface = composition.render(shown.frame, shown.seconds, scale)?;
        self.panel.show(surface, &frame, shown.alpha)
    }
}

/// Runs the AppKit loop on the calling thread, which must be the process's main thread.
fn present(
    config: Config,
    shared: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    start: Instant,
    waker: Arc<Waker>,
    config_path: &Path,
    demo: Option<crate::demo::Demo>,
) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("the macOS overlay must run on the main thread")?;
    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon and no menu bar: the resident is an accessory.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.finishLaunching();
    waker.set(app.clone());

    let (jobs_tx, jobs_rx) = mpsc::sync_channel::<(u64, Event, f64)>(1);
    let (results_tx, results_rx) = mpsc::sync_channel(1);
    let assets = config.clone();
    let done = stop.clone();
    let worker = std::thread::spawn(move || {
        let new_cache =
            || crate::resource_cache::AssetCache::new(16, crate::animation::MAX_BYTES as usize);
        let mut cache = new_cache();
        // Artwork is prepared for one render scale at a time.
        let mut cached_scale = 0.0;
        while !done.load(Ordering::Acquire) {
            match jobs_rx.recv_timeout(Duration::from_millis(30)) {
                Ok((id, event, scale)) => {
                    if scale != cached_scale {
                        cache = new_cache();
                        cached_scale = scale;
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
                                std::thread::park_timeout(Duration::from_millis(2));
                            }
                        }
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => break,
            }
        }
    });

    let usage_enabled = config.codex.usage.enabled;
    let usage = crate::usage_service::Service::codex(
        usage_enabled,
        Duration::from_secs(config.codex.usage.refresh_interval_sec),
    );
    // Claude quota polling is opt-in via `[claude.usage] enabled`; it reads the Keychain item
    // only while it is on (AH-066).
    let claude = config.claude.usage.enabled.then(|| {
        crate::usage_service::Service::claude(
            true,
            Duration::from_secs(config.claude.usage.refresh_interval_sec),
        )
    });
    let result = Panel::new(mtm).map(|panel| {
        let mut ui = Ui {
            shared,
            start,
            panel,
            mtm,
            jobs: Some(jobs_tx),
            results: results_rx,
            scale: config.notifications.scale,
            composition: None,
            selection: None,
            art: None,
            pending: false,
            art_wait: None,
            compose_due: false,
            animation_start: 0,
            frame: None,
            screen_watch: geometry::ScreenWatch::default(),
            shown_card: Default::default(),
            presenter: crate::presenter::Presenter::default(),
            source_labels: if usage_enabled {
                crate::session_source::Service::shared(usage.names())
            } else {
                crate::session_source::Service::codex()
            },
            demo,
            tray: Tray::new(mtm),
            usage,
            claude,
            usage_claude: false,
            usage_next: 0,
            menu_key: None,
            tooltip: String::new(),
            stop: stop.clone(),
            waker: waker.clone(),
            config_path: config_path.to_owned(),
            notice: None,
            usage_text: String::new(),
            usage_text_next: 0,
            usage_page: 0,
            usage_window: false,
            usage_surface: None,
            layout_count: 0,
            audio: Vec::new(),
            audio_cancel: Arc::new(AtomicBool::new(false)),
            activation: crate::activation::Worker::default(),
            config,
        };
        // Events queued before the loop started are picked up by the first step.
        while !stop.load(Ordering::Acquire) {
            ui.step();
            let wait = if ui.animating() {
                NSDate::dateWithTimeIntervalSinceNow(FRAME_MS as f64 / 1000.0)
            } else if ui.config.codex.usage.enabled {
                // Usage polling keeps the icon and tooltip fresh while nothing is shown.
                NSDate::dateWithTimeIntervalSinceNow(1.0)
            } else {
                NSDate::distantFuture()
            };
            // SAFETY: the default mode is a valid run-loop mode and the loop runs on the main thread.
            let event = unsafe {
                app.nextEventMatchingMask_untilDate_inMode_dequeue(
                    NSEventMask::Any,
                    Some(&wait),
                    NSDefaultRunLoopMode,
                    true,
                )
            };
            if let Some(event) = event {
                app.sendEvent(&event);
            }
            ui.handle_clicks();
            ui.handle_scrolls();
            ui.handle_commands();
        }
        ui.jobs.take();
        ui.audio_cancel.store(true, Ordering::Release);
        for job in ui.audio.drain(..) {
            let _ = job.join();
        }
    });
    stop.store(true, Ordering::Release);
    let _ = worker.join();
    result
}
