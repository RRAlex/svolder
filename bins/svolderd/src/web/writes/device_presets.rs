// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A device's own presets from the page: recall a slot, save the bands to one, apply a built-in
//! setting, each through the device's writer as the web's. A slot goes as the recall or save
//! command of its number, in order behind the writes in flight; a built-in setting goes as the
//! writer's one batch. A save replaces what the device holds in the slot and cannot be undone, so
//! it is made only when the body says `"confirm": true`, which the page's own form does and
//! nothing else does by accident.

use std::sync::Arc;

use axum::Router;
use axum::extract::{Path, State};
use axum::response::Response;
use axum::routing::post;
use hyper::{HeaderMap, StatusCode};
use serde::Deserialize;
use svolder_ipc::Response as Answer;
use svolder_profiles::device_presets;
use svolder_profiles::{Action, Profile, roles};

use super::{refused, with_parts, writable};
use crate::device::Command;
use crate::web::Web;
use crate::web::api::Loaded;
use crate::web::model::Part;
use crate::web::router::{bare, error, parse};
use crate::writer::{Mode, Options, Outcome};

/// The device presets' routes.
pub(super) fn routes() -> Router<Web> {
    Router::new()
        .route(
            "/api/v1/devices/{device}/device-presets/recall",
            post(recall),
        )
        .route("/api/v1/devices/{device}/device-presets/save", post(save))
        .route("/api/v1/devices/{device}/device-presets/apply", post(apply))
}

/// The profile of `device`: 409 before it is found, 404 when it has no presets of its own.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn profile_of(device: &Loaded) -> Result<Arc<Profile>, Response> {
    let Some(profile) = device.feed.profile.clone() else {
        return Err(error(
            StatusCode::CONFLICT,
            svolder_ipc::not_found_yet(&device.info.name),
        ));
    };
    if !device_presets::offered(&profile) {
        return Err(error(
            StatusCode::NOT_FOUND,
            format!("{} has no presets of its own", device.info.name),
        ));
    }
    Ok(profile)
}

/// The command `action` is, for slot `slot` of `asked`: 422 for a slot the device lacks.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn command_of(action: &Action, asked: i64) -> Result<(Command, u8), Response> {
    let slot = device_presets::parse_slot(&asked.to_string(), action.slots).ok_or_else(|| {
        error(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("a slot is 1 to {}", action.slots),
        )
    })?;
    let command = Command::Set {
        key: action.key.clone(),
        value: slot.to_string(),
        step: 1,
        no_wait: false,
    };
    Ok((command, slot))
}

/// How a slot's write ended, as the page is answered: `{"slot","name"}` with a toast saying what
/// was sent and the device presets part; 204 for a write a newer one replaced; 409 with the
/// words of a refusal.
fn slot_answer(web: &Web, device: &str, ended: Outcome, slot: u8, toast: &str) -> Response {
    match ended {
        Outcome::Superseded => bare(StatusCode::NO_CONTENT),
        Outcome::Cancelled(why) => error(StatusCode::CONFLICT, why),
        Outcome::Done(answer @ Answer::Error { .. }) => refused(answer),
        Outcome::Done(_) => sent(
            web,
            device,
            toast,
            serde_json::json!({ "slot": slot, "name": device_presets::slot_name(slot) }),
        ),
    }
}

/// `json` with the toast and the device presets part.
fn sent(web: &Web, device: &str, toast: &str, mut json: serde_json::Value) -> Response {
    json["toast"] = toast.into();
    with_parts(web, device, json, &[Part::DevicePresets])
}

/// What `device-presets/recall` takes.
#[derive(Deserialize)]
struct SlotWrite {
    slot: i64,
}

/// Recall a slot: the device sets its bands from it and reports them.
async fn recall(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    let device = match writable(&web, &headers, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    let asked = match parse::<SlotWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let profile = match profile_of(&device) {
        Ok(profile) => profile,
        Err(refused) => return refused,
    };
    let Some(action) = roles::recall_action(&profile) else {
        return error(
            StatusCode::NOT_FOUND,
            format!("{} has no slots to recall", device.info.name),
        );
    };
    let (command, slot) = match command_of(action, asked.slot) {
        Ok(found) => found,
        Err(refused) => return refused,
    };
    let ended = web
        .fleet
        .submit(&device.info, command, Options::from(Mode::Ordered))
        .await;
    let toast = format!("Recalled {}", device_presets::slot_name(slot));
    slot_answer(&web, &name, ended, slot, &toast)
}

/// What `device-presets/save` takes: the slot and the page's confirmation, which is `true` and
/// nothing else.
#[derive(Deserialize)]
struct SaveWrite {
    slot: i64,
    confirm: Option<serde_json::Value>,
}

/// Save the device's bands to a slot, over what it holds there.
async fn save(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    let device = match writable(&web, &headers, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    let asked = match parse::<SaveWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    if asked.confirm != Some(serde_json::Value::Bool(true)) {
        return error(
            StatusCode::UNPROCESSABLE_ENTITY,
            "a save replaces what the device holds in the slot: send \"confirm\": true to make it",
        );
    }
    let profile = match profile_of(&device) {
        Ok(profile) => profile,
        Err(refused) => return refused,
    };
    let Some(action) = roles::save_action(&profile) else {
        return error(
            StatusCode::NOT_FOUND,
            format!("{} has no slots to save to", device.info.name),
        );
    };
    let (command, slot) = match command_of(action, asked.slot) {
        Ok(found) => found,
        Err(refused) => return refused,
    };
    let ended = web
        .fleet
        .submit(&device.info, command, Options::from(Mode::Ordered))
        .await;
    let toast = format!("Saved to {}", device_presets::slot_name(slot));
    slot_answer(&web, &name, ended, slot, &toast)
}

/// What `device-presets/apply` takes.
#[derive(Deserialize)]
struct SettingWrite {
    name: String,
}

/// Apply a built-in setting as the writer's one batch: its bands are written in an order that
/// never sounds a boost first, and each write waits for the device's report.
async fn apply(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    let device = match writable(&web, &headers, &name) {
        Ok(device) => device,
        Err(refused) => return refused,
    };
    let asked = match parse::<SettingWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    if let Err(refused) = profile_of(&device) {
        return refused;
    }
    match web.fleet.apply(&device.info, &asked.name).await {
        Err(why) => error(StatusCode::CONFLICT, why),
        Ok((_, answer @ Answer::Error { .. })) => refused(answer),
        Ok((applied, _)) => {
            let toast = match applied.writes.is_empty() {
                true => format!("{} is already on", applied.name),
                false => format!("{} on", applied.name),
            };
            let json = serde_json::json!({ "name": applied.name, "writes": applied.writes.len() });
            sent(&web, &name, &toast, json)
        }
    }
}
