//! AH-099: permission mode is not evidence of reviewer identity or resolution.
use aherald::{
    agent::Agent,
    events::{Kind, Source},
    queue::Queue,
};

#[test]
fn permission_modes_preserve_unresolved_notifications_and_silent_replies() {
    for mode in [
        None,
        Some("default"),
        Some("acceptEdits"),
        Some("plan"),
        Some("dontAsk"),
        Some("bypassPermissions"),
        Some("future-mode"),
    ] {
        let mut payload: serde_json::Value =
            serde_json::from_slice(include_bytes!("../fixtures/permission-request.json")).unwrap();
        if let Some(mode) = mode {
            payload["permission_mode"] = mode.into();
        } else {
            payload.as_object_mut().unwrap().remove("permission_mode");
        }
        let event = Agent::Codex
            .parse_hook(&serde_json::to_vec(&payload).unwrap())
            .unwrap()
            .unwrap();
        assert_eq!(event.kind, Kind::PermissionRequired, "mode {mode:?}");
        assert_eq!(event.source, Source::CodexHook);
        assert_eq!(event.session_id.as_deref(), Some("fixture-session"));
        assert_eq!(event.turn_id.as_deref(), Some("fixture-turn"));
        assert_eq!(event.request_id, None);
        assert_eq!(Agent::Codex.hook_reply(&event), None);
        let mut queue = Queue::new(8, 1000);
        assert!(queue.push(event.clone(), 0, 0).is_queued());
        assert_eq!(queue.selected().unwrap().event, event);
    }
}
