//! Artwork that is configured but cannot be used is replaced by the built-in picture and
//! reported once per file, with no path in the log. One test only: it points the process-wide
//! logger at its own directory.
mod support;
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
    let directory = support::TempDir::new("artwork-log");
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

    // The agent override is the selected file, even without a shared pet.
    config.notifications.pet = None;
    config.agents.insert(
        event.source.agent_key().to_owned(),
        aherald::config::AgentAssets {
            pet: Some(directory.join("secret-agent-pet")),
            ..Default::default()
        },
    );
    let notices = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let received = notices.clone();
    aherald::notice::install(move |event| received.lock().unwrap().push(event));
    for _ in 0..3 {
        assert!(resolve(&config, &event, &builtin).unwrap().is_some());
    }
    aherald::notice::clear();
    assert_eq!(lines(&directory, "artwork.rejected pet"), 2);
    let notices = notices.lock().unwrap();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].source, aherald::events::Source::App);

    // An aHerald notice belongs to no agent: neither the custom animation nor the pet applies,
    // so the broken settings are not even tried and nothing more is reported.
    let before = lines(&directory, "artwork.rejected");
    let notice = Event::notice("Sound too long: a.wav");
    config.events[notice.kind].visual = EventVisual::Custom;
    config.events[notice.kind].animation = Some(directory.join("another-secret-name.toml"));
    assert!(resolve(&config, &notice, &builtin).unwrap().is_some());
    assert_eq!(lines(&directory, "artwork.rejected"), before);

    // Missing built-in resources degrade to text, with one diagnostic and one notice.
    let missing_builtin = directory.join("secret-builtin-resources");
    config.events[event.kind].visual = EventVisual::Builtin;
    let missing_notices = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let received = missing_notices.clone();
    aherald::notice::install(move |event| received.lock().unwrap().push(event));
    for _ in 0..3 {
        assert!(
            resolve(&config, &event, &missing_builtin)
                .unwrap()
                .is_none()
        );
    }
    assert_eq!(lines(&directory, "artwork.rejected builtin"), 1);
    assert_eq!(missing_notices.lock().unwrap().len(), 1);
    // An error notice with no error artwork must not recursively generate notices.
    assert!(
        resolve(&config, &notice, &missing_builtin)
            .unwrap()
            .is_none()
    );
    assert_eq!(missing_notices.lock().unwrap().len(), 1);
    aherald::notice::clear();

    let log = std::fs::read_to_string(directory.join("aherald.log")).unwrap();
    assert!(!log.contains("secret-"), "{log}");
}
