// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! AutoEq from the page: search the index, preview a measurement for a side, import it as a
//! preset and upload a file as one. Each is the daemon's `auto_eq` request ([`AutoEqOp`]), sent
//! through the fleet as the socket would send it, and the route answers once: `search` with
//! the groups, `upload` and `import` with the saved preset (`{"name", "filters", "skipped"}`),
//! `preview` with the preview ([`svolder_ipc::AutoEqPreview`]).
//!
//! `preview` and `import` are jobs in the daemon. A route that starts one follows it inside the
//! daemon's own stream and waits for its end, no longer than [`WAIT_MAX`], and cancels the job
//! when the HTTP client goes (the handler's future is dropped) or the wait runs out; no event
//! reaches the page.
//!
//! Every route is a signed-in writer's, search included: a guest never makes the daemon reach the
//! internet. A guest is refused 401, a page with `[web] writes` off 403. The body is the op's own
//! keys; the side of the EQ is `"target": {"side": "device", "device": NAME}` or
//! `{"side": "computer"}`, and the measurements are the `variant` objects a search answered.
//!
//! A refusal is `{"error": words, "reason": word}`: 422 for `taken`, `invalid` and `bad_name`,
//! 404 for `missing` and `not_listed`, 409 for `busy`, `read_only`, `file_busy` and a device not
//! found yet, 502 for `unreachable`, 503 for `stopping`, 500 for `internal`
//! (`router::status_of`). A body that is not what a route takes is a 400. A body over
//! [`BODY_MAX`] is a 413 (the upload's own cap is [`UPLOAD_BODY_MAX`], applied by the router).

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use futures::StreamExt;
use hyper::{HeaderMap, StatusCode};
use svolder_ipc::{AutoEqOp, CODE_USAGE, Event, JobId, JobState, Request, Response as Answer};

use super::writer;
use crate::fleet::Fleet;
use crate::web::Web;
use crate::web::router::{error, refusal};

/// The longest body a route but the upload reads: a query, two measurements and a name.
const BODY_MAX: usize = 8 * 1024;

/// The longest body the upload route reads, in bytes: the file's 64 KiB of text with its JSON
/// escapes. The daemon refuses the text itself past 64 KiB.
pub(crate) const UPLOAD_BODY_MAX: usize = 128 * 1024;

/// The path whose body may be [`UPLOAD_BODY_MAX`].
pub(crate) const UPLOAD_PATH: &str = "/api/v1/autoeq/upload";

/// The longest a route waits for a job: the index (60 s, when the job must load it), two
/// downloads of 20 s and the fits, with room.
const WAIT_MAX: Duration = Duration::from_secs(150);

/// The AutoEq routes.
pub(super) fn routes() -> Router<Web> {
    Router::new()
        .route("/api/v1/autoeq/search", post(search))
        .route("/api/v1/autoeq/preview", post(preview))
        .route("/api/v1/autoeq/import", post(import))
        .route(UPLOAD_PATH, post(upload))
        .layer(axum::extract::DefaultBodyLimit::max(UPLOAD_BODY_MAX))
}

/// The op of `kind` the body asks for: the body's keys as the op's own, under `op`. A body that is
/// not what the op takes is a 400 that names the key at fault.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn op_of(kind: &str, body: &[u8], most: usize) -> Result<AutoEqOp, Response> {
    let refuse = |why: &str| {
        error(
            StatusCode::BAD_REQUEST,
            format!("the body is not what this takes: {why}"),
        )
    };
    if body.len() > most {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("this request's body is at most {} KiB", most / 1024),
        ));
    }
    let mut keys = match body {
        [] => serde_json::Map::new(),
        body => match serde_json::from_slice::<serde_json::Value>(body) {
            Ok(serde_json::Value::Object(keys)) => keys,
            Ok(_) => return Err(refuse("it must be a JSON object")),
            Err(_) => return Err(refuse("it is not JSON")),
        },
    };
    keys.insert("op".into(), kind.into());
    serde_json::from_value(serde_json::Value::Object(keys)).map_err(|e| refuse(&e.to_string()))
}

/// Cancels a job when dropped while armed: the HTTP client went before the job ended.
struct CancelOnDrop {
    fleet: Arc<dyn Fleet>,
    job: JobId,
    armed: bool,
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let (fleet, job) = (Arc::clone(&self.fleet), self.job);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = fleet
                    .request(Request::AutoEq {
                        op: AutoEqOp::Cancel { job },
                    })
                    .await;
            });
        }
    }
}

/// Start the job `op` asks for, wait for its end, and answer its result as JSON.
async fn run_job(web: &Web, op: AutoEqOp) -> Response {
    let job = match web.fleet.request(Request::AutoEq { op }).await {
        Answer::Job { job } => job,
        other => return refusal(other),
    };
    let mut guard = CancelOnDrop {
        fleet: Arc::clone(&web.fleet),
        job,
        armed: true,
    };
    let mut events = match web
        .fleet
        .stream(Request::AutoEq {
            op: AutoEqOp::Follow,
        })
        .await
    {
        Ok(events) => events,
        Err(refused) => return refusal(*refused),
    };
    let ended = tokio::time::timeout(WAIT_MAX, async {
        while let Some(event) = events.next().await {
            if let Event::AutoEq(event) = event
                && event.job == job
                && event.state.is_final()
            {
                return Some(event.state);
            }
        }
        None
    })
    .await;
    match ended {
        Ok(Some(JobState::Done { payload })) => {
            guard.armed = false;
            axum::Json(payload).into_response()
        }
        Ok(Some(JobState::Failed { message, reason })) => {
            guard.armed = false;
            refusal(Answer::error(CODE_USAGE, message).with_reason(reason))
        }
        Ok(Some(_)) => {
            guard.armed = false;
            error(
                StatusCode::CONFLICT,
                "the job was cancelled: another client stopped it, or the daemon is stopping",
            )
        }
        Ok(None) => error(StatusCode::BAD_GATEWAY, "the daemon's job stream ended"),
        Err(_) => error(
            StatusCode::GATEWAY_TIMEOUT,
            "the AutoEq job took too long and was cancelled",
        ),
    }
}

/// Search the index.
async fn search(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let op = match op_of("search", &body, BODY_MAX) {
        Ok(op) => op,
        Err(refused) => return refused,
    };
    match web.fleet.request(Request::AutoEq { op }).await {
        Answer::AutoEqSearch(found) => axum::Json(*found).into_response(),
        other => refusal(other),
    }
}

/// Preview a measurement for a side.
async fn preview(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    match op_of("preview", &body, BODY_MAX) {
        Ok(op) => run_job(&web, op).await,
        Err(refused) => refused,
    }
}

/// Import a measurement as a preset.
async fn import(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let op = match op_of("import", &body, BODY_MAX) {
        Ok(op) => op,
        Err(refused) => return refused,
    };
    let answered = run_job(&web, op).await;
    if answered.status().is_success() {
        web.fleet_changed.send_modify(|turn| *turn += 1);
    }
    answered
}

/// Save uploaded text as a preset.
async fn upload(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let op = match op_of("upload", &body, UPLOAD_BODY_MAX) {
        Ok(op) => op,
        Err(refused) => return refused,
    };
    match web.fleet.request(Request::AutoEq { op }).await {
        Answer::AutoEqSaved(saved) => {
            web.fleet_changed.send_modify(|turn| *turn += 1);
            axum::Json(saved).into_response()
        }
        other => refusal(other),
    }
}

#[cfg(test)]
mod tests;
