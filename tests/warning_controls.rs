use aherald::{
    agent::Agent,
    config::Config,
    events::{Event, Source},
    session_warnings::Warnings,
};
use std::path::Path;

#[test]
fn saved_warning_switches_suppress_delivery_without_removing_thresholds() {
    let config = Config::parse("[claude.warnings]\ncontext_tokens=1\ncontext_enabled=false\nidle_enabled=false\n[codex.warnings]\ncontext_tokens=1\ncontext_enabled=false\nidle_enabled=false", Path::new(".")).unwrap();
    let mut warnings = Warnings::default();
    for (agent, source) in [
        (Agent::Claude, Source::ClaudeHook),
        (Agent::Codex, Source::CodexHook),
    ] {
        assert!(warnings.context(agent, "saved", Some(2), &config).is_none());
        let mut event = Event::demo("turn_complete").unwrap();
        event.session_id = Some("saved".into());
        event.source = source;
        warnings.observe(&event, &config, 0);
    }
    assert!(!warnings.has_deadline());
    assert!(warnings.due(4_000_000, false).is_empty());
}
