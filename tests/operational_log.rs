use aherald::config::LogLevel;
use aherald::operational_log::{
    self, Level, Log, MAX_DEBUG_LOG_BYTES, MAX_LOG_BYTES, Record, format_timestamp,
};
use std::path::{Path, PathBuf};

fn temp(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("aherald-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

/// Splits `<timestamp> <LEVEL> <message>`; the level is padded to five characters.
fn parse(line: &str) -> (&str, &str, &str) {
    let (stamp, rest) = line.split_once(' ').unwrap();
    (stamp, &rest[..5], &rest[6..])
}

fn lines(root: &Path, name: &str) -> Vec<String> {
    std::fs::read_to_string(root.join(name))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn timestamps_are_iso_8601_with_milliseconds_and_the_utc_offset() {
    assert_eq!(format_timestamp(0, 0), "1970-01-01T00:00:00.000+00:00");
    // 2001-09-09T01:46:40Z, shown east and west of Greenwich, across a date boundary.
    assert_eq!(
        format_timestamp(1_000_000_000_123, 180),
        "2001-09-09T04:46:40.123+03:00"
    );
    assert_eq!(
        format_timestamp(1_000_000_000_123, -330),
        "2001-09-08T20:16:40.123-05:30"
    );
    // Leap day, and a time before the epoch.
    assert_eq!(
        format_timestamp(1_709_164_800_000, 0),
        "2024-02-29T00:00:00.000+00:00"
    );
    assert_eq!(format_timestamp(-1, 0), "1969-12-31T23:59:59.999+00:00");
}

#[test]
fn every_line_has_a_local_timestamp_a_padded_level_and_a_dotted_message() {
    let root = temp("log-format");
    let mut log = Log::new(&root, LogLevel::Debug);
    log.record(Record::Started).unwrap();
    log.record(Record::IpcFailure).unwrap();
    log.record(Record::ResidentStartupFailure).unwrap();
    log.record(Record::AgentActivated { millis: 1234 }).unwrap();
    let written = lines(&root, "aherald.log");
    let expected = [
        ("INFO ", "resident.started"),
        ("WARN ", "ipc.failure"),
        ("ERROR", "resident.startup_failed"),
        ("DEBUG", "agent.activated ms=1234"),
    ];
    assert_eq!(written.len(), expected.len());
    for (line, (level, message)) in written.iter().zip(expected) {
        let (stamp, got_level, got_message) = parse(line);
        assert_eq!((got_level, got_message), (level, message), "{line}");
        // 2026-10-27T14:03:22.481+03:00
        let b = stamp.as_bytes();
        assert_eq!(b.len(), 29, "{stamp}");
        for (i, c) in b.iter().enumerate() {
            let ok = match i {
                4 | 7 => *c == b'-',
                10 => *c == b'T',
                13 | 16 | 26 => *c == b':',
                19 => *c == b'.',
                23 => *c == b'+' || *c == b'-',
                _ => c.is_ascii_digit(),
            };
            assert!(ok, "{stamp} at {i}");
        }
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn records_below_the_configured_level_are_not_written() {
    let root = temp("log-levels");
    Log::new(&root, LogLevel::Info)
        .record(Record::AgentActivated { millis: 1 })
        .unwrap();
    assert!(!root.join("aherald.log").exists());
    Log::new(&root, LogLevel::Off)
        .record(Record::ResidentStartupFailure)
        .unwrap();
    assert!(!root.join("aherald.log").exists());
    Log::new(&root, LogLevel::Warn)
        .record(Record::Started)
        .unwrap();
    assert!(!root.join("aherald.log").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn rotation_keeps_the_log_and_its_backup_within_the_cap_and_recovers_from_unwritable_paths() {
    let root = temp("log-rotate");
    let mut log = Log::new(&root, LogLevel::Warn);
    for _ in 0..6000 {
        log.record(Record::IpcFailure).unwrap();
    }
    for name in ["aherald.log", "aherald.previous.log"] {
        let bytes = std::fs::read(root.join(name)).unwrap();
        assert!(bytes.len() <= MAX_LOG_BYTES as usize);
        assert!(
            String::from_utf8(bytes)
                .unwrap()
                .lines()
                .all(|line| parse(line).2 == "ipc.failure")
        );
    }
    drop(log);
    let blocked = root.join("blocked");
    std::fs::write(&blocked, b"file").unwrap();
    let mut broken = Log::new(&blocked, LogLevel::Warn);
    assert!(broken.record(Record::IpcFailure).is_err());
    std::fs::remove_file(&blocked).unwrap();
    assert!(broken.record(Record::IpcFailure).is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn debug_logging_keeps_a_larger_window_than_the_default() {
    let root = temp("log-debug-cap");
    let mut log = Log::new(&root, LogLevel::Debug);
    for _ in 0..12_000 {
        log.record(Record::AgentActivated { millis: 7 }).unwrap();
    }
    let size = std::fs::metadata(root.join("aherald.log")).unwrap().len();
    assert!(size > MAX_LOG_BYTES, "debug log was cut at the default cap");
    for name in ["aherald.log", "aherald.previous.log"] {
        let size = std::fs::metadata(root.join(name)).unwrap().len();
        assert!(size <= MAX_DEBUG_LOG_BYTES);
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn the_shared_logger_gates_by_level_and_follows_reconfiguration() {
    let first = temp("log-shared-a");
    let second = temp("log-shared-b");
    operational_log::log(Record::IpcFailure); // before init: dropped, never a panic
    operational_log::init(&first, LogLevel::Warn);
    assert!(operational_log::enabled(Level::Error));
    assert!(operational_log::enabled(Level::Warn));
    assert!(!operational_log::enabled(Level::Debug));
    operational_log::log(Record::IpcFailure);
    operational_log::log(Record::AgentActivated { millis: 5 });
    assert_eq!(lines(&first, "aherald.log").len(), 1);

    operational_log::init(&second, LogLevel::Debug);
    assert!(operational_log::enabled(Level::Debug));
    operational_log::log(Record::AgentActivated { millis: 5 });
    assert_eq!(lines(&second, "aherald.log").len(), 1);
    assert_eq!(lines(&first, "aherald.log").len(), 1);

    // Re-applying the same settings keeps appending to the same file.
    operational_log::init(&second, LogLevel::Debug);
    operational_log::log(Record::AgentActivated { millis: 6 });
    assert_eq!(lines(&second, "aherald.log").len(), 2);

    operational_log::init(&second, LogLevel::Off);
    assert!(!operational_log::enabled(Level::Error));
    for root in [first, second] {
        std::fs::remove_dir_all(root).unwrap();
    }
}
