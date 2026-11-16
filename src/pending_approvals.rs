//! Evidence that a Codex or Claude Code wait was answered: Codex requests held back for
//! `[codex] permission_delay_sec` (AH-304) and wait cards already shown (AH-305).
//!
//! Codex's automatic reviewer answers most requests within seconds, and a hook cannot tell that
//! request from one a person must answer. The queue therefore holds a Codex request for a short
//! delay. A held request and a shown card both leave when the same source, session, agent and
//! (Codex only) turn show that it was answered: a tool that was open when the request arrived
//! finished (all of them, when several were open), the next Codex model request completed, the
//! user submitted a prompt, or the turn or session ended. `PreToolUse` alone is not evidence
//! (`docs/codex-approval-observation.md`, `docs/claude-hooks.md`). Claude hooks carry no turn,
//! so a Claude wait is keyed by its session. Time is the caller's clock in milliseconds; nothing
//! here does I/O.
use crate::events::{Event, Kind, Source, ToolPhase};
use std::collections::VecDeque;

/// Turns (Claude: sessions) whose open tool calls are remembered; the oldest is forgotten first.
const MAX_TURNS: usize = 64;
/// Open tool calls remembered per turn; the oldest is forgotten first.
const MAX_TOOLS: usize = 32;
/// Requests held at once; a further request becomes a card at once.
const MAX_HELD: usize = 64;
/// Shown cards watched at once (the queue holds at most 64); the oldest is forgotten first.
const MAX_CARDS: usize = 64;
/// Claude sessions whose answered prompt may still send its trailing notification.
const MAX_ANSWERED: usize = 64;

/// Where a wait belongs: a Codex turn, or a Claude session (its hooks carry no turn).
#[derive(Clone, Debug, PartialEq, Eq)]
struct WaitKey {
    source: Source,
    session: String,
    turn: Option<String>,
    agent: Option<String>,
}

impl WaitKey {
    fn of(event: &Event) -> Option<Self> {
        let turn = match event.source {
            Source::CodexHook => Some(event.turn_id.clone()?),
            Source::ClaudeHook => None,
            _ => return None,
        };
        Some(Self {
            source: event.source.clone(),
            session: event.session_id.clone()?,
            turn,
            agent: event.agent_id.clone(),
        })
    }
}

/// One wait and the evidence that can still settle it.
#[derive(Clone, Debug)]
pub struct Waiter {
    key: WaitKey,
    /// Tool calls open when the request arrived and not finished since.
    waiting_for: Vec<String>,
    /// Whether any tool was open on arrival: only then can tool finishes settle it.
    tool_bound: bool,
}

/// What one piece of lifecycle evidence says.
enum Evidence<'a> {
    ToolFinished(&'a WaitKey, &'a str),
    /// A prompt, a completed model request, or the end of the turn.
    StepOver(&'a WaitKey),
    SessionOver {
        source: &'a Source,
        session: &'a str,
        agent: &'a Option<String>,
    },
}

impl Waiter {
    /// Applies `evidence` and says whether the wait is now settled. The single rule for held
    /// requests and shown cards of every hook source.
    fn settled_by(&mut self, evidence: &Evidence) -> bool {
        match evidence {
            Evidence::ToolFinished(key, id) if self.key == **key => {
                self.waiting_for.retain(|open| open != id);
                self.tool_bound && self.waiting_for.is_empty()
            }
            Evidence::ToolFinished(..) => false,
            Evidence::StepOver(key) => self.key == **key,
            Evidence::SessionOver {
                source,
                session,
                agent,
            } => {
                self.key.source == **source
                    && self.key.session == *session
                    && self.key.agent == **agent
            }
        }
    }
}

/// A request waiting for its delay, with what the queue needs to admit it later.
#[derive(Clone, Debug)]
pub struct Held {
    pub event: Event,
    pub outputs: crate::focus::Outputs,
    pub duration: u64,
    release_at: u64,
    /// Carried over to the card when the request is released.
    pub waiter: Waiter,
}

/// What evidence settled: how many held requests, and which shown cards (queue entry ids).
#[derive(Debug, Default)]
pub struct Settled {
    pub held: usize,
    pub cards: Vec<u64>,
}

impl Settled {
    fn any(&self) -> bool {
        self.held > 0 || !self.cards.is_empty()
    }
}

#[derive(Debug, Default)]
pub struct PendingApprovals {
    held: Vec<Held>,
    cards: VecDeque<(u64, Waiter)>,
    open: VecDeque<(WaitKey, VecDeque<String>)>,
    /// Claude sessions whose prompt a tool finish answered while no other tool was open. Claude's
    /// `permission_prompt` notification follows a prompt by about six seconds, so it can arrive
    /// after the card left; every new prompt comes after a `PreToolUse`, which forgets the entry.
    answered: VecDeque<WaitKey>,
}

impl PendingApprovals {
    /// Whether `event` is a Codex request to hold now. A request without a turn is never held:
    /// its evidence could not be told apart from another turn's. With the held list full, a
    /// further request becomes a card at once.
    pub fn takes(&self, event: &Event, delay_ms: u64) -> bool {
        delay_ms > 0
            && self.held.len() < MAX_HELD
            && event.source == Source::CodexHook
            && event.kind == Kind::PermissionRequired
            && self.waiter(event).is_some()
    }

    /// The waiter of a Codex wait with a turn or a Claude wait with a session, with the tools
    /// open now; `None` for any other event.
    pub fn waiter(&self, event: &Event) -> Option<Waiter> {
        if !matches!(event.kind, Kind::PermissionRequired | Kind::InputRequired)
            || event.activity
            || event.ended
        {
            return None;
        }
        let key = WaitKey::of(event)?;
        let waiting_for: Vec<String> = self
            .open
            .iter()
            .find(|(k, _)| *k == key)
            .map(|(_, ids)| ids.iter().cloned().collect())
            .unwrap_or_default();
        Some(Waiter {
            tool_bound: !waiting_for.is_empty(),
            waiting_for,
            key,
        })
    }

    /// Whether `event` is the trailing `permission_prompt` of a Claude prompt already answered.
    pub fn trails_answered(&self, event: &Event) -> bool {
        event.source == Source::ClaudeHook
            && event.kind == Kind::PermissionRequired
            && WaitKey::of(event).is_some_and(|key| self.answered.contains(&key))
    }

    /// Holds `event` until `release_at`. Call it only for an event `takes` accepted.
    pub fn hold(
        &mut self,
        event: Event,
        outputs: crate::focus::Outputs,
        duration: u64,
        release_at: u64,
    ) {
        let Some(waiter) = self.waiter(&event) else {
            debug_assert!(false, "hold() called for an event takes() refuses");
            return;
        };
        self.held.push(Held {
            event,
            outputs,
            duration,
            release_at,
            waiter,
        });
    }

    /// Watches the shown card `id` for the evidence that settles `waiter`.
    pub fn watch(&mut self, id: u64, waiter: Waiter) {
        if self.cards.len() == MAX_CARDS {
            self.cards.pop_front();
        }
        self.cards.push_back((id, waiter));
    }

    /// Forgets watched cards that left the queue some other way.
    pub fn keep_cards(&mut self, live: impl Fn(u64) -> bool) {
        self.cards.retain(|(id, _)| live(*id));
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

    /// Records Codex or Claude lifecycle evidence and returns what it settled.
    pub fn observe(&mut self, event: &Event) -> Settled {
        if !matches!(event.source, Source::CodexHook | Source::ClaudeHook)
            || event.validate().is_err()
        {
            return Settled::default();
        }
        if event.ended {
            let Some(session) = &event.session_id else {
                return Settled::default();
            };
            let gone = |k: &WaitKey| {
                k.source == event.source && k.session == *session && k.agent == event.agent_id
            };
            self.open.retain(|(k, _)| !gone(k));
            self.answered.retain(|k| !gone(k));
            return self.settle(&Evidence::SessionOver {
                source: &event.source,
                session,
                agent: &event.agent_id,
            });
        }
        let Some(key) = WaitKey::of(event) else {
            return Settled::default();
        };
        if event.activity {
            return match (event.tool_phase, &event.tool_use_id) {
                (Some(ToolPhase::Start), Some(id)) => {
                    self.answered.retain(|k| *k != key);
                    self.open_tool(key, id);
                    Settled::default()
                }
                (Some(ToolPhase::Finish), Some(id)) => self.finish_tool(key, id),
                (Some(_), None) => Settled::default(),
                // Tool activity from a hook that predates `tool_phase`: start or finish unknown.
                (None, Some(_)) => Settled::default(),
                // A user prompt, a completed Codex model request or a busy Claude `Stop`. The
                // step that opened the tools is over: a call without a finish by now was denied
                // and never sends one.
                (None, None) => self.step_over(&key),
            };
        }
        if event.is_terminal_evidence() {
            return self.step_over(&key);
        }
        Settled::default()
    }

    fn step_over(&mut self, key: &WaitKey) -> Settled {
        self.open.retain(|(k, _)| k != key);
        self.answered.retain(|k| k != key);
        self.settle(&Evidence::StepOver(key))
    }

    fn finish_tool(&mut self, key: WaitKey, id: &str) -> Settled {
        let mut idle = true;
        if let Some((_, ids)) = self.open.iter_mut().find(|(k, _)| *k == key) {
            ids.retain(|open| open != id);
            idle = ids.is_empty();
        }
        let settled = self.settle(&Evidence::ToolFinished(&key, id));
        if settled.any()
            && idle
            && key.source == Source::ClaudeHook
            && !self.answered.contains(&key)
        {
            if self.answered.len() == MAX_ANSWERED {
                self.answered.pop_front();
            }
            self.answered.push_back(key);
        }
        settled
    }

    fn open_tool(&mut self, key: WaitKey, id: &str) {
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

    fn settle(&mut self, evidence: &Evidence) -> Settled {
        let before = self.held.len();
        self.held.retain_mut(|h| !h.waiter.settled_by(evidence));
        let mut cards = Vec::new();
        self.cards.retain_mut(|(id, waiter)| {
            let settled = waiter.settled_by(evidence);
            if settled {
                cards.push(*id);
            }
            !settled
        });
        Settled {
            held: before - self.held.len(),
            cards,
        }
    }
}
