use aherald::perf_report::{report, summarize};

const LOG: &str = "\
2026-10-27T14:00:00.000+00:00 DEBUG perf frame.render n=100 avg=0.50ms sd=0.10ms min=0.30ms p95=0.80ms max=2.00ms
2026-10-27T14:00:05.000+00:00 DEBUG perf frame.render n=300 avg=1.00ms sd=0.20ms min=0.30ms p95=1.50ms max=9.00ms
2026-10-27T14:00:05.001+00:00 DEBUG perf scan.process n=2 avg=40.00ms sd=1.00ms min=39.00ms p95=41.00ms max=41.00ms
2026-10-27T14:00:06.000+00:00 WARN  slow scan.process ms=41.00
2026-10-27T14:00:06.100+00:00 WARN  slow scan.process ms=45.00 suppressed=3
this line is not a log line
2026-10-27T14:00:06.200+00:00 DEBUG perf broken n=x avg=?
2026-10-27T14:00:07.000+00:00 DEBUG count asset_cache.hit n=18
2026-10-27T14:00:08.000+00:00 DEBUG count asset_cache.hit n=2
2026-10-27T14:00:08.000+00:00 DEBUG count asset_cache.miss n=1
2026-10-27T14:00:09.000+00:00 INFO  resident.started
";

#[test]
fn spans_are_ranked_by_total_time_and_windows_of_one_span_are_combined() {
    let summary = summarize(LOG);
    let rows: Vec<_> = summary
        .spans
        .iter()
        .map(|r| {
            (
                r.name.as_str(),
                r.samples,
                r.total_us,
                r.mean_us,
                r.p95_us,
                r.max_us,
                r.slow,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("frame.render", 400, 350_000, 875, 1_500, 9_000, 0),
            // Two slow lines plus the three samples the throttle folded into the second.
            ("scan.process", 2, 80_000, 40_000, 41_000, 41_000, 5),
        ]
    );
}

#[test]
fn counters_are_summed_and_unreadable_lines_are_ignored() {
    let summary = summarize(LOG);
    assert_eq!(
        summary.counters,
        [
            ("asset_cache.hit".to_owned(), 20),
            ("asset_cache.miss".to_owned(), 1)
        ]
    );
    assert!(summarize("").spans.is_empty());
    assert!(summarize("garbage\n\n   \n").counters.is_empty());
}

#[test]
fn a_slow_span_with_no_statistics_is_still_listed() {
    let summary = summarize("2026-10-27T14:00:06.000+00:00 WARN  slow gui.timer ms=70.00\n");
    assert_eq!(summary.spans.len(), 1);
    assert_eq!(summary.spans[0].name, "gui.timer");
    assert_eq!((summary.spans[0].samples, summary.spans[0].slow), (0, 1));
}

#[test]
fn the_report_reads_the_backup_before_the_current_log() {
    let root = std::env::temp_dir().join(format!("aherald-report-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    assert!(report(&root).is_err(), "no log yet");
    std::fs::write(
        root.join("aherald.previous.log"),
        "2026-10-27T14:00:00.000+00:00 DEBUG perf placement.compute n=1 avg=1.00ms sd=0.00ms min=1.00ms p95=1.00ms max=1.00ms\n",
    )
    .unwrap();
    std::fs::write(
        root.join("aherald.log"),
        "2026-10-27T14:00:09.000+00:00 DEBUG perf placement.compute n=3 avg=2.00ms sd=0.00ms min=2.00ms p95=2.00ms max=2.00ms\n",
    )
    .unwrap();
    let text = report(&root).unwrap();
    let mut lines = text.lines();
    let header: Vec<_> = lines.next().unwrap().split_whitespace().collect();
    assert_eq!(
        header,
        [
            "span", "samples", "total_ms", "mean_ms", "p95_ms", "max_ms", "slow"
        ]
    );
    let row: Vec<_> = lines.next().unwrap().split_whitespace().collect();
    assert_eq!(
        row,
        [
            "placement.compute",
            "4",
            "7.00",
            "1.75",
            "2.00",
            "2.00",
            "0"
        ]
    );
    std::fs::remove_dir_all(root).unwrap();
}
