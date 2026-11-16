// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Virtual speakers' sets from the page: list them, fetch one, import a file, forget one. Each is
//! the daemon's `speakers` request ([`SpeakersOp`]), sent through the fleet as the socket would
//! send it, and the route answers once: `catalogue` with the sets
//! ([`svolder_ipc::SpeakersCatalogue`]), `fetch` and `import` with the saved set
//! ([`svolder_ipc::SpeakersSaved`]), `forget` with `{"id"}`.
//!
//! `fetch` and `import` are jobs in the daemon. A route that starts one follows it inside the
//! daemon's own stream and waits for its end, no longer than [`WAIT_MAX`], and cancels the job
//! when the HTTP client goes (the handler's future is dropped) or the wait runs out; no event
//! reaches the page.
//!
//! `import` takes the file in the body as base64 (`{"data", "label"?, "room"?}`, the file at most
//! [`IMPORT_FILE_MAX`] bytes; a larger file imports with the CLI): the route puts it in a private
//! temporary file under the daemon's own speakers directory (`.import`, asked of the daemon, swept
//! when it starts) that the
//! daemon reads, and removes that when the job ends. Decoding and writing run on a blocking thread,
//! and the body is dropped as soon as it is decoded. The body never names a
//! path: a page never makes the daemon read a file of its own choosing.
//!
//! Every route is a signed-in writer's, the catalogue included: a guest never makes the daemon
//! reach the internet or the disk. A guest is refused 401, a page with `[web] writes` off 403.
//! A refusal is `{"error": words, "reason": word}` under `router::status_of`: 404 for `missing`
//! and `not_listed`, 409 for `busy`, 422 for `invalid`, 502 for `unreachable`. A body that is not
//! what a route takes is a 400.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use futures::StreamExt;
use hyper::{HeaderMap, StatusCode};
use serde::{Deserialize, Serialize};
use svolder_ipc::{
    CODE_USAGE, Event, JobId, JobState, Request, Response as Answer, SpeakersOp, SpeakersRoom,
    SpeakersSet,
};

use super::writer;
use crate::fleet::Fleet;
use crate::web::Web;
use crate::web::router::{error, refusal};

/// The most a file may be to import through the page, in bytes.
pub(crate) const IMPORT_FILE_MAX: usize = 32 * 1024 * 1024;

/// The longest body the import route reads: the file as base64, with room for the other keys.
pub(crate) const IMPORT_BODY_MAX: usize = IMPORT_FILE_MAX / 3 * 4 + 4 + 1024;

/// The path whose body may be [`IMPORT_BODY_MAX`].
pub(crate) const IMPORT_PATH: &str = "/api/v1/speakers/import";

/// How many imports may be read at once: their bodies are megabytes each.
pub(crate) const IMPORTS_AT_ONCE: usize = 2;

/// How long the import's body may take to arrive: the file is megabytes, from any link.
pub(crate) const IMPORT_BODY_TIMEOUT: Duration = Duration::from_secs(60);

/// The longest body a route but the import reads.
const BODY_MAX: usize = 4 * 1024;

/// The longest a route waits for a job: the download's 120 s and the conversion, with room.
const WAIT_MAX: Duration = Duration::from_secs(170);

static IMPORT_SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(IMPORTS_AT_ONCE);

/// A slot for one large import, held (as a request extension) from before its body is read until
/// the file is on disk and the body freed; none when [`IMPORTS_AT_ONCE`] are under way. The slot
/// bounds the memory the bodies hold, not the job.
pub(crate) fn import_permit() -> Option<Arc<tokio::sync::SemaphorePermit<'static>>> {
    IMPORT_SLOTS.try_acquire().ok().map(Arc::new)
}

/// The words of an import refused for want of a slot.
pub(crate) const IMPORT_BUSY: &str = "two imports are being read; try again when one ends";

/// The speakers routes: the import's body may be [`IMPORT_BODY_MAX`], every other's
/// [`BODY_MAX`].
pub(super) fn routes() -> Router<Web> {
    use axum::extract::DefaultBodyLimit;
    Router::new()
        .route("/api/v1/speakers/catalogue", post(catalogue))
        .route("/api/v1/speakers/fetch", post(fetch))
        .route("/api/v1/speakers/forget", post(forget))
        .layer(DefaultBodyLimit::max(BODY_MAX))
        .route(
            IMPORT_PATH,
            post(import).layer(DefaultBodyLimit::max(IMPORT_BODY_MAX)),
        )
}

/// The body as the JSON object `T`, or a 400 that names the key at fault, or a 413.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn parsed<T: for<'de> Deserialize<'de>>(body: &[u8], most: usize) -> Result<T, Response> {
    if body.len() > most {
        return Err(error(
            StatusCode::PAYLOAD_TOO_LARGE,
            format!("this request's body is at most {} KiB", most / 1024),
        ));
    }
    let body = if body.is_empty() { b"{}" } else { body };
    serde_json::from_slice(body).map_err(|e| {
        error(
            StatusCode::BAD_REQUEST,
            format!("the body is not what this takes: {e}"),
        )
    })
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
                    .request(Request::Speakers {
                        op: SpeakersOp::Cancel { job },
                    })
                    .await;
            });
        }
    }
}

/// Start the job `op` asks for, wait for its end, and answer its result as JSON.
async fn run_job(web: &Web, op: SpeakersOp) -> Response {
    let job = match web.fleet.request(Request::Speakers { op }).await {
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
        .stream(Request::Speakers {
            op: SpeakersOp::Follow,
        })
        .await
    {
        Ok(events) => events,
        Err(refused) => return refusal(*refused),
    };
    let ended = tokio::time::timeout(WAIT_MAX, async {
        while let Some(event) = events.next().await {
            if let Event::Speakers(event) = event
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
            axum::Json(saved_payload(payload)).into_response()
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
            "the speakers job took too long and was cancelled",
        ),
    }
}

/// A set as the page is given it: the set, and the words of its choice and its technical term
/// (svolder-speakers' own), so no front end writes them.
#[derive(Serialize)]
struct Row {
    #[serde(flatten)]
    set: SpeakersSet,
    room_name: &'static str,
    term: &'static str,
}

impl Row {
    fn of(set: SpeakersSet) -> Row {
        let room = crate::handler::room_of(set.room);
        Row {
            room_name: room.label(),
            term: room.term(),
            set,
        }
    }
}

/// What `fetch` and `import` answer: the saved set as a row, and the sets that made room.
#[derive(Serialize)]
struct Saved {
    set: Row,
    evicted: Vec<String>,
}

/// The payload of a finished `fetch` or `import` job, with the saved set as a row; a payload
/// that is not a saved set goes out as it is.
fn saved_payload(payload: serde_json::Value) -> serde_json::Value {
    match serde_json::from_value::<svolder_ipc::SpeakersSaved>(payload.clone()) {
        Ok(saved) => serde_json::to_value(Saved {
            set: Row::of(saved.set),
            evicted: saved.evicted,
        })
        .unwrap_or(payload),
        Err(_) => payload,
    }
}

/// The catalogue answer with each row's room words.
#[derive(Serialize)]
struct Catalogue {
    sets: Vec<Row>,
    fetching: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    note: Option<String>,
    max_sets: usize,
}

impl Catalogue {
    fn of(found: svolder_ipc::SpeakersCatalogue) -> Catalogue {
        Catalogue {
            sets: found.sets.into_iter().map(Row::of).collect(),
            fetching: found.fetching,
            note: found.note,
            max_sets: found.max_sets,
        }
    }
}

/// List the sets. The body is `{}`, or any JSON of at most [`BODY_MAX`].
async fn catalogue(
    State(web): State<Web>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    if let Err(refused) = parsed::<serde::de::IgnoredAny>(&body, BODY_MAX) {
        return refused;
    }
    let op = SpeakersOp::Catalogue;
    match web.fleet.request(Request::Speakers { op }).await {
        Answer::SpeakersCatalogue(found) => axum::Json(Catalogue::of(*found)).into_response(),
        other => refusal(other),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Named {
    id: String,
}

/// Fetch a catalogue set.
async fn fetch(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    match parsed::<Named>(&body, BODY_MAX) {
        Ok(Named { id }) => run_job(&web, SpeakersOp::Fetch { id }).await,
        Err(refused) => refused,
    }
}

/// Remove a set.
async fn forget(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let Named { id } = match parsed(&body, BODY_MAX) {
        Ok(named) => named,
        Err(refused) => return refused,
    };
    match web
        .fleet
        .request(Request::Speakers {
            op: SpeakersOp::Forget { id: id.clone() },
        })
        .await
    {
        Answer::Ack => axum::Json(serde_json::json!({ "id": id })).into_response(),
        other => refusal(other),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Upload {
    /// the file's bytes, base64 (standard alphabet)
    data: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    room: Option<SpeakersRoom>,
}

/// A private temporary file, removed when it drops.
struct Temp(std::path::PathBuf);

impl Temp {
    fn with(dir: &std::path::Path, bytes: &[u8]) -> std::io::Result<Temp> {
        use std::io::Write;
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let path = dir.join(format!(
            "import-{}-{nanos}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)?;
        let temp = Temp(path);
        file.write_all(bytes)?;
        Ok(temp)
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Import a file as the person's own set.
async fn import(
    State(web): State<Web>,
    headers: HeaderMap,
    permit: Option<axum::Extension<Arc<tokio::sync::SemaphorePermit<'static>>>>,
    body: axum::body::Bytes,
) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let parsed_body = parsed::<Upload>(&body, IMPORT_BODY_MAX);
    drop(body);
    let Upload { data, label, room } = match parsed_body {
        Ok(upload) => upload,
        Err(refused) => return refused,
    };
    // Where the daemon keeps what waits to be read: under its own store, never the process's
    // state home.
    let dir = match web
        .fleet
        .request(Request::Speakers {
            op: SpeakersOp::ImportDir,
        })
        .await
    {
        Answer::SpeakersImportDir { path } => std::path::PathBuf::from(path),
        other => return refusal(other),
    };
    // Decoding and the write of megabytes are blocking work.
    let written = tokio::task::spawn_blocking(move || {
        let bytes = decode_base64(&data).filter(|bytes| bytes.len() <= IMPORT_FILE_MAX)?;
        drop(data);
        Some(Temp::with(&dir, &bytes))
    })
    .await;
    // The body and the bytes are freed: the slot is too.
    drop(permit);
    let temp = match written {
        Ok(Some(Ok(temp))) => temp,
        Ok(Some(Err(e))) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("the file could not be handed to the daemon: {e}"),
            );
        }
        Ok(None) => {
            return error(
                StatusCode::BAD_REQUEST,
                format!(
                    "the body is not what this takes: data is base64 of a file of at most {} MiB",
                    IMPORT_FILE_MAX / (1024 * 1024)
                ),
            );
        }
        Err(_) => {
            return error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the file could not be handed to the daemon",
            );
        }
    };
    let op = SpeakersOp::Import {
        path: temp.0.display().to_string(),
        // The temporary file's name says nothing, so a body with no label is `Own file`.
        label: Some(label.unwrap_or_else(|| "Own file".to_string())),
        room,
    };
    // The temporary file lives until the job has ended (the route waits for it).
    let answered = run_job(&web, op).await;
    drop(temp);
    answered
}

/// `text` as base64 in the standard alphabet, padded or not; none for any other character.
fn decode_base64(text: &str) -> Option<Vec<u8>> {
    let text = text.trim_end_matches('=');
    let mut out = Vec::with_capacity(text.len() / 4 * 3 + 3);
    let (mut bits, mut held) = (0u32, 0u32);
    for byte in text.bytes() {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        bits = bits << 6 | u32::from(value);
        held += 6;
        if held >= 8 {
            held -= 8;
            out.push((bits >> held) as u8);
            bits &= (1 << held) - 1;
        }
    }
    // A last group of one character holds no whole byte: not base64.
    (text.len() % 4 != 1).then_some(out)
}

#[cfg(test)]
mod tests;
