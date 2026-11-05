//! Civil dates (proleptic Gregorian) for the places that turn Unix time into text and back.

/// Days since 1970-01-01 of a civil date (Howard Hinnant's `days_from_civil`). `month` is 1-12
/// and `day` 1-31; the caller has checked them.
pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date `(year, month, day)` of a day count since 1970-01-01 (`civil_from_days`).
pub(crate) fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATES: [(i64, i64, i64, i64); 8] = [
        (1970, 1, 1, 0),
        (1969, 12, 31, -1),
        (2000, 2, 29, 11_016),
        (2024, 2, 29, 19_782),
        (2100, 2, 28, 47_540),
        (2100, 3, 1, 47_541),
        (1900, 3, 1, -25_508),
        (2026, 11, 5, 20_762),
    ];

    #[test]
    fn known_dates_map_to_their_day_numbers_and_back() {
        for (year, month, day, days) in DATES {
            assert_eq!(
                days_from_civil(year, month, day),
                days,
                "{year}-{month}-{day}"
            );
            assert_eq!(civil_from_days(days), (year, month, day), "{days}");
        }
    }

    #[test]
    fn a_century_that_is_not_a_leap_year_has_no_february_29() {
        // 2100 is not a leap year, 2000 is: the day after 2100-02-28 is March 1.
        assert_eq!(
            civil_from_days(days_from_civil(2100, 2, 28) + 1),
            (2100, 3, 1)
        );
        assert_eq!(
            civil_from_days(days_from_civil(2000, 2, 28) + 1),
            (2000, 2, 29)
        );
    }

    #[test]
    fn every_day_of_four_centuries_round_trips() {
        let first = days_from_civil(1800, 1, 1);
        let last = days_from_civil(2200, 12, 31);
        for days in first..=last {
            let (year, month, day) = civil_from_days(days);
            assert_eq!(days_from_civil(year, month, day), days);
        }
    }
}
