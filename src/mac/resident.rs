//! The macOS resident: IPC on a worker thread, one floating card on the main thread.
//!
//! This is the slice of the Windows resident (`resident.rs`, `resident_overlay.rs`) that the
//! macOS overlay needs: queue, selection, artwork, fades and countdown. Usage, tray, audio
//! and click handling arrive with their own tasks (AH-063..AH-065).
use super::{
    geometry::{self, Frame},
    input,
    panel::{self, Panel},
    surface::{Composition, Surface},
    tray::Tray,
};
use crate::{
    animation::PreparedAnimation,
    artwork_worker::ArtworkWorker,
    card::Card,
    card_paint::{CardText, event_card_text},
    config::Config,
    events::Event,
    local::Scope,
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
    },
    time::{Duration, Instant},
};

pub use crate::resident_client::{ensure, forward, forward_if_running};

/// How long a newly selected card waits for its artwork before showing the text card.
const ART_WAIT_MS: u64 = 300;
/// Frame interval while a card is on screen.
const FRAME_MS: u64 = 16;

/// What the status menu was last built from: the five switches, then the usage lines.
type MenuKey = ((bool, bool, bool, bool, bool), Option<bool>, String);

type Shared = crate::resident_request::Shared;

/// What the resident shows on its own, besides real events.
enum Preview {
    None,
    /// `--demo`: the looping configuration preview.
    Demo(crate::demo::Demo),
    /// Apply + Test: one card per event kind, with arrows (the Windows and Linux preview).
    Test(crate::demo::Carousel),
}

/// Wakes the AppKit event loop from another thread.
struct Waker(Mutex<Option<Retained<NSApplication>>>);

// SAFETY: the only use of the retained application off the main thread is posting an
// application-defined event, which AppKit documents as callable from any thread.
unsafe impl Send for Waker {}
// SAFETY: as for Send above; the mutex serializes access to the retained application.
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

pub fn run_demo(config: Config, path: &Path, pause_sec: u64) -> Result<(), String> {
    let demo = crate::demo::Demo::new(&config, pause_sec)?;
    let shared = new_shared(&config);
    let stop = Arc::new(AtomicBool::new(false));
    present(
        config,
        shared,
        stop,
        Instant::now(),
        Arc::new(Waker(Mutex::new(None))),
        path,
        Preview::Demo(demo),
    )
}

/// Shows one aHerald notice with this process's own AppKit loop and exits after it has expired.
/// It owns no resident scope and no IPC, so a running resident is unaffected.
pub fn run_notice(event: Event, config: Config) -> Result<(), String> {
    let shared = new_shared(&config);
    let stop = Arc::new(AtomicBool::new(false));
    let start = Instant::now();
    let waker = Arc::new(Waker(Mutex::new(None)));
    let wake = waker.clone();
    crate::resident_request::notice_sink(
        shared.clone(),
        config.clone(),
        start,
        move |_: &Shared| wake.wake(),
    )(event);
    let lifetime = Duration::from_millis(
        crate::resident_request::NOTICE_MS + config.notifications.fade_out_ms + 1000,
    );
    {
        let (stop, waker) = (stop.clone(), waker.clone());
        std::thread::spawn(move || {
            std::thread::sleep(lifetime);
            stop.store(true, Ordering::Release);
            waker.wake();
        });
    }
    present(
        config,
        shared,
        stop,
        start,
        waker,
        Path::new("."),
        Preview::None,
    )
}

pub fn run(scope: Scope, config: Config, path: &Path) -> Result<(), String> {
    run_after_apply(scope, config, path, false)
}

/// `test` is Apply + Test: the replacement resident shows each configured card once.
pub fn run_after_apply(
    scope: Scope,
    config: Config,
    path: &Path,
    test: bool,
) -> Result<(), String> {
    let preview = if test {
        Preview::Test(crate::demo::Carousel::new(&config))
    } else {
        Preview::None
    };
    let Some(_owner) = scope.acquire()? else {
        return Ok(());
    };
    let start = Instant::now();
    let shared = new_shared(&config);
    let waker = Arc::new(Waker(Mutex::new(None)));
    let runtime = crate::resident_runtime::start(scope, &config, path, &shared, start, {
        let waker = waker.clone();
        move |_: &Shared| waker.wake()
    })?;
    // Dropping `runtime` afterwards stops the listener and the sinks.
    present(
        config,
        shared,
        runtime.stop_flag(),
        start,
        waker,
        path,
        preview,
    )
}

fn new_shared(config: &Config) -> Arc<Mutex<Shared>> {
    Arc::new(Mutex::new(Shared::new(config, ())))
}

struct Ui {
    shared: Arc<Mutex<Shared>>,
    config: Config,
    start: Instant,
    panel: Panel,
    mtm: MainThreadMarker,
    artwork: ArtworkWorker,
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
    /// The missing monitor number and the display count it was last reported for.
    monitor_watch: crate::notice::MonitorWatch,
    shown_card: (crate::card::Heading, usize, usize, String),
    presenter: crate::presenter::Presenter,
    source_labels: crate::session_source::Service,
    demo: Option<crate::demo::Demo>,
    /// Apply + Test: the samples being browsed; ends on dismissal or on the first real card.
    carousel: Option<crate::demo::Carousel>,
    /// The sample whose sound has played, so a card sounds once per visit.
    test_sound: Option<u64>,
    tray: Tray,
    usage: crate::usage_service::Service,
    claude: Option<crate::usage_service::Service>,
    /// Which agent the usage card describes: Claude when true, otherwise Codex.
    usage_claude: bool,
    usage_next: u64,
    /// Set from the usage services' threads when new numbers arrived; the tray then refreshes at
    /// once instead of at the next beat of the loop.
    usage_changed: Arc<AtomicBool>,
    /// What the menu was last built from, so it is rebuilt only when that changes.
    menu_key: Option<MenuKey>,
    tooltip: String,
    /// The tray icon update error already shown on a card, so a repeating one is shown once.
    tray_failure: Option<String>,
    stop: Arc<AtomicBool>,
    waker: Arc<Waker>,
    config_path: std::path::PathBuf,
    /// Replaces the usage card text, e.g. the about text or a refused Restart; cleared when
    /// the card closes.
    notice: Option<crate::usage_service::UsageNotice>,
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
        let (sound, visual, paused, context, idle) = self
            .shared
            .lock()
            .map(|s| {
                (
                    s.sounds,
                    s.visuals,
                    s.paused,
                    s.warnings.enabled(crate::events::Kind::ContextWarning),
                    s.warnings.enabled(crate::events::Kind::IdleWarning),
                )
            })
            .unwrap_or((false, false, false, true, true));
        let display = self.config.codex.usage.display;
        let views = crate::tray_usage::Views::new(
            self.usage.view(),
            self.claude.as_ref().map(|c| c.view()),
            self.config.codex.usage.enabled,
        );
        let lines = views.lines(display, crate::usage_service::unix_now());
        let tooltip = lines.tooltip("\n");
        let crate::tray_usage::Lines { codex, claude } = lines;
        let messenger = crate::forward::enabled();
        let key = (
            (sound, visual, paused, context, idle),
            messenger,
            format!(
                "{}\n{}",
                codex.as_deref().unwrap_or(""),
                claude.as_deref().unwrap_or("")
            ),
        );
        if self.menu_key.as_ref() != Some(&key) {
            self.tray
                .set_menu(&crate::tray_menu::items(&crate::tray_menu::MenuState {
                    sound,
                    visual,
                    messenger,
                    paused,
                    context,
                    idle,
                    codex: codex.clone(),
                    claude: claude.clone(),
                }));
            self.menu_key = Some(key);
        }
        if now >= self.usage_next || self.usage_changed.swap(false, Ordering::AcqRel) {
            self.usage_next = now.saturating_add(1000);
            if tooltip != self.tooltip {
                self.tray.set_tooltip(&tooltip);
                self.tooltip = tooltip;
            }
            let levels = views.levels(self.config.tray.graph, display);
            crate::notice::report_changed(
                &mut self.tray_failure,
                self.tray.set_levels(levels, self.config.tray.style),
            );
        }
    }

    fn selected_usage(&self) -> &crate::usage_service::Service {
        crate::tray_usage::selected(&self.usage, self.claude.as_ref(), self.usage_claude)
    }

    /// Asks the agent whose details are open for fresh data.
    fn refresh_selected(&self) {
        self.selected_usage().refresh();
    }

    /// Opens the usage card with `notice` instead of the quota details.
    fn show_notice(&mut self, notice: crate::usage_service::UsageNotice) {
        self.notice = Some(notice);
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
        self.notice
            .as_ref()
            .map(crate::usage_service::UsageNotice::text)
            .unwrap_or_else(|| self.selected_usage().card_text())
    }

    /// The usage card's text, page, size or place changed: redraw it from scratch.
    fn invalidate_usage(&mut self) {
        self.usage_surface = None;
        self.frame = None;
        self.presenter.invalidate();
    }

    /// Silences what is playing; the next sound starts with a fresh flag.
    fn cancel_sound(&mut self) {
        self.audio_cancel.store(true, Ordering::Release);
        self.audio_cancel = Arc::new(AtomicBool::new(false));
    }

    /// Pages through a long usage card, or browses the Apply + Test samples, with the wheel.
    fn handle_scrolls(&mut self) {
        let scrolls = self.panel.take_scrolls();
        let now = self.now();
        if let Some(cards) = &mut self.carousel {
            // One step per wake: a trackpad's inertia must not spin through the samples.
            let total: f64 = scrolls.iter().sum();
            if total < 0.0 {
                cards.navigate(1, now);
            } else if total > 0.0 {
                cards.navigate(-1, now);
            }
            return;
        }
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
        use crate::resident_request::Effect;
        for command in self.tray.take_commands() {
            let now = self.now();
            let effects = match self.shared.lock() {
                Ok(mut state) => {
                    crate::resident_request::apply(command, &mut *state, &self.config_path, now)
                }
                Err(_) => continue,
            };
            for effect in effects {
                match effect {
                    Effect::SwitchesChanged => {
                        if let Ok(state) = self.shared.lock() {
                            self.config.sounds.enabled = state.sounds;
                            self.config.notifications.enabled = state.visuals;
                        }
                    }
                    Effect::CancelAudio | Effect::ResumeAudio => {
                        crate::resident_request::apply_audio_effect(
                            &effect,
                            &mut self.audio_cancel,
                        );
                    }
                    Effect::Redraw => self.presenter.invalidate(),
                    // The menu is rebuilt on the next tick because the state is in its key.
                    Effect::ToggleMessenger => {
                        crate::forward::toggle();
                    }
                    Effect::OpenConfig => {
                        crate::resident_request::open_configuration(&self.config_path)
                    }
                    Effect::ShowNotice(message) => {
                        self.show_notice(crate::usage_service::UsageNotice::Text(format!(
                            "aHerald\n{message}"
                        )));
                    }
                    Effect::Restart { test } => {
                        let (stop, waker) = (self.stop.clone(), self.waker.clone());
                        crate::resident_request::restart(test, crate::restart::launch, move || {
                            stop.store(true, Ordering::Release);
                            waker.wake();
                        });
                    }
                    Effect::Quit => self.stop.store(true, Ordering::Release),
                    Effect::ShowUsage { claude } => {
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
                    Effect::ShowAbout => {
                        // The same item closes its open card, like a usage line.
                        if self.usage_window
                            && self.notice == Some(crate::usage_service::UsageNotice::About)
                        {
                            if let Ok(mut state) = self.shared.lock() {
                                state.queue.dismiss();
                            }
                            self.notice = None;
                            self.reset_usage_card();
                        } else {
                            self.show_notice(crate::usage_service::UsageNotice::About);
                        }
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
        // Said once for each combination of the setting and the connected displays.
        self.monitor_watch
            .check(self.config.notifications.monitor, screens.len());
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
        if let Some(cards) = &mut self.carousel {
            let mut dismissed = false;
            for point in clicks {
                let pixels = input::to_pixels(point, window, &layout);
                match input::carousel_click(cards.index(), cards.len(), &layout, pixels) {
                    Some(crate::card::Action::Previous) => cards.navigate(-1, now),
                    Some(crate::card::Action::Next) => cards.navigate(1, now),
                    Some(crate::card::Action::Dismiss) => {
                        dismissed = true;
                        break;
                    }
                    None => {}
                }
            }
            if dismissed {
                self.carousel = None;
                self.cancel_sound();
            }
            return;
        }
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
            || self.carousel.is_some()
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
            crate::resident_request::tick(&mut *state, &self.config, now);
            state.queue.tick(now, paused);
            (
                state.queue.selected().cloned(),
                state.queue.selected_index(),
                state.queue.visible_count(),
                paused,
            )
        };
        let (mut entry, mut index, mut count) = (entry, index, count);
        // A real card ends the preview, as on Windows and Linux.
        if self.carousel.is_some() && entry.is_some() {
            self.carousel = None;
            self.cancel_sound();
        }
        let mut preview_sound = None;
        if let Some(cards) = &mut self.carousel {
            cards.tick(now, paused);
            let selected = cards.selected().clone();
            if self.test_sound != Some(selected.id) && !paused {
                preview_sound = Some(selected.clone());
                self.test_sound = Some(selected.id);
            }
            entry = Some(selected);
            index = cards.index();
            count = cards.len();
        }
        if preview_sound.is_some() {
            self.cancel_sound();
        }
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
        let fresh = if self.carousel.is_some() {
            preview_sound.into_iter().collect()
        } else {
            self.shared
                .lock()
                .map(|mut state| state.queue.take_unannounced(room))
                .unwrap_or_default()
        };
        for fresh in fresh {
            if !fresh.outputs.sound {
                continue;
            }
            if let Some(job) = crate::audio::start_controlled(
                &fresh.event,
                &self.config,
                self.audio_cancel.clone(),
            ) {
                self.audio.push(job);
            }
        }
        // The preview shows its samples even when pop-ups are switched off.
        if !self.config.notifications.enabled && self.carousel.is_none() {
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
            self.art_wait = Some(now + ART_WAIT_MS);
        }
        if self.pending && self.artwork.submit(entry.id, &entry.event, self.scale) {
            self.pending = false;
        }
        let mut loaded = false;
        while let Ok((id, scale, art)) = self.artwork.results.try_recv() {
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
        let heading = if let Some(cards) = &self.carousel {
            crate::card::Heading::new(
                if entry.event.source.agent().is_some() {
                    entry.event.source.label()
                } else {
                    "aHerald"
                },
                Some("[Test]"),
                cards.disabled().then_some("Disabled in config"),
            )
        } else {
            self.source_labels.label(&entry.event)
        };
        let age = crate::card::age_text(now.saturating_sub(entry.arrived_ms));
        let controls = Card::new(
            heading,
            index,
            count,
            self.carousel.is_none() && self.config.notifications.click_to_dismiss,
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
            fade_in_ms: if self.carousel.is_some() {
                0
            } else {
                self.config.notifications.fade_in_ms
            },
            fade_out_ms: if self.carousel.is_some() {
                0
            } else {
                self.config.notifications.fade_out_ms
            },
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
    preview: Preview,
) -> Result<(), String> {
    let (demo, carousel) = match preview {
        Preview::None => (None, None),
        Preview::Demo(demo) => (Some(demo), None),
        Preview::Test(cards) => (None, Some(cards)),
    };
    let mtm = MainThreadMarker::new().ok_or("the macOS overlay must run on the main thread")?;
    let app = NSApplication::sharedApplication(mtm);
    // No Dock icon and no menu bar: the resident is an accessory.
    app.setActivationPolicy(NSApplicationActivationPolicy::Accessory);
    app.finishLaunching();
    waker.set(app.clone());

    let artwork = ArtworkWorker::spawn(&config, {
        let waker = waker.clone();
        move || waker.wake()
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
            config.claude.usage.source,
        )
    });
    let result = Panel::new(mtm).map(|panel| {
        let usage_changed = Arc::new(AtomicBool::new(false));
        for service in std::iter::once(&usage).chain(claude.iter()) {
            let (changed, waker) = (usage_changed.clone(), waker.clone());
            service.on_change(move || {
                changed.store(true, Ordering::Release);
                waker.wake();
            });
        }
        let mut ui = Ui {
            shared,
            start,
            panel,
            mtm,
            artwork,
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
            monitor_watch: crate::notice::MonitorWatch::new(),
            shown_card: Default::default(),
            presenter: crate::presenter::Presenter::default(),
            source_labels: if usage_enabled {
                crate::session_source::Service::shared(usage.names())
            } else {
                crate::session_source::Service::codex()
            },
            demo,
            carousel,
            test_sound: None,
            tray: Tray::new(mtm),
            usage,
            claude,
            usage_claude: false,
            usage_next: 0,
            usage_changed,
            menu_key: None,
            tooltip: String::new(),
            tray_failure: None,
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
            } else if crate::resident_request::idle_poll_needed(
                ui.config.codex.usage.enabled,
                ui.claude.is_some(),
                ui.shared.lock().is_ok_and(|s| s.warnings.has_deadline()),
            ) {
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
        ui.audio_cancel.store(true, Ordering::Release);
        for job in ui.audio.drain(..) {
            let _ = job.join();
        }
    });
    stop.store(true, Ordering::Release);
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{agent::Agent, events::Kind};

    #[test]
    fn a_warning_switch_gates_only_its_own_warnings() {
        let config = Config::parse(
            "[codex.warnings]\nidle_enabled=true\ncontext_tokens=1\nidle_minutes=1",
            Path::new("."),
        )
        .unwrap();
        let shared = new_shared(&config);
        let mut state = shared.lock().unwrap();
        assert!(
            state
                .warnings
                .context(Agent::Codex, "first", Some(2), &config)
                .is_some()
        );
        crate::resident_request::apply(
            crate::tray_menu::Command::ContextWarnings,
            &mut *state,
            std::path::Path::new(""),
            0,
        );
        assert!(!state.warnings.enabled(Kind::ContextWarning));
        assert!(state.warnings.pending().is_empty());
        assert!(
            state
                .warnings
                .context(Agent::Codex, "second", Some(2), &config)
                .is_none(),
            "a switched-off context warning must not be produced"
        );
        assert!(state.warnings.enabled(Kind::IdleWarning));
        crate::resident_request::apply(
            crate::tray_menu::Command::ContextWarnings,
            &mut *state,
            std::path::Path::new(""),
            0,
        );
        assert!(state.warnings.enabled(Kind::ContextWarning));
    }
}
