// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A moment as UTC text, from the standard library's clock alone.

use std::time::{SystemTime, UNIX_EPOCH};

/// `time` as `YYYY-MM-DD`, in UTC.
pub(crate) fn date(time: SystemTime) -> String {
    civil(seconds(time) / 86_400)
}

/// `time` as `YYYY-MM-DD HH:MM:SS`, in UTC.
pub(crate) fn date_time(time: SystemTime) -> String {
    let secs = seconds(time);
    let in_day = secs % 86_400;
    format!(
        "{} {:02}:{:02}:{:02}",
        civil(secs / 86_400),
        in_day / 3_600,
        in_day % 3_600 / 60,
        in_day % 60
    )
}

/// The whole seconds since the Unix epoch, or 0 for a time before it.
fn seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// The civil date of a day count since the epoch, as `YYYY-MM-DD`.
fn civil(days: u64) -> String {
    // The civil calendar from a day count, after Howard Hinnant's `civil_from_days`.
    let shifted = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = if month_part < 10 {
        month_part + 3
    } else {
        month_part - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

#[cfg(test)]
mod tests;
