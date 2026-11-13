// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device walk from the page: start it, answer its prompt, step back, cancel it. Each is the
//! daemon's `walk` request ([`WalkOp`]), sent through the fleet as the socket would send it, as
//! the web's: `POST api/v1/walk` answers `{"job": id, "run": stamp}`, and `POST
//! api/v1/walk/{job}/answer` (`{"choice": n}` or `{"text": "..."}`), `.../back` and `.../cancel`
//! answer 204. Job ids restart with each daemon run, so the page names the run it was told of
//! (`"run"` in the body of each of the three): a run that is not the current one is refused
//! `no_such_job`, and a body with no `run` is let through. Where the walk stands, its prompt and the
//! devices it heard, comes on the page's events stream as `walk` events (`needs=walk`), one per
//! state, each whole.
//!
//! Only a signed-in writer reaches any of them: a guest is refused 401, a page with `[web]
//! writes` off 403. A passphrase travels in an answer's body only, and nothing a route answers
//! carries one.
//!
//! A refusal is `{"error": words, "reason": word}` with the daemon's words and its reason: 409
//! for `busy` (another walk runs) and `not_waiting` (the walk works on the last answer, or has
//! ended), 404 for `no_such_job`, 503 for `stopping`, 500 for `internal` (`router::status_of`).
//! A body that is not what a route takes is a 400 with no reason, whose words never quote it. A
//! body over [`BODY_MAX`] is the framework's own 413, plain text.

use axum::Router;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use hyper::{HeaderMap, StatusCode};
use svolder_ipc::{JobId, Request, Response as Answer, WalkOp};

use super::writer;
use crate::web::Web;
use crate::web::router::{bare, error, refusal};

/// The longest body a walk route reads, in bytes: a choice, or a name or a passphrase.
const BODY_MAX: usize = 4096;

/// The walk's routes.
pub(super) fn routes() -> Router<Web> {
    Router::new()
        .route("/api/v1/walk", post(start))
        .route("/api/v1/walk/{job}/answer", post(answer))
        .route("/api/v1/walk/{job}/back", post(back))
        .route("/api/v1/walk/{job}/cancel", post(cancel))
        .layer(axum::extract::DefaultBodyLimit::max(BODY_MAX))
}

/// A 400 whose words say why the request is not what the route takes, never quoting it.
fn not_taken(why: &str) -> Response {
    error(
        StatusCode::BAD_REQUEST,
        format!("the request is not what this takes: {why}"),
    )
}

/// The daemon's answer to `op`, for a viewer who may write: the job's id for a start, 204 for an
/// acknowledgement, or the refusal with its reason.
async fn run(web: &Web, headers: &HeaderMap, op: Result<WalkOp, Response>) -> Response {
    if let Err(refused) = writer(web, headers) {
        return refused;
    }
    let op = match op {
        Ok(op) => op,
        Err(refused) => return refused,
    };
    match web.fleet.request(Request::Walk { op }).await {
        Answer::Job { job } => {
            let run = crate::web::events::run_stamp();
            axum::Json(serde_json::json!({ "job": job, "run": run })).into_response()
        }
        Answer::Ack => bare(StatusCode::NO_CONTENT),
        other => refusal(other),
    }
}

/// The job the path names.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn job_of(job: &str) -> Result<JobId, Response> {
    job.parse()
        .map_err(|_| not_taken("a walk's id is a whole number"))
}

/// The keys of a body, which is empty or a JSON object.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn keys_of(body: &[u8]) -> Result<serde_json::Map<String, serde_json::Value>, Response> {
    match body {
        [] => Ok(serde_json::Map::new()),
        body => match serde_json::from_slice::<serde_json::Value>(body) {
            Ok(serde_json::Value::Object(keys)) => Ok(keys),
            Ok(_) => Err(not_taken("the body must be a JSON object")),
            Err(_) => Err(not_taken("the body is not JSON")),
        },
    }
}

/// Refuse `no_such_job` an op that names the daemon's run (`run` in its body, as the walk events
/// and the start's answer carry it) when that is not the run now: the job it names is one of an
/// earlier run, whose ids this run gives to other walks. An op with no `run` is let through, as
/// the socket's are.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn current_run(
    keys: &mut serde_json::Map<String, serde_json::Value>,
    job: JobId,
) -> Result<(), Response> {
    match keys.remove("run") {
        None | Some(serde_json::Value::Null) => Ok(()),
        Some(serde_json::Value::Number(run)) if run.as_u64().is_some() => {
            if run.as_u64() == Some(crate::web::events::run_stamp()) {
                Ok(())
            } else {
                Err(refusal(
                    Answer::error(svolder_ipc::CODE_NOT_FOUND, format!("no job {job}"))
                        .with_reason(svolder_ipc::Reason::NoSuchJob),
                ))
            }
        }
        Some(_) => Err(not_taken("`run` must be a whole number")),
    }
}

/// The op `tail` (`back` or `cancel`) for walk `job`, whose body may name the run.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn bare_op(job: &str, body: &[u8], make: fn(JobId) -> WalkOp) -> Result<WalkOp, Response> {
    let job = job_of(job)?;
    current_run(&mut keys_of(body)?, job)?;
    Ok(make(job))
}

/// The answer a body gives to walk `job`: its `choice` and its `text`, through the op's own hand
/// reader, which names the key at fault and never what it held.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn answer_of(job: &str, body: &[u8]) -> Result<WalkOp, Response> {
    let job = job_of(job)?;
    let mut keys = keys_of(body)?;
    current_run(&mut keys, job)?;
    keys.insert("op".into(), "answer".into());
    keys.insert("job".into(), job.into());
    serde_json::from_value(serde_json::Value::Object(keys)).map_err(|e| not_taken(&e.to_string()))
}

/// Start a walk.
async fn start(State(web): State<Web>, headers: HeaderMap) -> Response {
    run(&web, &headers, Ok(WalkOp::Start)).await
}

/// Answer the walk's prompt.
async fn answer(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(job): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    run(&web, &headers, answer_of(&job, &body)).await
}

/// Step the walk back.
async fn back(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(job): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    let op = bare_op(&job, &body, |job| WalkOp::Back { job });
    run(&web, &headers, op).await
}

/// Cancel the walk.
async fn cancel(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(job): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    let op = bare_op(&job, &body, |job| WalkOp::Cancel { job });
    run(&web, &headers, op).await
}

#[cfg(test)]
mod tests;
