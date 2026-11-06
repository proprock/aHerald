use aherald::{
    events::{Event, Kind, Source, parse_hook},
    queue::Queue,
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
fn terminal_evidence_clears_only_matching_turn_waits_and_suppresses_late_delivery() {
    for terminal in ["Stop", "Interrupt"] {
        let mut queue = Queue::new(8, 100);
        let wait = hook("PermissionRequest", "one", "turn-a");
        queue.push(wait.clone(), 0, 0);
        queue.push(hook("PermissionRequest", "two", "turn-a"), 1, 0);
        queue.push(hook("PermissionRequest", "one", "turn-b"), 2, 0);
        let mut input = wait.clone();
        input.kind = Kind::InputRequired;
        input.request_id = Some("input-request".into());
        queue.push(input, 3, 0);
        queue.push(hook(terminal, "one", "turn-a"), 4, 3000);
        assert_eq!(queue.items().len(), 3);
        assert!(
            !queue
                .items()
                .iter()
                .any(|e| e.event.session_id.as_deref() == Some("one")
                    && e.event.turn_id.as_deref() == Some("turn-a")
                    && matches!(e.event.kind, Kind::PermissionRequired | Kind::InputRequired))
        );
        if terminal == "Interrupt" {
            assert!(
                !queue.push(wait, 1000, 0).is_queued(),
                "late wait must not reopen an interrupted turn"
            );
        }
    }
}

#[test]
fn ambiguous_or_unrelated_terminal_evidence_does_not_clear_pending_cards() {
    let wait = hook("PermissionRequest", "one", "turn-a");
    let mut missing_turn = hook("Stop", "one", "turn-a");
    missing_turn.turn_id = None;
    let mut missing_session = hook("Stop", "one", "turn-a");
    missing_session.session_id = None;
    for terminal in [missing_turn, missing_session] {
        let mut queue = Queue::new(8, 100);
        queue.push(wait.clone(), 0, 0);
        queue.push(terminal, 1, 3000);
        assert!(queue.items().iter().any(|e| e.event == wait));
    }
    let mut queue = Queue::new(8, 100);
    queue.push(wait.clone(), 0, 0);
    let mut demo = hook("Stop", "one", "turn-a");
    demo.source = Source::Demo;
    queue.push(demo, 1, 3000);
    assert!(queue.items().iter().any(|e| e.event == wait));
    assert!(
        parse_hook(br#"{"hook_event_name":"PostToolUse","tool_response":{"exit_code":1}}"#)
            .unwrap()
            .is_none()
    );
    queue.dismiss();
    assert!(!queue.items().iter().any(|e| e.event == wait));
}

#[test]
fn parent_terminal_does_not_clear_subagent_waits() {
    let mut queue = Queue::new(8, 1000);
    let wait = parse_hook(br#"{"hook_event_name":"PermissionRequest","session_id":"one","turn_id":"turn-a","agent_id":"child-a"}"#).unwrap().unwrap();
    queue.push(wait.clone(), 0, 0);
    queue.push(hook("Stop", "one", "turn-a"), 1, 1000);
    assert!(queue.items().iter().any(|e| e.event == wait));
}

#[test]
fn requests_without_request_identity_are_not_merged_or_resolved_by_tool_results() {
    let mut queue = Queue::new(8, 1000);
    let wait = hook("PermissionRequest", "one", "turn-a");
    assert!(queue.push(wait.clone(), 0, 0).is_queued());
    assert!(
        queue.push(wait, 1, 0).is_queued(),
        "two hook boundaries may be different requests"
    );
    assert_eq!(queue.items().len(), 2);
}

#[test]
fn source_and_request_identity_isolation_and_duplicate_delivery() {
    let mut queue = Queue::new(8, 1000);
    let mut wait = hook("PermissionRequest", "one", "turn-a");
    wait.request_id = Some("request-a".into());
    assert!(queue.push(wait.clone(), 0, 0).is_queued());
    assert!(!queue.push(wait.clone(), 1, 0).is_queued());
    wait.request_id = Some("request-b".into());
    assert!(queue.push(wait.clone(), 2, 0).is_queued());
    wait.source = Source::Demo;
    assert!(queue.push(wait, 3, 0).is_queued());
    assert_eq!(queue.items().len(), 3);
}

#[test]
fn terminal_processing_is_independent_of_admission_and_preserves_selection() {
    let mut queue = Queue::new(8, 1000);
    queue.push(hook("PermissionRequest", "one", "turn-a"), 0, 0);
    queue.push(hook("PermissionRequest", "two", "turn-a"), 1, 0);
    queue.next(1);
    let selected = queue.selected().unwrap().id;
    // This path also handles disabled terminal presentations and paused residents.
    assert_eq!(queue.observe_terminal(&hook("Stop", "one", "turn-a")), 1);
    assert_eq!(queue.selected().unwrap().id, selected);
    assert_eq!(queue.observe_terminal(&hook("Stop", "one", "turn-a")), 0);
    assert_eq!(queue.items().len(), 1);
    assert_eq!(
        queue.observe_terminal(&hook("Interrupt", "two", "turn-a")),
        1
    );
    assert!(queue.selected().is_none());
    for turn in 0..200 {
        queue.observe_terminal(&hook("Interrupt", "flood", &format!("turn-{turn}")));
    }
    assert!(
        !queue
            .push(hook("PermissionRequest", "flood", "turn-199"), 300, 0)
            .is_queued()
    );
    // History is intentionally bounded to the most recent 128 turns.
    assert!(
        queue
            .push(hook("PermissionRequest", "flood", "turn-0"), 301, 0)
            .is_queued()
    );
}

#[test]
fn stop_boundary_does_not_hide_a_new_wait_if_another_hook_continues_the_turn() {
    let mut queue = Queue::new(8, 1000);
    queue.push(hook("Stop", "one", "turn-a"), 0, 3000);
    assert!(
        queue
            .push(hook("PermissionRequest", "one", "turn-a"), 1, 0)
            .is_queued()
    );
}
