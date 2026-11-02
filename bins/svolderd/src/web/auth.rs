// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Signing a browser in: the web token compared in constant time, and the stateless cookie it
//! earns — `svolder_web_<port>=v1.<login seconds>.<mac>`, the MAC an HMAC-SHA256 keyed by the
//! token over `svolder-web/v1/<login seconds>` — renewed on each page load with the same login
//! time and refused three years after it. Nothing is stored: a rotated token signs every browser
//! out, and a restart signs nobody out. And the checks every POST meets: its `Origin`.

use hyper::HeaderMap;
use hyper::header::{self, HeaderValue};
use ring::hmac;
use svolder_ipc::config::normal_origin;
use svolder_ipc::token::{base64url, token_matches};

use super::Web;

/// How long a browser keeps the cookie, in seconds: 400 days, the cap Chromium applies and the
/// cookie standard's revision asks of every browser. Each page load renews it.
pub(crate) const COOKIE_MAX_AGE: u64 = 34_560_000;

/// How long a sign-in lasts from its login, in seconds: three years.
pub(crate) const LOGIN_LIFE: u64 = 1096 * 86_400;

/// The largest body a POST may carry.
pub(crate) const MAX_BODY: usize = 16 * 1024;

/// The cookie's name for a server on `port`: cookies ignore ports, so two daemons on one host
/// keep theirs apart by name.
pub(crate) fn cookie_name(port: u16) -> String {
    format!("svolder_web_{port}")
}

/// The cookie's value for a login at `login` seconds, signed with `token`.
pub(crate) fn mint(token: &str, login: u64) -> String {
    format!("v1.{login}.{}", mac(token, login))
}

/// The MAC of a login at `login` seconds under `token`, base64url.
fn mac(token: &str, login: u64) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, token.as_bytes());
    base64url(hmac::sign(&key, format!("svolder-web/v1/{login}").as_bytes()).as_ref())
}

/// What a cookie's value says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Check {
    /// Signed in since this login time.
    Valid(u64),
    /// Signed with this token, but more than [`LOGIN_LIFE`] ago.
    Ended,
    /// Not this server's: another token, another format, or tampered with.
    Invalid,
}

/// Check `value` against `token` at `now` seconds. The MAC is compared in constant time. A
/// login time ahead of `now` (a clock stepped back) is still signed in.
pub(crate) fn check(token: &str, value: &str, now: u64) -> Check {
    let mut parts = value.split('.');
    let (Some("v1"), Some(secs), Some(given), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Check::Invalid;
    };
    if secs.is_empty() || !secs.bytes().all(|b| b.is_ascii_digit()) {
        return Check::Invalid;
    }
    let Ok(login) = secs.parse::<u64>() else {
        return Check::Invalid;
    };
    if !token_matches(&mac(token, login), given) {
        return Check::Invalid;
    }
    match now.saturating_sub(login) > LOGIN_LIFE {
        true => Check::Ended,
        false => Check::Valid(login),
    }
}

/// The wall clock, in unix seconds.
pub(crate) fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Where a request stands with its cookie.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Cookie {
    /// A valid sign-in, with the `Set-Cookie` that renews it: minted with the very token the
    /// cookie was checked against, so a token replaced meanwhile cannot be handed a cookie.
    Signed {
        /// The renewal, with the same login time; none when it cannot be written.
        renewal: Option<HeaderValue>,
    },
    /// This server's cookie, three years old.
    Ended,
    /// None, or none of this server's.
    Absent,
}

/// What the cookie in a request says, with the token it was checked against.
pub(crate) enum Seen {
    /// A valid sign-in from `login`, checked against `token`.
    Valid { login: u64, token: String },
    /// This server's cookie, three years old.
    Ended,
    /// None, or none of this server's.
    Absent,
}

/// The server's own cookie in `headers`, checked against its token (read once, here). A server
/// with no token signs nobody in by cookie. Mints nothing: for who is asking, not for renewing.
pub(crate) fn seen(web: &Web, headers: &HeaderMap) -> Seen {
    let Some(key) = web.key.as_ref() else {
        return Seen::Absent;
    };
    let name = cookie_name(web.bound.port());
    let found = headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|line| line.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(n, _)| *n == name)
        .map(|(_, value)| value.to_string());
    let token = key.token();
    match found.map(|value| check(&token, &value, now_secs())) {
        Some(Check::Valid(login)) => Seen::Valid { login, token },
        Some(Check::Ended) => Seen::Ended,
        _ => Seen::Absent,
    }
}

/// The sign-in `headers` carry for `web` ([`seen`]), with the renewal minted under the very
/// token the cookie was checked against.
pub(crate) fn cookie(web: &Web, headers: &HeaderMap) -> Cookie {
    match seen(web, headers) {
        Seen::Valid { login, token } => Cookie::Signed {
            renewal: set_cookie(web, headers, &token, login),
        },
        Seen::Ended => Cookie::Ended,
        Seen::Absent => Cookie::Absent,
    }
}

/// Whether the cookie goes out with `Secure`: the page's address is `https`, or a proxy says
/// the browser came over it. A client that lies about the second only loses its own cookie.
fn secure(web: &Web, headers: &HeaderMap) -> bool {
    let proxied = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .and_then(|chain| chain.split(',').next())
        .is_some_and(|proto| proto.trim().eq_ignore_ascii_case("https"));
    proxied
        || web
            .config
            .url
            .as_deref()
            .is_some_and(|url| url.starts_with("https://"))
}

/// The `Set-Cookie` that signs a browser in from `login`, signed with `token`: the one the
/// caller checked the request against. No `Path` and no `Domain`: set only by root-level URLs,
/// it is scoped to wherever the page is mounted.
pub(crate) fn set_cookie(
    web: &Web,
    headers: &HeaderMap,
    token: &str,
    login: u64,
) -> Option<HeaderValue> {
    let mut line = format!(
        "{}={}; HttpOnly; SameSite=Strict; Max-Age={COOKIE_MAX_AGE}",
        cookie_name(web.bound.port()),
        mint(token, login)
    );
    if secure(web, headers) {
        line.push_str("; Secure");
    }
    HeaderValue::from_str(&line).ok()
}

/// The `Set-Cookie` that signs a browser out.
pub(crate) fn clear_cookie(web: &Web, headers: &HeaderMap) -> HeaderValue {
    let mut line = format!(
        "{}=; HttpOnly; SameSite=Strict; Max-Age=0",
        cookie_name(web.bound.port())
    );
    if secure(web, headers) {
        line.push_str("; Secure");
    }
    HeaderValue::from_str(&line).unwrap_or_else(|_| HeaderValue::from_static(""))
}

/// Whether a POST's `Origin` may send it: exactly one, an `http` or `https` origin whose host and
/// port are the request's own `Host`, or one `[web] origins` lists. None, `null` and any other
/// is refused.
pub(crate) fn origin_allowed(web: &Web, headers: &HeaderMap) -> bool {
    let mut origins = headers.get_all(header::ORIGIN).iter();
    let (Some(origin), None) = (origins.next(), origins.next()) else {
        return false;
    };
    let Some(origin) = origin.to_str().ok().and_then(normal_origin) else {
        return false;
    };
    if web.config.origins.contains(&origin) {
        return true;
    }
    let Some(host) = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };
    let scheme = origin.split("://").next().unwrap_or_default();
    normal_origin(&format!("{scheme}://{host}")).as_deref() == Some(origin.as_str())
}

#[cfg(test)]
mod tests;
