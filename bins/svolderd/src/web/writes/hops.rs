// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The hops after the DAC from the page: add one (declared by hand or linked to a configured
//! device), edit, remove and move one. Each edits the daemon's `config.toml` through the core's
//! writers, under their lock, then has the daemon reload it, as `svolder hops` does, answers the
//! hops as the model lists them and tells every open page the fleet changed. What a writer
//! refuses (a name taken, a blank name) is a 422 in its words; a hop that is not there is a 404.
//! A hop that links a device cannot take a name or kind of its own, as on the command line.

use std::path::Path as FilePath;

use axum::Router;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::post;
use hyper::{HeaderMap, StatusCode};
use serde::{Deserialize, Deserializer};
use svolder_ipc::config::{Hop, HopKind, HopWhat};
use svolder_ipc::{Config, ConfigError};
use svolder_profiles::tone::preset::same_name;

use super::config_edit::{config_file, edit_then_reload};
use super::writer;
use crate::web::Web;
use crate::web::model::fleet_model;
use crate::web::router::{devices, error, parse};

/// What a refused write calls itself: `Saving hops is off: …`.
const WHAT: &str = "hops";

/// The hops' routes.
pub(super) fn routes() -> Router<Web> {
    Router::new()
        .route("/api/v1/hops", post(add))
        .route("/api/v1/hops/edit", post(edit))
        .route("/api/v1/hops/remove", post(remove))
        .route("/api/v1/hops/move", post(move_to))
}

/// A writer's refusal as a 422 in its words; a failure of the file as a 409.
fn refusal(why: ConfigError) -> Response {
    match why {
        ConfigError::Parse(words) => error(StatusCode::UNPROCESSABLE_ENTITY, words),
        other => error(StatusCode::CONFLICT, other.to_string()),
    }
}

/// A 422 in `words`.
fn refused(words: impl Into<String>) -> Response {
    error(StatusCode::UNPROCESSABLE_ENTITY, words)
}

/// The 404 for an id no hop carries.
fn no_such_hop(id: &str) -> Response {
    error(StatusCode::NOT_FOUND, format!("no hop named {id:?}"))
}

/// The kind `word` names, or the 422 that lists the words.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn kind_of(word: &str) -> Result<HopKind, Response> {
    HopKind::named(word)
        .ok_or_else(|| refused(format!("kind {word:?} is not one of {}", HopKind::words())))
}

/// After a write: tell the open pages and answer the hops as the model lists them.
async fn answer(web: &Web) -> Response {
    web.fleet_changed.send_modify(|turn| *turn += 1);
    let fleet = fleet_model(web, &devices(web).await).await;
    axum::Json(serde_json::json!({ "hops": fleet.hops })).into_response()
}

/// Make the edit `edit` of the file at `path`, then answer.
async fn written(
    web: &Web,
    path: std::path::PathBuf,
    edit: impl FnOnce(&FilePath) -> Result<(), ConfigError> + Send + 'static,
) -> Response {
    match edit_then_reload(web, path, refusal, edit).await {
        Ok(()) => answer(web).await,
        Err(refused) => refused,
    }
}

/// What `hops` takes: a name and a kind for a declared hop, or a configured device to link.
#[derive(Deserialize)]
struct AddWrite {
    name: Option<String>,
    kind: Option<String>,
    device: Option<String>,
    note: Option<String>,
    after: Option<String>,
}

/// Add a hop at the end of the list.
async fn add(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let asked = match parse::<AddWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let (path, config) = match config_file(&web, WHAT) {
        Ok(found) => found,
        Err(refused) => return refused,
    };
    let what = match (asked.name, asked.kind, asked.device) {
        (Some(name), Some(kind), None) => match kind_of(&kind) {
            Ok(kind) => HopWhat::Declared { kind, name },
            Err(refused) => return refused,
        },
        (None, None, Some(device)) => match configured(&config, &device) {
            Ok(device) => HopWhat::Linked { device },
            Err(refused) => return refused,
        },
        _ => return refused("a hop takes a name and a kind, or a device"),
    };
    let hop = Hop {
        what,
        note: asked.note,
        after: asked.after,
    };
    written(&web, path, move |path| Config::add_hop(path, &hop)).await
}

/// The name the configuration gives the device `device` it holds, or the 422 that lists them.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
fn configured(config: &Config, device: &str) -> Result<String, Response> {
    match config
        .devices
        .iter()
        .find(|configured| same_name(&configured.name, device))
    {
        Some(found) => Ok(found.name.clone()),
        None => {
            let names: Vec<&str> = config.devices.iter().map(|d| d.name.as_str()).collect();
            let known = match names.is_empty() {
                true => "none is configured".to_string(),
                false => format!("the configured devices are {}", names.join(", ")),
            };
            Err(refused(format!(
                "no configured device is named {device:?}; {known}"
            )))
        }
    }
}

/// What `hops/edit` takes: the hop's id and what to change. A key left out keeps what the hop
/// has; `note: null` takes the note away.
#[derive(Deserialize)]
struct EditWrite {
    id: String,
    name: Option<String>,
    kind: Option<String>,
    #[serde(default, deserialize_with = "present")]
    note: Option<Option<String>>,
    after: Option<String>,
}

/// A key that is there, `null` or not, as `Some`; one that is left out stays `None` by default.
fn present<'de, D, T>(from: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(from).map(Some)
}

/// Change a hop.
async fn edit(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let asked = match parse::<EditWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let (path, config) = match config_file(&web, WHAT) {
        Ok(found) => found,
        Err(refused) => return refused,
    };
    let Some(old) = config.hop(&asked.id) else {
        return no_such_hop(&asked.id);
    };
    let what = match (&old.what, asked.name, asked.kind) {
        (HopWhat::Declared { kind, name }, new_name, new_kind) => HopWhat::Declared {
            kind: match new_kind {
                Some(word) => match kind_of(&word) {
                    Ok(kind) => kind,
                    Err(refused) => return refused,
                },
                None => *kind,
            },
            name: new_name.unwrap_or_else(|| name.clone()),
        },
        (HopWhat::Linked { device }, None, None) => HopWhat::Linked {
            device: device.clone(),
        },
        (HopWhat::Linked { device }, _, _) => {
            return refused(format!(
                "{device:?} links a device, which gives it its name and kind; \
                 remove the hop and add it declared to name it"
            ));
        }
    };
    let hop = Hop {
        what,
        note: match asked.note {
            Some(note) => note,
            None => old.note.clone(),
        },
        after: asked.after.or_else(|| old.after.clone()),
    };
    let id = old.id().to_string();
    written(&web, path, move |path| Config::update_hop(path, &id, &hop)).await
}

/// What `hops/remove` takes.
#[derive(Deserialize)]
struct RemoveWrite {
    id: String,
}

/// Take a hop out.
async fn remove(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let asked = match parse::<RemoveWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let (path, config) = match config_file(&web, WHAT) {
        Ok(found) => found,
        Err(refused) => return refused,
    };
    let Some(id) = config.hop(&asked.id).map(|hop| hop.id().to_string()) else {
        return no_such_hop(&asked.id);
    };
    written(&web, path, move |path| Config::remove_hop(path, &id)).await
}

/// What `hops/move` takes: the hop's id and its new place, counted from 0.
#[derive(Deserialize)]
struct MoveWrite {
    id: String,
    to: usize,
}

/// Put a hop at a place in the list.
async fn move_to(State(web): State<Web>, headers: HeaderMap, body: axum::body::Bytes) -> Response {
    if let Err(refused) = writer(&web, &headers) {
        return refused;
    }
    let asked = match parse::<MoveWrite>(&body) {
        Ok(asked) => asked,
        Err(refused) => return *refused,
    };
    let (path, config) = match config_file(&web, WHAT) {
        Ok(found) => found,
        Err(refused) => return refused,
    };
    let Some(id) = config.hop(&asked.id).map(|hop| hop.id().to_string()) else {
        return no_such_hop(&asked.id);
    };
    let to = asked.to;
    written(&web, path, move |path| Config::move_hop(path, &id, to)).await
}

#[cfg(test)]
mod tests;
