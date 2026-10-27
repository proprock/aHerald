#![cfg(windows)]
//! Performance baseline: a debug resident shows a burst of animated cards, and the mean
//! cost of each hot path must stay inside a budget. Budgets are several times what the
//! reference machine measures (see docs/milestone-5.md, AH-082), so they trip on a real
//! regression, not on scheduling noise. Run with `--nocapture` to print the table.
use aherald::{
    events::Event,
    ipc::{Request, Response, pipe},
    local::Scope,
    perf_report::{render, summarize},
    resident,
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

/// Mean microseconds a sample of the span may cost. Measured on the unoptimized test
/// build (Windows 11, three runs within 5% of each other): ipc.handle 10, artwork.prepare
/// 8500, composition.build 7600, frame.render 870, frame.present 170, placement.compute 20.
/// Budgets are about three times that.
const BUDGETS: &[(&str, u64)] = &[
    ("ipc.handle", 200),
    ("artwork.prepare", 30_000),
    ("composition.build", 25_000),
    ("frame.render", 3_000),
    ("frame.present", 1_000),
    ("placement.compute", 500),
];
/// Upper bounds for counters over the 24-event burst. Measured: asset_cache.miss 4 (one
/// per event kind, the rest are hits), composition.rebuild about 70 (nearly three per
/// card: selection, artwork arrival and every change of the queue count; a known cost).
const COUNTER_LIMITS: &[(&str, u64)] = &[("asset_cache.miss", 8), ("composition.rebuild", 90)];

struct Cleanup(Scope, PathBuf);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = pipe::request(&self.0, &Request::Quit, Duration::from_millis(200));
        let _ = std::fs::remove_dir_all(&self.1);
    }
}

#[test]
fn hot_paths_stay_inside_their_baseline_budgets() {
    let scope = Scope::current(&format!("resident-baseline-{}", std::process::id())).unwrap();
    let root = std::env::temp_dir().join(&scope.namespace);
    std::fs::create_dir_all(&root).unwrap();
    let config = root.join("config.toml");
    let mut text = String::from(
        "[general]\nlogging='debug'\n[sounds]\nenabled=false\n[notifications]\nfade_in_ms=0\nfade_out_ms=0\n",
    );
    for kind in [
        "turn_complete",
        "permission_required",
        "input_required",
        "error",
    ] {
        text.push_str(&format!("[events.{kind}]\nduration_ms=400\n"));
    }
    std::fs::write(&config, text).unwrap();
    let _cleanup = Cleanup(scope.clone(), root.clone());
    let exe = PathBuf::from(env!("CARGO_BIN_EXE_aherald"));
    assert!(matches!(
        resident::ensure(&exe, &config, &scope).unwrap(),
        Response::Status { .. }
    ));

    let kinds = [
        "turn_complete",
        "permission_required",
        "input_required",
        "error",
    ];
    for i in 0..24 {
        let mut event = Event::demo(kinds[i % kinds.len()]).unwrap();
        event.session_id = Some(format!("baseline-{i}"));
        pipe::request(
            &scope,
            &Request::Event { event },
            Duration::from_millis(500),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(120));
    }
    // Cards expire, the animation timer stops and the render thread closes its window.
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let log = std::fs::read_to_string(root.join("aherald.log")).unwrap_or_default();
        let idle = matches!(
            pipe::request(&scope, &Request::Status, Duration::from_millis(500)),
            Ok(Response::Status { queued: 0, .. })
        );
        if idle && log.contains(" perf frame.render n=") {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "resident never went idle:\n{log}"
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    pipe::request(&scope, &Request::Quit, Duration::from_millis(500)).unwrap();
    std::thread::sleep(Duration::from_millis(500));

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
