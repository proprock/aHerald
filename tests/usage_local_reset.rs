//! Quota reset times are shown in the local zone with their UTC offset, on Unix as on Windows.
//! One test only: it changes the process's `TZ`, which other tests must not race with.
#![cfg(unix)]
use aherald::usage::local_reset;

fn at(zone: &str, timestamp: i64) -> Option<String> {
    // SAFETY: this is the only test in the process, so nothing else reads the environment.
    unsafe { std::env::set_var("TZ", zone) };
    local_reset(timestamp)
}

#[test]
fn reset_times_are_converted_to_the_local_zone_with_their_offset() {
    // 2023-11-14 22:13:20 UTC and 2023-07-22 04:26:40 UTC.
    let (winter, summer) = (1_700_000_000, 1_690_000_000);
    assert_eq!(
        at("UTC", winter).as_deref(),
        Some("2023-11-14 22:13:20 (UTC+00:00)")
    );
    assert_eq!(
        at("Europe/Minsk", winter).as_deref(),
        Some("2023-11-15 01:13:20 (UTC+03:00)")
    );
    assert_eq!(
        at("Asia/Kolkata", winter).as_deref(),
        Some("2023-11-15 03:43:20 (UTC+05:30)")
    );
    // Daylight saving follows the instant, not the current date.
    assert_eq!(
        at("America/New_York", winter).as_deref(),
        Some("2023-11-14 17:13:20 (UTC-05:00)")
    );
    assert_eq!(
        at("America/New_York", summer).as_deref(),
        Some("2023-07-22 00:26:40 (UTC-04:00)")
    );
    // A timestamp the platform cannot represent is no text rather than a wrong one.
    assert_eq!(at("UTC", i64::MAX), None);
}
