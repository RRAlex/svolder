// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The scenes from the page: the list, and save, recall, rename and delete. A writer only: a
//! guest is refused 401 and a page with `[web] writes` off 403, as for every write. Each route
//! sends one `Request::Scene` to the daemon as the web's origin and answers what the daemon
//! says: `{"name"}` (a recall adds `"outcome"`, one entry per part, and a save one for each
//! configured device that is not connected), or a refusal as
//! `{"error", "reason"}`: 422 for `bad_name` and `taken`, 409 for every other reason (`missing`,
//! `busy`, `nothing_to_save`, `read_only`, `file_busy`, `invalid`, `stopping`, `internal`). The
//! question a recall asks when a preset would play twice is a 409 with `ask`
//! (`{"kind": "both_sides", "name", "devices", "question"}`); yes sends the request again with
//! `"confirm": true`. A page acts on `reason` and shows `error` as words, never parses them.

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use hyper::{HeaderMap, StatusCode};
use serde::Deserialize;
use svolder_ipc::socket::protocol::{PartOutcome, Reason, SceneOp};
use svolder_ipc::{Request, Response as Answer};

use super::writer;
use crate::web::Web;
use crate::web::router::{error, parse};

/// The scenes' routes.
pub(super) fn routes() -> Router<Web> {
    Router::new()
        .route("/api/v1/scenes", get(list))
        .route("/api/v1/scenes/save", post(save))
        .route("/api/v1/scenes/recall", post(recall))
        .route("/api/v1/scenes/rename", post(rename))
        .route("/api/v1/scenes/delete", post(delete))
}

/// The scenes as the daemon keeps them and which one is on.
async fn list(State(web): State<Web>, headers: HeaderMap) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    match web.fleet.scenes() {
        Some(view) => axum::Json(view.borrow().clone()).into_response(),
        // A fleet with no scenes is a daemon that is going away.
        None => refused(Answer::Error {
            code: svolder_ipc::CODE_LINK,
            message: "the daemon is stopping".to_string(),
            reason: Some(Reason::Stopping),
        }),
    }
}

/// What `save` takes.
#[derive(Deserialize)]
struct SaveWrite {
    name: String,
    #[serde(default)]
    force: bool,
}

/// What `recall` takes.
#[derive(Deserialize)]
struct RecallWrite {
    name: String,
    #[serde(default)]
    confirm: bool,
}

/// What `rename` takes.
#[derive(Deserialize)]
struct RenameWrite {
    from: String,
    to: String,
}

/// What `delete` takes.
#[derive(Deserialize)]
struct DeleteWrite {
    name: String,
}

/// Save the settings as they are now under a name.
async fn save(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let asked = match parse::<SaveWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let op = SceneOp::Save {
        name: asked.name,
        force: asked.force,
    };
    saved(web.fleet.request(Request::Scene { op }).await)
}

/// Recall a scene.
async fn recall(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let asked = match parse::<RecallWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let op = SceneOp::Recall {
        name: asked.name,
        confirm: asked.confirm,
    };
    outcome(web.fleet.request(Request::Scene { op }).await)
}

/// Rename a scene.
async fn rename(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let asked = match parse::<RenameWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let op = SceneOp::Rename {
        from: asked.from,
        to: asked.to,
    };
    named(web.fleet.request(Request::Scene { op }).await)
}

/// Delete a scene.
async fn delete(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let asked = match parse::<DeleteWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let op = SceneOp::Delete { name: asked.name };
    named(web.fleet.request(Request::Scene { op }).await)
}

/// `{"name"}`, or the daemon's refusal.
fn named(answer: Answer) -> Response {
    match answer {
        Answer::Scene { name, .. } => {
            axum::Json(serde_json::json!({ "name": name })).into_response()
        }
        other => refused(other),
    }
}

/// `{"name", "outcome"}` for a save: the configured devices that are not connected, each named as
/// a part left out of the scene; or the daemon's refusal.
fn saved(answer: Answer) -> Response {
    match answer {
        Answer::Scene { name, outcome } => {
            axum::Json(serde_json::json!({ "name": name, "outcome": outcome as Vec<PartOutcome> }))
                .into_response()
        }
        other => refused(other),
    }
}

/// `{"name", "outcome"}`, or the daemon's refusal or question.
fn outcome(answer: Answer) -> Response {
    match answer {
        Answer::Scene { name, outcome } => {
            axum::Json(serde_json::json!({ "name": name, "outcome": outcome as Vec<PartOutcome> }))
                .into_response()
        }
        Answer::Ask {
            message,
            name,
            devices,
        } => {
            let body = serde_json::json!({
                "error": message,
                "ask": {
                    "kind": "both_sides",
                    "name": name,
                    "devices": devices,
                    "question": message,
                },
            });
            with_status(StatusCode::CONFLICT, body)
        }
        other => refused(other),
    }
}

/// `body` under `status`.
fn with_status(status: StatusCode, body: serde_json::Value) -> Response {
    let mut response = axum::Json(body).into_response();
    *response.status_mut() = status;
    response
}

/// The daemon's refusal as `{"error", "reason"}`: 422 for a name that is bad or taken, 409 for
/// every other reason, and for a refusal with none.
fn refused(answer: Answer) -> Response {
    let Answer::Error {
        message, reason, ..
    } = answer
    else {
        return error(StatusCode::BAD_GATEWAY, "the daemon gave no answer");
    };
    let status = match reason {
        Some(Reason::BadName | Reason::Taken) => StatusCode::UNPROCESSABLE_ENTITY,
        _ => StatusCode::CONFLICT,
    };
    let mut body = serde_json::json!({ "error": message });
    if let Some(reason) = reason {
        body["reason"] = serde_json::json!(reason);
    }
    with_status(status, body)
}

#[cfg(test)]
mod tests;
