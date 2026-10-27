//! Deterministic bounded event queue; time is supplied by the caller.
use crate::events::{Event, Priority, Source};
use std::collections::VecDeque;
#[derive(Clone, Debug)]
pub struct Entry {
    pub id: u64,
    pub event: Event,
    pub duration: u64,
    pub visible_ms: u64,
}
pub struct Queue {
    items: Vec<Entry>,
    selected: Option<u64>,
    history: VecDeque<(Event, u64)>,
    capacity: usize,
    dedup_ms: u64,
    sequence: u64,
    last_tick: Option<u64>,
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
        && a.session_id == b.session_id
        && a.turn_id == b.turn_id
        && a.request_id == b.request_id
        && (a.session_id.is_some() || (a.source == Source::Demo && b.source == Source::Demo))
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
        }
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
        if self.is_duplicate(&event, now) {
            for (e, t) in &mut self.history {
                if same(e, &event) {
                    *t = now;
                }
            }
            return false;
        }
        if self.items.len() == self.capacity {
            let worst = self
                .items
                .iter()
                .enumerate()
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
        });
        self.items.sort_by_key(|e| (rank(&e.event), e.id));
        let selected = self.selected();
        if selected.is_none_or(|e| rank(&self.items[0].event) < rank(&e.event)) {
            self.selected = Some(self.items[0].id);
        }
        true
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
        let elapsed = self
            .last_tick
            .replace(now)
            .map(|t| now.saturating_sub(t))
            .unwrap_or(0);
        if paused {
            return;
        }
        let id = self.selected;
        if let Some(e) = self.items.iter_mut().find(|e| Some(e.id) == id) {
            e.visible_ms = e.visible_ms.saturating_add(elapsed);
            if e.duration > 0 && e.visible_ms >= e.duration {
                self.dismiss();
            }
        }
    }
}
