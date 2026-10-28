//! Resident lifecycle; IPC never waits for notification presentation.
use crate::{
    config::Config,
    ipc::{Response, pipe},
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

#[cfg(test)]
use crate::events::Event;
#[cfg(test)]
use crate::ipc::Request;
pub use crate::resident_client::{ensure, forward, forward_if_running};

pub struct Shared {
    pub queue: crate::queue::Queue,
    pub paused: bool,
    pub sounds: bool,
    pub visuals: bool,
    /// Raw handle of the resident window (0 until created); IPC posts `WAKE` to it.
    pub window: isize,
}
impl crate::resident_request::Controls for Shared {
    fn queue(&mut self) -> &mut crate::queue::Queue {
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
/// Posted to the resident window when IPC changed the queue or requested shutdown.
pub const WAKE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 1;
pub(crate) fn wake(window: isize) {
    use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::PostMessageW};
    if window != 0 {
        // SAFETY: posting to a stale or destroyed handle fails harmlessly.
        unsafe {
            let _ = PostMessageW(
                Some(HWND(window as *mut std::ffi::c_void)),
                WAKE,
                WPARAM(0),
                LPARAM(0),
            );
        }
    }
}
#[path = "resident_overlay.rs"]
mod overlay;
/// Preview in this process without acquiring the resident mutex or exposing IPC.
pub fn run_demo(mut config: Config, path: &Path, pause_sec: u64) -> Result<(), String> {
    let demo = crate::demo::Demo::new(&config, pause_sec)?;
    // Demo cards remain clearly labelled while using the supported agent's artwork.
    if !config.agents.contains_key("demo")
        && let Some(assets) = config.agents.get("codex").cloned()
    {
        config.agents.insert("demo".into(), assets);
    }
    let shared = Arc::new(std::sync::Mutex::new(Shared {
        queue: crate::queue::Queue::new(64, config.general.deduplicate_ms),
        paused: false,
        sounds: config.sounds.enabled,
        visuals: config.notifications.enabled,
        window: 0,
    }));
    let usage = crate::usage_service::Service::codex(
        config.codex.usage.enabled,
        Duration::from_secs(config.codex.usage.refresh_interval_sec),
    );
    overlay::run_preview(
        config,
        shared,
        Arc::new(AtomicBool::new(false)),
        Instant::now(),
        path.to_owned(),
        usage,
        Some(demo),
    )
}

pub fn run(scope: Scope, config: Config, path: &Path) -> Result<(), String> {
    let enabled = config.codex.usage.enabled;
    let interval = Duration::from_secs(config.codex.usage.refresh_interval_sec);
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
        window: 0,
    }));
    let (ready_tx, ready_rx) = mpsc::sync_channel(1);
    let state = shared.clone();
    let shutdown = stop.clone();
    let stop_event = Arc::new(pipe::StopEvent::new()?);
    let listener_stop = stop_event.clone();
    let settings = config.clone();
    let log_directory = path.parent().unwrap_or_else(|| Path::new(".")).to_owned();
    let listener = std::thread::spawn(move || {
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
        let mut snapshot_clock = crate::perf::Every::new(Duration::from_secs(60));
        while !shutdown.load(Ordering::Acquire) {
            crate::perf::tick();
            if snapshot_clock.due(std::time::Instant::now()) {
                crate::process_stats::log_snapshot();
            }
            let result = server.serve_until(&listener_stop, |request| {
                let _timer = crate::perf::span(crate::perf::Span::IpcHandle);
                let Ok(mut state) = state.lock() else {
                    return Response::Rejected;
                };
                let now = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
                let (response, effects) =
                    crate::resident_request::respond(request, &mut *state, &settings, now);
                let window = state.window;
                drop(state);
                if effects.quit {
                    shutdown.store(true, Ordering::Release);
                }
                if effects.wake {
                    wake(window);
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
    });
    let startup = ready_rx
        .recv_timeout(Duration::from_secs(1))
        .unwrap_or_else(|_| Err("resident listener startup deadline exceeded".into()));
    if let Err(error) = startup {
        stop.store(true, Ordering::Release);
        stop_event.signal();
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
    stop_event.signal();
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
                Some(WPARAM(crate::tray::SUMMARY)),
                Some(LPARAM(0)),
            );
        }
        unsafe {
            assert!(
                IsWindowVisible(overlay).as_bool(),
                "usage card works even when event visuals are disabled"
            );
            assert!(
                window_for(thread_id, "aHerald.Information").is_invalid(),
                "usage must not create a separate details window"
            );
            assert_ne!(
                GetWindowLongW(overlay, GWL_EXSTYLE) as u32 & WS_EX_LAYERED.0,
                0
            );
            SendMessageW(
                overlay,
                WM_LBUTTONUP,
                Some(WPARAM(0)),
                Some(LPARAM(30 | (60 << 16))),
            );
            assert!(
                !IsWindowVisible(overlay).as_bool(),
                "body click closes usage card"
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
            !unsafe { IsWindow(Some(overlay)) }.as_bool(),
            "quit must close usage details"
        );
    }
}

#[cfg(test)]
mod usage_preview {
    use super::*;
    #[test]
    #[ignore = "interactive synthetic usage card, closes on dismissal or after 180 seconds"]
    fn manual_usage_card_preview() {
        use crate::app_server::{Error, Transport};
        use serde_json::{Value, json};
        use windows::Win32::{
            Foundation::*, System::Threading::GetCurrentThreadId, UI::WindowsAndMessaging::*,
        };
        struct Mock(Option<Value>);
        impl Transport for Mock {
            fn send(&mut self, v: &Value, _: Duration) -> Result<(), Error> {
                if let Some(id) = v.get("id") {
                    let now = crate::usage_service::unix_now();
                    let result = if v["method"] == "account/rateLimits/read" {
                        json!({"rateLimits":{"planType":"plus","primary":{"usedPercent":8,"resetsAt":now+8700},"secondary":{"usedPercent":14,"resetsAt":now+432000},"credits":{"hasCredits":false,"unlimited":false,"balance":"0"}},"ordinaryUsageAllowed":true,"rateLimitResetCredits":{"availableCount":2,"credits":[{"id":"one","title":"Full reset (Weekly + 5 hr)","status":"available","expiresAt":1793296620},{"id":"two","title":"Full reset (Weekly + 5 hr)","status":"available","expiresAt":1793997360}]}})
                    } else {
                        json!({})
                    };
                    self.0 = Some(json!({"id":id,"result":result}));
                }
                Ok(())
            }
            fn receive(&mut self, _: Duration) -> Result<Value, Error> {
                self.0.take().ok_or(Error::Timeout)
            }
        }
        let scope = Scope::current(&format!("usage-preview-{}", std::process::id())).unwrap();
        let stop_scope = scope.clone();
        let thread_id = unsafe { GetCurrentThreadId() };
        let opener = std::thread::spawn(move || {
            unsafe extern "system" fn find(hwnd: HWND, p: LPARAM) -> windows::core::BOOL {
                unsafe {
                    let mut c = [0u16; 64];
                    let n = GetClassNameW(hwnd, &mut c);
                    if String::from_utf16_lossy(&c[..n as usize]) == "aHerald.Resident" {
                        *(p.0 as *mut HWND) = hwnd;
                    }
                }
                true.into()
            }
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let mut hwnd = HWND::default();
                unsafe {
                    let _ = EnumThreadWindows(
                        thread_id,
                        Some(find),
                        LPARAM(&mut hwnd as *mut _ as isize),
                    );
                }
                if !hwnd.is_invalid() {
                    unsafe {
                        let style = GetWindowLongW(hwnd, GWL_EXSTYLE) as u32;
                        SetWindowLongW(
                            hwnd,
                            GWL_EXSTYLE,
                            ((style & !WS_EX_TOOLWINDOW.0) | WS_EX_APPWINDOW.0) as i32,
                        );
                        let _ = SendMessageW(
                            hwnd,
                            WM_COMMAND,
                            Some(WPARAM(crate::tray::SUMMARY)),
                            Some(LPARAM(0)),
                        );
                    }
                    break;
                }
                assert!(Instant::now() < deadline);
                std::thread::yield_now();
            }
            let _ = pipe::request(
                &stop_scope,
                &Request::Event {
                    event: Event::demo("permission_required").unwrap(),
                },
                Duration::from_millis(300),
            );
            let deadline = Instant::now() + Duration::from_secs(180);
            while Instant::now() < deadline {
                if matches!(
                    pipe::request(&stop_scope, &Request::Status, Duration::from_millis(100)),
                    Ok(Response::Status { queued: 0, .. })
                ) {
                    break;
                }
                std::thread::park_timeout(Duration::from_millis(200));
            }
            let _ = pipe::request(&stop_scope, &Request::Quit, Duration::from_millis(300));
        });
        let mut config = Config::default();
        config.sounds.enabled = false;
        run_with_usage(scope, config, Path::new("usage-preview.toml"), || {
            crate::usage_service::Service::spawn(
                true,
                Duration::from_secs(300),
                Arc::new(AtomicBool::new(false)),
                || Ok(Mock(None)),
            )
        })
        .unwrap();
        opener.join().unwrap();
    }
}

#[cfg(test)]
mod idle_tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use windows::Win32::{
        Foundation::*, System::Threading::GetCurrentThreadId, UI::WindowsAndMessaging::*,
    };
    static TIMERS: AtomicUsize = AtomicUsize::new(0);
    unsafe extern "system" fn count(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
        // SAFETY: for WH_GETMESSAGE with code >= 0, `lp` points to the retrieved MSG.
        unsafe {
            if code >= 0 && (*(lp.0 as *const MSG)).message == WM_TIMER {
                TIMERS.fetch_add(1, Ordering::SeqCst);
            }
            CallNextHookEx(None, code, wp, lp)
        }
    }
    fn resident_window(thread_id: u32) -> HWND {
        unsafe extern "system" fn find(hwnd: HWND, param: LPARAM) -> windows::core::BOOL {
            unsafe {
                let mut text = [0u16; 64];
                let n = GetClassNameW(hwnd, &mut text);
                if String::from_utf16_lossy(&text[..n as usize]) == "aHerald.Resident" {
                    *(param.0 as *mut HWND) = hwnd;
                }
                true.into()
            }
        }
        let mut window = HWND::default();
        unsafe {
            let _ = EnumThreadWindows(
                thread_id,
                Some(find),
                LPARAM(&mut window as *mut _ as isize),
            );
        }
        window
    }
    #[test]
    fn idle_resident_has_no_periodic_wakeups_and_shows_a_forwarded_event_promptly() {
        let scope = Scope::current(&format!("idle-wakeups-{}", std::process::id())).unwrap();
        let thread_scope = scope.clone();
        let mut config = Config::default();
        config.sounds.enabled = false;
        let (thread_tx, thread_rx) = mpsc::sync_channel(1);
        let worker = std::thread::spawn(move || {
            thread_tx.send(unsafe { GetCurrentThreadId() }).unwrap();
            run_with_usage(thread_scope, config, Path::new("idle-test.toml"), || {
                crate::usage_service::Service::codex(false, Duration::from_secs(300))
            })
        });
        let thread_id = thread_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        let window = loop {
            let window = resident_window(thread_id);
            if !window.is_invalid()
                && pipe::request(&scope, &Request::Status, Duration::from_millis(50)).is_ok()
            {
                break window;
            }
            assert!(Instant::now() < deadline, "resident did not start");
        };
        let hook =
            unsafe { SetWindowsHookExW(WH_GETMESSAGE, Some(count), None, thread_id) }.unwrap();
        // Let startup settle, then observe a quiet period.
        std::thread::sleep(Duration::from_millis(200));
        TIMERS.store(0, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(600));
        let idle_timers = TIMERS.load(Ordering::SeqCst);
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
        let shown = Instant::now();
        while !unsafe { IsWindowVisible(window) }.as_bool() {
            assert!(
                shown.elapsed() < Duration::from_secs(1),
                "event not shown promptly"
            );
            std::thread::yield_now();
        }
        unsafe {
            let _ = UnhookWindowsHookEx(hook);
        }
        let _ = pipe::request(&scope, &Request::Quit, Duration::from_millis(500));
        worker.join().unwrap().unwrap();
        assert_eq!(idle_timers, 0, "an idle resident must not run a timer");
    }
}
