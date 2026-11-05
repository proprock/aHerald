//! Evidence that the user is looking at the session behind an event (AH-222), on an X11 session.
//!
//! Suppressing a card needs positive evidence of both the window and the session, as on Windows
//! and macOS; anything that cannot be proven leaves the event delivered. The window is proven
//! focused when it is the X server's active window. The session is proven only for a terminal that
//! shows one session per window, because X11 cannot say which tab of a tabbed terminal is selected:
//! the window's owner must be a known one-session terminal and must own no other window. A
//! Wayland session, a tabbed terminal (GNOME Terminal, Konsole, Kitty), a multiplexer such as tmux
//! (the hook's ancestry never reaches the terminal) and every error all give "unknown".
use super::activation::{atom, property, same_user};
use crate::{
    events::{Event, Source},
    focus::{Evidence, Session},
};
use x11rb::{
    connection::Connection,
    protocol::xproto::{AtomEnum, ConnectionExt},
};

/// Terminals with exactly one session in a window. Compared with the owner process's `comm`.
const ONE_SESSION: [&str; 10] = [
    "xterm",
    "uxterm",
    "koi8rxterm",
    "urxvt",
    "rxvt",
    "mlterm",
    "aterm",
    "eterm",
    "st",
    "alacritty",
];

const UNKNOWN: Evidence = Evidence {
    window_focused: None,
    session: Session::Unknown,
};

/// Whether the environment is an X11 session. `XDG_SESSION_TYPE` and `WAYLAND_DISPLAY` are
/// checked because under XWayland `_NET_ACTIVE_WINDOW` can still name the last X11 window while a
/// native Wayland window has the focus.
pub fn is_x11_session(get: impl Fn(&str) -> Option<String>) -> bool {
    let set = |key: &str| get(key).is_some_and(|value| !value.is_empty());
    set("DISPLAY")
        && !set("WAYLAND_DISPLAY")
        && get("XDG_SESSION_TYPE").is_none_or(|kind| kind.is_empty() || kind == "x11")
}

/// The evidence for `window` given what the X server reports. `owner` is the window's owning
/// process id and name, `windows` the `(window, owner pid)` pairs of the client list.
pub fn decide(
    active: Option<u32>,
    window: u32,
    owner: Option<(u32, &str)>,
    windows: &[(u32, u32)],
) -> Evidence {
    // A window that is gone, or that no process owns, proves nothing.
    let Some((pid, name)) = owner else {
        return UNKNOWN;
    };
    if !windows.contains(&(window, pid)) {
        return UNKNOWN;
    }
    let alone = windows.iter().filter(|(_, owner)| *owner == pid).count() == 1;
    Evidence {
        window_focused: active.map(|active| active == window),
        session: if alone && ONE_SESSION.contains(&name) {
            Session::Same
        } else {
            Session::Unknown
        },
    }
}

fn comm(pid: u32) -> Option<String> {
    let name = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
    Some(name.trim().to_owned())
}

fn query(window: u32) -> Option<Evidence> {
    let (connection, screen) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots[screen].root;
    let active = property(&connection, root, b"_NET_ACTIVE_WINDOW", AtomEnum::WINDOW);
    let list = connection
        .get_property(
            false,
            root,
            atom(&connection, b"_NET_CLIENT_LIST").ok()?,
            AtomEnum::WINDOW,
            0,
            4096,
        )
        .ok()?
        .reply()
        .ok()?;
    let windows: Vec<(u32, u32)> = list
        .value32()?
        .filter_map(|client| {
            property(&connection, client, b"_NET_WM_PID", AtomEnum::CARDINAL)
                .map(|pid| (client, pid))
        })
        .collect();
    let pid = windows
        .iter()
        .find(|(client, _)| *client == window)
        .map(|(_, pid)| *pid)
        .filter(|pid| same_user(*pid))?;
    let name = comm(pid)?;
    Some(decide(active, window, Some((pid, &name)), &windows))
}

pub fn observe(event: &Event) -> Evidence {
    // The same origin rules as the Windows console and macOS: a main Claude session with a valid
    // identity. Codex hooks may be hosted by an app-server instead of the client terminal.
    if event.source != Source::ClaudeHook
        || event.agent_id.is_some()
        || event.session_id.as_deref().is_none_or(str::is_empty)
        || event.validate().is_err()
        || !is_x11_session(|key| std::env::var(key).ok())
    {
        return UNKNOWN;
    }
    event
        .window
        .and_then(|window| u32::try_from(window).ok())
        .and_then(query)
        .unwrap_or(UNKNOWN)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |key| {
            pairs
                .iter()
                .find(|(name, _)| *name == key)
                .map(|(_, value)| (*value).to_owned())
        }
    }

    #[test]
    fn only_an_x11_session_is_trusted_with_the_active_window() {
        assert!(is_x11_session(env(&[
            ("DISPLAY", ":0"),
            ("XDG_SESSION_TYPE", "x11")
        ])));
        assert!(is_x11_session(env(&[("DISPLAY", ":0")])));
        for pairs in [
            &[("DISPLAY", ":0"), ("XDG_SESSION_TYPE", "wayland")][..],
            &[("DISPLAY", ":0"), ("WAYLAND_DISPLAY", "wayland-0")][..],
            &[("XDG_SESSION_TYPE", "x11")][..],
            &[][..],
        ] {
            let pairs: &'static [(&str, &str)] = Box::leak(pairs.to_vec().into_boxed_slice());
            assert!(!is_x11_session(env(pairs)), "{pairs:?}");
        }
    }

    #[test]
    fn a_lone_window_of_a_one_session_terminal_gives_a_positive_answer() {
        let windows = [(10, 100), (20, 200)];
        let focused = decide(Some(10), 10, Some((100, "xterm")), &windows);
        assert_eq!(focused.window_focused, Some(true));
        assert_eq!(focused.session, Session::Same);
        let elsewhere = decide(Some(20), 10, Some((100, "xterm")), &windows);
        assert_eq!(elsewhere.window_focused, Some(false));
        assert_eq!(elsewhere.session, Session::Same);
        let none_active = decide(None, 10, Some((100, "alacritty")), &windows);
        assert_eq!(none_active.window_focused, None);
    }

    #[test]
    fn tabbed_terminals_and_shared_owners_never_prove_the_session() {
        for name in [
            "gnome-terminal-",
            "konsole",
            "kitty",
            "tilix",
            "wezterm-gui",
        ] {
            let evidence = decide(Some(10), 10, Some((100, name)), &[(10, 100)]);
            assert_eq!(evidence.session, Session::Unknown, "{name}");
        }
        // Two windows of one process (an alacritty with a second window): which one hosts the
        // session is not known.
        let shared = decide(
            Some(10),
            10,
            Some((100, "alacritty")),
            &[(10, 100), (11, 100)],
        );
        assert_eq!(shared.session, Session::Unknown);
    }

    #[test]
    fn a_missing_window_or_owner_is_no_evidence() {
        assert!(
            decide(Some(10), 10, None, &[(10, 100)])
                .window_focused
                .is_none()
        );
        let gone = decide(Some(10), 10, Some((100, "xterm")), &[(11, 100)]);
        assert_eq!(
            (gone.window_focused, gone.session),
            (None, Session::Unknown)
        );
        let other_owner = decide(Some(10), 10, Some((999, "xterm")), &[(10, 100)]);
        assert_eq!(other_owner.window_focused, None);
    }

    #[test]
    fn the_known_terminals_are_one_session_ones() {
        for tabbed in [
            "gnome-terminal-",
            "konsole",
            "kitty",
            "tilix",
            "terminator",
            "xfce4-terminal",
        ] {
            assert!(!ONE_SESSION.contains(&tabbed), "{tabbed}");
        }
    }

    #[test]
    fn other_sources_and_codex_hooks_are_delivered_without_asking_the_server() {
        let mut event = Event::demo("turn_complete").unwrap();
        event.session_id = Some("session".into());
        event.window = Some(7);
        for source in [Source::CodexHook, Source::Demo] {
            event.source = source;
            let evidence = observe(&event);
            assert_eq!(
                (evidence.window_focused, evidence.session),
                (None, Session::Unknown)
            );
        }
    }
}
