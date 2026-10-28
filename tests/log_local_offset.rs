//! Log lines carry the local time and its UTC offset on Unix as on Windows. One test only: it
//! changes the process's `TZ`.
#![cfg(unix)]
use aherald::{
    config::LogLevel,
    operational_log::{Log, Record},
};

fn stamp(zone: &str, directory: &std::path::Path) -> String {
    // SAFETY: this is the only test in the process, so nothing else reads the environment.
    unsafe { std::env::set_var("TZ", zone) };
    let mut log = Log::new(directory, LogLevel::Info);
    log.record(Record::Started).unwrap();
    let text = std::fs::read_to_string(directory.join("aherald.log")).unwrap();
    std::fs::remove_file(directory.join("aherald.log")).unwrap();
    text.split_whitespace().next().unwrap().to_owned()
}

#[test]
fn a_log_line_ends_its_timestamp_with_the_local_offset() {
    let directory = std::env::temp_dir().join(format!("ah-log-offset-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    // Zones without daylight saving, so the expected offset does not depend on the date.
    for (zone, offset) in [
        ("UTC", "+00:00"),
        ("Asia/Kolkata", "+05:30"),
        ("Europe/Minsk", "+03:00"),
    ] {
        let stamp = stamp(zone, &directory);
        assert!(stamp.ends_with(offset), "{zone}: {stamp}");
    }
    let _ = std::fs::remove_dir_all(&directory);
}
