//! Resident lifecycle; IPC never waits for notification presentation.
use crate::{
    config::Config,
    events::Event,
    ipc::{Request, Response, pipe},
    local::Scope,
};
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

pub fn ensure(exe: &Path, config: &Path, scope: &Scope) -> Result<Response, String> {
    if let Ok(status @ Response::Status { .. }) =
        pipe::request(scope, &Request::Status, Duration::from_millis(30))
    {
        return Ok(status);
    }
    let mut child = crate::detached::spawn(
        exe,
        &[
            std::ffi::OsStr::new("--resident-child"),
            std::ffi::OsStr::new(&scope.namespace),
            std::ffi::OsStr::new("--config"),
            config.as_os_str(),
        ],
        false,
    )?;
    let deadline = Instant::now() + Duration::from_millis(1500);
    while Instant::now() < deadline {
        if let Ok(status @ Response::Status { .. }) =
            pipe::request(scope, &Request::Status, Duration::from_millis(80))
        {
            if let Response::Status { pid, .. } = &status
                && *pid != child.id()
            {
                // A concurrent launch won. Reap our own loser before it can start
                // late and acquire ownership after that resident has been quit.
                let _ = child.kill();
                let _ = child.wait();
            }
            return Ok(status);
        }
        if let Some(status) = child
            .try_wait()
            .map_err(|_| "cannot inspect resident startup")?
            && !status.success()
        {
            return Err("resident startup failed".into());
        }
    }
    let _ = child.kill();
    let _ = child.wait();
    Err("resident startup deadline exceeded".into())
}
pub fn forward(exe: &Path, config: &Path, scope: &Scope, event: &Event) -> Result<(), String> {
    ensure(exe, config, scope)?;
    match pipe::request(
        scope,
        &Request::Event {
            event: event.clone(),
        },
        Duration::from_millis(150),
    )? {
        Response::Accepted => Ok(()),
        _ => Err("resident event queue rejected notification".into()),
    }
}

pub struct Shared {
    pub queue: crate::queue::Queue,
    pub paused: bool,
    pub sounds: bool,
    pub visuals: bool,
}
#[path = "resident_overlay.rs"]
mod overlay;
pub fn run(scope: Scope, config: Config, path: &Path) -> Result<(), String> {
    let Some(_owner) = scope.acquire()? else {
        return Ok(());
    };
    let stop = Arc::new(AtomicBool::new(false));
    let start = Instant::now();
    let shared = Arc::new(std::sync::Mutex::new(Shared {
        queue: crate::queue::Queue::new(64, config.general.deduplicate_ms),
        paused: false,
        sounds: config.sounds.enabled,
        visuals: config.notifications.enabled,
    }));
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let state = shared.clone();
    let shutdown = stop.clone();
    let settings = config.clone();
    let listener = std::thread::spawn(move || {
        let server = match scope.create_pipe() {
            Ok(s) => s,
            Err(e) => {
                let _ = ready_tx.send(Err(e));
                return;
            }
        };
        let _ = ready_tx.send(Ok(()));
        while !shutdown.load(Ordering::Acquire) {
            let _ = server.serve_once(|request| {
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
                        Response::Bye
                    }
                    Request::Event { event } => {
                        if event.validate().is_err() {
                            return Response::Rejected;
                        }
                        let resolved = state.queue.observe_terminal(&event);
                        let setting = &settings.events[event.kind.name()];
                        if !setting.enabled {
                            return Response::Accepted;
                        }
                        let now = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
                        let duplicate = state.queue.is_duplicate(&event, now)
                            || state.queue.is_obsolete(&event);
                        if state.queue.push(
                            event,
                            now,
                            if setting.duration_ms == 0 {
                                0
                            } else {
                                setting
                                    .duration_ms
                                    .saturating_add(settings.notifications.fade_out_ms)
                            },
                        ) || duplicate
                            || resolved > 0
                        {
                            Response::Accepted
                        } else {
                            Response::Rejected
                        }
                    }
                }
            });
        }
    });
    let startup = ready_rx
        .recv_timeout(Duration::from_secs(1))
        .unwrap_or_else(|_| Err("resident listener startup deadline exceeded".into()));
    if let Err(error) = startup {
        stop.store(true, Ordering::Release);
        let _ = listener.join();
        return Err(error);
    }
    let result = overlay::run(config, shared, stop.clone(), start, path.to_owned());
    stop.store(true, Ordering::Release);
    let _ = listener.join();
    result
}
