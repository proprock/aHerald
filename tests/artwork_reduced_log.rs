//! Artwork that is too large for the memory budget at the monitor's DPI scale is shown at the
//! configured scale instead; that is reported once per scale, with nothing of the file in the
//! log. One test only: it points the process-wide logger at its own directory.
use aherald::{
    artwork::prepare_at,
    config::{Config, LogLevel},
    events::Event,
    operational_log,
};
use image::RgbaImage;
use std::path::PathBuf;

fn lines(directory: &std::path::Path, needle: &str) -> usize {
    std::fs::read_to_string(directory.join("aherald.log"))
        .unwrap_or_default()
        .lines()
        .filter(|line| line.contains(needle))
        .count()
}

#[test]
fn artwork_prepared_at_a_smaller_scale_is_logged_once_per_scale() {
    let directory = std::env::temp_dir().join(format!("ah-art-reduced-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(&directory).unwrap();
    operational_log::init(&directory, LogLevel::Warn);
    RgbaImage::new(1024, 1024)
        .save(directory.join("secret-large.png"))
        .unwrap();
    std::fs::write(
        directory.join("animation.toml"),
        "type='spritesheet'\nimage='secret-large.png'\nframe_width=1024\nframe_height=1024\nframes=1",
    )
    .unwrap();
    let config =
        Config::parse("[events.error]\nvisual='custom'\nanimation='.'", &directory).unwrap();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets");
    let event = Event::demo("error").unwrap();

    for _ in 0..3 {
        prepare_at(&config, &event, &root, 2.0).unwrap().unwrap();
    }
    assert_eq!(lines(&directory, "artwork.reduced"), 0, "fits the budget");
    for _ in 0..3 {
        prepare_at(&config, &event, &root, 4.0).unwrap().unwrap();
    }
    assert_eq!(lines(&directory, "artwork.reduced"), 1);
    prepare_at(&config, &event, &root, 5.0).unwrap().unwrap();
    assert_eq!(lines(&directory, "artwork.reduced"), 2, "another scale");

    let log = std::fs::read_to_string(directory.join("aherald.log")).unwrap();
    assert!(!log.contains("secret-"), "{log}");
    let _ = std::fs::remove_dir_all(&directory);
}
