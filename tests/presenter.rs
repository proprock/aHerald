//! The presenter decides what the resident card shows now, from queue state and a
//! controlled clock, and reports a card only when something visible changed.
use aherald::{
    events::{Event, Kind},
    presenter::{Input, Presenter, Shown},
    queue::Entry,
};

fn entry(duration: u64, visible_ms: u64) -> Entry {
    Entry {
        id: 1,
        event: Event::demo(Kind::PermissionRequired.name()).unwrap(),
        duration,
        visible_ms,
        announced: true,
    }
}

fn input<'a>(entry: &'a Entry, durations: &'a [u64], animation_ms: u64) -> Input<'a> {
    Input {
        entry,
        durations,
        looping: true,
        animation_ms,
        fade_in_ms: 0,
        fade_out_ms: 200,
    }
}

#[test]
fn static_persistent_card_is_presented_once() {
    let mut presenter = Presenter::default();
    let mut e = entry(0, 0);
    let first = presenter.next(&input(&e, &[1000], 0));
    assert_eq!(
        first,
        Some(Shown {
            frame: 0,
            alpha: 255,
            seconds: None
        })
    );
    for ms in (16..5000).step_by(16) {
        e.visible_ms = ms;
        assert_eq!(presenter.next(&input(&e, &[1000], ms)), None, "at {ms} ms");
    }
}

#[test]
fn countdown_card_is_presented_only_when_the_second_changes() {
    let mut presenter = Presenter::default();
    // Queue lifetime 5200 ms includes the 200 ms fade-out: 5 s of countdown.
    let mut shown = Vec::new();
    for ms in (0..5000).step_by(16) {
        let e = entry(5200, ms);
        if let Some(s) = presenter.next(&input(&e, &[1000], ms)) {
            shown.push((ms, s.seconds));
        }
    }
    assert_eq!(
        shown.iter().map(|(_, s)| *s).collect::<Vec<_>>(),
        [Some(5), Some(4), Some(3), Some(2), Some(1)]
    );
    assert_eq!(shown[1].0, 16 * 63); // first tick past 1000 ms
}

#[test]
fn animation_frame_change_is_presented() {
    let mut presenter = Presenter::default();
    let e = entry(0, 0);
    let frames = [100, 100, 100];
    let seen: Vec<usize> = [0, 50, 100, 150, 200, 250, 300, 350]
        .into_iter()
        .filter_map(|ms| presenter.next(&input(&e, &frames, ms)))
        .map(|s| s.frame)
        .collect();
    assert_eq!(seen, [0, 1, 2, 0]);
}

#[test]
fn alpha_change_is_presented_during_fades() {
    let mut presenter = Presenter::default();
    let mut config = |visible| {
        let e = entry(0, visible);
        let mut i = input(&e, &[1000], 0);
        i.fade_in_ms = 100;
        presenter.next(&i).map(|s| s.alpha)
    };
    assert_eq!(config(0), Some(0));
    assert_eq!(config(50), Some(127));
    assert_eq!(config(50), None);
    assert_eq!(config(100), Some(255));
    assert_eq!(config(150), None);
}

#[test]
fn fade_out_alpha_is_presented() {
    let mut presenter = Presenter::default();
    let alphas: Vec<u8> = [4800, 5000, 5100, 5150]
        .into_iter()
        .filter_map(|ms| presenter.next(&input(&entry(5200, ms), &[1000], 0)))
        .map(|s| s.alpha)
        .collect();
    assert_eq!(alphas.len(), 4);
    assert!(alphas.windows(2).all(|w| w[0] >= w[1]));
    assert!(alphas[2] < 255);
}

#[test]
fn static_card_is_presented_once_until_invalidated() {
    let mut presenter = Presenter::default();
    assert!(presenter.next_static().is_some());
    assert!(presenter.next_static().is_none());
    presenter.invalidate();
    assert!(presenter.next_static().is_some());
}

#[test]
fn invalidate_forces_the_next_presentation() {
    let mut presenter = Presenter::default();
    let e = entry(0, 0);
    assert!(presenter.next(&input(&e, &[1000], 0)).is_some());
    assert!(presenter.next(&input(&e, &[1000], 16)).is_none());
    presenter.invalidate();
    assert!(presenter.next(&input(&e, &[1000], 32)).is_some());
}
