use aherald::{
    config::{Config, UsageDisplay},
    usage::Snapshot,
    usage_service::View,
};
use serde_json::json;
use std::time::Duration;

fn view() -> View {
    let mut v = View::new(true, Duration::from_secs(300));
    v.updated = Some(Duration::ZERO);
    v.snapshot = Some(Snapshot::parse(json!({"rateLimits":{"primary":{"usedPercent":75,"resetsAt":10300},"secondary":{"usedPercent":90,"resetsAt":519400}}})).unwrap());
    v
}
#[test]
fn compact_modes_and_countdowns() {
    assert_eq!(Config::default().usage.display, UsageDisplay::Remain);
    assert_eq!(
        view().compact(Duration::ZERO, 1000, UsageDisplay::Remain),
        "Codex remain: 5h 25% / 2:35, week 10% / 6d"
    );
    assert_eq!(
        view().compact(Duration::ZERO, 1000, UsageDisplay::Used),
        "Codex used: 5h 75% / 2:35, week 90% / 6d"
    );
    assert!(Config::parse("[usage]\ndisplay = 'invalid'", std::path::Path::new(".")).is_err());
}
#[test]
fn countdown_boundaries_and_persistent_values() {
    for (seconds, expected) in [
        (0, "0 min"),
        (1, "1 min"),
        (59, "1 min"),
        (60, "1 min"),
        (3599, "60 min"),
        (3600, "1:00"),
        (86399, "24:00"),
        (86400, "1d"),
    ] {
        assert_eq!(
            aherald::usage_service::countdown(Some(1000 + seconds), 1000),
            expected
        );
    }
    assert_eq!(aherald::usage_service::countdown(None, 1000), "—");
    assert_eq!(aherald::usage_service::countdown(Some(1), 1000), "0 min");
    assert!(
        view()
            .compact(Duration::from_secs(300), 1000, UsageDisplay::Remain)
            .contains("Codex remain: 5h 25%")
    );
    assert!(
        View::new(false, Duration::from_secs(300))
            .compact(Duration::ZERO, 1000, UsageDisplay::Remain)
            .contains("disabled")
    );
}

#[test]
fn card_keeps_extended_account_information_and_all_pages() {
    let mut v = view();
    v.snapshot=Some(Snapshot::parse(json!({"rateLimitsByLimitId":{
        "codex":{"planType":"plus","primary":{"usedPercent":8,"resetsAt":9700},"secondary":{"usedPercent":14,"resetsAt":433000},"credits":{"hasCredits":false,"unlimited":false,"balance":"0"}},
        "extra":{"limitName":"Extra","primary":{"usedPercent":50}}
    },"ordinaryUsageAllowed":true,"rateLimitResetCredits":{"availableCount":2,"credits":[{"id":"one","title":"Full reset (Weekly + 5 hr)","status":"available","expiresAt":1793296620},{"id":"two","title":"Second reset","status":"available"}]}})).unwrap());
    let text = v.card_text(Duration::ZERO, 1000);
    for expected in [
        "Codex plan: plus",
        "5h: 8% used / 2:25",
        "Week: 14% used / 5d",
        "Workspace credits: available=no, unlimited=no, balance=0",
        "Ordinary usage allowed: yes",
        "Available reset credits: 2 (read-only)",
        "Full reset (Weekly + 5 hr): available; expires",
        "Second reset",
        "Extra",
    ] {
        assert!(text.contains(expected), "missing {expected} in {text}");
    }
    let pages = aherald::usage_service::card_pages(&text);
    assert!(pages.len() > 1);
    assert!(
        pages
            .iter()
            .all(|p| p.lines().count() <= 12 && p.lines().all(|l| l.chars().count() <= 72))
    );
    assert!(pages.join("\n").contains("Second reset"));
}
