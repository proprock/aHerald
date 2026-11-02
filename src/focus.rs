//! Output policy requires positive evidence for both the window and its active session.
use crate::{config::Config, events::Event};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Session {
    Same,
    Different,
    Unknown,
}

#[derive(Clone, Copy, Debug)]
pub struct Evidence {
    pub window_focused: Option<bool>,
    pub session: Session,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Outputs {
    pub window: bool,
    pub sound: bool,
}

impl Default for Outputs {
    fn default() -> Self {
        Self {
            window: true,
            sound: true,
        }
    }
}

pub fn outputs(config: &Config, evidence: Evidence) -> Outputs {
    let active = evidence.window_focused == Some(true) && evidence.session == Session::Same;
    Outputs {
        window: !active || !config.notifications.suppress_when_focused,
        sound: !active || !config.sounds.suppress_when_focused,
    }
}

#[cfg(not(windows))]
pub fn observe(_event: &Event) -> Evidence {
    Evidence {
        window_focused: None,
        session: Session::Unknown,
    }
}

#[cfg(windows)]
pub fn observe(event: &Event) -> Evidence {
    native::observe(event)
}

#[cfg(windows)]
mod native {
    use super::*;
    use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::*};

    pub fn observe(event: &Event) -> Evidence {
        let unknown = Evidence {
            window_focused: None,
            session: Session::Unknown,
        };
        // Codex hooks can be hosted by an app-server instead of the client terminal.
        // Desktop apps and tabbed terminals need an active-session API; an app/window
        // match alone cannot prove that the user is looking at this session.
        if event.source != crate::events::Source::ClaudeHook
            || event.agent_id.is_some()
            || event.session_id.as_deref().is_none_or(str::is_empty)
            || event.validate().is_err()
        {
            return unknown;
        }
        let Some(window) = event.window.and_then(|w| usize::try_from(w).ok()) else {
            return unknown;
        };
        let hwnd = HWND(window as *mut std::ffi::c_void);
        // Only a classic console has a single directly visible console surface.
        // In a pseudoconsole GetConsoleWindow is a hidden message-only surrogate:
        // never use its root owner to infer which terminal tab is selected.
        // SAFETY: read-only queries accept arbitrary/stale handles and write only
        // into the stack buffer. No handle or borrowed pointer is kept.
        unsafe {
            if !IsWindow(Some(hwnd)).as_bool() || !IsWindowVisible(hwnd).as_bool() {
                return unknown;
            }
            let mut class = [0u16; 64];
            let count = GetClassNameW(hwnd, &mut class);
            if count <= 0
                || class[..count as usize]
                    != "ConsoleWindowClass".encode_utf16().collect::<Vec<_>>()
            {
                return unknown;
            }
            let foreground = GetForegroundWindow();
            if foreground.is_invalid() {
                return unknown;
            }
            Evidence {
                window_focused: Some(foreground == hwnd),
                session: Session::Same,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn independent_outputs_require_known_focus_and_matching_session() {
        for visual in [false, true] {
            for sound in [false, true] {
                let mut config = Config::default();
                config.notifications.suppress_when_focused = visual;
                config.sounds.suppress_when_focused = sound;
                for focused in [None, Some(false), Some(true)] {
                    for session in [Session::Same, Session::Different, Session::Unknown] {
                        let actual = outputs(
                            &config,
                            Evidence {
                                window_focused: focused,
                                session,
                            },
                        );
                        let expected = match (focused, session) {
                            (Some(true), Session::Same) => Outputs {
                                window: !visual,
                                sound: !sound,
                            },
                            _ => Outputs {
                                window: true,
                                sound: true,
                            },
                        };
                        assert_eq!(
                            actual, expected,
                            "visual={visual} sound={sound} focus={focused:?} session={session:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn unknown_origin_and_non_agent_events_are_delivered() {
        let config = Config::default();
        for source in [
            crate::events::Source::ClaudeHook,
            crate::events::Source::CodexHook,
            crate::events::Source::Demo,
        ] {
            let mut event = Event::demo("turn_complete").unwrap();
            event.source = source;
            event.session_id = Some("sanitized-session".into());
            assert_eq!(
                outputs(&config, observe(&event)),
                Outputs {
                    window: true,
                    sound: true
                }
            );
            event.window = Some(u64::MAX);
            assert_eq!(
                outputs(&config, observe(&event)),
                Outputs {
                    window: true,
                    sound: true
                }
            );
        }
    }
}
