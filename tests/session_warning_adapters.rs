use aherald::{agent::Agent, events::Kind};

#[test]
fn matching_release_user_submission_fixture_is_activity_not_completion() {
    let event = Agent::Codex
        .parse_hook(include_bytes!("fixtures/codex-user-prompt-submit.json"))
        .unwrap()
        .unwrap();
    assert!(event.activity);
    assert_eq!(event.session_id.as_deref(), Some("synthetic-session"));
    assert_eq!(Agent::Codex.hook_reply(&event), None);
    assert!(
        !serde_json::to_string(&event)
            .unwrap()
            .contains("Sanitized example")
    );
}

#[test]
fn activity_hooks_keep_only_identity_and_never_reply_or_end_a_turn() {
    for agent in Agent::ALL {
        for name in ["UserPromptSubmit", "PreToolUse", "PostToolUse"] {
            let input = format!(
                r#"{{"hook_event_name":"{name}","session_id":"s","turn_id":"t","prompt":"SECRET","tool_input":{{"command":"SECRET"}}}}"#
            );
            let event = agent
                .parse_hook(input.as_bytes())
                .unwrap()
                .expect("activity evidence");
            assert!(event.activity);
            assert!(!event.is_terminal_evidence());
            assert_eq!(agent.hook_reply(&event), None);
            assert!(!serde_json::to_string(&event).unwrap().contains("SECRET"));
        }
    }
}

#[test]
fn codex_end_is_evidence_only_and_subagent_identity_is_preserved() {
    let end = Agent::Codex
        .parse_hook(include_bytes!("fixtures/codex-session-end.json"))
        .unwrap()
        .unwrap();
    assert!(end.ended);
    assert_eq!(Agent::Codex.hook_reply(&end), None);
    let child = Agent::Codex
        .parse_hook(
            br#"{"hook_event_name":"Stop","session_id":"s","turn_id":"t","agent_id":"child"}"#,
        )
        .unwrap()
        .unwrap();
    assert_eq!(child.agent_id.as_deref(), Some("child"));
}

#[test]
fn context_report_counts_input_including_cache_once_and_handles_unknown() {
    for (usage, expected) in [
        (
            r#"{"input_tokens":100,"output_tokens":999,"cache_read_input_tokens":200,"cache_creation_input_tokens":300}"#,
            serde_json::json!(600),
        ),
        ("null", serde_json::Value::Null),
        (r#"{"input_tokens":100}"#, serde_json::Value::Null),
        (
            r#"{"input_tokens":-1,"cache_read_input_tokens":0,"cache_creation_input_tokens":0}"#,
            serde_json::Value::Null,
        ),
        (
            r#"{"input_tokens":18446744073709551615,"cache_read_input_tokens":1,"cache_creation_input_tokens":0}"#,
            serde_json::Value::Null,
        ),
    ] {
        let input = format!(
            r#"{{"session_id":"s","context_window":{{"current_usage":{usage}}},"prompt":"SECRET"}}"#
        );
        let report = aherald::statusline::parse(input.as_bytes()).unwrap();
        let report = serde_json::to_value(report).unwrap();
        assert_eq!(report.get("context_tokens"), Some(&expected));
        assert!(!report.to_string().contains("SECRET"));
    }
}

#[test]
fn warning_kinds_are_distinct_and_hook_installer_includes_activity() {
    assert_ne!(Kind::ContextWarning, Kind::IdleWarning);
    for agent in Agent::ALL {
        let events = aherald::hook_setup::events(agent);
        for name in [
            "UserPromptSubmit",
            "PreToolUse",
            "PostToolUse",
            "SessionEnd",
        ] {
            assert!(events.contains(&name), "{agent:?}: {name}");
        }
    }
}

#[test]
fn warning_cards_reuse_existing_builtin_artwork() {
    for name in ["context_warning", "idle_warning"] {
        let event = aherald::events::Event::demo(name).unwrap();
        let art = aherald::artwork::resolve(
            &aherald::config::Config::default(),
            &event,
            std::path::Path::new("assets"),
        )
        .unwrap();
        assert!(art.is_some(), "{name} needs a builtin picture");
    }
}
