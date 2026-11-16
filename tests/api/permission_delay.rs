//! AH-304: a Codex permission request waits `[codex] permission_delay_sec` before it becomes a card,
//! and is dropped when evidence shows the request was answered within the delay.
use aherald::{
    agent::Agent,
    config::Config,
    events::{Event, Kind, parse_hook},
    focus::Outputs,
    ipc::{Request, Response},
    queue::{Outcome, Queue},
    resident_request::{Controls, Shared, respond, tick},
    session_warnings::Warnings,
};

const DELAY: u64 = 3_000;

fn hook(json: &str) -> Event {
    parse_hook(json.as_bytes()).unwrap().unwrap()
}
fn named(name: &str, session: &str, turn: &str) -> Event {
    hook(&format!(
        r#"{{"hook_event_name":"{name}","session_id":"{session}","turn_id":"{turn}"}}"#
    ))
}
fn tool(name: &str, session: &str, turn: &str, id: &str) -> Event {
    hook(&format!(
        r#"{{"hook_event_name":"{name}","session_id":"{session}","turn_id":"{turn}","tool_use_id":"{id}"}}"#
    ))
}
fn permission() -> Event {
    named("PermissionRequest", "s", "t")
}
fn hold(queue: &mut Queue, event: Event, now: u64) -> Outcome {
    queue.push_with_delay(event, now, 0, Outputs::default(), DELAY)
}
/// Feeds evidence the way the resident does: activity and terminal events both reach the queue.
fn observe(queue: &mut Queue, event: Event, now: u64) {
    if event.activity {
        queue.observe_activity(&event);
    } else {
        queue.push(event, now, 0);
    }
}
fn permission_cards(queue: &Queue) -> usize {
    queue
        .items()
        .iter()
        .filter(|e| e.event.kind == Kind::PermissionRequired)
        .count()
}
/// The request is released after the delay unless something resolved it.
fn released(queue: &mut Queue, now: u64) -> bool {
    !queue.release_due(now + DELAY).is_empty()
}

#[test]
fn a_request_approved_within_the_delay_never_becomes_a_card() {
    let mut queue = Queue::new(8, 0);
    observe(&mut queue, tool("PreToolUse", "s", "t", "a"), 0);
    assert_eq!(hold(&mut queue, permission(), 0), Outcome::Held);
    assert!(queue.items().is_empty() && queue.has_held());
    observe(&mut queue, tool("PostToolUse", "s", "t", "a"), 500);
    assert!(!queue.has_held());
    assert_eq!(queue.take_auto_resolved(), 1);
    assert!(!released(&mut queue, 0));
    assert_eq!(permission_cards(&queue), 0);
}

#[test]
fn a_request_held_past_the_delay_becomes_one_card() {
    let mut queue = Queue::new(8, 0);
    observe(&mut queue, tool("PreToolUse", "s", "t", "a"), 0);
    hold(&mut queue, permission(), 0);
    assert!(queue.release_due(DELAY - 1).is_empty());
    let out = queue.release_due(DELAY);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].kind, Kind::PermissionRequired);
    assert_eq!(permission_cards(&queue), 1);
    assert!(!queue.has_held());
    assert!(queue.release_due(DELAY * 2).is_empty());
    assert_eq!(queue.take_auto_resolved(), 0);
}

#[test]
fn a_tool_start_alone_is_not_evidence() {
    let mut queue = Queue::new(8, 0);
    observe(&mut queue, tool("PreToolUse", "s", "t", "a"), 0);
    hold(&mut queue, permission(), 0);
    observe(&mut queue, tool("PreToolUse", "s", "t", "b"), 100);
    assert!(released(&mut queue, 0));
}

#[test]
fn with_two_open_tools_both_must_finish() {
    let mut queue = Queue::new(8, 0);
    observe(&mut queue, tool("PreToolUse", "s", "t", "a"), 0);
    observe(&mut queue, tool("PreToolUse", "s", "t", "b"), 0);
    hold(&mut queue, permission(), 0);
    observe(&mut queue, tool("PostToolUse", "s", "t", "a"), 100);
    assert!(queue.has_held());
    observe(&mut queue, tool("PostToolUse", "s", "t", "b"), 200);
    assert!(!queue.has_held());
}

#[test]
fn a_tool_that_started_after_the_request_does_not_resolve_it() {
    let mut queue = Queue::new(8, 0);
    hold(&mut queue, permission(), 0);
    observe(&mut queue, tool("PreToolUse", "s", "t", "late"), 100);
    observe(&mut queue, tool("PostToolUse", "s", "t", "late"), 200);
    assert!(released(&mut queue, 0));
}

#[test]
fn prompt_stop_interrupt_and_session_end_resolve_the_request() {
    for evidence in [
        named("UserPromptSubmit", "s", "t"),
        named("Stop", "s", "t"),
        named("Interrupt", "s", "t"),
        // A session end covers every turn of the session.
        named("SessionEnd", "s", "other"),
    ] {
        let mut queue = Queue::new(8, 0);
        hold(&mut queue, permission(), 0);
        observe(&mut queue, evidence.clone(), 100);
        assert!(!queue.has_held(), "{evidence:?}");
        assert!(!released(&mut queue, 0), "{evidence:?}");
        assert_eq!(permission_cards(&queue), 0, "{evidence:?}");
    }
}

#[test]
fn another_session_or_turn_does_not_resolve_the_request() {
    for evidence in [
        tool("PostToolUse", "s", "other", "a"),
        tool("PostToolUse", "other", "t", "a"),
        named("UserPromptSubmit", "other", "t"),
        named("Stop", "s", "other"),
        named("Interrupt", "other", "t"),
        named("SessionEnd", "other", "t"),
    ] {
        let mut queue = Queue::new(8, 0);
        observe(&mut queue, tool("PreToolUse", "s", "t", "a"), 0);
        hold(&mut queue, permission(), 0);
        observe(&mut queue, evidence.clone(), 100);
        assert!(released(&mut queue, 0), "{evidence:?}");
    }
}

#[test]
fn a_subagent_event_does_not_resolve_the_parent_request() {
    let sub = |name: &str| {
        hook(&format!(
            r#"{{"hook_event_name":"{name}","session_id":"s","turn_id":"t","agent_id":"child","tool_use_id":"a"}}"#
        ))
    };
    for evidence in [sub("PostToolUse"), sub("Stop"), sub("UserPromptSubmit")] {
        let mut queue = Queue::new(8, 0);
        observe(&mut queue, tool("PreToolUse", "s", "t", "a"), 0);
        hold(&mut queue, permission(), 0);
        observe(&mut queue, evidence.clone(), 100);
        assert!(released(&mut queue, 0), "{evidence:?}");
    }
}

/// A denied call never finishes; the next model step must forget it, or every later request of
/// the turn would wait for it.
#[test]
fn a_denied_call_does_not_keep_later_requests_of_the_turn_waiting() {
    let mut queue = Queue::new(8, 0);
    observe(&mut queue, tool("PreToolUse", "s", "t", "denied"), 0);
    hold(&mut queue, permission(), 0);
    // The completed model request after the denial (same shape as a UserPromptSubmit here).
    observe(&mut queue, named("UserPromptSubmit", "s", "t"), 100);
    assert!(!queue.has_held());
    observe(&mut queue, tool("PreToolUse", "s", "t", "b"), 200);
    hold(&mut queue, permission(), 200);
    observe(&mut queue, tool("PostToolUse", "s", "t", "b"), 300);
    assert!(!queue.has_held());
}

/// A hook from before `tool_phase` sends tool activity without it; that is not proof of an answer.
#[test]
fn tool_activity_without_a_phase_does_not_resolve_a_request() {
    let mut queue = Queue::new(8, 0);
    observe(&mut queue, tool("PreToolUse", "s", "t", "a"), 0);
    hold(&mut queue, permission(), 0);
    let mut old = tool("PreToolUse", "s", "t", "b");
    old.tool_phase = None;
    observe(&mut queue, old, 100);
    assert!(released(&mut queue, 0));
}

#[test]
fn a_zero_delay_queues_the_request_at_once() {
    let mut queue = Queue::new(8, 0);
    let outcome = queue.push_with_delay(permission(), 0, 0, Outputs::default(), 0);
    assert_eq!(outcome, Outcome::Queued);
    assert_eq!(permission_cards(&queue), 1);
    assert!(!queue.has_held());
}

#[test]
fn claude_requests_and_requests_without_a_turn_are_not_held() {
    let claude = Agent::Claude
        .parse_hook(br#"{"hook_event_name":"PermissionRequest","session_id":"s"}"#)
        .unwrap()
        .unwrap();
    let turnless = hook(r#"{"hook_event_name":"PermissionRequest","session_id":"s"}"#);
    for event in [claude, turnless] {
        let mut queue = Queue::new(8, 0);
        assert_eq!(hold(&mut queue, event, 0), Outcome::Queued);
        assert!(!queue.has_held());
    }
}

#[test]
fn a_request_of_a_turn_already_interrupted_is_obsolete_not_held() {
    let mut queue = Queue::new(8, 0);
    observe(&mut queue, named("Interrupt", "s", "t"), 0);
    assert_eq!(hold(&mut queue, permission(), 0), Outcome::Obsolete);
    assert!(!queue.has_held());
}

#[test]
fn the_open_tool_set_keeps_the_newest_32_ids_of_a_turn() {
    let mut queue = Queue::new(8, 0);
    for i in 0..33 {
        observe(
            &mut queue,
            tool("PreToolUse", "s", "t", &format!("id{i}")),
            0,
        );
    }
    hold(&mut queue, permission(), 0);
    // id0 was dropped, so the 32 newest finishing is enough.
    for i in 1..33 {
        observe(
            &mut queue,
            tool("PostToolUse", "s", "t", &format!("id{i}")),
            10,
        );
    }
    assert!(!queue.has_held());
}

#[test]
fn open_tools_are_kept_for_the_newest_64_turns() {
    let mut queue = Queue::new(8, 0);
    for turn in 0..65 {
        observe(
            &mut queue,
            tool("PreToolUse", "s", &format!("t{turn}"), "a"),
            0,
        );
    }
    // Turn t0 was dropped: its request sees no open tool, so a finish is no evidence.
    hold(&mut queue, named("PermissionRequest", "s", "t0"), 0);
    observe(&mut queue, tool("PostToolUse", "s", "t0", "a"), 10);
    assert!(released(&mut queue, 0));

    // Turn t1 is still known.
    let mut queue = Queue::new(8, 0);
    for turn in 0..65 {
        observe(
            &mut queue,
            tool("PreToolUse", "s", &format!("t{turn}"), "a"),
            0,
        );
    }
    hold(&mut queue, named("PermissionRequest", "s", "t1"), 0);
    observe(&mut queue, tool("PostToolUse", "s", "t1", "a"), 10);
    assert!(!queue.has_held());
}

#[test]
fn at_most_64_requests_are_held_and_the_next_is_queued() {
    let mut queue = Queue::new(64, 0);
    for turn in 0..64 {
        let outcome = hold(
            &mut queue,
            named("PermissionRequest", "s", &format!("t{turn}")),
            0,
        );
        assert_eq!(outcome, Outcome::Held);
    }
    let outcome = hold(&mut queue, named("PermissionRequest", "s", "t64"), 0);
    assert_eq!(outcome, Outcome::Queued);
    assert_eq!(permission_cards(&queue), 1);
    // The queue holds 64 cards, so one released request finds it full.
    assert_eq!(queue.release_due(DELAY).len(), 63);
    assert!(!queue.has_held());
}

// The resident: release, sound and messenger forward.

struct State {
    queue: Queue,
    warnings: Warnings,
    paused: bool,
    forwarded: Vec<Event>,
}
impl Controls for State {
    fn queue(&mut self) -> &mut Queue {
        &mut self.queue
    }
    fn warnings(&mut self) -> &mut Warnings {
        &mut self.warnings
    }
    fn paused(&self) -> bool {
        self.paused
    }
    fn sounds(&self) -> bool {
        true
    }
    fn visuals(&self) -> bool {
        true
    }
    fn set_switches(&mut self, paused: bool, _: bool, _: bool) {
        self.paused = paused;
    }
    fn forward(&mut self, event: &Event) {
        self.forwarded.push(event.clone());
    }
}
fn resident() -> State {
    State {
        queue: Queue::new(8, 0),
        warnings: Warnings::default(),
        paused: false,
        forwarded: Vec::new(),
    }
}
fn send(state: &mut State, settings: &Config, event: Event, now: u64) -> Response {
    respond(Request::Event { event }, state, settings, now).0
}

#[test]
fn the_built_in_delay_is_three_seconds() {
    assert_eq!(Config::default().codex.permission_delay_sec, 3);
}

#[test]
fn a_human_wait_is_shown_sounded_and_forwarded_once_after_the_delay() {
    let settings = Config::default();
    let mut state = resident();
    send(&mut state, &settings, tool("PreToolUse", "s", "t", "a"), 0);
    assert!(matches!(
        send(&mut state, &settings, permission(), 0),
        Response::Accepted
    ));
    assert!(state.queue.items().is_empty() && state.forwarded.is_empty());
    tick(&mut state, &settings, DELAY - 1);
    assert!(state.queue.items().is_empty() && state.forwarded.is_empty());
    tick(&mut state, &settings, DELAY);
    assert_eq!(permission_cards(&state.queue), 1);
    assert_eq!(state.forwarded.len(), 1);
    assert_eq!(state.queue.take_unannounced(8).len(), 1);
    tick(&mut state, &settings, DELAY * 3);
    assert_eq!(state.forwarded.len(), 1);
    assert!(state.queue.take_unannounced(8).is_empty());
}

#[test]
fn a_paused_resident_still_releases_a_due_request() {
    let settings = Config::default();
    let mut state = resident();
    state.paused = true;
    send(&mut state, &settings, permission(), 0);
    tick(&mut state, &settings, DELAY);
    assert_eq!(permission_cards(&state.queue), 1);
}

#[test]
fn a_denied_request_is_resolved_by_the_next_model_request() {
    let settings = Config::default();
    let mut state = resident();
    send(&mut state, &settings, tool("PreToolUse", "s", "t", "a"), 0);
    send(&mut state, &settings, permission(), 0);
    let progress = Request::CodexProgress {
        session_id: "s".into(),
        turn_id: Some("t".into()),
        agent_id: None,
    };
    assert!(matches!(
        respond(progress, &mut state, &settings, 500).0,
        Response::Accepted
    ));
    tick(&mut state, &settings, DELAY);
    assert!(state.queue.items().is_empty() && state.forwarded.is_empty());
}

#[test]
fn a_shown_card_leaves_on_the_next_model_request() {
    let mut settings = Config::default();
    settings.codex.permission_delay_sec = 0;
    let mut state = resident();
    send(&mut state, &settings, tool("PreToolUse", "s", "t", "a"), 0);
    send(&mut state, &settings, permission(), 0);
    assert_eq!(permission_cards(&state.queue), 1);
    let progress = Request::CodexProgress {
        session_id: "s".into(),
        turn_id: Some("t".into()),
        agent_id: None,
    };
    respond(progress, &mut state, &settings, 500);
    assert_eq!(permission_cards(&state.queue), 0);
}

#[test]
fn a_released_card_waits_for_the_tools_open_when_the_request_arrived() {
    let mut queue = Queue::new(8, 0);
    observe(&mut queue, tool("PreToolUse", "s", "t", "a"), 0);
    hold(&mut queue, permission(), 0);
    observe(&mut queue, tool("PreToolUse", "s", "t", "late"), 100);
    assert!(released(&mut queue, 0));
    observe(&mut queue, tool("PostToolUse", "s", "t", "a"), DELAY + 100);
    assert_eq!(permission_cards(&queue), 0);
}

#[test]
fn a_released_card_ignores_a_tool_that_started_during_the_delay() {
    let mut queue = Queue::new(8, 0);
    hold(&mut queue, permission(), 0);
    observe(&mut queue, tool("PreToolUse", "s", "t", "late"), 100);
    assert!(released(&mut queue, 0));
    observe(
        &mut queue,
        tool("PostToolUse", "s", "t", "late"),
        DELAY + 100,
    );
    assert_eq!(permission_cards(&queue), 1);
}

#[test]
fn the_resident_configured_without_a_delay_shows_the_card_at_once() {
    let mut settings = Config::default();
    settings.codex.permission_delay_sec = 0;
    let mut state = resident();
    send(&mut state, &settings, permission(), 0);
    assert_eq!(permission_cards(&state.queue), 1);
    assert_eq!(state.forwarded.len(), 1);
}

#[test]
fn a_held_request_is_a_deadline_the_resident_wakes_for() {
    let settings = Config::default();
    let mut shared = Shared::new(&settings, ());
    assert!(!shared.has_deadline());
    respond(
        Request::Event {
            event: permission(),
        },
        &mut shared,
        &settings,
        0,
    );
    assert!(shared.has_deadline());
}
