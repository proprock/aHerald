//! Codex permission requests held back for `[codex] permission_delay_sec` (AH-304).
//!
//! Codex's automatic reviewer answers most requests within seconds, and a hook cannot tell that
//! request from one a person must answer. The queue therefore holds a Codex request for a short
//! delay and drops it when the same session, turn and agent shows that it was answered: a tool
//! that was open when the request arrived finished (all of them, when several were open), the
//! next model request completed, the user submitted a prompt, or the turn or session ended.
//! `PreToolUse` alone is not evidence (`docs/codex-approval-observation.md`). Time is the
//! caller's clock in milliseconds; nothing here does I/O.
use crate::events::{Event, Kind, Source, ToolPhase};
use std::collections::VecDeque;

/// Turns whose open tool calls are remembered; the oldest is forgotten first.
const MAX_TURNS: usize = 64;
/// Open tool calls remembered per turn; the oldest is forgotten first.
const MAX_TOOLS: usize = 32;
/// Requests held at once; a further request becomes a card at once.
const MAX_HELD: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
struct TurnKey {
    session: String,
    turn: String,
    agent: Option<String>,
}

impl TurnKey {
    fn of(event: &Event) -> Option<Self> {
        Some(Self {
            session: event.session_id.clone()?,
            turn: event.turn_id.clone()?,
            agent: event.agent_id.clone(),
        })
    }
}

/// A request waiting for its delay, with what the queue needs to admit it later.
#[derive(Clone, Debug)]
pub struct Held {
    pub event: Event,
    pub outputs: crate::focus::Outputs,
    pub duration: u64,
    release_at: u64,
    key: TurnKey,
    /// Tool calls open when the request arrived and not finished since.
    waiting_for: Vec<String>,
    /// Whether any tool was open on arrival: only then can tool finishes resolve it.
    tool_bound: bool,
}

#[derive(Debug, Default)]
pub struct PendingApprovals {
    held: Vec<Held>,
    open: VecDeque<(TurnKey, VecDeque<String>)>,
}

impl PendingApprovals {
    /// Whether `event` is a request to hold now. A request without a turn is never held: its
    /// evidence could not be told apart from another turn's. With the held list full, a
    /// further request becomes a card at once.
    pub fn takes(&self, event: &Event, delay_ms: u64) -> bool {
        delay_ms > 0
            && self.held.len() < MAX_HELD
            && event.source == Source::CodexHook
            && event.kind == Kind::PermissionRequired
            && !event.activity
            && !event.ended
            && TurnKey::of(event).is_some()
    }

    /// Holds `event` until `release_at`. Call it only for an event `takes` accepted.
    pub fn hold(
        &mut self,
        event: Event,
        outputs: crate::focus::Outputs,
        duration: u64,
        release_at: u64,
    ) {
        let Some(key) = TurnKey::of(&event) else {
            debug_assert!(false, "hold() called for an event takes() refuses");
            return;
        };
        let waiting_for: Vec<String> = self
            .open
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, ids)| ids.iter().cloned().collect())
            .unwrap_or_default();
        self.held.push(Held {
            event,
            outputs,
            duration,
            release_at,
            tool_bound: !waiting_for.is_empty(),
            waiting_for,
            key,
        });
    }

    pub fn is_empty(&self) -> bool {
        self.held.is_empty()
    }

    /// Removes and returns the requests whose delay has passed, oldest first.
    pub fn due(&mut self, now: u64) -> Vec<Held> {
        let (due, kept) = std::mem::take(&mut self.held)
            .into_iter()
            .partition(|h| h.release_at <= now);
        self.held = kept;
        due
    }

    /// Records Codex lifecycle evidence and returns how many held requests it resolved.
    pub fn observe(&mut self, event: &Event) -> usize {
        if event.source != Source::CodexHook || event.validate().is_err() {
            return 0;
        }
        if event.ended {
            let Some(session) = &event.session_id else {
                return 0;
            };
            let gone = |k: &TurnKey| k.session == *session && k.agent == event.agent_id;
            self.open.retain(|(k, _)| !gone(k));
            return self.resolve(|h| gone(&h.key));
        }
        let Some(key) = TurnKey::of(event) else {
            return 0;
        };
        if event.activity {
            return match (event.tool_phase, &event.tool_use_id) {
                (Some(ToolPhase::Start), Some(id)) => {
                    self.open_tool(key, id);
                    0
                }
                (Some(ToolPhase::Finish), Some(id)) => self.finish_tool(&key, id),
                (Some(_), None) => 0,
                // Tool activity from a hook that predates `tool_phase`: start or finish unknown.
                (None, Some(_)) => 0,
                // A user prompt or a completed model request. The model step that opened the
                // tools is over: a call without `PostToolUse` by now was denied and never will.
                (None, None) => {
                    self.open.retain(|(k, _)| *k != key);
                    self.resolve(|h| h.key == key)
                }
            };
        }
        if matches!(event.kind, Kind::TurnComplete | Kind::Interrupted) {
            self.open.retain(|(k, _)| *k != key);
            return self.resolve(|h| h.key == key);
        }
        0
    }

    fn open_tool(&mut self, key: TurnKey, id: &str) {
        let at = match self.open.iter().position(|(k, _)| *k == key) {
            Some(at) => at,
            None => {
                if self.open.len() == MAX_TURNS {
                    self.open.pop_front();
                }
                self.open.push_back((key, VecDeque::new()));
                self.open.len() - 1
            }
        };
        let ids = &mut self.open[at].1;
        if !ids.iter().any(|open| open == id) {
            if ids.len() == MAX_TOOLS {
                ids.pop_front();
            }
            ids.push_back(id.to_owned());
        }
    }

    fn finish_tool(&mut self, key: &TurnKey, id: &str) -> usize {
        if let Some((_, ids)) = self.open.iter_mut().find(|(k, _)| k == key) {
            ids.retain(|open| open != id);
        }
        for held in self.held.iter_mut().filter(|h| h.key == *key) {
            held.waiting_for.retain(|open| open != id);
        }
        self.resolve(|h| h.key == *key && h.tool_bound && h.waiting_for.is_empty())
    }

    fn resolve(&mut self, matches: impl Fn(&Held) -> bool) -> usize {
        let before = self.held.len();
        self.held.retain(|h| !matches(h));
        before - self.held.len()
    }
}
