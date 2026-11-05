//! Run with a private D-Bus session and an isolated X11/XWayland display.
#![cfg(target_os = "linux")]
mod support;
use aherald::{
    events::Event,
    ipc::{Request, Response, pipe},
    local::Scope,
};
use std::{
    cell::RefCell,
    os::fd::{AsRawFd, FromRawFd, OwnedFd},
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};
use x11rb::{
    connection::Connection,
    protocol::xproto::{
        BUTTON_PRESS_EVENT, ButtonPressEvent, ConnectionExt, EventMask, ImageFormat, MapState,
    },
    rust_connection::RustConnection,
};

const WAIT: Duration = Duration::from_secs(10);

struct Fixture {
    scope: Scope,
    root: PathBuf,
    config: PathBuf,
    exe: PathBuf,
    processes: RefCell<Vec<OwnedFd>>,
}
impl Fixture {
    fn new(tag: &str) -> Self {
        assert_eq!(
            std::env::var("AH113_ISOLATED").as_deref(),
            Ok("1"),
            "use a private bus and display"
        );
        let scope = Scope::current(&format!("ah113-{tag}-{}", std::process::id())).unwrap();
        let root = std::env::temp_dir().join(format!("ah113-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let exe = root.join("aherald");
        std::fs::copy(env!("CARGO_BIN_EXE_aherald"), &exe).unwrap();
        // Exercise a complete installation: missing built-ins intentionally queue
        // diagnostics, which would interfere with the lifecycle cards under test.
        let assets = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
        for entry in std::fs::read_dir(assets).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_dir() {
                let destination = root.join("assets").join(entry.file_name());
                std::fs::create_dir_all(&destination).unwrap();
                for file in std::fs::read_dir(entry.path()).unwrap() {
                    let file = file.unwrap();
                    std::fs::copy(file.path(), destination.join(file.file_name())).unwrap();
                }
            }
        }
        let config = root.join("settings.toml");
        let fixture = Self {
            scope,
            root,
            config,
            exe,
            processes: RefCell::new(Vec::new()),
        };
        fixture.valid(true);
        fixture
    }
    fn valid(&self, sounds: bool) {
        let mut text = format!(
            "[sounds]\nenabled={sounds}\n[notifications]\nenabled=true\nactivate_agent_on_click=false\nfade_in_ms=0\nfade_out_ms=0\n"
        );
        for name in [
            "turn_complete",
            "permission_required",
            "input_required",
            "error",
            "interrupted",
            "context_warning",
            "idle_warning",
        ] {
            text.push_str(&format!("[events.{name}]\nduration_sec=60\n"));
        }
        std::fs::write(&self.config, text).unwrap();
    }
    fn status(&self) -> Option<(u32, usize, bool)> {
        match pipe::request(&self.scope, &Request::Status, Duration::from_millis(100)).ok()? {
            Response::Status {
                pid,
                queued,
                sounds,
                ..
            } => Some((pid, queued, sounds)),
            _ => None,
        }
    }
    fn start(&self) -> u32 {
        match aherald::resident::ensure(&self.exe, &self.config, &self.scope).unwrap() {
            Response::Status { pid, .. } => {
                self.track(pid);
                pid
            }
            _ => panic!("resident did not start"),
        }
    }
    fn track(&self, pid: u32) {
        // A pidfd refers to this exact process even if its PID is later reused.
        // SAFETY: pidfd_open takes two integers and returns a new descriptor or -1; it reads no
        // memory of ours.
        let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, 0) } as i32;
        if raw < 0 {
            let _ = pipe::request(&self.scope, &Request::Quit, Duration::from_millis(300));
            panic!(
                "cannot open resident pidfd: {}",
                std::io::Error::last_os_error()
            );
        }
        // SAFETY: pidfd_open returned a new owned descriptor.
        let fd = unsafe { OwnedFd::from_raw_fd(raw) };
        let actual = std::fs::read_link(format!("/proc/{pid}/exe"));
        assert!(
            actual.as_deref().is_ok_and(|path| path == self.exe),
            "resident executable ownership mismatch"
        );
        self.processes.borrow_mut().push(fd);
    }
    fn wait(&self, mut check: impl FnMut() -> bool, what: &str) {
        self.wait_for(WAIT, &mut check, what);
    }
    fn wait_for(&self, timeout: Duration, mut check: impl FnMut() -> bool, what: &str) {
        support::wait_until(timeout, Duration::from_millis(30), what, &mut check)
            .unwrap_or_else(|error| panic!("{error}; resident status: {:?}", self.status()));
    }
    fn tray_name(&self, pid: u32) -> String {
        let mut name = None;
        self.wait(
            || {
                let output = Command::new("busctl")
                    .args([
                        "--user",
                        "--timeout=2s",
                        "--no-legend",
                        "--no-pager",
                        "list",
                    ])
                    .output()
                    .unwrap();
                let text = String::from_utf8_lossy(&output.stdout);
                name = text.lines().find_map(|line| {
                    let mut words = line.split_whitespace();
                    let name = words.next()?;
                    let owner = words.next()?.parse::<u32>().ok()?;
                    (name.starts_with(':') && owner == pid).then(|| name.to_owned())
                });
                name.is_some()
            },
            "resident tray D-Bus connection",
        );
        name.unwrap()
    }
    fn menu(&self, pid: u32, id: i32) {
        let name = self.tray_name(pid);
        let result = Command::new("busctl")
            .args([
                "--user",
                "--timeout=2s",
                "call",
                &name,
                "/MenuBar",
                "com.canonical.dbusmenu",
                "Event",
                "isvu",
                &id.to_string(),
                "clicked",
                "i",
                "0",
                "0",
            ])
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    fn wait_toggle(&self, pid: u32, id: i32, enabled: bool) {
        let name = self.tray_name(pid);
        let expected = format!("v i {}", i32::from(enabled));
        self.wait(
            || {
                let result = Command::new("busctl")
                    .args([
                        "--user",
                        "--timeout=2s",
                        "call",
                        &name,
                        "/MenuBar",
                        "com.canonical.dbusmenu",
                        "GetProperty",
                        "is",
                        &id.to_string(),
                        "toggle-state",
                    ])
                    .output()
                    .unwrap();
                result.status.success()
                    && String::from_utf8_lossy(&result.stdout).trim() == expected
            },
            "tray toggle state",
        );
    }
    fn replacement(&self, old: u32, sounds: bool) -> u32 {
        let mut new = 0;
        self.wait(
            || {
                if let Some((pid, _, actual)) = self.status()
                    && pid != old
                    && actual == sounds
                {
                    new = pid;
                    true
                } else {
                    false
                }
            },
            "replacement resident with reloaded settings",
        );
        self.track(new);
        new
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = pipe::request(&self.scope, &Request::Quit, Duration::from_millis(300));
        let mut clean = true;
        for process in self.processes.get_mut().iter() {
            clean &= stop_owned(process, Duration::from_secs(3));
        }
        if clean {
            if let Err(error) = std::fs::remove_dir_all(&self.root) {
                eprintln!("cannot remove test fixture: {error}");
            }
        } else {
            eprintln!(
                "owned resident did not exit; retaining {}",
                self.root.display()
            );
        }
    }
}
fn stop_owned(process: &OwnedFd, graceful: Duration) -> bool {
    if wait_for_exit(process, graceful) {
        return true;
    }
    // SAFETY: the caller owns the pidfd for the process that may be terminated.
    let result = unsafe {
        libc::syscall(
            libc::SYS_pidfd_send_signal,
            process.as_raw_fd(),
            libc::SIGKILL,
            std::ptr::null::<libc::siginfo_t>(),
            0,
        )
    };
    if result < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
        eprintln!(
            "cannot stop owned resident: {}",
            std::io::Error::last_os_error()
        );
    }
    wait_for_exit(process, Duration::from_secs(3))
}
fn wait_for_exit(process: &OwnedFd, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    loop {
        let left = deadline.saturating_duration_since(Instant::now());
        let milliseconds = left.as_millis().min(i32::MAX as u128) as i32;
        let mut fd = libc::pollfd {
            fd: process.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: fd points to a live pollfd and the timeout is bounded.
        let result = unsafe { libc::poll(&mut fd, 1, milliseconds) };
        if result > 0 {
            return fd.revents & libc::POLLIN != 0;
        }
        if result == 0 || Instant::now() >= deadline {
            return false;
        }
        if std::io::Error::last_os_error().kind() != std::io::ErrorKind::Interrupted {
            return false;
        }
    }
}

#[test]
fn bounded_cleanup_terminates_the_owned_hung_process() {
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut hung = Child(Command::new("sleep").arg("30").spawn().unwrap());
    let mut other = Child(Command::new("sleep").arg("30").spawn().unwrap());
    // SAFETY: pidfd_open returns a new descriptor for the owned child.
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, hung.0.id(), 0) } as i32;
    assert!(raw >= 0);
    // SAFETY: `raw` is the new descriptor pidfd_open returned (checked above); nothing else owns it.
    let pidfd = unsafe { OwnedFd::from_raw_fd(raw) };
    assert!(!wait_for_exit(&pidfd, Duration::from_millis(20)));
    assert!(stop_owned(&pidfd, Duration::from_millis(20)));
    assert!(!hung.0.wait().unwrap().success());
    assert!(
        other.0.try_wait().unwrap().is_none(),
        "unrelated child must remain running"
    );
}
fn window(conn: &RustConnection, root: u32) -> Option<u32> {
    conn.query_tree(root)
        .ok()?
        .reply()
        .ok()?
        .children
        .into_iter()
        .find(|w| {
            let name = conn
                .get_property(
                    false,
                    *w,
                    x11rb::protocol::xproto::AtomEnum::WM_NAME,
                    x11rb::protocol::xproto::AtomEnum::STRING,
                    0,
                    100,
                )
                .ok()
                .and_then(|c| c.reply().ok());
            name.is_some_and(|v| v.value == b"aHerald notification")
        })
}
fn shown(conn: &RustConnection, win: u32) -> bool {
    conn.get_window_attributes(win)
        .ok()
        .and_then(|c| c.reply().ok())
        .is_some_and(|a| a.map_state == MapState::VIEWABLE)
}
fn pixels(conn: &RustConnection, win: u32) -> Vec<u8> {
    let g = conn.get_geometry(win).unwrap().reply().unwrap();
    // Compare the kind text, excluding independently animated artwork at the left.
    conn.get_image(
        ImageFormat::Z_PIXMAP,
        win,
        100,
        66,
        g.width - 100,
        35,
        u32::MAX,
    )
    .unwrap()
    .reply()
    .unwrap()
    .data
}
fn click(conn: &RustConnection, root: u32, win: u32, x: i16, y: i16, button: u8) {
    let event = ButtonPressEvent {
        response_type: BUTTON_PRESS_EVENT,
        detail: button,
        sequence: 0,
        time: 0,
        root,
        event: win,
        child: 0,
        root_x: x,
        root_y: y,
        event_x: x,
        event_y: y,
        state: 0u16.into(),
        same_screen: true,
    };
    conn.send_event(false, win, EventMask::BUTTON_PRESS, event)
        .unwrap()
        .check()
        .unwrap();
    conn.flush().unwrap();
}

#[test]
#[ignore = "requires an isolated X11 display and private D-Bus session"]
fn linux_apply_validates_replaces_and_controls_the_preview() {
    let (conn, screen) = x11rb::connect(None).unwrap();
    let root = conn.setup().roots[screen].root;
    for test in [false, true] {
        let f = Fixture::new(if test { "test" } else { "apply" });
        let old = f.start();
        f.wait(|| window(&conn, root).is_some(), "resident window");
        std::fs::write(&f.config, "[not valid").unwrap();
        f.menu(old, if test { 15 } else { 14 });
        f.wait(
            || {
                f.status()
                    .is_some_and(|(pid, queued, sounds)| pid == old && queued > 0 && sounds)
            },
            "invalid config notice on original resident",
        );
        f.valid(false);
        f.menu(old, if test { 15 } else { 14 });
        let new = f.replacement(old, false);
        let mut win = 0;
        f.wait(
            || {
                if let Some(w) = window(&conn, root) {
                    win = w;
                    !test || shown(&conn, w)
                } else {
                    false
                }
            },
            "replacement window",
        );
        if !test {
            assert!(!shown(&conn, win), "ordinary Apply must not start preview");
            continue;
        }
        let first = pixels(&conn, win);
        click(&conn, root, win, 10, 10, 5);
        f.wait(
            || pixels(&conn, win) != first,
            "navigation to next preview card",
        );
        click(&conn, root, win, 10, 10, 4);
        f.wait(
            || pixels(&conn, win) == first,
            "navigation back to the first preview card",
        );
        // The 64px built-in artwork puts the text column and dismiss button at x=100.
        let geo = conn.get_geometry(win).unwrap().reply().unwrap();
        click(&conn, root, win, 130, geo.height as i16 - 22, 1);
        f.wait(|| !shown(&conn, win), "dismissed preview");
        f.menu(new, 15);
        let latest = f.replacement(new, false);
        f.wait(
            || window(&conn, root).is_some_and(|w| shown(&conn, w)),
            "restarted preview",
        );
        win = window(&conn, root).unwrap();
        let mut event = Event::demo("turn_complete").unwrap();
        event.source = aherald::events::Source::CodexHook;
        event.session_id = Some("real-session".into());
        assert!(matches!(
            pipe::request(&f.scope, &Request::Event { event }, Duration::from_secs(1)).unwrap(),
            Response::Accepted
        ));
        f.wait(
            || {
                f.status()
                    .is_some_and(|(pid, queued, _)| pid == latest && queued == 1)
            },
            "real event queued",
        );
        std::thread::sleep(Duration::from_millis(150));
        click(&conn, root, win, 150, 70, 1);
        f.wait(
            || f.status().is_some_and(|(_, queued, _)| queued == 0) && !shown(&conn, win),
            "real event dismissed and preview stays ended",
        );
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            !shown(&conn, win),
            "preview must not resume after the event"
        );
        f.menu(latest, 14);
        f.replacement(latest, false);
        f.wait(
            || window(&conn, root).is_some(),
            "ordinary Apply after preview",
        );
        std::thread::sleep(Duration::from_millis(200));
        assert!(
            !window(&conn, root).is_some_and(|w| shown(&conn, w)),
            "ordinary Apply must not inherit the preview flag"
        );
    }
}

#[test]
#[ignore = "requires an isolated X11 display and private D-Bus session"]
fn linux_context_adapters_reach_cards_and_the_tray_switch() {
    use std::io::Write;
    let (conn, screen) = x11rb::connect(None).unwrap();
    let root = conn.setup().roots[screen].root;
    let f = Fixture::new("warnings");
    f.valid(false);
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(&f.config)
        .unwrap();
    writeln!(
        file,
        "[codex.warnings]\ncontext_tokens=100\n[claude.warnings]\ncontext_tokens=200"
    )
    .unwrap();
    drop(file);
    let pid = f.start();
    let mut win = 0;
    f.wait(
        || {
            if let Some(w) = window(&conn, root) {
                win = w;
                true
            } else {
                false
            }
        },
        "warning resident window",
    );
    let send = |codex: bool, session: &str| {
        let request = if codex {
            let input = format!(
                r#"{{"hook_event_name":"PostModelRequest","session_id":"{session}","status":"completed","token_usage":{{"input_tokens":101}}}}"#
            );
            Request::CodexContext {
                report: aherald::codex_context::parse(input.as_bytes())
                    .unwrap()
                    .unwrap(),
            }
        } else {
            let input = format!(
                r#"{{"session_id":"{session}","context_window":{{"current_usage":{{"input_tokens":201,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}}}}}}"#
            );
            Request::StatusLine {
                report: aherald::statusline::parse(input.as_bytes()).unwrap(),
                raw: None,
            }
        };
        assert!(matches!(
            pipe::request(&f.scope, &request, Duration::from_secs(1)).unwrap(),
            Response::Accepted
        ));
    };
    for (codex, session) in [(true, "codex-first"), (false, "claude-first")] {
        send(codex, session);
        f.wait(
            || f.status().is_some_and(|(_, queued, _)| queued == 1) && shown(&conn, win),
            "context warning card",
        );
        click(&conn, root, win, 150, 70, 1);
        f.wait(
            || f.status().is_some_and(|(_, queued, _)| queued == 0) && !shown(&conn, win),
            "context warning dismissed",
        );
    }
    f.menu(pid, 12);
    f.wait_toggle(pid, 12, false);
    send(true, "codex-disabled");
    send(false, "claude-disabled");
    assert!(f.status().is_some_and(|(_, queued, _)| queued == 0));
    f.menu(pid, 12);
    f.wait_toggle(pid, 12, true);
    send(true, "codex-enabled");
    f.wait(
        || f.status().is_some_and(|(_, queued, _)| queued == 1) && shown(&conn, win),
        "context warning after re-enabling",
    );
}

#[test]
#[ignore = "requires an isolated X11 display and private D-Bus session; takes one minute"]
fn linux_idle_warnings_reach_cards_and_clear_on_activity() {
    use aherald::events::Source;
    let (conn, screen) = x11rb::connect(None).unwrap();
    let root = conn.setup().roots[screen].root;
    let f = Fixture::new("idle");
    std::fs::write(&f.config, "[sounds]\nenabled=false\n[notifications]\nenabled=true\nactivate_agent_on_click=false\nfade_in_ms=0\nfade_out_ms=0\n[codex.warnings]\nidle_enabled=true\nidle_minutes=1\n[claude.warnings]\nidle_enabled=true\nidle_minutes=1\n[events.turn_complete]\nenabled=false\n[events.idle_warning]\nduration_sec=0\n").unwrap();
    let pid = f.start();
    let mut win = 0;
    f.wait(
        || {
            if let Some(w) = window(&conn, root) {
                win = w;
                true
            } else {
                false
            }
        },
        "idle resident window",
    );
    let send = |source: Source, session: &str, activity: bool| {
        let mut event = Event::demo("turn_complete").unwrap();
        event.source = source;
        event.session_id = Some(session.into());
        event.activity = activity;
        assert!(matches!(
            pipe::request(&f.scope, &Request::Event { event }, Duration::from_secs(1)).unwrap(),
            Response::Accepted
        ));
    };
    send(Source::CodexHook, "codex-idle", false);
    send(Source::ClaudeHook, "claude-idle", false);
    assert!(
        f.status()
            .is_some_and(|(actual, queued, _)| actual == pid && queued == 0)
    );
    f.wait_for(
        Duration::from_secs(70),
        || f.status().is_some_and(|(_, queued, _)| queued == 2) && shown(&conn, win),
        "both one-minute idle warning cards",
    );
    send(Source::CodexHook, "codex-idle", true);
    f.wait(
        || f.status().is_some_and(|(_, queued, _)| queued == 1),
        "Codex idle warning cleared on activity",
    );
    send(Source::ClaudeHook, "claude-idle", true);
    f.wait(
        || f.status().is_some_and(|(_, queued, _)| queued == 0) && !shown(&conn, win),
        "Claude idle warning cleared on activity",
    );
    f.menu(pid, 13);
    f.wait_toggle(pid, 13, false);
    f.menu(pid, 13);
    f.wait_toggle(pid, 13, true);
}

#[test]
#[ignore = "requires an isolated X11 display and private D-Bus session"]
fn fixture_panic_cleans_its_resident_without_stopping_another() {
    let survivor = Fixture::new("survivor");
    let survivor_pid = survivor.start();
    let failed = Fixture::new("panic");
    assert_ne!(survivor.scope.namespace, failed.scope.namespace);
    let failed_pid = failed.start();
    let root = failed.root.clone();
    // SAFETY: pidfd_open returns a new descriptor for the known test resident.
    let raw = unsafe { libc::syscall(libc::SYS_pidfd_open, failed_pid, 0) } as i32;
    assert!(raw >= 0);
    // SAFETY: `raw` is the new descriptor pidfd_open returned (checked above); nothing else owns it.
    let pidfd = unsafe { OwnedFd::from_raw_fd(raw) };
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
        let _fixture = failed;
        panic!("exercise fixture cleanup");
    }));
    assert!(caught.is_err());
    assert!(wait_for_exit(&pidfd, Duration::from_secs(1)));
    assert!(!root.exists());
    assert!(
        survivor
            .status()
            .is_some_and(|(pid, _, _)| pid == survivor_pid)
    );
}

#[test]
#[ignore = "requires an isolated X11 display and private D-Bus session"]
fn fixture_removes_files_after_resident_startup_failure() {
    let failed = Fixture::new("startup-failure");
    let root = failed.root.clone();
    std::fs::write(&failed.config, "[invalid").unwrap();
    assert!(aherald::resident::ensure(&failed.exe, &failed.config, &failed.scope).is_err());
    assert!(failed.status().is_none());
    drop(failed);
    assert!(!root.exists());
}
