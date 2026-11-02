// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use tokio::time::Instant;

use super::*;

/// A pairing code is six digits and signs one browser in, once, spaces aside.
#[test]
fn a_pairing_code_signs_in_once() {
    let now = Instant::now();
    let mut codes = Codes::default();
    let code = codes.new_pair(now).expect("a code");
    assert_eq!(code.len(), 6);
    assert!(code.bytes().all(|b| b.is_ascii_digit()), "{code}");
    let typed = format!(" {} {} ", &code[..3], &code[3..]);
    assert_eq!(codes.pair(&typed, now), Paired::Signed);
    assert_eq!(codes.pair(&code, now), Paired::Ended);
}

/// A pairing code lasts five minutes.
#[test]
fn a_pairing_code_lasts_five_minutes() {
    assert_eq!(CODE_LIFE, Duration::from_secs(300));
    let now = Instant::now();
    let mut codes = Codes::default();
    let code = codes.new_pair(now).expect("a code");
    assert_eq!(
        codes.pair(&code, now + CODE_LIFE + Duration::from_millis(1)),
        Paired::Ended
    );
    let code = codes.new_pair(now).expect("a code");
    assert_eq!(codes.pair(&code, now + CODE_LIFE), Paired::Signed);
}

/// Five wrong tries end the code, whoever made them: the right one after them is refused.
#[test]
fn the_fifth_wrong_try_ends_the_code() {
    let now = Instant::now();
    let mut codes = Codes::default();
    let code = codes.new_pair(now).expect("a code");
    let wrong = if code == "000000" { "000001" } else { "000000" };
    for left in (1..PAIR_TRIES).rev() {
        assert_eq!(codes.pair(wrong, now), Paired::Wrong(left));
    }
    assert_eq!(codes.pair(wrong, now), Paired::Ended);
    assert_eq!(codes.pair(&code, now), Paired::Ended);
}

/// A new pairing code replaces the open one.
#[test]
fn a_new_pairing_code_replaces_the_open_one() {
    let now = Instant::now();
    let mut codes = Codes::default();
    let first = codes.new_pair(now).expect("a code");
    let second = codes.new_pair(now).expect("a code");
    if first != second {
        assert!(matches!(codes.pair(&first, now), Paired::Wrong(_)));
    }
    assert_eq!(codes.pair(&second, now), Paired::Signed);
}

/// A link code is 22 base64url characters, signs one browser in once, lasts five minutes, and
/// at most eight are open: a ninth ends the oldest.
#[test]
fn link_codes_are_one_use_five_minutes_and_eight_at_most() {
    let now = Instant::now();
    let mut codes = Codes::default();
    let first = codes.new_link(now).expect("a code");
    assert_eq!(first.len(), 22);
    assert!(
        first
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    );
    let mut more: Vec<String> = (0..MAX_LINKS)
        .map(|_| codes.new_link(now).expect("a code"))
        .collect();
    assert!(!codes.link(&first, now), "the ninth ended the oldest");
    let last = more.pop().expect("a code");
    assert!(codes.link(&last, now));
    assert!(!codes.link(&last, now), "used once");
    let late = codes.new_link(now).expect("a code");
    assert!(!codes.link(&late, now + CODE_LIFE + Duration::from_millis(1)));
    assert!(!codes.link("", now));
}

/// Asking whether a link code is open spends nothing: it stays open until it is used, and is not
/// open once it is used, has lasted its five minutes, was never made or is empty.
#[test]
fn asking_whether_a_link_is_open_spends_nothing() {
    let now = Instant::now();
    let mut codes = Codes::default();
    let link = codes.new_link(now).expect("a code");
    assert!(codes.link_open(&link, now));
    assert!(codes.link_open(&link, now), "asking twice spent it");
    assert!(codes.link_open(&link, now + CODE_LIFE));
    assert!(!codes.link_open(&link, now + CODE_LIFE + Duration::from_millis(1)));
    assert!(!codes.link_open("", now));
    assert!(!codes.link_open("AAAAAAAAAAAAAAAAAAAAAA", now));
    assert!(codes.link(&link, now), "asking left it to be used");
    assert!(!codes.link_open(&link, now), "a used code is not open");
}

/// Clearing the codes, as a rotated token does, ends every one.
#[test]
fn clearing_ends_every_code() {
    let now = Instant::now();
    let mut codes = Codes::default();
    let pair = codes.new_pair(now).expect("a code");
    let link = codes.new_link(now).expect("a code");
    codes.clear();
    assert_eq!(codes.pair(&pair, now), Paired::Ended);
    assert!(!codes.link(&link, now));
}

/// The six digits are uniform: no digit is favoured over a large draw.
#[test]
fn the_digits_are_spread_evenly() {
    let now = Instant::now();
    let mut codes = Codes::default();
    let mut counts = [0usize; 10];
    for _ in 0..2_000 {
        for digit in codes.new_pair(now).expect("a code").bytes() {
            counts[usize::from(digit - b'0')] += 1;
        }
    }
    // 12,000 digits: 1,200 each expected; 1,000 to 1,400 is far beyond chance either way.
    assert!(
        counts.iter().all(|n| (1_000..=1_400).contains(n)),
        "{counts:?}"
    );
}

/// Binding the same token again keeps the codes; another token, or none, ends them.
#[test]
fn another_token_ends_the_codes() {
    let now = Instant::now();
    let mut codes = Codes::default();
    codes.bind(Some("a"));
    let pair = codes.new_pair(now).expect("a code");
    let link = codes.new_link(now).expect("a code");
    codes.bind(Some("a"));
    assert!(codes.link(&link, now));
    codes.bind(Some("b"));
    assert_eq!(codes.pair(&pair, now), Paired::Ended);
    let link = codes.new_link(now).expect("a code");
    codes.bind(None);
    assert!(!codes.link(&link, now));
}
