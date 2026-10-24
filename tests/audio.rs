use aherald::{
    audio::{Wave, selected},
    config::Config,
    events::Event,
};
fn wav(samples: &[i16]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend(b"RIFF");
    b.extend((36 + samples.len() as u32 * 2).to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend(8000u32.to_le_bytes());
    b.extend(16000u32.to_le_bytes());
    b.extend(2u16.to_le_bytes());
    b.extend(16u16.to_le_bytes());
    b.extend(b"data");
    b.extend((samples.len() as u32 * 2).to_le_bytes());
    for s in samples {
        b.extend(s.to_le_bytes());
    }
    b
}
#[test]
fn playback_volume_scales_only_owned_samples() {
    let bytes = wav(&[-32768, 0, 32766]);
    let half = Wave::parse(&bytes, 0.5).unwrap();
    assert_eq!(half.samples, [-16384, 0, 16383]);
    assert_eq!(half.channels, 1);
    assert_eq!(half.sample_rate, 8000);
    assert_eq!(Wave::parse(&bytes, 0.0).unwrap().samples, [0, 0, 0]);
    assert_eq!(
        Wave::parse(&bytes, 1.0).unwrap().samples,
        [-32768, 0, 32766]
    );
}
#[test]
fn invalid_and_missing_audio_is_rejected_without_panicking() {
    for bytes in [
        vec![],
        b"not WAV".to_vec(),
        wav(&[]),
        wav(&[1])[..40].to_vec(),
    ] {
        assert!(Wave::parse(&bytes, 1.0).is_err());
    }
    let mut bytes = wav(&[1]);
    bytes[20] = 3;
    assert!(Wave::parse(&bytes, 1.0).is_err());
    assert!(Wave::parse(&wav(&[1]), f64::NAN).is_err());
    assert!(Wave::load(std::path::Path::new("does-not-exist.wav"), 0.5).is_err());
}
#[test]
fn global_event_and_volume_mutes_are_honored() {
    let event = Event::demo("turn_complete").unwrap();
    let mut config = Config::default();
    config.events.get_mut("turn_complete").unwrap().sound = Some("tone.wav".into());
    assert_eq!(
        selected(&config, &event).unwrap(),
        std::path::Path::new("tone.wav")
    );
    config.sounds.volume = 0.0;
    assert!(selected(&config, &event).is_none());
    config.sounds.volume = 0.5;
    config.sounds.enabled = false;
    assert!(selected(&config, &event).is_none());
    config.sounds.enabled = true;
    config
        .events
        .get_mut("turn_complete")
        .unwrap()
        .sound_enabled = false;
    assert!(selected(&config, &event).is_none());
}

#[test]
fn shipped_resources_decode_for_every_event() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in [
        "turn_complete",
        "permission_required",
        "input_required",
        "error",
        "interrupted",
    ] {
        let path = root.join("assets").join(name);
        let animation = aherald::animation::Animation::load(&path).unwrap();
        assert_eq!(animation.frames.len(), 8);
        assert_eq!(animation.frames[0].dimensions(), (64, 64));
        let wave = Wave::load(&path.join("sound.wav"), 0.5).unwrap();
        assert_eq!(wave.sample_rate, 22050);
        assert!(!wave.samples.is_empty());
    }
}
