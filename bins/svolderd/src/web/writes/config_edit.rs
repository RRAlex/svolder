// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the page's configuration writes share (the user's presets, the hops): the edit, which the
//! daemon makes ([`crate::handler`]'s `config_write`) through the core's writers, under their
//! lock, then a reload, as the CLI's writes do, so everything that follows the file follows the
//! page. What is here only maps the daemon's answers to the page's, and hands the hops the path
//! of the file their answer reads back.

use std::path::{Path as FilePath, PathBuf};
use std::sync::Arc;

use axum::response::Response;
use hyper::StatusCode;
use svolder_ipc::{Config, ConfigError};

use crate::handler::ConfigWriteError;
use crate::web::Web;
use crate::web::router::error;

/// A write that did not happen, as the answer to the page: a name the core refuses is a 422 in
/// its words; every other refusal is `refuse`'s, or a 409 or a 500.
fn refused_write(why: ConfigWriteError, refuse: fn(ConfigError) -> Response) -> Response {
    match why {
        ConfigWriteError::Unknown | ConfigWriteError::Off(_) => {
            error(StatusCode::CONFLICT, why.to_string())
        }
        ConfigWriteError::Name(refusal) => {
            error(StatusCode::UNPROCESSABLE_ENTITY, refusal.to_string())
        }
        ConfigWriteError::Builtin(words) => error(StatusCode::CONFLICT, words),
        ConfigWriteError::NoSuchPreset(words) => error(StatusCode::NOT_FOUND, words),
        ConfigWriteError::Refused(why) => refuse(why),
        ConfigWriteError::Stopped | ConfigWriteError::Unchecked => {
            error(StatusCode::INTERNAL_SERVER_ERROR, why.to_string())
        }
        ConfigWriteError::Stopping => error(StatusCode::SERVICE_UNAVAILABLE, why.to_string()),
    }
}

/// What the daemon holds of the configuration from its last reload, for a write of `what`
/// (`presets`, `hops`) that checks names against it: 409 with why not when the file cannot be
/// written.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(super) async fn config_for(web: &Web, what: &str) -> Result<Arc<Config>, Response> {
    web.fleet
        .config_for_write(what)
        .await
        .map_err(|why| refused_write(why, conflict))
}

/// [`config_for`], and the path of the file a write of `what` edits.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(super) async fn config_file(web: &Web, what: &str) -> Result<(PathBuf, Arc<Config>), Response> {
    let Some(path) = web.fleet.config_path() else {
        return Err(refused_write(ConfigWriteError::Unknown, conflict));
    };
    Ok((path, config_for(web, what).await?))
}

/// A writer's refusal as a 409 in its words.
pub(super) fn conflict(why: ConfigError) -> Response {
    error(StatusCode::CONFLICT, why.to_string())
}

/// Run `edit` on the file off the async threads, then have the daemon reload it; a writer's
/// refusal is answered by `refuse`.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(super) async fn edit_then_reload(
    web: &Web,
    refuse: fn(ConfigError) -> Response,
    edit: impl FnOnce(&FilePath) -> Result<(), ConfigError> + Send + 'static,
) -> Result<(), Response> {
    web.fleet
        .edit_config(Box::new(edit))
        .await
        .map_err(|why| refused_write(why, refuse))
}

/// Rename the user's preset `from` to `to`: a built-in is a 409, a preset that is not there a
/// 404, a name the core refuses a 422. Answers with the name as the file spells it.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(super) async fn rename_preset(web: &Web, from: &str, to: &str) -> Result<String, Response> {
    web.fleet
        .rename_preset(from, to)
        .await
        .map_err(|why| refused_write(why, conflict))
}

/// Delete the user's preset `name`: a built-in is a 409, a preset that is not there a 404.
/// Answers with the name as the file spelled it.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(super) async fn delete_preset(web: &Web, name: &str) -> Result<String, Response> {
    web.fleet
        .delete_preset(name)
        .await
        .map_err(|why| refused_write(why, conflict))
}

/// Save `saved` as the user's preset `name`, over the user's preset of that name when `force`: a
/// name the core refuses is a 422, a writer's refusal a 409. Answers with the name as the file spells it.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(super) async fn save_preset(
    web: &Web,
    name: &str,
    saved: svolder_profiles::tone::preset::Preset,
    force: bool,
) -> Result<String, Response> {
    web.fleet
        .save_preset(name, saved, force)
        .await
        .map_err(|why| refused_write(why, conflict))
}
