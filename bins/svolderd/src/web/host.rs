// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the page asks of the computer's side: the current cover and the transport presses. The
//! cover is the picture the daemon read for the followed player, served only under its hash
//! (`GET api/v1/host/cover?v=<hash>`, which a track's card names): any other hash, or none, is a
//! 404, and the picture is revalidated by its ETag. A press (`POST api/v1/host/press`, body
//! `{"button": "previous|play|pause|next"}`) is a write like any other: a guest is refused 401
//! and a page with `[web] writes` off 403; the daemon takes it from there. The frames stream
//! (`GET api/v1/host/frames`, see [`frames`]) carries what the computer plays, analysed, to the
//! page's visualizer.

use axum::Router;
use axum::body::Body;
use axum::extract::{Query, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use hyper::header::{self, HeaderValue};
use hyper::{HeaderMap, StatusCode};
use serde::Deserialize;
use svolder_ipc::{Request, Response as Answer};

use super::Web;
use super::api::reader;
use super::router::{bare, error, parse};
use super::writes::writer;

mod frames;

/// The buttons a press names.
const BUTTONS: [&str; 4] = ["previous", "play", "pause", "next"];

/// The host's routes.
pub(crate) fn routes() -> Router<Web> {
    Router::new()
        .route("/api/v1/host/cover", get(cover))
        .route("/api/v1/host/press", post(press))
        .route("/api/v1/host/frames", get(frames::frames))
}

/// What `cover` takes: the hash the card names.
#[derive(Deserialize)]
struct Asked {
    v: Option<String>,
}

/// The current cover when `v` is its hash; the picture's address changes with it, so the
/// browser may keep it, and an ETag lets it ask whether it still stands.
async fn cover(State(web): State<Web>, headers: HeaderMap, Query(asked): Query<Asked>) -> Response {
    if let Err(refused) = reader(&web, &headers) {
        return refused;
    }
    let Some((hash, cover)) = asked
        .v
        .and_then(|hash| web.fleet.host_cover(&hash).map(|cover| (hash, cover)))
    else {
        return bare(StatusCode::NOT_FOUND);
    };
    let etag = format!("\"{hash}\"");
    let fresh = headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|tags| etag_matches(tags, &etag));
    let mut response = match fresh {
        true => bare(StatusCode::NOT_MODIFIED),
        false => {
            let mut response = Body::from(cover.bytes).into_response();
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static(cover.mime));
            response
        }
    };
    let out = response.headers_mut();
    out.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, max-age=31536000, immutable"),
    );
    if let Ok(tag) = HeaderValue::from_str(&etag) {
        out.insert(header::ETAG, tag);
    }
    response
}

/// Whether an `If-None-Match` value names `etag`: `*`, or any of a comma list of validators, each
/// weak (`W/"…"`) or strong, since a proxy may weaken the tag it passes on.
fn etag_matches(header: &str, etag: &str) -> bool {
    header
        .split(',')
        .map(str::trim)
        .any(|tag| tag == "*" || tag.strip_prefix("W/").unwrap_or(tag) == etag)
}

/// What `press` takes.
#[derive(Deserialize)]
struct Press {
    button: String,
}

/// Press a button on the followed player, through the daemon: `{"ok": true}` when the player
/// took it, 409 with the daemon's words when there is no player or it did not.
async fn press(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let asked = match parse::<Press>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    if !BUTTONS.contains(&asked.button.as_str()) {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!(
                "no button {:?}; use previous, play, pause or next",
                asked.button
            ),
        );
    }
    match web
        .fleet
        .request(Request::HostPress {
            button: asked.button,
        })
        .await
    {
        Answer::Ack => axum::Json(serde_json::json!({ "ok": true })).into_response(),
        Answer::Error { message, .. } => error(StatusCode::CONFLICT, message),
        _ => error(StatusCode::BAD_GATEWAY, "the daemon gave no answer"),
    }
}

#[cfg(test)]
mod tests;
