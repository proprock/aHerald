use aherald::usage_service::card_pages;
#[test]
fn every_credit_remains_reachable_on_bounded_pages() {
    let text = (0..150)
        .map(|n| format!("Reset credit {n}: available; expires 2026-11-06 22:36\n"))
        .collect::<String>();
    let pages = card_pages(&text);
    assert!(pages.len() > 1);
    assert_eq!(pages.join("\n"), text.trim_end());
    assert!(pages.iter().all(|p| p.lines().count() <= 12));
}
#[test]
fn unicode_and_long_values_are_preserved_when_wrapping() {
    let text = "абвг😀".repeat(100);
    let pages = card_pages(&text);
    assert_eq!(pages.join("").replace('\n', ""), text);
    assert!(
        pages
            .iter()
            .all(|p| p.lines().all(|l| l.chars().count() <= 72))
    );
}
