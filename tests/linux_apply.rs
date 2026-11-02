//! Run with a private D-Bus session and an isolated X11/XWayland display.
#![cfg(target_os = "linux")]
use aherald::{
    events::Event,
    ipc::{Request, Response, pipe},
    local::Scope,
};
use std::{
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
        let config = root.join("settings.toml");
        let fixture = Self {
            scope,
            root,
            config,
            exe,
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
            text.push_str(&format!("[events.{name}]\nduration_ms=60000\n"));
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
            Response::Status { pid, .. } => pid,
            _ => panic!("resident did not start"),
        }
    }
    fn wait(&self, mut check: impl FnMut() -> bool, what: &str) {
        let deadline = Instant::now() + WAIT;
        while !check() {
            assert!(Instant::now() < deadline, "timed out: {what}");
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    fn menu(&self, pid: u32, id: i32) {
        let mut name = None;
        self.wait(
            || {
                let output = Command::new("busctl")
                    .args(["--user", "--no-legend", "--no-pager", "list"])
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
        let name = name.unwrap();
        let result = Command::new("busctl")
            .args([
                "--user",
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
        new
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = pipe::request(&self.scope, &Request::Quit, Duration::from_millis(300));
        let _ = std::fs::remove_dir_all(&self.root);
    }
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
    conn.get_image(ImageFormat::Z_PIXMAP, win, 0, 66, g.width, 35, u32::MAX)
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
        // No artwork exists beside the copied executable, so the dismiss button starts at x=22.
        let geo = conn.get_geometry(win).unwrap().reply().unwrap();
        click(&conn, root, win, 50, geo.height as i16 - 22, 1);
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
