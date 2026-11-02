// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The code matcher.

use std::time::{Duration, Instant};

use crossterm::event::KeyCode;

use super::super::codes::{Code, Codes, KONAMI_KEYS as KONAMI};

/// Feed `keys` 100 ms apart from `t0`; the last answer.
fn feed_all(codes: &mut Codes, keys: &[KeyCode], t0: Instant) -> Option<Code> {
    let mut last = None;
    for (n, key) in keys.iter().enumerate() {
        last = codes.feed(*key, t0 + Duration::from_millis(100 * n as u64));
    }
    last
}

fn letters(text: &str) -> Vec<KeyCode> {
    text.chars().map(KeyCode::Char).collect()
}

#[test]
fn konami_completes() {
    let mut codes = Codes::default();
    assert_eq!(
        feed_all(&mut codes, &KONAMI, Instant::now()),
        Some(Code::Konami)
    );
}

#[test]
fn iddqd_completes() {
    let mut codes = Codes::default();
    assert_eq!(
        feed_all(&mut codes, &letters("iddqd"), Instant::now()),
        Some(Code::Iddqd)
    );
}

#[test]
fn a_breaking_key_restarts_the_code() {
    let mut codes = Codes::default();
    assert_eq!(
        feed_all(&mut codes, &letters("idxiddqd"), Instant::now()),
        Some(Code::Iddqd)
    );
}

#[test]
fn a_repeated_first_key_still_completes_the_code() {
    let mut codes = Codes::default();
    let mut keys = vec![KeyCode::Up];
    keys.extend(KONAMI);
    assert_eq!(
        feed_all(&mut codes, &keys, Instant::now()),
        Some(Code::Konami)
    );
    let mut codes = Codes::default();
    assert_eq!(
        feed_all(&mut codes, &letters("iiddqd"), Instant::now()),
        Some(Code::Iddqd)
    );
    let mut codes = Codes::default();
    assert_eq!(
        feed_all(&mut codes, &letters("ididdqd"), Instant::now()),
        Some(Code::Iddqd)
    );
}

#[test]
fn a_gap_of_over_two_seconds_resets() {
    let mut codes = Codes::default();
    let t0 = Instant::now();
    feed_all(&mut codes, &letters("idd"), t0);
    let late = t0 + Duration::from_millis(2_100 + 200);
    assert_eq!(codes.feed(KeyCode::Char('q'), late), None);
    assert_eq!(codes.feed(KeyCode::Char('d'), late), None);
    let mut codes = Codes::default();
    feed_all(&mut codes, &letters("idd"), t0);
    assert_eq!(
        codes.feed(KeyCode::Char('q'), t0 + Duration::from_millis(1_900)),
        None
    );
    assert_eq!(
        codes.feed(KeyCode::Char('d'), t0 + Duration::from_millis(2_000)),
        Some(Code::Iddqd)
    );
}

#[test]
fn holds_idd_after_idd_and_not_after_id() {
    let mut codes = Codes::default();
    let t0 = Instant::now();
    feed_all(&mut codes, &letters("id"), t0);
    assert!(!codes.holds_idd(t0 + Duration::from_millis(300)));
    codes.feed(KeyCode::Char('d'), t0 + Duration::from_millis(300));
    assert!(codes.holds_idd(t0 + Duration::from_millis(400)));
    assert!(!codes.holds_idd(t0 + Duration::from_millis(2_400)));
    codes.clear();
    assert!(!codes.holds_idd(t0 + Duration::from_millis(400)));
}
