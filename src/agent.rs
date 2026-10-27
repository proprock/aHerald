//! Supported coding agents: identity, hook parsing and per-agent switches.
//!
//! A concrete enum rather than a plugin system: each new agent adds one variant
//! and one match arm per method.
use crate::{
    config::Config,
    events::{Event, Kind, MAX_INPUT, Source},
};
use serde::Deserialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Agent {
    Codex,
    Claude,
}

impl Agent {
    pub const ALL: [Agent; 2] = [Agent::Codex, Agent::Claude];

    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "codex" => Some(Self::Codex),
            "claude" => Some(Self::Claude),
            _ => None,
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Codex => "Codex",
            Self::Claude => "Claude",
        }
    }
    pub fn hook_source(self) -> Source {
        match self {
            Self::Codex => Source::CodexHook,
            Self::Claude => Source::ClaudeHook,
        }
    }
    pub fn hooks_enabled(self, config: &Config) -> bool {
        match self {
            Self::Codex => config.codex.hooks_enabled,
            Self::Claude => config.claude.hooks_enabled,
        }
    }
    /// Normalizes one hook payload; `Ok(None)` means a valid but irrelevant event.
    pub fn parse_hook(self, bytes: &[u8]) -> Result<Option<Event>, String> {
        match self {
            Self::Codex => crate::events::parse_hook(bytes),
            Self::Claude => parse_claude_hook(bytes),
        }
    }
    /// Whether the hook contract expects `{}` on stdout for this event.
    pub fn hook_reply(self, event: &Event) -> Option<&'static str> {
        match self {
            Self::Codex if event.kind == Kind::TurnComplete => Some("{}"),
            // Claude Code treats empty stdout and exit 0 as "no decision".
            _ => None,
        }
    }
}

fn parse_claude_hook(bytes: &[u8]) -> Result<Option<Event>, String> {
    if bytes.len() > MAX_INPUT {
        return Err("hook input exceeds 64 KiB".into());
    }
    #[derive(Deserialize)]
    struct Hook {
        hook_event_name: String,
        session_id: Option<String>,
        notification_type: Option<String>,
    }
    let hook: Hook = serde_json::from_slice(bytes).map_err(|_| "invalid hook JSON")?;
    let kind = match hook.hook_event_name.as_str() {
        "Stop" => Kind::TurnComplete,
        "StopFailure" => Kind::Error,
        "PermissionRequest" => Kind::PermissionRequired,
        "Notification" => match hook.notification_type.as_deref() {
            Some("permission_prompt") => Kind::PermissionRequired,
            Some("idle_prompt" | "elicitation_dialog" | "agent_needs_input") => Kind::InputRequired,
            _ => return Ok(None),
        },
        _ => return Ok(None),
    };
    let event = Event {
        kind,
        source: Source::ClaudeHook,
        session_id: hook.session_id,
        turn_id: None,
        request_id: None,
        agent_id: None,
        window: None,
    };
    event.validate()?;
    Ok(Some(event))
}
