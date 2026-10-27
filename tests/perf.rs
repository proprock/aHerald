use aherald::config::LogLevel;
use aherald::operational_log::{self, Level, Record};
use aherald::perf::{self, Collector, Counter, Every, FramePacer, Gauge, Span, WINDOW};
use std::time::{Duration, Instant};

fn texts(records: &[Record]) -> Vec<String> {
    records
        .iter()
        .map(|r| r.describe().1.into_owned())
        .collect()
}

fn feed(collector: &mut Collector, span: Span, samples: &[u64], at: Instant) -> Vec<Record> {
    let mut out = Vec::new();
    for micros in samples {
        collector.record(span, *micros, at, true, &mut out);
    }
    out
}

#[test]
fn a_flushed_span_reports_count_mean_deviation_extremes_and_p95() {
    let mut c = Collector::new();
    let t = Instant::now();
    assert!(feed(&mut c, Span::FrameRender, &[100, 200, 300, 400], t).is_empty());
    let mut out = Vec::new();
    c.flush(&mut out);
    assert_eq!(
        texts(&out),
        ["perf frame.render n=4 avg=0.25ms sd=0.11ms min=0.10ms p95=0.40ms max=0.40ms"]
    );
    assert!(out.iter().all(|r| r.level() == Level::Debug));
}

#[test]
fn p95_follows_the_tail_that_the_mean_hides() {
    let mut c = Collector::new();
    let t = Instant::now();
    let mut samples = vec![100; 94];
    samples.extend([10_000; 6]);
    feed(&mut c, Span::Placement, &samples, t);
    let mut out = Vec::new();
    c.flush(&mut out);
    assert!(texts(&out)[0].contains("p95=10.00ms"), "{:?}", texts(&out));

    let mut samples = vec![100; 96];
    samples.extend([10_000; 4]);
    feed(&mut c, Span::Placement, &samples, t);
    c.flush(&mut out);
    // The 95th of 100 samples is still a fast one; resolution is 1/8 of an octave.
    let line = &texts(&out)[1];
    assert!(line.contains("p95=0.10ms"), "{line}");
    assert!(line.contains("max=10.00ms"), "{line}");
}

#[test]
fn a_window_is_flushed_by_the_first_sample_after_it_elapses_and_then_starts_over() {
    let mut c = Collector::new();
    let t = Instant::now();
    assert!(feed(&mut c, Span::UsagePoll, &[1000], t).is_empty());
    assert!(feed(&mut c, Span::UsagePoll, &[3000], t + WINDOW / 2).is_empty());
    let out = feed(&mut c, Span::UsagePoll, &[2000], t + WINDOW);
    assert_eq!(
        texts(&out),
        ["perf usage.poll n=3 avg=2.00ms sd=0.81ms min=1.00ms p95=3.00ms max=3.00ms"]
    );
    let mut out = Vec::new();
    c.flush(&mut out);
    assert!(out.is_empty(), "flush after a flush has nothing left");
    // The next window is measured from its own first sample.
    let later = t + WINDOW * 2;
    assert!(feed(&mut c, Span::UsagePoll, &[1000], later).is_empty());
    assert!(feed(&mut c, Span::UsagePoll, &[1000], later + WINDOW / 2).is_empty());
}

#[test]
fn an_idle_thread_closes_an_elapsed_window_without_a_new_sample() {
    let mut c = Collector::new();
    let t = Instant::now();
    feed(&mut c, Span::Placement, &[500], t);
    let mut out = Vec::new();
    c.flush_expired(t + WINDOW / 2, &mut out);
    assert!(out.is_empty(), "the window is still open");
    c.flush_expired(t + WINDOW, &mut out);
    assert_eq!(out.len(), 1);
    c.flush_expired(t + WINDOW * 3, &mut out);
    assert_eq!(
        out.len(),
        1,
        "nothing is pending, so nothing more is written"
    );
}

#[test]
fn spans_are_reported_separately_and_idle_ones_not_at_all() {
    let mut c = Collector::new();
    let t = Instant::now();
    feed(&mut c, Span::ArtworkPrepare, &[5000], t);
    feed(&mut c, Span::AudioLoad, &[700, 900], t);
    let mut out = Vec::new();
    c.flush(&mut out);
    let lines = texts(&out);
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines
            .iter()
            .any(|l| l.starts_with("perf artwork.prepare n=1 "))
    );
    assert!(lines.iter().any(|l| l.starts_with("perf audio.load n=2 ")));
}

#[test]
fn one_slow_sample_is_reported_at_warn_immediately_and_repeats_are_throttled() {
    let mut c = Collector::new();
    let t = Instant::now();
    let mut out = Vec::new();
    // Fast sample: nothing. Slow sample (threshold 33 ms): a warning right away.
    c.record(Span::FrameRender, 5_000, t, false, &mut out);
    assert!(out.is_empty());
    c.record(Span::FrameRender, 100_000, t, false, &mut out);
    assert_eq!(texts(&out), ["slow frame.render ms=100.00"]);
    assert_eq!(out[0].level(), Level::Warn);
    // Within one second further slow samples are counted, not logged.
    c.record(
        Span::FrameRender,
        90_000,
        t + Duration::from_millis(400),
        false,
        &mut out,
    );
    assert_eq!(out.len(), 1);
    c.record(
        Span::FrameRender,
        50_000,
        t + Duration::from_millis(1100),
        false,
        &mut out,
    );
    assert_eq!(texts(&out)[1], "slow frame.render ms=50.00 suppressed=1");
    // Slow-only recording keeps no statistics.
    let mut flushed = Vec::new();
    c.flush(&mut flushed);
    assert!(flushed.is_empty());
}

#[test]
fn a_gui_message_that_blocks_for_over_fifty_milliseconds_is_a_stall_named_by_its_class() {
    let t = Instant::now();
    let mut out = Vec::new();
    let mut c = Collector::new();
    for (span, name) in [
        (Span::GuiTimer, "gui.timer"),
        (Span::GuiWake, "gui.wake"),
        (Span::GuiInput, "gui.input"),
        (Span::GuiOther, "gui.other"),
    ] {
        c.record(span, 49_000, t, false, &mut out);
        assert!(out.is_empty(), "{name} below the threshold");
        c.record(span, 60_000, t, false, &mut out);
        assert_eq!(texts(&out).pop().unwrap(), format!("slow {name} ms=60.00"));
        out.clear();
    }
}

#[test]
fn frames_the_animation_jumped_over_are_counted_as_skipped() {
    let mut pacer = FramePacer::default();
    assert_eq!(pacer.present(1, 0, 10), 0, "the first frame skips nothing");
    assert_eq!(pacer.present(1, 1, 10), 0);
    assert_eq!(pacer.present(1, 2, 10), 0);
    assert_eq!(
        pacer.present(1, 2, 10),
        0,
        "a fade or countdown repaint of the same frame"
    );
    assert_eq!(
        pacer.present(1, 5, 10),
        2,
        "frames 3 and 4 were never shown"
    );
    assert_eq!(pacer.present(1, 9, 10), 3);
    assert_eq!(
        pacer.present(1, 0, 10),
        0,
        "a loop wraps from the last frame to the first"
    );
    assert_eq!(pacer.present(1, 8, 10), 7);
    // A different card starts a new run; where it begins is not a skip.
    assert_eq!(pacer.present(2, 7, 10), 0);
    assert_eq!(pacer.present(3, 0, 1), 0);
    assert_eq!(pacer.present(3, 0, 1), 0, "a still card never skips");
}

#[test]
fn a_periodic_task_is_due_once_per_period_and_never_at_the_start() {
    let mut every = Every::new(Duration::from_secs(60));
    let t = Instant::now();
    let s = Duration::from_secs;
    assert!(!every.due(t), "the first call only starts the clock");
    assert!(!every.due(t + s(59)));
    assert!(every.due(t + s(60)));
    assert!(!every.due(t + s(61)));
    assert!(
        every.due(t + s(130)),
        "a late caller is not owed the missed periods"
    );
    assert!(!every.due(t + s(131)));
}

#[test]
fn counters_add_up_and_gauges_keep_the_latest_value() {
    let mut c = Collector::new();
    let t = Instant::now();
    let mut out = Vec::new();
    for _ in 0..3 {
        c.count(Counter::AssetCacheHit, t, &mut out);
    }
    c.count(Counter::AssetCacheMiss, t, &mut out);
    c.gauge(Gauge::CompositionBytes, 10, t, &mut out);
    c.gauge(Gauge::CompositionBytes, 20, t, &mut out);
    assert!(out.is_empty());
    c.flush(&mut out);
    let mut lines = texts(&out);
    lines.sort();
    assert_eq!(
        lines,
        [
            "count asset_cache.hit n=3",
            "count asset_cache.miss n=1",
            "gauge composition.bytes value=20",
        ]
    );
}

#[test]
fn spans_reach_the_log_only_at_debug_and_only_after_a_flush() {
    let root = std::env::temp_dir().join(format!("aherald-perf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let log = root.join("aherald.log");

    operational_log::init(&root, LogLevel::Warn);
    drop(perf::span(Span::Placement));
    perf::flush();
    assert!(!log.exists(), "perf lines must not appear below debug");

    operational_log::init(&root, LogLevel::Debug);
    for _ in 0..3 {
        drop(perf::span(Span::Placement));
    }
    perf::count(Counter::CompositionRebuild);
    assert!(
        !log.exists(),
        "samples are held until the window ends or flush()"
    );
    perf::flush();
    let text = std::fs::read_to_string(&log).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert!(
        lines[0].contains(" DEBUG perf placement.compute n=3 "),
        "{text}"
    );
    assert!(
        lines[1].ends_with(" DEBUG count composition.rebuild n=1"),
        "{text}"
    );
    std::fs::remove_dir_all(root).unwrap();
}
