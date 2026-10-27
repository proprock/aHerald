use aherald::{config::UsageDisplay, tray_usage::Levels, usage::Snapshot, usage_service::View};
use serde_json::json;
use std::time::Duration;

fn view(primary: Option<f64>, secondary: Option<f64>) -> View {
    let mut v = View::new(true, Duration::from_secs(300));
    v.snapshot = Some(
        Snapshot::parse(json!({"rateLimitsByLimitId":{"codex":{
            "primary":{"usedPercent":primary},"secondary":{"usedPercent":secondary}
        }}}))
        .unwrap(),
    );
    v.updated = Some(Duration::ZERO);
    v
}
fn pixel(pixels: &[u8], size: usize, x: usize, y: usize) -> [u8; 4] {
    pixels[(y * size + x) * 4..(y * size + x) * 4 + 4]
        .try_into()
        .unwrap()
}
const GREEN: [u8; 4] = [85, 180, 34, 255];
const YELLOW: [u8; 4] = [0, 165, 216, 255];
const RED: [u8; 4] = [68, 68, 224, 255];
const GRAY: [u8; 4] = [133, 133, 133, 255];

#[test]
fn bars_keep_primary_left_secondary_right_and_thresholds_use_remaining() {
    for (used, expected) in [
        (0., GREEN),
        (39.9, GREEN),
        (40., YELLOW),
        (79.9, YELLOW),
        (80., RED),
        (100., RED),
    ] {
        for display in [UsageDisplay::Used, UsageDisplay::Remain] {
            let pixels = Levels::from_view(&view(Some(used), None), display)
                .render(16)
                .unwrap();
            assert_eq!(
                pixel(&pixels, 16, 2, 14),
                expected,
                "used={used}, display={display:?}"
            );
            assert_eq!(pixel(&pixels, 16, 10, 2), GRAY);
        }
    }
}
#[test]
fn fill_follows_display_while_color_follows_remaining() {
    let v = view(Some(25.), Some(90.));
    let remain = Levels::from_view(&v, UsageDisplay::Remain)
        .render(16)
        .unwrap();
    let used = Levels::from_view(&v, UsageDisplay::Used)
        .render(16)
        .unwrap();
    assert_eq!(pixel(&remain, 16, 2, 5), GREEN);
    assert_ne!(pixel(&used, 16, 2, 5), GREEN);
    assert_ne!(pixel(&remain, 16, 10, 5), RED);
    assert_eq!(pixel(&used, 16, 10, 5), RED);
}
#[test]
fn no_data_disabled_and_partial_windows_are_gray() {
    for mut v in [
        View::new(true, Duration::from_secs(300)),
        view(None, None),
        view(Some(20.), Some(30.)),
    ] {
        if v.snapshot.as_ref().is_some_and(|s| {
            s.buckets["codex"]
                .primary
                .as_ref()
                .unwrap()
                .used_percent
                .is_some()
        }) {
            v.enabled = false;
        }
        let pixels = Levels::from_view(&v, UsageDisplay::Remain)
            .render(16)
            .unwrap();
        assert_eq!(pixel(&pixels, 16, 2, 2), GRAY);
        assert_eq!(pixel(&pixels, 16, 10, 14), GRAY);
    }
}
#[test]
fn cached_values_keep_color_and_no_stale_status_after_time_or_failure() {
    let mut v = view(Some(25.), Some(90.));
    let before = Levels::from_view(&v, UsageDisplay::Remain);
    v.error = Some("refresh failed".into());
    assert_eq!(before, Levels::from_view(&v, UsageDisplay::Remain));
    v.updated = Some(Duration::from_secs(10));
    let now = Duration::from_secs(9000);
    assert!(v.summary(now).contains("current"));
    for text in [v.card_text(now, 1000), v.details(now)] {
        assert!(text.contains("refresh failed"), "{text}");
        assert!(text.contains("8990"), "{text}");
    }
    for text in [
        v.summary(now),
        v.compact(now, 1000, UsageDisplay::Remain),
        v.card_text(now, 1000),
        v.details(now),
    ] {
        assert!(!text.contains("stale"), "{text}");
    }
}
#[test]
fn raster_has_transparent_padding_separate_bars_and_bounded_sizes() {
    let levels = Levels::from_view(&view(Some(0.), Some(0.)), UsageDisplay::Remain);
    for size in [16, 20, 24, 32, 48, 64] {
        let pixels = levels.render(size).unwrap();
        assert_eq!(pixels.len(), (size * size * 4) as usize);
        assert_eq!(pixel(&pixels, size as usize, 0, 0), [0; 4]);
        assert!(pixels.as_chunks::<4>().0.contains(&GREEN));
        assert_eq!(
            pixel(
                &pixels,
                size as usize,
                (size / 2) as usize,
                (size / 2) as usize
            ),
            [0; 4]
        );
    }
    for size in [0, 7, 257, u32::MAX] {
        assert!(levels.render(size).is_err());
    }
}
#[test]
fn empty_and_full_keep_their_status_color_visible() {
    let pixels = Levels::from_view(&view(Some(100.), Some(0.)), UsageDisplay::Remain)
        .render(16)
        .unwrap();
    assert_eq!(pixel(&pixels, 16, 2, 14), RED);
    assert_eq!(pixel(&pixels, 16, 10, 1), GREEN);
    assert_ne!(pixel(&pixels, 16, 2, 1), RED);
}

#[test]
fn malformed_values_are_gray_and_the_codex_bucket_has_priority() {
    let mut v = view(Some(25.0), Some(30.0));
    let snapshot = v.snapshot.as_mut().unwrap();
    let mut other = snapshot.buckets["codex"].clone();
    other.primary.as_mut().unwrap().used_percent = Some(99.0);
    snapshot.buckets.insert("aaa-other".into(), other);
    let selected = Levels::from_view(&v, UsageDisplay::Remain)
        .render(16)
        .unwrap();
    assert_eq!(pixel(&selected, 16, 2, 14), GREEN);
    for invalid in [f64::NAN, f64::INFINITY, -1.0, 101.0] {
        v.snapshot
            .as_mut()
            .unwrap()
            .buckets
            .get_mut("codex")
            .unwrap()
            .primary
            .as_mut()
            .unwrap()
            .used_percent = Some(invalid);
        let pixels = Levels::from_view(&v, UsageDisplay::Remain)
            .render(16)
            .unwrap();
        assert_eq!(pixel(&pixels, 16, 2, 2), GRAY);
    }
    v.snapshot.as_mut().unwrap().buckets.remove("codex");
    let fallback = Levels::from_view(&v, UsageDisplay::Remain)
        .render(16)
        .unwrap();
    assert_eq!(pixel(&fallback, 16, 2, 14), RED);
}
