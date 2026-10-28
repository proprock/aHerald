//! Artwork that is configured but cannot be used is replaced by the built-in picture and
//! reported once per file, with no path in the log. One test only: it points the process-wide
//! logger at its own directory.
use aherald::{
    artwork::resolve,
    config::{Config, EventVisual, LogLevel},
    events::Event,
    operational_log,
};
use std::path::Path;

fn lines(directory: &Path, needle: &str) -> usize {
    std::fs::read_to_string(directory.join("aherald.log"))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains(needle))
        .count()
}

#[test]
fn unusable_configured_artwork_falls_back_to_the_builtin_one_and_is_logged_once() {
    let directory = std::env::temp_dir().join(format!("ah-artwork-log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    operational_log::init(&directory, LogLevel::Warn);
    let builtin = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets");
    let event = Event::demo("turn_complete").unwrap();
    let mut config = Config::default();

    // A custom animation that is not there.
    let missing = directory.join("secret-animation-name.toml");
    config.events[event.kind].visual = EventVisual::Custom;
    config.events[event.kind].animation = Some(missing.clone());
    for _ in 0..3 {
        let found = resolve(&config, &event, &builtin).unwrap();
        assert!(found.is_some(), "the built-in artwork replaces it");
    }
    assert_eq!(lines(&directory, "artwork.rejected custom"), 1);

    // A different failure of the same path is a new one.
    std::fs::write(&missing, "not an animation").unwrap();
    resolve(&config, &event, &builtin).unwrap();
    resolve(&config, &event, &builtin).unwrap();
    assert_eq!(lines(&directory, "artwork.rejected custom"), 2);

    // A pet that cannot be loaded.
    config.events[event.kind].visual = EventVisual::Pet;
    config.notifications.pet = Some(directory.join("secret-pet-name"));
    for _ in 0..2 {
        assert!(resolve(&config, &event, &builtin).unwrap().is_some());
    }
    assert_eq!(lines(&directory, "artwork.rejected pet"), 1);

    let log = std::fs::read_to_string(directory.join("aherald.log")).unwrap();
    assert!(!log.contains("secret-"), "{log}");
    let _ = std::fs::remove_dir_all(&directory);
}
