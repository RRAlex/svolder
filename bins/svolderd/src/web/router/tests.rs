// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::sync::Arc;

use svolder_ipc::config::{WebAuth, WebConfig, WebGuest};

use super::page_for;
use crate::fleet::Unavailable;
use crate::web::test_support::{Code, call, data_block, get, open, page_model, web};
use crate::web::{Viewer, Web};

/// The CSP every HTML answer carries, word for word.
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
                   font-src 'self'; connect-src 'self'; manifest-src 'self'; base-uri 'none'; \
                   form-action 'self'; frame-ancestors 'none'";

/// A Host the server does not answer to is refused, with nothing in the body, whatever it asks.
#[tokio::test]
async fn a_foreign_host_is_refused_with_nothing_said() {
    let web = web(open(), Arc::new(Unavailable));
    for host in ["evil.example", "evil.example:8738", "127.0.0.1:8739", ""] {
        for path in [
            "/",
            "/assets/themes/ember/style.css",
            "/api/v1/session",
            "/nowhere",
        ] {
            let mut request = get(path);
            request.headers_mut().insert("host", host.parse().unwrap());
            let reply = call(&web, request).await;
            assert_eq!(reply.status, Code::FORBIDDEN, "{host} {path}");
            assert_eq!(reply.body, "", "{host} {path}");
            assert_eq!(reply.header("x-content-type-options"), Some("nosniff"));
            assert_eq!(reply.header("x-frame-options"), Some("DENY"));
            assert_eq!(reply.header("cache-control"), Some("no-store"));
            assert_eq!(reply.header("content-security-policy"), None);
        }
    }
    for host in [
        "127.0.0.1:8738",
        "localhost:8738",
        "[::1]:8738",
        "LOCALHOST:8738",
    ] {
        let mut request = get("/");
        request.headers_mut().insert("host", host.parse().unwrap());
        assert_eq!(call(&web, request).await.status, Code::OK, "{host}");
    }
}

/// A name `hosts` lists is a Host the server answers to, with any port when it names none.
#[tokio::test]
async fn a_configured_host_name_is_answered() {
    let config = WebConfig {
        hosts: vec!["svolder.home.lan".into()],
        ..open()
    };
    let web = web(config, Arc::new(Unavailable));
    for host in ["svolder.home.lan", "svolder.home.lan:443"] {
        let mut request = get("/");
        request.headers_mut().insert("host", host.parse().unwrap());
        assert_eq!(call(&web, request).await.status, Code::OK, "{host}");
    }
}

/// The host of `[web] url`, the address the printed link and QR code carry, is a Host the server
/// answers to when the list is not open: a phone that follows the link gets the page, not 403.
#[tokio::test]
async fn the_host_of_the_configured_url_is_answered() {
    let config = WebConfig {
        url: Some("https://svolder.home.lan:8443/".into()),
        ..open()
    };
    let web = web(config, Arc::new(Unavailable));
    for host in ["svolder.home.lan:8443", "SVOLDER.home.lan:8443"] {
        let mut request = get("/");
        request.headers_mut().insert("host", host.parse().unwrap());
        assert_eq!(call(&web, request).await.status, Code::OK, "{host}");
    }
    let mut request = get("/");
    request
        .headers_mut()
        .insert("host", "evil.example".parse().unwrap());
    assert_eq!(call(&web, request).await.status, Code::FORBIDDEN);
}

/// Every HTML answer carries the CSP; every answer carries the other headers; a page and the
/// API are never cached.
#[tokio::test]
async fn every_answer_carries_the_headers_and_html_the_csp() {
    let web = web(open(), Arc::new(Unavailable));
    let page = call(&web, get("/")).await;
    assert_eq!(page.status, Code::OK);
    assert_eq!(
        page.header("content-type"),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(page.header("content-security-policy"), Some(CSP));
    assert_eq!(page.header("cache-control"), Some("no-store"));
    for reply in [
        page,
        call(&web, get("/api/v1/session")).await,
        call(&web, get("/nowhere")).await,
    ] {
        assert_eq!(reply.header("x-content-type-options"), Some("nosniff"));
        assert_eq!(reply.header("referrer-policy"), Some("no-referrer"));
        assert_eq!(reply.header("x-frame-options"), Some("DENY"));
        assert_eq!(
            reply.header("cross-origin-opener-policy"),
            Some("same-origin")
        );
        assert_eq!(
            reply.header("cross-origin-resource-policy"),
            Some("same-origin")
        );
        assert_eq!(reply.header("access-control-allow-origin"), None);
    }
    let session = call(&web, get("/api/v1/session")).await;
    assert_eq!(session.header("content-security-policy"), None);
    assert_eq!(session.header("cache-control"), Some("no-store"));
}

/// An asset is served with its type, an ETag of its bytes and `no-cache`; the same ETag asked
/// again is a 304 with no body; a name the server does not have is a 404 with none either.
#[tokio::test]
async fn assets_are_served_with_an_etag_and_revalidated() {
    let web = web(open(), Arc::new(Unavailable));
    let css = call(&web, get("/assets/themes/ember/style.css")).await;
    assert_eq!(css.status, Code::OK);
    assert_eq!(css.header("content-type"), Some("text/css; charset=utf-8"));
    assert_eq!(css.header("cache-control"), Some("no-cache"));
    let etag = css.header("etag").expect("an ETag").to_string();
    assert!(
        etag.starts_with('"') && etag.ends_with('"') && etag.len() == 18,
        "{etag}"
    );
    let mut again = get("/assets/themes/ember/style.css");
    again
        .headers_mut()
        .insert("if-none-match", etag.parse().unwrap());
    let again = call(&web, again).await;
    assert_eq!(again.status, Code::NOT_MODIFIED);
    assert_eq!(again.body, "");
    let font = call(&web, get("/assets/fonts/IBMPlexSans-Light-Latin1.woff2")).await;
    assert_eq!(font.header("content-type"), Some("font/woff2"));
    for path in [
        "/assets/nope.css",
        "/assets/style.css",
        "/assets/IBMPlexSans-Light-Latin1.woff2",
        "/assets/core/../themes/ember/style.css",
        "/assets/..%2fCargo.toml",
        "/assets/fonts%2fOFL.txt",
    ] {
        let reply = call(&web, get(path)).await;
        assert_eq!(reply.status, Code::NOT_FOUND, "{path}");
        assert_eq!(reply.body, "", "{path}");
    }
}

/// With no sign-in asked for, every visitor is signed in: the page's frame, and a session that
/// says so. With a token asked for and none shown, the sign-in page, naming the Host asked for
/// in escaped text.
#[tokio::test]
async fn the_page_is_the_shell_when_signed_in_and_the_sign_in_page_otherwise() {
    let open_web = web(open(), Arc::new(Unavailable));
    let page = call(&open_web, get("/")).await;
    assert!(page.body.contains("data-theme=\"ember\""), "{}", page.body);
    assert!(!page.body.contains("Experimental"), "{}", page.body);
    let session = call(&open_web, get("/api/v1/session")).await;
    let said: serde_json::Value = serde_json::from_str(&session.body).unwrap();
    assert_eq!(
        said,
        serde_json::json!({"signed_in": true, "guest": false, "writes": true})
    );

    let config = WebConfig {
        enabled: true,
        auth: WebAuth::Token,
        hosts: vec!["<b>x</b>".into()],
        ..WebConfig::default()
    };
    let token_web = web(config, Arc::new(Unavailable));
    let mut request = get("/");
    request
        .headers_mut()
        .insert("host", "<b>x</b>".parse().unwrap());
    let sign_in = call(&token_web, request).await;
    assert_eq!(sign_in.status, Code::OK);
    assert!(sign_in.body.contains("Sign in"), "{}", sign_in.body);
    // askama's escaper writes `<` and `>` as numeric references.
    assert!(
        sign_in.body.contains("&#60;b&#62;x&#60;/b&#62;"),
        "{}",
        sign_in.body
    );
    assert!(!sign_in.body.contains("<b>x</b>"), "{}", sign_in.body);
    assert!(!sign_in.body.contains("data-theme"));
    let session = call(&token_web, get("/api/v1/session")).await;
    let said: serde_json::Value = serde_json::from_str(&session.body).unwrap();
    assert_eq!(said["signed_in"], false);
}

/// The page is the frame: the theme's root, the page's model and the list of themes as data, the
/// theme's module before the core's start, the head a theme needs; the screen is the theme's to
/// draw, so no element of it is in the served HTML.
#[tokio::test]
async fn the_page_is_the_frame_shell_with_its_model_and_theme() {
    let page = call(&web(open(), Arc::new(Unavailable)), get("/")).await;
    assert_eq!(page.status, Code::OK);
    let html = &page.body;
    assert!(html.trim_start().starts_with("<!doctype html>"), "{html}");
    for wanted in [
        r#"<html lang="en" data-theme="ember">"#,
        r#"<meta name="color-scheme" content="dark">"#,
        r##"<meta name="theme-color" content="#0f0d0b">"##,
        r#"<link rel="stylesheet" href="assets/core/frame.css">"#,
        r#"<link rel="stylesheet" href="assets/themes/ember/style.css" data-sheet="ember">"#,
        r#"<meta name="htmx-config" content="{&#34;includeIndicatorStyles&#34;:false,"#,
        r#"&#34;selfRequestsOnly&#34;:true"#,
        r#" data-sheet="ember">"#,
        r#"<script src="assets/themes/ember/htmx.min.js"></script>"#,
        r#"<script src="assets/themes/ember/viz-pref.js"></script>"#,
        r#"<div id="theme"></div>"#,
        r#"<template id="svolder-wordmark">"#,
        r#"id="svolder-page""#,
        r#"id="svolder-themes""#,
    ] {
        assert!(html.contains(wanted), "{wanted}: {html}");
    }
    let (theme, boot) = (
        html.find(r#"<script type="module" src="assets/themes/ember/ember.js"></script>"#),
        html.find(r#"<script type="module" src="assets/core/boot.js"></script>"#),
    );
    assert!(
        theme.is_some() && theme < boot && boot < html.find("</head>"),
        "{html}"
    );
    assert!(
        !html.contains("id=\"screen\""),
        "no screen in the frame: {html}"
    );
    assert!(
        !html.contains("hx-"),
        "no htmx attribute in the frame: {html}"
    );
    // The data: the model of the page (nothing shown, no device) and the themes' list.
    let model = page_model(html);
    assert_eq!(model["v"], 1);
    assert_eq!(model["viewer"]["writes"], true);
    let themes = data_block(html, "svolder-themes");
    assert_eq!(themes[0]["id"], "ember");
    assert_eq!(themes[0]["module"], "themes/ember/ember.js");
    // The module and its page's start are served.
    for name in ["themes/ember/ember.js", "core/boot.js", "core/core.js"] {
        let script = call(
            &web(open(), Arc::new(Unavailable)),
            get(&format!("/assets/{name}")),
        )
        .await;
        assert_eq!(script.status, Code::OK, "{name}");
    }
}

/// The frame's page for the request `uri` carrying the cookie `cookie`, if any.
async fn frame_for(uri: &str, cookie: Option<&str>) -> String {
    let web = web(open(), Arc::new(Unavailable));
    let mut request = get(uri);
    if let Some(cookie) = cookie {
        request
            .headers_mut()
            .insert("cookie", cookie.parse().unwrap());
    }
    call(&web, request).await.body
}

/// A cookie that says dark paper paints dark paper in the first byte: its sheet, its one browser
/// colour.
#[tokio::test]
async fn a_paper_dark_cookie_paints_dark_paper_first() {
    let page = frame_for("/", Some("svolder_theme=paper.dark")).await;
    assert!(
        page.contains(r#"<html lang="en" data-theme="paper" data-scheme="dark">"#),
        "{page}"
    );
    assert!(page.contains("assets/themes/paper/style.css"), "{page}");
    assert!(
        !page.contains("assets/themes/ember/style.css"),
        "no Ember sheet: {page}"
    );
    assert!(
        page.contains(r#"<script type="module" src="assets/themes/paper/paper.js"></script>"#),
        "{page}"
    );
    assert_eq!(page.matches(r#"name="theme-color""#).count(), 1, "{page}");
    assert!(
        page.contains(r##"<meta name="theme-color" content="#1e1f22">"##),
        "{page}"
    );
}

/// A theme the page does not ship, in the cookie or in the address, is Ember; a scheme Ember does
/// not have is dropped.
#[tokio::test]
async fn an_unknown_theme_cookie_or_query_serves_ember() {
    for (uri, cookie) in [
        ("/", Some("svolder_theme=ember")),
        ("/", Some("svolder_theme=ember.dark")),
        ("/", Some("svolder_theme=ember.light")),
        ("/", Some("svolder_theme=nope")),
        ("/?theme=nope", None),
        ("/?theme=nope", Some("svolder_theme=nope")),
        ("/?theme=", None),
    ] {
        let page = frame_for(uri, cookie).await;
        assert!(
            page.contains(r#"data-theme="ember""#),
            "{uri} {cookie:?}: {page}"
        );
        assert!(
            !page.contains("data-scheme"),
            "{uri} {cookie:?}: Ember has one scheme: {page}"
        );
    }
}

/// A painted scheme gives the page that scheme and its browser colour, once.
#[tokio::test]
async fn a_painted_scheme_sets_the_pages_colours() {
    let page = frame_for("/", Some("svolder_theme=probe.dark")).await;
    assert!(
        page.contains(r#"<meta name="color-scheme" content="dark">"#),
        "{page}"
    );
    assert_eq!(page.matches(r#"name="theme-color""#).count(), 1, "{page}");
    assert!(
        page.contains(r##"<meta name="theme-color" content="#000000">"##),
        "{page}"
    );
}

/// With no scheme painted, a theme with a dark colour offers both, by the browser's own scheme.
#[tokio::test]
async fn an_auto_page_offers_both_browser_colours() {
    let page = frame_for("/", Some("svolder_theme=probe")).await;
    assert!(
        page.contains(r#"<meta name="color-scheme" content="light dark">"#),
        "{page}"
    );
    let light =
        r##"<meta name="theme-color" content="#ffffff" media="(prefers-color-scheme: light)">"##;
    let dark =
        r##"<meta name="theme-color" content="#000000" media="(prefers-color-scheme: dark)">"##;
    let (at_light, at_dark) = (page.find(light), page.find(dark));
    assert!(at_light.is_some() && at_dark.is_some(), "{page}");
    assert!(at_light < at_dark, "light first: {page}");
    assert_eq!(page.matches(r#"name="theme-color""#).count(), 2, "{page}");
}

/// The cookie picks the theme the first paint wears, and `?theme=` beats it when it names a theme
/// the page has; a name it does not have leaves the cookie's choice.
#[tokio::test]
async fn the_theme_cookie_picks_the_first_paint() {
    let probe = Some("svolder_theme=probe");
    let page = frame_for("/", probe).await;
    assert!(page.contains(r#"data-theme="probe""#), "{page}");
    assert!(
        !page.contains("data-scheme"),
        "a cookie with no scheme: {page}"
    );
    let page = frame_for("/", Some("svolder_theme=probe.dark")).await;
    assert!(page.contains(r#"data-scheme="dark""#), "{page}");
    let page = frame_for("/?theme=ember", probe).await;
    assert!(page.contains(r#"data-theme="ember""#), "{page}");
    let page = frame_for("/?theme=probe", None).await;
    assert!(page.contains(r#"data-theme="probe""#), "{page}");
    let page = frame_for("/?theme=probe.light", Some("svolder_theme=probe.dark")).await;
    assert!(page.contains(r#"data-scheme="light""#), "{page}");
    let page = frame_for("/?theme=probe", Some("svolder_theme=probe.dark")).await;
    assert!(
        page.contains(r#"data-scheme="dark""#),
        "the cookie's scheme for the same theme: {page}"
    );
    let page = frame_for("/?theme=nope", probe).await;
    assert!(
        page.contains(r#"data-theme="probe""#),
        "an unknown name leaves the cookie's choice: {page}"
    );
}

/// Only GET and POST are routed: another method on a page is refused, and nothing answers a
/// path the server does not have.
#[tokio::test]
async fn other_methods_and_paths_are_refused() {
    let web = web(open(), Arc::new(Unavailable));
    for method in ["PUT", "DELETE", "PATCH", "OPTIONS"] {
        let request = hyper::Request::builder()
            .method(method)
            .uri("/")
            .header("host", crate::web::test_support::BOUND)
            .body(axum::body::Body::empty())
            .unwrap();
        let reply = call(&web, request).await;
        assert_eq!(reply.status, Code::METHOD_NOT_ALLOWED, "{method}");
    }
    assert_eq!(call(&web, get("/mcp")).await.status, Code::NOT_FOUND);
}

/// No template and no answer the server builds carries an inline script or a `style=`
/// attribute: the CSP would block it.
#[tokio::test]
async fn no_page_carries_an_inline_script_or_style() {
    let open_web = web(open(), Arc::new(Unavailable));
    let token_web = web(
        WebConfig {
            enabled: true,
            ..WebConfig::default()
        },
        Arc::new(Unavailable),
    );
    for body in [
        call(&open_web, get("/")).await.body,
        call(&token_web, get("/")).await.body,
    ] {
        assert!(!body.contains("style="), "{body}");
        assert!(!body.contains("<script>"), "{body}");
        assert_eq!(handler_attributes(&body), Vec::<String>::new(), "{body}");
        for script in body.match_indices("<script") {
            let tag: String = body[script.0..].chars().take_while(|c| *c != '>').collect();
            assert!(
                tag.contains("src=") || tag.contains("type=\"application/json\""),
                "{tag}"
            );
        }
    }
}

/// Every attribute named `on…` inside a tag of `html`: an inline event handler, which the CSP
/// blocks and no page may carry.
fn handler_attributes(html: &str) -> Vec<String> {
    html.split('<')
        .skip(1)
        .filter_map(|tag| tag.split_once('>').map(|(inside, _)| inside))
        .flat_map(|inside| inside.split_whitespace().skip(1))
        .filter(|attribute| {
            let name = attribute.split('=').next().unwrap_or_default();
            name.len() > 2 && name.to_ascii_lowercase().starts_with("on")
        })
        .map(str::to_string)
        .collect()
}

/// A server bound to a wildcard address with no `hosts` answers whatever Host a request names:
/// the sign-in is its check.
#[tokio::test]
async fn a_wildcard_bind_with_no_hosts_answers_any_host() {
    let web = Web::new(
        open(),
        "0.0.0.0:8738".parse().unwrap(),
        Arc::new(Unavailable),
        None,
        crate::web::assets::Assets::Embedded,
        Default::default(),
    );
    for host in ["evil.example", "192.0.2.7:8738", ""] {
        let mut request = get("/");
        request.headers_mut().insert("host", host.parse().unwrap());
        assert_eq!(call(&web, request).await.status, Code::OK, "{host}");
    }
}

use crate::web::auth::{COOKIE_MAX_AGE, LOGIN_LIFE, mint, now_secs};
use crate::web::test_support::{post, web_with};

const TOKEN: &str = "web-router-test-token-0123456789abcdefghij";

fn token_config() -> WebConfig {
    WebConfig {
        enabled: true,
        ..WebConfig::default()
    }
}

/// `request` with `value` sent as this server's cookie.
fn with_cookie(
    mut request: hyper::Request<axum::body::Body>,
    value: &str,
) -> hyper::Request<axum::body::Body> {
    request.headers_mut().insert(
        "cookie",
        format!("other=1; svolder_web_8738={value}")
            .parse()
            .unwrap(),
    );
    request
}

/// The right token earns the cookie, with every attribute; a wrong one earns a 401 and none.
#[tokio::test]
async fn the_token_signs_a_browser_in_and_a_wrong_one_does_not() {
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    let wrong = call(&web, post("/login", r#"{"token":"nope"}"#)).await;
    assert_eq!(wrong.status, Code::UNAUTHORIZED);
    assert_eq!(wrong.header("set-cookie"), None);
    assert!(wrong.body.contains("\"error\""), "{}", wrong.body);
    let right = call(&web, post("/login", &format!(r#"{{"token":"{TOKEN}"}}"#))).await;
    assert_eq!(right.status, Code::OK, "{}", right.body);
    let cookie = right.header("set-cookie").expect("a cookie");
    assert!(cookie.starts_with("svolder_web_8738=v1."), "{cookie}");
    assert!(
        cookie.ends_with(&format!(
            "; HttpOnly; SameSite=Strict; Max-Age={COOKIE_MAX_AGE}"
        )),
        "{cookie}"
    );
    assert!(!cookie.contains("Path") && !cookie.contains("Domain") && !cookie.contains("Secure"));
    let value = cookie
        .split(';')
        .next()
        .and_then(|pair| pair.split_once('='))
        .map(|(_, v)| v.to_string())
        .unwrap();
    let session = call(&web, with_cookie(get("/api/v1/session"), &value)).await;
    let said: serde_json::Value = serde_json::from_str(&session.body).unwrap();
    assert_eq!(said["signed_in"], true);
}

/// A token pasted with its newline or spaces around it still signs in.
#[tokio::test]
async fn a_token_pasted_with_whitespace_still_signs_in() {
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    let body = format!(r#"{{"token":"  {TOKEN}\n"}}"#);
    assert_eq!(call(&web, post("/login", &body)).await.status, Code::OK);
}

/// A page load renews the cookie with the same login time; a three-year-old one shows the
/// sign-in page saying so and clears it; a rotated token signs the browser out.
#[tokio::test]
async fn a_page_load_renews_the_cookie_and_an_ended_one_says_so() {
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    let login = now_secs() - 86_400;
    let page = call(&web, with_cookie(get("/"), &mint(TOKEN, login))).await;
    assert!(page.body.contains("data-theme=\"ember\""), "{}", page.body);
    let renewed = page.header("set-cookie").expect("a renewal");
    assert!(
        renewed.starts_with(&format!("svolder_web_8738=v1.{login}.")),
        "{renewed}"
    );

    let old = now_secs() - LOGIN_LIFE - 10;
    let ended = call(&web, with_cookie(get("/"), &mint(TOKEN, old))).await;
    assert!(
        ended
            .body
            .contains("Your sign-in on this browser ended after three years. Sign in again."),
        "{}",
        ended.body
    );
    assert_eq!(
        ended.header("set-cookie"),
        Some("svolder_web_8738=; HttpOnly; SameSite=Strict; Max-Age=0")
    );

    let rotated = web_with(
        token_config(),
        Arc::new(Unavailable),
        Some("a-new-token-0123456789abcdefghijkl"),
    );
    let out = call(&rotated, with_cookie(get("/"), &mint(TOKEN, login))).await;
    assert!(out.body.contains("Sign in"), "{}", out.body);
    assert!(!out.body.contains("id=\"screen\""), "{}", out.body);
}

/// The cookie goes out `Secure` when the page's address is https, or a proxy says the browser
/// came over https.
#[tokio::test]
async fn the_cookie_is_secure_behind_https() {
    let https = WebConfig {
        url: Some("https://svolder.home.lan/".into()),
        ..token_config()
    };
    let web = web_with(https, Arc::new(Unavailable), Some(TOKEN));
    let right = call(&web, post("/login", &format!(r#"{{"token":"{TOKEN}"}}"#))).await;
    assert!(right.header("set-cookie").unwrap().ends_with("; Secure"));
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    let mut proxied = post("/login", &format!(r#"{{"token":"{TOKEN}"}}"#));
    proxied
        .headers_mut()
        .insert("x-forwarded-proto", "https".parse().unwrap());
    assert!(
        call(&web, proxied)
            .await
            .header("set-cookie")
            .unwrap()
            .ends_with("; Secure")
    );
}

/// Signing out clears the cookie.
#[tokio::test]
async fn signing_out_clears_the_cookie() {
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    let out = call(
        &web,
        with_cookie(post("/logout", "{}"), &mint(TOKEN, now_secs())),
    )
    .await;
    assert_eq!(out.status, Code::OK);
    assert_eq!(
        out.header("set-cookie"),
        Some("svolder_web_8738=; HttpOnly; SameSite=Strict; Max-Age=0")
    );
}

/// Every POST needs an `Origin` naming the request's own host and port, or one the config lists;
/// a JSON body; and at most 16 KiB of it.
#[tokio::test]
async fn every_post_meets_the_origin_type_and_size_checks() {
    let listed = WebConfig {
        origins: vec!["https://proxy.home.lan".into()],
        ..token_config()
    };
    let web = web_with(listed, Arc::new(Unavailable), Some(TOKEN));
    let body = format!(r#"{{"token":"{TOKEN}"}}"#);
    for origin in [
        None,
        Some("null"),
        Some("https://evil.example"),
        Some("http://127.0.0.1:8739"),
        Some("http://127.0.0.1:8738/x"),
    ] {
        let mut request = post("/login", &body);
        request.headers_mut().remove("origin");
        if let Some(origin) = origin {
            request
                .headers_mut()
                .insert("origin", origin.parse().unwrap());
        }
        let reply = call(&web, request).await;
        assert_eq!(reply.status, Code::FORBIDDEN, "{origin:?}");
        assert_eq!(reply.body, "", "{origin:?}");
    }
    let mut listed = post("/login", &body);
    listed
        .headers_mut()
        .insert("origin", "HTTPS://proxy.home.lan:443".parse().unwrap());
    assert_eq!(call(&web, listed).await.status, Code::OK);
    let mut form = post("/login", &format!("token={TOKEN}"));
    form.headers_mut().insert(
        "content-type",
        "application/x-www-form-urlencoded".parse().unwrap(),
    );
    let form = call(&web, form).await;
    assert_eq!(form.status, Code::UNSUPPORTED_MEDIA_TYPE);
    assert!(form.body.contains("\"error\""));
    let big = call(
        &web,
        post(
            "/login",
            &format!(r#"{{"token":"{}"}}"#, "a".repeat(17 * 1024)),
        ),
    )
    .await;
    assert_eq!(big.status, Code::PAYLOAD_TOO_LARGE);
    let mut charset = post("/login", &body);
    charset.headers_mut().insert(
        "content-type",
        "Application/JSON; charset=utf-8".parse().unwrap(),
    );
    assert_eq!(call(&web, charset).await.status, Code::OK);
}

/// With no sign-in asked for, `login` says the browser is signed in and sets no cookie.
#[tokio::test]
async fn with_no_auth_login_sets_no_cookie() {
    let web = web(open(), Arc::new(Unavailable));
    let reply = call(&web, post("/login", r#"{"token":"anything"}"#)).await;
    assert_eq!(reply.status, Code::OK);
    assert_eq!(reply.header("set-cookie"), None);
}

/// A body that is not what `login` takes is a 400 in the error shape.
#[tokio::test]
async fn a_body_that_is_not_what_login_takes_is_a_bad_request() {
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    let reply = call(&web, post("/login", "{not json")).await;
    assert_eq!(reply.status, Code::BAD_REQUEST);
    assert!(reply.body.contains("\"error\""), "{}", reply.body);
}

/// A POST whose body stops arriving is answered 408 once the header timeout has passed, so a
/// slow body cannot hold one of the loop's connections for ever; `login` needs no sign-in.
#[tokio::test(start_paused = true)]
async fn a_body_that_stops_arriving_is_refused_in_time() {
    use futures::StreamExt;
    let web = web(open(), Arc::new(Unavailable));
    let first = axum::body::Bytes::from_static(b"{");
    let stalled = futures::stream::once(async move { Ok::<_, std::io::Error>(first) })
        .chain(futures::stream::pending());
    let request = hyper::Request::post("/login")
        .header("host", crate::web::test_support::BOUND)
        .header(
            "origin",
            format!("http://{}", crate::web::test_support::BOUND),
        )
        .header("content-type", "application/json")
        .body(axum::body::Body::from_stream(stalled))
        .unwrap();
    let reply = call(&web, request).await;
    assert_eq!(reply.status, Code::REQUEST_TIMEOUT);
    assert!(reply.body.contains("\"error\""), "{}", reply.body);
}

/// The sign-in page loads its script, which is served; a signed-in page offers Sign out only
/// when the server asks for a token.
#[tokio::test]
async fn the_sign_in_page_loads_its_script_and_the_page_offers_sign_out() {
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    let page = call(&web, get("/")).await;
    assert!(
        page.body.contains("src=\"assets/core/login.js\""),
        "{}",
        page.body
    );
    let script = call(&web, get("/assets/core/login.js")).await;
    assert_eq!(script.status, Code::OK);
    assert_eq!(
        script.header("content-type"),
        Some("text/javascript; charset=utf-8")
    );
    let signed = call(&web, with_cookie(get("/"), &mint(TOKEN, now_secs()))).await;
    assert_eq!(page_model(&signed.body)["viewer"]["sign_out"], true);
    let open = call(&self::web(open(), Arc::new(Unavailable)), get("/")).await;
    assert_eq!(page_model(&open.body)["viewer"]["sign_out"], false);
}

/// A valid cookie under another port's name is not this server's: two daemons of one user can
/// share the web token file, and only the name keeps their browsers apart.
#[tokio::test]
async fn a_cookie_under_another_ports_name_does_not_sign_in() {
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    let mut request = get("/");
    request.headers_mut().insert(
        "cookie",
        format!("svolder_web_8739={}", mint(TOKEN, now_secs()))
            .parse()
            .unwrap(),
    );
    let page = call(&web, request).await;
    assert!(!page.body.contains("id=\"screen\""), "{}", page.body);
    assert!(page.body.contains("Sign in"), "{}", page.body);
}

/// A body that fails in transit is a 400 saying so, not the size limit's 413.
#[tokio::test]
async fn a_body_that_fails_in_transit_is_a_bad_request() {
    use futures::StreamExt;
    let web = web(open(), Arc::new(Unavailable));
    let first = axum::body::Bytes::from_static(b"{");
    let broken = futures::stream::once(async move { Ok::<_, std::io::Error>(first) }).chain(
        futures::stream::once(async { Err(std::io::Error::other("the connection broke")) }),
    );
    let request = hyper::Request::post("/login")
        .header("host", crate::web::test_support::BOUND)
        .header(
            "origin",
            format!("http://{}", crate::web::test_support::BOUND),
        )
        .header("content-type", "application/json")
        .body(axum::body::Body::from_stream(broken))
        .unwrap();
    let reply = call(&web, request).await;
    assert_eq!(reply.status, Code::BAD_REQUEST);
    assert!(reply.body.contains("\"error\""), "{}", reply.body);
    assert!(!reply.body.contains("16 KiB"), "{}", reply.body);
    assert!(reply.body.contains("could not be read"), "{}", reply.body);
}

/// A link code signs a browser in once; used again, it is refused in words that say how to get
/// another, so the page lands on its form rather than in a loop.
#[tokio::test]
async fn a_link_code_works_once_and_then_names_the_fix() {
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    let code = web.new_link().expect("a code");
    let first = call(&web, post("/login", &format!(r#"{{"link":"{code}"}}"#))).await;
    assert_eq!(first.status, Code::OK, "{}", first.body);
    assert!(first.header("set-cookie").is_some());
    let again = call(&web, post("/login", &format!(r#"{{"link":"{code}"}}"#))).await;
    assert_eq!(again.status, Code::UNAUTHORIZED);
    assert!(
        again
            .body
            .contains("This link has been used or has ended. Make another with `svolder web`."),
        "{}",
        again.body
    );
    assert_eq!(again.header("set-cookie"), None);
}

/// A token and a link together, or neither, are not a sign-in.
#[tokio::test]
async fn a_login_names_a_token_or_a_link_not_both() {
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    for body in [
        r#"{}"#.to_string(),
        format!(r#"{{"token":"{TOKEN}","link":"x"}}"#),
    ] {
        let reply = call(&web, post("/login", &body)).await;
        assert_eq!(reply.status, Code::BAD_REQUEST, "{body}");
        assert_eq!(reply.header("set-cookie"), None, "{body}");
    }
}

/// The pairing code signs a browser in from the page; a wrong one says how many tries are left;
/// an ended one names the fix.
#[tokio::test]
async fn the_pairing_code_signs_in_from_the_page() {
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    let code = web.new_pair().expect("a code");
    let wrong = if code == "000000" { "000001" } else { "000000" };
    let reply = call(&web, post("/pair", &format!(r#"{{"code":"{wrong}"}}"#))).await;
    assert_eq!(reply.status, Code::UNAUTHORIZED);
    assert!(reply.body.contains("4 tries left"), "{}", reply.body);
    let typed = format!("{} {}", &code[..3], &code[3..]);
    let reply = call(&web, post("/pair", &format!(r#"{{"code":"{typed}"}}"#))).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
    assert!(reply.header("set-cookie").is_some());
    let reply = call(&web, post("/pair", &format!(r#"{{"code":"{code}"}}"#))).await;
    assert!(
        reply
            .body
            .contains("This code has ended. Make another with `svolder web pair`."),
        "{}",
        reply.body
    );
}

/// The cookie a code earns is signed with the token, so the page takes it.
#[tokio::test]
async fn the_cookie_a_code_earns_opens_the_page() {
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    let code = web.new_link().expect("a code");
    let reply = call(&web, post("/login", &format!(r#"{{"link":"{code}"}}"#))).await;
    let cookie = reply.header("set-cookie").expect("a cookie");
    let value = cookie
        .split(';')
        .next()
        .and_then(|pair| pair.strip_prefix("svolder_web_8738="))
        .expect("the value");
    let page = call(&web, with_cookie(get("/"), value)).await;
    assert!(page.body.contains("data-theme=\"ember\""), "{}", page.body);
}

/// A rotated token ends every open code with every sign-in.
#[tokio::test]
async fn a_rotated_token_ends_every_open_code() {
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    let link = web.new_link().expect("a code");
    let pair = web.new_pair().expect("a code");
    web.rekey_value("a-new-token-0123456789abcdefghijkl", None);
    let reply = call(&web, post("/login", &format!(r#"{{"link":"{link}"}}"#))).await;
    assert_eq!(reply.status, Code::UNAUTHORIZED);
    let reply = call(&web, post("/pair", &format!(r#"{{"code":"{pair}"}}"#))).await;
    assert_eq!(reply.status, Code::UNAUTHORIZED);
}

/// The same token again ends no code.
#[tokio::test]
async fn the_same_token_again_keeps_the_open_codes() {
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    let link = web.new_link().expect("a code");
    web.rekey_value(TOKEN, None);
    let reply = call(&web, post("/login", &format!(r#"{{"link":"{link}"}}"#))).await;
    assert_eq!(reply.status, Code::OK, "{}", reply.body);
}

/// With guests allowed, a visitor without a sign-in gets the page read-only, with no version in
/// its model; `?sign-in` gets it the sign-in page; and `session` says it is a guest.
#[tokio::test]
async fn a_guest_gets_the_page_read_only_and_can_ask_to_sign_in() {
    let guests = WebConfig {
        guest: svolder_ipc::config::WebGuest::Read,
        ..token_config()
    };
    let web = web_with(guests, Arc::new(Unavailable), Some(TOKEN));
    let page = call(&web, get("/")).await;
    assert!(
        page.body.contains("<body class=\"guest\">"),
        "{}",
        page.body
    );
    assert!(
        !page.body.contains(env!("CARGO_PKG_VERSION")),
        "{}",
        page.body
    );
    let viewer = &page_model(&page.body)["viewer"];
    assert_eq!(viewer["guest"], true);
    assert_eq!(viewer["sign_out"], false);
    assert_eq!(viewer["version"], serde_json::Value::Null);
    let sign_in = call(&web, get("/?sign-in")).await;
    assert!(
        sign_in.body.contains("id=\"token-form\""),
        "{}",
        sign_in.body
    );
    let session = call(&web, get("/api/v1/session")).await;
    let said: serde_json::Value = serde_json::from_str(&session.body).unwrap();
    assert_eq!(
        said,
        serde_json::json!({"signed_in": false, "guest": true, "writes": false})
    );
    // Without guests, the same visitor gets the sign-in page.
    let closed = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    assert!(
        call(&closed, get("/"))
            .await
            .body
            .contains("id=\"token-form\"")
    );
}

/// A token that changes bumps the sign-out counter streams end on; the same token again does not.
#[tokio::test]
async fn only_a_changed_token_bumps_the_sign_out_counter() {
    let web = web_with(token_config(), Arc::new(Unavailable), Some(TOKEN));
    let mut watching = web.signed_out.subscribe();
    web.rekey_value(TOKEN, None);
    assert!(!watching.has_changed().unwrap());
    web.rekey_value("a-new-token-0123456789abcdefghijkl", None);
    assert!(watching.has_changed().unwrap());
    watching.mark_changed();
    let _ = watching.borrow_and_update();
    web.rekey_value("a-new-token-0123456789abcdefghijkl", None);
    assert!(!watching.has_changed().unwrap());
}

/// With two devices listed the model names each by its percent-encoded name and marks the one
/// shown; the page shows the one asked for.
#[tokio::test]
async fn the_shell_switches_between_devices_by_encoded_name() {
    let device = crate::web::test_support::fake_daemon("gungnir2-v2", "Living room/DAC").await;
    let web = web(open(), Arc::clone(&device.fleet));
    let mine = web.fleet.resolve(None).unwrap();
    let other = svolder_ipc::DeviceInfo {
        name: "Desk & more".to_string(),
        ..mine.clone()
    };
    let page = page_for(
        &web,
        Viewer::Signed,
        &[mine, other],
        &std::collections::HashMap::from([("device".to_string(), "Living room/DAC".to_string())]),
        &hyper::HeaderMap::new(),
    )
    .await;
    let model = data_block(&render_page(page), "svolder-page");
    let devices = model["fleet"]["devices"].as_array().expect("the devices");
    assert_eq!(
        devices
            .iter()
            .map(|d| d["url"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["Living%20room%2FDAC", "Desk%20%26%20more"]
    );
    assert_eq!(model["shown"], "Living%20room%2FDAC");
    assert_eq!(model["device"]["url"], "Living%20room%2FDAC");
    device.stop().await;
}

/// `page` as the HTML a browser is sent.
fn render_page(page: crate::web::pages::Page) -> String {
    use askama::Template;
    page.render().expect("the frame renders")
}

/// A guest's page starts the core, which spends a `#login=` link and asks the session once; the
/// script reads the fragment.
#[tokio::test]
async fn a_guests_page_carries_the_script_that_spends_a_link() {
    let web = web_with(
        WebConfig {
            enabled: true,
            guest: WebGuest::Read,
            ..WebConfig::default()
        },
        Arc::new(Unavailable),
        Some("a-test-token-0123456789abcdefghijklmn"),
    );
    let page = call(&web, get("/")).await;
    assert_eq!(page.status, Code::OK);
    assert!(page.body.contains("class=\"guest\""), "{}", page.body);
    assert!(
        page.body
            .contains(r#"<script type="module" src="assets/core/boot.js"></script>"#),
        "{}",
        page.body
    );
    let script = call(&web, get("/assets/core/boot.js")).await;
    assert_eq!(script.status, Code::OK);
    assert!(
        script.body.contains("#login="),
        "the page's script does not read a link"
    );
}

/// The manifest names the app, starts and scopes it at `./`, and lists its three icons; it is
/// served without a sign-in, as a browser fetches it without one.
#[tokio::test]
async fn the_manifest_names_the_app_and_its_icons() {
    let token = "a-test-token-0123456789abcdefghijklmn";
    let web = web_with(
        WebConfig {
            enabled: true,
            ..WebConfig::default()
        },
        Arc::new(Unavailable),
        Some(token),
    );
    let reply = call(&web, get("/manifest.webmanifest")).await;
    assert_eq!(reply.status, Code::OK);
    assert_eq!(
        reply.header("content-type"),
        Some("application/manifest+json")
    );
    let manifest: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(manifest["name"], "svolder");
    assert_eq!(manifest["short_name"], "svolder");
    assert_eq!(manifest["start_url"], "./");
    assert_eq!(manifest["scope"], "./");
    assert_eq!(manifest["display"], "standalone");
    assert_eq!(manifest["background_color"], "#0f0d0b");
    assert_eq!(manifest["theme_color"], "#0f0d0b");
    let icons = manifest["icons"].as_array().unwrap();
    let sources: Vec<&str> = icons.iter().map(|i| i["src"].as_str().unwrap()).collect();
    assert_eq!(
        sources,
        [
            "assets/icon-192.png",
            "assets/icon-512.png",
            "assets/icon.svg"
        ]
    );
    assert_eq!(icons[1]["purpose"], "maskable");
}

/// Both pages link the manifest, the icon and the theme colour.
#[tokio::test]
async fn the_pages_link_the_manifest_and_the_icon() {
    let token = "a-test-token-0123456789abcdefghijklmn";
    let signed_out = web_with(
        WebConfig {
            enabled: true,
            ..WebConfig::default()
        },
        Arc::new(Unavailable),
        Some(token),
    );
    let open_page = web(open(), Arc::new(Unavailable));
    for web in [&signed_out, &open_page] {
        let page = call(web, get("/")).await;
        for wanted in [
            r#"<link rel="manifest" href="manifest.webmanifest">"#,
            r#"<link rel="icon" href="assets/icon.svg" type="image/svg+xml">"#,
            r##"<meta name="theme-color" content="#0f0d0b">"##,
        ] {
            assert!(page.body.contains(wanted), "{wanted}");
        }
    }
}

/// The page's head runs the small script that tells the style sheet the visualizer is off, before
/// the theme's module and the core's start (so a reload does not show the card for a moment), and
/// it is served.
#[tokio::test]
async fn the_head_reads_the_visualizer_switch_before_the_first_paint() {
    let open_page = web(open(), Arc::new(Unavailable));
    let page = call(&open_page, get("/")).await.body;
    let head = page.find("</head>").expect("the head");
    let pref = page
        .find(r#"<script src="assets/themes/ember/viz-pref.js"></script>"#)
        .expect("the preference script");
    let module = page
        .find(r#"<script type="module" src="assets/themes/ember/ember.js"></script>"#)
        .expect("the theme's module");
    assert!(pref < module && module < head, "{page}");
    let script = call(&open_page, get("/assets/themes/ember/viz-pref.js")).await;
    assert!(script.body.contains("svolder.viz.2"), "{}", script.body);
}

/// The body says what the viewer may do: `writes` for one who may write, `guest` for a guest,
/// nothing for a read-only server's visitor; the keys the theme lists follow from the model's
/// viewer (`ember-parts.mjs` draws them).
#[tokio::test]
async fn the_body_class_follows_what_the_viewer_may_do() {
    let writer = web(open(), Arc::new(Unavailable));
    let page = call(&writer, get("/")).await;
    assert!(
        page.body.contains("<body class=\"writes\">"),
        "{}",
        page.body
    );
    assert_eq!(page_model(&page.body)["viewer"]["writes"], true);
    let read_only = web(
        WebConfig {
            writes: false,
            ..open()
        },
        Arc::new(Unavailable),
    );
    let page = call(&read_only, get("/")).await;
    assert!(page.body.contains("<body>"), "{}", page.body);
    let viewer = &page_model(&page.body)["viewer"];
    assert_eq!(viewer["writes"], false);
    assert_eq!(viewer["read_only"], "Read-only: [web] writes is off");
    let guests = WebConfig {
        guest: svolder_ipc::config::WebGuest::Read,
        ..token_config()
    };
    let guest = web_with(guests, Arc::new(Unavailable), Some(TOKEN));
    let page = call(&guest, get("/")).await;
    assert!(
        page.body.contains("<body class=\"guest\">"),
        "{}",
        page.body
    );
    assert_eq!(page_model(&page.body)["viewer"]["writes"], false);
}

/// A device that is listed but cannot be loaded this moment (a reload race) is shown as not
/// available, not as "No devices yet"; a daemon with no device says how to add one.
#[tokio::test]
async fn a_listed_device_that_cannot_be_loaded_is_not_no_devices() {
    let device = crate::web::test_support::fake_daemon("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&device.fleet));
    let info = web.fleet.resolve(None).unwrap();
    let page = crate::web::model::page_model(
        &web,
        Viewer::Signed,
        std::slice::from_ref(&info),
        Some(&info),
        None,
        None,
    )
    .await;
    let title = |page: &crate::web::model::PageModel| {
        page.card
            .as_ref()
            .map(|card| card.title.clone())
            .unwrap_or_default()
    };
    assert!(
        title(&page).contains("dac is not available"),
        "{}",
        title(&page)
    );
    assert!(page.device.is_none());
    let none = crate::web::model::page_model(&web, Viewer::Signed, &[], None, None, None).await;
    assert_eq!(title(&none), "No devices");
    device.stop().await;
}

/// The page of a daemon that watches the computer's player carries the Now playing card in its
/// model, with the fake track, its cover and the transport; one that does not carries none.
#[tokio::test]
async fn the_page_carries_now_playing_when_the_host_is_there() {
    use crate::web::test_support::{fake_daemon, fake_daemon_playing};
    let dac = fake_daemon_playing("gungnir2-v2", "dac").await;
    for _ in 0..200 {
        if dac
            .fleet
            .host_view()
            .playing
            .is_some_and(|p| p.cover.is_some())
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    let page = call(&web(open(), Arc::clone(&dac.fleet)), get("/"))
        .await
        .body;
    let model = page_model(&page);
    let playing = &model["device"]["playing"];
    let track = &playing["track"];
    assert_eq!(track["headline"], "Test Pressing");
    assert_eq!(track["artist"], "The Fakes");
    assert_eq!(track["source"], "Demo Tape · Fake Player");
    assert_eq!(track["status"], "playing");
    assert!(track["cover"].is_string(), "{track}");
    assert_eq!(playing["buttons"], true, "a writer has the transport");
    assert!(track["progress"]["read_at_ms"].is_number(), "{track}");
    assert_eq!(model["host"]["track"]["headline"], "Test Pressing");
    dac.stop().await;
    let quiet = fake_daemon("gungnir2-v2", "dac").await;
    let page = call(&web(open(), Arc::clone(&quiet.fleet)), get("/"))
        .await
        .body;
    assert!(
        page_model(&page)["device"]["playing"].is_null(),
        "no host, no card"
    );
    quiet.stop().await;
}

/// A guest, and a page with writes off, see the card without its buttons.
#[tokio::test]
async fn a_guest_and_a_read_only_page_see_now_playing_without_buttons() {
    let dac = crate::web::test_support::fake_daemon_playing("gungnir2-v2", "dac").await;
    let guests = web_with(
        WebConfig {
            enabled: true,
            guest: WebGuest::Read,
            ..WebConfig::default()
        },
        Arc::clone(&dac.fleet),
        Some("a-test-token-0123456789abcdefghijklmn"),
    );
    let read_only = web(
        WebConfig {
            writes: false,
            ..open()
        },
        Arc::clone(&dac.fleet),
    );
    for page in [
        call(&guests, get("/")).await.body,
        call(&read_only, get("/")).await.body,
    ] {
        let playing = &page_model(&page)["device"]["playing"];
        assert_eq!(
            playing["track"]["headline"], "Test Pressing",
            "the card shows"
        );
        assert_eq!(playing["buttons"], false, "{playing}");
    }
    dac.stop().await;
}

/// With no player the card is the one quiet line: no title, no cover, and no route to show
/// (`ember-parts.mjs` holds the same name for the markup).
#[tokio::test]
async fn with_no_player_the_card_is_one_quiet_line() {
    let dac = crate::web::test_support::fake_daemon_playing("gungnir2-v2", "dac").await;
    // The card of a host with no player, from a host view the test hands the screen.
    let Ok(loaded) = crate::web::api::load(&web(open(), Arc::clone(&dac.fleet)), "dac") else {
        panic!("the device loads");
    };
    let none = svolder_ipc::host::HostView {
        available: true,
        now_ms: 1_700_000_000_000,
        ..Default::default()
    };
    let loaded = crate::web::api::Loaded {
        host: none,
        ..loaded
    };
    let s = crate::web::view::screen(&loaded.snapshot(
        &web(open(), Arc::clone(&dac.fleet)),
        crate::web::view::Access {
            writes: true,
            guest: false,
        },
    ));
    let model = crate::web::model::device_model(&s);
    let playing = model.playing.expect("the card of a host that watches");
    assert!(playing.track.is_none(), "{playing:?}");
    assert!(playing.path.is_none(), "{playing:?}");
    dac.stop().await;
}

/// The path line is its verdict and then its hops, so a line too long for its box loses hop detail
/// and never the answer (`ember-parts.mjs` holds the case `the_verdict_chip_comes_before_the_hops` for the order the page draws them
/// in).
#[tokio::test]
async fn the_path_has_its_verdict_and_its_hops() {
    use svolder_host::{PlayerMatch, RouteFacts, SinkFacts, StreamFacts};
    let dac = crate::web::test_support::fake_daemon_playing("gungnir2-v2", "dac").await;
    let web = web(open(), Arc::clone(&dac.fleet));
    let Ok(loaded) = crate::web::api::load(&web, "dac") else {
        panic!("the device loads");
    };
    let mut host = dac.fleet.host_view();
    host.route = Some(RouteFacts {
        sink: Some(SinkFacts {
            description: "Schiit Gungnir 2".into(),
            rate_hz: Some(48_000),
            volume_milli: Some(540),
            usb_vendor_id: Some(svolder_ipc::host::SCHIIT_VENDOR_ID),
            usb_product: Some("Schiit Gungnir 2".into()),
            ..SinkFacts::default()
        }),
        player: Some(StreamFacts {
            rate_hz: Some(48_000),
            volume_milli: Some(1000),
            ..StreamFacts::default()
        }),
        player_match: PlayerMatch::Pid,
        ..RouteFacts::default()
    });
    let loaded = crate::web::api::Loaded { host, ..loaded };
    let access = crate::web::view::Access {
        writes: true,
        guest: false,
    };
    let s = crate::web::view::screen(&loaded.snapshot(&web, access));
    let model = crate::web::model::device_model(&s);
    let path = model.playing.and_then(|p| p.path).expect("the path");
    assert!(!path.words.is_empty(), "{path:?}");
    assert!(path.words.contains("computer volume 54 %"), "{path:?}");
    dac.stop().await;
}
