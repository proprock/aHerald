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
fn shared(config: &Config) -> Arc<Mutex<Shared>> {
    Arc::new(Mutex::new(Shared {
        queue: Queue::new(64, config.general.deduplicate_ms),
        paused: false,
        sounds: config.sounds.enabled,
        visuals: config.notifications.enabled,
    }))
}

/// About is an application card, not an error or an agent event.
fn about_event() -> Event {
    let mut event = Event::notice(&format!("Version {}", crate::VERSION));
    event.kind = crate::events::Kind::TurnComplete;
    event
}

fn is_about(event: &Event) -> bool {
    let about = about_event();
    event.source == about.source && event.kind == about.kind && event.message == about.message
}

fn about_heading(event: &Event) -> crate::card::Heading {
    let mut heading = crate::card::Heading::notice(event.message.as_deref().unwrap_or_default());
    heading.title = crate::NAME.into();
    heading
}

fn toggle_about(queue: &mut Queue, now: u64) -> crate::queue::Outcome {
    let about = about_event();
    if queue.selected().is_some_and(|entry| is_about(&entry.event)) {
        queue.dismiss();
        crate::queue::Outcome::Resolved
    } else {
        queue.push(about, now, 0)
    }
}

pub fn run(scope: Scope, config: Config, path: &Path) -> Result<(), String> {
    let Some(_owner) = scope.acquire()? else {
        return Ok(());
    };
    // Open the display before reporting the listener ready.
    let window = x11::Window::new()?;
    let state = shared(&config);
    let stop = Arc::new(AtomicBool::new(false));
    let started = Instant::now();
    crate::notice::install(crate::resident_request::notice_sink(
        state.clone(),
        config.clone(),
        started,
        |_: &Shared| {},
    ));
    let stop_event = Arc::new(pipe::StopEvent::new()?);
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let listener = {
        let (state, stop, stop_event, settings) = (
            state.clone(),
            stop.clone(),
            stop_event.clone(),
            config.clone(),
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
                    if effects.rejected {
                        log(Record::QueueRejected);
                    }
                    if let Some(reason) = effects.dropped {
                        log(Record::EventDropped(reason));
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
    let result = match startup {
        Ok(()) => present(
            window,
            config,
            state,
            stop.clone(),
            started,
            None,
            Some(path),
        ),
        Err(e) => Err(e),
    };
    crate::notice::clear();
    stop.store(true, Ordering::Release);
    stop_event.signal();
    let _ = listener.join();
    result
}

pub fn run_demo(mut config: Config, path: &Path, pause_sec: u64) -> Result<(), String> {
    let demo = crate::demo::Demo::new(&config, pause_sec)?;
    if !config.agents.contains_key("demo")
        && let Some(assets) = config.agents.get("codex").cloned()
    {
        config.agents.insert("demo".into(), assets);
    }
    let state = shared(&config);
    present(
        x11::Window::new()?,
        config,
        state,
        Arc::new(AtomicBool::new(false)),
        Instant::now(),
        Some(demo),
        Some(path),
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
        Arc::new(AtomicBool::new(false)),
        Instant::now(),
        None,
        None,
    )
}

fn present(
    window: x11::Window,
    config: Config,
    state: Arc<Mutex<Shared>>,
    stop: Arc<AtomicBool>,
    start: Instant,
    mut demo: Option<crate::demo::Demo>,
    config_path: Option<&Path>,
) -> Result<(), String> {
    let transient = config_path.is_none();
    let tray = config_path.map(|_| {
        super::tray::Tray::start(config.sounds.enabled, config.notifications.enabled, false)
    });
    let mut labels = if transient {
        crate::session_source::Service::transient_codex(config.codex.transient_names)
    } else {
        crate::session_source::Service::codex()
    };
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
        if let Some(tray) = &tray {
            for command in tray.commands.try_iter() {
                use super::tray::Command;
                match command {
                    Command::Exit => stop.store(true, Ordering::Release),
                    Command::About => {
                        let mut guard = state.lock().map_err(|_| "resident state unavailable")?;
                        let now = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
                        if !toggle_about(&mut guard.queue, now).is_accepted() {
                            crate::operational_log::log(
                                crate::operational_log::Record::QueueRejected,
                            );
                        }
                        presenter.invalidate();
                    }
                    Command::Restart => {
                        if let Err(error) =
                            Config::load(config_path.expect("resident configuration path"), true)
                        {
                            crate::notice::post("Restart cancelled: invalid configuration");
                            eprintln!("aHerald: {error}");
                        } else {
                            let stop = stop.clone();
                            std::thread::spawn(move || match crate::restart::launch() {
                                Ok(()) => stop.store(true, Ordering::Release),
                                Err(error) => eprintln!("aHerald: {error}"),
                            });
                        }
                    }
                    Command::Config => {
                        let path = config_path.expect("resident configuration path").to_owned();
                        std::thread::spawn(move || {
                            if let Err(error) = crate::shell::edit_configuration(&path) {
                                eprintln!("aHerald: {error}");
                            }
                        });
                    }
                    Command::Sound | Command::Visual | Command::Pause => {
                        let mut guard = state.lock().map_err(|_| "resident state unavailable")?;
                        let now = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
                        let paused = guard.paused;
                        guard.queue.tick(now, paused);
                        match command {
                            Command::Sound => guard.sounds = !guard.sounds,
                            Command::Visual => guard.visuals = !guard.visuals,
                            Command::Pause => guard.paused = !guard.paused,
                            _ => unreachable!(),
                        }
                        if !guard.sounds || guard.paused {
                            audio_cancel.store(true, Ordering::Release);
                        } else if audio_cancel.load(Ordering::Acquire) {
                            audio_cancel = Arc::new(AtomicBool::new(false));
                        }
                        tray.set_state(guard.sounds, guard.visuals, guard.paused);
                        presenter.invalidate();
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
                if event.detail == 1 {
                    let body = input::on_body(
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
                if event.detail == 1
                    && input::click(
                        &mut guard.queue,
                        config.notifications.click_to_dismiss,
                        &layout,
                        point,
                    )
                {
                    presenter.invalidate();
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
        let (entry, index, count, paused, visuals, fresh, sounds) = {
            let mut guard = state.lock().map_err(|_| "resident state unavailable")?;
            let paused = guard.paused;
            if let Some(demo) = &mut demo {
                demo.tick(now, paused, &mut guard.queue);
            }
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
                guard.queue.items().len(),
                paused,
                guard.visuals,
                fresh,
                guard.sounds,
            )
        };
        let mut audio_config = config.clone();
        audio_config.sounds.enabled = sounds;
        for fresh in fresh {
            if let Some(job) =
                crate::audio::start_controlled(&fresh.event, &audio_config, audio_cancel.clone())
            {
                audio_jobs.push(job);
            }
        }
        if transient && entry.is_none() {
            break;
        }
        if let Some(entry) = entry.filter(|entry| !paused && (visuals || is_about(&entry.event))) {
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
            let heading = if is_about(&entry.event) {
                about_heading(&entry.event)
            } else {
                labels.label(&entry.event)
            };
            let age = crate::card::age_text(now.saturating_sub(entry.arrived_ms));
            let controls = Card::new(heading, index, count, config.notifications.click_to_dismiss);
            let card_key = (controls.heading.clone(), index, count, age);
            let new_key = (entry.id, scale.to_bits(), monitor.area);
            if key != Some(new_key) {
                let art = crate::artwork::prepared_for_event(&config, &entry.event, scale)
                    .ok()
                    .flatten()
                    .map(Arc::new);
                composition = Some(Composition::compose(scale, art, |w, h, left| {
                    Surface::card(
                        w,
                        h,
                        scale,
                        left,
                        event_card_text(&config, &entry.event, &controls, Some(&card_key.3)),
                    )
                })?);
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
                fade_in_ms: config.notifications.fade_in_ms,
                fade_out_ms: config.notifications.fade_out_ms,
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
            composition = None;
            layout = None;
            presenter.invalidate();
        }
        std::thread::sleep(Duration::from_millis(16));
    }
    audio_cancel.store(true, Ordering::Release);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn about_is_persistent_green_and_dismissible() {
        let mut queue = Queue::new(8, 0);
        assert_eq!(toggle_about(&mut queue, 0), crate::queue::Outcome::Queued);
        let entry = queue.selected().unwrap();
        assert_eq!(entry.duration, 0);
        assert_eq!(entry.event.kind, crate::events::Kind::TurnComplete);
        let heading = about_heading(&entry.event);
        assert_eq!(heading.title, "aHerald");
        assert_eq!(heading.name.as_deref(), entry.event.message.as_deref());
        assert_eq!(
            Config::default().event_color(entry.event.kind).0,
            [65, 210, 140]
        );
        assert_eq!(
            entry.event.message.as_deref(),
            Some(format!("Version {}", crate::VERSION).as_str())
        );
        queue.tick(0, false);
        queue.tick(120_000, false);
        assert!(queue.selected().is_some());
        assert_eq!(
            toggle_about(&mut queue, 120_000),
            crate::queue::Outcome::Resolved
        );
        assert!(queue.selected().is_none());
    }
}
