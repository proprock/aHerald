//! A sound that cannot be used leaves one log line per failing file, not one per event and not
//! none. One test only: it points the process-wide logger at its own directory.
#![cfg(any(windows, target_os = "macos"))]
use aherald::{
    audio::start_controlled,
    config::{Config, LogLevel},
    events::Event,
    operational_log,
};
use std::sync::{Arc, Mutex, atomic::AtomicBool};

fn wav(seconds: u32) -> Vec<u8> {
    let samples = 8000 * seconds;
    let mut b = Vec::new();
    b.extend(b"RIFF");
    b.extend((36 + samples * 2).to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(8000u32.to_le_bytes());
    b.extend(16000u32.to_le_bytes());
    b.extend(2u16.to_le_bytes());
    b.extend(16u16.to_le_bytes());
    b.extend(b"data");
    b.extend((samples * 2).to_le_bytes());
    b.extend(vec![0u8; samples as usize * 2]);
    b
}

fn lines(directory: &std::path::Path, needle: &str) -> usize {
    std::fs::read_to_string(directory.join("aherald.log"))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains(needle))
        .count()
}

#[test]
fn an_unusable_sound_is_logged_once_per_file_and_never_with_its_path() {
    let directory = std::env::temp_dir().join(format!("ah-audio-log-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    operational_log::init(&directory, LogLevel::Warn);
    // A resident would show these as cards; a hook or transient process has no sink.
    let cards = Arc::new(Mutex::new(Vec::new()));
    let sink = cards.clone();
    aherald::notice::install(move |event| sink.lock().unwrap().push(event.message));
    let sound = directory.join("secret-folder-name.wav");
    let event = Event::demo("turn_complete").unwrap();
    let mut config = Config::default();
    config.sounds.enabled = true;
    config.sounds.volume = 0.5;
    config.events[event.kind].sound = Some(sound.clone());
    let play = |config: &Config| {
        start_controlled(&event, config, Arc::new(AtomicBool::new(true)))
            .expect("a sound is selected")
            .join()
            .unwrap();
    };

    // A six-second file is refused at every event but reported once.
    std::fs::write(&sound, wav(6)).unwrap();
    for _ in 0..3 {
        play(&config);
    }
    assert_eq!(lines(&directory, "audio.rejected too_long"), 1);

    // A different bad file at the same path is a new failure.
    std::fs::write(&sound, b"not a wav file at all").unwrap();
    play(&config);
    play(&config);
    assert_eq!(lines(&directory, "audio.rejected malformed"), 1);

    // So is a missing one.
    std::fs::remove_file(&sound).unwrap();
    play(&config);
    play(&config);
    assert_eq!(lines(&directory, "audio.rejected unreadable"), 1);

    // One card per failing file state, naming the file but not its folder.
    let cards = cards.lock().unwrap();
    assert_eq!(
        cards
            .iter()
            .flatten()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "Sound is longer than 5 s: secret-folder-name.wav",
            "Sound is not a valid WAV: secret-folder-name.wav",
            "Sound cannot be read: secret-folder-name.wav",
        ]
    );
    let log = std::fs::read_to_string(directory.join("aherald.log")).unwrap();
    assert!(!log.contains("secret-folder-name"), "{log}");
    aherald::notice::clear();
    let _ = std::fs::remove_dir_all(&directory);
}
