//! Evidence that the user is looking at the session behind an event (AH-120).
//!
//! Whether the terminal application is in front comes from `NSRunningApplication::isActive` and
//! needs no permission. Which tab of a
//! terminal is selected does: it is read through AppleScript, which makes macOS ask for
//! Automation access, so it only runs when `[general] detect_terminal_tab` is on. Anything
//! that cannot be proven (another application in front, an unsupported terminal, a denied
//! permission, a slow reply) leaves the evidence unknown and the event is delivered.
use crate::{
    config::Config,
    events::{Event, Source},
    focus::{Evidence, Session},
};
use objc2_app_kit::NSRunningApplication;
use std::{
    io::Read,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

const QUERY_LIMIT: Duration = Duration::from_millis(1500);

/// The session is the event's only when both devices are known and the same.
pub fn session_of(event_tty: Option<&str>, selected_tty: Option<&str>) -> Session {
    match (event_tty, selected_tty) {
        (Some(event), Some(selected)) if event == selected => Session::Same,
        (Some(_), Some(_)) => Session::Different,
        _ => Session::Unknown,
    }
}

/// The AppleScript that returns the device of the selected tab, for terminals that expose it.
pub fn selected_tty_script(bundle: &str) -> Option<&'static str> {
    match bundle {
        "com.apple.Terminal" => Some(
            "tell application id \"com.apple.Terminal\" to return tty of selected tab of front window",
        ),
        "com.googlecode.iterm2" => Some(
            "tell application id \"com.googlecode.iterm2\" to return tty of current session of current tab of current window",
        ),
        _ => None,
    }
}

/// A device path from the script's output; anything else is no evidence.
pub fn parse_tty(output: &str) -> Option<String> {
    let line = output.trim();
    crate::events::is_terminal_device(line).then(|| line.to_owned())
}

/// Runs `program` and returns its standard output when it exits successfully within `limit`;
/// a slower program is killed.
pub fn run_bounded(program: &str, args: &[&str], limit: Duration) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        stdout.read_to_string(&mut text).ok().map(|_| text)
    });
    let deadline = Instant::now() + limit;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let text = reader.join().ok().flatten();
    status.filter(|status| status.success()).and(text)
}

fn selected_tty(bundle: &str) -> Option<String> {
    let script = selected_tty_script(bundle)?;
    parse_tty(&run_bounded(
        "/usr/bin/osascript",
        &["-e", script],
        QUERY_LIMIT,
    )?)
}

/// Whether the application running as `pid` is the frontmost one.
fn is_frontmost(pid: u32) -> Option<bool> {
    let app =
        NSRunningApplication::runningApplicationWithProcessIdentifier(i32::try_from(pid).ok()?)?;
    Some(app.isActive())
}

pub fn observe(event: &Event, config: &Config) -> Evidence {
    let unknown = Evidence {
        window_focused: None,
        session: Session::Unknown,
    };
    // The same origin rules as the Windows console: a main Claude session with a valid
    // identity. Codex hooks may be hosted by an app-server instead of the client terminal.
    if !config.general.detect_terminal_tab
        || event.source != Source::ClaudeHook
        || event.agent_id.is_some()
        || event.session_id.as_deref().is_none_or(str::is_empty)
        || event.validate().is_err()
    {
        return unknown;
    }
    let terminal = event.window.and_then(|pid| u32::try_from(pid).ok());
    let focused = terminal.and_then(is_frontmost);
    if focused != Some(true) {
        return Evidence {
            window_focused: focused,
            session: Session::Unknown,
        };
    }
    let selected = terminal
        .and_then(super::terminal::bundle_of)
        .and_then(|bundle| selected_tty(&bundle));
    Evidence {
        window_focused: focused,
        session: session_of(event.tty.as_deref(), selected.as_deref()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_matches_only_on_equal_known_devices() {
        let tty = Some("/dev/ttys003");
        assert_eq!(session_of(tty, tty), Session::Same);
        assert_eq!(session_of(tty, Some("/dev/ttys004")), Session::Different);
        assert_eq!(session_of(tty, None), Session::Unknown);
        assert_eq!(session_of(None, tty), Session::Unknown);
        assert_eq!(session_of(None, None), Session::Unknown);
    }

    #[test]
    fn only_terminals_with_a_selected_tab_query_have_a_script() {
        for bundle in ["com.apple.Terminal", "com.googlecode.iterm2"] {
            assert!(selected_tty_script(bundle).unwrap().contains(bundle));
        }
        for bundle in ["com.mitchellh.ghostty", "dev.warp.Warp-Stable", ""] {
            assert_eq!(selected_tty_script(bundle), None, "{bundle}");
        }
    }

    #[test]
    fn script_output_is_a_device_path_or_nothing() {
        assert_eq!(parse_tty("/dev/ttys003\n").as_deref(), Some("/dev/ttys003"));
        for bad in ["", "missing value\n", "ttys003", "/dev/../x", "/etc/passwd"] {
            assert_eq!(parse_tty(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_program_is_read_when_quick_and_killed_when_slow() {
        let limit = Duration::from_secs(5);
        assert_eq!(
            run_bounded("/bin/echo", &["hi"], limit).as_deref(),
            Some("hi\n")
        );
        assert_eq!(run_bounded("/usr/bin/false", &[], limit), None);
        assert_eq!(run_bounded("/nonexistent/program", &[], limit), None);
        let started = Instant::now();
        assert_eq!(
            run_bounded("/bin/sleep", &["30"], Duration::from_millis(100)),
            None
        );
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn nothing_is_proven_when_the_option_is_off_or_the_origin_is_unclear() {
        let mut config = Config::default();
        let mut event = Event::demo("turn_complete").unwrap();
        event.source = Source::ClaudeHook;
        event.session_id = Some("s".into());
        event.window = Some(u64::from(std::process::id()));
        event.tty = Some("/dev/ttys003".into());
        let unknown = |e: Evidence| e.window_focused.is_none() && e.session == Session::Unknown;
        assert!(unknown(observe(&event, &config)));
        config.general.detect_terminal_tab = true;
        for change in [
            |e: &mut Event| e.source = Source::CodexHook,
            |e: &mut Event| e.agent_id = Some("sub".into()),
            |e: &mut Event| e.session_id = None,
            |e: &mut Event| e.tty = Some("bad".into()),
        ] {
            let mut other = event.clone();
            change(&mut other);
            assert!(unknown(observe(&other, &config)), "{other:?}");
        }
    }
}
