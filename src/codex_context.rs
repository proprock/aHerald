//! Versioned PostModelRequest observer, verified with Codex 0.166.0-alpha.5.1 and 0.169.0.
//! Input tokens already include cached input; never sum them again.
use serde::{Deserialize, Serialize};

/// The input context of a main-agent model request, for the context warning. `turn_id` is new
/// in AH-303; an older resident ignores it and still takes the report.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub session_id: String,
    pub tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
}

impl Report {
    pub fn valid(&self) -> bool {
        crate::events::is_identity(&self.session_id)
            && self
                .turn_id
                .as_deref()
                .is_none_or(crate::events::is_identity)
            && self.tokens <= i64::MAX as u64
    }
}

/// A completed model request of a session, its turn and, for a subagent, the agent: progress
/// the resident takes as activity. `tokens` is the main agent's input context when reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Progress {
    pub session_id: String,
    pub turn_id: Option<String>,
    pub agent_id: Option<String>,
    pub tokens: Option<u64>,
}

impl Progress {
    /// The context report of a main-agent request that has a token count. A subagent's tokens
    /// are not its parent's context, so its progress never becomes one.
    pub fn report(&self) -> Option<Report> {
        if self.agent_id.is_some() {
            return None;
        }
        Some(Report {
            session_id: self.session_id.clone(),
            tokens: self.tokens?,
            turn_id: self.turn_id.clone(),
        })
    }

    /// The request for a running resident. A report goes as `CodexContext`, which an older
    /// resident still accepts; anything else goes as `CodexProgress`, which it rejects (it had
    /// no use for it).
    pub fn request(self) -> crate::ipc::Request {
        match self.report() {
            Some(report) => crate::ipc::Request::CodexContext { report },
            None => crate::ipc::Request::CodexProgress {
                session_id: self.session_id,
                turn_id: self.turn_id,
                agent_id: self.agent_id,
            },
        }
    }

    /// The same progress without its token count, as a guest's request is taken: a guest
    /// reports progress, not the context of this machine's warnings.
    pub fn without_tokens(self) -> Self {
        Self {
            tokens: None,
            ..self
        }
    }
}

/// A completed `PostModelRequest` of any agent; `None` for other events, unfinished requests and
/// invalid identities.
pub fn progress(bytes: &[u8]) -> Result<Option<Progress>, String> {
    if bytes.len() > crate::events::MAX_INPUT {
        return Err("hook input exceeds 64 KiB".into());
    }
    #[derive(Deserialize)]
    struct Usage {
        input_tokens: Option<serde_json::Value>,
    }
    #[derive(Deserialize)]
    struct Wire {
        hook_event_name: String,
        session_id: Option<serde_json::Value>,
        turn_id: Option<serde_json::Value>,
        agent_id: Option<serde_json::Value>,
        status: Option<String>,
        token_usage: Option<Usage>,
    }
    let wire: Wire = serde_json::from_slice(bytes).map_err(|_| "invalid hook JSON")?;
    if wire.hook_event_name != "PostModelRequest" || wire.status.as_deref() != Some("completed") {
        return Ok(None);
    }
    // A present id must be a valid one; a missing optional id is fine.
    let identity = |value: Option<serde_json::Value>| -> Result<Option<String>, ()> {
        match value {
            None | Some(serde_json::Value::Null) => Ok(None),
            Some(serde_json::Value::String(id)) if crate::events::is_identity(&id) => Ok(Some(id)),
            Some(_) => Err(()),
        }
    };
    let (Ok(Some(session_id)), Ok(turn_id), Ok(agent_id)) = (
        identity(wire.session_id),
        identity(wire.turn_id),
        identity(wire.agent_id),
    ) else {
        return Ok(None);
    };
    let tokens = wire
        .token_usage
        .and_then(|u| u.input_tokens)
        .and_then(|n| n.as_u64())
        .filter(|&n| n <= i64::MAX as u64);
    Ok(Some(Progress {
        session_id,
        turn_id,
        agent_id,
        tokens,
    }))
}

/// The context report of a completed main-agent `PostModelRequest` with a token count.
pub fn parse(bytes: &[u8]) -> Result<Option<Report>, String> {
    Ok(progress(bytes)?.and_then(|progress| progress.report()))
}
