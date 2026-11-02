// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the page's configuration writes share (the user's presets, the hops): the file they edit
//! and the edit itself, made through the core's writers, under their lock, then a reload of the
//! daemon, as the CLI's writes do, so everything that follows the file follows the page.

use std::path::{Path as FilePath, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use axum::response::Response;
use hyper::StatusCode;
use svolder_ipc::{Config, ConfigError, Request};

use crate::log;
use crate::web::Web;
use crate::web::router::error;

/// How long a configuration write waits for the daemon's reload before it answers anyway; the
/// reload goes on.
const RELOAD_WAIT: Duration = Duration::from_secs(5);

/// The configuration file a write of `what` (`presets`, `hops`) edits, and what it holds now:
/// 409 with why not when it cannot be written.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(super) fn config_file(web: &Web, what: &str) -> Result<(PathBuf, Config), Response> {
    let Some(path) = web.fleet.config_path() else {
        return Err(error(
            StatusCode::CONFLICT,
            "svolder's configuration file is unknown here",
        ));
    };
    if let Err(why) = Config::writable(&path) {
        return Err(error(
            StatusCode::CONFLICT,
            format!("Saving {what} is off: {why}"),
        ));
    }
    match Config::load_from(&path) {
        Ok(config) => Ok((path, config)),
        Err(_) => Err(error(
            StatusCode::CONFLICT,
            "the daemon's configuration could not be read; see its log",
        )),
    }
}

/// A writer's refusal as a 409 in its words.
pub(super) fn conflict(why: ConfigError) -> Response {
    error(StatusCode::CONFLICT, why.to_string())
}

/// Run `edit` on the file at `path` off the async threads, then have the daemon reload it; a
/// writer's refusal is answered by `refuse`. The reload is waited for no longer than
/// [`RELOAD_WAIT`]: a reload that finds `[web]` changed in the file stops this very server, and
/// this answer must not be what holds that stop up.
#[allow(
    clippy::result_large_err,
    reason = "the refusal is the answer the handler returns as it is"
)]
pub(super) async fn edit_then_reload(
    web: &Web,
    path: PathBuf,
    refuse: fn(ConfigError) -> Response,
    edit: impl FnOnce(&FilePath) -> Result<(), ConfigError> + Send + 'static,
) -> Result<(), Response> {
    match tokio::task::spawn_blocking(move || edit(&path)).await {
        Ok(Ok(())) => {}
        Ok(Err(why)) => return Err(refuse(why)),
        Err(_) => {
            return Err(error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "the configuration write stopped",
            ));
        }
    }
    let fleet = Arc::clone(&web.fleet);
    let reload = tokio::spawn(async move { fleet.request(Request::Reload).await });
    match tokio::time::timeout(RELOAD_WAIT, reload).await {
        Ok(Ok(svolder_ipc::Response::Error { message, .. })) => {
            log::warn(&format!(
                "web: the reload after a configuration write failed: {message}"
            ));
        }
        Ok(Err(_)) => log::warn("web: the reload after a configuration write stopped"),
        _ => {}
    }
    Ok(())
}
