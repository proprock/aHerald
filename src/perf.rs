//! Hot-path timing for the debug log. A span measures one operation; samples are
//! aggregated per thread and written as a single line per span and window (count, mean,
//! deviation, extremes, p95) so a busy path costs one line, not one line per call.
//! Names are a closed set and values are numbers: nothing from an event can get here.
use crate::operational_log::{self, Level, Record};
use std::{
    cell::RefCell,
    time::{Duration, Instant},
};

/// Samples are written when the first sample after this much time arrives, or on `flush`.
pub const WINDOW: Duration = Duration::from_secs(5);
/// A single over-threshold sample is reported at most this often per span.
const SLOW_REPEAT: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Span {
    IpcHandle,
    ProcessScan,
    SessionLookup,
    ArtworkPrepare,
    CompositionBuild,
    FrameRender,
    FramePresent,
    Placement,
    UsagePoll,
    AudioLoad,
    ConfigReload,
    /// Handling one GUI-thread message, by class; over 50 ms is a UI stall.
    GuiTimer,
    GuiWake,
    GuiInput,
    GuiOther,
}
const SPANS: [Span; 15] = [
    Span::IpcHandle,
    Span::ProcessScan,
    Span::SessionLookup,
    Span::ArtworkPrepare,
    Span::CompositionBuild,
    Span::FrameRender,
    Span::FramePresent,
    Span::Placement,
    Span::UsagePoll,
    Span::AudioLoad,
    Span::ConfigReload,
    Span::GuiTimer,
    Span::GuiWake,
    Span::GuiInput,
    Span::GuiOther,
];
impl Span {
    pub fn name(self) -> &'static str {
        match self {
            Self::IpcHandle => "ipc.handle",
            Self::ProcessScan => "scan.process",
            Self::SessionLookup => "scan.session",
            Self::ArtworkPrepare => "artwork.prepare",
            Self::CompositionBuild => "composition.build",
            Self::FrameRender => "frame.render",
            Self::FramePresent => "frame.present",
            Self::Placement => "placement.compute",
            Self::UsagePoll => "usage.poll",
            Self::AudioLoad => "audio.load",
            Self::ConfigReload => "config.reload",
            Self::GuiTimer => "gui.timer",
            Self::GuiWake => "gui.wake",
            Self::GuiInput => "gui.input",
            Self::GuiOther => "gui.other",
        }
    }
    /// A sample at or above this many microseconds is worth a warning on its own.
    fn slow_micros(self) -> u64 {
        1000 * match self {
            Self::FrameRender | Self::FramePresent => 33,
            Self::IpcHandle | Self::Placement => 50,
            Self::GuiTimer | Self::GuiWake | Self::GuiInput | Self::GuiOther => 50,
            Self::ProcessScan | Self::ConfigReload => 100,
            Self::CompositionBuild | Self::AudioLoad | Self::SessionLookup => 250,
            Self::ArtworkPrepare => 500,
            Self::UsagePoll => 5000,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counter {
    AssetCacheHit,
    AssetCacheMiss,
    AssetCacheEvict,
    ArtworkBudgetFallback,
    CompositionRebuild,
    FramePresented,
    /// Animation frames jumped over because a draw came late.
    FramesSkipped,
}
const COUNTERS: [Counter; 7] = [
    Counter::AssetCacheHit,
    Counter::AssetCacheMiss,
    Counter::AssetCacheEvict,
    Counter::ArtworkBudgetFallback,
    Counter::CompositionRebuild,
    Counter::FramePresented,
    Counter::FramesSkipped,
];
impl Counter {
    fn name(self) -> &'static str {
        match self {
            Self::AssetCacheHit => "asset_cache.hit",
            Self::AssetCacheMiss => "asset_cache.miss",
            Self::AssetCacheEvict => "asset_cache.evict",
            Self::ArtworkBudgetFallback => "artwork.budget_fallback",
            Self::CompositionRebuild => "composition.rebuild",
            Self::FramePresented => "frame.presented",
            Self::FramesSkipped => "frame.skipped",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gauge {
    AssetCacheBytes,
    CompositionBytes,
}
const GAUGES: [Gauge; 2] = [Gauge::AssetCacheBytes, Gauge::CompositionBytes];
impl Gauge {
    fn name(self) -> &'static str {
        match self {
            Self::AssetCacheBytes => "asset_cache.bytes",
            Self::CompositionBytes => "composition.bytes",
        }
    }
}

/// Histogram resolution: 8 sub-buckets per power of two (about 12% worst-case error),
/// covering values up to 2^36 microseconds.
const BUCKETS: usize = 280;
fn bucket(micros: u64) -> usize {
    let v = micros.min(1 << 36);
    if v < 8 {
        return v as usize;
    }
    let exponent = 63 - v.leading_zeros();
    (((exponent - 2) as usize) << 3) | ((v >> (exponent - 3)) & 7) as usize
}
/// Largest value that falls in `index`.
fn bucket_top(index: usize) -> u64 {
    if index < 8 {
        return index as u64;
    }
    let shift = (index >> 3) as u32 - 1;
    (((8 + (index & 7)) as u64 + 1) << shift) - 1
}

#[derive(Default)]
struct Stats {
    n: u64,
    mean: f64,
    m2: f64,
    min: u64,
    max: u64,
    buckets: Vec<u32>,
}
impl Stats {
    fn add(&mut self, micros: u64) {
        if self.n == 0 {
            self.min = micros;
            self.max = micros;
            self.buckets = vec![0; BUCKETS];
        }
        self.n += 1;
        // Welford's running mean and sum of squared deviations.
        let delta = micros as f64 - self.mean;
        self.mean += delta / self.n as f64;
        self.m2 += delta * (micros as f64 - self.mean);
        self.min = self.min.min(micros);
        self.max = self.max.max(micros);
        self.buckets[bucket(micros)] += 1;
    }
    fn p95(&self) -> u64 {
        let rank = (self.n * 95).div_ceil(100);
        let mut seen = 0;
        for (index, count) in self.buckets.iter().enumerate() {
            seen += u64::from(*count);
            if seen >= rank {
                return bucket_top(index).min(self.max);
            }
        }
        self.max
    }
}

#[derive(Default)]
struct SpanState {
    stats: Stats,
    last_slow: Option<Instant>,
    suppressed: u64,
}

/// Aggregation state for one thread. Time is passed in, so the windowing and the slow
/// throttle are testable without sleeping.
pub struct Collector {
    spans: Vec<SpanState>,
    counters: [u64; COUNTERS.len()],
    gauges: [Option<u64>; GAUGES.len()],
    window_start: Option<Instant>,
}
impl Default for Collector {
    fn default() -> Self {
        Self::new()
    }
}
impl Collector {
    pub fn new() -> Self {
        Self {
            spans: SPANS.iter().map(|_| SpanState::default()).collect(),
            counters: [0; COUNTERS.len()],
            gauges: [None; GAUGES.len()],
            window_start: None,
        }
    }
    /// Records one sample. With `stats` false only the slow-sample warning applies.
    pub fn record(
        &mut self,
        span: Span,
        micros: u64,
        now: Instant,
        stats: bool,
        out: &mut Vec<Record>,
    ) {
        let state = &mut self.spans[span as usize];
        if micros >= span.slow_micros() {
            if state
                .last_slow
                .is_none_or(|at| now.duration_since(at) >= SLOW_REPEAT)
            {
                out.push(Record::Slow {
                    span: span.name(),
                    micros,
                    suppressed: std::mem::take(&mut state.suppressed),
                });
                state.last_slow = Some(now);
            } else {
                state.suppressed += 1;
            }
        }
        if stats {
            state.stats.add(micros);
            self.flush_if_due(now, out);
        }
    }
    pub fn count(&mut self, counter: Counter, now: Instant, out: &mut Vec<Record>) {
        self.add(counter, 1, now, out);
    }
    pub fn add(&mut self, counter: Counter, n: u64, now: Instant, out: &mut Vec<Record>) {
        self.counters[counter as usize] += n;
        self.flush_if_due(now, out);
    }
    pub fn gauge(&mut self, gauge: Gauge, value: u64, now: Instant, out: &mut Vec<Record>) {
        self.gauges[gauge as usize] = Some(value);
        self.flush_if_due(now, out);
    }
    fn flush_if_due(&mut self, now: Instant, out: &mut Vec<Record>) {
        let start = *self.window_start.get_or_insert(now);
        if now.duration_since(start) >= WINDOW {
            self.flush(out);
        }
    }
    /// For a thread that has gone quiet: closes a window that has already elapsed.
    pub fn flush_expired(&mut self, now: Instant, out: &mut Vec<Record>) {
        if self
            .window_start
            .is_some_and(|start| now.duration_since(start) >= WINDOW)
        {
            self.flush(out);
        }
    }
    /// Emits everything collected so far and starts a new window.
    pub fn flush(&mut self, out: &mut Vec<Record>) {
        self.window_start = None;
        for span in SPANS {
            let stats = std::mem::take(&mut self.spans[span as usize].stats);
            if stats.n > 0 {
                out.push(Record::Perf {
                    span: span.name(),
                    n: stats.n,
                    avg: stats.mean.round() as u64,
                    sd: (stats.m2 / stats.n as f64).sqrt().round() as u64,
                    min: stats.min,
                    p95: stats.p95(),
                    max: stats.max,
                });
            }
        }
        for counter in COUNTERS {
            let n = std::mem::take(&mut self.counters[counter as usize]);
            if n > 0 {
                out.push(Record::Count {
                    name: counter.name(),
                    n,
                });
            }
        }
        for gauge in GAUGES {
            if let Some(value) = self.gauges[gauge as usize].take() {
                out.push(Record::Gauge {
                    name: gauge.name(),
                    value,
                });
            }
        }
    }
}

fn emit(records: Vec<Record>) {
    for record in records {
        operational_log::log(record);
    }
}
/// Flushes what the thread collected when the thread ends.
struct Local(Collector);
impl Drop for Local {
    fn drop(&mut self) {
        let mut out = Vec::new();
        self.0.flush(&mut out);
        emit(out);
    }
}
thread_local! {
    static LOCAL: RefCell<Local> = RefCell::new(Local(Collector::new()));
}
fn with_local(action: impl FnOnce(&mut Collector, &mut Vec<Record>)) {
    let mut out = Vec::new();
    // Unavailable only while the thread is being torn down; the sample is then dropped.
    let _ = LOCAL.try_with(|local| action(&mut local.borrow_mut().0, &mut out));
    emit(out);
}

/// Times an operation until dropped. Cheap when logging is below `debug`: only an
/// over-threshold sample is recorded then.
pub struct Timer {
    span: Span,
    start: Instant,
}
pub fn span(span: Span) -> Timer {
    Timer {
        span,
        start: Instant::now(),
    }
}
impl Drop for Timer {
    fn drop(&mut self) {
        let micros = self.start.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
        let stats = operational_log::enabled(Level::Debug);
        if !stats && micros < self.span.slow_micros() {
            return;
        }
        let now = Instant::now();
        with_local(|c, out| c.record(self.span, micros, now, stats, out));
    }
}
/// Runs `work` inside a span.
pub fn time<T>(name: Span, work: impl FnOnce() -> T) -> T {
    let _timer = span(name);
    work()
}
pub fn count(counter: Counter) {
    add(counter, 1);
}
pub fn add(counter: Counter, n: u64) {
    if n > 0 && operational_log::enabled(Level::Debug) {
        let now = Instant::now();
        with_local(|c, out| c.add(counter, n, now, out));
    }
}

/// Spots animation frames that were never shown because the GUI thread was late: the
/// animation follows the clock, so a late tick jumps ahead by more than one frame.
#[derive(Default)]
pub struct FramePacer {
    last: Option<(u64, usize)>,
}
impl FramePacer {
    /// Notes a drawn frame of card `run` (`count` frames in all) and returns how many
    /// frames were jumped over since the previous draw of that card.
    pub fn present(&mut self, run: u64, frame: usize, count: usize) -> u64 {
        let skipped = match self.last {
            Some((last_run, last)) if last_run == run && count > 0 => {
                ((frame + count - last % count) % count).saturating_sub(1) as u64
            }
            _ => 0,
        };
        self.last = Some((run, frame));
        skipped
    }
}
pub fn gauge(gauge: Gauge, value: u64) {
    if operational_log::enabled(Level::Debug) {
        let now = Instant::now();
        with_local(|c, out| c.gauge(gauge, value, now, out));
    }
}
/// Cheap check for loops that wake periodically: writes the pending samples once their
/// window has elapsed, even if no new sample arrives.
pub fn tick() {
    let now = Instant::now();
    with_local(|c, out| c.flush_expired(now, out));
}
/// Writes this thread's pending samples. Call before a thread goes idle or stops: a
/// window is otherwise only closed by the next sample.
pub fn flush() {
    with_local(|c, out| c.flush(out));
}
