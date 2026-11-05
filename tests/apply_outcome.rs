//! Apply validates the configuration before it starts a replacement resident, so that a broken file
//! keeps the running one. All three residents (Windows, macOS, Linux) decide that, say it and log it
//! through `apply_outcome`; this holds that one decision (AH-233). One test only: it points the
//! process-wide logger at its own directory.
mod support;
use aherald::{config::LogLevel, operational_log, resident_request::apply_outcome};
use std::path::Path;

fn logged(directory: &Path, needle: &str) -> usize {
    std::fs::read_to_string(directory.join("aherald.log"))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains(needle))
        .count()
}

#[test]
fn a_valid_file_applies_and_a_broken_or_missing_one_is_refused_and_logged_once_each() {
    let directory = support::TempDir::new("apply-outcome");
    operational_log::init(&directory, LogLevel::Warn);

    let valid = directory.join("valid.toml");
    std::fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("config.example.toml"),
        &valid,
    )
    .unwrap();
    assert!(apply_outcome(&valid).is_ok());
    assert_eq!(logged(&directory, "config.invalid"), 0);

    let broken = directory.join("broken.toml");
    std::fs::write(&broken, "[general\nmode =").unwrap();
    let error = apply_outcome(&broken).expect_err("a broken file is refused");
    assert!(
        error
            .message()
            .starts_with("Apply cancelled; the running configuration stays active.\n"),
        "{}",
        error.message()
    );
    // The load error follows, so the user sees what to fix; the record itself carries no text.
    assert!(
        error.message().len() > "Apply cancelled; the running configuration stays active.\n".len()
    );
    assert_eq!(logged(&directory, "config.invalid"), 1);

    let missing = directory.join("missing.toml");
    let error = apply_outcome(&missing).expect_err("a missing file is refused");
    assert!(error.message().contains("cannot open configuration"));
    assert_eq!(logged(&directory, "config.invalid"), 2);
    assert!(
        !std::fs::read_to_string(directory.join("aherald.log"))
            .unwrap()
            .contains("broken.toml"),
        "the log names no path"
    );
}
