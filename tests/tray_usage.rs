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

fn graph(
    codex: &View,
    claude: &View,
    mode: aherald::config::TrayGraph,
    display: UsageDisplay,
) -> Levels {
    Levels::from_views(codex, Some(claude), mode, display)
}

#[test]
fn auto_selects_whole_agent_by_worst_remaining_window_and_updates() {
    use aherald::config::TrayGraph::*;
    let codex = view(Some(20.), Some(90.));
    let mut claude = view(Some(85.), Some(30.));
    for display in [UsageDisplay::Remain, UsageDisplay::Used] {
        assert_eq!(
            graph(&codex, &claude, Auto, display),
            Levels::from_view(&codex, display)
        );
    }
    claude = view(Some(95.), Some(30.));
    assert_eq!(
        graph(&codex, &claude, Auto, UsageDisplay::Remain),
        Levels::from_view(&claude, UsageDisplay::Remain)
    );
    claude.error = Some("refresh failed".into());
    assert_eq!(
        graph(&codex, &claude, Auto, UsageDisplay::Remain),
        Levels::from_view(&claude, UsageDisplay::Remain)
    );
    claude = view(Some(90.), Some(20.));
    assert_eq!(
        graph(&codex, &claude, Auto, UsageDisplay::Remain),
        Levels::from_view(&codex, UsageDisplay::Remain)
    );
}

#[test]
fn auto_ignores_disabled_missing_and_invalid_data_but_keeps_partial_windows() {
    use aherald::config::TrayGraph::*;
    let codex = view(Some(20.), Some(30.));
    let mut claude = view(Some(100.), None);
    assert_eq!(
        graph(&codex, &claude, Auto, UsageDisplay::Remain),
        Levels::from_view(&claude, UsageDisplay::Remain)
    );
    claude.enabled = false;
    assert_eq!(
        graph(&codex, &claude, Auto, UsageDisplay::Remain),
        Levels::from_view(&codex, UsageDisplay::Remain)
    );
    for invalid in [
        None,
        Some(-1.),
        Some(101.),
        Some(f64::NAN),
        Some(f64::INFINITY),
    ] {
        claude = view(None, None);
        claude
            .snapshot
            .as_mut()
            .unwrap()
            .buckets
            .get_mut("codex")
            .unwrap()
            .primary
            .as_mut()
            .unwrap()
            .used_percent = invalid;
        assert_eq!(
            graph(&codex, &claude, Auto, UsageDisplay::Remain),
            Levels::from_view(&codex, UsageDisplay::Remain)
        );
    }
    let empty = View::new(true, Duration::ZERO);
    let claude = view(Some(90.), None);
    assert_eq!(
        graph(&empty, &claude, Auto, UsageDisplay::Remain),
        Levels::from_view(&claude, UsageDisplay::Remain)
    );
    let mut disabled = codex.clone();
    disabled.enabled = false;
    assert_eq!(
        graph(&disabled, &claude, Auto, UsageDisplay::Remain),
        Levels::from_view(&claude, UsageDisplay::Remain)
    );
    assert_eq!(
        graph(&empty, &empty, Auto, UsageDisplay::Remain),
        Levels::from_view(&empty, UsageDisplay::Remain)
    );
    assert_eq!(
        Levels::from_views(&codex, None, Auto, UsageDisplay::Remain),
        Levels::from_view(&codex, UsageDisplay::Remain)
    );
}

#[test]
fn exclusive_graphs_never_fall_back_to_another_agent() {
    use aherald::config::TrayGraph::*;
    let codex = view(Some(20.), Some(30.));
    let claude = view(Some(90.), None);
    let empty = View::new(false, Duration::ZERO);
    for display in [UsageDisplay::Remain, UsageDisplay::Used] {
        assert_eq!(
            graph(&codex, &claude, Codex, display),
            Levels::from_view(&codex, display)
        );
        assert_eq!(
            graph(&codex, &claude, Claude, display),
            Levels::from_view(&claude, display)
        );
        assert_eq!(
            graph(&empty, &claude, Codex, display),
            Levels::from_view(&empty, display)
        );
        assert_eq!(
            Levels::from_views(&codex, None, Claude, display),
            Levels::from_view(&empty, display)
        );
    }
}

#[test]
fn both_splits_each_window_into_claude_then_codex_at_every_size() {
    use aherald::config::TrayGraph::Both;
    let codex = view(Some(80.), Some(0.));
    let claude = view(Some(0.), Some(50.));
    for size in [8u32, 16, 20, 24, 32, 48, 64, 256] {
        let margin = ((size + 8) / 16).max(1);
        let gap = ((size + 4) / 8).max(1);
        let space = size - 2 * margin - gap;
        for display in [UsageDisplay::Remain, UsageDisplay::Used] {
            let pixels = graph(&codex, &claude, Both, display).render(size).unwrap();
            for (window, colors) in [[GREEN, RED], [YELLOW, GREEN]].into_iter().enumerate() {
                let i = window as u32;
                let left = margin + i * space / 2 + i * gap;
                let width = (i + 1) * space / 2 - i * space / 2;
                for (agent, color) in colors.into_iter().enumerate() {
                    let x = left + if agent == 0 { 0 } else { width / 2 };
                    assert_eq!(
                        pixel(
                            &pixels,
                            size as usize,
                            x as usize,
                            (size - margin - 1) as usize
                        ),
                        color
                    );
                }
            }
            assert_eq!(pixel(&pixels, size as usize, 0, 0), [0; 4]);
        }
    }
    let remain = graph(&codex, &claude, Both, UsageDisplay::Remain)
        .render(16)
        .unwrap();
    let used = graph(&codex, &claude, Both, UsageDisplay::Used)
        .render(16)
        .unwrap();
    assert_eq!(pixel(&remain, 16, 1, 1), GREEN);
    assert_ne!(pixel(&used, 16, 1, 1), GREEN);
    assert_ne!(pixel(&remain, 16, 4, 1), RED);
    assert_ne!(pixel(&remain, 16, 4, 5), RED);
    assert_eq!(pixel(&used, 16, 4, 5), RED);
    assert_eq!(pixel(&used, 16, 4, 14), RED);
    assert_eq!(pixel(&remain, 16, 8, 8), [0; 4]);
    let missing = Levels::from_views(&codex, None, Both, UsageDisplay::Remain)
        .render(16)
        .unwrap();
    assert_eq!(pixel(&missing, 16, 1, 1), GRAY);
    assert_eq!(pixel(&missing, 16, 4, 14), RED);
    assert_eq!(pixel(&missing, 16, 9, 1), GRAY);
    assert_eq!(pixel(&missing, 16, 12, 1), GREEN);
    let mut disabled = codex.clone();
    disabled.enabled = false;
    let missing = graph(&disabled, &claude, Both, UsageDisplay::Remain)
        .render(16)
        .unwrap();
    assert_eq!(pixel(&missing, 16, 1, 1), GREEN);
    assert_eq!(pixel(&missing, 16, 4, 1), GRAY);
    assert_eq!(pixel(&missing, 16, 12, 1), GRAY);
}

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
    let card = v.card_text(now, 1000);
    assert!(!card.contains("Status:"), "{card}");
    assert!(card.contains("refresh failed"), "{card}");
    assert!(card.contains("8990"), "{card}");
    for text in [v.compact(1000, UsageDisplay::Remain), card] {
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
