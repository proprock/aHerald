//! Versioned PostModelRequest observer, verified with Codex 0.166.0-alpha.5.1.
//! Input tokens already include cached input; never sum them again.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub session_id: String,
    pub tokens: u64,
}

impl Report {
    pub fn valid(&self) -> bool {
        !self.session_id.is_empty()
            && self.session_id.len() <= 256
            && !self.session_id.chars().any(char::is_control)
            && self.tokens <= i64::MAX as u64
    }
}

pub fn parse(bytes: &[u8]) -> Result<Option<Report>, String> {
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
        session_id: Option<String>,
        agent_id: Option<serde_json::Value>,
        status: Option<String>,
        token_usage: Option<Usage>,
    }
    let wire: Wire = serde_json::from_slice(bytes).map_err(|_| "invalid hook JSON")?;
    if wire.hook_event_name != "PostModelRequest"
        || wire.status.as_deref() != Some("completed")
        || wire.agent_id.is_some()
    {
        return Ok(None);
    }
    let tokens = wire
        .token_usage
        .and_then(|u| u.input_tokens)
        .and_then(|n| n.as_u64());
    let report = match (wire.session_id, tokens) {
        (Some(session_id), Some(tokens)) => Report { session_id, tokens },
        _ => return Ok(None),
    };
    Ok(report.valid().then_some(report))
}
