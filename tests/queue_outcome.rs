//! `Queue::push` reports why an event was or was not queued, so the IPC listener does
//! not have to repeat the queue's own terminal, duplicate and obsolete checks.
use aherald::{
    events::{Event, Kind, parse_hook},
    queue::{Outcome, Queue},
};

fn hook(name: &str, session: &str, turn: &str) -> Event {
    parse_hook(
        format!(r#"{{"hook_event_name":"{name}","session_id":"{session}","turn_id":"{turn}"}}"#)
            .as_bytes(),
    )
    .unwrap()
    .unwrap()
}

#[test]
fn a_new_event_is_queued() {
    let mut queue = Queue::new(8, 1000);
    let outcome = queue.push(hook("PermissionRequest", "one", "turn-a"), 0, 0);
    assert_eq!(outcome, Outcome::Queued);
    assert!(outcome.is_queued() && outcome.is_accepted());
}

#[test]
fn a_repeat_inside_the_window_is_a_duplicate_that_is_still_accepted() {
    let mut queue = Queue::new(8, 1000);
    let mut wait = hook("PermissionRequest", "one", "turn-a");
    wait.request_id = Some("request-a".into());
    let _ = queue.push(wait.clone(), 0, 0);
    let outcome = queue.push(wait, 1, 0);
    assert_eq!(outcome, Outcome::Duplicate);
    assert!(!outcome.is_queued() && outcome.is_accepted());
    assert_eq!(queue.items().len(), 1);
}

#[test]
fn a_wait_delivered_after_its_turn_was_interrupted_is_obsolete() {
    let mut queue = Queue::new(8, 1000);
    let _ = queue.push(hook("Interrupt", "one", "turn-a"), 0, 3000);
    let outcome = queue.push(hook("PermissionRequest", "one", "turn-a"), 1, 0);
    assert_eq!(outcome, Outcome::Obsolete);
    assert!(outcome.is_accepted());
}

#[test]
fn a_repeated_terminal_event_that_cleared_a_late_wait_is_resolved() {
    let mut queue = Queue::new(8, 1000);
    let _ = queue.push(hook("Stop", "one", "turn-a"), 0, 3000);
    // A wait of the finished turn is delivered late, then the Stop is delivered again.
    assert!(
        queue
            .push(hook("PermissionRequest", "one", "turn-a"), 1, 0)
            .is_queued()
    );
    let outcome = queue.push(hook("Stop", "one", "turn-a"), 2, 3000);
    assert_eq!(outcome, Outcome::Resolved);
    assert!(outcome.is_accepted() && !outcome.is_queued());
}

#[test]
fn a_flood_of_lower_priority_events_is_full_and_rejected() {
    let mut queue = Queue::new(2, 1000);
    let _ = queue.push(hook("PermissionRequest", "one", "turn-a"), 0, 0);
    let _ = queue.push(hook("PermissionRequest", "two", "turn-b"), 0, 0);
    let outcome = queue.push(hook("Stop", "three", "turn-c"), 1, 3000);
    assert_eq!(outcome, Outcome::Full);
    assert!(!outcome.is_accepted());
}

#[test]
fn an_event_with_an_invalid_identity_is_invalid_and_rejected() {
    let mut queue = Queue::new(8, 1000);
    let mut event = Event::demo(Kind::PermissionRequired.name()).unwrap();
    event.session_id = Some("bad\nid".into());
    let outcome = queue.push(event, 0, 0);
    assert_eq!(outcome, Outcome::Invalid);
    assert!(!outcome.is_accepted());
    assert!(queue.items().is_empty());
}
