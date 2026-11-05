#![cfg(windows)]
//! Performance baseline: a debug resident shows a burst of animated cards, and the mean
//! cost of each hot path must stay inside a budget. Historical reference measurements
//! are in docs/notes/milestone-5.md (AH-082). Rendering retains the target monitor DPI,
//! so these are environment-dependent regression guards. Use --nocapture for the table.
use aherald::{
    events::Event,
    ipc::{Request, Response, pipe},
    perf_report::{render, summarize},
};
use std::time::Duration;

/// Mean microseconds a sample of the span may cost. Measured on the unoptimized test
/// build (Windows 11, three runs within 5% of each other): ipc.handle 10, artwork.prepare
/// 8500, composition.build 7600, frame.render 870, frame.present 170, placement.compute 20.
/// Historical reference budgets; the DPI-dependent artwork adjustment is explained below.
const BUDGETS: &[(&str, u64)] = &[
    ("ipc.handle", 200),
    // Retain real monitor DPI. The observed full-suite artwork mean reached
    // 55.805 ms; 61 ms adds 9.3% headroom above that observation.
    // Composition reached 25.83 ms; 28 ms adds 8.4%. Both include debug DPI cost.
    // Ten fresh 1.5x runs: artwork 35.37-40.42 ms, sample SD 1.55 ms.
    // Fixed-1x perf_artwork stays at 30 ms; other DPI is not fully calibrated.
    // Evidence and limits: docs/notes/perf-baseline-investigation.md (2026-11-01).
    ("artwork.prepare", 61_000),
    ("composition.build", 28_000),
    ("frame.render", 3_000),
    ("frame.present", 1_000),
    ("placement.compute", 500),
];
/// Upper bounds for counters over the 24-event burst. Measured: asset_cache.miss 4 (one
/// per event kind, the rest are hits), composition.rebuild 25 (one per card: it waits
/// for its artwork; a queue-count change only redraws the heading).
const COUNTER_LIMITS: &[(&str, u64)] = &[("asset_cache.miss", 8), ("composition.rebuild", 30)];

mod support;
use support::{ResidentFixture, wait_until};

#[test]
fn hot_paths_stay_inside_their_baseline_budgets() {
    let mut text = String::from(
        "[general]\nlogging='debug'\n[sounds]\nenabled=false\n[notifications]\nfade_in_ms=0\nfade_out_ms=0\n",
    );
    for kind in [
        "turn_complete",
        "permission_required",
        "input_required",
        "error",
    ] {
        text.push_str(&format!("[events.{kind}]\nduration_sec=1\n"));
    }
    let mut fixture = ResidentFixture::new("resident-baseline", &text);
    fixture.start().unwrap();
    let scope = &fixture.scope;
    let root = fixture.root.to_path_buf();

    let kinds = [
        "turn_complete",
        "permission_required",
        "input_required",
        "error",
    ];
    for i in 0..24 {
        let mut event = Event::demo(kinds[i % kinds.len()]).unwrap();
        event.session_id = Some(format!("baseline-{i}"));
        pipe::request(scope, &Request::Event { event }, Duration::from_millis(500)).unwrap();
        std::thread::sleep(Duration::from_millis(120));
    }
    // Cards expire, the animation timer stops and the render thread closes its window. A card
    // now lives at least one second (`duration_sec`), so the 24-card burst drains in about
    // half a minute on a slow runner.
    wait_until(
        Duration::from_secs(90),
        Duration::from_millis(100),
        "resident baseline idle and render log",
        || {
            let log = std::fs::read_to_string(root.join("aherald.log")).unwrap_or_default();
            let idle = matches!(
                pipe::request(scope, &Request::Status, Duration::from_millis(500)),
                Ok(Response::Status { queued: 0, .. })
            );
            idle && log.contains(" perf frame.render n=")
        },
    )
    .unwrap();
    fixture.shutdown().unwrap();

    let log = std::fs::read_to_string(root.join("aherald.log")).unwrap();
    let summary = summarize(&log);
    println!("{}", render(&summary));
    for (name, budget) in BUDGETS {
        let row = summary
            .spans
            .iter()
            .find(|r| r.name == *name)
            .unwrap_or_else(|| panic!("no samples for {name}:\n{}", render(&summary)));
        assert!(
            row.mean_us <= *budget,
            "{name} averaged {}us, budget {budget}us\n{}",
            row.mean_us,
            render(&summary)
        );
    }
    for (name, limit) in COUNTER_LIMITS {
        let n = summary
            .counters
            .iter()
            .find(|(counter, _)| counter == name)
            .map_or(0, |(_, n)| *n);
        assert!(
            n <= *limit,
            "{name} was {n}, limit {limit}\n{}",
            render(&summary)
        );
    }
    assert!(
        !summary
            .counters
            .iter()
            .any(|(name, _)| name == "frame.skipped"),
        "animation frames were skipped\n{}",
        render(&summary)
    );
}
