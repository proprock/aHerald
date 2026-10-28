//! Finds the terminal application that hosts the process tree a hook runs in, and raises it.
//!
//! A hook has no window of its own, so the parent chain is walked up to the first application
//! that is a known terminal. Its process id is what the event carries (`Event::window`); on a
//! click it is checked again, because the number came over IPC. Only the application comes
//! forward: choosing a window or tab would need Automation access, which aHerald does not ask for.
use objc2_app_kit::NSRunningApplication;
use std::path::{Path, PathBuf};

const MAX_ANCESTORS: usize = 16;

/// Bundle identifiers of terminal applications whose process may be raised.
const TERMINALS: [&str; 7] = [
    "com.apple.Terminal",
    "com.googlecode.iterm2",
    "com.mitchellh.ghostty",
    "dev.warp.Warp-Stable",
    "net.kovidgoyal.kitty",
    "org.alacritty",
    "com.github.wez.wezterm",
];

pub fn is_terminal(bundle: &str) -> bool {
    TERMINALS.contains(&bundle)
}

/// Parent process of `pid`, or `None` when it cannot be read or has none.
///
/// The kernel refuses `proc_pidinfo` for processes of other users, and `login`, which sits
/// between a terminal and its shell, is one of them; `ps` can read those.
pub fn parent_of(pid: u32) -> Option<u32> {
    direct_parent(pid).or_else(|| ps_parent(pid))
}

fn ps_parent(pid: u32) -> Option<u32> {
    let pid = i32::try_from(pid).ok().filter(|pid| *pid > 0)?;
    let output = std::process::Command::new("/bin/ps")
        .args(["-o", "ppid=", "-p", &pid.to_string()])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let parent: u32 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .ok()?;
    (parent != 0).then_some(parent)
}

fn direct_parent(pid: u32) -> Option<u32> {
    let pid = i32::try_from(pid).ok().filter(|pid| *pid > 0)?;
    // SAFETY: `info` is plain data of exactly the size passed; the kernel fills it for the
    // process `pid` or reports an error.
    let parent = unsafe {
        let mut info: libc::proc_bsdinfo = std::mem::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdinfo>() as i32;
        let read = libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTBSDINFO,
            0,
            (&mut info as *mut libc::proc_bsdinfo).cast(),
            size,
        );
        if read != size {
            return None;
        }
        info.pbi_ppid
    };
    (parent != 0).then_some(parent)
}

/// The parent chain of `start`, nearest first. Bounded, so pid reuse loops and very deep trees
/// cannot stall a hook.
pub fn ancestors(start: u32, parent: impl Fn(u32) -> Option<u32>) -> Vec<u32> {
    let mut chain = Vec::new();
    let mut pid = start;
    while chain.len() < MAX_ANCESTORS {
        let Some(next) = parent(pid) else { break };
        if chain.contains(&next) {
            break;
        }
        chain.push(next);
        pid = next;
    }
    chain
}

/// The first of `chain` whose bundle identifier (from `bundle`) names a terminal.
pub fn nearest_terminal(chain: &[u32], bundle: impl Fn(u32) -> Option<String>) -> Option<u32> {
    chain
        .iter()
        .copied()
        .find(|pid| bundle(*pid).is_some_and(|name| is_terminal(&name)))
}

/// Claude Desktop's session folder under the user's home directory, when it exists.
pub fn claude_session_roots(home: &Path) -> Vec<PathBuf> {
    let root = home.join("Library/Application Support/Claude/claude-code-sessions");
    if root.is_dir() {
        vec![root]
    } else {
        Vec::new()
    }
}

/// The bundle identifier of the application running as `pid`.
pub fn bundle_of(pid: u32) -> Option<String> {
    let pid = i32::try_from(pid).ok()?;
    NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?
        .bundleIdentifier()
        .map(|name| name.to_string())
}

/// The terminal application the current process runs under, as a process id for `Event::window`.
pub fn current() -> Option<u64> {
    let chain = ancestors(std::process::id(), parent_of);
    nearest_terminal(&chain, bundle_of).map(u64::from)
}

/// Brings the terminal application `window` (a process id from `current`) to the front. The id
/// is untrusted: it must still be a running terminal.
pub fn raise(window: u64) -> Result<(), String> {
    let pid = u32::try_from(window).map_err(|_| "agent_window_unavailable")?;
    let bundle = bundle_of(pid)
        .filter(|name| is_terminal(name))
        .ok_or("agent_window_unavailable")?;
    open_bundle(&bundle)
}

/// Opens or raises the application with `bundle` through LaunchServices.
pub fn open_bundle(bundle: &str) -> Result<(), String> {
    let mut child = std::process::Command::new("/usr/bin/open")
        .args(["-b", bundle])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|_| "agent_activation_failed")?;
    let status = child.wait().map_err(|_| "agent_activation_failed")?;
    if status.success() {
        Ok(())
    } else {
        Err("agent_activation_failed".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn only_known_terminals_are_terminals() {
        for bundle in [
            "com.apple.Terminal",
            "com.googlecode.iterm2",
            "dev.warp.Warp-Stable",
        ] {
            assert!(is_terminal(bundle), "{bundle}");
        }
        for bundle in [
            "",
            "com.apple.finder",
            "com.anthropic.claudefordesktop",
            "com.openai.codex",
            "com.apple.Terminal.evil",
            "COM.APPLE.TERMINAL",
        ] {
            assert!(!is_terminal(bundle), "{bundle}");
        }
    }

    #[test]
    fn the_parent_chain_is_nearest_first_and_stops_at_the_top() {
        let table: HashMap<u32, u32> = [(30, 20), (20, 10), (10, 1), (1, 0)].into();
        let parent = |pid| table.get(&pid).copied().filter(|p| *p != 0);
        assert_eq!(ancestors(30, parent), [20, 10, 1]);
        assert_eq!(ancestors(1, parent), Vec::<u32>::new());
        assert_eq!(ancestors(99, parent), Vec::<u32>::new());
    }

    #[test]
    fn a_looping_or_very_deep_chain_is_cut_off() {
        assert_eq!(
            ancestors(5, |pid| Some(if pid == 5 { 6 } else { 5 })),
            [6, 5]
        );
        let deep = ancestors(1000, |pid| pid.checked_sub(1).filter(|p| *p > 0));
        assert_eq!(deep.len(), MAX_ANCESTORS);
        assert_eq!(deep[0], 999);
    }

    #[test]
    fn the_nearest_terminal_wins_and_other_applications_are_skipped() {
        let names: HashMap<u32, &str> = [
            (20, "com.anthropic.claudefordesktop"),
            (10, "com.googlecode.iterm2"),
            (2, "com.apple.Terminal"),
        ]
        .into();
        let bundle = |pid| names.get(&pid).map(|name| (*name).to_owned());
        assert_eq!(nearest_terminal(&[30, 20, 10, 2], bundle), Some(10));
        assert_eq!(nearest_terminal(&[30, 20], bundle), None);
        assert_eq!(nearest_terminal(&[], bundle), None);
    }

    #[test]
    fn the_parent_of_this_process_is_what_the_system_reports() {
        assert_eq!(
            parent_of(std::process::id()),
            Some(std::os::unix::process::parent_id())
        );
        assert_eq!(parent_of(0), None);
        assert_eq!(parent_of(u32::MAX), None);
    }

    #[test]
    fn the_parent_of_a_process_owned_by_another_user_is_read_too() {
        // `login` and the daemons launchd starts belong to root; so does this process's view
        // of them, which `proc_pidinfo` refuses.
        let listing = std::process::Command::new("/bin/ps")
            .args(["-axo", "pid=,user=,ppid="])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&listing.stdout).into_owned();
        let root_child = text
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let (pid, user, ppid) = (fields.next()?, fields.next()?, fields.next()?);
                (user == "root" && ppid == "1").then(|| pid.parse::<u32>().ok())?
            })
            .next()
            .expect("launchd always has a root child");
        assert_eq!(parent_of(root_child), Some(1));
    }

    #[test]
    fn claude_desktop_sessions_are_found_under_application_support_only_when_present() {
        let home = std::env::temp_dir().join(format!("ah-term-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        assert!(claude_session_roots(&home).is_empty());
        let root = home.join("Library/Application Support/Claude/claude-code-sessions");
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(claude_session_roots(&home), [root]);
        let _ = std::fs::remove_dir_all(&home);
    }
}
