//! StatusNotifierItem and dbusmenu service for Linux desktops.
use std::{collections::HashMap, sync::mpsc::Sender};
use zbus::{
    interface,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

pub const MENU_PATH: &str = "/MenuBar";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Sound,
    Visual,
    Pause,
    Config,
    Restart,
    About,
    CodexUsage,
    ClaudeUsage,
    Exit,
}

#[derive(Clone, Default, PartialEq)]
struct TrayState {
    toggles: [bool; 3],
    codex: Option<String>,
    claude: Option<String>,
    icon: Vec<u8>,
    revision: u32,
}

type SharedState = std::sync::Arc<std::sync::Mutex<TrayState>>;
type Pixmaps = Vec<(i32, i32, Vec<u8>)>;
type ToolTip = (String, Pixmaps, String, String);
fn update_usage(
    state: &mut TrayState,
    codex: Option<String>,
    claude: Option<String>,
    levels: &crate::tray_usage::Levels,
) -> Result<(), String> {
    // StatusNotifierItem expects ARGB bytes in network order; the renderer returns BGRA.
    let bgra = levels.render(32).map_err(str::to_owned)?;
    let icon = bgra
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[3], p[2], p[1], p[0]])
        .collect::<Vec<_>>();
    if state.codex != codex || state.claude != claude || state.icon != icon {
        state.codex = codex;
        state.claude = claude;
        state.icon = icon;
        state.revision = state.revision.wrapping_add(1);
    }
    Ok(())
}

pub struct Item {
    state: SharedState,
}
#[interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn category(&self) -> &str {
        "ApplicationStatus"
    }
    #[zbus(property)]
    fn id(&self) -> &str {
        "aherald"
    }
    #[zbus(property)]
    fn title(&self) -> &str {
        "aHerald"
    }
    #[zbus(property)]
    fn status(&self) -> &str {
        "Active"
    }
    #[zbus(property)]
    fn icon_name(&self) -> &str {
        ""
    }
    #[zbus(property)]
    fn icon_pixmap(&self) -> Pixmaps {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.icon.is_empty() {
            Vec::new()
        } else {
            vec![(32, 32, state.icon.clone())]
        }
    }
    #[zbus(property)]
    fn tool_tip(&self) -> ToolTip {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let description = [state.claude.as_deref(), state.codex.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("\n");
        (String::new(), Vec::new(), "aHerald".into(), description)
    }
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(MENU_PATH).expect("static path")
    }
    fn context_menu(&self, _x: i32, _y: i32) {}
    fn activate(&self, _x: i32, _y: i32) {}
    fn secondary_activate(&self, _x: i32, _y: i32) {}
    fn scroll(&self, _delta: i32, _orientation: &str) {}
}

type Props = HashMap<String, OwnedValue>;
type Layout = (i32, Props, Vec<OwnedValue>);
// About, controls, then application actions, matching the other platform menus.
const MENU_ITEMS: [i32; 11] = [10, 11, 6, 8, 1, 2, 3, 9, 4, 5, 7];

pub struct Menu {
    pub sender: Sender<Command>,
    state: SharedState,
}
fn label(value: &str) -> OwnedValue {
    OwnedValue::try_from(Value::from(value)).expect("string variant")
}
fn properties(id: i32, state: &TrayState) -> Props {
    let mut p = Props::new();
    let name = match id {
        1 => "Sound",
        2 => "Pop-up",
        3 => "Pause notifications",
        4 => "Edit configuration",
        5 => "Restart",
        6 => "About aHerald",
        7 => "Exit",
        10 => state.claude.as_deref().unwrap_or(""),
        11 => state.codex.as_deref().unwrap_or(""),
        _ => "",
    };
    if id == 0 {
        p.insert("children-display".into(), label("submenu"));
    } else if id == 8 || id == 9 {
        p.insert("type".into(), label("separator"));
    } else {
        p.insert("label".into(), label(name));
    }
    if (1..=3).contains(&id) {
        p.insert("toggle-type".into(), label("checkmark"));
        p.insert(
            "toggle-state".into(),
            OwnedValue::from(i32::from(state.toggles[(id - 1) as usize])),
        );
    }
    p
}
#[interface(name = "com.canonical.dbusmenu")]
impl Menu {
    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }
    #[zbus(property)]
    fn status(&self) -> &str {
        "normal"
    }
    #[zbus(property)]
    fn text_direction(&self) -> &str {
        "ltr"
    }
    fn get_layout(
        &self,
        parent_id: i32,
        recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> (u32, Layout) {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner()).clone();
        let children = if parent_id == 0 && recursion_depth != 0 {
            MENU_ITEMS
                .into_iter()
                .filter(|id| match id {
                    10 => state.claude.is_some(),
                    11 => state.codex.is_some(),
                    _ => true,
                })
                .map(|id| {
                    OwnedValue::try_from(Value::from((
                        id,
                        properties(id, &state),
                        Vec::<OwnedValue>::new(),
                    )))
                    .expect("layout variant")
                })
                .collect()
        } else {
            Vec::new()
        };
        (
            state.revision,
            (parent_id, properties(parent_id, &state), children),
        )
    }
    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        _property_names: Vec<String>,
    ) -> Vec<(i32, Props)> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner()).clone();
        ids.into_iter()
            .filter(|id| (0..=11).contains(id))
            .map(|id| (id, properties(id, &state)))
            .collect()
    }
    fn get_property(&self, id: i32, name: &str) -> zbus::fdo::Result<OwnedValue> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner()).clone();
        properties(id, &state)
            .remove(name)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("unknown property".into()))
    }
    fn event(&self, id: i32, event_id: &str, _data: Value<'_>, _timestamp: u32) {
        if event_id != "clicked" {
            return;
        }
        let command = match id {
            1 => Command::Sound,
            2 => Command::Visual,
            3 => Command::Pause,
            4 => Command::Config,
            5 => Command::Restart,
            6 => Command::About,
            7 => Command::Exit,
            10 => Command::ClaudeUsage,
            11 => Command::CodexUsage,
            _ => return,
        };
        let _ = self.sender.send(command);
    }
    fn about_to_show(&self, _id: i32) -> bool {
        true
    }
}

/// Owns the D-Bus service and re-registers when the watcher changes owner.
pub struct Tray {
    pub commands: std::sync::mpsc::Receiver<Command>,
    stop: Sender<()>,
    state: SharedState,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Tray {
    pub fn start(sound: bool, visual: bool, paused: bool) -> Self {
        let state = std::sync::Arc::new(std::sync::Mutex::new(TrayState {
            toggles: [sound, visual, paused],
            ..TrayState::default()
        }));
        let menu_state = state.clone();
        let (command_tx, commands) = std::sync::mpsc::channel();
        let (stop, stop_rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            if let Err(error) = serve(command_tx, stop_rx, menu_state) {
                eprintln!("aHerald: tray unavailable: {error}");
            }
        });
        Self {
            commands,
            stop,
            state,
            thread: Some(thread),
        }
    }
    pub fn set_state(&self, sound: bool, visual: bool, paused: bool) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let toggles = [sound, visual, paused];
        if state.toggles != toggles {
            state.toggles = toggles;
            state.revision = state.revision.wrapping_add(1);
        }
    }
    pub fn set_usage(
        &self,
        codex: Option<String>,
        claude: Option<String>,
        levels: &crate::tray_usage::Levels,
    ) -> Result<(), String> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        update_usage(&mut state, codex, claude, levels)
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn serve(
    sender: Sender<Command>,
    stop: std::sync::mpsc::Receiver<()>,
    state: SharedState,
) -> Result<(), String> {
    let connection = zbus::blocking::connection::Builder::session()
        .map_err(|e| e.to_string())?
        .serve_at(
            "/StatusNotifierItem",
            Item {
                state: state.clone(),
            },
        )
        .map_err(|e| e.to_string())?
        .serve_at(
            MENU_PATH,
            Menu {
                sender,
                state: state.clone(),
            },
        )
        .map_err(|e| e.to_string())?
        .build()
        .map_err(|e| e.to_string())?;
    let bus = zbus::blocking::fdo::DBusProxy::new(&connection).map_err(|e| e.to_string())?;
    let watcher = zbus::blocking::Proxy::new(
        &connection,
        "org.kde.StatusNotifierWatcher",
        "/StatusNotifierWatcher",
        "org.kde.StatusNotifierWatcher",
    )
    .map_err(|e| e.to_string())?;
    let name = zbus::names::BusName::try_from("org.kde.StatusNotifierWatcher")
        .map_err(|e| e.to_string())?;
    let mut registered = None;
    let mut notified = u32::MAX;
    loop {
        let revision = state.lock().unwrap_or_else(|e| e.into_inner()).revision;
        if revision != notified {
            connection
                .emit_signal(
                    None::<&str>,
                    "/StatusNotifierItem",
                    "org.kde.StatusNotifierItem",
                    "NewIcon",
                    &(),
                )
                .map_err(|e| e.to_string())?;
            connection
                .emit_signal(
                    None::<&str>,
                    "/StatusNotifierItem",
                    "org.kde.StatusNotifierItem",
                    "NewToolTip",
                    &(),
                )
                .map_err(|e| e.to_string())?;
            connection
                .emit_signal(
                    None::<&str>,
                    MENU_PATH,
                    "com.canonical.dbusmenu",
                    "LayoutUpdated",
                    &(revision, 0i32),
                )
                .map_err(|e| e.to_string())?;
            notified = revision;
        }
        let owner = bus.get_name_owner(name.clone()).ok().map(|o| o.to_string());
        if owner != registered {
            registered = None;
            if let Some(owner) = owner {
                let response: zbus::Result<()> =
                    watcher.call("RegisterStatusNotifierItem", &("/StatusNotifierItem",));
                if response.is_ok() {
                    registered = Some(owner);
                }
            }
        }
        match stop.recv_timeout(std::time::Duration::from_secs(1)) {
            Ok(()) | Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn menu_exposes_toggle_state_and_dispatches_only_clicks() {
        let initial = TrayState {
            toggles: [true, false, true],
            codex: Some("Codex remain: 5h 80%".into()),
            claude: Some("Claude remain: 5h 50%".into()),
            ..TrayState::default()
        };
        let state = Arc::new(std::sync::Mutex::new(initial.clone()));
        let (sender, receiver) = std::sync::mpsc::channel();
        let menu = Menu { sender, state };
        let (_, (_, _, children)) = menu.get_layout(0, -1, vec![]);
        assert_eq!(children.len(), 11);
        assert_eq!(MENU_ITEMS, [10, 11, 6, 8, 1, 2, 3, 9, 4, 5, 7]);
        for separator in [8, 9] {
            let value = properties(separator, &initial).remove("type").unwrap();
            assert_eq!(String::try_from(value).unwrap(), "separator");
        }
        assert_eq!(
            i32::try_from(properties(1, &initial).remove("toggle-state").unwrap()).unwrap(),
            1
        );
        assert_eq!(
            i32::try_from(properties(2, &initial).remove("toggle-state").unwrap()).unwrap(),
            0
        );
        assert_eq!(
            String::try_from(properties(10, &initial).remove("label").unwrap()).unwrap(),
            "Claude remain: 5h 50%"
        );
        assert_eq!(
            String::try_from(properties(11, &initial).remove("label").unwrap()).unwrap(),
            "Codex remain: 5h 80%"
        );
        menu.event(10, "clicked", Value::from(0i32), 0);
        assert_eq!(receiver.try_recv().unwrap(), Command::ClaudeUsage);
        menu.event(11, "clicked", Value::from(0i32), 0);
        assert_eq!(receiver.try_recv().unwrap(), Command::CodexUsage);
        menu.event(2, "hovered", Value::from(0i32), 0);
        assert!(receiver.try_recv().is_err());
        menu.event(2, "clicked", Value::from(0i32), 0);
        assert_eq!(receiver.try_recv().unwrap(), Command::Visual);
    }

    #[test]
    fn changing_usage_updates_pixmap_and_revision_without_spurious_updates() {
        use crate::{
            config::UsageDisplay, tray_usage::Levels, usage::Snapshot, usage_service::View,
        };
        fn levels(used: f64) -> Levels {
            let mut view = View::new(true, std::time::Duration::from_secs(300));
            view.snapshot = Some(
                Snapshot::parse(serde_json::json!({
                    "rateLimitsByLimitId": {"codex": {"primary": {"usedPercent": used}}}
                }))
                .unwrap(),
            );
            Levels::from_view(&view, UsageDisplay::Remain)
        }
        let mut state = TrayState::default();
        update_usage(
            &mut state,
            Some("Codex remain: 80%".into()),
            None,
            &levels(20.0),
        )
        .unwrap();
        let first = state.icon.clone();
        assert_eq!(first.len(), 32 * 32 * 4);
        assert_eq!(state.revision, 1);
        update_usage(
            &mut state,
            Some("Codex remain: 80%".into()),
            None,
            &levels(20.0),
        )
        .unwrap();
        assert_eq!(state.revision, 1);
        update_usage(
            &mut state,
            Some("Codex remain: 10%".into()),
            None,
            &levels(90.0),
        )
        .unwrap();
        assert_ne!(state.icon, first);
        assert_eq!(state.revision, 2);
    }

    struct Watcher(Arc<AtomicUsize>);
    #[interface(name = "org.kde.StatusNotifierWatcher")]
    impl Watcher {
        fn register_status_notifier_item(&self, _service: &str) {
            self.0.fetch_add(1, Ordering::SeqCst);
        }
    }
    fn wait_for(counter: &AtomicUsize, count: usize) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while counter.load(Ordering::SeqCst) < count && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        assert!(
            counter.load(Ordering::SeqCst) >= count,
            "watcher did not receive registration {count}"
        );
    }
    #[test]
    #[ignore = "requires a private D-Bus session; run with dbus-run-session"]
    fn registers_again_after_watcher_restarts() {
        let count = Arc::new(AtomicUsize::new(0));
        let watcher = || {
            zbus::blocking::connection::Builder::session()
                .unwrap()
                .name("org.kde.StatusNotifierWatcher")
                .unwrap()
                .serve_at("/StatusNotifierWatcher", Watcher(count.clone()))
                .unwrap()
                .build()
                .unwrap()
        };
        let first = watcher();
        let tray = Tray::start(true, true, false);
        wait_for(&count, 1);
        drop(first);
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let second = watcher();
        wait_for(&count, 2);
        drop(tray);
        drop(second);
    }
}
