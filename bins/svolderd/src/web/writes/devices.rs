// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The configured devices from the page: list them as an editor needs them, add one, edit one
//! (name, profile, passphrase, each optional), remove one and make one the default. Each is the
//! daemon's `device_config` request ([`DeviceConfigOp`]), sent through the fleet as the socket
//! would send it, and answers `{"devices": [...], "profiles": [...], "config_changed": ...}`: the
//! configured devices after it, and the profiles a device can be pinned to, each `{"name"}` with
//! its `display` where the profile names a product and `"base": true` where it describes none of
//! its own.
//! The daemon judges an edit's `profile` against that same list, so the page offers what an
//! edit takes. The answer also carries `config_changed`: the daemon's words saying `config.toml`
//! changed since its last reload (a hand edit it has not taken), or `null`; a write's own answer
//! is `null` unless the file was edited again meanwhile. A write also tells every open page the
//! fleet changed.
//!
//! Only a signed-in writer reaches any of them, the list included: a guest is refused 401, a
//! page with `[web] writes` off 403. A passphrase travels in a request's body only, and no
//! answer carries one. A device is named in the path by its configured name or its unit id.
//!
//! A refusal is `{"error": words, "reason": word}` with the daemon's words and its reason:
//! 422 for `taken`, `invalid` and `bad_name`, 404 for `missing`, 409 for `read_only` and
//! `file_busy`, 503 for `stopping`, 500 for `internal`; one with no reason goes by the daemon's
//! code (`router::status_of`). A body that is not what a route takes is a 400 with no reason. A
//! body over [`BODY_MAX`] is the framework's own 413, plain text and not `{"error"}`: the page
//! cannot send one.

use axum::Router;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use hyper::{HeaderMap, StatusCode};
use svolder_ipc::{DeviceConfigOp, Request, Response as Answer};

use super::writer;
use crate::web::Web;
use crate::web::router::{error, refusal};

/// The longest body a device route reads, in bytes: a unit, a name, a profile and a passphrase
/// are a few hundred at most, and the body is read before the write gate is judged.
const BODY_MAX: usize = 4096;

/// The configured devices' routes.
pub(super) fn routes() -> Router<Web> {
    Router::new()
        .route("/api/v1/devices", get(list).post(add))
        .route("/api/v1/devices/{device}/edit", post(edit))
        .route("/api/v1/devices/{device}/remove", post(remove))
        .route("/api/v1/devices/{device}/default", post(make_default))
        .layer(axum::extract::DefaultBodyLimit::max(BODY_MAX))
}

/// The daemon's answer to `op` as the page's: the devices, or the refusal with its reason
/// ([`refusal`]).
async fn answer(web: &Web, op: DeviceConfigOp) -> Response {
    let wrote = !matches!(op, DeviceConfigOp::List);
    match web.fleet.request(Request::DeviceConfig { op }).await {
        Answer::DeviceConfig { devices, profiles } => {
            if wrote {
                web.fleet_changed.send_modify(|turn| *turn += 1);
            }
            let config_changed = web.fleet.config_changed_words().await;
            axum::Json(serde_json::json!({
                "devices": devices,
                "profiles": profiles,
                "config_changed": config_changed,
            }))
            .into_response()
        }
        other => refusal(other),
    }
}

/// The op a body asks for: the body's keys as the op's own, under `op` and, for an op on one
/// device, the device the path names. An empty body is an op with no keys. A body that is not
/// what the op takes is a 400 that names the key at fault and never what the body held, since a
/// body may hold a passphrase: the words are this function's and the op's own parser's, which
/// quote no value.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn op_of(kind: &str, device: Option<String>, body: &[u8]) -> Result<DeviceConfigOp, Response> {
    let refuse = |why: &str| {
        error(
            StatusCode::BAD_REQUEST,
            format!("the body is not what this takes: {why}"),
        )
    };
    let mut keys = match body {
        [] => serde_json::Map::new(),
        body => match serde_json::from_slice::<serde_json::Value>(body) {
            Ok(serde_json::Value::Object(keys)) => keys,
            Ok(_) => return Err(refuse("it must be a JSON object")),
            Err(_) => return Err(refuse("it is not JSON")),
        },
    };
    keys.insert("op".into(), kind.into());
    if let Some(device) = device {
        keys.insert("device".into(), device.into());
    }
    serde_json::from_value(serde_json::Value::Object(keys)).map_err(|e| refuse(&e.to_string()))
}

/// Answer the op of `kind` the request asks for, for a viewer who may write.
async fn run(
    web: &Web,
    headers: &HeaderMap,
    kind: &str,
    device: Option<String>,
    body: &[u8],
) -> Response {
    if let Err(refused) = writer(web, headers) {
        return refused;
    }
    match op_of(kind, device, body) {
        Ok(op) => answer(web, op).await,
        Err(refused) => refused,
    }
}

/// The configured devices, for an editor.
async fn list(State(web): State<Web>, headers: HeaderMap) -> Response {
    run(&web, &headers, "list", None, &[]).await
}

/// Configure a new device: `unit`, `name`, and `profile` and `passphrase` where they say
/// something.
async fn add(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    run(&web, &headers, "add", None, &body).await
}

/// Change a device: `name`, `profile` (`null` takes the pin off) and `passphrase`, each only
/// when the body carries it.
async fn edit(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(device): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    run(&web, &headers, "update", Some(device), &body).await
}

/// Take a device out of the configuration.
async fn remove(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(device): Path<String>,
) -> Response {
    run(&web, &headers, "remove", Some(device), &[]).await
}

/// Make a device the default.
async fn make_default(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(device): Path<String>,
) -> Response {
    run(&web, &headers, "set_default", Some(device), &[]).await
}

#[cfg(test)]
mod tests;
