// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

#[test]
fn parses_levels() {
    assert_eq!(level_from("debug"), Level::Debug);
    assert_eq!(level_from("svolderd=warn"), Level::Warn);
    assert_eq!(level_from(""), Level::Info);
    assert_eq!(level_from("ERROR"), Level::Error);
    assert_eq!(level_from(" trace "), Level::Trace);
    assert_eq!(level_from("svolderd=nonsense"), Level::Info);
    assert_eq!(level_from("info"), Level::Info);
}

#[test]
fn levels_order_from_quiet_to_loud() {
    assert!(Level::Error < Level::Warn);
    assert!(Level::Info < Level::Debug);
    assert_eq!(Level::Info.max(Level::Debug), Level::Debug);
    assert_eq!(Level::Trace.max(Level::Debug), Level::Trace);
}

/// A stamped line reads the local date and time, then the daemon's name.
#[test]
fn a_stamped_line_reads_date_time_then_the_prefix() {
    assert_eq!(
        render(Level::Info, "listening", Some("2026-10-04 18:04:26")),
        "2026-10-04 18:04:26 svolderd: listening"
    );
}

/// Under the journal there is no stamp and no name, since the journal adds both.
#[test]
fn an_unstamped_line_is_the_message() {
    assert_eq!(render(Level::Info, "listening", None), "listening");
}

/// Warnings and errors name their level after the prefix, and the other levels do not.
#[test]
fn warnings_and_errors_carry_their_label() {
    assert_eq!(
        render(Level::Warn, "slow", Some("T")),
        "T svolderd: warning: slow"
    );
    assert_eq!(render(Level::Error, "gone", None), "error: gone");
    assert_eq!(render(Level::Debug, "x", None), "x");
}

/// The journal check compares the `device:inode` pair with stderr's.
#[test]
fn the_journal_is_told_by_device_and_inode() {
    assert!(stderr_is_journal(Some("8:12345"), Some((8, 12345))));
    assert!(!stderr_is_journal(Some("8:12345"), Some((8, 99))));
    assert!(!stderr_is_journal(Some("9:12345"), Some((8, 12345))));
    assert!(!stderr_is_journal(Some("garbage"), Some((8, 12345))));
    assert!(!stderr_is_journal(None, Some((8, 12345))));
    assert!(!stderr_is_journal(Some("8:1"), None));
}

/// The reply names the journal, a file, or nothing.
#[test]
fn the_target_is_the_journal_a_file_or_nothing() {
    let file = Some(PathBuf::from(
        "/home/user/.local/state/svolder/svolderd.log",
    ));
    assert_eq!(target_from(true, file.clone()).as_deref(), Some("journal"));
    assert_eq!(
        target_from(false, file).as_deref(),
        Some("/home/user/.local/state/svolder/svolderd.log")
    );
    assert_eq!(target_from(false, None), None);
}
