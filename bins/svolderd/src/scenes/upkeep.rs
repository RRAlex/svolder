// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Renaming and deleting a scene. The recalled-scene memory follows a rename and forgets a
//! delete.

use svolder_ipc::{Config, ConfigError, Response};

use crate::handler::{ConfigWriteError, Daemon};

/// The refusal for a scene the config does not have.
fn missing(name: &str) -> Response {
    ConfigWriteError::Refused(ConfigError::Missing {
        kind: "scene",
        name: name.trim().to_string(),
    })
    .answer()
}

/// The scene called `from`, as the config spells it, or the refusal that says it is not there
/// (boxed: a `Response` is large).
async fn spelled(daemon: &Daemon, from: &str) -> Result<String, Box<Response>> {
    let held = daemon
        .config_for_write("scenes")
        .await
        .map_err(|why| Box::new(why.answer()))?;
    held.scene(from)
        .map(|found| found.name.clone())
        .ok_or_else(|| Box::new(missing(from)))
}

/// Rename the scene `from` to `to`, leaving what it holds as it is.
pub(crate) async fn rename(daemon: &Daemon, from: String, to: String) -> Response {
    let old = match spelled(daemon, &from).await {
        Ok(old) => old,
        Err(refused) => return *refused,
    };
    let new = to.trim().to_string();
    let (was, now) = (old.clone(), to);
    let edit = Box::new(move |path: &std::path::Path| Config::rename_scene(path, &was, &now));
    if let Err(why) = daemon.edit_then_reload(edit).await {
        return why.answer();
    }
    let scenes = daemon.scenes();
    if scenes.renamed(&old, &new) {
        scenes.flush().await;
    }
    scenes.changed(daemon);
    Response::Scene {
        name: new,
        outcome: Vec::new(),
    }
}

/// Delete the scene `name`.
pub(crate) async fn delete(daemon: &Daemon, name: String) -> Response {
    let gone = match spelled(daemon, &name).await {
        Ok(gone) => gone,
        Err(refused) => return *refused,
    };
    let taken = gone.clone();
    let edit = Box::new(move |path: &std::path::Path| Config::remove_scene(path, &taken));
    if let Err(why) = daemon.edit_then_reload(edit).await {
        return why.answer();
    }
    let scenes = daemon.scenes();
    if scenes.forgotten(&gone) {
        scenes.flush().await;
    }
    scenes.changed(daemon);
    Response::Scene {
        name: gone,
        outcome: Vec::new(),
    }
}
