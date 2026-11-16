//! AH-303: a tool call's id is kept on Pre/PostToolUse activity, validated, and survives the wire.
use aherald::{
    agent::Agent,
    ipc::{self, Request},
};
use serde_json::Value;

fn parse(agent: Agent, payload: &str) -> Result<Option<aherald::events::Event>, String> {
    agent.parse_hook(payload.as_bytes())
}

fn tool_use_id(event: &aherald::events::Event) -> Value {
    serde_json::to_value(event).unwrap()["tool_use_id"].clone()
}

#[test]
fn tool_activity_of_both_agents_carries_its_tool_use_id() {
    for agent in [Agent::Codex, Agent::Claude] {
        for name in ["PreToolUse", "PostToolUse"] {
            let payload = format!(
                r#"{{"hook_event_name":"{name}","session_id":"s","turn_id":"t","tool_use_id":"call-1"}}"#
            );
            let event = parse(agent, &payload).unwrap().unwrap();
            assert!(event.activity, "{agent:?} {name}");
            assert_eq!(tool_use_id(&event), "call-1", "{agent:?} {name}");
        }
    }
}

#[test]
fn the_tool_use_id_survives_the_resident_wire() {
    let event = parse(
        Agent::Codex,
        r#"{"hook_event_name":"PostToolUse","session_id":"s","turn_id":"t","tool_use_id":"call-1"}"#,
    )
    .unwrap()
    .unwrap();
    let frame = ipc::encode(&Request::Event { event }).unwrap();
    let Request::Event { event } = ipc::decode::<Request>(&frame).unwrap() else {
        panic!("not an event");
    };
    assert_eq!(tool_use_id(&event), "call-1");
}

#[test]
fn a_hostile_tool_use_id_rejects_the_hook() {
    let long = "x".repeat(257);
    for id in ["", "bell\\u0007", "line\\nbreak", long.as_str()] {
        let payload = format!(
            r#"{{"hook_event_name":"PreToolUse","session_id":"s","turn_id":"t","tool_use_id":"{id}"}}"#
        );
        for agent in [Agent::Codex, Agent::Claude] {
            assert!(parse(agent, &payload).is_err(), "{agent:?} accepted {id:?}");
        }
    }
}

#[test]
fn only_tool_activity_keeps_a_tool_use_id() {
    for name in ["PermissionRequest", "Stop", "UserPromptSubmit"] {
        let payload = format!(
            r#"{{"hook_event_name":"{name}","session_id":"s","turn_id":"t","tool_use_id":"call-1"}}"#
        );
        let event = parse(Agent::Codex, &payload).unwrap().unwrap();
        assert_eq!(tool_use_id(&event), Value::Null, "{name}");
    }
}

#[test]
fn tool_input_never_reaches_the_event() {
    let payload = r#"{"hook_event_name":"PreToolUse","session_id":"s","turn_id":"t","tool_use_id":"call-1","tool_name":"shell","tool_input":{"command":"echo SECRET"},"tool_response":"SECRET"}"#;
    for agent in [Agent::Codex, Agent::Claude] {
        let event = parse(agent, payload).unwrap().unwrap();
        assert!(!serde_json::to_string(&event).unwrap().contains("SECRET"));
    }
}
