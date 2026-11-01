//! Bounded, session-local warning decisions. All time comes from the resident.
use crate::{
    config::Config,
    events::{Event, Kind, Source},
};
use std::collections::VecDeque;

#[derive(Default)]
pub struct Warnings {
    context_disabled: bool,
    idle_disabled: bool,
    sessions: VecDeque<Session>,
    sequence: u64,
    pending: VecDeque<Pending>,
}

struct Pending {
    event: Event,
    logged: bool,
}

struct Session {
    event: Event,
    stopped: bool,
    deadline: Option<u64>,
    above: bool,
}

impl Warnings {
    /// Runtime gates shared by both agents; configured thresholds remain unchanged.
    pub fn enabled(&self, kind: Kind) -> bool {
        match kind {
            Kind::ContextWarning => !self.context_disabled,
            Kind::IdleWarning => !self.idle_disabled,
            _ => true,
        }
    }

    pub fn set_enabled(&mut self, kind: Kind, enabled: bool) {
        match kind {
            Kind::ContextWarning => self.context_disabled = !enabled,
            Kind::IdleWarning => {
                self.idle_disabled = !enabled;
                if !enabled {
                    for session in &mut self.sessions {
                        session.deadline = None;
                        session.stopped = false;
                    }
                }
            }
            _ => return,
        }
        if !enabled {
            self.pending.retain(|p| p.event.kind != kind);
        }
    }

    fn session(&mut self, event: &Event) -> &mut Session {
        let index = self
            .sessions
            .iter()
            .position(|s| s.event.source == event.source && s.event.session_id == event.session_id);
        let state = index
            .and_then(|i| self.sessions.remove(i))
            .unwrap_or_else(|| Session {
                event: event.clone(),
                stopped: false,
                deadline: None,
                above: false,
            });
        if self.sessions.len() >= 64
            && let Some(evicted) = self.sessions.pop_front()
        {
            self.pending
                .retain(|p| !same_session(&p.event, &evicted.event));
        }
        self.sessions.push_back(state);
        // A session was just pushed, so this index is always in bounds.
        let last = self.sessions.len() - 1;
        &mut self.sessions[last]
    }

    pub fn observe(&mut self, event: &Event, config: &Config, now: u64) {
        if event.session_id.is_none() || event.agent_id.is_some() || event.validate().is_err() {
            return;
        }
        let minutes = match event.source {
            Source::ClaudeHook => config.claude.warnings.idle_threshold(),
            Source::CodexHook => config.codex.warnings.idle_threshold(),
            _ => return,
        };
        if event.ended {
            self.sessions.retain(|s| {
                s.event.source != event.source || s.event.session_id != event.session_id
            });
            self.pending.retain(|p| !same_session(&p.event, event));
            return;
        }
        if self.idle_disabled
            || minutes.is_none()
            || matches!(event.kind, Kind::ContextWarning | Kind::IdleWarning)
        {
            return;
        }
        let pending_idle = self
            .pending
            .iter()
            .any(|p| same_session(&p.event, event) && p.event.kind == Kind::IdleWarning);
        let session = self.session(event);
        if event.activity || event.kind != Kind::TurnComplete {
            session.deadline = None;
            session.stopped = false;
            self.pending
                .retain(|p| !same_session(&p.event, event) || p.event.kind != Kind::IdleWarning);
        } else if !session.stopped || session.deadline.is_some() || pending_idle {
            session.stopped = true;
            session.event = event.clone();
            session.deadline = minutes
                .and_then(|m| m.checked_mul(60_000))
                .and_then(|ms| now.checked_add(ms));
            self.pending
                .retain(|p| !same_session(&p.event, event) || p.event.kind != Kind::IdleWarning);
        }
    }

    pub fn has_deadline(&self) -> bool {
        !self.pending.is_empty() || self.sessions.iter().any(|s| s.deadline.is_some())
    }

    pub fn pending(&self) -> Vec<Event> {
        self.pending.iter().map(|p| p.event.clone()).collect()
    }

    /// Returns true once when admission fails, so queue saturation logs once per warning.
    pub fn admission(&mut self, event: &Event, accepted: bool) -> bool {
        let Some(index) = self.pending.iter().position(|p| p.event == *event) else {
            return false;
        };
        if accepted {
            self.pending.remove(index);
            return false;
        }
        !std::mem::replace(&mut self.pending[index].logged, true)
    }

    fn warning(event: &Event, kind: Kind, sequence: &mut u64) -> Event {
        *sequence = sequence.wrapping_add(1);
        let mut warning = event.clone();
        warning.kind = kind;
        warning.request_id = Some(format!("aherald-warning-{sequence}"));
        warning.activity = false;
        warning.ended = false;
        warning
    }

    pub fn due(&mut self, now: u64, paused: bool) -> Vec<Event> {
        if paused {
            return Vec::new();
        }
        let mut due = Vec::new();
        for session in &mut self.sessions {
            if session.deadline.is_some_and(|deadline| now > deadline) {
                session.deadline = None;
                due.push(Self::warning(
                    &session.event,
                    Kind::IdleWarning,
                    &mut self.sequence,
                ));
            }
        }
        self.pending
            .extend(due.iter().cloned().map(|event| Pending {
                event,
                logged: false,
            }));
        due
    }

    pub fn context(
        &mut self,
        agent: crate::agent::Agent,
        id: &str,
        tokens: Option<u64>,
        config: &Config,
    ) -> Option<Event> {
        if self.context_disabled {
            return None;
        }
        let (threshold, source) = match agent {
            crate::agent::Agent::Claude => (
                config.claude.warnings.context_threshold()?,
                Source::ClaudeHook,
            ),
            crate::agent::Agent::Codex => (
                config.codex.warnings.context_threshold()?,
                Source::CodexHook,
            ),
        };
        let tokens = tokens?;
        let mut event = Event::demo("context_warning").ok()?;
        event.source = source;
        event.session_id = Some(id.into());
        event.validate().ok()?;
        let session = self.session(&event);
        let above = tokens > threshold.0;
        let warn = above && !session.above;
        session.above = above;
        if !above {
            self.pending.retain(|p| {
                !same_session(&p.event, &event) || p.event.kind != Kind::ContextWarning
            });
        }
        if !warn {
            return None;
        }
        let warning = Self::warning(&event, Kind::ContextWarning, &mut self.sequence);
        self.pending.push_back(Pending {
            event: warning.clone(),
            logged: false,
        });
        Some(warning)
    }
}

fn same_session(a: &Event, b: &Event) -> bool {
    a.source == b.source && a.session_id == b.session_id
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::Config,
        events::{Event, Kind, Source},
    };

    fn config() -> Config {
        Config::parse("[claude.warnings]\ncontext_tokens = \"120k\"\nidle_minutes = 5\n[codex.warnings]\nidle_minutes = 10", std::path::Path::new(".")).unwrap()
    }
    fn event(agent: Source, session: &str) -> Event {
        let mut e = Event::demo("turn_complete").unwrap();
        e.source = agent;
        e.session_id = Some(session.into());
        e
    }
    #[test]
    fn a_later_stop_rebases_a_pending_deadline_after_pure_model_continuation() {
        let mut state = Warnings::default();
        let config = config();
        let event = event(Source::ClaudeHook, "continued");
        state.observe(&event, &config, 0);
        state.observe(&event, &config, 240_000);
        assert!(state.due(300_001, false).is_empty());
        assert!(state.due(540_000, false).is_empty());
        assert_eq!(state.due(540_001, false).len(), 1);
    }

    #[test]
    fn pending_warnings_log_once_and_are_cancelled_by_new_activity_or_low_context() {
        let mut state = Warnings::default();
        let config = config();
        let warning = state
            .context(crate::agent::Agent::Claude, "a", Some(120001), &config)
            .unwrap();
        assert!(state.admission(&warning, false));
        assert!(!state.admission(&warning, false));
        assert_eq!(state.pending().len(), 1);
        state.context(crate::agent::Agent::Claude, "a", Some(120000), &config);
        assert!(state.pending().is_empty());
        let mut stop = event(Source::ClaudeHook, "a");
        state.observe(&stop, &config, 0);
        state.due(300001, false);
        assert_eq!(state.pending().len(), 1);
        stop.activity = true;
        state.observe(&stop, &config, 300002);
        assert!(state.pending().is_empty());
    }

    #[test]
    fn a_new_stop_replaces_an_idle_warning_still_blocked_by_a_full_queue() {
        let mut state = Warnings::default();
        let config = config();
        let stop = event(Source::ClaudeHook, "continued");
        state.observe(&stop, &config, 0);
        state.due(300001, false);
        assert_eq!(state.pending().len(), 1);
        state.observe(&stop, &config, 310000);
        assert!(state.pending().is_empty());
        assert!(state.due(610000, false).is_empty());
        assert_eq!(state.due(610001, false).len(), 1);
    }

    #[test]
    fn idle_is_strict_once_per_episode_and_separate_per_agent_and_session() {
        let mut state = Warnings::default();
        let config = config();
        let a = event(Source::ClaudeHook, "a");
        state.observe(&a, &config, 0);
        state.observe(&event(Source::ClaudeHook, "b"), &config, 1000);
        state.observe(&event(Source::CodexHook, "a"), &config, 0);
        assert!(state.due(300_000, false).is_empty());
        let first = state.due(300_001, false);
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].kind, Kind::IdleWarning);
        assert_eq!(first[0].session_id.as_deref(), Some("a"));
        assert_eq!(first[0].source, Source::ClaudeHook);
        state.admission(&first[0], true);
        let second = state.due(301_001, false);
        assert_eq!(second[0].session_id.as_deref(), Some("b"));
        state.admission(&second[0], true);
        assert!(state.due(600_000, false).is_empty());
        let third = state.due(600_001, false);
        assert_eq!(third[0].source, Source::CodexHook);
        state.admission(&third[0], true);
        assert!(state.due(900_000, false).is_empty());
        state.observe(&a, &config, 900_000); // duplicate completion is not a new episode
        assert!(state.due(1_300_000, false).is_empty());
    }
    #[test]
    fn activity_cancels_and_next_completion_rearms_without_cross_session_effects() {
        let mut state = Warnings::default();
        let config = config();
        let mut a = event(Source::ClaudeHook, "a");
        state.observe(&a, &config, 0);
        a.activity = true;
        state.observe(&a, &config, 200_000);
        assert!(state.due(400_000, false).is_empty());
        a.activity = false;
        state.observe(&a, &config, 500_000);
        assert_eq!(state.due(800_001, false).len(), 1);
        a.activity = true;
        state.observe(&a, &config, 900_000);
        a.activity = false;
        state.observe(&a, &config, 1_000_000);
        assert_eq!(state.due(1_300_001, false).len(), 1);
    }
    #[test]
    fn pause_postpones_delivery_and_session_end_cleans_context_and_timer() {
        let mut state = Warnings::default();
        let config = config();
        let mut a = event(Source::ClaudeHook, "a");
        state.observe(&a, &config, 0);
        assert!(state.due(400_000, true).is_empty());
        assert_eq!(state.due(400_001, false).len(), 1);
        assert!(
            state
                .context(crate::agent::Agent::Claude, "a", Some(120001), &config)
                .is_some()
        );
        a.ended = true;
        state.observe(&a, &config, 500_000);
        assert!(
            state
                .context(crate::agent::Agent::Claude, "a", Some(120001), &config)
                .is_some()
        );
        assert!(state.due(1_000_000, false).is_empty());
    }
    #[test]
    fn context_unknown_does_not_rearm_but_a_known_low_reading_does() {
        let mut state = Warnings::default();
        let config = config();
        assert!(
            state
                .context(crate::agent::Agent::Claude, "a", Some(120000), &config)
                .is_none()
        );
        let first = state
            .context(crate::agent::Agent::Claude, "a", Some(120001), &config)
            .unwrap();
        assert_eq!(first.kind, Kind::ContextWarning);
        assert_eq!(first.session_id.as_deref(), Some("a"));
        assert!(
            state
                .context(crate::agent::Agent::Claude, "a", None, &config)
                .is_none()
        );
        assert!(
            state
                .context(crate::agent::Agent::Claude, "a", Some(130000), &config)
                .is_none()
        );
        assert!(
            state
                .context(crate::agent::Agent::Claude, "b", Some(130000), &config)
                .is_some()
        );
        assert!(
            state
                .context(crate::agent::Agent::Claude, "a", Some(119999), &config)
                .is_none()
        );
        let next = state
            .context(crate::agent::Agent::Claude, "a", Some(120001), &config)
            .unwrap();
        assert_ne!(first.request_id, next.request_id);
    }
    #[test]
    fn subagents_invalid_identity_and_disabled_settings_do_not_start_timers() {
        let mut state = Warnings::default();
        let config = config();
        let mut e = event(Source::CodexHook, "a");
        e.agent_id = Some("child".into());
        state.observe(&e, &config, 0);
        e.agent_id = None;
        e.session_id = None;
        state.observe(&e, &config, 0);
        let disabled = Config::parse(
            "[claude.warnings]\nidle_minutes=0\n[codex.warnings]\nidle_minutes=0",
            std::path::Path::new("."),
        )
        .unwrap();
        state.observe(&event(Source::ClaudeHook, "b"), &disabled, 0);
        assert!(!state.has_deadline());
        assert!(
            state
                .context(
                    crate::agent::Agent::Claude,
                    "a",
                    Some(999999),
                    &Config::default()
                )
                .is_none()
        );
    }
    #[test]
    fn continuation_permission_error_and_interrupt_cancel_provisional_deadlines() {
        for kind in [
            Kind::PermissionRequired,
            Kind::InputRequired,
            Kind::Error,
            Kind::Interrupted,
        ] {
            let mut state = Warnings::default();
            let config = config();
            let mut e = event(Source::ClaudeHook, "a");
            state.observe(&e, &config, 0);
            e.kind = kind;
            state.observe(&e, &config, 100);
            assert!(!state.has_deadline());
            assert!(state.due(600_000, false).is_empty());
        }
    }
    #[test]
    fn state_is_bounded_and_oldest_session_is_evicted() {
        let mut state = Warnings::default();
        let config = config();
        for n in 0..65 {
            state.observe(&event(Source::ClaudeHook, &n.to_string()), &config, 0);
        }
        let warnings = state.due(300_001, false);
        assert_eq!(warnings.len(), 64);
        assert!(
            warnings
                .iter()
                .all(|e| e.session_id.as_deref() != Some("0"))
        );
    }
}
