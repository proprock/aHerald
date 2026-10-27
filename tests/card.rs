use aherald::card::{Action, Card};

#[test]
fn single_card_has_no_counter_or_navigation_and_body_click_dismisses() {
    let card = Card::new("Codex", 0, 1, true);
    assert_eq!(card.heading, "Codex");
    assert!(!card.navigation);
    assert!(!card.dismiss_button);
    for (x, y) in [(0, 0), (25, 40), (310, 100)] {
        assert_eq!(card.hit_test(320, 110, 1.0, x, y), Some(Action::Dismiss));
    }
}

#[test]
fn arrows_at_bottom_right_navigate_without_dismissing() {
    let card = Card::new("Codex", 1, 3, true);
    assert_eq!(card.heading, "Codex [2/3]");
    assert!(card.navigation);
    assert_eq!(
        card.hit_test(320, 110, 1.0, 248, 88),
        Some(Action::Previous)
    );
    assert_eq!(card.hit_test(320, 110, 1.0, 290, 88), Some(Action::Next));
    assert_eq!(card.hit_test(320, 110, 1.0, 100, 88), Some(Action::Dismiss));
    assert_eq!(card.hit_test(640, 220, 2.0, 580, 176), Some(Action::Next));
    assert_eq!(card.hit_test(320, 110, 1.0, -1, 88), None);
}

#[test]
fn explicit_dismiss_remains_available_when_body_dismissal_is_disabled() {
    let card = Card::new("Codex", 0, 1, false);
    assert!(card.dismiss_button);
    assert_eq!(card.hit_test(320, 110, 1.0, 100, 40), None);
    assert_eq!(card.hit_test(320, 110, 1.0, 270, 88), Some(Action::Dismiss));
}

#[test]
fn countdown_uses_ceiling_seconds_without_zero_or_persistent_timer() {
    use aherald::card::remaining_seconds;
    assert_eq!(remaining_seconds(3000, 0), Some(3));
    assert_eq!(remaining_seconds(3000, 999), Some(3));
    assert_eq!(remaining_seconds(3000, 1000), Some(2));
    assert_eq!(remaining_seconds(3000, 2000), Some(1));
    assert_eq!(remaining_seconds(3000, 3000), None);
    assert_eq!(remaining_seconds(0, 0), None);
    assert_eq!(remaining_seconds(0, u64::MAX), None);
}
