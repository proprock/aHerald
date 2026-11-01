use aherald::{config::Config, demo::Carousel, events::Kind};

#[test]
fn warning_samples_show_the_selected_agents_configured_values() {
    let config = Config::parse(
        "[codex.warnings]\ncontext_tokens='250k'\nidle_minutes=25",
        std::path::Path::new("."),
    )
    .unwrap();
    let mut cards = Carousel::new(&config);
    cards.navigate(5, 0);
    assert_eq!(cards.selected().event.title(), "Context: 250.001k > 250k");
    assert_eq!(cards.selected().event.source.label(), "Codex");
    cards.navigate(1, 0);
    assert_eq!(cards.selected().event.title(), "Idle: 25m 1s (limit 25m)");
    let config = Config::parse(
        "[claude.warnings]\ncontext_tokens='400k'",
        std::path::Path::new("."),
    )
    .unwrap();
    let mut cards = Carousel::new(&config);
    cards.navigate(5, 0);
    assert_eq!(cards.selected().event.title(), "Context: 400.001k > 400k");
    assert_eq!(cards.selected().event.source.label(), "Claude");
}

#[test]
fn carousel_keeps_all_cards_and_moves_without_an_empty_phase() {
    let mut config = Config::parse("", std::path::Path::new(".")).unwrap();
    for setting in config.events.values_mut() {
        setting.enabled = false;
        setting.duration_ms = 0;
    }
    let mut cards = Carousel::new(&config);
    assert_eq!(cards.len(), 7);
    assert_eq!(cards.index(), 0);
    assert_eq!(cards.selected().event.kind, Kind::ALL[0]);
    assert_eq!(cards.selected().event.folder.as_deref(), Some("[Test]"));
    assert!(cards.disabled());
    cards.tick(0, false);
    cards.tick(4999, false);
    assert_eq!(cards.index(), 0);
    cards.tick(5000, false);
    assert_eq!(cards.index(), 1);
    cards.navigate(-1, 6000);
    assert_eq!(cards.index(), 0);
    cards.tick(10999, false);
    assert_eq!(cards.index(), 0);
    cards.tick(11000, false);
    assert_eq!(cards.index(), 1);
    assert_eq!(cards.len(), 7);
    for _ in 0..6 {
        cards.navigate(1, 12000);
    }
    assert_eq!(cards.index(), 0);
}

#[test]
fn carousel_pauses_its_clock_and_uses_configured_durations() {
    let mut config = Config::parse("", std::path::Path::new(".")).unwrap();
    config.events[Kind::ALL[0]].duration_ms = 8000;
    let mut cards = Carousel::new(&config);
    cards.tick(0, false);
    cards.tick(7000, false);
    cards.tick(20000, true);
    cards.tick(20999, false);
    assert_eq!(cards.index(), 0);
    cards.tick(21000, false);
    assert_eq!(cards.index(), 1);
}
