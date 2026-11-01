use aherald::{
    agent::Agent,
    card::Card,
    card_paint::event_card_text,
    config::Config,
    events::{Event, Source},
    session_warnings::Warnings,
};

fn config() -> Config {
    Config::parse(
        "[codex.warnings]\ncontext_tokens='250k'\nidle_minutes=25\n[claude.warnings]\ncontext_tokens='400k'\nidle_minutes=55",
        std::path::Path::new("."),
    ).unwrap()
}

#[test]
fn context_cards_show_exact_kilotokens_and_the_agents_own_threshold() {
    let config = config();
    let mut warnings = Warnings::default();
    for (agent, tokens, expected) in [
        (Agent::Codex, 250001, "Context: 250.001k > 250k"),
        (Agent::Claude, 400500, "Context: 400.5k > 400k"),
    ] {
        let event = warnings
            .context(agent, "same-session", Some(tokens), &config)
            .unwrap();
        let controls = Card::new(event.source.label(), 0, 1, false);
        let text = event_card_text(&config, &event, &controls, None);
        assert_eq!(text.label, expected);
        assert_eq!(text.heading.title, agent.label());
        let wire = serde_json::to_vec(&event).unwrap();
        let restored: Event = serde_json::from_slice(&wire).unwrap();
        assert_eq!(restored.title(), expected);
    }
}

#[test]
fn idle_cards_show_elapsed_time_and_the_agents_own_limit_even_after_a_delay() {
    let config = config();
    let mut warnings = Warnings::default();
    for source in [Source::CodexHook, Source::ClaudeHook] {
        let mut event = Event::demo("turn_complete").unwrap();
        event.source = source;
        event.session_id = Some("same-session".into());
        warnings.observe(&event, &config, 1000);
    }
    assert!(warnings.due(1_501_000, false).is_empty());
    assert!(warnings.due(1_651_000, true).is_empty());
    let codex = warnings.due(1_651_000, false).pop().unwrap();
    assert_eq!(codex.source, Source::CodexHook);
    assert_eq!(codex.title(), "Idle: 27m 30s (limit 25m)");
    let claude = warnings.due(3_301_001, false).pop().unwrap();
    assert_eq!(claude.source, Source::ClaudeHook);
    assert_eq!(claude.title(), "Idle: 55m 0s (limit 55m)");
    assert_eq!(warnings.pending()[0].title(), codex.title());
}
