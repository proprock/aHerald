use aherald::{agent::Agent, config::Config, events::Source, session_warnings::Warnings};
use std::path::Path;

#[test]
fn codex_context_uses_request_input_without_adding_cached_tokens() {
    let report = aherald::codex_context::parse(br#"{"hook_event_name":"PostModelRequest","session_id":"s","turn_id":"t","status":"completed","token_usage":{"input_tokens":150,"cached_input_tokens":100,"output_tokens":9},"error":"SECRET","cwd":"SECRET"}"#).unwrap().unwrap();
    assert_eq!(report.session_id, "s");
    assert_eq!(report.tokens, 150);
    assert!(!serde_json::to_string(&report).unwrap().contains("SECRET"));
}

#[test]
fn unavailable_failed_subagent_and_invalid_readings_do_not_supply_context() {
    for extra in [
        r#""status":"completed","token_usage":null"#,
        r#""status":"failed","token_usage":{"input_tokens":200}"#,
        r#""status":"cancelled","token_usage":{"input_tokens":200}"#,
        r#""status":"completed","agent_id":"child","token_usage":{"input_tokens":200}"#,
        r#""status":"completed","token_usage":{"input_tokens":-1}"#,
        r#""status":"completed","token_usage":{"input_tokens":1.5}"#,
        r#""status":"completed","token_usage":{}"#,
    ] {
        let payload = format!(
            r#"{{"hook_event_name":"PostModelRequest","session_id":"s","turn_id":"t",{extra}}}"#
        );
        assert!(
            aherald::codex_context::parse(payload.as_bytes())
                .unwrap()
                .is_none()
        );
    }
    assert!(
        aherald::codex_context::parse(br#"{"hook_event_name":"Stop"}"#)
            .unwrap()
            .is_none()
    );
}

#[test]
fn agent_thresholds_and_same_named_sessions_are_independent() {
    let config = Config::parse(
        "[claude.warnings]\ncontext_tokens='1k'\n[codex.warnings]\ncontext_tokens=100",
        Path::new("."),
    )
    .unwrap();
    let mut state = Warnings::default();
    assert!(
        state
            .context(Agent::Codex, "same", Some(100), &config)
            .is_none()
    );
    let warning = state
        .context(Agent::Codex, "same", Some(101), &config)
        .unwrap();
    assert_eq!(warning.source, Source::CodexHook);
    assert!(
        state
            .context(Agent::Claude, "same", Some(101), &config)
            .is_none()
    );
    assert!(
        state
            .context(Agent::Claude, "same", Some(1001), &config)
            .is_some()
    );
    assert!(state.context(Agent::Codex, "same", None, &config).is_none());
    assert!(
        state
            .context(Agent::Codex, "same", Some(102), &config)
            .is_none()
    );
    assert!(
        state
            .context(Agent::Codex, "other", Some(102), &config)
            .is_some()
    );
    assert!(
        state
            .context(Agent::Codex, "same", Some(100), &config)
            .is_none()
    );
    assert!(
        state
            .context(Agent::Codex, "same", Some(101), &config)
            .is_some()
    );
    let disabled = Config::parse("[claude.warnings]\ncontext_tokens=1", Path::new(".")).unwrap();
    assert!(
        state
            .context(Agent::Codex, "disabled", Some(1000), &disabled)
            .is_none()
    );
}
