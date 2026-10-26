use serde::{Deserialize, Serialize};
use std::{io::Read, sync::mpsc, time::Duration};
pub const MAX_INPUT: usize = 65_536;
pub const INPUT_TIMEOUT: Duration = Duration::from_millis(250);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    TurnComplete,
    PermissionRequired,
    InputRequired,
    Error,
    Interrupted,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    High,
    Medium,
    Low,
}
impl Kind {
    pub fn priority(self) -> Priority {
        match self {
            Self::Error => Priority::Medium,
            Self::Interrupted => Priority::Low,
            _ => Priority::High,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::TurnComplete => "turn_complete",
            Self::PermissionRequired => "permission_required",
            Self::InputRequired => "input_required",
            Self::Error => "error",
            Self::Interrupted => "interrupted",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Internal, user-requested usage card; never accepted over hook/IPC JSON.
    #[serde(skip)]
    Usage,
    CodexHook,
    ClaudeHook,
    Demo,
}
impl Source {
    /// The agent that produced a hook event; `None` for internal/demo sources.
    pub fn agent(&self) -> Option<crate::agent::Agent> {
        match self {
            Self::CodexHook => Some(crate::agent::Agent::Codex),
            Self::ClaudeHook => Some(crate::agent::Agent::Claude),
            Self::Usage | Self::Demo => None,
        }
    }
    pub fn agent_key(&self) -> &'static str {
        match self {
            Self::Usage => "usage",
            Self::Demo => "demo",
            hook => hook.agent().map_or("demo", |a| a.key()),
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            Self::Usage => "Codex",
            Self::Demo => "aHerald demo",
            hook => hook.agent().map_or("aHerald demo", |a| a.label()),
        }
    }
}
impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Self::TurnComplete => "Turn complete",
            Self::PermissionRequired => "Permission required",
            Self::InputRequired => "Input required",
            Self::Error => "Agent error",
            Self::Interrupted => "Turn interrupted",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub kind: Kind,
    pub source: Source,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub request_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
}
impl Event {
    pub fn is_terminal_evidence(&self) -> bool {
        let turn_ended = match self.source {
            // Codex ends are correlated by turn.
            Source::CodexHook => {
                matches!(self.kind, Kind::TurnComplete | Kind::Interrupted)
                    && self.turn_id.is_some()
            }
            // Claude hooks have no turn id; Stop/StopFailure end the session's turn.
            Source::ClaudeHook => matches!(self.kind, Kind::TurnComplete | Kind::Error),
            _ => false,
        };
        turn_ended && self.session_id.is_some() && self.validate().is_ok()
    }
    pub fn demo(name: &str) -> Result<Self, String> {
        let kind: Kind = serde_json::from_value(serde_json::Value::String(name.into()))
            .map_err(|_| "unknown event")?;
        Ok(Self {
            kind,
            source: Source::Demo,
            session_id: None,
            turn_id: None,
            request_id: None,
            agent_id: None,
        })
    }
    pub fn validate(&self) -> Result<(), String> {
        for id in [
            &self.session_id,
            &self.turn_id,
            &self.request_id,
            &self.agent_id,
        ]
        .into_iter()
        .flatten()
        {
            if id.is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
                return Err("invalid event identity".into());
            }
        }
        Ok(())
    }
}

pub fn parse_hook(bytes: &[u8]) -> Result<Option<Event>, String> {
    if bytes.len() > MAX_INPUT {
        return Err("hook input exceeds 64 KiB".into());
    }
    #[derive(Deserialize)]
    struct Hook {
        hook_event_name: String,
        session_id: Option<String>,
        turn_id: Option<String>,
        agent_id: Option<String>,
    }
    let hook: Hook = serde_json::from_slice(bytes).map_err(|_| "invalid hook JSON")?;
    let kind = match hook.hook_event_name.as_str() {
        "Stop" => Kind::TurnComplete,
        "PermissionRequest" => Kind::PermissionRequired,
        "Interrupt" => Kind::Interrupted,
        _ => return Ok(None),
    };
    let event = Event {
        kind,
        source: Source::CodexHook,
        session_id: hook.session_id,
        turn_id: hook.turn_id,
        request_id: None,
        agent_id: if kind == Kind::PermissionRequired {
            hook.agent_id
        } else {
            None
        },
    };
    event.validate()?;
    Ok(Some(event))
}

pub fn read_bounded(
    reader: impl Read + Send + 'static,
    timeout: Duration,
) -> Result<Vec<u8>, String> {
    let (tx, rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let result = reader
            .take(MAX_INPUT as u64 + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "cannot read hook input".to_owned())
            .and_then(|_| {
                if bytes.len() > MAX_INPUT {
                    Err("hook input exceeds 64 KiB".into())
                } else {
                    Ok(bytes)
                }
            });
        let _ = tx.send(result);
    });
    rx.recv_timeout(timeout)
        .map_err(|_| "hook input deadline exceeded".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normalize_ignores_sensitive_and_unknown_fields() {
        let event = parse_hook(br#"{"hook_event_name":"PermissionRequest","session_id":"s","turn_id":"t","tool_input":{"command":"SECRET"},"future":true}"#).unwrap().unwrap();
        assert_eq!(event.kind, Kind::PermissionRequired);
        assert_eq!(event.kind.priority(), Priority::High);
        assert_eq!(event.session_id.as_deref(), Some("s"));
        assert_eq!(event.turn_id.as_deref(), Some("t"));
        assert_eq!(event.request_id, None);
        assert!(!serde_json::to_string(&event).unwrap().contains("SECRET"));
    }
    #[test]
    fn optional_identity_and_event_mapping() {
        for (name, kind) in [
            ("Stop", Kind::TurnComplete),
            ("Interrupt", Kind::Interrupted),
        ] {
            let event = parse_hook(format!(r#"{{"hook_event_name":"{name}"}}"#).as_bytes())
                .unwrap()
                .unwrap();
            assert_eq!(event.kind, kind);
            assert_eq!(event.session_id, None);
        }
        assert_eq!(Kind::Error.priority(), Priority::Medium);
        assert_eq!(Kind::Interrupted.priority(), Priority::Low);
        assert!(
            parse_hook(br#"{"hook_event_name":"PostToolUse"}"#)
                .unwrap()
                .is_none()
        );
    }
    fn claude(json: &str) -> Option<Event> {
        crate::agent::Agent::Claude
            .parse_hook(json.as_bytes())
            .unwrap()
    }
    #[test]
    fn claude_notification_types_map_to_event_kinds() {
        for (ty, kind) in [
            ("permission_prompt", Some(Kind::PermissionRequired)),
            ("idle_prompt", Some(Kind::InputRequired)),
            ("elicitation_dialog", Some(Kind::InputRequired)),
            ("agent_needs_input", Some(Kind::InputRequired)),
            ("auth_success", None),
            ("future_type", None),
        ] {
            let event = claude(&format!(
                r#"{{"hook_event_name":"Notification","session_id":"s","notification_type":"{ty}","message":"SECRET"}}"#
            ));
            assert_eq!(event.as_ref().map(|e| e.kind), kind, "{ty}");
            if let Some(event) = event {
                assert_eq!(event.source, Source::ClaudeHook);
                assert_eq!(event.session_id.as_deref(), Some("s"));
                assert!(!serde_json::to_string(&event).unwrap().contains("SECRET"));
            }
        }
    }
    #[test]
    fn claude_stop_failure_and_permission_request_events() {
        for (json, kind) in [
            (
                r#"{"hook_event_name":"Stop","session_id":"s"}"#,
                Kind::TurnComplete,
            ),
            (
                r#"{"hook_event_name":"StopFailure","session_id":"s","error":"rate_limit"}"#,
                Kind::Error,
            ),
            (
                r#"{"hook_event_name":"PermissionRequest","session_id":"s","tool_name":"Bash","tool_input":{"command":"SECRET"}}"#,
                Kind::PermissionRequired,
            ),
        ] {
            let event = claude(json).unwrap();
            assert_eq!(event.kind, kind);
            assert_eq!(event.source, Source::ClaudeHook);
            assert!(!serde_json::to_string(&event).unwrap().contains("SECRET"));
        }
        assert!(claude(r#"{"hook_event_name":"PostToolUse"}"#).is_none());
        assert!(
            crate::agent::Agent::Claude.parse_hook(b"{").is_err(),
            "malformed JSON is an error"
        );
    }
    #[test]
    fn agent_identity_is_derived_from_source() {
        use crate::agent::Agent;
        assert_eq!(Source::CodexHook.agent(), Some(Agent::Codex));
        assert_eq!(Source::ClaudeHook.agent(), Some(Agent::Claude));
        assert_eq!(Source::Demo.agent(), None);
        assert_eq!(Source::ClaudeHook.agent_key(), "claude");
        assert_eq!(Source::ClaudeHook.label(), "Claude");
        assert_eq!(
            Agent::Codex
                .parse_hook(br#"{"hook_event_name":"Stop"}"#)
                .unwrap()
                .unwrap()
                .source,
            Source::CodexHook
        );
    }
    #[test]
    fn input_errors_and_boundaries() {
        for input in [
            b"{".as_slice(),
            b"[]",
            br#"{"hook_event_name":9}"#,
            br#"{"hook_event_name":"Stop","session_id":7}"#,
        ] {
            assert!(parse_hook(input).is_err());
        }
        assert!(parse_hook(&vec![b' '; MAX_INPUT + 1]).is_err());
        assert_eq!(
            read_bounded(
                std::io::Cursor::new(vec![0; MAX_INPUT]),
                Duration::from_secs(1)
            )
            .unwrap()
            .len(),
            MAX_INPUT
        );
        assert!(
            read_bounded(
                std::io::Cursor::new(vec![0; MAX_INPUT + 1]),
                Duration::from_secs(1)
            )
            .is_err()
        );
    }
}
