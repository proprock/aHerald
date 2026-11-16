//! AH-305: a shown Claude Code permission or input card leaves when its session shows the
//! request was resolved, not only at `Stop`.
use aherald::{
    agent::Agent,
    events::{Event, Kind, ToolPhase, parse_hook},
    queue::{Outcome, Queue},
};

fn claude(json: &str) -> Event {
    Agent::Claude.parse_hook(json.as_bytes()).unwrap().unwrap()
}
fn named(name: &str, session: &str) -> Event {
    claude(&format!(
        r#"{{"hook_event_name":"{name}","session_id":"{session}"}}"#
    ))
}
fn tool(name: &str, session: &str, id: &str) -> Event {
    claude(&format!(
        r#"{{"hook_event_name":"{name}","session_id":"{session}","tool_use_id":"{id}"}}"#
    ))
}
fn child(name: &str, id: &str) -> Event {
    claude(&format!(
        r#"{{"hook_event_name":"{name}","session_id":"s","agent_id":"child","tool_use_id":"{id}"}}"#
    ))
}
fn prompt_notification(session: &str) -> Event {
    claude(&format!(
        r#"{{"hook_event_name":"Notification","session_id":"{session}","notification_type":"permission_prompt"}}"#
    ))
}
fn waits(queue: &Queue) -> usize {
    queue
        .items()
        .iter()
        .filter(|e| matches!(e.event.kind, Kind::PermissionRequired | Kind::InputRequired))
        .count()
}
/// A permission card of session `s` shown while its tool `a` is open.
fn card_with_open_tool() -> Queue {
    let mut queue = Queue::new(8, 0);
    queue.observe_activity(&tool("PreToolUse", "s", "a"));
    assert!(
        queue
            .push(named("PermissionRequest", "s"), 0, 0)
            .is_queued()
    );
    queue
}

#[test]
fn a_failed_tool_is_parsed_as_the_finish_of_its_call() {
    let event = tool("PostToolUseFailure", "s", "a");
    assert!(event.activity);
    assert_eq!(event.tool_phase, Some(ToolPhase::Finish));
    assert_eq!(event.tool_use_id.as_deref(), Some("a"));
}

#[test]
fn the_card_leaves_when_its_tool_finishes() {
    let mut queue = card_with_open_tool();
    assert_eq!(queue.observe_activity(&tool("PostToolUse", "s", "a")), 1);
    assert_eq!(waits(&queue), 0);
}

#[test]
fn the_card_leaves_when_its_tool_fails() {
    let mut queue = card_with_open_tool();
    queue.observe_activity(&tool("PostToolUseFailure", "s", "a"));
    assert_eq!(waits(&queue), 0);
}

#[test]
fn the_card_leaves_on_a_new_prompt() {
    let mut queue = card_with_open_tool();
    queue.observe_activity(&named("UserPromptSubmit", "s"));
    assert_eq!(waits(&queue), 0);
}

#[test]
fn an_input_card_leaves_when_the_tool_that_asked_finishes() {
    let mut queue = Queue::new(8, 0);
    queue.observe_activity(&tool("PreToolUse", "s", "mcp"));
    queue.push(
        claude(r#"{"hook_event_name":"Notification","session_id":"s","notification_type":"elicitation_dialog"}"#),
        0,
        0,
    );
    queue.observe_activity(&tool("PostToolUse", "s", "mcp"));
    assert_eq!(waits(&queue), 0);
}

#[test]
fn a_busy_stop_clears_the_card() {
    let mut queue = card_with_open_tool();
    // A Stop with background work reaches the queue as activity (`suppress_while_busy`).
    let mut stop = named("Stop", "s");
    stop.activity = true;
    queue.observe_activity(&stop);
    assert_eq!(waits(&queue), 0);
}

#[test]
fn a_parallel_tool_that_was_open_keeps_the_card_until_it_finishes_too() {
    let mut queue = Queue::new(8, 0);
    queue.observe_activity(&tool("PreToolUse", "s", "a"));
    queue.observe_activity(&tool("PreToolUse", "s", "b"));
    queue.push(named("PermissionRequest", "s"), 0, 0);
    queue.observe_activity(&tool("PostToolUse", "s", "b"));
    assert_eq!(waits(&queue), 1);
    queue.observe_activity(&tool("PostToolUse", "s", "a"));
    assert_eq!(waits(&queue), 0);
}

#[test]
fn a_tool_started_after_the_card_does_not_remove_it() {
    let mut queue = card_with_open_tool();
    queue.observe_activity(&tool("PreToolUse", "s", "late"));
    queue.observe_activity(&tool("PostToolUse", "s", "late"));
    assert_eq!(waits(&queue), 1);
}

#[test]
fn another_session_does_not_remove_the_card() {
    let mut queue = card_with_open_tool();
    queue.observe_activity(&tool("PostToolUse", "other", "a"));
    queue.observe_activity(&named("UserPromptSubmit", "other"));
    assert_eq!(waits(&queue), 1);
}

#[test]
fn a_subagent_tool_does_not_remove_the_main_card() {
    let mut queue = card_with_open_tool();
    queue.observe_activity(&child("PostToolUse", "a"));
    assert_eq!(waits(&queue), 1);
}

#[test]
fn a_main_session_tool_does_not_remove_a_subagent_card() {
    let mut queue = Queue::new(8, 0);
    queue.observe_activity(&child("PreToolUse", "a"));
    queue.push(child("PermissionRequest", "a"), 0, 0);
    queue.observe_activity(&tool("PostToolUse", "s", "a"));
    assert_eq!(waits(&queue), 1);
    queue.observe_activity(&child("PostToolUse", "a"));
    assert_eq!(waits(&queue), 0);
}

#[test]
fn a_claude_tool_does_not_remove_a_codex_card_of_the_same_session_id() {
    let codex = |json: &str| parse_hook(json.as_bytes()).unwrap().unwrap();
    let mut queue = Queue::new(8, 0);
    queue.observe_activity(&codex(
        r#"{"hook_event_name":"PreToolUse","session_id":"s","turn_id":"t","tool_use_id":"a"}"#,
    ));
    queue.push(
        codex(r#"{"hook_event_name":"PermissionRequest","session_id":"s","turn_id":"t"}"#),
        0,
        0,
    );
    queue.observe_activity(&tool("PostToolUse", "s", "a"));
    assert_eq!(waits(&queue), 1);
}

#[test]
fn a_permission_request_and_its_notification_are_one_card() {
    let mut queue = card_with_open_tool();
    assert_eq!(
        queue.push(prompt_notification("s"), 6_000, 0),
        Outcome::Duplicate
    );
    assert_eq!(waits(&queue), 1);
}

#[test]
fn the_notification_of_an_answered_prompt_shows_no_card() {
    let mut queue = card_with_open_tool();
    queue.observe_activity(&tool("PostToolUse", "s", "a"));
    assert_eq!(
        queue.push(prompt_notification("s"), 6_000, 0),
        Outcome::Duplicate
    );
    assert_eq!(waits(&queue), 0);
    assert!(queue.take_unannounced(8).is_empty());
}

#[test]
fn the_next_tool_call_asking_for_permission_shows_a_card() {
    let mut queue = card_with_open_tool();
    queue.observe_activity(&tool("PostToolUse", "s", "a"));
    queue.observe_activity(&tool("PreToolUse", "s", "b"));
    assert!(
        queue
            .push(named("PermissionRequest", "s"), 7_000, 0)
            .is_queued()
    );
}
