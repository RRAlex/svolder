// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `daemon reload`: have the running daemon read its configuration file and the user's profiles
//! again and apply them, without restarting it or dropping a link it keeps.
//!
//! The same request every `svolder` verb that changes the file sends after its write, here on
//! its own: it is the way out of the `config changed since the last reload` line `status`,
//! `doctor`, the TUI and the web page show after a hand edit. The daemon answers with the
//! devices it now runs, or refuses with the file and line it could not read, in which case
//! nothing changed and the verb exits with the daemon's own code (1), never the 3 of no daemon.
//! A configuration file this `svolder` cannot read itself is refused first, before the daemon
//! is asked (exit 1, naming the file).

use std::path::Path;

use svolder_ipc::socket::daemon_version::is_config_missing_refusal;
use svolder_ipc::{Client, Request, Response};

use crate::backend::Globals;
use crate::recovery::{Failure, Kind};

use super::resolved_socket;

/// Ask the daemon to reload and print the devices it now runs.
pub async fn reload(g: &Globals<'_>) -> Result<(), Failure> {
    let socket = resolved_socket(g)?;
    println!("{}", reload_on(&socket).await?);
    Ok(())
}

/// [`reload`] against the daemon on `socket`: what to print when it reloaded.
async fn reload_on(socket: &Path) -> Result<String, Failure> {
    let mut client = Client::connect(socket).await.map_err(|e| {
        Failure::of(
            Kind::NoDaemon,
            format!("no daemon answers on {}: {e}", socket.display()),
        )
    })?;
    match client
        .request_within(Request::Reload, Some(svolder_ipc::RELOAD_TIMEOUT))
        .await
    {
        Ok(Response::Reloaded { devices }) => Ok(match devices.is_empty() {
            true => "reloaded: the daemon holds no devices".to_string(),
            false => format!("reloaded: the daemon now holds {}", devices.join(", ")),
        }),
        // The daemon's own code and reason, as every answered verb exits: a refusal is not the
        // code of a missing daemon.
        Ok(Response::Error {
            code,
            message,
            reason,
        }) => Err(Failure {
            code,
            what: format!("the daemon did not reload: {message}"),
            next: match reason {
                Some(svolder_ipc::Reason::Stopping) => {
                    "The daemon is stopping: run `svolder daemon restart`".to_string()
                }
                // the missing-file words give the step themselves
                _ if is_config_missing_refusal(&message) => String::new(),
                _ => "Fix or restore the file it names, then run `svolder daemon reload` again"
                    .to_string(),
            },
            reason,
        }),
        Ok(other) => Err(Failure::of(
            Kind::DaemonError,
            format!("the daemon answered {other:?} rather than reloading"),
        )),
        Err(e) => Err(Failure::of(
            Kind::DaemonError,
            format!("the daemon did not reload: {e}"),
        )),
    }
}

#[cfg(test)]
mod tests;
