//! StatusNotifierItem and dbusmenu service for Linux desktops.
use std::{collections::HashMap, sync::mpsc::Sender};
use zbus::{
    interface,
    zvariant::{OwnedObjectPath, OwnedValue, Value},
};

pub const MENU_PATH: &str = "/MenuBar";

pub use crate::tray_menu::Command;
use crate::tray_menu::{self, MenuState};

#[derive(Clone, Default, PartialEq)]
struct TrayState {
    /// What the menu lists; its messenger switch is `None` when no sink is configured.
    menu: MenuState,
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
    if state.menu.codex != codex || state.menu.claude != claude || state.icon != icon {
        state.menu.codex = codex;
        state.menu.claude = claude;
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
        let description = [state.menu.claude.as_deref(), state.menu.codex.as_deref()]
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
/// A dbusmenu entry: the command's tag is its id, and a separator takes 100 plus the tag of the
/// entry it precedes.
enum Entry {
    Separator(i32),
    Item(i32, tray_menu::Item),
}
impl Entry {
    fn id(&self) -> i32 {
        match self {
            Self::Separator(id) | Self::Item(id, _) => *id,
        }
    }
}
fn entries(menu: &MenuState) -> Vec<Entry> {
    let mut list = Vec::new();
    for item in tray_menu::items(menu) {
        let id = item.command.tag() as i32;
        if item.separator_before {
            list.push(Entry::Separator(100 + id));
        }
        list.push(Entry::Item(id, item));
    }
    list
}

pub struct Menu {
    pub sender: Sender<Command>,
    /// Called after each command is sent, so the resident's loop wakes to read it.
    wake: Wake,
    state: SharedState,
}
type Wake = std::sync::Arc<dyn Fn() + Send + Sync>;
fn label(value: &str) -> OwnedValue {
    OwnedValue::try_from(Value::from(value)).expect("string variant")
}
fn properties(id: i32, state: &TrayState) -> Props {
    let mut p = Props::new();
    if id == 0 {
        p.insert("children-display".into(), label("submenu"));
        return p;
    }
    match entries(&state.menu).into_iter().find(|e| e.id() == id) {
        Some(Entry::Separator(_)) => {
            p.insert("type".into(), label("separator"));
        }
        Some(Entry::Item(_, item)) => {
            p.insert("label".into(), label(&item.label));
            if matches!(
                item.command,
                Command::Sound
                    | Command::Visual
                    | Command::Messenger
                    | Command::Pause
                    | Command::ContextWarnings
                    | Command::IdleWarnings
            ) {
                p.insert("toggle-type".into(), label("checkmark"));
                p.insert(
                    "toggle-state".into(),
                    OwnedValue::from(i32::from(item.checked)),
                );
            }
        }
        None => {}
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
            entries(&state.menu)
                .into_iter()
                .map(|entry| {
                    let id = entry.id();
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
            .filter(|id| *id == 0 || entries(&state.menu).iter().any(|e| e.id() == *id))
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
        let Some(command) = Command::from_tag(id as isize) else {
            return;
        };
        let _ = self.sender.send(command);
        (self.wake)();
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
    /// `wake` runs after each menu command is queued in `commands`.
    pub fn start(
        sound: bool,
        visual: bool,
        paused: bool,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let state = std::sync::Arc::new(std::sync::Mutex::new(TrayState {
            menu: MenuState {
                sound,
                visual,
                paused,
                context: true,
                idle: true,
                ..MenuState::default()
            },
            ..TrayState::default()
        }));
        let menu_state = state.clone();
        let (command_tx, commands) = std::sync::mpsc::channel();
        let (stop, stop_rx) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            if let Err(error) = serve(command_tx, std::sync::Arc::new(wake), stop_rx, menu_state) {
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
    pub fn set_state(&self, sound: bool, visual: bool, paused: bool, context: bool, idle: bool) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let menu = MenuState {
            sound,
            visual,
            paused,
            context,
            idle,
            ..state.menu.clone()
        };
        if state.menu != menu {
            state.menu = menu;
            state.revision = state.revision.wrapping_add(1);
        }
    }
    /// The state of the messenger switch, or `None` when no sink runs (the item is then hidden).
    pub fn set_messenger(&self, messenger: Option<bool>) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.menu.messenger != messenger {
            state.menu.messenger = messenger;
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
    wake: Wake,
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
                wake,
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

    fn menu_of(state: TrayState) -> (Menu, std::sync::mpsc::Receiver<Command>) {
        let (sender, receiver) = std::sync::mpsc::channel();
        let menu = Menu {
            sender,
            wake: Arc::new(|| {}),
            state: Arc::new(std::sync::Mutex::new(state)),
        };
        (menu, receiver)
    }
    fn tag(command: Command) -> i32 {
        command.tag() as i32
    }
    fn child_ids(menu: &Menu) -> Vec<i32> {
        let (_, (_, _, children)) = menu.get_layout(0, -1, vec![]);
        children
            .into_iter()
            .map(|child| <(i32, Props, Vec<OwnedValue>)>::try_from(child).unwrap().0)
            .collect()
    }
    fn text(state: &TrayState, id: i32, key: &str) -> OwnedValue {
        properties(id, state).remove(key).unwrap()
    }

    /// The messenger item exists only when a sink runs, right after Pop-up.
    #[test]
    fn the_messenger_switch_sits_after_pop_up_and_only_when_configured() {
        let with = |messenger| TrayState {
            menu: MenuState {
                messenger,
                ..MenuState::default()
            },
            ..TrayState::default()
        };
        let m = tag(Command::Messenger);
        assert!(!child_ids(&menu_of(with(None)).0).contains(&m));
        let listed = child_ids(&menu_of(with(Some(false))).0);
        let position = |id| listed.iter().position(|i| *i == id).unwrap();
        assert_eq!(position(m), position(tag(Command::Visual)) + 1);
        assert_eq!(position(tag(Command::Pause)), position(m) + 1);
        for (messenger, expected) in [(Some(false), 0), (Some(true), 1)] {
            let state = with(messenger);
            assert_eq!(
                String::try_from(text(&state, m, "label")).unwrap(),
                "Messenger"
            );
            assert_eq!(
                i32::try_from(text(&state, m, "toggle-state")).unwrap(),
                expected
            );
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        let woken = Arc::new(AtomicUsize::new(0));
        let count = woken.clone();
        let menu = Menu {
            sender,
            wake: Arc::new(move || {
                count.fetch_add(1, Ordering::SeqCst);
            }),
            state: Arc::new(std::sync::Mutex::new(TrayState::default())),
        };
        menu.event(m, "clicked", Value::from(0i32), 0);
        assert_eq!(receiver.try_recv().unwrap(), Command::Messenger);
        // The resident's loop sleeps until woken, so a queued command must wake it, once.
        assert_eq!(woken.load(Ordering::SeqCst), 1);
        // An event that is not a command wakes nothing.
        menu.event(tag(Command::Visual), "hovered", Value::from(0i32), 0);
        assert_eq!(woken.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn changing_the_messenger_switch_bumps_the_revision_once() {
        let tray = Tray::start(true, true, false, || {});
        let revision = || tray.state.lock().unwrap().revision;
        let before = revision();
        tray.set_messenger(Some(true));
        assert_eq!(revision(), before.wrapping_add(1));
        tray.set_messenger(Some(true));
        assert_eq!(revision(), before.wrapping_add(1));
        tray.set_messenger(None);
        assert_eq!(revision(), before.wrapping_add(2));
    }

    #[test]
    fn menu_exposes_toggle_state_and_dispatches_only_clicks() {
        let initial = TrayState {
            menu: MenuState {
                sound: true,
                visual: false,
                paused: true,
                context: true,
                idle: false,
                codex: Some("Codex remain: 5h 80%".into()),
                claude: Some("Claude remain: 5h 50%".into()),
                ..MenuState::default()
            },
            ..TrayState::default()
        };
        let (menu, receiver) = menu_of(initial.clone());
        let ids = child_ids(&menu);
        // Two usage lines, a separator, the items, and the separators of the shared menu.
        assert_eq!(ids.len(), 16);
        for separator in [
            Command::Sound,
            Command::ContextWarnings,
            Command::About,
            Command::Config,
        ] {
            let id = 100 + tag(separator);
            assert!(ids.contains(&id), "{separator:?}");
            assert_eq!(
                String::try_from(text(&initial, id, "type")).unwrap(),
                "separator"
            );
        }
        let state_of = |c| i32::try_from(text(&initial, tag(c), "toggle-state")).unwrap();
        assert_eq!(state_of(Command::Sound), 1);
        assert_eq!(state_of(Command::Visual), 0);
        assert_eq!(state_of(Command::ContextWarnings), 1);
        assert_eq!(state_of(Command::IdleWarnings), 0);
        assert_eq!(
            String::try_from(text(&initial, tag(Command::ClaudeSummary), "label")).unwrap(),
            "Claude remain: 5h 50%"
        );
        assert_eq!(
            String::try_from(text(&initial, tag(Command::CodexSummary), "label")).unwrap(),
            "Codex remain: 5h 80%"
        );
        for command in Command::ALL {
            menu.event(tag(command), "clicked", Value::from(0i32), 0);
            assert_eq!(receiver.try_recv().unwrap(), command);
        }
        for (command, name) in [
            (Command::Apply, "Apply"),
            (Command::ApplyTest, "Apply + Test"),
        ] {
            assert_eq!(
                String::try_from(text(&initial, tag(command), "label")).unwrap(),
                name
            );
        }
        // Separators and unknown ids are not commands, and hovering is not a click.
        for id in [100 + tag(Command::Sound), 0, 99] {
            menu.event(id, "clicked", Value::from(0i32), 0);
        }
        menu.event(tag(Command::Visual), "hovered", Value::from(0i32), 0);
        assert!(receiver.try_recv().is_err());
        let (empty, _) = menu_of(TrayState::default());
        assert_eq!(child_ids(&empty).len(), 13); // no usage lines and no leading separator
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

    #[test]
    fn tray_state_tracks_warning_switches_without_spurious_revisions() {
        let tray = Tray::start(true, true, false, || {});
        tray.set_state(true, true, false, false, true);
        {
            let state = tray.state.lock().unwrap();
            assert_eq!(
                (state.menu.sound, state.menu.visual, state.menu.paused),
                (true, true, false)
            );
            assert_eq!((state.menu.context, state.menu.idle), (false, true));
            assert_eq!(state.revision, 1);
        }
        tray.set_state(true, true, false, false, true);
        assert_eq!(tray.state.lock().unwrap().revision, 1);
        tray.set_state(true, true, true, false, false);
        assert_eq!(tray.state.lock().unwrap().revision, 2);
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
        let tray = Tray::start(true, true, false, || {});
        wait_for(&count, 1);
        drop(first);
        std::thread::sleep(std::time::Duration::from_millis(1200));
        let second = watcher();
        wait_for(&count, 2);
        drop(tray);
        drop(second);
    }
}
