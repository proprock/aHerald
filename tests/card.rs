use aherald::card::{Action, Card, Heading, age_text, height};

/// Card size the geometry tests use: 320 x 150 logical pixels at scale 1.
const W: i32 = 320;
const H: i32 = 150;

#[test]
fn heading_is_harness_and_folder_never_a_session_id() {
    let full = Heading::new("Codex", Some("aHerald"), Some("Fix the build"));
    assert_eq!(full.title, "Codex, aHerald");
    assert_eq!(full.name.as_deref(), Some("Fix the build"));
    assert_eq!(full.window_text(), "Codex, aHerald: Fix the build");
    let no_name = Heading::new("Claude", Some("aHerald"), None);
    assert_eq!(
        (no_name.title.as_str(), no_name.name),
        ("Claude, aHerald", None)
    );
    let no_folder = Heading::new("Claude", None, None);
    assert_eq!(no_folder.title, "Claude");
    assert_eq!(no_folder.window_text(), "Claude");
}

#[test]
fn usage_summary_line_splits_into_agent_and_summary_rows() {
    let heading = Heading::from_summary("Codex: 5h 25% / 2:35, 7d 10% / 6d 11h");
    assert_eq!(heading.title, "Codex");
    assert_eq!(
        heading.name.as_deref(),
        Some("5h 25% / 2:35, 7d 10% / 6d 11h")
    );
    let notice = Heading::from_summary("aHerald");
    assert_eq!((notice.title.as_str(), notice.name), ("aHerald", None));
    // Only a short leading word is an agent name; any other text stays whole.
    let prose = Heading::from_summary("Restart cancelled: the configuration stays active");
    assert_eq!(
        prose.title,
        "Restart cancelled: the configuration stays active"
    );
    assert_eq!(prose.name, None);
    assert_eq!(Heading::from_summary("Claude:").title, "Claude:");
}

#[test]
fn single_card_has_no_counter_or_navigation_and_body_click_dismisses() {
    let card = Card::new("Codex", 0, 1, true);
    assert_eq!(card.heading.title, "Codex");
    assert_eq!(card.counter(), None);
    assert!(!card.navigation);
    assert!(!card.dismiss_button);
    for (x, y) in [(0, 0), (25, 40), (310, 140)] {
        assert_eq!(card.hit_test(W, H, 1.0, x, y), Some(Action::Dismiss));
    }
}

#[test]
fn counter_and_bordered_arrows_sit_at_the_bottom_right() {
    let card = Card::new("Codex", 1, 3, true);
    assert_eq!(card.counter().as_deref(), Some("2/3"));
    assert!(card.navigation);
    let buttons = card.buttons(W, H, 1.0);
    let area = |action| {
        buttons
            .iter()
            .find(|(a, _)| *a == action)
            .map(|(_, r)| (r.left, r.top, r.right, r.bottom))
            .unwrap()
    };
    let (previous, next) = (area(Action::Previous), area(Action::Next));
    // Same row, Next flush with the right padding, a counter gap between the two.
    assert_eq!((previous.1, previous.3), (next.1, next.3));
    assert_eq!(next.2, W - 18);
    assert!(
        previous.2 < next.0 - 30,
        "room for the 'n/m' counter between arrows"
    );
    assert!(
        next.3 <= H - 8 && next.1 >= H - 40,
        "arrows live in the bottom row"
    );
    // Square-ish bordered buttons, not slivers.
    assert!(next.2 - next.0 >= 24 && next.3 - next.1 >= 22);
    let centre = |r: (i32, i32, i32, i32)| ((r.0 + r.2) / 2, (r.1 + r.3) / 2);
    let (px, py) = centre(previous);
    let (nx, ny) = centre(next);
    assert_eq!(card.hit_test(W, H, 1.0, px, py), Some(Action::Previous));
    assert_eq!(card.hit_test(W, H, 1.0, nx, ny), Some(Action::Next));
    assert_eq!(card.hit_test(W, H, 1.0, 100, py), Some(Action::Dismiss));
    assert_eq!(card.hit_test(W, H, 1.0, -1, py), None);
    // The same cell at twice the scale.
    assert_eq!(
        card.hit_test(2 * W, 2 * H, 2.0, 2 * nx, 2 * ny),
        Some(Action::Next)
    );
}

#[test]
fn explicit_dismiss_is_bottom_left_and_the_age_follows_it() {
    let mut card = Card::new("Codex", 0, 1, false);
    card.text_left = Some(100);
    assert!(card.dismiss_button);
    let buttons = card.buttons(W, H, 1.0);
    let (action, dismiss) = &buttons[0];
    assert_eq!(*action, Action::Dismiss);
    assert_eq!(dismiss.left, 100, "aligned with the text column");
    assert!(dismiss.top >= H - 40 && dismiss.bottom <= H - 8);
    let age = card.age_area(W, H, 1.0);
    assert!(age.left > dismiss.right, "age sits right of the button");
    assert_eq!((age.top, age.bottom), (dismiss.top, dismiss.bottom));
    let mid = (dismiss.left + dismiss.right) / 2;
    let row = (dismiss.top + dismiss.bottom) / 2;
    assert_eq!(card.hit_test(W, H, 1.0, mid, row), Some(Action::Dismiss));
    assert_eq!(card.hit_test(W, H, 1.0, 120, 40), None);
}

#[test]
fn age_starts_at_the_text_column_without_a_dismiss_button() {
    let mut card = Card::new("Codex", 0, 1, true);
    card.text_left = Some(100);
    assert_eq!(card.age_area(W, H, 1.0).left, 100);
}

#[test]
fn age_ends_before_the_arrows_so_the_two_never_overlap() {
    let card = Card::new("Codex", 0, 4, false);
    let previous = card
        .buttons(W, H, 1.0)
        .into_iter()
        .find(|(a, _)| *a == Action::Previous)
        .unwrap()
        .1;
    assert!(card.age_area(W, H, 1.0).right <= previous.left);
}

#[test]
fn card_height_is_one_value_whatever_the_queue_holds() {
    assert_eq!(height(1.0), 150);
    assert_eq!(height(1.5), 225);
    assert_eq!(height(2.0), 300);
}

#[test]
fn age_text_is_coarse_so_the_card_redraws_rarely() {
    assert_eq!(age_text(0), "just now");
    assert_eq!(age_text(59_999), "just now");
    assert_eq!(age_text(60_000), "1 min ago");
    assert_eq!(age_text(59 * 60_000 + 59_999), "59 min ago");
    assert_eq!(age_text(60 * 60_000), "1 h ago");
    assert_eq!(age_text(23 * 3_600_000 + 3_599_999), "23 h ago");
    assert_eq!(age_text(24 * 3_600_000), "1 d ago");
    assert_eq!(
        age_text(u64::MAX),
        format!("{} d ago", u64::MAX / 86_400_000)
    );
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
