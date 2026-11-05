#![cfg(windows)]
#![allow(clippy::undocumented_unsafe_blocks)] // TODO(AH-249): write the SAFETY comments of these Windows test helpers.
mod support;
use std::{
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use windows::Win32::{Foundation::*, UI::WindowsAndMessaging::*};

struct Owned(Child, std::path::PathBuf);
impl Drop for Owned {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
        let _ = std::fs::remove_dir_all(&self.1);
    }
}
struct Search {
    pid: u32,
    window: HWND,
}
unsafe extern "system" fn find(hwnd: HWND, data: LPARAM) -> windows::core::BOOL {
    // SAFETY: synchronous callback receives its caller's live Search.
    unsafe {
        let search = &mut *(data.0 as *mut Search);
        let mut pid = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        if pid == search.pid && IsWindowVisible(hwnd).as_bool() {
            search.window = hwnd;
            return false.into();
        }
    }
    true.into()
}

#[test]
fn invalid_startup_config_shows_a_notice_then_exits_but_hook_stays_prompt_and_silent() {
    let root = std::env::temp_dir().join(format!("aherald-config-notice-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("config.toml");
    std::fs::write(&path, "[notifications]\nmonitor='PRIVATE_INVALID_VALUE'\n").unwrap();
    let mut owned = Owned(
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .arg("--config")
            .arg(&path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
        root.clone(),
    );
    let mut search = Search {
        pid: owned.0.id(),
        window: HWND::default(),
    };
    let deadline = Instant::now() + Duration::from_secs(5);
    while search.window.is_invalid() {
        // SAFETY: search lives for the synchronous enumeration.
        unsafe {
            let _ = EnumWindows(Some(find), LPARAM(&mut search as *mut Search as isize));
        }
        assert!(
            owned.0.try_wait().unwrap().is_none(),
            "startup exited without a configuration notice"
        );
        assert!(
            Instant::now() < deadline,
            "configuration notice did not appear"
        );
        std::thread::yield_now();
    }
    let shown = Instant::now();
    // SAFETY: inspect only the owned process's window.
    unsafe {
        let mut title = [0u16; 256];
        let length = GetWindowTextW(search.window, &mut title);
        let title = String::from_utf16_lossy(&title[..length as usize]);
        assert!(title.contains("aHerald notice"), "{title}");
        assert!(!title.contains("PRIVATE"));
        assert_ne!(GetForegroundWindow(), search.window);
        assert_ne!(
            GetWindowLongPtrW(search.window, GWL_EXSTYLE) as u32 & WS_EX_NOACTIVATE.0,
            0
        );
    }
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        if let Some(status) = owned.0.try_wait().unwrap() {
            assert_eq!(status.code(), Some(2));
            break;
        }
        assert!(
            Instant::now() < deadline,
            "configuration notice did not finish failed startup"
        );
        std::thread::yield_now();
    }
    assert!(
        shown.elapsed() >= Duration::from_millis(7500),
        "configuration notice did not stay for eight seconds"
    );
    use std::io::Read;
    let mut stdout = Vec::new();
    owned
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_end(&mut stdout)
        .unwrap();
    assert!(
        stdout.is_empty(),
        "startup must not emit a child readiness acknowledgement"
    );

    let before = Instant::now();
    let output = support::run_bounded_with_input(
        Command::new(env!("CARGO_BIN_EXE_aherald"))
            .args(["--hook", "--config"])
            .arg(&path)
            .stdin(Stdio::piped()),
        include_bytes!("fixtures/stop.json"),
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(output.status.success());
    assert_eq!(output.stdout, b"{}\n");
    assert!(
        before.elapsed() < Duration::from_secs(2),
        "hook waited for a notice"
    );
    let text = std::fs::read_to_string(root.join("aherald.log")).unwrap();
    assert_eq!(text.matches("config.invalid").count(), 2, "{text}");
    assert!(!text.contains("PRIVATE"));
    assert!(!text.contains(&root.to_string_lossy().to_string()));
}
