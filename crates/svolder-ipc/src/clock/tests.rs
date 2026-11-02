// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// A broken-down time built by hand, so no case depends on the zone the process runs in.
fn at(year: i32, month: i32, day: i32, hour: i32, min: i32, sec: i32) -> libc::tm {
    // SAFETY: `tm` is plain integers (and, on some targets, a pointer) that zero is valid for.
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    tm.tm_year = year - 1900;
    tm.tm_mon = month - 1;
    tm.tm_mday = day;
    tm.tm_hour = hour;
    tm.tm_min = min;
    tm.tm_sec = sec;
    tm
}

#[test]
fn hhmm_formats_hours_and_minutes_with_two_digits() {
    assert_eq!(hhmm(&at(1970, 1, 2, 14, 2, 0)), "14:02");
    assert_eq!(hhmm(&at(1970, 1, 2, 0, 5, 59)), "00:05");
}

#[test]
fn stamp_formats_the_date_and_the_time() {
    assert_eq!(stamp(&at(2026, 10, 4, 18, 4, 26)), "2026-10-04 18:04:26");
    assert_eq!(stamp(&at(987, 1, 2, 3, 4, 5)), "0987-01-02 03:04:05");
}

/// The wrapper over `localtime_r` gives some time of day in the shape the formatters give.
#[test]
fn the_local_wrappers_give_the_shape_whatever_the_zone() {
    let now = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_791_137_066);
    let shown = local_hhmm(now);
    assert_eq!(shown.len(), 5);
    assert_eq!(&shown[2..3], ":");
    let stamped = local_stamp(now);
    assert_eq!(stamped.len(), 19);
    assert!(stamped.starts_with("2026-10-0"), "{stamped}");
}

#[test]
fn the_month_and_day_come_from_the_same_local_time_as_the_clock() {
    let now = std::time::SystemTime::now();
    let (month, day) = local_month_day(now);
    let stamp = local_stamp(now);
    assert_eq!(format!("{month:02}-{day:02}"), stamp[5..10]);
}
