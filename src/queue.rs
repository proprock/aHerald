//! Deterministic bounded event queue; time is supplied by the caller.
use crate::events::{Event, Kind, Priority, Source};
use std::collections::VecDeque;
#[derive(Clone, Debug)]
pub struct Entry {
    pub id: u64,
    pub event: Event,
    pub duration: u64,
    pub visible_ms: u64,
    /// Set once the entry's sound was handed out, selected or not.
    pub announced: bool,
}
impl Entry {
    pub fn is_usage(&self) -> bool {
        self.event.source == Source::Usage
    }
    /// The queue lifetime includes fade-out; zero denotes a persistent card.
    pub fn display_duration(&self, fade_out_ms: u64) -> u64 {
        self.duration.saturating_sub(fade_out_ms)
    }
}
pub struct Queue {
    items: Vec<Entry>,
    selected: Option<u64>,
    history: VecDeque<(Event, u64)>,
    capacity: usize,
    dedup_ms: u64,
    sequence: u64,
    last_tick: Option<u64>,
    /// Whether a card was selected at the previous tick; idle gaps are not display time.
    counting: bool,
    terminal_history: VecDeque<Event>,
}
fn rank(e: &Event) -> u8 {
    match e.kind.priority() {
        Priority::High => 0,
        Priority::Medium => 1,
        Priority::Low => 2,
    }
}
fn same(a: &Event, b: &Event) -> bool {
    a.kind == b.kind
        && a.source.agent_key() == b.source.agent_key()
        && a.agent_id == b.agent_id
        // Hooks expose no request ID: identical turn IDs can be distinct waits.
        // Claude hooks carry no turn either, so one session has one pending card
        // (a PermissionRequest/permission_prompt pair is one prompt).
        && (!pending(a) || a.source != Source::CodexHook || a.request_id.is_some())
        && a.session_id == b.session_id
        && a.turn_id == b.turn_id
        && a.request_id == b.request_id
        && (a.session_id.is_some() || (a.source == Source::Demo && b.source == Source::Demo))
}
fn pending(event: &Event) -> bool {
    matches!(event.kind, Kind::PermissionRequired | Kind::InputRequired)
}
fn same_turn(a: &Event, b: &Event) -> bool {
    a.source.agent_key() == b.source.agent_key()
        && a.agent_id == b.agent_id
        && a.session_id.is_some()
        && a.turn_id.is_some()
        && a.session_id == b.session_id
        && a.turn_id == b.turn_id
}
impl Queue {
    pub fn new(capacity: usize, dedup_ms: u64) -> Self {
        Self {
            items: Vec::new(),
            selected: None,
            history: VecDeque::new(),
            capacity: capacity.clamp(1, 64),
            dedup_ms,
            sequence: 0,
            last_tick: None,
            counting: false,
            terminal_history: VecDeque::new(),
        }
    }
    pub fn is_obsolete(&self, event: &Event) -> bool {
        pending(event)
            && self
                .terminal_history
                .iter()
                .any(|terminal| same_turn(terminal, event))
    }
    /// Process verified lifecycle evidence even while presentation is disabled.
    pub fn observe_terminal(&mut self, event: &Event) -> usize {
        if !event.is_terminal_evidence() {
            return 0;
        }
        if event.source == Source::ClaudeHook {
            // Claude turns in one session are sequential, so the end of a turn
            // resolves every pending card of that session (and no other agent's).
            return self.resolve(|e| {
                pending(e)
                    && e.source == Source::ClaudeHook
                    && e.agent_id == event.agent_id
                    && e.session_id == event.session_id
            });
        }
        // Another Stop hook can continue the same turn. Only interruption proves
        // that a later-delivered wait belongs to an already terminated turn.
        if event.kind == Kind::Interrupted
            && !self.terminal_history.iter().any(|e| same_turn(e, event))
        {
            self.terminal_history.push_back(event.clone());
            while self.terminal_history.len() > 128 {
                self.terminal_history.pop_front();
            }
        }
        self.resolve(|e| pending(e) && same_turn(e, event))
    }
    fn resolve(&mut self, matches: impl Fn(&Event) -> bool) -> usize {
        let previous_index = self.selected_index();
        let count = self.items.len();
        self.items.retain(|entry| !matches(&entry.event));
        if self.selected().is_none() {
            self.selected = self
                .items
                .get(previous_index.min(self.items.len().saturating_sub(1)))
                .map(|e| e.id);
        }
        count - self.items.len()
    }
    /// Entries whose sound has not played yet, at most `limit`, in queue order.
    /// Every new agent event is announced once, whether or not it is selected.
    pub fn take_unannounced(&mut self, limit: usize) -> Vec<Entry> {
        let mut taken = Vec::new();
        for entry in self.items.iter_mut().filter(|e| !e.announced) {
            if taken.len() == limit {
                break;
            }
            entry.announced = true;
            taken.push(entry.clone());
        }
        taken
    }
    pub fn is_duplicate(&self, event: &Event, now: u64) -> bool {
        self.dedup_ms > 0
            && self
                .history
                .iter()
                .any(|(e, time)| now.saturating_sub(*time) <= self.dedup_ms && same(e, event))
    }
    pub fn push(&mut self, event: Event, now: u64, duration: u64) -> bool {
        if event.validate().is_err() {
            return false;
        }
        self.observe_terminal(&event);
        if self.is_obsolete(&event) {
            return false;
        }
        // Claude's PermissionRequest and permission_prompt Notification for one prompt can
        // arrive seconds apart (6 s observed), so an unresolved card absorbs the repeat.
        let repeats_open_claude_wait = event.source == Source::ClaudeHook
            && pending(&event)
            && self.items.iter().any(|e| same(&e.event, &event));
        if repeats_open_claude_wait || self.is_duplicate(&event, now) {
            for (e, t) in &mut self.history {
                if same(e, &event) {
                    *t = now;
                }
            }
            return false;
        }
        if self.items.iter().filter(|e| !e.is_usage()).count() == self.capacity {
            let worst = self
                .items
                .iter()
                .enumerate()
                .filter(|(_, e)| !e.is_usage())
                .max_by_key(|(_, e)| (rank(&e.event), std::cmp::Reverse(e.id)))
                .map(|(i, _)| i)
                .unwrap_or(0);
            if rank(&event) >= rank(&self.items[worst].event) {
                return false;
            }
            self.items.remove(worst);
        }
        self.sequence += 1;
        let id = self.sequence;
        self.history.push_back((event.clone(), now));
        while self.history.len() > 128 {
            self.history.pop_front();
        }
        self.items.push(Entry {
            id,
            event,
            duration,
            visible_ms: 0,
            announced: false,
        });
        self.items.sort_by_key(|e| (rank(&e.event), e.id));
        let selected = self.selected();
        if selected.is_none_or(|e| rank(&self.items[0].event) < rank(&e.event)) {
            self.selected = Some(self.items[0].id);
        }
        true
    }
    /// One internal persistent card shares navigation with agent notifications.
    /// It has a reserved slot so opening usage never evicts an agent event.
    pub fn show_usage(&mut self) {
        if let Some(entry) = self.items.iter().find(|e| e.is_usage()) {
            self.selected = Some(entry.id);
            return;
        }
        self.sequence += 1;
        let event = Event {
            source: Source::Usage,
            kind: Kind::TurnComplete,
            session_id: None,
            turn_id: None,
            request_id: None,
            agent_id: None,
            window: None,
        };
        self.items.push(Entry {
            id: self.sequence,
            event,
            duration: 0,
            visible_ms: 0,
            announced: true,
        });
        self.selected = Some(self.sequence);
    }
    pub fn items(&self) -> &[Entry] {
        &self.items
    }
    pub fn selected(&self) -> Option<&Entry> {
        self.items.iter().find(|e| Some(e.id) == self.selected)
    }
    pub fn selected_index(&self) -> usize {
        self.items
            .iter()
            .position(|e| Some(e.id) == self.selected)
            .unwrap_or(0)
    }
    pub fn next(&mut self, delta: i32) {
        if !self.items.is_empty() {
            let i =
                (self.selected_index() as i32 + delta).rem_euclid(self.items.len() as i32) as usize;
            self.selected = Some(self.items[i].id);
        }
    }
    pub fn dismiss(&mut self) {
        if self.items.is_empty() {
            return;
        }
        let i = self.selected_index();
        self.items.remove(i);
        self.selected = self
            .items
            .get(i.min(self.items.len().saturating_sub(1)))
            .map(|e| e.id);
    }
    pub fn tick(&mut self, now: u64, paused: bool) {
        // While idle the resident ticks rarely (usage polling), so a card that arrives after
        // a long gap must not be charged for the time before it existed.
        let elapsed = match self.last_tick.replace(now) {
            Some(t) if self.counting => now.saturating_sub(t),
            _ => 0,
        };
        if !paused {
            let id = self.selected;
            if let Some(e) = self.items.iter_mut().find(|e| Some(e.id) == id) {
                e.visible_ms = e.visible_ms.saturating_add(elapsed);
                if e.duration > 0 && e.visible_ms >= e.duration {
                    self.dismiss();
                }
            }
        }
        self.counting = self.selected.is_some();
    }
}
