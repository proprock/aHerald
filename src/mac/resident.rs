//! The macOS resident: IPC on a worker thread, one floating card on the main thread.
//!
//! This is the slice of the Windows resident (`resident.rs`, `resident_overlay.rs`) that the
//! macOS overlay needs: queue, selection, artwork, fades and countdown. Usage, tray, audio
//! and click handling arrive with their own tasks (AH-063..AH-065).
use super::{
    geometry::{self, Frame},
    panel::{self, Panel},
    surface::{Composition, Surface},
};
use crate::{
    animation::PreparedAnimation,
    card::Card,
    card_paint::CardText,
    config::Config,
    events::Event,
    ipc::{Request, Response, pipe},
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
                    match request {
                        Request::Status => Response::Status {
                            pid: std::process::id(),
                            queued: state.queue.items().len(),
                            paused: state.paused,
                            sounds: state.sounds,
                            visuals: state.visuals,
                        },
                        Request::Quit => {
                            shutdown.store(true, Ordering::Release);
                            waker.wake();
                            Response::Bye
                        }
                        Request::Event { event } => {
                            if event.validate().is_err() {
                                return Response::Rejected;
                            }
                            let setting = &settings.events[event.kind];
                            if !setting.enabled {
                                // A disabled kind shows nothing but still ends its turn's waits.
                                state.queue.observe_terminal(&event);
                                return Response::Accepted;
                            }
                            let now = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
                            let duration = if setting.duration_ms == 0 {
                                0
                            } else {
                                setting
                                    .duration_ms
                                    .saturating_add(settings.notifications.fade_out_ms)
                            };
                            if state.queue.push(event, now, duration).is_accepted() {
                                waker.wake();
                                Response::Accepted
                            } else {
                                drop(state);
                                log(Record::QueueRejected);
                                Response::Rejected
                            }
                        }
                    }
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

/// What the event card shows: its heading, kind, queue position and age.
fn event_card_text<'a>(
    config: &Config,
    event: &Event,
    controls: &'a Card,
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
    shown_card: (crate::card::Heading, usize, usize, String),
    presenter: crate::presenter::Presenter,
    source_labels: crate::session_source::Service,
    demo: Option<crate::demo::Demo>,
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
        if self.draw().is_err() {
            self.frame = None;
            self.composition = None;
            self.hide();
            self.compose_due = true;
        }
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
        let Some(entry) = entry.filter(|e| !paused && !e.is_usage()) else {
            self.hide();
            return Ok(());
        };
        // Sounds belong to AH-064; mark the events so the queue does not hold them back.
        if let Ok(mut state) = self.shared.lock() {
            let _ = state.queue.take_unannounced(usize::MAX);
        }
        if !self.config.notifications.enabled {
            self.hide();
            return Ok(());
        }
        let screens = panel::screens(self.mtm);
        if screens.is_empty() {
            return Err("no display".into());
        }
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
    _config_path: &Path,
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
            shown_card: Default::default(),
            presenter: crate::presenter::Presenter::default(),
            source_labels: crate::session_source::Service::codex(),
            demo,
            config,
        };
        // Events queued before the loop started are picked up by the first step.
        while !stop.load(Ordering::Acquire) {
            ui.step();
            let wait = if ui.animating() {
                NSDate::dateWithTimeIntervalSinceNow(FRAME_MS as f64 / 1000.0)
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
        }
        ui.jobs.take();
    });
    stop.store(true, Ordering::Release);
    let _ = worker.join();
    result
}
