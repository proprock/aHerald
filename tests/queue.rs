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
    assert!(queue.push(event(Kind::PermissionRequired, "recovered"), 70_001, 0));
    assert_eq!(queue.items().len(), 1);
}
#[test]
fn priority_fifo_and_overflow_preserve_unrelated_sessions() {
    let mut q = Queue::new(3, 1000);
    assert!(q.push(event(Kind::Interrupted, "low"), 0, 3000));
    assert!(q.push(event(Kind::Error, "medium"), 0, 3000));
    assert!(q.push(event(Kind::PermissionRequired, "high-1"), 0, 0));
    assert!(q.push(event(Kind::InputRequired, "high-2"), 0, 0));
    assert_eq!(
        q.items()
            .iter()
            .map(|e| e.event.session_id.as_deref().unwrap())
            .collect::<Vec<_>>(),
        ["high-1", "high-2", "medium"]
    );
    assert!(!q.push(event(Kind::Interrupted, "new-low"), 1, 3000));
    assert!(!q.push(event(Kind::PermissionRequired, "high-1"), 2, 0));
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
    assert!(q.push(e.clone(), 0, 0));
    q.dismiss();
    assert!(!q.push(e.clone(), 50, 0));
    e.request_id = Some("different".into());
    assert!(q.push(e.clone(), 60, 0));
    e.session_id = Some("another-session".into());
    assert!(q.push(e.clone(), 70, 0));
    assert!(q.push(event(Kind::PermissionRequired, "same"), 200, 0));
    let mut unknown = Event::demo("error").unwrap();
    unknown.source = Source::CodexHook;
    let mut q = Queue::new(4, 100);
    assert!(q.push(unknown.clone(), 0, 0));
    assert!(q.push(unknown, 1, 0));
}

#[test]
fn usage_is_one_persistent_queue_entry_and_dismissal_selects_next_message() {
    let mut q = aherald::queue::Queue::new(2, 5000);
    q.show_usage();
    q.show_usage();
    assert_eq!(q.items().len(), 1);
    assert!(q.selected().unwrap().is_usage());
    assert!(q.push(
        aherald::events::Event::demo("permission_required").unwrap(),
        0,
        1000
    ));
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
