#![cfg(windows)]
#![allow(clippy::undocumented_unsafe_blocks)] // TODO(AH-249): write the SAFETY comments of these Windows test helpers.
use aherald::{
    config::{Config, LogLevel, Monitor},
    events::Event,
    notice, operational_log,
};
use std::sync::{Arc, Mutex};

#[test]
fn native_monitor_fallback_logs_and_queues_one_notice() {
    let root = std::env::temp_dir().join(format!("aherald-native-notice-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let log = root.join("aherald.log");
    let _ = std::fs::remove_file(&log);
    operational_log::init(&root, LogLevel::Warn);
    let seen = Arc::new(Mutex::new(Vec::new()));
    let received = seen.clone();
    notice::install(move |event| received.lock().unwrap().push(event));
    let mut config = Config::default();
    config.notifications.monitor = Monitor::Index(999);
    config.notifications.fade_in_ms = 0;
    config.notifications.fade_out_ms = 0;
    config.sounds.enabled = false;
    let event = Event::notice("Monitor 999 is not connected; using the main display");
    config.events[event.kind].duration_ms = 100;
    for _ in 0..2 {
        aherald::notification::show(&event, &config).unwrap();
    }
    notice::clear();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0], event);
    let text = std::fs::read_to_string(&log).unwrap();
    assert_eq!(text.matches("display.monitor_missing").count(), 1, "{text}");
    std::fs::remove_file(log).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
#[ignore = "visual inspection of the Windows resident notice, bounded to 180 seconds"]
fn preview_notice() {
    use aherald::{
        ipc::{Request, pipe},
        local::Scope,
    };
    use std::{
        path::PathBuf,
        process::{Child, Command},
        time::{Duration, Instant},
    };
    struct Preview(Child, PathBuf);
    impl Drop for Preview {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
            let _ = std::fs::remove_dir_all(&self.1);
        }
    }
    let name = format!("ah090-preview-{}", std::process::id());
    let root = std::env::temp_dir().join(&name);
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("config.toml");
    std::fs::write(&path, "[notifications]\nmonitor='999'\nposition='center'\nfade_in_ms=0\nfade_out_ms=0\n[sounds]\nenabled=false\n[codex.usage]\nenabled=false\n[claude.usage]\nenabled=false\n").unwrap();
    let mut preview = Preview(
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(["--resident-child", &name, "--config"])
            .arg(path)
            .spawn()
            .unwrap(),
        root,
    );
    let scope = Scope::current(&name).unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if pipe::request(&scope, &Request::Status, Duration::from_millis(100)).is_ok() {
            break;
        }
        assert!(Instant::now() < deadline, "preview resident did not start");
        assert!(preview.0.try_wait().unwrap().is_none());
        std::thread::yield_now();
    }
    pipe::request(
        &scope,
        &Request::Event {
            event: Event::demo("permission_required").unwrap(),
        },
        Duration::from_secs(1),
    )
    .unwrap();
    use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};
    unsafe extern "system" fn expose(hwnd: HWND, param: LPARAM) -> windows::core::BOOL {
        // SAFETY: inspect only this fixture's owned child, without activating its window.
        unsafe {
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == param.0 as u32 && IsWindowVisible(hwnd).as_bool() {
                let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
                SetWindowLongPtrW(
                    hwnd,
                    GWL_EXSTYLE,
                    ((style & !WS_EX_TOOLWINDOW.0) | WS_EX_APPWINDOW.0) as isize,
                );
                let _ = ShowWindow(hwnd, SW_HIDE);
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                return false.into();
            }
        }
        true.into()
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        // SAFETY: synchronous enumeration; the parameter is the owned child's PID.
        if unsafe { EnumWindows(Some(expose), LPARAM(preview.0.id() as isize)) }.is_err() {
            break;
        }
        assert!(Instant::now() < deadline, "preview did not show a window");
        std::thread::yield_now();
    }
    println!("Preview ready: use the card navigation to select aHerald notice");
    let deadline = Instant::now() + Duration::from_secs(180);
    while Instant::now() < deadline && preview.0.try_wait().unwrap().is_none() {
        std::thread::park_timeout(Duration::from_millis(100));
    }
}
