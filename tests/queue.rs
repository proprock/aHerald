use aherald::{
    events::{Event, Kind, Source},
    queue::Queue,
};
fn event(kind: Kind, session: &str) -> Event {
    let mut e = Event::demo(kind.name()).unwrap();
    e.session_id = Some(session.into());
    e
}
#[test]
fn flood_and_duplicate_storm_stay_bounded_and_allow_recovery() {
    let mut queue = Queue::new(64, 60_000);
    let repeated = event(Kind::PermissionRequired, "duplicate");
    for i in 0..10_000 {
        queue.push(repeated.clone(), i, 0);
        queue.push(event(Kind::Error, &format!("session-{i}")), i, 0);
        assert!(queue.items().len() <= 64);
    }
    assert_eq!(
        queue
            .items()
            .iter()
            .filter(|e| e.event.session_id.as_deref() == Some("duplicate"))
            .count(),
        1
    );
    while !queue.items().is_empty() {
        queue.dismiss();
    }
    assert!(
        queue
            .push(event(Kind::PermissionRequired, "recovered"), 70_001, 0)
            .is_queued()
    );
    assert_eq!(queue.items().len(), 1);
}
#[test]
fn priority_fifo_and_overflow_preserve_unrelated_sessions() {
    let mut q = Queue::new(3, 1000);
    assert!(q.push(event(Kind::Interrupted, "low"), 0, 3000).is_queued());
    assert!(q.push(event(Kind::Error, "medium"), 0, 3000).is_queued());
    assert!(
        q.push(event(Kind::PermissionRequired, "high-1"), 0, 0)
            .is_queued()
    );
    assert!(
        q.push(event(Kind::InputRequired, "high-2"), 0, 0)
            .is_queued()
    );
    assert_eq!(
        q.items()
            .iter()
            .map(|e| e.event.session_id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["high-1", "high-2", "medium"]
    );
    assert!(
        !q.push(event(Kind::Interrupted, "new-low"), 1, 3000)
            .is_queued()
    );
    assert!(
        !q.push(event(Kind::PermissionRequired, "high-1"), 2, 0)
            .is_queued()
    );
    assert_eq!(q.items().len(), 3);
}

#[test]
fn persistent_navigation_individual_dismissal_and_selected_time_only() {
    let mut q = Queue::new(4, 1000);
    q.push(event(Kind::PermissionRequired, "one"), 0, 0);
    q.push(event(Kind::InputRequired, "two"), 0, 0);
    q.push(event(Kind::Error, "three"), 0, 100);
    q.tick(0, false);
    q.tick(10000, false);
    assert_eq!(q.items().len(), 3);
    q.next(1);
    assert_eq!(
        q.selected().unwrap().event.session_id.as_deref(),
        Some("two")
    );
    q.dismiss();
    assert_eq!(q.items().len(), 2);
    assert_eq!(
        q.selected().unwrap().event.session_id.as_deref(),
        Some("three")
    );
    q.tick(10050, true);
    q.tick(10100, true);
    assert_eq!(q.items().len(), 2);
    q.tick(10150, false);
    q.tick(10200, false);
    assert_eq!(q.items().len(), 1);
    assert_eq!(
        q.selected().unwrap().event.session_id.as_deref(),
        Some("one")
    );
}
#[test]
fn identity_dedup_and_dismissed_history_are_bounded_by_time() {
    let mut q = Queue::new(4, 100);
    let mut e = event(Kind::PermissionRequired, "same");
    e.source = Source::CodexHook;
    e.turn_id = Some("turn".into());
    e.request_id = Some("request".into());
    assert!(q.push(e.clone(), 0, 0).is_queued());
    q.dismiss();
    assert!(!q.push(e.clone(), 50, 0).is_queued());
    e.request_id = Some("different".into());
    assert!(q.push(e.clone(), 60, 0).is_queued());
    e.session_id = Some("another-session".into());
    assert!(q.push(e.clone(), 70, 0).is_queued());
    assert!(
        q.push(event(Kind::PermissionRequired, "same"), 200, 0)
            .is_queued()
    );
    let mut unknown = Event::demo("error").unwrap();
    unknown.source = Source::CodexHook;
    let mut q = Queue::new(4, 100);
    assert!(q.push(unknown.clone(), 0, 0).is_queued());
    assert!(q.push(unknown, 1, 0).is_queued());
}

#[test]
fn usage_is_one_persistent_queue_entry_and_dismissal_selects_next_message() {
    let mut q = aherald::queue::Queue::new(2, 5000);
    q.show_usage();
    q.show_usage();
    assert_eq!(q.items().len(), 1);
    assert!(q.selected().unwrap().is_usage());
    assert!(
        q.push(
            aherald::events::Event::demo("permission_required").unwrap(),
            0,
            1000
        )
        .is_queued()
    );
    q.tick(0, false);
    q.tick(50_000, false);
    assert!(q.selected().unwrap().is_usage());
    assert_eq!(q.items().len(), 2);
    q.next(1);
    assert!(!q.selected().unwrap().is_usage());
    q.next(-1);
    assert!(q.selected().unwrap().is_usage());
    q.dismiss();
    assert!(!q.selected().unwrap().is_usage());
    assert_eq!(q.items().len(), 1);
    q.show_usage();
    assert!(q.selected().unwrap().is_usage());
}

fn claude(kind: Kind, session: &str) -> Event {
    Event {
        kind,
        source: Source::ClaudeHook,
        session_id: Some(session.into()),
        turn_id: None,
        request_id: None,
        agent_id: None,
        window: None,
    }
}
fn codex_wait(session: &str) -> Event {
    Event {
        source: Source::CodexHook,
        ..claude(Kind::PermissionRequired, session)
    }
}
fn sessions(q: &Queue) -> Vec<(Kind, String)> {
    q.items()
        .iter()
        .map(|e| (e.event.kind, e.event.session_id.clone().unwrap()))
        .collect()
}

#[test]
fn claude_stop_and_stop_failure_resolve_only_pending_cards_of_the_same_session() {
    for end in [Kind::TurnComplete, Kind::Error] {
        let mut q = Queue::new(8, 0);
        q.push(claude(Kind::PermissionRequired, "a"), 0, 0);
        q.push(claude(Kind::InputRequired, "a"), 0, 0);
        q.push(claude(Kind::PermissionRequired, "b"), 0, 0);
        q.push(codex_wait("a"), 0, 0);
        q.push(claude(end, "a"), 1, 5000);
        let left = sessions(&q);
        assert_eq!(
            q.items()
                .iter()
                .filter(|e| e.event.source == Source::ClaudeHook
                    && e.event.session_id.as_deref() == Some("a")
                    && matches!(e.event.kind, Kind::PermissionRequired | Kind::InputRequired))
                .count(),
            0
        );
        assert!(left.contains(&(Kind::PermissionRequired, "b".into())));
        assert_eq!(
            q.items()
                .iter()
                .filter(|e| e.event.source == Source::CodexHook)
                .count(),
            1,
            "a Claude Stop must not clear Codex cards"
        );
        assert!(left.contains(&(end, "a".into())));
    }
}

#[test]
fn claude_stop_resolves_hidden_persistent_card_even_when_waits_are_disabled() {
    let mut q = Queue::new(8, 0);
    q.push(claude(Kind::PermissionRequired, "a"), 0, 0);
    assert!(claude(Kind::TurnComplete, "a").is_terminal_evidence());
    assert_eq!(q.observe_terminal(&claude(Kind::TurnComplete, "a")), 1);
    assert!(q.items().is_empty());
}

#[test]
fn claude_stop_without_session_resolves_nothing() {
    let mut q = Queue::new(8, 0);
    q.push(claude(Kind::PermissionRequired, "a"), 0, 0);
    let mut stop = claude(Kind::TurnComplete, "a");
    stop.session_id = None;
    assert!(!stop.is_terminal_evidence());
    assert_eq!(q.observe_terminal(&stop), 0);
    assert_eq!(q.items().len(), 1);
}

#[test]
fn claude_permission_request_and_notification_pair_make_one_card() {
    let mut q = Queue::new(8, 1500);
    assert!(
        q.push(claude(Kind::PermissionRequired, "a"), 0, 0)
            .is_queued()
    );
    assert!(
        !q.push(claude(Kind::PermissionRequired, "a"), 200, 0)
            .is_queued()
    );
    assert!(
        q.push(claude(Kind::PermissionRequired, "b"), 200, 0)
            .is_queued()
    );
    assert_eq!(q.items().len(), 2);
    // Codex hooks carry no request id, so identical waits stay distinct.
    assert!(q.push(codex_wait("a"), 300, 0).is_queued());
    assert!(q.push(codex_wait("a"), 400, 0).is_queued());
}

#[test]
fn claude_notification_arriving_seconds_after_permission_request_joins_the_pending_card() {
    // Live capture (Claude Code, 2026-10-26): the pair arrived 6 s apart, beyond deduplicate_ms.
    let mut q = Queue::new(8, 1500);
    assert!(
        q.push(claude(Kind::PermissionRequired, "a"), 0, 0)
            .is_queued()
    );
    assert!(
        !q.push(claude(Kind::PermissionRequired, "a"), 6000, 0)
            .is_queued()
    );
    assert_eq!(q.items().len(), 1);
    // Another session still gets its own card, and a resolved prompt may ask again.
    assert!(
        q.push(claude(Kind::PermissionRequired, "b"), 6100, 0)
            .is_queued()
    );
    q.observe_terminal(&claude(Kind::TurnComplete, "a"));
    assert!(
        q.push(claude(Kind::PermissionRequired, "a"), 9000, 0)
            .is_queued()
    );
}

#[test]
fn every_new_event_is_announced_once_even_behind_a_persistent_card() {
    let mut q = Queue::new(8, 0);
    q.push(claude(Kind::PermissionRequired, "a"), 0, 0);
    assert_eq!(q.take_unannounced(4).len(), 1);
    assert!(q.take_unannounced(4).is_empty());
    // Same rank, so the persistent card keeps the selection.
    q.push(claude(Kind::TurnComplete, "b"), 1, 5000);
    assert_eq!(q.selected().unwrap().event.kind, Kind::PermissionRequired);
    let fresh = q.take_unannounced(4);
    assert_eq!(fresh.len(), 1);
    assert_eq!(fresh[0].event.session_id.as_deref(), Some("b"));
    assert!(q.take_unannounced(4).is_empty());
}

#[test]
fn announcements_respect_the_limit_and_skip_usage() {
    let mut q = Queue::new(8, 0);
    q.show_usage();
    for s in ["a", "b", "c"] {
        q.push(claude(Kind::Error, s), 0, 5000);
    }
    assert_eq!(q.take_unannounced(2).len(), 2);
    assert_eq!(q.take_unannounced(2).len(), 1);
    assert!(q.take_unannounced(2).is_empty());
}

#[test]
fn time_spent_idle_never_counts_against_a_new_cards_display_time() {
    // Resident mode ticks every 15 s while idle (usage polling), so the first tick after a
    // card arrives sees a large gap since the previous tick.
    let mut q = Queue::new(8, 0);
    q.tick(0, false);
    q.tick(15_000, false);
    assert!(
        q.push(event(Kind::TurnComplete, "idle-gap"), 22_000, 3000)
            .is_queued()
    );
    q.tick(22_010, false);
    q.tick(25_009, false);
    assert_eq!(q.items().len(), 1, "card expired before its duration");
    q.tick(25_010, false);
    assert!(q.items().is_empty());
}
