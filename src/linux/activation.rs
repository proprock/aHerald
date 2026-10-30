//! Capture and raise an X11 terminal without guessing another application's window.
use crate::{
    activation::{self, Agent},
    events::Event,
};
use std::{
    os::unix::fs::MetadataExt,
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

static CLICK_TIME: AtomicU32 = AtomicU32::new(x11rb::CURRENT_TIME);

/// Remember the X server timestamp of the most recent real card click.
pub fn note_click(time: u32) {
    CLICK_TIME.store(time, Ordering::Release);
}
use x11rb::{
    connection::Connection,
    protocol::xproto::{AtomEnum, ClientMessageEvent, ConnectionExt, EventMask},
    rust_connection::RustConnection,
};

fn atom(connection: &RustConnection, name: &[u8]) -> Result<u32, String> {
    connection
        .intern_atom(false, name)
        .map_err(|e| e.to_string())?
        .reply()
        .map(|r| r.atom)
        .map_err(|e| e.to_string())
}
fn property(connection: &RustConnection, window: u32, name: &[u8], kind: AtomEnum) -> Option<u32> {
    let atom = atom(connection, name).ok()?;
    connection
        .get_property(false, window, atom, kind, 0, 1)
        .ok()?
        .reply()
        .ok()?
        .value32()?
        .next()
}
fn parent(pid: u32) -> Option<u32> {
    let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    text.rsplit_once(')')?
        .1
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}
fn ancestors(mut pid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    for _ in 0..64 {
        if pid <= 1 || out.contains(&pid) {
            break;
        }
        out.push(pid);
        let Some(next) = parent(pid) else {
            break;
        };
        pid = next;
    }
    out
}
fn same_user(pid: u32) -> bool {
    // SAFETY: getuid has no preconditions.
    let uid = unsafe { libc::getuid() };
    std::fs::metadata(format!("/proc/{pid}")).is_ok_and(|meta| meta.uid() == uid)
}
/// Select a terminal only when its PID appears in the hook's process ancestry.
/// Prefer the active matching window; an ambiguous group of windows is not guessed.
fn choose_window(ancestors: &[u32], windows: &[(u32, u32)], active: Option<u32>) -> Option<u32> {
    for pid in ancestors {
        let mut candidates = windows
            .iter()
            .filter(|(_, owner)| owner == pid)
            .map(|(window, _)| *window);
        let first = candidates.next();
        if let Some(first) = first {
            let rest: Vec<_> = candidates.collect();
            if active == Some(first) || active.is_some_and(|id| rest.contains(&id)) {
                return active;
            }
            return rest.is_empty().then_some(first);
        }
    }
    None
}
/// Capture a window belonging to a process in the hook's ancestor chain.
pub fn current() -> Option<u64> {
    let (connection, screen) = x11rb::connect(None).ok()?;
    let root = connection.setup().roots[screen].root;
    let active = property(&connection, root, b"_NET_ACTIVE_WINDOW", AtomEnum::WINDOW);
    let list_atom = atom(&connection, b"_NET_CLIENT_LIST").ok()?;
    let reply = connection
        .get_property(false, root, list_atom, AtomEnum::WINDOW, 0, 4096)
        .ok()?
        .reply()
        .ok()?;
    let ancestors = ancestors(std::process::id());
    let candidates: Vec<_> = reply
        .value32()?
        .filter_map(|window| {
            let pid = property(&connection, window, b"_NET_WM_PID", AtomEnum::CARDINAL)?;
            (same_user(pid) && ancestors.contains(&pid)).then_some((window, pid))
        })
        .collect();
    choose_window(&ancestors, &candidates, active).map(u64::from)
}
/// Ask the window manager to activate a still-existing window of the same user.
pub fn raise(window: u64) -> Result<(), String> {
    let window = u32::try_from(window).map_err(|_| "agent_window_unavailable")?;
    let (connection, screen) = x11rb::connect(None).map_err(|_| "agent_window_unavailable")?;
    let root = connection.setup().roots[screen].root;
    let pid = property(&connection, window, b"_NET_WM_PID", AtomEnum::CARDINAL)
        .ok_or("agent_window_unavailable")?;
    if !same_user(pid) {
        return Err("agent_window_unavailable".into());
    }
    let active = atom(&connection, b"_NET_ACTIVE_WINDOW")?;
    let event = ClientMessageEvent::new(
        32,
        window,
        active,
        [2, CLICK_TIME.load(Ordering::Acquire), 0, 0, 0],
    );
    connection
        .send_event(
            false,
            root,
            EventMask::SUBSTRUCTURE_REDIRECT | EventMask::SUBSTRUCTURE_NOTIFY,
            event,
        )
        .map_err(|e| e.to_string())?
        .check()
        .map_err(|e| e.to_string())?;
    connection.flush().map_err(|e| e.to_string())?;
    Ok(())
}
struct LinuxAgent {
    window: Option<u64>,
}
impl Agent for LinuxAgent {
    fn open_thread(&mut self, url: &str) -> Result<(), String> {
        crate::shell::open(std::ffi::OsStr::new(url)).map_err(|_| "agent_link_failed".into())
    }
    fn activate_window(&mut self) -> Result<(), String> {
        self.window
            .ok_or_else(|| "agent_window_unavailable".to_owned())
            .and_then(raise)
    }
    fn focus_window(&mut self, window: u64) -> Result<(), String> {
        raise(window)
    }
    fn desktop_session(&mut self, id: &str) -> Option<String> {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| p.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(PathBuf::from)
                    .map(|p| p.join(".config"))
            })?;
        activation::claude_desktop::session_roots(&base)
            .iter()
            .find_map(|root| activation::claude_desktop::local_session_id(root, id))
    }
}
pub fn run(event: &Event) -> Result<(), String> {
    activation::click(
        event,
        true,
        true,
        &mut LinuxAgent {
            window: event.window,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn process_ancestry_includes_current_process() {
        assert_eq!(
            ancestors(std::process::id()).first(),
            Some(&std::process::id())
        );
        assert!(same_user(std::process::id()));
    }
    #[test]
    fn ambiguous_windows_are_never_guessed() {
        let windows = [(10, 100), (11, 100), (12, 200)];
        assert_eq!(choose_window(&[100, 200], &windows, None), None);
        assert_eq!(choose_window(&[100, 200], &windows, Some(11)), Some(11));
        assert_eq!(choose_window(&[200], &windows, None), Some(12));
        assert_eq!(choose_window(&[300], &windows, Some(11)), None);
    }
    #[test]
    fn invalid_window_id_is_refused_without_connecting() {
        assert!(raise(u64::MAX).is_err());
    }
}
