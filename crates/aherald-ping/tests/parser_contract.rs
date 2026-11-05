//! The sender decides in the guest which events leave it; the host parses what arrives with the
//! same rules as a local hook. When the two disagree an event is lost silently (the sender drops
//! what the host would show) or sent for nothing. This holds the sender's filter to the host's
//! parser for every event name either side knows.
use aherald::agent::Agent as Host;
use aherald_ping::{Agent, wanted};
use serde_json::{Value, json};
use std::{fs, path::Path};

/// Events the host's parser reads as lifecycle evidence only (`Event::activity`): a prompt or tool
/// use proves the session is working again, never a card. Both sides see the same stream in a
/// local install, but a guest forwards nothing for them, deliberately: every tool call would
/// otherwise cost a network round trip and leak the cadence of the session. Anything else on
/// which the two disagree is a bug.
const DROPPED_BY_THE_SENDER: [&str; 3] = ["UserPromptSubmit", "PreToolUse", "PostToolUse"];

const CLAUDE_EVENTS: [&str; 14] = [
    "Stop",
    "StopFailure",
    "PermissionRequest",
    "Notification",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "SubagentStop",
    "SessionStart",
    "PreCompact",
    "Interrupt",
    "TaskCompleted",
    "NotAnEvent",
];
const CODEX_EVENTS: [&str; 9] = [
    "Stop",
    "PermissionRequest",
    "Interrupt",
    "SessionEnd",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "SessionStart",
    "NotAnEvent",
];
const NOTIFICATION_TYPES: [&str; 6] = [
    "permission_prompt",
    "elicitation_dialog",
    "agent_needs_input",
    "idle_prompt",
    "auth_success",
    "",
];

/// Whether the host shows or counts the event, apart from the lifecycle evidence the sender
/// drops on purpose.
fn host_takes(agent: Host, payload: &Value) -> bool {
    let bytes = serde_json::to_vec(payload).unwrap();
    match agent.parse_hook(&bytes) {
        Ok(Some(event)) => !event.activity,
        Ok(None) => false,
        Err(error) => panic!("the host rejects {payload}: {error}"),
    }
}

fn payload(name: &str, notification_type: Option<&str>) -> Value {
    let mut value = json!({
        "hook_event_name": name,
        "session_id": "contract-session",
        "turn_id": "contract-turn",
        "cwd": "/work/project",
    });
    if let Some(kind) = notification_type {
        value["notification_type"] = kind.into();
    }
    value
}

fn assert_agree(host: Host, sender: Agent, payload: &Value) {
    assert_eq!(
        wanted(sender, payload),
        host_takes(host, payload),
        "{} and the sender disagree about {payload}",
        host.label()
    );
}

#[test]
fn the_sender_forwards_exactly_what_the_host_claude_parser_takes() {
    for name in CLAUDE_EVENTS {
        if name == "Notification" {
            for kind in NOTIFICATION_TYPES {
                assert_agree(Host::Claude, Agent::Claude, &payload(name, Some(kind)));
            }
            // A notification without a type is not a wait either.
            assert_agree(Host::Claude, Agent::Claude, &payload(name, None));
        } else {
            assert_agree(Host::Claude, Agent::Claude, &payload(name, None));
        }
    }
}

#[test]
fn the_sender_forwards_exactly_what_the_host_codex_parser_takes() {
    for name in CODEX_EVENTS {
        assert_agree(Host::Codex, Agent::Codex, &payload(name, None));
    }
}

#[test]
fn the_recorded_codex_payloads_agree_too() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures");
    let mut seen = 0;
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let Ok(value) = serde_json::from_slice::<Value>(&fs::read(&path).unwrap()) else {
            continue;
        };
        if value.get("hook_event_name").is_none() {
            continue;
        }
        assert_agree(Host::Codex, Agent::Codex, &value);
        seen += 1;
    }
    assert!(seen >= 4, "the hook fixtures moved: only {seen} found");
}

#[test]
fn the_events_the_sender_drops_on_purpose_are_the_ones_the_host_reads_as_activity() {
    // Pins the exception list above: it names events the host really parses as activity, so the
    // list cannot hide a disagreement about a real turn end or wait.
    for agent in [Host::Claude, Host::Codex] {
        for name in DROPPED_BY_THE_SENDER {
            let bytes = serde_json::to_vec(&payload(name, None)).unwrap();
            let event = agent.parse_hook(&bytes).unwrap().unwrap();
            assert!(event.activity, "{} {name}", agent.label());
        }
    }
    for (host, sender) in [(Host::Claude, Agent::Claude), (Host::Codex, Agent::Codex)] {
        for name in DROPPED_BY_THE_SENDER {
            assert!(!wanted(sender, &payload(name, None)), "{}", host.label());
        }
    }
}
