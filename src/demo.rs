//! Controlled-time scenarios for an isolated configuration preview.
use crate::{config::Config, events::Event, queue::Queue};

pub struct Demo {
    scenes: Vec<Vec<(Event, u64)>>,
    holds: Vec<u64>,
    pause_ms: u64,
    elapsed: u64,
    last_tick: u64,
    deadline: u64,
    scene: usize,
    showing: bool,
}
impl Demo {
    pub fn new(config: &Config, pause_sec: u64) -> Result<Self, String> {
        if !(1..=86_400).contains(&pause_sec) {
            return Err("demo pause must be 1..86400 seconds".into());
        }
        let mut scenes = Vec::new();
        let mut holds = Vec::new();
        for kind in KINDS {
            let setting = &config.events[kind];
            if setting.enabled {
                let duration = if setting.duration_ms == 0 {
                    0
                } else {
                    setting
                        .duration_ms
                        .saturating_add(config.notifications.fade_out_ms)
                };
                scenes.push(vec![(Event::demo(kind.name())?, duration)]);
                holds.push(
                    setting
                        .duration_ms
                        .max(3000)
                        .saturating_add(config.notifications.fade_out_ms),
                );
            }
        }
        if scenes.is_empty() {
            return Err("demo requires at least one enabled event".into());
        }
        if scenes.len() > 1 {
            // Each grouped card gets its own preview budget, including waits
            // that would be persistent outside the demo.
            let burst = scenes
                .iter()
                .zip(&holds)
                .map(|(scene, hold)| (scene[0].0.clone(), *hold))
                .collect();
            scenes.push(burst);
            holds.push(holds.iter().copied().max().unwrap_or(3000));
        }
        Ok(Self {
            scenes,
            holds,
            pause_ms: pause_sec * 1000,
            elapsed: 0,
            last_tick: 0,
            deadline: 0,
            scene: 0,
            showing: false,
        })
    }

    /// Advance at most one phase per tick; missed time never floods the queue.
    pub fn tick(&mut self, now: u64, paused: bool, queue: &mut Queue) {
        let delta = now.saturating_sub(self.last_tick);
        self.last_tick = now;
        if paused {
            return;
        }
        self.elapsed = self.elapsed.saturating_add(delta);
        let grouped = self.showing && self.scenes[self.scene].len() > 1;
        if grouped && !queue.items().is_empty() {
            return;
        }
        if !grouped && self.elapsed < self.deadline {
            return;
        }
        if self.showing {
            if !grouped {
                queue.dismiss();
            }
            self.scene = (self.scene + 1) % self.scenes.len();
            self.deadline = self.elapsed.saturating_add(self.pause_ms);
        } else {
            // Every scenario has fresh identity, even with long dedup windows.
            for (event, duration) in &self.scenes[self.scene] {
                let mut event = event.clone();
                event.session_id = Some(format!("aherald-demo-{now}"));
                queue.push(event, now, *duration);
            }
            self.deadline = self.elapsed.saturating_add(self.holds[self.scene]);
        }
        self.showing = !self.showing;
    }
}
/// Preview every notification format, including warnings without a live session.
pub const KINDS: [crate::events::Kind; 7] = crate::events::Kind::ALL;

/// Fixed configuration samples. Navigation never removes a card or touches IPC history.
pub struct Carousel {
    cards: Vec<crate::queue::Entry>,
    disabled: Vec<bool>,
    index: usize,
    elapsed: u64,
    last_tick: u64,
}
impl Carousel {
    pub fn new(config: &Config) -> Self {
        let cards = KINDS
            .into_iter()
            .enumerate()
            .map(|(index, kind)| {
                let mut event = Event::notice("");
                event.source = crate::events::Source::Demo;
                event.kind = kind;
                event.message = None;
                event.folder = Some("[Test]".into());
                crate::queue::Entry {
                    id: u64::MAX - index as u64,
                    event,
                    duration: if config.events[kind].duration_ms == 0 {
                        5000
                    } else {
                        config.events[kind].duration_ms
                    },
                    visible_ms: 0,
                    announced: false,
                    arrived_ms: 0,
                }
            })
            .collect();
        Self {
            cards,
            disabled: KINDS.map(|kind| !config.events[kind].enabled).into(),
            index: 0,
            elapsed: 0,
            last_tick: 0,
        }
    }
    pub fn len(&self) -> usize {
        self.cards.len()
    }
    pub fn is_empty(&self) -> bool {
        self.cards.is_empty()
    }
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn selected(&self) -> &crate::queue::Entry {
        &self.cards[self.index]
    }
    pub fn disabled(&self) -> bool {
        self.disabled[self.index]
    }
    pub fn navigate(&mut self, delta: i32, now: u64) {
        self.index = (self.index as i32 + delta).rem_euclid(self.cards.len() as i32) as usize;
        self.elapsed = 0;
        self.last_tick = now;
        self.cards[self.index].visible_ms = 0;
    }
    pub fn tick(&mut self, now: u64, paused: bool) {
        let delta = now.saturating_sub(self.last_tick);
        self.last_tick = now;
        if paused {
            return;
        }
        self.elapsed = self.elapsed.saturating_add(delta);
        if self.elapsed >= self.selected().duration {
            self.navigate(1, now);
        }
        self.cards[self.index].visible_ms = self.elapsed;
    }
}
