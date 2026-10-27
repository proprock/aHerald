use aherald::{config::Config, demo::Demo, events::Kind, queue::Queue};

#[test]
fn grouped_waits_show_countdowns_even_when_the_configured_wait_is_persistent() {
    let config = Config::default();
    assert_eq!(config.events[Kind::PermissionRequired].duration_ms, 0);
    let mut demo = Demo::new(&config, 1).unwrap();
    let mut queue = Queue::new(64, 1500);
    for now in [
        0, 3300, 4300, 7600, 8600, 11900, 12900, 16200, 17200, 20500, 21500,
    ] {
        demo.tick(now, false, &mut queue);
    }
    assert_eq!(queue.items().len(), 5);
    for entry in queue.items() {
        let duration = entry.display_duration(config.notifications.fade_out_ms);
        assert_eq!(duration, 3000);
        for (visible, expected) in [(0, Some(3)), (1000, Some(2)), (2000, Some(1)), (3000, None)] {
            assert_eq!(
                aherald::card::remaining_seconds(duration, visible),
                expected
            );
        }
        assert_eq!(
            aherald::notification::presentation(3150, duration, 0, 300, None).0,
            127
        );
    }
}

#[test]
fn grouped_messages_expire_one_at_a_time_with_a_fresh_timeout() {
    let mut config = Config::default();
    for (kind, setting) in config.events.iter_mut() {
        setting.enabled = !matches!(kind, Kind::PermissionRequired | Kind::InputRequired);
        setting.duration_ms = 3000;
    }
    config.notifications.fade_out_ms = 0;
    let mut demo = Demo::new(&config, 1).unwrap();
    let mut queue = Queue::new(64, 1500);
    for now in [0, 3000, 4000, 7000, 8000, 11000, 12000] {
        demo.tick(now, false, &mut queue);
    }
    queue.tick(12000, true);
    for (step, remaining) in [(0, 3), (1, 2), (2, 1)] {
        let now = 12000 + step * 3000;
        assert_eq!(queue.items().len(), remaining);
        assert_eq!(queue.selected_index(), 0);
        assert_eq!(queue.selected().unwrap().visible_ms, 0);
        demo.tick(now + 2999, false, &mut queue);
        queue.tick(now + 2999, false);
        assert_eq!(queue.items().len(), remaining);
        demo.tick(now + 3000, false, &mut queue);
        queue.tick(now + 3000, false);
        assert_eq!(queue.items().len(), remaining - 1);
    }
    demo.tick(21001, false, &mut queue);
    demo.tick(22000, false, &mut queue);
    assert!(queue.items().is_empty());
    demo.tick(22001, false, &mut queue);
    assert_eq!(queue.items().len(), 1);
}

#[test]
fn custom_pause_has_an_exact_boundary_and_missed_ticks_do_not_flood() {
    let config = Config::default();
    let mut demo = Demo::new(&config, 1).unwrap();
    let mut queue = Queue::new(64, 60_000);
    demo.tick(0, false, &mut queue);
    demo.tick(3300, false, &mut queue);
    assert!(queue.items().is_empty());
    demo.tick(4299, false, &mut queue);
    assert!(queue.items().is_empty());
    demo.tick(4300, false, &mut queue);
    assert_eq!(
        queue.selected().unwrap().event.kind.name(),
        "permission_required"
    );
    demo.tick(1_000_000, false, &mut queue);
    assert!(queue.items().is_empty());
    demo.tick(1_000_999, false, &mut queue);
    assert!(queue.items().is_empty());
    demo.tick(1_001_000, false, &mut queue);
    assert_eq!(queue.items().len(), 1);
    assert_eq!(
        queue.selected().unwrap().event.kind.name(),
        "input_required"
    );
}

#[test]
fn cycles_all_signals_then_navigation_and_repeats_with_bounded_queue() {
    let mut config = Config::default();
    for setting in config.events.values_mut() {
        setting.duration_ms = 100;
    }
    config.notifications.fade_out_ms = 0;
    let mut demo = Demo::new(&config, 3).unwrap();
    let mut queue = Queue::new(64, 1500);
    let mut now = 0;
    for _ in 0..3 {
        for kind in Kind::ALL {
            demo.tick(now, false, &mut queue);
            assert_eq!(queue.items().len(), 1);
            assert_eq!(queue.selected().unwrap().event.kind, kind);
            demo.tick(now + 3100, false, &mut queue);
            assert!(queue.items().is_empty());
            now += 6100;
        }
        demo.tick(now, false, &mut queue);
        assert_eq!(queue.items().len(), 5);
        let first = queue.selected().unwrap().event.kind;
        queue.next(1);
        assert_ne!(queue.selected().unwrap().event.kind, first);
        queue.dismiss();
        assert_eq!(queue.items().len(), 4);
        queue.tick(now, true);
        for remaining in (0..4).rev() {
            now += 3100;
            demo.tick(now, false, &mut queue);
            queue.tick(now, false);
            assert_eq!(queue.items().len(), remaining);
        }
        assert!(queue.items().is_empty());
        demo.tick(now, false, &mut queue);
        now += 3000;
    }
}

#[test]
fn respects_disabled_events_persistent_duration_pause_and_long_presentations() {
    let mut config = Config::default();
    for setting in config.events.values_mut() {
        setting.enabled = false;
    }
    assert!(Demo::new(&config, 3).is_err());
    config.events[Kind::PermissionRequired].enabled = true;
    let mut demo = Demo::new(&config, 3).unwrap();
    let mut queue = Queue::new(64, 1500);
    demo.tick(0, false, &mut queue);
    assert_eq!(queue.selected().unwrap().duration, 0);
    demo.tick(20_000, true, &mut queue);
    demo.tick(20_100, false, &mut queue);
    assert_eq!(queue.items().len(), 1);
    demo.tick(23_400, false, &mut queue);
    assert!(queue.items().is_empty());
    demo.tick(26_400, false, &mut queue);
    assert_eq!(queue.items().len(), 1);
    assert_eq!(
        queue.selected().unwrap().event.kind.name(),
        "permission_required"
    );

    config.events[Kind::PermissionRequired].duration_ms = 10_000;
    let mut demo = Demo::new(&config, 3).unwrap();
    demo.tick(0, false, &mut queue);
    demo.tick(9999, false, &mut queue);
    assert_eq!(queue.items().len(), 1);
    demo.tick(10_300, false, &mut queue);
    assert!(queue.items().is_empty());
    assert_eq!(config.events[Kind::PermissionRequired].duration_ms, 10_000);
}
