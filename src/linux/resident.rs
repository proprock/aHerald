//! Linux resident and detached notification presentation over X11.
use super::{
    input,
    surface::{Composition, Surface},
    x11,
};
use crate::placement::Area;
use crate::{
    artwork_worker::ArtworkWorker, card::Card, card_paint::event_card_text,
    card_surface::CardSurface, config::Config, events::Event, ipc::pipe, local::Scope,
    queue::Queue,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use x11rb::protocol::Event as XEvent;

#[cfg(test)]
use crate::ipc::Response;
pub use crate::resident_client::{ensure, forward, forward_if_running};
#[cfg(test)]
use std::sync::mpsc;

type Shared = crate::resident_request::Shared;
fn shared(config: &Config) -> Arc<Mutex<Shared>> {
    Arc::new(Mutex::new(Shared::new(config, ())))
}

/// About is the usage card's text in its reserved slot; the same item closes it.
fn toggle_about(queue: &mut Queue, open: &mut bool) {
    if *open && queue.selected().is_some_and(|entry| entry.is_usage()) {
        queue.dismiss();
        *open = false;
    } else {
        queue.show_usage();
        *open = true;
    }
}

pub fn run(scope: Scope, config: Config, path: &Path) -> Result<(), String> {
    run_after_apply(scope, config, path, false)
}

pub fn run_after_apply(
    scope: Scope,
    config: Config,
    path: &Path,
    test: bool,
) -> Result<(), String> {
    let Some(_owner) = scope.acquire()? else {
        return Ok(());
    };
    // Open the display before reporting the listener ready.
    let window = x11::Window::new()?;
    let state = shared(&config);
    let started = Instant::now();
    // Whatever changes the state the loop reads signals this, so the loop can sleep until then.
    let wake = Arc::new(pipe::StopEvent::new()?);
    let runtime = crate::resident_runtime::start(scope, &config, path, &state, started, {
        let wake = wake.clone();
        move |_: &Shared| wake.signal()
    })?;
    let usage = Usage::for_resident(&config);
    // Dropping `runtime` afterwards stops the listener and the sinks.
    present(
        window,
        config,
        state,
        Loop {
            stop: runtime.stop_flag(),
            start: started,
            wake,
        },
        if test { Preview::Test } else { Preview::None },
        Some(path),
        usage,
    )
}

pub fn run_demo(config: Config, path: &Path, pause_sec: u64) -> Result<(), String> {
    let demo = crate::demo::Demo::new(&config, pause_sec)?;
    let state = shared(&config);
    let usage = Usage::for_resident(&config);
    present(
        x11::Window::new()?,
        config,
        state,
        Loop::standalone()?,
        Preview::Demo(demo),
        Some(path),
        usage,
    )
}

pub fn show_transient(event: &Event, config: &Config) -> Result<(), String> {
    if !config.notifications.enabled {
        return Ok(());
    }
    let window = x11::Window::new()?;
    let state = shared(config);
    let duration = config.events[event.kind].duration_ms;
    let duration = if duration == 0 {
        0
    } else {
        duration.saturating_add(config.notifications.fade_out_ms)
    };
    state
        .lock()
        .map_err(|_| "resident state unavailable")?
        .queue
        .push(event.clone(), 0, duration);
    present(
        window,
        config.clone(),
        state,
        Loop::standalone()?,
        Preview::None,
        None,
        Usage::none(),
    )
}

/// What the presentation loop shares with the threads around it.
struct Loop {
    /// Set to end the loop; whoever sets it also signals `wake`.
    stop: Arc<AtomicBool>,
    /// Zero of the loop's millisecond clock.
    start: Instant,
    /// Signalled when something the loop reads has changed; the loop sleeps until then.
    wake: Arc<pipe::StopEvent>,
}
impl Loop {
    /// For a loop nothing else talks to (the demo and a one-card notice).
    fn standalone() -> Result<Self, String> {
        Ok(Self {
            stop: Arc::new(AtomicBool::new(false)),
            start: Instant::now(),
            wake: Arc::new(pipe::StopEvent::new()?),
        })
    }
}

/// Ends one pass of the loop: waits for an X11 event, a signal from `wake`, or the timeout.
fn block(
    window: &x11::Window,
    wake: &pipe::StopEvent,
    stop: &AtomicBool,
    busy: bool,
    housekeeping: bool,
) -> Result<(), String> {
    // A stop already requested, or an event already read into the connection's buffer, must not
    // wait for anything else to arrive.
    if stop.load(Ordering::Acquire) || window.has_event()? {
        return Ok(());
    }
    super::wake::wait(
        wake,
        window.descriptor(),
        super::wake::timeout(busy, housekeeping),
    )
}

enum Preview {
    None,
    Demo(crate::demo::Demo),
    Test,
}

/// The usage services behind the tray's numbers. `present` takes them as a parameter so a test
/// can hand it services fed by a stub source.
struct Usage {
    codex: Option<Arc<crate::usage_service::Service>>,
    claude: Option<Arc<crate::usage_service::Service>>,
}
impl Usage {
    /// No numbers: a one-card notice has no tray.
    fn none() -> Self {
        Self {
            codex: None,
            claude: None,
        }
    }
    /// The resident's own services: Codex always, Claude only when its usage is enabled.
    fn for_resident(config: &Config) -> Self {
        Self {
            codex: Some(Arc::new(crate::usage_service::Service::codex(
                config.codex.usage.enabled,
                Duration::from_secs(config.codex.usage.refresh_interval_sec),
            ))),
            claude: config.claude.usage.enabled.then(|| {
                Arc::new(crate::usage_service::Service::claude(
                    true,
                    Duration::from_secs(config.claude.usage.refresh_interval_sec),
                    config.claude.usage.source,
                ))
            }),
        }
    }
}

fn lock(state: &Mutex<Shared>) -> Result<std::sync::MutexGuard<'_, Shared>, String> {
    state
        .lock()
        .map_err(|_| "resident state unavailable".to_owned())
}

/// How long a new card waits for its artwork before the text card is drawn without it. Artwork
/// usually takes 0-130 ms; a stalled worker must not hide the notification.
const ART_WAIT_MS: u64 = 300;

/// What the usage card was last drawn from, so it is redrawn only when one of these changes.
type UsageKey = (String, usize, usize, usize, u64, Area);
/// The text of the event card on screen: heading, position, total and age.
type CardKey = (crate::card::Heading, usize, usize, String);

/// The monitor a card is drawn for, its scale there and where the pointer is.
struct Target<'a> {
    monitor: &'a x11::Monitor,
    scale: f64,
    pointer: (i32, i32),
}

/// What one pass of the loop reads from the shared queue.
struct Tick {
    entry: Option<crate::queue::Entry>,
    index: usize,
    count: usize,
    paused: bool,
    visuals: bool,
    fresh: Vec<crate::queue::Entry>,
    sounds: bool,
}

/// The resident's (or a one-card notice's) window loop and everything it keeps between passes.
struct Ui {
    window: x11::Window,
    config: Config,
    state: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    start: Instant,
    wake: Arc<pipe::StopEvent>,
    config_path: Option<PathBuf>,
    /// A one-card notice: no tray, no sound queue, ends with its card.
    transient: bool,
    tray: Option<super::tray::Tray>,
    usage: Option<Arc<crate::usage_service::Service>>,
    claude: Option<Arc<crate::usage_service::Service>>,
    /// Set when a usage service has new numbers, so the tray refreshes at once.
    usage_changed: Arc<AtomicBool>,
    usage_update: u64,
    labels: crate::session_source::Service,
    carousel: Option<crate::demo::Carousel>,
    demo: Option<crate::demo::Demo>,
    test_sound: Option<u64>,
    usage_claude: bool,
    about_open: bool,
    usage_page: usize,
    usage_drawn: Option<UsageKey>,
    composition: Option<Composition>,
    /// The card, scale and monitor the composition and artwork belong to.
    key: Option<(u64, u64, Area)>,
    shown_card: Option<CardKey>,
    artwork: ArtworkWorker,
    art: Option<Arc<crate::animation::PreparedAnimation>>,
    /// The artwork of the current card still has to be handed to the worker.
    art_pending: bool,
    /// Until when the text card waits for the artwork.
    art_wait: Option<u64>,
    presenter: crate::presenter::Presenter,
    layout: Option<input::Layout>,
    window_size: (f64, f64),
    displayed: bool,
    animation_start: u64,
    monitor_watch: crate::notice::MonitorWatch,
    activation: crate::activation::Worker,
    audio_cancel: Arc<AtomicBool>,
    audio_jobs: Vec<std::thread::JoinHandle<()>>,
}

fn present(
    window: x11::Window,
    config: Config,
    state: Arc<Mutex<Shared>>,
    run: Loop,
    preview: Preview,
    config_path: Option<&Path>,
    usage: Usage,
) -> Result<(), String> {
    Ui::new(window, config, state, run, preview, config_path, usage).run()
}

impl Ui {
    fn new(
        window: x11::Window,
        config: Config,
        state: Arc<Mutex<Shared>>,
        run: Loop,
        preview: Preview,
        config_path: Option<&Path>,
        usage: Usage,
    ) -> Self {
        let Loop { stop, start, wake } = run;
        let carousel =
            matches!(preview, Preview::Test).then(|| crate::demo::Carousel::new(&config));
        let demo = match preview {
            Preview::Demo(demo) => Some(demo),
            _ => None,
        };
        let transient = config_path.is_none();
        let tray = config_path.map(|_| {
            let tray = super::tray::Tray::start(
                config.sounds.enabled,
                config.notifications.enabled,
                false,
                {
                    let wake = wake.clone();
                    move || wake.signal()
                },
            );
            tray.set_messenger(crate::forward::enabled());
            tray
        });
        let Usage {
            codex: usage,
            claude,
        } = usage;
        // New usage numbers refresh the tray at once instead of at the next beat of the loop.
        let usage_changed = Arc::new(AtomicBool::new(false));
        for service in usage.iter().chain(claude.iter()) {
            let (changed, wake) = (usage_changed.clone(), wake.clone());
            service.on_change(move || {
                changed.store(true, Ordering::Release);
                wake.signal();
            });
        }
        let labels = crate::session_source::Service::for_mode(
            &config,
            if transient {
                crate::config::Mode::Transient
            } else {
                crate::config::Mode::Resident
            },
            usage.as_ref().map(|service| service.names()),
        );
        let artwork = ArtworkWorker::spawn(&config, || {}, {
            let wake = wake.clone();
            move || wake.signal()
        });
        Self {
            window,
            config,
            state,
            stop,
            start,
            wake,
            config_path: config_path.map(Path::to_owned),
            transient,
            tray,
            usage,
            claude,
            usage_changed,
            usage_update: 0,
            labels,
            carousel,
            demo,
            test_sound: None,
            usage_claude: false,
            about_open: false,
            usage_page: 0,
            usage_drawn: None,
            composition: None,
            key: None,
            shown_card: None,
            artwork,
            art: None,
            art_pending: false,
            art_wait: None,
            presenter: crate::presenter::Presenter::default(),
            layout: None,
            window_size: (0.0, 0.0),
            displayed: false,
            animation_start: 0,
            monitor_watch: crate::notice::MonitorWatch::new(),
            activation: crate::activation::Worker::default(),
            audio_cancel: Arc::new(AtomicBool::new(false)),
            audio_jobs: Vec::new(),
        }
    }

    fn now(&self) -> u64 {
        self.start.elapsed().as_millis().min(u64::MAX as u128) as u64
    }

    /// Silences what is playing; the next sound starts with a fresh flag.
    fn cancel_sound(&mut self) {
        self.audio_cancel.store(true, Ordering::Release);
        self.audio_cancel = Arc::new(AtomicBool::new(false));
    }

    fn run(&mut self) -> Result<(), String> {
        while !self.stop.load(Ordering::Acquire) {
            let now = self.now();
            self.refresh_usage_tray(now)?;
            self.handle_commands()?;
            self.handle_pointer()?;
            let now = self.now();
            let mut tick = self.read_queue(now)?;
            self.advance_carousel(&mut tick, now);
            self.start_sounds(std::mem::take(&mut tick.fresh), tick.sounds);
            if self.transient && tick.entry.is_none() {
                break;
            }
            let Tick {
                entry,
                index,
                count,
                paused,
                visuals,
                ..
            } = tick;
            let usage_card = match entry
                .filter(|e| !paused && (self.carousel.is_some() || visuals || e.is_usage()))
            {
                Some(entry) => self.draw(&entry, index, count, now)?,
                None => {
                    self.hide_window()?;
                    false
                }
            };
            self.wait(usage_card)?;
        }
        self.audio_cancel.store(true, Ordering::Release);
        Ok(())
    }

    /// Sends the tray the current usage lines and icon levels once a second, or at once when a
    /// service says its numbers changed.
    fn refresh_usage_tray(&mut self, now: u64) -> Result<(), String> {
        if now < self.usage_update && !self.usage_changed.swap(false, Ordering::AcqRel) {
            return Ok(());
        }
        self.usage_update = now.saturating_add(1000);
        if let (Some(tray), Some(codex)) = (&self.tray, &self.usage) {
            let display = self.config.codex.usage.display;
            let views = crate::tray_usage::Views::new(
                codex.view(),
                self.claude.as_ref().map(|service| service.view()),
                self.config.codex.usage.enabled,
            );
            let crate::tray_usage::Lines {
                codex: codex_line,
                claude: claude_line,
            } = views.lines(display, crate::usage_service::unix_now());
            let levels = views.levels(self.config.tray.graph, display);
            tray.set_usage(codex_line, claude_line, &levels)?;
        }
        Ok(())
    }

    /// Carries out what the tray menu asked for.
    fn handle_commands(&mut self) -> Result<(), String> {
        let Some(tray) = &self.tray else {
            return Ok(());
        };
        let commands: Vec<_> = tray.commands.try_iter().collect();
        for command in commands {
            let now = self.now();
            let path = self.config_path.clone().unwrap_or_default();
            let effects = {
                let mut guard = lock(&self.state)?;
                crate::resident_request::apply(command, &mut *guard, &path, now)
            };
            for effect in effects {
                self.execute(effect)?;
            }
        }
        Ok(())
    }

    fn execute(&mut self, effect: crate::resident_request::Effect) -> Result<(), String> {
        use crate::resident_request::Effect;
        match effect {
            Effect::Quit => self.stop.store(true, Ordering::Release),
            Effect::ShowUsage { claude } => self.toggle_usage_card(claude)?,
            Effect::ShowAbout => {
                {
                    let mut guard = lock(&self.state)?;
                    toggle_about(&mut guard.queue, &mut self.about_open);
                }
                self.usage_page = 0;
                self.usage_drawn = None;
                self.presenter.invalidate();
            }
            Effect::ShowNotice(message) => {
                crate::notice::post(&message);
                eprintln!("aHerald: {message}");
            }
            Effect::Restart { test } => {
                let (stop, wake) = (self.stop.clone(), self.wake.clone());
                crate::resident_request::restart(test, crate::restart::launch, move || {
                    stop.store(true, Ordering::Release);
                    wake.signal();
                });
            }
            Effect::OpenConfig => crate::resident_request::open_configuration(
                self.config_path
                    .as_deref()
                    .expect("resident configuration path"),
            ),
            Effect::ToggleMessenger => {
                crate::forward::toggle();
                if let Some(tray) = &self.tray {
                    tray.set_messenger(crate::forward::enabled());
                }
            }
            Effect::SwitchesChanged => {
                let guard = lock(&self.state)?;
                if let Some(tray) = &self.tray {
                    tray.set_state(
                        guard.sounds,
                        guard.visuals,
                        guard.paused,
                        guard.warnings.enabled(crate::events::Kind::ContextWarning),
                        guard.warnings.enabled(crate::events::Kind::IdleWarning),
                    );
                }
            }
            Effect::CancelAudio | Effect::ResumeAudio => {
                crate::resident_request::apply_audio_effect(&effect, &mut self.audio_cancel);
            }
            Effect::Redraw => self.presenter.invalidate(),
        }
        Ok(())
    }

    /// The same menu item opens the usage card of an agent and closes it again.
    fn toggle_usage_card(&mut self, claude: bool) -> Result<(), String> {
        {
            let mut guard = lock(&self.state)?;
            if guard.queue.selected().is_some_and(|entry| entry.is_usage())
                && !self.about_open
                && self.usage_claude == claude
            {
                guard.queue.dismiss();
            } else {
                guard.queue.show_usage();
                self.about_open = false;
                self.usage_claude = claude;
                self.usage_page = 0;
                let service = if claude { &self.claude } else { &self.usage };
                if let Some(service) = service {
                    service.refresh();
                }
            }
        }
        self.usage_drawn = None;
        self.presenter.invalidate();
        Ok(())
    }

    fn handle_pointer(&mut self) -> Result<(), String> {
        while let Some(event) = self.window.poll()? {
            if let XEvent::ButtonPress(event) = event
                && let Some(layout) = self.layout
            {
                let point = input::to_pixels(
                    (f64::from(event.event_x), f64::from(event.event_y)),
                    self.window_size,
                    &layout,
                );
                if self.carousel.is_some() {
                    self.press_carousel(event.detail, &layout, point);
                } else {
                    self.press_card(&event, &layout, point)?;
                }
            } else if matches!(event, XEvent::Expose(_)) {
                self.presenter.invalidate();
            }
        }
        Ok(())
    }

    /// A click or the wheel on the Apply + Test samples.
    fn press_carousel(&mut self, button: u8, layout: &input::Layout, point: (i32, i32)) {
        let now = self.now();
        let Some(cards) = &mut self.carousel else {
            return;
        };
        let mut controls = Card::new("", cards.index(), cards.len(), false);
        controls.text_left = Some(layout.text_left);
        let hit = |controls: &Card| {
            controls
                .hit_test(layout.width, layout.height, layout.scale, point.0, point.1)
                .or_else(|| {
                    controls
                        .body_hit_test(layout.width, layout.height, layout.scale, point.0, point.1)
                        .then_some(crate::card::Action::Next)
                })
        };
        let action = match button {
            4 => Some(crate::card::Action::Previous),
            5 => Some(crate::card::Action::Next),
            1 => hit(&controls),
            _ => None,
        };
        match action {
            Some(crate::card::Action::Previous) => cards.navigate(-1, now),
            Some(crate::card::Action::Next) => cards.navigate(1, now),
            Some(crate::card::Action::Dismiss) => {
                self.carousel = None;
                self.cancel_sound();
            }
            None => {}
        }
        self.presenter.invalidate();
    }

    /// A click or the wheel on the card in the queue (or on the usage card).
    fn press_card(
        &mut self,
        event: &x11rb::protocol::xproto::ButtonPressEvent,
        layout: &input::Layout,
        point: (i32, i32),
    ) -> Result<(), String> {
        let mut guard = self
            .state
            .lock()
            .map_err(|_| "resident state unavailable")?;
        let on_usage = guard.queue.selected().is_some_and(|entry| entry.is_usage());
        if event.detail == 1 {
            let body = !on_usage
                && input::on_body(
                    &guard.queue,
                    self.config.notifications.click_to_dismiss,
                    layout,
                    point,
                );
            if body {
                super::activation::note_click(event.time);
            }
            if let Some(selected) = guard.queue.selected() {
                let directory = self
                    .config_path
                    .as_deref()
                    .and_then(Path::parent)
                    .unwrap_or_else(|| Path::new("."));
                self.activation.submit(
                    &selected.event,
                    self.config.notifications.activate_agent_on_click,
                    body,
                    directory,
                    self.config.general.logging,
                );
            }
        }
        let dismiss = self.config.notifications.click_to_dismiss || on_usage;
        if event.detail == 1 && input::click(&mut guard.queue, dismiss, layout, point) {
            self.presenter.invalidate();
        } else if event.detail == 4 && on_usage {
            self.usage_page = self.usage_page.saturating_sub(1);
            self.usage_drawn = None;
        } else if event.detail == 5 && on_usage {
            self.usage_page = self.usage_page.saturating_add(1);
            self.usage_drawn = None;
        } else if event.detail == 4 {
            guard.queue.next(-1);
            self.presenter.invalidate();
        } else if event.detail == 5 {
            guard.queue.next(1);
            self.presenter.invalidate();
        }
        Ok(())
    }

    /// Advances the queue and the demo, and reads what the window and the sounds need.
    fn read_queue(&mut self, now: u64) -> Result<Tick, String> {
        self.audio_jobs.retain(|job| !job.is_finished());
        let room = crate::presenter::sound_slots(self.audio_jobs.len());
        let mut guard = self
            .state
            .lock()
            .map_err(|_| "resident state unavailable")?;
        let paused = guard.paused;
        if let Some(demo) = &mut self.demo {
            demo.tick(now, paused, &mut guard.queue);
        }
        crate::resident_request::tick(&mut *guard, &self.config, now);
        guard.queue.tick(now, paused);
        let fresh = if !self.transient && !paused {
            guard.queue.take_unannounced(room)
        } else {
            Vec::new()
        };
        Ok(Tick {
            entry: guard.queue.selected().cloned(),
            index: guard.queue.selected_index(),
            count: guard.queue.visible_count(),
            paused,
            visuals: guard.visuals,
            fresh,
            sounds: guard.sounds,
        })
    }

    /// The Apply + Test samples replace the queue's card until a real card arrives or they are
    /// dismissed; each sample sounds once.
    fn advance_carousel(&mut self, tick: &mut Tick, now: u64) {
        if self.carousel.is_some() && tick.entry.is_some() {
            self.carousel = None;
            self.cancel_sound();
        }
        let Some(cards) = &mut self.carousel else {
            return;
        };
        cards.tick(now, tick.paused);
        let selected = cards.selected().clone();
        tick.index = cards.index();
        tick.count = cards.len();
        if self.test_sound != Some(selected.id) && !tick.paused {
            self.test_sound = Some(selected.id);
            tick.fresh = vec![selected.clone()];
            self.cancel_sound();
        }
        tick.entry = Some(selected);
    }

    fn start_sounds(&mut self, fresh: Vec<crate::queue::Entry>, sounds: bool) {
        let mut audio_config = self.config.clone();
        audio_config.sounds.enabled = sounds;
        for fresh in fresh.into_iter().filter(|f| f.outputs.sound) {
            if let Some(job) = crate::audio::start_controlled(
                &fresh.event,
                &audio_config,
                self.audio_cancel.clone(),
            ) {
                self.audio_jobs.push(job);
            }
        }
    }

    fn hide_window(&mut self) -> Result<(), String> {
        if self.displayed {
            self.window.hide()?;
            self.displayed = false;
        }
        self.key = None;
        self.usage_drawn = None;
        self.composition = None;
        self.layout = None;
        self.art_wait = None;
        self.presenter.invalidate();
        Ok(())
    }

    /// Draws the card chosen by the queue; true when that was the usage card.
    fn draw(
        &mut self,
        entry: &crate::queue::Entry,
        index: usize,
        count: usize,
        now: u64,
    ) -> Result<bool, String> {
        let monitors = self.window.monitors()?;
        self.monitor_watch
            .check(self.config.notifications.monitor, monitors.len());
        let pointer = self.window.pointer()?;
        let monitor = x11::select_monitor(
            &monitors,
            self.config.notifications.monitor,
            self.config.notifications.position,
            pointer,
        )
        .ok_or("X11: no monitor")?;
        let scale = self.config.notifications.scale * monitor.scale;
        if entry.is_usage() {
            let target = Target {
                monitor: &monitor,
                scale,
                pointer,
            };
            self.draw_usage(index, count, &target)?;
            return Ok(true);
        }
        self.usage_drawn = None;
        let target = Target {
            monitor: &monitor,
            scale,
            pointer,
        };
        self.draw_event(entry, index, count, now, &target)?;
        Ok(false)
    }

    fn usage_text(&self) -> String {
        if self.about_open {
            return crate::about_card_text();
        }
        let service = if self.usage_claude {
            &self.claude
        } else {
            &self.usage
        };
        service
            .as_ref()
            .map(|service| service.card_text())
            .unwrap_or_else(|| "Usage unavailable".into())
    }

    fn draw_usage(&mut self, index: usize, count: usize, target: &Target) -> Result<(), String> {
        let Target {
            monitor,
            scale,
            pointer,
        } = *target;
        let text = self.usage_text();
        let (title, body) = text.split_once('\n').unwrap_or((&text, ""));
        let pages = crate::usage_service::card_pages(body);
        self.usage_page = self.usage_page.min(pages.len() - 1);
        let page = self.usage_page;
        let usage_key = (
            text.clone(),
            page,
            index,
            count,
            scale.to_bits(),
            monitor.area,
        );
        if self.usage_drawn.as_ref() == Some(&usage_key) {
            return Ok(());
        }
        let title = if pages.len() > 1 {
            format!("{title}  ({}/{})", page + 1, pages.len())
        } else {
            title.to_owned()
        };
        let controls = Card::new(
            crate::card::Heading::from_summary(&title),
            index,
            count,
            true,
        );
        let width = (760.0 * scale) as i32;
        let height = ((114.0 + 24.0 * pages[page].lines().count() as f64) * scale) as i32;
        let text_left = (22.0 * scale) as i32;
        let surface = Surface::card(
            width,
            height,
            scale,
            text_left,
            crate::card_paint::CardText {
                heading: &controls.heading,
                label: &pages[page],
                body: true,
                index,
                count,
                dismiss_button: false,
                age: None,
                color: crate::config::Color([80, 165, 250]),
                background_color: self.config.usage_background_color(),
                border: self.config.usage_border_color(),
            },
        )?;
        let place = crate::placement::layout_rendered(
            monitor.area,
            (width, height),
            scale,
            self.config.notifications.position,
            pointer,
        );
        show_scaled(&self.window, &surface, place, 255, "usage card")?;
        self.layout = Some(input::Layout {
            width,
            height,
            scale,
            text_left,
        });
        self.window_size = (f64::from(place.2), f64::from(place.3));
        self.displayed = true;
        self.usage_drawn = Some(usage_key);
        self.key = None;
        self.presenter.invalidate();
        Ok(())
    }

    /// Starts over for a new card, scale or monitor: the artwork is asked for again and the text
    /// card waits a moment for it.
    fn begin_card(&mut self, key: (u64, u64, Area), now: u64) {
        self.key = Some(key);
        self.art = None;
        self.art_pending = true;
        self.composition = None;
        self.shown_card = None;
        self.animation_start = now;
        self.art_wait = Some(now + ART_WAIT_MS);
        self.presenter.invalidate();
    }

    /// Hands the card's artwork job to the worker and takes what it has finished; true while the
    /// card should keep waiting for it.
    fn poll_artwork(&mut self, entry: &crate::queue::Entry, scale: f64, now: u64) -> bool {
        if self.art_pending && self.artwork.submit(entry.id, &entry.event, scale) {
            self.art_pending = false;
        }
        while let Ok((id, prepared_at, art)) = self.artwork.results.try_recv() {
            if id != entry.id {
                continue;
            }
            if prepared_at == scale {
                self.art = art;
                // A card already drawn without artwork is composed again with it.
                self.composition = None;
                self.art_wait = None;
            } else {
                // Prepared for a display the card has since left: ask again.
                self.art_pending = true;
            }
        }
        if self.art_wait.is_some_and(|deadline| now >= deadline) {
            self.art_wait = None;
        }
        self.art_wait.is_some()
    }

    fn draw_event(
        &mut self,
        entry: &crate::queue::Entry,
        index: usize,
        count: usize,
        now: u64,
        target: &Target,
    ) -> Result<(), String> {
        let Target {
            monitor,
            scale,
            pointer,
        } = *target;
        let new_key = (entry.id, scale.to_bits(), monitor.area);
        if self.key != Some(new_key) {
            self.begin_card(new_key, now);
        }
        if self.poll_artwork(entry, scale, now) {
            return Ok(());
        }
        self.compose(entry, index, count, now, scale)?;
        let config = &self.config;
        let c = self.composition.as_mut().ok_or("missing composed card")?;
        let in_test = self.carousel.is_some();
        let Some(shown) = self.presenter.next(&crate::presenter::Input {
            entry,
            durations: &c.durations,
            looping: c.looping,
            animation_ms: now.saturating_sub(self.animation_start),
            fade_in_ms: if in_test {
                0
            } else {
                config.notifications.fade_in_ms
            },
            fade_out_ms: if in_test {
                0
            } else {
                config.notifications.fade_out_ms
            },
        }) else {
            return Ok(());
        };
        let place = crate::placement::layout_rendered(
            monitor.area,
            c.size(),
            scale,
            config.notifications.position,
            pointer,
        );
        let text_left = c.text_left;
        let surface = c.render(shown.frame, shown.seconds, scale)?;
        show_scaled(&self.window, surface, place, shown.alpha, "card")?;
        self.layout = Some(input::Layout {
            width: surface.width(),
            height: surface.height(),
            scale,
            text_left,
        });
        self.window_size = (f64::from(place.2), f64::from(place.3));
        self.displayed = true;
        Ok(())
    }

    /// Composes the card (heading, text and artwork), or only refreshes its text when that is all
    /// that changed.
    fn compose(
        &mut self,
        entry: &crate::queue::Entry,
        index: usize,
        count: usize,
        now: u64,
        scale: f64,
    ) -> Result<(), String> {
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
            self.labels.label(&entry.event)
        };
        let age = crate::card::age_text(now.saturating_sub(entry.arrived_ms));
        let controls = Card::new(
            heading,
            index,
            count,
            self.carousel.is_none() && self.config.notifications.click_to_dismiss,
        );
        let card_key = (controls.heading.clone(), index, count, age);
        let config = &self.config;
        let paint = |w, h, left| {
            Surface::card(
                w,
                h,
                scale,
                left,
                event_card_text(config, &entry.event, &controls, Some(&card_key.3)),
            )
        };
        if self.composition.is_none() {
            let text_width = Surface::event_text_width(&entry.event, scale)?;
            self.composition = Some(Composition::compose_with_text_width(
                scale,
                self.art.clone(),
                text_width,
                paint,
            )?);
            self.presenter.invalidate();
        } else if self.shown_card.as_ref() != Some(&card_key) {
            if let Some(c) = self.composition.as_mut() {
                c.update_card(paint)?;
            }
            self.presenter.invalidate();
        } else {
            return Ok(());
        }
        self.shown_card = Some(card_key);
        Ok(())
    }

    /// Ends one pass: waits for an X11 event, a signal from `wake`, or the timeout.
    fn wait(&self, usage_card: bool) -> Result<(), String> {
        // A frame while something is shown or queued; otherwise until work arrives, with a slow
        // beat only if usage polling or a session-warning deadline keeps the tray fresh.
        let busy = usage_card
            || self.displayed
            || self.carousel.is_some()
            || self.demo.is_some()
            || self.art_wait.is_some()
            || !self.audio_jobs.is_empty()
            || self
                .state
                .lock()
                .map_or(true, |guard| !guard.queue.items().is_empty());
        let housekeeping = !usage_card
            && crate::resident_request::idle_poll_needed(
                self.usage.is_some() && self.config.codex.usage.enabled,
                self.claude.is_some(),
                self.state
                    .lock()
                    .is_ok_and(|guard| guard.warnings.has_deadline()),
            );
        block(&self.window, &self.wake, &self.stop, busy, housekeeping)
    }
}

/// Shows `surface` in the window at `place` (x, y, width, height), scaled when the monitor's
/// placement asks for another size than the surface was drawn at.
fn show_scaled(
    window: &x11::Window,
    surface: &Surface,
    place: (i32, i32, i32, i32),
    alpha: u8,
    what: &str,
) -> Result<(), String> {
    let (x, y, w, h) = place;
    let (source_w, source_h) = (surface.width(), surface.height());
    let resized;
    let pixels = if (w, h) == (source_w, source_h) {
        surface.pixels()
    } else {
        let image =
            image::RgbaImage::from_raw(source_w as u32, source_h as u32, surface.pixels().to_vec())
                .ok_or_else(|| format!("invalid {what} pixels"))?;
        resized = image::imageops::resize(
            &image,
            w as u32,
            h as u32,
            image::imageops::FilterType::Triangle,
        )
        .into_raw();
        &resized
    };
    window.show(
        x,
        y,
        u16::try_from(w).map_err(|_| format!("{what} too wide"))?,
        u16::try_from(h).map_err(|_| format!("{what} too tall"))?,
        pixels,
        alpha,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A usage source whose only read waits for the snapshot the test sends, so the test
    /// decides the moment of publication.
    struct Gate(mpsc::Receiver<crate::usage::Snapshot>);
    impl crate::usage_service::UsageSource for Gate {
        fn read(
            &mut self,
            _now: Duration,
        ) -> Result<crate::usage::Snapshot, crate::app_server::Error> {
            self.0
                .recv_timeout(Duration::from_secs(30))
                .map_err(|_| crate::app_server::Error::Timeout)
        }
    }

    /// The tooltip text of the StatusNotifierItem that the resident's tray serves on the
    /// session bus, as a host would read it; `None` while it is not there.
    fn tray_description(bus: &zbus::blocking::Connection) -> Option<String> {
        use zbus::names::InterfaceName;
        type ToolTip = (String, Vec<(i32, i32, Vec<u8>)>, String, String);
        let own = bus.unique_name().map(|name| name.to_string());
        let names = zbus::blocking::fdo::DBusProxy::new(bus)
            .ok()?
            .list_names()
            .ok()?;
        names
            .into_iter()
            .filter(|name| name.starts_with(':') && Some(name.as_str()) != own.as_deref())
            .find_map(|name| {
                let properties = zbus::blocking::fdo::PropertiesProxy::builder(bus)
                    .destination(name.to_string())
                    .ok()?
                    .path("/StatusNotifierItem")
                    .ok()?
                    .build()
                    .ok()?;
                let value = properties
                    .get(
                        InterfaceName::try_from("org.kde.StatusNotifierItem").ok()?,
                        "ToolTip",
                    )
                    .ok()?;
                ToolTip::try_from(value).ok().map(|tip| tip.3)
            })
    }

    /// Run with `xvfb-run -a dbus-run-session -- env AH283_ISOLATED=1 cargo test --locked --lib
    /// a_changed_usage_snapshot -- --ignored`. The usage worker publishes once, while the
    /// loop is idle (no card, no animation, no polling): the tray must show it within 500 ms.
    #[test]
    #[ignore = "requires an isolated X11 display and private D-Bus session"]
    fn a_changed_usage_snapshot_reaches_the_tray_within_one_wake_up() {
        assert_eq!(
            std::env::var("AH283_ISOLATED").as_deref(),
            Ok("1"),
            "use a private bus and display"
        );
        let (publish, gate) = mpsc::channel();
        let service = Arc::new(crate::usage_service::Service::spawn_source(
            crate::agent::Agent::Codex,
            true,
            Duration::from_secs(3600),
            Arc::new(AtomicBool::new(false)),
            move || Gate(gate),
        ));
        let config = Config::default();
        let state = shared(&config);
        let run = Loop::standalone().unwrap();
        let (stop, wake) = (run.stop.clone(), run.wake.clone());
        let usage = Usage {
            codex: Some(service.clone()),
            claude: None,
        };
        let resident = std::thread::spawn(move || {
            present(
                x11::Window::new().unwrap(),
                config,
                state,
                run,
                Preview::None,
                Some(Path::new("usage-wake-test.toml")),
                usage,
            )
        });
        let bus = zbus::blocking::Connection::session().unwrap();
        let wait_for = |wanted: &dyn Fn(&str) -> bool, limit: Duration| {
            let deadline = Instant::now() + limit;
            loop {
                match tray_description(&bus) {
                    Some(text) if wanted(&text) => return Some(Instant::now()),
                    _ if Instant::now() >= deadline => return None,
                    _ => std::thread::sleep(Duration::from_millis(5)),
                }
            }
        };
        wait_for(&|_| true, Duration::from_secs(10)).expect("the tray did not appear");
        // Let the loop go idle: it has drawn its first tray state and now blocks until woken.
        std::thread::sleep(Duration::from_millis(500));
        let bucket = crate::usage::Bucket {
            primary: Some(crate::usage::Window {
                used_percent: Some(30.0),
                window_duration_mins: Some(300),
                resets_at: None,
            }),
            ..Default::default()
        };
        let snapshot = crate::usage::Snapshot::single("codex", bucket).unwrap();
        let published = Instant::now();
        publish.send(snapshot).unwrap();
        let shown = wait_for(&|text| text.contains("70%"), Duration::from_secs(5));
        stop.store(true, Ordering::Release);
        wake.signal();
        let result = resident.join().unwrap();
        drop(publish);
        drop(service);
        result.unwrap();
        let shown = shown.expect("the tray never showed the snapshot");
        assert!(
            shown.duration_since(published) < Duration::from_millis(500),
            "the tray took {:?}",
            shown.duration_since(published)
        );
    }

    #[test]
    fn warning_switches_keep_queued_cards_clear_pending_and_reset_on_restart() {
        use crate::{
            agent::Agent,
            events::{Kind, Source},
        };
        let config = Config::parse("[codex.warnings]\nidle_enabled=true\ncontext_tokens=1\nidle_minutes=1\n[claude.warnings]\nidle_enabled=true\ncontext_tokens=1\nidle_minutes=1", Path::new(".")).unwrap();
        let state = shared(&config);
        let mut state = state.lock().unwrap();
        let queued = state
            .warnings
            .context(Agent::Codex, "queued", Some(2), &config)
            .unwrap();
        state.queue.push(queued.clone(), 0, 0);
        state.warnings.admission(&queued, true);
        state
            .warnings
            .context(Agent::Claude, "pending", Some(2), &config)
            .unwrap();
        crate::resident_request::apply(
            crate::tray_menu::Command::ContextWarnings,
            &mut *state,
            std::path::Path::new(""),
            0,
        );
        assert!(state.warnings.pending().is_empty());
        assert_eq!(state.queue.items().len(), 1);
        assert!(state.warnings.enabled(Kind::IdleWarning));
        let mut stop = Event::demo("turn_complete").unwrap();
        stop.source = Source::CodexHook;
        stop.session_id = Some("idle".into());
        state.warnings.observe(&stop, &config, 0);
        state.paused = true;
        crate::resident_request::tick(&mut *state, &config, 60_001);
        assert_eq!(state.queue.items().len(), 1);
        crate::resident_request::apply(
            crate::tray_menu::Command::IdleWarnings,
            &mut *state,
            std::path::Path::new(""),
            0,
        );
        state.paused = false;
        crate::resident_request::tick(&mut *state, &config, 120_000);
        assert_eq!(state.queue.items().len(), 1);
        crate::resident_request::apply(
            crate::tray_menu::Command::ContextWarnings,
            &mut *state,
            std::path::Path::new(""),
            0,
        );
        assert!(state.warnings.enabled(Kind::ContextWarning));
        assert!(!state.warnings.enabled(Kind::IdleWarning));
        let restarted = shared(&config);
        let restarted = restarted.lock().unwrap();
        assert!(restarted.warnings.enabled(Kind::ContextWarning));
        assert!(restarted.warnings.enabled(Kind::IdleWarning));
        assert_eq!(
            config.codex.warnings.context_tokens,
            Some(crate::config::TokenThreshold(1))
        );
        assert_eq!(config.claude.warnings.idle_minutes, Some(1));
    }

    #[test]
    fn linux_warning_reports_cross_and_rearm_independently() {
        use crate::ipc::Request;
        let config = Config::parse(
            "[codex.warnings]\ncontext_tokens=100\n[claude.warnings]\ncontext_tokens=200",
            Path::new("."),
        )
        .unwrap();
        let state = shared(&config);
        let mut state = state.lock().unwrap();
        for (codex, tokens, expected) in [
            (true, 101, 1),
            (false, 101, 1),
            (false, 201, 2),
            (true, 101, 2),
            (true, 100, 2),
            (true, 101, 3),
        ] {
            let request = if codex {
                let input = format!(
                    r#"{{"hook_event_name":"PostModelRequest","session_id":"same","status":"completed","token_usage":{{"input_tokens":{tokens}}}}}"#
                );
                Request::CodexContext {
                    report: crate::codex_context::parse(input.as_bytes())
                        .unwrap()
                        .unwrap(),
                }
            } else {
                let input = format!(
                    r#"{{"session_id":"same","context_window":{{"current_usage":{{"input_tokens":{tokens},"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}}}}"#
                );
                Request::StatusLine {
                    report: crate::statusline::parse(input.as_bytes()).unwrap(),
                    raw: None,
                }
            };
            let request = crate::ipc::decode(&crate::ipc::encode(&request).unwrap()).unwrap();
            assert!(matches!(
                crate::resident_request::respond(request, &mut *state, &config, 0).0,
                Response::Accepted
            ));
            assert_eq!(state.queue.items().len(), expected);
        }
        assert_eq!(
            state.queue.items()[0].event.title(),
            "Context: 0.101k > 0.1k"
        );
        assert_eq!(
            state.queue.items()[1].event.title(),
            "Context: 0.201k > 0.2k"
        );
    }

    #[test]
    fn linux_idle_deadline_pauses_retries_and_cancels_stale_cards() {
        use crate::{
            events::{Kind, Source},
            ipc::Request,
        };
        let config = Config::parse("[codex.warnings]\nidle_enabled=true\nidle_minutes=1\n[claude.warnings]\nidle_enabled=true\nidle_minutes=2\n[events.turn_complete]\nenabled=false", Path::new(".")).unwrap();
        let state = shared(&config);
        let mut state = state.lock().unwrap();
        state.queue = Queue::new(1, 0);
        let mut stop = Event::demo("turn_complete").unwrap();
        stop.source = Source::CodexHook;
        stop.session_id = Some("idle".into());
        crate::resident_request::respond(
            Request::Event {
                event: stop.clone(),
            },
            &mut *state,
            &config,
            0,
        );
        assert!(state.queue.items().is_empty());
        state.paused = true;
        crate::resident_request::tick(&mut *state, &config, 60_001);
        assert!(state.queue.items().is_empty());
        state.paused = false;
        let mut blocker = Event::demo("permission_required").unwrap();
        blocker.session_id = Some("other".into());
        state.queue.push(blocker, 60_001, 0);
        crate::resident_request::tick(&mut *state, &config, 60_001);
        state.queue.dismiss();
        crate::resident_request::tick(&mut *state, &config, 90_000);
        let event = &state.queue.items()[0].event;
        assert_eq!(event.kind, Kind::IdleWarning);
        assert_eq!(event.title(), "Idle: 1m 0s (limit 1m)");
        stop.activity = true;
        crate::resident_request::respond(
            Request::Event { event: stop },
            &mut *state,
            &config,
            90_001,
        );
        assert!(state.queue.items().is_empty());
        crate::resident_request::tick(&mut *state, &config, 180_000);
        assert!(state.queue.items().is_empty());
    }

    #[test]
    fn about_takes_the_usage_slot_and_the_same_item_closes_it() {
        let mut queue = Queue::new(8, 0);
        let mut open = false;
        toggle_about(&mut queue, &mut open);
        assert!(open && queue.selected().is_some_and(|entry| entry.is_usage()));
        queue.tick(120_000, false);
        assert!(queue.selected().is_some(), "About is persistent");
        toggle_about(&mut queue, &mut open);
        assert!(!open && queue.selected().is_none());
        // Closed by a click: the next About opens the card again.
        toggle_about(&mut queue, &mut open);
        queue.dismiss();
        toggle_about(&mut queue, &mut open);
        assert!(open && queue.selected().is_some_and(|entry| entry.is_usage()));
    }
}
