use aherald::{
    config::{Color, Config},
    events::Kind,
};
use std::path::Path;

fn parse(text: &str) -> Config {
    Config::parse(text, Path::new(".")).unwrap()
}

#[test]
fn no_border_color_is_configured_by_default() {
    let config = Config::default();
    for kind in [Kind::TurnComplete, Kind::PermissionRequired, Kind::Error] {
        assert_eq!(config.event_border_color(kind), None);
    }
    assert_eq!(config.usage_border_color(), None);
}

#[test]
fn global_border_color_is_inherited_and_overridden_per_event_and_for_usage() {
    let config = parse(
        "[notifications]\nborder_color = '#112233'\n[events.error]\nborder_color = '#445566'\n[codex.usage]\nborder_color = '#778899'",
    );
    assert_eq!(
        config.event_border_color(Kind::TurnComplete),
        Some(Color([0x11, 0x22, 0x33]))
    );
    assert_eq!(
        config.event_border_color(Kind::Error),
        Some(Color([0x44, 0x55, 0x66]))
    );
    assert_eq!(config.usage_border_color(), Some(Color([0x77, 0x88, 0x99])));
}

#[test]
fn invalid_border_color_is_rejected() {
    assert!(Config::parse("[notifications]\nborder_color = 'red'", Path::new(".")).is_err());
    assert!(Config::parse("[events.error]\nborder_color = '#12345'", Path::new(".")).is_err());
}
