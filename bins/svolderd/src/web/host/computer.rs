// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's volume, mute and live EQ from the page. Every route is a `POST` and a write
//! like any other (`writer`: a guest is refused 401, a page with `[web] writes` off 403); the
//! daemon takes it from there and the model carries the result (`HostModel.computer`).
//!
//! - `api/v1/host/volume` `{"pct"}`: 0 to 100, never above.
//! - `api/v1/host/mute` `{"muted"}`.
//! - `api/v1/host/eq` `{"on"}`.
//! - `api/v1/host/crossfeed` `{"on"?, "model"?, "preset"?, "amount_pct"?, "crossover_hz"?,
//!   "delay_ms"?, "done"?}`: change the computer's crossfeed (`HostEqOp::Crossfeed`; the daemon
//!   holds the values to the model's ranges and says what it refuses in words, a 409). `done` is
//!   false while a drag goes on and, when absent, true; a body that names nothing is a 409.
//! - `api/v1/host/speakers` `{"on"?, "set"?, "done"?}`: change the virtual speakers
//!   (`HostEqOp::Speakers`; `set` is the id of a fetched set, which rebuilds the chain once; turning
//!   them on turns the crossfeed off in the same update and the other way round; the daemon says
//!   what it refuses in words, a 409). `done` as for the crossfeed; a body that names nothing is a
//!   409. Fetching, importing and forgetting sets are routes of their own.
//! - `api/v1/host/loudness` `{"on"?, "strength_pct"?, "reference_pct"?, "reference_now"?,
//!   "done"?}`: change the computer's loudness (`HostEqOp::Loudness`; the daemon snaps the numbers to their
//!   steps and holds them to the view's ranges, says what it refuses in words, a 409, and
//!   `reference_now` takes the computer's volume as the reference). `done` as for the crossfeed; a
//!   body that names nothing is a 409.
//! - `api/v1/host/eq/preset` `{"name", "device"?, "confirm"?}`: a preset recalled on the
//!   computer.
//! - `api/v1/host/eq/points` `{"type", "f", "g", "q"}`: add a point; a value outside the hand
//!   ranges (20 Hz to 20 kHz, plus or minus 12 dB, Q 0.1 to 10) is a 422.
//! - `api/v1/host/eq/points/{n}` `{"type"?, "f"?, "g"?, "q"?, "done"?, "slot"?}`: change point `n`,
//!   counted from 1; `done` is false while a drag goes on and, when absent, true. With `slot` (1
//!   to 16, the point's slot in the model) the point is the one in that slot and `n` is not looked
//!   at: a slot no point holds is a 409 and nothing changes; a slot outside 1 to 16 is a 422.
//! - `api/v1/host/eq/points/{n}/remove` with an optional `{"slot"?}` body: the same rule.
//!
//! A write answers `{"ok": true}`; 409 with the daemon's words when it refused; 422 for a value
//! out of its range; 503 `computer controls unavailable: <why>` while the controls cannot be
//! used. A recall of a preset that a connected device plays too answers 409 with a question
//! (`ask`), see [`question`]; the page asks it and sends the recall again with `confirm: true`.

use axum::Router;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use hyper::{HeaderMap, StatusCode};
use serde::Deserialize;
use svolder_host::eq::{HAND, HandRanges, kind_word, parse_kind};
use svolder_ipc::{CODE_COMPUTER_UNAVAILABLE, HostEqOp, Request, Response as Answer};

use super::super::Web;
use super::super::router::{error, parse};
use super::super::writes::writer;

/// The most the computer's volume is written to.
const MAX_PCT: i64 = 100;
/// The most slots there are.
const MAX_SLOT: i64 = 16;

/// The computer's routes.
pub(super) fn routes() -> Router<Web> {
    Router::new()
        .route("/api/v1/host/volume", post(volume))
        .route("/api/v1/host/mute", post(mute))
        .route("/api/v1/host/eq", post(mode))
        .route("/api/v1/host/crossfeed", post(crossfeed))
        .route("/api/v1/host/speakers", post(speakers))
        .route("/api/v1/host/loudness", post(loudness))
        .route("/api/v1/host/eq/preset", post(preset))
        .route("/api/v1/host/eq/points", post(add))
        .route("/api/v1/host/eq/points/{n}", post(set))
        .route("/api/v1/host/eq/points/{n}/remove", post(remove))
}

/// `{"ok": true}`.
fn ok() -> Response {
    axum::Json(serde_json::json!({ "ok": true })).into_response()
}

/// The 409 that asks the question of a recall that two sides would play: `{"error": <the
/// daemon's words>, "ask": {"kind": "both_sides", "name", "devices", "question"}}`. `question` is
/// the sentence to put to the user (`<name> is on the <device> too: flatten the <device> and apply
/// on the computer?`); yes sends the same request again with `"confirm": true`.
fn question(message: String, name: &str, devices: &[String]) -> Response {
    let held = devices.join(" and the ");
    let body = serde_json::json!({
        "error": message,
        "ask": {
            "kind": "both_sides",
            "name": name,
            "devices": devices,
            "question": format!(
                "{name} is on the {held} too: flatten the {held} and apply on the computer?"
            ),
        },
    });
    let mut response = axum::Json(body).into_response();
    *response.status_mut() = StatusCode::CONFLICT;
    response
}

/// The answer to what the daemon said of a request.
fn answer(said: Answer) -> Response {
    match said {
        Answer::Ack => ok(),
        Answer::Error { code, message, .. } if code == CODE_COMPUTER_UNAVAILABLE => {
            error(StatusCode::SERVICE_UNAVAILABLE, message)
        }
        Answer::Error {
            message, reason, ..
        } => {
            // The reason rides with the words, as `router::refusal` carries it: a page acts on the
            // reason (`missing`: read the sets again), never on the words.
            let mut body = serde_json::json!({ "error": message });
            if let Some(reason) = reason {
                body["reason"] = serde_json::json!(reason);
            }
            let mut response = axum::Json(body).into_response();
            *response.status_mut() = StatusCode::CONFLICT;
            response
        }
        Answer::Ask {
            message,
            name,
            devices,
        } => question(message, &name, &devices),
        _ => error(StatusCode::BAD_GATEWAY, "the daemon gave no answer"),
    }
}

/// A 422 saying what is out of range.
fn out_of_range(why: impl Into<String>) -> Response {
    error(StatusCode::UNPROCESSABLE_ENTITY, why)
}

/// `kind` as the core spells it (`PK`, `LSC` or `HSC`), read as every front end reads it
/// ([`parse_kind`]), or a 422.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn kind_of(kind: &str) -> Result<String, Response> {
    parse_kind(kind)
        .map(|kind| kind_word(kind).to_string())
        .map_err(|why| out_of_range(why.to_string()))
}

/// The point `n` of the path, counted from 1, or a 422.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn index_of(n: &str) -> Result<u8, Response> {
    match n.parse::<u8>() {
        Ok(index) if index >= 1 => Ok(index),
        _ => Err(out_of_range("points count from 1")),
    }
}

/// The slot of a request body, which counts from 1 to the most points there are, or a 422.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn slot_of(slot: Option<i64>) -> Result<Option<u8>, Response> {
    match slot {
        None => Ok(None),
        Some(slot) if (1..=MAX_SLOT).contains(&slot) => Ok(u8::try_from(slot).ok()),
        Some(_) => Err(out_of_range(format!("slots count from 1 to {MAX_SLOT}"))),
    }
}

/// Send `op` to the daemon as an EQ operation.
async fn eq(web: &Web, op: HostEqOp) -> Response {
    answer(web.fleet.request(Request::HostEq { op }).await)
}

/// What `volume` takes.
#[derive(Deserialize)]
struct Volume {
    pct: i64,
}

/// Set the computer's volume.
async fn volume(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let asked = match parse::<Volume>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    if !(0..=MAX_PCT).contains(&asked.pct) {
        return out_of_range("the computer's volume goes from 0 to 100 %");
    }
    let pct = u8::try_from(asked.pct).unwrap_or(100);
    answer(web.fleet.request(Request::HostVolume { pct }).await)
}

/// What `mute` takes.
#[derive(Deserialize)]
struct Mute {
    muted: bool,
}

/// Mute or unmute the computer.
async fn mute(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    match parse::<Mute>(&body) {
        Ok(asked) => answer(
            web.fleet
                .request(Request::HostMute { muted: asked.muted })
                .await,
        ),
        Err(refused) => *refused,
    }
}

/// What `eq` takes.
#[derive(Deserialize)]
struct Mode {
    on: bool,
}

/// Turn the computer EQ on or off; off keeps the points.
async fn mode(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    match parse::<Mode>(&body) {
        Ok(asked) => eq(&web, HostEqOp::Mode { on: asked.on }).await,
        Err(refused) => *refused,
    }
}

/// What `crossfeed` takes: the fields of `HostEqOp::Crossfeed`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Crossfeed {
    on: Option<bool>,
    model: Option<String>,
    preset: Option<String>,
    amount_pct: Option<f64>,
    crossover_hz: Option<f64>,
    delay_ms: Option<f64>,
    #[serde(default = "done_by_default")]
    done: bool,
}

/// A change that does not say it goes on is the last of its drag.
fn done_by_default() -> bool {
    true
}

/// Change the computer's crossfeed.
async fn crossfeed(
    State(web): State<Web>,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    match parse::<Crossfeed>(&body) {
        Ok(asked) => {
            eq(
                &web,
                HostEqOp::Crossfeed {
                    on: asked.on,
                    model: asked.model,
                    preset: asked.preset,
                    amount_pct: asked.amount_pct,
                    crossover_hz: asked.crossover_hz,
                    delay_ms: asked.delay_ms,
                    done: asked.done,
                },
            )
            .await
        }
        Err(refused) => *refused,
    }
}

/// What `speakers` takes: the fields of `HostEqOp::Speakers`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Speakers {
    on: Option<bool>,
    set: Option<String>,
    #[serde(default = "done_by_default")]
    done: bool,
}

/// Change the computer's virtual speakers.
async fn speakers(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    match parse::<Speakers>(&body) {
        Ok(asked) => {
            eq(
                &web,
                HostEqOp::Speakers {
                    on: asked.on,
                    set: asked.set,
                    done: asked.done,
                },
            )
            .await
        }
        Err(refused) => *refused,
    }
}

/// What `loudness` takes: the fields of `HostEqOp::Loudness`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Loudness {
    on: Option<bool>,
    strength_pct: Option<f64>,
    reference_pct: Option<f64>,
    reference_now: Option<bool>,
    #[serde(default = "done_by_default")]
    done: bool,
}

/// Change the computer's loudness.
async fn loudness(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    match parse::<Loudness>(&body) {
        Ok(asked) => {
            eq(
                &web,
                HostEqOp::Loudness {
                    on: asked.on,
                    strength_pct: asked.strength_pct,
                    reference_pct: asked.reference_pct,
                    reference_now: asked.reference_now,
                    done: asked.done,
                },
            )
            .await
        }
        Err(refused) => *refused,
    }
}

/// What `eq/preset` takes.
#[derive(Deserialize)]
struct Preset {
    name: String,
    /// The device name (the URL segment, decoded), whose own built-ins the name is looked up in.
    device: Option<String>,
    #[serde(default)]
    confirm: bool,
}

/// Recall a preset on the computer.
async fn preset(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    match parse::<Preset>(&body) {
        Ok(asked) => {
            eq(
                &web,
                HostEqOp::Preset {
                    name: asked.name,
                    device: asked.device,
                    confirm: asked.confirm,
                },
            )
            .await
        }
        Err(refused) => *refused,
    }
}

/// What `eq/points` takes.
#[derive(Deserialize)]
struct Add {
    #[serde(rename = "type")]
    kind: String,
    f: f64,
    g: f64,
    q: f64,
}

/// Add a point.
async fn add(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let asked = match parse::<Add>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let kind = match kind_of(&asked.kind) {
        Ok(kind) => kind,
        Err(refused) => return refused,
    };
    let HandRanges { f, g, q } = HAND;
    for (name, value, (low, high)) in [("f", asked.f, f), ("g", asked.g, g), ("q", asked.q, q)] {
        if !(low..=high).contains(&value) {
            return out_of_range(format!("{name} goes from {low} to {high}"));
        }
    }
    let op = HostEqOp::Add {
        kind,
        f: asked.f,
        g: asked.g,
        q: asked.q,
    };
    eq(&web, op).await
}

/// What `eq/points/{n}` takes: only the fields that changed.
#[derive(Deserialize)]
struct Set {
    #[serde(rename = "type")]
    kind: Option<String>,
    f: Option<f64>,
    g: Option<f64>,
    q: Option<f64>,
    #[serde(default = "yes")]
    done: bool,
    /// The point's slot, which names it whatever place the table lists it in.
    slot: Option<i64>,
}

fn yes() -> bool {
    true
}

/// Change a point; `done` false while a drag goes on.
async fn set(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(n): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let index = match index_of(&n) {
        Ok(index) => index,
        Err(refused) => return refused,
    };
    let asked = match parse::<Set>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let kind = match asked.kind.as_deref().map(kind_of).transpose() {
        Ok(kind) => kind,
        Err(refused) => return refused,
    };
    let slot = match slot_of(asked.slot) {
        Ok(slot) => slot,
        Err(refused) => return refused,
    };
    let op = HostEqOp::Set {
        index,
        slot,
        kind,
        f: asked.f,
        g: asked.g,
        q: asked.q,
        done: asked.done,
    };
    eq(&web, op).await
}

/// What `eq/points/{n}/remove` takes: nothing, or the slot.
#[derive(Deserialize)]
struct Remove {
    slot: Option<i64>,
}

/// Remove a point.
async fn remove(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(n): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let index = match index_of(&n) {
        Ok(index) => index,
        Err(refused) => return refused,
    };
    let asked = match body.is_empty() {
        true => None,
        false => match parse::<Remove>(&body) {
            Ok(asked) => asked.slot,
            Err(refused) => return *refused,
        },
    };
    match slot_of(asked) {
        Ok(slot) => eq(&web, HostEqOp::Remove { index, slot }).await,
        Err(refused) => refused,
    }
}

#[cfg(test)]
mod tests;
