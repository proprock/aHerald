use aherald::usage::Snapshot;
use serde_json::json;

#[test]
fn multi_bucket_precedes_legacy_and_preserves_optional_values_and_credits() {
    let snapshot = Snapshot::parse(json!({
        "rateLimits":{"primary":{"usedPercent":99}},
        "rateLimitsByLimitId":{
            "codex":{"limitId":"codex","primary":{"usedPercent":0,"windowDurationMins":300,"resetsAt":1792882800},
                "secondary":{"usedPercent":42,"windowDurationMins":10080},
                "credits":{"hasCredits":true,"unlimited":false,"balance":"12.50"},"planType":"pro"},
            "other":{"limitName":"Other","primary":{"usedPercent":110},"secondary":null,"unknown":true}
        },
        "rateLimitResetCredits":{"availableCount":2,"credits":null},"ordinaryUsageAllowed":false
    })).unwrap();
    assert_eq!(snapshot.buckets.len(), 2);
    let main = &snapshot.buckets["codex"];
    assert_eq!(
        main.primary.as_ref().unwrap().remaining_percent(),
        Some(100.0)
    );
    assert_eq!(main.secondary.as_ref().unwrap().resets_at, None);
    assert_eq!(
        main.credits.as_ref().unwrap().balance.as_deref(),
        Some("12.50")
    );
    assert_eq!(
        snapshot.buckets["other"]
            .primary
            .as_ref()
            .unwrap()
            .remaining_percent(),
        Some(0.0)
    );
    assert_eq!(
        snapshot.reset_credits.as_ref().unwrap().available_count,
        Some(2)
    );
    assert!(snapshot.reset_credits.unwrap().credits.is_none());
    assert_eq!(snapshot.ordinary_usage_allowed, Some(false));
}
#[test]
fn legacy_missing_and_empty_are_not_zero_or_fallback() {
    let snapshot =
        Snapshot::parse(json!({"rateLimits":{"primary":{"usedPercent":null},"secondary":null}}))
            .unwrap();
    assert_eq!(
        snapshot.buckets["codex"]
            .primary
            .as_ref()
            .unwrap()
            .remaining_percent(),
        None
    );
    assert!(snapshot.buckets["codex"].secondary.is_none());
    assert!(
        Snapshot::parse(json!({"rateLimits":null}))
            .unwrap()
            .buckets
            .is_empty()
    );
    assert!(
        Snapshot::parse(
            json!({"rateLimits":{"primary":{"usedPercent":55}},"rateLimitsByLimitId":{}})
        )
        .unwrap()
        .buckets
        .is_empty()
    );
    for value in [
        json!({}),
        json!({"rateLimits":[]}),
        json!({"rateLimits":{"primary":{"usedPercent":-1}}}),
        json!({"rateLimits":{"primary":{"windowDurationMins":-3}}}),
    ] {
        assert!(Snapshot::parse(value).is_err());
    }
}
#[test]
fn sparse_updates_preserve_other_buckets_windows_and_nullable_metadata() {
    let mut snapshot = Snapshot::parse(json!({"rateLimitsByLimitId":{
        "codex":{"planType":"pro","primary":{"usedPercent":20,"resetsAt":123},"secondary":{"usedPercent":30}},
        "other":{"primary":{"usedPercent":80}}
    }})).unwrap();
    assert!(snapshot.update(json!({"rateLimits":{"limitId":"codex","planType":null,"primary":{"usedPercent":25}}})).unwrap());
    let bucket = &snapshot.buckets["codex"];
    assert_eq!(bucket.plan_type.as_deref(), Some("pro"));
    assert_eq!(bucket.primary.as_ref().unwrap().used_percent, Some(25.0));
    assert_eq!(bucket.primary.as_ref().unwrap().resets_at, Some(123));
    assert_eq!(bucket.secondary.as_ref().unwrap().used_percent, Some(30.0));
    assert_eq!(
        snapshot.buckets["other"]
            .primary
            .as_ref()
            .unwrap()
            .used_percent,
        Some(80.0)
    );
    // No bucket ID cannot safely identify a target in a multi-bucket snapshot: refetch.
    assert!(
        !snapshot
            .update(json!({"rateLimits":{"primary":{"usedPercent":1}}}))
            .unwrap()
    );
}

#[test]
fn reset_credit_details_are_optional_and_not_inferred_from_count() {
    let snapshot = Snapshot::parse(json!({"rateLimits":{},"rateLimitResetCredits":{
        "availableCount":3,"credits":[{"id":"synthetic-reset","resetType":"codexRateLimits",
            "status":"available","grantedAt":100,"expiresAt":null,"title":null}]
    }}))
    .unwrap();
    let credits = snapshot.reset_credits.unwrap();
    assert_eq!(credits.available_count, Some(3));
    let rows = credits.credits.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, "synthetic-reset");
    assert_eq!(rows[0].expires_at, None);
    let snapshot = Snapshot::parse(
        json!({"rateLimits":{},"rateLimitResetCredits":{"availableCount":0,"credits":[]}}),
    )
    .unwrap();
    assert!(snapshot.reset_credits.unwrap().credits.unwrap().is_empty());
}

#[cfg(windows)]
#[test]
fn reset_conversion_uses_rules_at_the_timestamp_across_both_dst_boundaries() {
    use aherald::usage::local_reset_in;
    use windows::Win32::{Foundation::SYSTEMTIME, System::Time::DYNAMIC_TIME_ZONE_INFORMATION};
    let mut zone = DYNAMIC_TIME_ZONE_INFORMATION {
        Bias: -60,
        DaylightBias: -60,
        StandardDate: SYSTEMTIME {
            wMonth: 10,
            wDay: 5,
            wDayOfWeek: 0,
            wHour: 3,
            ..Default::default()
        },
        DaylightDate: SYSTEMTIME {
            wMonth: 3,
            wDay: 5,
            wDayOfWeek: 0,
            wHour: 2,
            ..Default::default()
        },
        ..Default::default()
    };
    for (to, from) in zone
        .TimeZoneKeyName
        .iter_mut()
        .zip("Central European Standard Time".encode_utf16())
    {
        *to = from;
    }
    // Europe/Warsaw, 2026-03-29 00:59/01:00 UTC and 2026-10-25 00:59/01:00 UTC.
    assert_eq!(
        local_reset_in(1774745940, Some(&zone)).unwrap(),
        "2026-03-29 01:59:00 (UTC+01:00)"
    );
    assert_eq!(
        local_reset_in(1774746000, Some(&zone)).unwrap(),
        "2026-03-29 03:00:00 (UTC+02:00)"
    );
    assert_eq!(
        local_reset_in(1792889940, Some(&zone)).unwrap(),
        "2026-10-25 02:59:00 (UTC+02:00)"
    );
    assert_eq!(
        local_reset_in(1792890000, Some(&zone)).unwrap(),
        "2026-10-25 02:00:00 (UTC+01:00)"
    );
    assert!(local_reset_in(i64::MAX, Some(&zone)).is_none());
    assert!(local_reset_in(i64::MIN, Some(&zone)).is_none());
}
