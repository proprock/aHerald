//! Worker-owned LRU cache. Active cards share immutable decoded frames.
use crate::{
    animation::PreparedAnimation,
    perf::{self, Counter, Gauge},
};
use std::{collections::VecDeque, sync::Arc};
type Key = (&'static str, &'static str);
struct Entry {
    key: Key,
    value: Option<Arc<PreparedAnimation>>,
    bytes: usize,
}
pub struct AssetCache {
    entries: VecDeque<Entry>,
    capacity: usize,
    budget: usize,
    bytes: usize,
}
impl AssetCache {
    pub fn new(capacity: usize, budget: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            capacity: capacity.min(16),
            budget: budget.min(crate::animation::MAX_BYTES as usize),
            bytes: 0,
        }
    }
    pub fn retained_bytes(&self) -> usize {
        self.bytes
    }
    pub fn get_or_load(
        &mut self,
        key: Key,
        load: impl FnOnce() -> Option<PreparedAnimation>,
    ) -> Option<Arc<PreparedAnimation>> {
        if let Some(index) = self.entries.iter().position(|e| e.key == key)
            && let Some(entry) = self.entries.remove(index)
        {
            let value = entry.value.clone();
            self.entries.push_back(entry);
            perf::count(Counter::AssetCacheHit);
            return value;
        }
        perf::count(Counter::AssetCacheMiss);
        let value = load().map(Arc::new);
        let bytes = value
            .as_ref()
            .map(|a| {
                a.frames
                    .iter()
                    .fold(0usize, |total, f| total.saturating_add(f.bgra.len()))
            })
            .unwrap_or(0);
        if self.capacity > 0 && bytes <= self.budget {
            while self.entries.len() >= self.capacity || self.bytes > self.budget - bytes {
                if let Some(entry) = self.entries.pop_front() {
                    self.bytes -= entry.bytes;
                    perf::count(Counter::AssetCacheEvict);
                } else {
                    break;
                }
            }
            self.bytes += bytes;
            perf::gauge(Gauge::AssetCacheBytes, self.bytes as u64);
            self.entries.push_back(Entry {
                key,
                value: value.clone(),
                bytes,
            });
        }
        value
    }
}
