// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the page writes: a control, the volume, the end of a climb, Try now and Identify. Every
//! write goes through the device's writer as the web's (`Origin::Web`): the volume judged first
//! by the shared rule with the web's cap ([`Rules::web`]) and sent as a glide, so a raise above
//! three quarters of the range climbs a tenth of the range a second, as Home Assistant's does;
//! every other control newest-wins. A write answers once the writer has, so the part it returns
//! shows the device's echo; one a newer write took the place of answers 204. Only a signed-in
//! viewer writes: a guest is refused 401, a page with `[web] writes` off 403.
//!
//! A write answers a component's JSON with the parts it changed (`parts`, as the model's parts
//! are) and, where the page says something about it, the words of its toast (`toast`).
//!
//! - `config_edit`: what the configuration writes share, the file and the edit then reload.
//! - `device_presets`: a device's own presets from the page: recall a slot, save to one, apply a
//!   built-in setting.
//! - `hops`: the hops after the DAC from the page: add, edit, remove and move.
//! - `presets`: the user's presets from the page: save, rename and delete.
//! - `tone`: the tone control from the page: the mode, a band, the loudness and a preset's recall.

mod config_edit;
mod device_presets;
mod hops;
mod presets;
mod tone;

use axum::Router;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use hyper::{HeaderMap, StatusCode};
use serde::Deserialize;
use svolder_ipc::{NOT_CONNECTED, Request, Response as Answer, ValueObject};
use svolder_profiles::safety::{self, Rules, Situation};
use svolder_profiles::units::{self, Outside, Refusal};
use svolder_profiles::{Control, ControlKind, roles};

use super::api::{Loaded, access, load, load_live};
use super::model::{self, Part};
use super::router::{bare, error, parse};
use super::view::{self, minus};
use super::{Viewer, Web, viewer};
use crate::device::Command;
use crate::writer::{Glided, Mode, Options, Outcome};

/// The writes' routes.
pub(crate) fn routes() -> Router<Web> {
    Router::new()
        .route("/api/v1/devices/{device}/controls/{key}", post(control))
        .route("/api/v1/devices/{device}/volume", post(volume))
        .route("/api/v1/devices/{device}/volume/stop", post(stop))
        .route("/api/v1/devices/{device}/connect", post(connect))
        .route("/api/v1/devices/{device}/identify", post(identify))
        .merge(tone::routes())
        .merge(device_presets::routes())
        .merge(presets::routes())
        .merge(hops::routes())
}

/// What a page with `[web] writes` off says to a viewer who may read but not write.
pub(crate) const READ_ONLY: &str = "Read-only: [web] writes is off";

/// Whether the request may write: signed in, with `[web] writes` on. A guest is told to sign in
/// (401), a page with writes off that it is read-only (403); no sign-in at all is a 401 with
/// nothing said. Only [`Viewer::Signed`] passes: a guest is inside the `Option` too.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(crate) fn writer(web: &Web, headers: &HeaderMap) -> Result<(), Response> {
    match viewer(web, headers) {
        Some(Viewer::Signed) if web.config.writes => Ok(()),
        Some(Viewer::Signed) => Err(error(StatusCode::FORBIDDEN, READ_ONLY)),
        Some(Viewer::Guest) => Err(error(StatusCode::UNAUTHORIZED, "Sign in to change this")),
        None => Err(bare(StatusCode::UNAUTHORIZED)),
    }
}

/// The device `name`, for a request that may write: its row, feed and climb, with no presets
/// and the configuration file not read (a drag streams writes).
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(crate) fn writable(web: &Web, headers: &HeaderMap, name: &str) -> Result<Loaded, Response> {
    writer(web, headers)?;
    load_live(web, name)
}

/// What `controls` takes: a select's value by name, a toggle as a bool, a level as a number in
/// its units (a centred level from its centre) — or a level's raw step.
#[derive(Deserialize)]
struct ControlWrite {
    value: Option<serde_json::Value>,
    raw: Option<i64>,
}

/// Write one control.
async fn control(
    State(web): State<Web>,
    headers: HeaderMap,
    Path((name, key)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Response {
    let device = match writable(&web, &headers, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    let asked = match parse::<ControlWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let Some(profile) = device.feed.profile.clone() else {
        return error(
            StatusCode::CONFLICT,
            svolder_ipc::not_found_yet(&device.info.name),
        );
    };
    let Some(control) = profile.control_by_key(&key).cloned() else {
        return error(
            StatusCode::NOT_FOUND,
            format!("{} has no control named {key}", device.info.name),
        );
    };
    if roles::is_volume(&control) {
        let raw = match level_raw(&control, &asked) {
            Ok(raw) => raw,
            Err(refused) => return refused,
        };
        return glide(&web, &device, &control, raw).await;
    }
    let value = match text_of(&control, &asked) {
        Ok(value) => value,
        Err(refused) => return refused,
    };
    let command = Command::Set {
        key: control.key.clone(),
        value,
        step: 1,
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
        &[Part::Controls, Part::Power],
        |value| echo(&control, value),
    )
}

/// The value a `set` carries for `asked` on `control`, or a 422 saying why there is none.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn text_of(control: &Control, asked: &ControlWrite) -> Result<String, Response> {
    let wrong = |why: String| error(StatusCode::UNPROCESSABLE_ENTITY, why);
    match control.kind {
        ControlKind::Select => match &asked.value {
            Some(serde_json::Value::String(name))
                if control.values.iter().any(|(_, v)| v == name) =>
            {
                Ok(name.clone())
            }
            _ => Err(wrong(format!(
                "{} takes one of: {}",
                control.label,
                control
                    .values
                    .iter()
                    .map(|(_, v)| v.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        },
        ControlKind::Toggle => match asked.value {
            Some(serde_json::Value::Bool(on)) => Ok(if on { "on" } else { "off" }.to_string()),
            _ => Err(wrong(format!("{} takes true or false", control.label))),
        },
        ControlKind::Level => level_raw(control, asked).map(|raw| raw.to_string()),
        _ => Err(error(
            StatusCode::BAD_REQUEST,
            format!("{} is not something the page can change", control.label),
        )),
    }
}

/// A level's raw step from `asked`: its raw step as given, or its value in its own units through
/// the core's codec ([`units::raw_for`]), refused past its ends.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn level_raw(control: &Control, asked: &ControlWrite) -> Result<i64, Response> {
    if let Some(raw) = asked.raw {
        return in_range(control, raw);
    }
    let Some(number) = asked.value.as_ref().and_then(serde_json::Value::as_f64) else {
        return Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{} takes a number", control.label),
        ));
    };
    units::raw_for(control, number, Outside::Refuse).map_err(|why| {
        let words = match why {
            Refusal::Outside { min, max } => minus(&format!(
                "{} takes {} to {}",
                control.label,
                units::amount_text(units::Amount::Value(min), control.unit.as_deref()),
                units::amount_text(units::Amount::Value(max), control.unit.as_deref())
            )),
            Refusal::NotFinite | Refusal::NoRange => format!("{} takes a number", control.label),
        };
        error(StatusCode::UNPROCESSABLE_ENTITY, words)
    })
}

/// `raw`, when `control` has a step there.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn in_range(control: &Control, raw: i64) -> Result<i64, Response> {
    let (lo, hi) = control.range.unwrap_or((0, 0));
    match (lo..=hi).contains(&raw) {
        true => Ok(raw),
        false => Err(error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("{} takes a step from {lo} to {hi}", control.label),
        )),
    }
}

/// What `volume` takes.
#[derive(Deserialize)]
struct VolumeWrite {
    raw: i64,
}

/// Set the volume, by the shared rule.
async fn volume(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    let device = match writable(&web, &headers, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    let asked = match parse::<VolumeWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let control = match volume_of(&device) {
        Ok(control) => control,
        Err(refused) => return refused,
    };
    match in_range(control, asked.raw) {
        Ok(raw) => glide(&web, &device, control, raw).await,
        Err(refused) => refused,
    }
}

/// The device's volume control: 409 while the device has not been found, 404 for a profile
/// without one.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn volume_of(device: &Loaded) -> Result<&Control, Response> {
    let Some(profile) = device.feed.profile.as_deref() else {
        return Err(error(
            StatusCode::CONFLICT,
            svolder_ipc::not_found_yet(&device.info.name),
        ));
    };
    roles::volume(profile).ok_or_else(|| {
        error(
            StatusCode::NOT_FOUND,
            format!("{} has no volume", device.info.name),
        )
    })
}

/// The volume's last reading, in raw steps.
fn reading(device: &Loaded, control: &Control) -> Option<i64> {
    device
        .feed
        .readings
        .iter()
        .find(|m| m.id == control.id)
        .and_then(|m| m.value.as_i64())
}

/// Judge setting the volume to `raw` by the web's rules (the cap clamps) and glide the
/// capped step through the writer: `{"landed": {"raw","text"}}`, `{"climbing": {"target",
/// "text"}}`, 204 when a newer write took its place, 409 with the words of a refusal.
async fn glide(web: &Web, device: &Loaded, control: &Control, raw: i64) -> Response {
    let reading = reading(device, control);
    let situation = Situation {
        reading,
        base: reading,
        floor_db: None,
        asked_db: None,
    };
    let judged = safety::judge(
        control,
        &situation,
        raw,
        &Rules::web(web.config.max_volume_db),
    );
    if let Some(capped) = judged.capped.filter(|c| c.refused) {
        return error(
            StatusCode::CONFLICT,
            minus(&format!(
                "Capped at {:.1} dB by [web] max_volume_db: no step is that quiet",
                capped.cap_db
            )),
        );
    }
    let words = |raw: i64| minus(&units::text(control, raw).unwrap_or_else(|| raw.to_string()));
    match web
        .fleet
        .glide(&device.info, &control.key, judged.target)
        .await
    {
        Glided::Landed(Answer::Value(value)) => {
            let raw = value.raw.as_i64().unwrap_or(judged.target);
            axum::Json(serde_json::json!({ "landed": { "raw": raw, "text": words(raw) } }))
                .into_response()
        }
        Glided::Climbing { target, .. } => axum::Json(serde_json::json!({
            "climbing": { "target": target, "text": words(target) }
        }))
        .into_response(),
        Glided::Superseded => bare(StatusCode::NO_CONTENT),
        Glided::Landed(other) | Glided::Refused(other) => refused(other),
    }
}

/// A touch on the dial: glide to the live reading, which ends the climb on, whoever owns it.
async fn stop(State(web): State<Web>, headers: HeaderMap, Path(name): Path<String>) -> Response {
    let device = match writable(&web, &headers, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    let control = match volume_of(&device) {
        Ok(control) => control,
        Err(refused) => return refused,
    };
    let Some(reading) = reading(&device, control) else {
        return error(StatusCode::CONFLICT, "the volume has not been read yet");
    };
    glide(&web, &device, control, reading).await
}

/// Try now: end the device's wait between connect attempts.
async fn connect(State(web): State<Web>, headers: HeaderMap, Path(name): Path<String>) -> Response {
    let device = match writable(&web, &headers, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    match web.fleet.wake(&device.info).await {
        Answer::Error { message, .. } => error(StatusCode::CONFLICT, message),
        _ => axum::Json(serde_json::json!({})).into_response(),
    }
}

/// Identify: ask the device to show itself, through the session's one send per gap. Answers
/// `{"sent": bool}` (`false` inside the gap: nothing went out); a device that is down answers 409
/// in the daemon's words with the wait to the next attempt where there is one.
async fn identify(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(name): Path<String>,
) -> Response {
    let device = match writable(&web, &headers, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    let asked = Request::Identify {
        device: Some(device.info.name.clone()),
    };
    match web.fleet.request(asked).await {
        Answer::Identify { sent, .. } => {
            axum::Json(serde_json::json!({ "sent": sent })).into_response()
        }
        Answer::Error { message, .. } if message == NOT_CONNECTED => {
            let wait = device
                .info
                .retry_in_secs
                .map(|secs| format!(", next attempt in {secs} s"))
                .unwrap_or_default();
            error(StatusCode::CONFLICT, format!("{message}{wait}"))
        }
        other => refused(other),
    }
}

/// A daemon's refusal as a 409 in its own words.
pub(super) fn refused(answer: Answer) -> Response {
    match answer {
        Answer::Error { message, .. } => error(StatusCode::CONFLICT, message),
        other => error(
            StatusCode::CONFLICT,
            format!("the device answered {other:?}"),
        ),
    }
}

/// How a write ended, as the page is answered: what `json` makes of the device's echo for a
/// component, with the parts `parts` as they stand now; 204 for a write a newer one replaced; 409
/// with the words of a refusal.
pub(crate) fn outcome(
    web: &Web,
    name: &str,
    ended: Outcome,
    parts: &[Part],
    json: impl FnOnce(&ValueObject) -> serde_json::Value,
) -> Response {
    let answer = match ended {
        Outcome::Superseded => return bare(StatusCode::NO_CONTENT),
        Outcome::Cancelled(why) => return error(StatusCode::CONFLICT, why),
        Outcome::Done(answer) => answer,
    };
    let Answer::Value(value) = answer else {
        return refused(answer);
    };
    with_parts(web, name, json(&value), parts)
}

/// `json` with the parts `parts` of device `name` as they stand now under `parts`, for the
/// signed-in viewer a write's gate let through; a part the device does not have is left out, and
/// so are all of them where the device cannot be read again (the write was made, and the stream
/// carries what it changed). The presets part reads the configuration once, any other part not
/// at all.
pub(super) fn with_parts(
    web: &Web,
    name: &str,
    mut json: serde_json::Value,
    parts: &[Part],
) -> Response {
    let loaded = match parts.contains(&Part::Presets) {
        true => load(web, name),
        false => load_live(web, name),
    };
    if let (Ok(device), Some(object)) = (loaded, json.as_object_mut()) {
        let screen = view::screen(&device.snapshot(web, access(web, Viewer::Signed)));
        let model = model::device_model(&screen);
        let shown: serde_json::Map<String, serde_json::Value> = parts
            .iter()
            .map(|part| (part.name().to_string(), model::part_value(&model, *part)))
            .filter(|(_, value)| !value.is_null())
            .collect();
        object.insert("parts".into(), shown.into());
    }
    axum::Json(json).into_response()
}

/// A control's echo as a component reads it: `{"raw","text"}` in the core's words, a switch's
/// raw as 1 or 0.
fn echo(control: &Control, value: &ValueObject) -> serde_json::Value {
    let raw = value
        .raw
        .as_i64()
        .or_else(|| value.raw.as_bool().map(i64::from));
    let text = raw
        .and_then(|raw| units::text(control, raw))
        .map_or_else(|| value.display.clone(), |t| minus(&t));
    serde_json::json!({ "raw": raw, "text": text })
}

#[cfg(test)]
mod tests;
