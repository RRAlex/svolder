// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone control from the page: its mode, one band's frequency, gain or Q, the loudness
//! strength, and a preset's recall, each through the device's writer as the web's. A band
//! written while the mode reads Flat asks the writer to switch the mode to EQ after it
//! (`flat_to_eq`, the rule Home Assistant's writes follow too), and its answer says so for the
//! page's toast.

use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use hyper::{HeaderMap, StatusCode};
use serde::Deserialize;
use svolder_ipc::Response as Answer;
use svolder_profiles::tone::preset::Preset;
use svolder_profiles::{EqMode, EqParam, EqSpec, Profile, units};
use svolder_proto::{FunctionId, ValueType};

use super::{outcome, with_parts, writable, writer};
use crate::device::{Command, Feed};
use crate::web::Web;
use crate::web::api::{Loaded, load};
use crate::web::model::Part;
use crate::web::router::{error, parse};
use crate::web::view::minus;
use crate::writer::{Mode, Options, Outcome};

/// The strongest loudness setting the function takes.
const LOUDNESS_MAX: i64 = 127;

/// How long a band write that switched the mode waits to see the mode read EQ.
const SWITCH_WAIT: Duration = Duration::from_secs(1);

/// What the page says when a band write took the mode out of Flat.
const MODE_SET_TO_EQ: &str = "Mode set to EQ";

/// The tone control's routes.
pub(super) fn routes() -> Router<Web> {
    Router::new()
        .route("/api/v1/devices/{device}/eq/mode", post(mode))
        .route("/api/v1/devices/{device}/eq/bands/{band}", post(band))
        .route("/api/v1/devices/{device}/eq/loudness", post(loudness))
        .route("/api/v1/devices/{device}/presets/recall", post(recall))
}

/// The profile and tone control of `device`: 409 before it is found, 404 when it has none.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(super) fn eq_of(device: &Loaded) -> Result<(Arc<Profile>, EqSpec), Response> {
    let Some(profile) = device.feed.profile.clone() else {
        return Err(error(
            StatusCode::CONFLICT,
            svolder_ipc::not_found_yet(&device.info.name),
        ));
    };
    let Some(eq) = profile.eq.clone() else {
        return Err(error(
            StatusCode::NOT_FOUND,
            format!("{} has no tone control", device.info.name),
        ));
    };
    Ok((profile, eq))
}

/// Function `id`'s raw reading on `device`.
pub(super) fn raw_of(device: &Loaded, id: FunctionId) -> Option<i64> {
    device
        .feed
        .readings
        .iter()
        .find(|m| m.id == id)
        .and_then(|m| m.value.as_i64())
}

/// What `eq/mode` takes.
#[derive(Deserialize)]
struct ModeWrite {
    mode: String,
}

/// Set the tone control's mode.
async fn mode(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    let device = match writable(&web, &headers, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    let asked = match parse::<ModeWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let mode = match asked.mode.parse::<EqMode>() {
        Ok(mode) => mode,
        Err(why) => return error(StatusCode::UNPROCESSABLE_ENTITY, why.to_string()),
    };
    if let Err(refused) = eq_of(&device) {
        return refused;
    }
    let command = Command::EqMode {
        mode: mode.name().to_string(),
        no_wait: false,
    };
    let ended = web
        .fleet
        .submit(&device.info, command, Options::from(Mode::Latest))
        .await;
    outcome(
        &web,
        &name,
        ended,
        &[Part::Tone],
        |_| serde_json::json!({ "mode": mode.name() }),
    )
}

/// What `eq/bands/{band}` takes: one step either way, a value typed in the parameter's unit, or
/// the raw step itself, as a drag on the graph writes it. Exactly one of the three.
#[derive(Deserialize)]
struct BandWrite {
    param: String,
    step: Option<i64>,
    value: Option<serde_json::Value>,
    raw: Option<i64>,
}

/// Write one band's frequency, gain or Q. `band` is 1-based, as the table numbers it.
async fn band(
    State(web): State<Web>,
    headers: HeaderMap,
    Path((name, band)): Path<(String, usize)>,
    body: axum::body::Bytes,
) -> Response {
    let device = match writable(&web, &headers, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    let asked = match parse::<BandWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let (_, eq) = match eq_of(&device) {
        Ok(found) => found,
        Err(refused) => return refused,
    };
    let Some(index) = band.checked_sub(1).filter(|index| *index < eq.bands.len()) else {
        return error(
            StatusCode::NOT_FOUND,
            format!("{} has bands 1 to {}", device.info.name, eq.bands.len()),
        );
    };
    let param = match asked.param.parse::<EqParam>() {
        Ok(param) => param,
        Err(why) => return error(StatusCode::UNPROCESSABLE_ENTITY, why.to_string()),
    };
    let Some(id) = eq.band_id(index, param) else {
        return error(
            StatusCode::NOT_FOUND,
            format!("{} has no band {band}", device.info.name),
        );
    };
    let value = match (asked.step, &asked.value, asked.raw) {
        (Some(step), None, None) => {
            let Some(now) = raw_of(&device, id) else {
                return error(
                    StatusCode::CONFLICT,
                    "the tone control has not been read yet",
                );
            };
            let next = (now + step.signum()).clamp(0, i64::from(EqSpec::STEP_MAX));
            if next == now {
                return axum::Json(band_json(&eq, index, param, now, false)).into_response();
            }
            match value_at(&eq, index, param, next) {
                Some(value) => value,
                None => return error(StatusCode::CONFLICT, "that step has no value"),
            }
        }
        (None, Some(typed), None) => match typed_value(param, typed) {
            Ok(value) => value,
            Err(refused) => return refused,
        },
        (None, None, Some(raw)) => match raw_value(&eq, index, param, raw) {
            Ok(value) => value,
            Err(refused) => return refused,
        },
        _ => {
            return error(
                StatusCode::BAD_REQUEST,
                "a band write takes a step, a value or a raw step",
            );
        }
    };
    let flat = raw_of(&device, eq.mode_id()) == Some(EqMode::Flat as i64);
    let command = Command::EqSet {
        band,
        param: param.word().to_string(),
        value,
        no_wait: false,
    };
    let options = Options {
        mode: Mode::Latest,
        flat_to_eq: true,
    };
    let ended = web.fleet.submit(&device.info, command, options).await;
    let switched = flat && matches!(ended, Outcome::Done(Answer::Value(_)));
    if switched {
        mode_reads_eq(&web, &device, eq.mode_id()).await;
    }
    outcome(&web, &name, ended, &[Part::Tone], |echo| {
        let raw = echo.raw.as_i64().unwrap_or_default();
        band_json(&eq, index, param, raw, switched)
    })
}

/// Wait, for a moment, until the mode reads EQ on `device`: the switch is written after the band
/// is answered, and the parts a band write answers with should show the mode it ended in. The
/// stream carries the mode if it comes later.
async fn mode_reads_eq(web: &Web, device: &Loaded, mode_id: FunctionId) {
    let Some(watched) = web.fleet.watch(&device.info) else {
        return;
    };
    let mut feed = watched.feed;
    let eq = EqMode::Eq as i64;
    let reads_eq = |feed: &Feed| {
        feed.readings
            .iter()
            .find(|m| m.id == mode_id)
            .and_then(|m| m.value.as_i64())
            == Some(eq)
    };
    let _ = tokio::time::timeout(SWITCH_WAIT, feed.wait_for(reads_eq)).await;
}

/// The value a raw band write asks for: 422 for a step outside 0 to [`EqSpec::STEP_MAX`], 409
/// for an in-range step the band has no value for (as a step from a reading does).
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn raw_value(eq: &EqSpec, index: usize, param: EqParam, raw: i64) -> Result<f64, Response> {
    if !(0..=i64::from(EqSpec::STEP_MAX)).contains(&raw) {
        return Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("a step is 0 to {}", EqSpec::STEP_MAX),
        ));
    }
    value_at(eq, index, param, raw)
        .ok_or_else(|| error(StatusCode::CONFLICT, "that step has no value"))
}

/// What raw step `raw` of band `index`'s `param` stands for: Hz, dB or Q.
fn value_at(eq: &EqSpec, index: usize, param: EqParam, raw: i64) -> Option<f64> {
    let raw = u8::try_from(raw).ok()?;
    match param {
        EqParam::Frequency => eq.frequency(index, raw),
        EqParam::Gain => Some(eq.gain_db(raw)),
        EqParam::Q => Some(eq.q(raw)),
    }
}

/// A typed value for `param`: a number, or text in the parameter's unit read by the core
/// ([`units::number_in`]), the page's minus read as a hyphen. A frequency must be above zero.
/// Out of the band's steps it is clamped, as every write converts it.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn typed_value(param: EqParam, typed: &serde_json::Value) -> Result<f64, Response> {
    let (label, unit) = match param {
        EqParam::Frequency => ("Frequency", Some("Hz")),
        EqParam::Gain => ("Gain", Some("dB")),
        EqParam::Q => ("Q", None),
    };
    let number = match typed {
        serde_json::Value::Number(n) => n.as_f64(),
        serde_json::Value::String(text) => units::number_in(&text.replace('−', "-"), unit),
        _ => None,
    }
    .filter(|n| n.is_finite())
    .filter(|n| param != EqParam::Frequency || *n > 0.0);
    number.ok_or_else(|| {
        error(
            StatusCode::UNPROCESSABLE_ENTITY,
            match unit {
                Some(unit) => format!("{label} takes a number in {unit}"),
                None => format!("{label} takes a number"),
            },
        )
    })
}

/// A band value as the editor shows it, with the toast of a write that took the mode out of
/// Flat.
fn band_json(
    eq: &EqSpec,
    index: usize,
    param: EqParam,
    raw: i64,
    switched: bool,
) -> serde_json::Value {
    let text =
        units::band_text(eq, index, param, raw).map_or_else(|| raw.to_string(), |t| minus(&t));
    let mut shown = serde_json::json!({ "raw": raw, "text": text, "mode_set_to_eq": switched });
    if switched {
        shown["toast"] = MODE_SET_TO_EQ.into();
    }
    shown
}

/// What `eq/loudness` takes: one step either way, or the raw strength.
#[derive(Deserialize)]
struct LoudnessWrite {
    step: Option<i64>,
    raw: Option<i64>,
}

/// The strength a loudness write asks for: a step from the reading `read` (409 while it has not
/// been read), or the raw value itself, held to 0 to [`LOUDNESS_MAX`].
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn loudness_target(read: Option<i64>, asked: &LoudnessWrite) -> Result<i64, Response> {
    match (asked.step, asked.raw) {
        (Some(step), None) => match read {
            Some(now) => Ok((now + step.signum()).clamp(0, LOUDNESS_MAX)),
            None => Err(error(
                StatusCode::CONFLICT,
                "the tone control has not been read yet",
            )),
        },
        (None, Some(raw)) if (0..=LOUDNESS_MAX).contains(&raw) => Ok(raw),
        (None, Some(_)) => Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("Loudness strength takes a step from 0 to {LOUDNESS_MAX}"),
        )),
        _ => Err(error(
            StatusCode::BAD_REQUEST,
            "a loudness write takes a step or a raw value",
        )),
    }
}

/// Set the loudness strength: by key where the profile names a control on that function, and
/// straight to the function where it does not, as the TUI does.
async fn loudness(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    let device = match writable(&web, &headers, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    let asked = match parse::<LoudnessWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let (profile, eq) = match eq_of(&device) {
        Ok(found) => found,
        Err(refused) => return refused,
    };
    let id = eq.loudness_id();
    let target = match loudness_target(raw_of(&device, id), &asked) {
        Ok(target) => target,
        Err(refused) => return refused,
    };
    let command = match profile.control_by_id(id) {
        Some(control) => Command::Set {
            key: control.key.clone(),
            value: target.to_string(),
            step: 1,
            no_wait: false,
        },
        None => Command::Raw {
            id: id.to_string(),
            value_type: ValueType::U8.to_string(),
            value: target.to_string(),
            no_wait: false,
        },
    };
    let ended = web
        .fleet
        .submit(&device.info, command, Options::from(Mode::Latest))
        .await;
    outcome(&web, &name, ended, &[Part::Tone], |echo| {
        let raw = echo.raw.as_i64();
        serde_json::json!({ "raw": raw, "text": raw.map_or_else(String::new, |r| r.to_string()) })
    })
}

/// What `presets/recall` takes.
#[derive(Deserialize)]
struct RecallWrite {
    name: String,
}

/// Recall a preset through the writer, which remembers it for every front end. The presets that
/// fit are read from the configuration once, for this request.
async fn recall(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let device = match load(&web, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    let asked = match parse::<RecallWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    if let Err(refused) = eq_of(&device) {
        return refused;
    }
    let fitting: Vec<Preset> = device
        .presets
        .iter()
        .map(|(preset, _)| preset.clone())
        .collect();
    match web.fleet.recall(&device.info, &asked.name, fitting).await {
        Err(why) => error(StatusCode::CONFLICT, why),
        Ok((_, Answer::Error { message, .. })) => error(StatusCode::CONFLICT, message),
        Ok((recalled, _)) => with_parts(
            &web,
            &name,
            serde_json::json!({ "name": recalled.preset.name }),
            &[Part::Tone, Part::Presets],
        ),
    }
}

#[cfg(test)]
mod tests;
