// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use super::*;

/// The date is the cache file's, in UTC, across a leap day and the epoch.
#[test]
fn the_date_is_the_civil_day_in_utc() {
    let at = |days: u64| UNIX_EPOCH + Duration::from_secs(days * 86_400 + 3_599);
    assert_eq!(date(at(0)), "1970-01-01");
    assert_eq!(date(at(19_782)), "2024-02-29");
    assert_eq!(date(at(20_635)), "2026-07-01");
}

/// The time of day is the seconds into the UTC day, at the day's edges.
#[test]
fn the_date_time_is_the_utc_second() {
    let at = |secs: u64| UNIX_EPOCH + Duration::from_secs(secs);
    assert_eq!(date_time(at(0)), "1970-01-01 00:00:00");
    assert_eq!(date_time(at(86_399)), "1970-01-01 23:59:59");
    assert_eq!(
        date_time(at(19_782 * 86_400 + 3_661)),
        "2024-02-29 01:01:01"
    );
    assert_eq!(
        date_time(UNIX_EPOCH - Duration::from_secs(5)),
        "1970-01-01 00:00:00"
    );
}
