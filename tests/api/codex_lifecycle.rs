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
fn tool(name: &str, session: &str, turn: &str, id: &str) -> Event {
    parse_hook(
        format!(
            r#"{{"hook_event_name":"{name}","session_id":"{session}","turn_id":"{turn}","tool_use_id":"{id}"}}"#
        )
        .as_bytes(),
    )
    .unwrap()
    .unwrap()
}
fn permission_cards(queue: &Queue) -> usize {
    queue
        .items()
        .iter()
        .filter(|e| e.event.kind == Kind::PermissionRequired)
        .count()
}
/// A permission card shown while tool `a` of session `s`, turn `t` is open (AH-305).
fn card_with_open_tool() -> Queue {
    let mut queue = Queue::new(8, 0);
    queue.observe_activity(&tool("PreToolUse", "s", "t", "a"));
    assert!(
        queue
            .push(hook("PermissionRequest", "s", "t"), 0, 0)
            .is_queued()
    );
    queue
}

#[test]
fn a_shown_card_leaves_when_its_open_tool_finishes() {
    let mut queue = card_with_open_tool();
    assert_eq!(
        queue.observe_activity(&tool("PostToolUse", "s", "t", "a")),
        1
    );
    assert_eq!(permission_cards(&queue), 0);
}

#[test]
fn a_shown_card_leaves_on_a_prompt_of_its_turn() {
    let mut queue = card_with_open_tool();
    queue.observe_activity(&hook("UserPromptSubmit", "s", "t"));
    assert_eq!(permission_cards(&queue), 0);
}

#[test]
fn a_shown_card_stays_until_every_tool_open_on_arrival_finishes() {
    let mut queue = Queue::new(8, 0);
    queue.observe_activity(&tool("PreToolUse", "s", "t", "a"));
    queue.observe_activity(&tool("PreToolUse", "s", "t", "b"));
    queue.push(hook("PermissionRequest", "s", "t"), 0, 0);
    queue.observe_activity(&tool("PostToolUse", "s", "t", "a"));
    assert_eq!(permission_cards(&queue), 1);
    queue.observe_activity(&tool("PostToolUse", "s", "t", "b"));
    assert_eq!(permission_cards(&queue), 0);
}

#[test]
fn a_tool_started_after_the_card_does_not_remove_it() {
    let mut queue = card_with_open_tool();
    queue.observe_activity(&tool("PreToolUse", "s", "t", "late"));
    queue.observe_activity(&tool("PostToolUse", "s", "t", "late"));
    assert_eq!(permission_cards(&queue), 1);
}

#[test]
fn another_turn_does_not_remove_the_card() {
    let mut queue = card_with_open_tool();
    queue.observe_activity(&tool("PostToolUse", "s", "other", "a"));
    queue.observe_activity(&hook("UserPromptSubmit", "s", "other"));
    assert_eq!(permission_cards(&queue), 1);
}

#[test]
fn another_session_does_not_remove_the_card() {
    let mut queue = card_with_open_tool();
    queue.observe_activity(&tool("PostToolUse", "other", "t", "a"));
    queue.observe_activity(&hook("UserPromptSubmit", "other", "t"));
    assert_eq!(permission_cards(&queue), 1);
}

#[test]
fn a_subagent_does_not_remove_the_parent_card() {
    let mut queue = card_with_open_tool();
    let child = |name: &str| {
        parse_hook(
            format!(r#"{{"hook_event_name":"{name}","session_id":"s","turn_id":"t","agent_id":"child","tool_use_id":"a"}}"#)
                .as_bytes(),
        )
        .unwrap()
        .unwrap()
    };
    queue.observe_activity(&child("PostToolUse"));
    assert_eq!(permission_cards(&queue), 1);
}

#[test]
fn a_resolved_selected_card_passes_the_selection_to_the_next_visible_card() {
    let mut queue = card_with_open_tool();
    queue.push(hook("PermissionRequest", "s2", "t"), 1, 0);
    let first = queue.items()[0].id;
    assert_eq!(queue.selected().unwrap().id, first);
    queue.observe_activity(&tool("PostToolUse", "s", "t", "a"));
    let next = queue.selected().expect("the other card is selected");
    assert_eq!(next.event.session_id.as_deref(), Some("s2"));
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
fn requests_without_request_identity_are_not_merged() {
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
