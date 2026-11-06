use aherald::{
    audio::{SoundError, Wave, selected},
    config::Config,
    events::{Event, Kind},
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
fn every_rejection_has_its_own_fixed_reason() {
    let long = wav(&vec![0i16; 8000 * 6]);
    let mut float = wav(&[1]);
    float[20] = 3;
    let mut eight_bit = wav(&[1]);
    eight_bit[34] = 8;
    let cases = [
        (long, SoundError::TooLong, "too_long"),
        (
            vec![0u8; 2 * 1024 * 1024 + 1],
            SoundError::TooLarge,
            "too_large",
        ),
        (float, SoundError::Unsupported, "unsupported"),
        (eight_bit, SoundError::Unsupported, "unsupported"),
        (b"not WAV".to_vec(), SoundError::Malformed, "malformed"),
        (wav(&[]), SoundError::Malformed, "malformed"),
        (wav(&[1])[..40].to_vec(), SoundError::Malformed, "malformed"),
    ];
    for (bytes, error, code) in cases {
        let found = Wave::parse(&bytes, 1.0).unwrap_err();
        assert_eq!(found, error);
        assert_eq!(found.code(), code);
    }
    let missing = Wave::load(std::path::Path::new("does-not-exist.wav"), 0.5).unwrap_err();
    assert_eq!(
        (missing, missing.code()),
        (SoundError::Unreadable, "unreadable")
    );
}
#[test]
fn an_aherald_notice_never_has_a_sound() {
    let mut config = Config::default();
    config.sounds.enabled = true;
    config.sounds.volume = 1.0;
    let notice = Event::notice("Sound over 5 s: a.wav");
    config.events[notice.kind].sound = Some("alert.wav".into());
    assert!(selected(&config, &notice).is_none());
    // The same setting still sounds for an agent's error.
    assert!(selected(&config, &Event::demo("error").unwrap()).is_some());
}
#[test]
fn global_event_and_volume_mutes_are_honored() {
    let event = Event::demo("turn_complete").unwrap();
    let mut config = Config::default();
    config.events[Kind::TurnComplete].sound = Some("tone.wav".into());
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
    config.events[Kind::TurnComplete].sound_enabled = false;
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

mod cache {
    use super::wav;
    use aherald::audio::SoundCache;
    use std::{path::PathBuf, sync::Arc};

    struct Dir(PathBuf);
    impl Dir {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("aherald-sound-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn file(&self, name: &str, samples: &[i16]) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, wav(samples)).unwrap();
            path
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn the_same_file_and_volume_is_decoded_once() {
        let dir = Dir::new("once");
        let path = dir.file("a.wav", &[100, -100, 200]);
        let mut cache = SoundCache::new(4);
        let first = cache.get(&path, 0.5).unwrap();
        let second = cache.get(&path, 0.5).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.samples, [50, -50, 100]);
    }

    #[test]
    fn another_volume_is_a_separate_decode() {
        let dir = Dir::new("volume");
        let path = dir.file("a.wav", &[1000]);
        let mut cache = SoundCache::new(4);
        let quiet = cache.get(&path, 0.25).unwrap();
        let loud = cache.get(&path, 1.0).unwrap();
        assert_eq!((quiet.samples[0], loud.samples[0]), (250, 1000));
    }

    #[test]
    fn a_replaced_file_is_decoded_again() {
        let dir = Dir::new("replaced");
        let path = dir.file("a.wav", &[10, 20]);
        let mut cache = SoundCache::new(4);
        let old = cache.get(&path, 1.0).unwrap();
        dir.file("a.wav", &[10, 20, 30, 40]);
        let new = cache.get(&path, 1.0).unwrap();
        assert_eq!(old.samples, [10, 20]);
        assert_eq!(new.samples, [10, 20, 30, 40]);
    }

    #[test]
    fn a_deleted_or_corrupt_file_is_an_error_not_a_stale_sound() {
        let dir = Dir::new("gone");
        let path = dir.file("a.wav", &[1, 2]);
        let mut cache = SoundCache::new(4);
        cache.get(&path, 1.0).unwrap();
        std::fs::remove_file(&path).unwrap();
        assert!(cache.get(&path, 1.0).is_err());
        std::fs::write(&path, b"not a wav").unwrap();
        assert!(cache.get(&path, 1.0).is_err());
    }

    #[test]
    fn the_cache_is_bounded_and_evicts_the_least_recently_used() {
        let dir = Dir::new("lru");
        let mut cache = SoundCache::new(2);
        let (a, b, c) = (
            dir.file("a.wav", &[1]),
            dir.file("b.wav", &[2]),
            dir.file("c.wav", &[3]),
        );
        let first_a = cache.get(&a, 1.0).unwrap();
        cache.get(&b, 1.0).unwrap();
        cache.get(&a, 1.0).unwrap(); // a is now the most recent
        cache.get(&c, 1.0).unwrap(); // evicts b
        assert!(Arc::ptr_eq(&first_a, &cache.get(&a, 1.0).unwrap()));
        assert_eq!(cache.len(), 2);
    }
}
