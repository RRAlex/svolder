// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the Scenes box asks of the daemon, run off the loop: one scene request on a connection of
//! its own, answered as a [`SceneDone`] that carries the op it was for.
//!
//! A recall writes to every connected device, so its wait is long; a refusal comes back in the
//! daemon's own words.

use std::path::PathBuf;

use svolder_ipc::socket::protocol::SceneOp;
use svolder_ipc::{DEFAULT_REQUEST_TIMEOUT, IpcError, Request, Response};
use tokio::sync::mpsc::UnboundedSender;

use crate::tui::boxes::presets::Done;
use crate::tui::boxes::scenes::{NEEDS_DAEMON, SceneDone};

/// What the daemon's reply to `op` means for the box.
fn meant(op: SceneOp, reply: Result<Response, IpcError>) -> SceneDone {
    match reply {
        Ok(Response::Scene { name, outcome }) => SceneDone::Answered { op, name, outcome },
        Ok(Response::Ask { message, .. }) => SceneDone::Asked { op, message },
        // A refusal arrives as an answer of its own: its message is the daemon's words.
        Ok(Response::Error { message, .. }) => SceneDone::Refused { op, words: message },
        Ok(other) => SceneDone::Refused {
            op,
            words: format!("the daemon sent an unexpected reply: {other:?}"),
        },
        Err(IpcError::Refused(answer)) => match *answer {
            Response::Error { message, .. } => SceneDone::Refused { op, words: message },
            other => SceneDone::Refused {
                op,
                words: format!("the daemon sent an unexpected reply: {other:?}"),
            },
        },
        Err(e) => SceneDone::Refused {
            op,
            words: crate::recovery::Failure::from_ipc(&e).what,
        },
    }
}

/// Send `op` to the daemon at `socket` (none: this session holds no daemon) on a task of its own,
/// and send what it says to `done`.
pub(super) fn run(done: UnboundedSender<Done>, socket: Option<PathBuf>, op: SceneOp) {
    tokio::spawn(async move {
        let said = match socket {
            None => SceneDone::Refused {
                op,
                words: NEEDS_DAEMON.to_string(),
            },
            Some(socket) => {
                let request = Request::Scene { op: op.clone() };
                let reply = svolder_ipc::request_on_own_connection(
                    &socket,
                    request,
                    Some(DEFAULT_REQUEST_TIMEOUT),
                )
                .await;
                meant(op, reply)
            }
        };
        let _ = done.send(Done::Scene(Box::new(said)));
    });
}

#[cfg(test)]
mod tests;
