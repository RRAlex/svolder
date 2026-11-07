// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The user's presets from the page: save the bands as a new preset, rename one, delete one.
//! Each edits the daemon's `config.toml` through the core's writers, under their lock, then
//! has the daemon reload it, as `svolder eq save` does, so the MQTT bridge's preset list
//! follows. A name is refused in the core's one wording ([`preset::new_name`]); a built-in is
//! never touched; a taken name is never overwritten.

use std::path::Path as FilePath;

use axum::Router;
use axum::extract::{Path, State};
use axum::response::Response;
use axum::routing::post;
use hyper::{HeaderMap, StatusCode};
use serde::Deserialize;
use svolder_ipc::Config;
use svolder_profiles::tone::preset;

use super::config_edit::{config_file, conflict, edit_then_reload};
use super::tone::{eq_of, raw_of};
use super::{with_parts, writable};
use crate::web::Web;
use crate::web::model::Part;
use crate::web::router::{error, parse};

/// The presets' routes.
pub(super) fn routes() -> Router<Web> {
    Router::new()
        .route("/api/v1/devices/{device}/presets", post(save))
        .route("/api/v1/devices/{device}/presets/rename", post(rename))
        .route("/api/v1/devices/{device}/presets/delete", post(delete))
}

/// `{"name"}` with the presets part.
fn answer(web: &Web, device: &str, name: &str) -> Response {
    with_parts(
        web,
        device,
        serde_json::json!({ "name": name }),
        &[Part::Presets],
    )
}

/// A name the core refuses, as a 422 in its words.
fn refused_name(why: impl std::fmt::Display) -> Response {
    error(StatusCode::UNPROCESSABLE_ENTITY, why.to_string())
}

/// What `presets` takes.
#[derive(Deserialize)]
struct SaveWrite {
    name: String,
    /// `"computer"` saves the computer EQ's points instead of the device's bands.
    from: Option<String>,
}

/// Save the device's bands, mode and loudness as a new preset, or, with `"from": "computer"`,
/// every point of the computer EQ (as the core's imported preset, [`preset::from_computer`]).
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
    let from_computer = match asked.from.as_deref() {
        None | Some("device") => false,
        Some("computer") => true,
        Some(other) => {
            return error(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("cannot save from {other:?}; use device or computer"),
            );
        }
    };
    let (path, config) = match config_file(&web, "presets") {
        Ok(found) => found,
        Err(refused) => return refused,
    };
    let names = config.presets.iter().map(|p| p.name.as_str());
    let wanted = match preset::new_name(&asked.name, names, None) {
        Ok(wanted) => wanted.to_string(),
        Err(why) => return refused_name(why),
    };
    if from_computer {
        let op = svolder_ipc::HostEqOp::SaveAs {
            name: wanted.clone(),
        };
        return match web.fleet.request(svolder_ipc::Request::HostEq { op }).await {
            svolder_ipc::Response::Ack => answer(&web, &name, &wanted),
            svolder_ipc::Response::Error { message, .. } => error(StatusCode::CONFLICT, message),
            _ => error(StatusCode::BAD_GATEWAY, "the daemon gave no answer"),
        };
    }
    let (_, eq) = match eq_of(&device) {
        Ok(found) => found,
        Err(refused) => return refused,
    };
    let Some(saved) = preset::from_readings(&eq, |id| raw_of(&device, id), wanted.clone()) else {
        return error(
            StatusCode::CONFLICT,
            "the tone control has not been read yet",
        );
    };
    let edit = move |path: &FilePath| Config::add_preset(path, &saved);
    if let Err(refused) = edit_then_reload(&web, path, conflict, edit).await {
        return refused;
    }
    answer(&web, &name, &wanted)
}

/// What `presets/rename` takes.
#[derive(Deserialize)]
struct RenameWrite {
    from: String,
    to: String,
}

/// Rename one of the user's presets.
async fn rename(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    if let Err(refused) = writable(&web, &headers, &name) {
        return refused;
    }
    let asked = match parse::<RenameWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    if preset::is_builtin(&asked.from) {
        return error(
            StatusCode::CONFLICT,
            format!(
                "{:?} is a built-in preset; it cannot be renamed",
                asked.from.trim()
            ),
        );
    }
    let (path, config) = match config_file(&web, "presets") {
        Ok(found) => found,
        Err(refused) => return refused,
    };
    let Some(old) = config.preset(&asked.from).map(|p| p.name.clone()) else {
        return error(
            StatusCode::NOT_FOUND,
            format!("no preset named {:?}", asked.from.trim()),
        );
    };
    let names = config.presets.iter().map(|p| p.name.as_str());
    let new = match preset::new_name(&asked.to, names, Some(&old)) {
        Ok(new) => new.to_string(),
        Err(why) => return refused_name(why),
    };
    let to = new.clone();
    let edit = move |path: &FilePath| Config::rename_preset(path, &old, &to);
    if let Err(refused) = edit_then_reload(&web, path, conflict, edit).await {
        return refused;
    }
    answer(&web, &name, &new)
}

/// What `presets/delete` takes.
#[derive(Deserialize)]
struct DeleteWrite {
    name: String,
}

/// Delete one of the user's presets. The bands on the device stay as they are.
async fn delete(
    State(web): State<Web>,
    headers: HeaderMap,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> Response {
    if let Err(refused) = writable(&web, &headers, &name) {
        return refused;
    }
    let asked = match parse::<DeleteWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    if preset::is_builtin(&asked.name) {
        return error(
            StatusCode::CONFLICT,
            format!(
                "{:?} is a built-in preset; it cannot be deleted",
                asked.name.trim()
            ),
        );
    }
    let (path, config) = match config_file(&web, "presets") {
        Ok(found) => found,
        Err(refused) => return refused,
    };
    let Some(gone) = config.preset(&asked.name).map(|p| p.name.clone()) else {
        return error(
            StatusCode::NOT_FOUND,
            format!("no preset named {:?}", asked.name.trim()),
        );
    };
    let taken = gone.clone();
    let edit = move |path: &FilePath| Config::remove_preset(path, &taken);
    if let Err(refused) = edit_then_reload(&web, path, conflict, edit).await {
        return refused;
    }
    answer(&web, &name, &gone)
}

#[cfg(test)]
mod tests;
