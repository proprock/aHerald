//! Linux resident and detached notification presentation over X11.
use super::{
    input,
    surface::{Composition, Surface},
    x11,
};
use crate::{
    card::Card,
    card_paint::event_card_text,
    card_surface::CardSurface,
    config::Config,
    events::Event,
    ipc::{Response, pipe},
    local::Scope,
    queue::Queue,
};
use std::{
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use x11rb::protocol::Event as XEvent;

pub use crate::resident_client::{ensure, forward, forward_if_running};

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
    let stop = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    // Whatever changes the state the loop reads signals this, so the loop can sleep until then.
    let wake = Arc::new(pipe::StopEvent::new()?);
    crate::notice::install(crate::resident_request::notice_sink(
        state.clone(),
        config.clone(),
        started,
        {
            let wake = wake.clone();
            move |_: &Shared| wake.signal()
        },
    ));
    crate::forward::start(&config.forward);
    let remote = crate::remote::start_if_enabled(
        &config,
        path,
        crate::resident_request::remote_deliver(state.clone(), config.clone(), started, {
            let wake = wake.clone();
            move |_: &Shared| wake.signal()
        }),
    );
    let stop_event = Arc::new(pipe::StopEvent::new()?);
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let listener = {
        let (state, stop, stop_event, settings, wake) = (
            state.clone(),
            stop.clone(),
            stop_event.clone(),
            config.clone(),
            wake.clone(),
        );
        let directory = path.parent().unwrap_or_else(|| Path::new(".")).to_owned();
        std::thread::spawn(move || {
            use crate::operational_log::{Record, init, log};
            init(&directory, settings.general.logging);
            let server = match scope.create_pipe() {
                Ok(server) => server,
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                    return;
                }
            };
            let _ = ready_tx.send(Ok(()));
            log(Record::Started);
            let mut diagnostics = crate::resident_request::Diagnostics::default();
            let mut ipc_failing = false;
            while !stop.load(Ordering::Acquire) {
                let result = server.serve_until(&stop_event, |request| {
                    let Ok(mut state) = state.lock() else {
                        return Response::Rejected;
                    };
                    let now = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
                    let (response, effects) =
                        crate::resident_request::respond(request, &mut *state, &settings, now);
                    drop(state);
                    if effects.quit {
                        stop.store(true, Ordering::Release);
                    }
                    // A request may have queued a card, a warning or a stop.
                    wake.signal();
                    diagnostics.record(&response, effects);
                    response
                });
                if result.is_err() && !ipc_failing {
                    log(Record::IpcFailure);
                }
                ipc_failing = result.is_err();
            }
            log(Record::Stopped);
        })
    };
    let startup = ready_rx
        .recv_timeout(Duration::from_secs(1))
        .unwrap_or_else(|_| Err("resident listener startup deadline exceeded".into()));
    let usage = Usage::for_resident(&config);
    let result = match startup {
        Ok(()) => present(
            window,
            config,
            state,
            Loop {
                stop: stop.clone(),
                start: started,
                wake: wake.clone(),
            },
            if test { Preview::Test } else { Preview::None },
            Some(path),
            usage,
        ),
        Err(e) => Err(e),
    };
    crate::notice::clear();
    drop(remote);
    crate::forward::stop();
    stop.store(true, Ordering::Release);
    stop_event.signal();
    let _ = listener.join();
    result
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

fn present(
    window: x11::Window,
    config: Config,
    state: Arc<Mutex<Shared>>,
    run: Loop,
    preview: Preview,
    config_path: Option<&Path>,
    usage: Usage,
) -> Result<(), String> {
    let Loop { stop, start, wake } = run;
    let mut carousel =
        matches!(preview, Preview::Test).then(|| crate::demo::Carousel::new(&config));
    let mut demo = match preview {
        Preview::Demo(demo) => Some(demo),
        _ => None,
    };
    let mut test_sound = None;
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
    let mut labels = if transient {
        crate::session_source::Service::transient_codex(config.codex.transient_names)
    } else if config.codex.usage.enabled {
        crate::session_source::Service::shared(
            usage.as_ref().expect("resident usage service").names(),
        )
    } else {
        crate::session_source::Service::codex()
    };
    let mut usage_claude = false;
    let mut about_open = false;
    let mut usage_page = 0usize;
    let mut usage_drawn = None;
    let mut usage_update = 0u64;
    let mut composition: Option<Composition> = None;
    let mut key = None;
    let mut shown_card = None;
    let mut presenter = crate::presenter::Presenter::default();
    let mut layout: Option<input::Layout> = None;
    let mut window_size = (0.0, 0.0);
    let mut displayed = false;
    let mut animation_start = 0;
    let mut monitor_watch = crate::notice::MonitorWatch::new();
    let mut activation = crate::activation::Worker::default();
    let mut audio_cancel = Arc::new(AtomicBool::new(false));
    let mut audio_jobs: Vec<std::thread::JoinHandle<()>> = Vec::new();
    while !stop.load(Ordering::Acquire) {
        let now = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
        if now >= usage_update || usage_changed.swap(false, Ordering::AcqRel) {
            usage_update = now.saturating_add(1000);
            if let (Some(tray), Some(codex)) = (&tray, &usage) {
                let display = config.codex.usage.display;
                let codex_line = (config.codex.usage.enabled || claude.is_none())
                    .then(|| codex.compact(display));
                let claude_line = claude.as_ref().map(|service| service.compact(display));
                let levels = crate::tray_usage::Levels::from_views(
                    &codex.view(),
                    claude.as_ref().map(|service| service.view()).as_ref(),
                    config.tray.graph,
                    display,
                );
                tray.set_usage(codex_line, claude_line, &levels)?;
            }
        }
        if let Some(tray) = &tray {
            for command in tray.commands.try_iter() {
                use crate::resident_request::Effect;
                let now = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
                let effects = {
                    let mut guard = state.lock().map_err(|_| "resident state unavailable")?;
                    crate::resident_request::apply(
                        command,
                        &mut *guard,
                        config_path.unwrap_or_else(|| std::path::Path::new("")),
                        now,
                    )
                };
                for effect in effects {
                    match effect {
                        Effect::Quit => stop.store(true, Ordering::Release),
                        Effect::ShowUsage {
                            claude: selected_claude,
                        } => {
                            let mut guard =
                                state.lock().map_err(|_| "resident state unavailable")?;
                            if guard.queue.selected().is_some_and(|entry| entry.is_usage())
                                && !about_open
                                && usage_claude == selected_claude
                            {
                                guard.queue.dismiss();
                            } else {
                                guard.queue.show_usage();
                                about_open = false;
                                usage_claude = selected_claude;
                                usage_page = 0;
                                if selected_claude {
                                    if let Some(service) = &claude {
                                        service.refresh();
                                    }
                                } else if let Some(service) = &usage {
                                    service.refresh();
                                }
                            }
                            usage_drawn = None;
                            presenter.invalidate();
                        }
                        Effect::ShowAbout => {
                            let mut guard =
                                state.lock().map_err(|_| "resident state unavailable")?;
                            toggle_about(&mut guard.queue, &mut about_open);
                            usage_page = 0;
                            usage_drawn = None;
                            presenter.invalidate();
                        }
                        Effect::ShowNotice(message) => {
                            crate::notice::post(&message);
                            eprintln!("aHerald: {message}");
                        }
                        Effect::Restart { test } => {
                            let (stop, wake) = (stop.clone(), wake.clone());
                            crate::resident_request::restart(
                                test,
                                crate::restart::launch_with_test,
                                move || {
                                    stop.store(true, Ordering::Release);
                                    wake.signal();
                                },
                            );
                        }
                        Effect::OpenConfig => crate::resident_request::open_configuration(
                            config_path.expect("resident configuration path"),
                        ),
                        Effect::ToggleMessenger => {
                            crate::forward::toggle();
                            tray.set_messenger(crate::forward::enabled());
                        }
                        Effect::SwitchesChanged => {
                            let guard = state.lock().map_err(|_| "resident state unavailable")?;
                            tray.set_state(
                                guard.sounds,
                                guard.visuals,
                                guard.paused,
                                guard.warnings.enabled(crate::events::Kind::ContextWarning),
                                guard.warnings.enabled(crate::events::Kind::IdleWarning),
                            );
                        }
                        Effect::CancelAudio | Effect::ResumeAudio => {
                            crate::resident_request::apply_audio_effect(&effect, &mut audio_cancel);
                        }
                        Effect::Redraw => presenter.invalidate(),
                    }
                }
            }
        }
        while let Some(event) = window.poll()? {
            if let XEvent::ButtonPress(event) = event
                && let Some(layout) = layout
            {
                let mut guard = state.lock().map_err(|_| "resident state unavailable")?;
                let point = input::to_pixels(
                    (f64::from(event.event_x), f64::from(event.event_y)),
                    window_size,
                    &layout,
                );
                if let Some(cards) = &mut carousel {
                    let now = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
                    let mut controls = Card::new("", cards.index(), cards.len(), false);
                    controls.text_left = Some(layout.text_left);
                    let action = match event.detail {
                        4 => Some(crate::card::Action::Previous),
                        5 => Some(crate::card::Action::Next),
                        1 => controls
                            .hit_test(layout.width, layout.height, layout.scale, point.0, point.1)
                            .or_else(|| {
                                controls
                                    .body_hit_test(
                                        layout.width,
                                        layout.height,
                                        layout.scale,
                                        point.0,
                                        point.1,
                                    )
                                    .then_some(crate::card::Action::Next)
                            }),
                        _ => None,
                    };
                    match action {
                        Some(crate::card::Action::Previous) => cards.navigate(-1, now),
                        Some(crate::card::Action::Next) => cards.navigate(1, now),
                        Some(crate::card::Action::Dismiss) => {
                            carousel = None;
                            audio_cancel.store(true, Ordering::Release);
                            audio_cancel = Arc::new(AtomicBool::new(false));
                        }
                        None => {}
                    }
                    presenter.invalidate();
                    continue;
                }
                if event.detail == 1 {
                    let is_usage = guard.queue.selected().is_some_and(|entry| entry.is_usage());
                    let body = !is_usage
                        && input::on_body(
                            &guard.queue,
                            config.notifications.click_to_dismiss,
                            &layout,
                            point,
                        );
                    if body {
                        super::activation::note_click(event.time);
                    }
                    if let Some(selected) = guard.queue.selected() {
                        let directory = config_path
                            .and_then(Path::parent)
                            .unwrap_or_else(|| Path::new("."));
                        activation.submit(
                            &selected.event,
                            config.notifications.activate_agent_on_click,
                            body,
                            directory,
                            config.general.logging,
                        );
                    }
                }
                let dismiss = config.notifications.click_to_dismiss
                    || guard.queue.selected().is_some_and(|entry| entry.is_usage());
                if event.detail == 1 && input::click(&mut guard.queue, dismiss, &layout, point) {
                    presenter.invalidate();
                } else if event.detail == 4
                    && guard.queue.selected().is_some_and(|entry| entry.is_usage())
                {
                    usage_page = usage_page.saturating_sub(1);
                    usage_drawn = None;
                } else if event.detail == 5
                    && guard.queue.selected().is_some_and(|entry| entry.is_usage())
                {
                    usage_page = usage_page.saturating_add(1);
                    usage_drawn = None;
                } else if event.detail == 4 {
                    guard.queue.next(-1);
                    presenter.invalidate();
                } else if event.detail == 5 {
                    guard.queue.next(1);
                    presenter.invalidate();
                }
            } else if matches!(event, XEvent::Expose(_)) {
                presenter.invalidate();
            }
        }
        let now = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
        audio_jobs.retain(|job| !job.is_finished());
        let (mut entry, mut index, mut count, paused, visuals, mut fresh, sounds) = {
            let mut guard = state.lock().map_err(|_| "resident state unavailable")?;
            let paused = guard.paused;
            if let Some(demo) = &mut demo {
                demo.tick(now, paused, &mut guard.queue);
            }
            crate::resident_request::tick(&mut *guard, &config, now);
            guard.queue.tick(now, paused);
            let room = crate::presenter::sound_slots(audio_jobs.len());
            let fresh = if !transient && !paused {
                guard.queue.take_unannounced(room)
            } else {
                Vec::new()
            };
            (
                guard.queue.selected().cloned(),
                guard.queue.selected_index(),
                guard.queue.visible_count(),
                paused,
                guard.visuals,
                fresh,
                guard.sounds,
            )
        };
        if carousel.is_some() && entry.is_some() {
            carousel = None;
            audio_cancel.store(true, Ordering::Release);
            audio_cancel = Arc::new(AtomicBool::new(false));
        }
        if let Some(cards) = &mut carousel {
            cards.tick(now, paused);
            let selected = cards.selected().clone();
            if test_sound != Some(selected.id) && !paused {
                audio_cancel.store(true, Ordering::Release);
                audio_cancel = Arc::new(AtomicBool::new(false));
                fresh = vec![selected.clone()];
                test_sound = Some(selected.id);
            }
            entry = Some(selected);
            index = cards.index();
            count = cards.len();
        }
        let mut audio_config = config.clone();
        audio_config.sounds.enabled = sounds;
        for fresh in fresh {
            if !fresh.outputs.sound {
                continue;
            }
            if let Some(job) =
                crate::audio::start_controlled(&fresh.event, &audio_config, audio_cancel.clone())
            {
                audio_jobs.push(job);
            }
        }
        if transient && entry.is_none() {
            break;
        }
        if let Some(entry) =
            entry.filter(|entry| !paused && (carousel.is_some() || visuals || entry.is_usage()))
        {
            let monitors = window.monitors()?;
            monitor_watch.check(config.notifications.monitor, monitors.len());
            let pointer = window.pointer()?;
            let monitor = x11::select_monitor(
                &monitors,
                config.notifications.monitor,
                config.notifications.position,
                pointer,
            )
            .ok_or("X11: no monitor")?;
            let scale = config.notifications.scale * monitor.scale;
            if entry.is_usage() {
                let text = if about_open {
                    crate::about_card_text()
                } else {
                    if usage_claude {
                        claude.as_ref().map(|service| service.card_text())
                    } else {
                        usage.as_ref().map(|service| service.card_text())
                    }
                    .unwrap_or_else(|| "Usage unavailable".into())
                };
                let (title, body) = text.split_once('\n').unwrap_or((&text, ""));
                let pages = crate::usage_service::card_pages(body);
                usage_page = usage_page.min(pages.len() - 1);
                let usage_key = (
                    text.clone(),
                    usage_page,
                    index,
                    count,
                    scale.to_bits(),
                    monitor.area,
                );
                if usage_drawn.as_ref() != Some(&usage_key) {
                    let title = if pages.len() > 1 {
                        format!("{title}  ({}/{})", usage_page + 1, pages.len())
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
                    let height =
                        ((114.0 + 24.0 * pages[usage_page].lines().count() as f64) * scale) as i32;
                    let surface = Surface::card(
                        width,
                        height,
                        scale,
                        (22.0 * scale) as i32,
                        crate::card_paint::CardText {
                            heading: &controls.heading,
                            label: &pages[usage_page],
                            body: true,
                            index,
                            count,
                            dismiss_button: false,
                            age: None,
                            color: crate::config::Color([80, 165, 250]),
                            background_color: config.usage_background_color(),
                            border: config.usage_border_color(),
                        },
                    )?;
                    let (x, y, w, h) = crate::placement::layout_rendered(
                        monitor.area,
                        (width, height),
                        scale,
                        config.notifications.position,
                        pointer,
                    );
                    let resized;
                    let pixels = if (w, h) == (width, height) {
                        surface.pixels()
                    } else {
                        let image = image::RgbaImage::from_raw(
                            width as u32,
                            height as u32,
                            surface.pixels().to_vec(),
                        )
                        .ok_or("invalid usage card pixels")?;
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
                        u16::try_from(w).map_err(|_| "usage card too wide")?,
                        u16::try_from(h).map_err(|_| "usage card too tall")?,
                        pixels,
                        255,
                    )?;
                    layout = Some(input::Layout {
                        width,
                        height,
                        scale,
                        text_left: (22.0 * scale) as i32,
                    });
                    window_size = (f64::from(w), f64::from(h));
                    displayed = true;
                    usage_drawn = Some(usage_key);
                    key = None;
                    presenter.invalidate();
                }
                block(&window, &wake, &stop, true, false)?;
                continue;
            }
            usage_drawn = None;
            let heading = if let Some(cards) = &carousel {
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
                labels.label(&entry.event)
            };
            let age = crate::card::age_text(now.saturating_sub(entry.arrived_ms));
            let controls = Card::new(
                heading,
                index,
                count,
                carousel.is_none() && config.notifications.click_to_dismiss,
            );
            let card_key = (controls.heading.clone(), index, count, age);
            let new_key = (entry.id, scale.to_bits(), monitor.area);
            if key != Some(new_key) {
                let art = crate::artwork::prepared_for_event(&config, &entry.event, scale)
                    .ok()
                    .flatten()
                    .map(Arc::new);
                let text_width = Surface::event_text_width(&entry.event, scale)?;
                composition = Some(Composition::compose_with_text_width(
                    scale,
                    art,
                    text_width,
                    |w, h, left| {
                        Surface::card(
                            w,
                            h,
                            scale,
                            left,
                            event_card_text(&config, &entry.event, &controls, Some(&card_key.3)),
                        )
                    },
                )?);
                key = Some(new_key);
                shown_card = Some(card_key.clone());
                animation_start = now;
                presenter.invalidate();
            } else if shown_card.as_ref() != Some(&card_key) {
                if let Some(c) = composition.as_mut() {
                    c.update_card(|w, h, left| {
                        Surface::card(
                            w,
                            h,
                            scale,
                            left,
                            event_card_text(&config, &entry.event, &controls, Some(&card_key.3)),
                        )
                    })?;
                }
                shown_card = Some(card_key);
                presenter.invalidate();
            }
            let c = composition.as_mut().ok_or("missing composed card")?;
            if let Some(shown) = presenter.next(&crate::presenter::Input {
                entry: &entry,
                durations: &c.durations,
                looping: c.looping,
                animation_ms: now.saturating_sub(animation_start),
                fade_in_ms: if carousel.is_some() {
                    0
                } else {
                    config.notifications.fade_in_ms
                },
                fade_out_ms: if carousel.is_some() {
                    0
                } else {
                    config.notifications.fade_out_ms
                },
            }) {
                let (x, y, w, h) = crate::placement::layout_rendered(
                    monitor.area,
                    c.size(),
                    scale,
                    config.notifications.position,
                    pointer,
                );
                let text_left = c.text_left;
                let surface = c.render(shown.frame, shown.seconds, scale)?;
                let (source_w, source_h) = (surface.width(), surface.height());
                let resized;
                let pixels = if (w, h) == (source_w, source_h) {
                    surface.pixels()
                } else {
                    let image = image::RgbaImage::from_raw(
                        source_w as u32,
                        source_h as u32,
                        surface.pixels().to_vec(),
                    )
                    .ok_or("invalid card pixels")?;
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
                    u16::try_from(w).map_err(|_| "card too wide")?,
                    u16::try_from(h).map_err(|_| "card too tall")?,
                    pixels,
                    shown.alpha,
                )?;
                layout = Some(input::Layout {
                    width: source_w,
                    height: source_h,
                    scale,
                    text_left,
                });
                window_size = (f64::from(w), f64::from(h));
                displayed = true;
            }
        } else {
            if displayed {
                window.hide()?;
                displayed = false;
            }
            key = None;
            usage_drawn = None;
            composition = None;
            layout = None;
            presenter.invalidate();
        }
        // A frame while something is shown or queued; otherwise until work arrives, with a slow
        // beat only if usage polling or a session-warning deadline keeps the tray fresh.
        let busy = displayed
            || carousel.is_some()
            || demo.is_some()
            || !audio_jobs.is_empty()
            || state
                .lock()
                .map_or(true, |guard| !guard.queue.items().is_empty());
        let housekeeping = crate::resident_request::idle_poll_needed(
            usage.is_some() && config.codex.usage.enabled,
            claude.is_some(),
            state
                .lock()
                .is_ok_and(|guard| guard.warnings.has_deadline()),
        );
        block(&window, &wake, &stop, busy, housekeeping)?;
    }
    audio_cancel.store(true, Ordering::Release);
    Ok(())
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
