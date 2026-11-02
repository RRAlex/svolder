// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Local wall-clock time with no dependency beyond `libc`, shared by the front ends and the
//! daemon.

/// The broken-down local time of `now`.
fn local_tm(now: std::time::SystemTime) -> libc::tm {
    // `as _` takes `time_t` from `localtime_r`'s signature: naming `libc::time_t` is
    // deprecated on musl, whose `time_t` is 64-bit on every target.
    let secs = now
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as _)
        .unwrap_or(0);
    let mut tm = unsafe { std::mem::zeroed::<libc::tm>() };
    // SAFETY: `localtime_r` writes only into the `tm` we own and reads only the `time_t` we
    // pass.
    unsafe { libc::localtime_r(&secs, &mut tm) };
    tm
}

/// The local wall-clock time as `HH:MM`.
pub fn local_hhmm(now: std::time::SystemTime) -> String {
    hhmm(&local_tm(now))
}

/// The local date and time as `YYYY-MM-DD HH:MM:SS`, the stamp the daemon's log lines carry.
pub fn local_stamp(now: std::time::SystemTime) -> String {
    stamp(&local_tm(now))
}

/// The local month (1 to 12) and day of the month (1 to 31).
pub fn local_month_day(now: std::time::SystemTime) -> (u8, u8) {
    let tm = local_tm(now);
    ((tm.tm_mon + 1) as u8, tm.tm_mday as u8)
}

/// `tm` as `HH:MM`.
fn hhmm(tm: &libc::tm) -> String {
    format!("{:02}:{:02}", tm.tm_hour, tm.tm_min)
}

/// `tm` as `YYYY-MM-DD HH:MM:SS`.
fn stamp(tm: &libc::tm) -> String {
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        tm.tm_year + 1900,
        tm.tm_mon + 1,
        tm.tm_mday,
        tm.tm_hour,
        tm.tm_min,
        tm.tm_sec
    )
}

#[cfg(test)]
mod tests;
