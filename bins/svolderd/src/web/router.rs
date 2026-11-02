// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The web UI's routes, every one relative to where the page is mounted, and the checks every
//! request meets first: `Host`, then the headers every answer carries (the CSP on HTML).

use askama::Template;
use axum::Router;
use axum::body::Body;
use axum::extract::{Path, Query, Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use hyper::header::{self, HeaderMap, HeaderValue};
use hyper::{Method, StatusCode};
use svolder_ipc::DeviceInfo;
use svolder_ipc::config::WebAuth;
use svolder_ipc::token::token_matches;

use super::auth::{self, MAX_BODY};
use super::codes::Paired;
use super::model;
use super::{Viewer, Web, api, events, guest, pages, themes, viewer};
use crate::locks::locked;
use crate::log;

/// The policy every HTML answer carries: nothing from elsewhere, no inline script or style,
/// no framing.
const CSP: &str = "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
                   font-src 'self'; connect-src 'self'; manifest-src 'self'; base-uri 'none'; \
                   form-action 'self'; frame-ancestors 'none'";

/// The web UI's router over `web`.
pub(crate) fn router(web: Web) -> Router {
    Router::new()
        .route("/", get(page))
        .route("/assets/{*file}", get(asset))
        .route("/manifest.webmanifest", get(manifest))
        .route("/api/v1/session", get(session))
        .route("/login", post(login))
        .route("/pair", post(pair))
        .route("/logout", post(logout))
        .merge(api::routes())
        .merge(super::host::routes())
        .merge(events::routes())
        .merge(super::writes::routes())
        .fallback(not_found)
        .layer(middleware::from_fn_with_state(web.clone(), guard))
        .with_state(web)
}

/// The web app manifest. Standalone needs a secure origin: over plain HTTP, "Add to Home
/// Screen" gives an icon that opens a browser tab.
async fn manifest() -> Response {
    let body = serde_json::json!({
        "name": "svolder",
        "short_name": "svolder",
        "start_url": "./",
        "scope": "./",
        "display": "standalone",
        "background_color": "#0f0d0b",
        "theme_color": "#0f0d0b",
        "icons": [
            { "src": "assets/icon-192.png", "sizes": "192x192", "type": "image/png" },
            { "src": "assets/icon-512.png", "sizes": "512x512", "type": "image/png",
              "purpose": "maskable" },
            { "src": "assets/icon.svg", "sizes": "any", "type": "image/svg+xml" },
        ],
    });
    (
        [(header::CONTENT_TYPE, "application/manifest+json")],
        body.to_string(),
    )
        .into_response()
}

/// Refuse a request that may not go on (see [`admitted`]), then set the headers every answer, a
/// refusal included, carries: no sniffing, no referrer, no framing, the same-origin policies,
/// `no-store` unless the answer chose its own caching, and the CSP on HTML.
async fn guard(State(web): State<Web>, request: Request, next: Next) -> Response {
    let mut response = match admitted(&web, request).await {
        Ok(request) => next.run(request).await,
        Err(refused) => refused,
    };
    secure(response.headers_mut());
    response
}

/// `request`, when it may go on: its `Host` is one the server answers to, and a POST names an
/// allowed `Origin` (403, nothing said), carries JSON (415) and at most [`MAX_BODY`] of it
/// (413). The body of a POST is read here, within the cap, and handed on whole.
async fn admitted(web: &Web, request: Request) -> Result<Request, Response> {
    let host = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok());
    if !host_allowed(&web.hosts, host) {
        return Err(bare(StatusCode::FORBIDDEN));
    }
    if request.method() != Method::POST {
        return Ok(request);
    }
    if !auth::origin_allowed(web, request.headers()) {
        return Err(bare(StatusCode::FORBIDDEN));
    }
    let json = request
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|kind| kind.split(';').next())
        .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("application/json"));
    if !json {
        return Err(error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "a request's body is JSON (Content-Type: application/json)",
        ));
    }
    let (parts, body) = request.into_parts();
    // The head has its timeout in the loop; the body gets the same, so a body that trickles or
    // stops cannot hold a connection.
    let read = axum::body::to_bytes(body, MAX_BODY);
    match tokio::time::timeout(crate::http::HEADER_TIMEOUT, read).await {
        Ok(Ok(bytes)) => Ok(Request::from_parts(parts, Body::from(bytes))),
        Ok(Err(e)) if too_long(&e) => Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "a request's body is at most 16 KiB",
        )),
        Ok(Err(_)) => Err(error(
            StatusCode::BAD_REQUEST,
            "a request's body could not be read",
        )),
        Err(_) => Err(error(
            StatusCode::REQUEST_TIMEOUT,
            "a request's body took too long to arrive",
        )),
    }
}

/// Set the headers every answer carries, the CSP on HTML.
fn secure(headers: &mut HeaderMap) {
    for (name, value) in [
        ("x-content-type-options", "nosniff"),
        ("referrer-policy", "no-referrer"),
        ("x-frame-options", "DENY"),
        ("cross-origin-opener-policy", "same-origin"),
        ("cross-origin-resource-policy", "same-origin"),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    if !headers.contains_key(header::CACHE_CONTROL) {
        headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    }
    let is_html = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|kind| kind.starts_with("text/html"));
    if is_html {
        headers.insert("content-security-policy", HeaderValue::from_static(CSP));
    }
}

/// Whether `host`, a request's `Host`, is one `allowed` lets through: any when the list is
/// empty (a wildcard bind with no `hosts`), else one entry equal to it, case aside, or an entry
/// with no port equal to its name.
fn host_allowed(allowed: &[String], host: Option<&str>) -> bool {
    if allowed.is_empty() {
        return true;
    }
    let Some(host) = host.map(str::to_ascii_lowercase) else {
        return false;
    };
    let name = without_port(&host);
    allowed.iter().any(|entry| {
        let entry = entry.to_ascii_lowercase();
        entry == host || (without_port(&entry) == entry && entry == name)
    })
}

/// `host` without its `:port`, an IPv6 address keeping its brackets.
fn without_port(host: &str) -> &str {
    match host.strip_prefix('[') {
        Some(_) => host
            .split_once("]:")
            .map_or(host, |(name, _)| &host[..name.len() + 1]),
        None => host.rsplit_once(':').map_or(host, |(name, _)| name),
    }
}

/// The page: its shell for a viewer who is signed in (renewing the cookie), the shell read-only
/// for a guest, else the sign-in page, which names the `Host` the browser asked for and says
/// when a sign-in has ended. A guest asks for the sign-in page with `?sign-in`.
async fn page(
    State(web): State<Web>,
    Query(query): Query<std::collections::HashMap<String, String>>,
    headers: HeaderMap,
) -> Response {
    // One look at the cookie decides everything: who is signed in, and the renewal minted under
    // the token it was checked against.
    let (viewer, renewal, ended) = match web.config.auth {
        WebAuth::None => (Some(Viewer::Signed), None, false),
        WebAuth::Token => match auth::cookie(&web, &headers) {
            auth::Cookie::Signed { renewal } => (Some(Viewer::Signed), renewal, false),
            auth::Cookie::Ended => (guest(&web), None, true),
            auth::Cookie::Absent => (guest(&web), None, false),
        },
    };
    let asks_to_sign_in = query.contains_key("sign-in");
    match viewer {
        Some(Viewer::Signed) => {
            let devices = devices(&web).await;
            let mut response =
                render(page_for(&web, Viewer::Signed, &devices, &query, &headers).await);
            if let Some(renewal) = renewal {
                response.headers_mut().insert(header::SET_COOKIE, renewal);
            }
            return response;
        }
        Some(Viewer::Guest) if !asks_to_sign_in => {
            let devices = devices(&web).await;
            return render(page_for(&web, Viewer::Guest, &devices, &query, &headers).await);
        }
        _ => {}
    }
    let mut response = render(pages::SignIn {
        host: host_of(&headers),
        ended,
    });
    if ended {
        response
            .headers_mut()
            .insert(header::SET_COOKIE, auth::clear_cookie(&web, &headers));
    }
    response
}

/// The devices the daemon lists, in its order; none when it cannot say.
pub(crate) async fn devices(web: &Web) -> Vec<DeviceInfo> {
    match web.fleet.request(svolder_ipc::Request::Devices).await {
        svolder_ipc::Response::Devices { devices } => devices,
        _ => Vec::new(),
    }
}

/// The page for `viewer` showing the device and wearing the theme `query` asks for (`?device=`,
/// `?theme=`): the device loaded once, with the strip from the daemon's ring for the first paint,
/// and the first snapshot of the model.
async fn page_for(
    web: &Web,
    viewer: Viewer,
    devices: &[DeviceInfo],
    query: &std::collections::HashMap<String, String>,
    headers: &HeaderMap,
) -> pages::Page {
    let shown = shown(web, devices, query.get("device").map(String::as_str));
    let loaded = shown
        .as_ref()
        .and_then(|info| api::load(web, &info.name).ok());
    let (theme, scheme) = themes::chosen(headers, query.get("theme").map(String::as_str));
    page_over(web, viewer, devices, shown, loaded, theme, scheme).await
}

/// The page for `viewer` over `shown`, the device the page shows, as `loaded` read it, in
/// `theme` and its `scheme`: the frame, carrying the first snapshot of the model for the theme
/// to draw from.
async fn page_over(
    web: &Web,
    viewer: Viewer,
    devices: &[DeviceInfo],
    shown: Option<DeviceInfo>,
    loaded: Option<api::Loaded>,
    theme: &'static themes::Theme,
    scheme: Option<&'static str>,
) -> pages::Page {
    let strip = match &loaded {
        Some(device) => api::strip_of(web, device, 300).await,
        None => None,
    };
    let model =
        model::page_model(web, viewer, devices, shown.as_ref(), loaded.as_ref(), strip).await;
    pages::Page {
        theme,
        scheme,
        guest: viewer == Viewer::Guest,
        writes: api::access(web, viewer).writes,
        page: api::embed(&model),
        themes: themes::list_json(),
    }
}

/// The device the page shows: the one asked for (`?device=`), else the default or only
/// device, else the first listed.
fn shown(web: &Web, devices: &[DeviceInfo], asked: Option<&str>) -> Option<DeviceInfo> {
    asked
        .and_then(|name| web.fleet.resolve(Some(name)).ok())
        .or_else(|| web.fleet.resolve(None).ok())
        .or_else(|| devices.first().cloned())
}

/// Whether `error`, from reading a body, is the length limit rather than a failure of the body.
fn too_long(error: &axum::Error) -> bool {
    let mut source = std::error::Error::source(error);
    while let Some(cause) = source {
        if cause.is::<http_body_util::LengthLimitError>() {
            return true;
        }
        source = cause.source();
    }
    false
}

/// What `login` takes.
#[derive(serde::Deserialize)]
struct Login {
    token: Option<String>,
    link: Option<String>,
}

/// `body` as `T`, or a 400 saying the body is not what the endpoint takes.
pub(crate) fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, Box<Response>> {
    serde_json::from_slice(body).map_err(|e| {
        Box::new(error(
            StatusCode::BAD_REQUEST,
            format!("the body is not what this takes: {e}"),
        ))
    })
}

/// Sign a browser in with the web token or a link's code: the cookie on the right one, a 401 on
/// any other. With `auth = "none"` every browser is signed in already.
async fn login(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    let login = match parse::<Login>(&body) {
        Ok(login) => login,
        Err(refused) => return *refused,
    };
    let Some(key) = web.key.as_ref() else {
        return axum::Json(serde_json::json!({ "signed_in": true })).into_response();
    };
    match (login.token, login.link) {
        (Some(given), None) => {
            // The token is read once: the one the given token is compared with signs the cookie.
            let token = key.token();
            if !token_matches(&token, given.trim()) {
                return error(
                    StatusCode::UNAUTHORIZED,
                    "That is not this svolder's token.",
                );
            }
            signed_in(&web, &headers, &token, auth::now_secs())
        }
        (None, Some(link)) => {
            // The token is read under the codes' lock, so a rotation cannot fall between
            // spending the code and signing the cookie.
            let token = {
                let mut codes = locked(&web.codes);
                if !codes.link(link.trim(), tokio::time::Instant::now()) {
                    return error(
                        StatusCode::UNAUTHORIZED,
                        "This link has been used or has ended. Make another with `svolder web`.",
                    );
                }
                key.token()
            };
            signed_in(&web, &headers, &token, auth::now_secs())
        }
        _ => error(
            StatusCode::BAD_REQUEST,
            "send a token or a link, one of them",
        ),
    }
}

/// What `pair` takes.
#[derive(serde::Deserialize)]
struct PairCode {
    code: String,
}

/// Sign a browser in with the pairing code `svolder web pair` printed.
async fn pair(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    let typed = match parse::<PairCode>(&body) {
        Ok(typed) => typed,
        Err(refused) => return *refused,
    };
    let Some(key) = web.key.as_ref() else {
        return axum::Json(serde_json::json!({ "signed_in": true })).into_response();
    };
    // A signed-in pairing reads the token under the codes' lock, as `login` does.
    let (paired, token) = {
        let mut codes = locked(&web.codes);
        let paired = codes.pair(&typed.code, tokio::time::Instant::now());
        let token = (paired == Paired::Signed).then(|| key.token());
        (paired, token)
    };
    match paired {
        Paired::Signed => signed_in(&web, &headers, &token.unwrap_or_default(), auth::now_secs()),
        Paired::Wrong(left) => error(
            StatusCode::UNAUTHORIZED,
            format!(
                "That is not the code shown. {left} {} left.",
                if left == 1 { "try" } else { "tries" }
            ),
        ),
        Paired::Ended => error(
            StatusCode::UNAUTHORIZED,
            "This code has ended. Make another with `svolder web pair`.",
        ),
    }
}

/// 200 `{"signed_in": true}` with the cookie for a login at `login`, signed with `token`.
fn signed_in(web: &Web, headers: &HeaderMap, token: &str, login: u64) -> Response {
    let mut response = axum::Json(serde_json::json!({ "signed_in": true })).into_response();
    if let Some(cookie) = auth::set_cookie(web, headers, token, login) {
        response.headers_mut().insert(header::SET_COOKIE, cookie);
    }
    response
}

/// Sign this browser out: its cookie cleared.
async fn logout(State(web): State<Web>, headers: HeaderMap) -> Response {
    let mut response = axum::Json(serde_json::json!({ "signed_in": false })).into_response();
    response
        .headers_mut()
        .insert(header::SET_COOKIE, auth::clear_cookie(&web, &headers));
    response
}

/// What the page's script asks before it shows a form: whether this browser is signed in, a
/// guest, and may change a device.
async fn session(State(web): State<Web>, headers: HeaderMap) -> Response {
    let viewer = viewer(&web, &headers);
    let signed_in = viewer == Some(Viewer::Signed);
    axum::Json(serde_json::json!({
        "signed_in": signed_in,
        "guest": viewer == Some(Viewer::Guest),
        "writes": signed_in && web.config.writes,
    }))
    .into_response()
}

/// One of the web UI's files, revalidated by its ETag.
async fn asset(State(web): State<Web>, Path(file): Path<String>, headers: HeaderMap) -> Response {
    let Some(asset) = web.assets.get(&file) else {
        return bare(StatusCode::NOT_FOUND);
    };
    let fresh = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|tags| tags.split(',').any(|tag| tag.trim() == asset.etag));
    let mut response = match fresh {
        true => bare(StatusCode::NOT_MODIFIED),
        false => {
            let mut response = Body::from(asset.bytes.into_owned()).into_response();
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static(asset.mime));
            response
        }
    };
    let headers = response.headers_mut();
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    if let Ok(tag) = HeaderValue::from_str(&asset.etag) {
        headers.insert(header::ETAG, tag);
    }
    response
}

/// Nothing at this path.
async fn not_found() -> Response {
    bare(StatusCode::NOT_FOUND)
}

/// The `Host` a request named, for the sign-in page's line.
fn host_of(headers: &HeaderMap) -> String {
    headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_string()
}

/// A template rendered as an HTML answer, or a bodiless 500 when it cannot be (logged).
pub(crate) fn render(template: impl Template) -> Response {
    match template.render() {
        Ok(text) => html(text),
        Err(e) => {
            log::error(&format!("web: a page could not be rendered: {e}"));
            bare(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// `text` as an HTML answer.
pub(crate) fn html(text: String) -> Response {
    let mut response = text.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

/// An answer of `status` with nothing in the body.
pub(crate) fn bare(status: StatusCode) -> Response {
    let mut response = Body::empty().into_response();
    *response.status_mut() = status;
    response
}

/// An answer of `status` with `{"error": message}` as its JSON body.
pub(crate) fn error(status: StatusCode, message: impl Into<String>) -> Response {
    let mut response = axum::Json(serde_json::json!({ "error": message.into() })).into_response();
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests;
