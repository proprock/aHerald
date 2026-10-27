#![cfg(windows)]
use aherald::{config::LogLevel, operational_log, process_stats};

#[test]
fn a_snapshot_describes_this_process() {
    let s = process_stats::snapshot().expect("snapshot");
    assert!(
        s.working_set_kb > 1024,
        "a running test uses over a megabyte"
    );
    assert!(s.threads >= 1);
    // Handle counts are zero for a process that has not touched the GUI; they only
    // need to be readable.
    assert!(process_stats::snapshot().unwrap().threads >= 1);
}

#[test]
fn the_snapshot_line_has_fixed_numeric_fields_and_appears_only_at_debug() {
    let root = std::env::temp_dir().join(format!("aherald-stats-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let log = root.join("aherald.log");

    operational_log::init(&root, LogLevel::Info);
    process_stats::log_snapshot();
    assert!(!log.exists());

    operational_log::init(&root, LogLevel::Debug);
    process_stats::log_snapshot();
    let text = std::fs::read_to_string(&log).unwrap();
    let message = text.trim_end().splitn(3, ' ').nth(2).unwrap();
    let fields: Vec<_> = message
        .strip_prefix("resources ")
        .unwrap()
        .split(' ')
        .collect();
    assert!(text.contains(" DEBUG resources "), "{text}");
    let keys: Vec<_> = fields
        .iter()
        .map(|f| f.split_once('=').unwrap().0)
        .collect();
    assert_eq!(keys, ["working_set_kb", "gdi", "user", "threads"]);
    assert!(
        fields
            .iter()
            .all(|f| f.split_once('=').unwrap().1.parse::<u64>().is_ok()),
        "{message}"
    );
    std::fs::remove_dir_all(root).unwrap();
}
