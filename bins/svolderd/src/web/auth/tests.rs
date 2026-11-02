// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const TOKEN: &str = "web-unit-test-token-0123456789abcdefghijk";
const NOW: u64 = 1_800_000_000;

/// A cookie minted with the token checks out, with its login time; the MAC is base64url.
#[test]
fn a_minted_cookie_checks_out_with_its_login_time() {
    let value = mint(TOKEN, NOW - 60);
    assert!(value.starts_with(&format!("v1.{}.", NOW - 60)), "{value}");
    let mac = value.rsplit('.').next().unwrap();
    assert_eq!(mac.len(), 43, "{value}");
    assert!(
        mac.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    );
    assert_eq!(check(TOKEN, &value, NOW), Check::Valid(NOW - 60));
    assert_eq!(cookie_name(8738), "svolder_web_8738");
}

/// A login older than three years has ended; one a second younger has not.
#[test]
fn a_login_older_than_three_years_has_ended() {
    let old = NOW - LOGIN_LIFE - 1;
    assert_eq!(check(TOKEN, &mint(TOKEN, old), NOW), Check::Ended);
    let young = NOW - LOGIN_LIFE;
    assert_eq!(check(TOKEN, &mint(TOKEN, young), NOW), Check::Valid(young));
}

/// A clock stepped back after the sign-in leaves a login time ahead of it: still signed in,
/// with no underflow in the age.
#[test]
fn a_login_time_ahead_of_the_clock_still_signs_in() {
    let ahead = NOW + 3_600;
    assert_eq!(check(TOKEN, &mint(TOKEN, ahead), NOW), Check::Valid(ahead));
    assert_eq!(
        check(TOKEN, &mint(TOKEN, u64::MAX), 0),
        Check::Valid(u64::MAX)
    );
}

/// A cookie another daemon minted (another token), one a rotation left behind, and every
/// malformed value are refused.
#[test]
fn a_cookie_minted_by_another_daemon_is_refused() {
    let theirs = mint("another-daemons-token-0123456789abcdef", NOW);
    assert_eq!(check(TOKEN, &theirs, NOW), Check::Invalid);
    let good = mint(TOKEN, NOW);
    let (head, mac) = good.rsplit_once('.').unwrap();
    for bad in [
        String::new(),
        "v1".to_string(),
        format!("v2.{NOW}.{mac}"),
        format!("v1.{}.{mac}", NOW + 1),
        format!("v1.+{NOW}.{mac}"),
        format!("{head}.{}", &mac[1..]),
        format!("{head}.{mac}x"),
        format!("{good}.extra"),
    ] {
        assert_eq!(check(TOKEN, &bad, NOW), Check::Invalid, "{bad}");
    }
}

/// A renewal is minted from the token the cookie was checked with, in the same step: a token
/// replaced after the check cannot give the browser a cookie under the new one.
#[test]
fn a_renewal_is_minted_from_the_token_the_cookie_was_checked_with() {
    use crate::web::test_support::web_with;
    let web = web_with(
        svolder_ipc::config::WebConfig::default(),
        std::sync::Arc::new(crate::fleet::Unavailable),
        Some(TOKEN),
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        header::COOKIE,
        format!("svolder_web_8738={}", mint(TOKEN, NOW))
            .parse()
            .unwrap(),
    );
    let Cookie::Signed { renewal } = cookie(&web, &headers) else {
        panic!("not signed in");
    };
    web.key
        .as_ref()
        .unwrap()
        .replace_for_test("a-new-token-0123456789abcdefghijkl");
    let renewal = renewal.expect("a renewal");
    let value = renewal.to_str().unwrap();
    let minted = value
        .strip_prefix("svolder_web_8738=")
        .and_then(|rest| rest.split(';').next())
        .unwrap();
    assert_eq!(check(TOKEN, minted, NOW), Check::Valid(NOW));
    assert_eq!(
        check("a-new-token-0123456789abcdefghijkl", minted, NOW),
        Check::Invalid
    );
}

/// `X-Forwarded-Proto` names the browser's own scheme in its first value, however many proxies
/// the request crossed.
#[test]
fn a_chain_of_proxies_is_read_by_its_first_value() {
    use crate::web::test_support::web_with;
    let web = web_with(
        svolder_ipc::config::WebConfig::default(),
        std::sync::Arc::new(crate::fleet::Unavailable),
        Some(TOKEN),
    );
    for (value, secure) in [
        ("https", true),
        (" HTTPS , http", true),
        ("http, https", false),
        ("http", false),
    ] {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-proto", value.parse().unwrap());
        let line = set_cookie(&web, &headers, TOKEN, NOW).unwrap();
        assert_eq!(
            line.to_str().unwrap().ends_with("; Secure"),
            secure,
            "{value}"
        );
    }
}
