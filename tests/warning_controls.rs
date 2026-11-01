use aherald::{
    agent::Agent,
    config::Config,
    events::{Event, Kind, Source},
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

#[test]
fn runtime_switches_suppress_both_agents_independently_of_each_other() {
    let config = Config::parse("[claude.warnings]\ncontext_tokens=1\nidle_minutes=1\n[codex.warnings]\ncontext_tokens=1\nidle_minutes=1", Path::new(".")).unwrap();
    let mut warnings = Warnings::default();
    warnings.set_enabled(Kind::ContextWarning, false);
    assert!(!warnings.enabled(Kind::ContextWarning));
    assert!(warnings.enabled(Kind::IdleWarning));
    for agent in [Agent::Claude, Agent::Codex] {
        assert!(
            warnings
                .context(agent, "session", Some(2), &config)
                .is_none()
        );
    }
    let mut event = Event::demo("turn_complete").unwrap();
    event.session_id = Some("session".into());
    event.source = Source::CodexHook;
    warnings.observe(&event, &config, 0);
    assert_eq!(warnings.due(60_001, false).len(), 1);
    warnings.set_enabled(Kind::IdleWarning, false);
    assert!(warnings.pending().is_empty());
    assert!(!warnings.has_deadline());
    warnings.set_enabled(Kind::ContextWarning, true);
    for agent in [Agent::Claude, Agent::Codex] {
        assert!(
            warnings
                .context(agent, "session", Some(2), &config)
                .is_some()
        );
    }
    for source in [Source::ClaudeHook, Source::CodexHook] {
        event.source = source;
        warnings.observe(&event, &config, 70_000);
    }
    assert!(warnings.due(200_000, false).is_empty());
    warnings.set_enabled(Kind::IdleWarning, true);
    assert!(
        warnings.due(200_001, false).is_empty(),
        "disabled stops must not create stale reminders"
    );
    for source in [Source::ClaudeHook, Source::CodexHook] {
        event.source = source;
        warnings.observe(&event, &config, 210_000);
    }
    assert_eq!(warnings.due(270_001, false).len(), 2);
}

#[test]
fn context_switch_discards_pending_delivery_without_repeating_old_crossings() {
    let config = Config::parse("[codex.warnings]\ncontext_tokens=1", Path::new(".")).unwrap();
    let mut warnings = Warnings::default();
    warnings
        .context(Agent::Codex, "s", Some(2), &config)
        .unwrap();
    warnings.set_enabled(Kind::ContextWarning, false);
    assert!(warnings.pending().is_empty());
    warnings.set_enabled(Kind::ContextWarning, true);
    assert!(
        warnings
            .context(Agent::Codex, "s", Some(2), &config)
            .is_none()
    );
    warnings.context(Agent::Codex, "s", Some(1), &config);
    assert!(
        warnings
            .context(Agent::Codex, "s", Some(2), &config)
            .is_some()
    );
}
