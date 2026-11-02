// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::{Duration, Instant};

use super::*;

fn addr(text: &str) -> std::net::SocketAddr {
    text.parse().unwrap()
}

/// Reach reads the listen address in words, and stepping it keeps the port.
#[test]
fn reach_reads_the_listen_address_and_steps_keeping_the_port() {
    assert_eq!(reach(addr("127.0.0.1:8738")).word(), "this computer");
    assert_eq!(reach(addr("0.0.0.0:8738")).word(), "your network");
    assert_eq!(
        reach(addr("192.168.1.5:9000")).word(),
        "custom (192.168.1.5:9000)"
    );
    assert_eq!(stepped_listen(addr("127.0.0.1:9000")), addr("0.0.0.0:9000"));
    assert_eq!(stepped_listen(addr("0.0.0.0:9000")), addr("127.0.0.1:9000"));
    assert_eq!(
        stepped_listen(addr("192.168.1.5:9000")),
        addr("127.0.0.1:9000")
    );
}

/// A link's code is what follows the last `#login=`; a pairing code, or text with no fragment, has
/// none, and a `#` in the base does not confuse it.
#[test]
fn a_links_code_is_read_from_its_fragment() {
    let code = |text: &str| Code {
        text: text.into(),
        until: Instant::now(),
    };
    let link = |base: &str| code(&format!("{base}#login=c3R1Yi1saW5r"));
    assert_eq!(
        link("http://192.168.1.20:8738/").link_code(),
        Some("c3R1Yi1saW5r")
    );
    assert_eq!(
        link("https://home.lan/amp/#section/").link_code(),
        Some("c3R1Yi1saW5r")
    );
    assert_eq!(code("482913").link_code(), None);
    assert_eq!(code("http://192.168.1.20:8738/").link_code(), None);
}

/// A code shows with its countdown in minutes and seconds, a pairing code in two threes, and
/// `ended` once its time is up.
#[test]
fn a_code_shows_its_countdown_then_ended() {
    let now = Instant::now();
    let pair = Code {
        text: "482913".into(),
        until: now + Duration::from_secs(118),
    };
    assert_eq!(code_line(&pair, now), "482 913 · 1:58 left");
    assert_eq!(code_line(&pair, now + Duration::from_secs(119)), "ended");
    let link = Code {
        text: "http://192.168.1.20:8738/#login=abc".into(),
        until: now + Duration::from_secs(61),
    };
    assert_eq!(timer(&link, now), "1:01 left");
    assert_eq!(timer(&link, now + Duration::from_secs(61)), "ended");
}

/// The State row's words: listening, off, failed with why, or no daemon.
#[test]
fn the_state_row_says_where_the_web_ui_stands() {
    use svolder_ipc::WebStatus;
    use svolder_ipc::config::{WebAuth, WebGuest};
    let listening = WebStatus::Listening {
        listen: addr("0.0.0.0:8738"),
        auth: WebAuth::Token,
        writes: true,
        guest: WebGuest::None,
    };
    assert_eq!(
        state_word(Some(&listening), Some(true), false),
        "listening on 0.0.0.0:8738"
    );
    assert_eq!(state_word(Some(&WebStatus::Off), Some(true), false), "off");
    let failed = WebStatus::Failed {
        listen: addr("0.0.0.0:8738"),
        error: "address in use".into(),
    };
    assert_eq!(
        state_word(Some(&failed), Some(true), false),
        "failed: address in use"
    );
    assert_eq!(state_word(None, Some(false), false), "no daemon");
    // A daemon session that has not been answered yet is waiting, not without a daemon.
    assert_eq!(state_word(None, None, true), "…");
    assert_eq!(state_word(None, None, false), "no daemon");
}

/// A code never prints in a debug dump: it signs a browser in.
#[test]
fn a_code_prints_as_stars_in_a_debug_dump() {
    let code = Code {
        text: "482913".into(),
        until: Instant::now(),
    };
    let dump = format!(
        "{code:?} {:?}",
        WebBox {
            link: Some(code.clone()),
            ..WebBox::default()
        }
    );
    assert!(!dump.contains("482913"), "{dump}");
}
