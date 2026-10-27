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
    let enabled = config.usage.enabled;
    let interval = Duration::from_secs(config.usage.refresh_interval_sec);
    run_with_usage(scope, config, path, move || {
        crate::usage_service::Service::codex(enabled, interval)
    })
}
fn run_with_usage(
    scope: Scope,
    config: Config,
    path: &Path,
    usage: impl FnOnce() -> crate::usage_service::Service,
) -> Result<(), String> {
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
    let result = overlay::run(
        config,
        shared,
        stop.clone(),
        start,
        path.to_owned(),
        usage(),
    );
    stop.store(true, Ordering::Release);
    let _ = listener.join();
    result
}

#[cfg(test)]
mod usage_tests {
    use super::*;
    use crate::{
        app_server::{Error, Transport},
        usage_service::Service,
    };
    struct Missing;
    impl Transport for Missing {
        fn send(&mut self, _: &serde_json::Value, _: Duration) -> Result<(), Error> {
            Err(Error::Disconnected)
        }
        fn receive(&mut self, _: Duration) -> Result<serde_json::Value, Error> {
            Err(Error::Disconnected)
        }
    }
    #[test]
    fn unavailable_usage_leaves_resident_event_delivery_and_shutdown_responsive() {
        use windows::Win32::{
            Foundation::*, System::Threading::GetCurrentThreadId, UI::WindowsAndMessaging::*,
        };
        let service = Service::spawn(
            true,
            Duration::from_secs(300),
            Arc::new(AtomicBool::new(false)),
            || Err::<Missing, _>(Error::Unavailable),
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while service.view().error.is_none() {
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        let scope = Scope::current(&format!("usage-independent-{}", std::process::id())).unwrap();
        let thread_scope = scope.clone();
        let mut config = Config::default();
        config.sounds.enabled = false;
        config.notifications.enabled = false;
        let (thread_tx, thread_rx) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            thread_tx.send(unsafe { GetCurrentThreadId() }).unwrap();
            run_with_usage(thread_scope, config, Path::new("usage-test.toml"), || {
                service
            })
        });
        struct Cleanup(Scope, Option<std::thread::JoinHandle<Result<(), String>>>);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = pipe::request(&self.0, &Request::Quit, Duration::from_millis(300));
                if let Some(worker) = self.1.take() {
                    let _ = worker.join();
                }
            }
        }
        let mut cleanup = Cleanup(scope.clone(), Some(worker));
        let thread_id = thread_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        fn window_for(thread_id: u32, class: &str) -> HWND {
            struct Search<'a> {
                class: &'a str,
                window: HWND,
            }
            unsafe extern "system" fn find(hwnd: HWND, param: LPARAM) -> windows::core::BOOL {
                unsafe {
                    let search = &mut *(param.0 as *mut Search<'_>);
                    let mut text = [0u16; 128];
                    let count = GetClassNameW(hwnd, &mut text);
                    if String::from_utf16_lossy(&text[..count as usize]) == search.class {
                        search.window = hwnd;
                    }
                    true.into()
                }
            }
            let mut search = Search {
                class,
                window: HWND::default(),
            };
            unsafe {
                let _ = EnumThreadWindows(
                    thread_id,
                    Some(find),
                    LPARAM(&mut search as *mut _ as isize),
                );
            }
            search.window
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            if pipe::request(&scope, &Request::Status, Duration::from_millis(50)).is_ok()
                && !window_for(thread_id, "aHerald.Resident").is_invalid()
            {
                break;
            }
            assert!(Instant::now() < deadline);
        }
        let overlay = window_for(thread_id, "aHerald.Resident");
        unsafe {
            SendMessageW(
                overlay,
                WM_COMMAND,
                Some(WPARAM(crate::tray::DETAILS)),
                Some(LPARAM(0)),
            );
        }
        let details = window_for(thread_id, "aHerald.Information");
        unsafe {
            assert!(
                IsWindowVisible(details).as_bool(),
                "usage details must work while the notification overlay is hidden"
            );
            let child = GetDlgItem(Some(details), 1).unwrap();
            let mut text = [0u16; 1024];
            let count = GetWindowTextW(child, &mut text);
            assert!(String::from_utf16_lossy(&text[..count as usize]).contains("unavailable"));
            SendMessageW(
                overlay,
                WM_COMMAND,
                Some(WPARAM(crate::tray::REFRESH)),
                Some(LPARAM(0)),
            );
        }
        assert!(matches!(
            pipe::request(
                &scope,
                &Request::Event {
                    event: Event::demo("permission_required").unwrap()
                },
                Duration::from_millis(500)
            )
            .unwrap(),
            Response::Accepted
        ));
        assert!(matches!(
            pipe::request(&scope, &Request::Status, Duration::from_millis(500)).unwrap(),
            Response::Status { queued: 1, .. }
        ));
        assert!(matches!(
            pipe::request(&scope, &Request::Quit, Duration::from_millis(500)).unwrap(),
            Response::Bye
        ));
        cleanup.1.take().unwrap().join().unwrap().unwrap();
        assert!(
            !unsafe { IsWindow(Some(details)) }.as_bool(),
            "quit must close usage details"
        );
    }
}
