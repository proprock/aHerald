use aherald::config::LogLevel;
use aherald::operational_log::{Log, MAX_LOG_BYTES, Record};
#[test]
fn activation_latency_is_logged_in_milliseconds_only_at_debug_level() {
    let root = std::env::temp_dir().join(format!("aherald-log-latency-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    Log::new(&root, LogLevel::Info)
        .record(Record::AgentActivated { millis: 1234 })
        .unwrap();
    assert!(!root.join("aherald.log").exists());
    Log::new(&root, LogLevel::Debug)
        .record(Record::AgentActivated { millis: 1234 })
        .unwrap();
    let text = std::fs::read_to_string(root.join("aherald.log")).unwrap();
    assert_eq!(
        text.trim_end().split_once(' ').unwrap().1,
        "debug agent_activated ms=1234"
    );
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn fixed_records_honor_levels_rotate_and_recover_from_unwritable_paths() {
    let root = std::env::temp_dir().join(format!("aherald-log-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let mut log = Log::new(&root, LogLevel::Off);
    log.record(Record::IpcFailure).unwrap();
    assert!(!root.join("aherald.log").exists());
    let mut log = Log::new(&root, LogLevel::Warn);
    log.record(Record::Started).unwrap();
    assert!(!root.join("aherald.log").exists());
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
                .all(|line| line.split_once(' ').unwrap().1 == "warn ipc_failure")
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
