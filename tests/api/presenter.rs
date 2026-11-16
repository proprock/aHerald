//! The presenter decides what the resident card shows now, from queue state and a
//! controlled clock, and reports a card only when something visible changed.
use aherald::{
    events::{Event, Kind},
    presenter::{Input, Presenter, Shown},
    queue::Entry,
};

fn entry(duration: u64, visible_ms: u64) -> Entry {
    Entry {
        outputs: Default::default(),
        id: 1,
        event: Event::demo(Kind::PermissionRequired.name()).unwrap(),
        duration,
        visible_ms,
        announced: true,
        arrived_ms: 0,
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

#[test]
fn sound_slots_cap_concurrent_playback() {
    use aherald::presenter::{MAX_SOUNDS, sound_slots};
    assert_eq!(sound_slots(0), MAX_SOUNDS);
    assert_eq!(sound_slots(MAX_SOUNDS - 1), 1);
    assert_eq!(sound_slots(MAX_SOUNDS), 0);
    assert_eq!(sound_slots(MAX_SOUNDS + 3), 0);
}

mod announcer {
    //! Which sounds a resident pass starts (AH-311): one rule for Windows, macOS and Linux.
    use aherald::{
        events::{Event, Kind},
        focus::Outputs,
        presenter::{Announce, Announcer, MAX_SOUNDS},
        queue::{Entry, Queue},
    };

    fn queue_with(sessions: &[&str], outputs: Outputs) -> Queue {
        let mut queue = Queue::new(16, 0);
        for session in sessions {
            let mut event = Event::demo(Kind::Error.name()).unwrap();
            event.session_id = Some((*session).into());
            assert!(queue.push_with_outputs(event, 0, 5000, outputs).is_queued());
        }
        queue
    }

    fn sample(id: u64) -> Entry {
        Entry {
            outputs: Outputs::default(),
            id,
            event: Event::demo(Kind::TurnComplete.name()).unwrap(),
            duration: 0,
            visible_ms: 0,
            announced: true,
            arrived_ms: 0,
        }
    }

    fn sessions(announce: &Announce) -> Vec<String> {
        announce
            .play
            .iter()
            .map(|e| e.event.session_id.clone().unwrap_or_default())
            .collect()
    }

    #[test]
    fn a_new_event_sounds_while_the_usage_card_is_selected() {
        let mut queue = Queue::new(16, 0);
        queue.show_usage();
        let mut event = Event::demo(Kind::Error.name()).unwrap();
        event.session_id = Some("new".into());
        queue.push(event, 0, 5000);
        assert!(queue.selected().is_some_and(|e| e.is_usage()));
        let announce = Announcer::default().pass(&mut queue, None, false, true, 0);
        assert_eq!(sessions(&announce), ["new"]);
        assert!(!announce.cancel);
    }

    #[test]
    fn a_new_event_sounds_when_its_window_is_suppressed_and_no_card_is_visible() {
        let hidden = Outputs {
            window: false,
            sound: true,
        };
        let mut queue = queue_with(&["focused"], hidden);
        assert!(queue.selected().is_none());
        let announce = Announcer::default().pass(&mut queue, None, false, true, 0);
        assert_eq!(sessions(&announce), ["focused"]);
    }

    #[test]
    fn a_suppressed_sound_never_plays() {
        let silent = Outputs {
            window: true,
            sound: false,
        };
        let mut queue = queue_with(&["silent"], silent);
        let announce = Announcer::default().pass(&mut queue, None, false, true, 0);
        assert!(announce.play.is_empty());
    }

    #[test]
    fn pause_holds_new_sounds_until_it_ends() {
        let mut queue = queue_with(&["held"], Outputs::default());
        let mut announcer = Announcer::default();
        let paused = announcer.pass(&mut queue, None, true, true, 0);
        assert!(paused.play.is_empty() && !paused.cancel);
        let resumed = announcer.pass(&mut queue, None, false, true, 0);
        assert_eq!(sessions(&resumed), ["held"]);
    }

    #[test]
    fn pause_holds_the_preview_sample_until_it_ends() {
        let mut queue = Queue::new(16, 0);
        let mut announcer = Announcer::default();
        let paused = announcer.pass(&mut queue, Some(&sample(7)), true, true, 0);
        assert!(paused.play.is_empty() && !paused.cancel);
        let resumed = announcer.pass(&mut queue, Some(&sample(7)), false, true, 0);
        assert_eq!(resumed.play.iter().map(|e| e.id).collect::<Vec<_>>(), [7]);
    }

    #[test]
    fn events_that_arrive_while_sound_is_off_never_play_later() {
        let mut queue = queue_with(&["a", "b", "c", "d", "e", "f"], Outputs::default());
        let mut announcer = Announcer::default();
        let off = announcer.pass(&mut queue, None, false, false, 0);
        assert!(off.play.is_empty());
        let on = announcer.pass(&mut queue, None, false, true, 0);
        assert!(on.play.is_empty(), "replayed {:?}", sessions(&on));
    }

    #[test]
    fn the_preview_sample_is_silent_while_sound_is_off() {
        let mut queue = Queue::new(16, 0);
        let announce = Announcer::default().pass(&mut queue, Some(&sample(7)), false, false, 0);
        assert!(announce.play.is_empty());
    }

    #[test]
    fn each_preview_sample_sounds_once_per_visit_and_silences_the_previous_one() {
        let mut queue = Queue::new(16, 0);
        let mut announcer = Announcer::default();
        let mut pass = |id| {
            let a = announcer.pass(&mut queue, Some(&sample(id)), false, true, 0);
            (a.play.iter().map(|e| e.id).collect::<Vec<_>>(), a.cancel)
        };
        assert_eq!(pass(1), (vec![1], true));
        assert_eq!(pass(1), (vec![], false));
        assert_eq!(pass(2), (vec![2], true));
        assert_eq!(pass(1), (vec![1], true));
    }

    #[test]
    fn the_preview_sample_sounds_even_when_every_slot_is_busy() {
        let mut queue = Queue::new(16, 0);
        let announce =
            Announcer::default().pass(&mut queue, Some(&sample(3)), false, true, MAX_SOUNDS);
        assert_eq!(announce.play.iter().map(|e| e.id).collect::<Vec<_>>(), [3]);
        assert!(announce.cancel);
    }

    #[test]
    fn at_most_max_sounds_play_at_once_and_the_rest_wait() {
        let mut queue = queue_with(&["a", "b", "c", "d", "e"], Outputs::default());
        let mut announcer = Announcer::default();
        let first = announcer.pass(&mut queue, None, false, true, 0);
        assert_eq!(first.play.len(), MAX_SOUNDS);
        let busy = announcer.pass(&mut queue, None, false, true, MAX_SOUNDS);
        assert!(busy.play.is_empty());
        let later = announcer.pass(&mut queue, None, false, true, 0);
        assert_eq!(sessions(&later), ["e"]);
    }
}
