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
    Exit,
}

pub struct Item;
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
        "dialog-information"
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
const MENU_ITEMS: [i32; 9] = [6, 8, 1, 2, 3, 9, 4, 5, 7];

pub struct Menu {
    pub sender: Sender<Command>,
    pub state: std::sync::Arc<std::sync::Mutex<[bool; 3]>>,
}
fn label(value: &str) -> OwnedValue {
    OwnedValue::try_from(Value::from(value)).expect("string variant")
}
fn properties(id: i32, state: [bool; 3]) -> Props {
    let mut p = Props::new();
    let name = match id {
        1 => "Sound",
        2 => "Pop-up",
        3 => "Pause notifications",
        4 => "Edit configuration",
        5 => "Restart",
        6 => "About aHerald",
        7 => "Exit",
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
            OwnedValue::from(i32::from(state[(id - 1) as usize])),
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
        let state = *self.state.lock().unwrap_or_else(|e| e.into_inner());
        let children = if parent_id == 0 && recursion_depth != 0 {
            MENU_ITEMS
                .into_iter()
                .map(|id| {
                    OwnedValue::try_from(Value::from((
                        id,
                        properties(id, state),
                        Vec::<OwnedValue>::new(),
                    )))
                    .expect("layout variant")
                })
                .collect()
        } else {
            Vec::new()
        };
        (1, (parent_id, properties(parent_id, state), children))
    }
    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        _property_names: Vec<String>,
    ) -> Vec<(i32, Props)> {
        let state = *self.state.lock().unwrap_or_else(|e| e.into_inner());
        ids.into_iter()
            .filter(|id| (0..=9).contains(id))
            .map(|id| (id, properties(id, state)))
            .collect()
    }
    fn get_property(&self, id: i32, name: &str) -> zbus::fdo::Result<OwnedValue> {
        let state = *self.state.lock().unwrap_or_else(|e| e.into_inner());
        properties(id, state)
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
    state: std::sync::Arc<std::sync::Mutex<[bool; 3]>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Tray {
    pub fn start(sound: bool, visual: bool, paused: bool) -> Self {
        let state = std::sync::Arc::new(std::sync::Mutex::new([sound, visual, paused]));
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
        *self.state.lock().unwrap_or_else(|e| e.into_inner()) = [sound, visual, paused];
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
    state: std::sync::Arc<std::sync::Mutex<[bool; 3]>>,
) -> Result<(), String> {
    let connection = zbus::blocking::connection::Builder::session()
        .map_err(|e| e.to_string())?
        .serve_at("/StatusNotifierItem", Item)
        .map_err(|e| e.to_string())?
        .serve_at(MENU_PATH, Menu { sender, state })
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
    loop {
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
        let state = Arc::new(std::sync::Mutex::new([true, false, true]));
        let (sender, receiver) = std::sync::mpsc::channel();
        let menu = Menu { sender, state };
        let (_, (_, _, children)) = menu.get_layout(0, -1, vec![]);
        assert_eq!(children.len(), 9);
        assert_eq!(MENU_ITEMS, [6, 8, 1, 2, 3, 9, 4, 5, 7]);
        for separator in [8, 9] {
            let value = properties(separator, [true, false, true])
                .remove("type")
                .unwrap();
            assert_eq!(String::try_from(value).unwrap(), "separator");
        }
        assert_eq!(
            i32::try_from(
                properties(1, [true, false, true])
                    .remove("toggle-state")
                    .unwrap()
            )
            .unwrap(),
            1
        );
        assert_eq!(
            i32::try_from(
                properties(2, [true, false, true])
                    .remove("toggle-state")
                    .unwrap()
            )
            .unwrap(),
            0
        );
        menu.event(2, "hovered", Value::from(0i32), 0);
        assert!(receiver.try_recv().is_err());
        menu.event(2, "clicked", Value::from(0i32), 0);
        assert_eq!(receiver.try_recv().unwrap(), Command::Visual);
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
