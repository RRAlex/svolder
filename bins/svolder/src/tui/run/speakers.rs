// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the set list asks of the daemon, run off the loop: the catalogue, a fetch or an import
//! with its progress, and a forget, through the binary's one client ([`crate::speakers`]).
//!
//! Each call runs on a task of its own, so nothing else the loop sends waits behind a download.
//! What a call says on the way, and how it ends, goes back to the loop as a [`SpeakersDone`]. A
//! fetch or import is stopped by its [`Stop`] (the client
//! cancels the job on the daemon), and a task that is dropped while its job runs cancels the job
//! as well (the client's drop guard): [`Running`] aborts the task when the loop lets go of it.

use std::path::{Path, PathBuf};

use svolder_ipc::SpeakersProgress;
use tokio::sync::mpsc::UnboundedSender;
use tokio::task::{AbortHandle, JoinHandle};

use crate::speakers::{self, NEEDS_DAEMON, Refused};
use crate::tui::boxes::presets::Done;
use crate::tui::boxes::speakers::{CANCELLED, SpeakersCall, SpeakersDone, Stop};

/// The task of a fetch or an import in flight, aborted when this is dropped.
pub(in crate::tui) struct Running {
    task: Option<JoinHandle<()>>,
    abort: AbortHandle,
    stop: Option<Stop>,
}

impl Running {
    /// Leave the job: stop it, which has the client cancel it on the daemon, and wait for the
    /// task to end for at most `wait`, so the cancel is out before the runtime goes. A task
    /// still running after that is aborted when this drops.
    pub(in crate::tui) async fn finish(mut self, wait: std::time::Duration) {
        if let (Some(stop), Some(task)) = (self.stop.take(), self.task.take()) {
            stop.press();
            let _ = tokio::time::timeout(wait, task).await;
        }
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        self.abort.abort();
    }
}

/// What a refusal says in the set list: the daemon's words, a stop as a stop.
fn words(refused: &Refused) -> String {
    match refused.interrupted {
        true => CANCELLED.to_string(),
        false => refused.message.clone(),
    }
}

/// Carry out `call` against the daemon at `socket` (none: this session holds no daemon), on a
/// task of its own, and send what it says to `done`. The task of a fetch or an import is
/// answered, so the caller can keep it.
pub(super) fn run(
    done: UnboundedSender<Done>,
    socket: Option<PathBuf>,
    call: SpeakersCall,
) -> Option<Running> {
    let stop = match &call {
        SpeakersCall::Fetch { stop, .. } | SpeakersCall::Import { stop, .. } => Some(stop.clone()),
        _ => None,
    };
    let task = tokio::spawn(async move {
        let send = |what: SpeakersDone| {
            let _ = done.send(Done::Speakers(Box::new(what)));
        };
        let Some(socket) = socket else {
            fail_without_daemon(call, send);
            return;
        };
        match call {
            SpeakersCall::Catalogue => send(SpeakersDone::Catalogue(
                speakers::catalogue(&socket)
                    .await
                    .map(Box::new)
                    .map_err(|refused| refused.message),
            )),
            SpeakersCall::Forget { id } => {
                let result = speakers::forget(&socket, &id)
                    .await
                    .map_err(|refused| refused.message);
                send(SpeakersDone::Forgotten { id, result });
            }
            SpeakersCall::Fetch { id, stop, .. } => {
                let result =
                    speakers::fetch(&socket, &id, Some(stop.0), &mut |p| progress(&send, &id, p))
                        .await
                        .map(Box::new)
                        .map_err(|refused| words(&refused));
                send(SpeakersDone::Ended { id, result });
            }
            SpeakersCall::Import { path, room, stop } => {
                let result =
                    speakers::import(&socket, Path::new(&path), None, Some(room), Some(stop.0))
                        .await
                        .map(Box::new)
                        .map_err(|refused| words(&refused));
                send(SpeakersDone::Ended { id: path, result });
            }
        }
    });
    stop.map(|stop| Running {
        abort: task.abort_handle(),
        task: Some(task),
        stop: Some(stop),
    })
}

/// Say a download's progress.
fn progress(send: &impl Fn(SpeakersDone), id: &str, report: &SpeakersProgress) {
    send(SpeakersDone::Progress {
        id: id.to_string(),
        received: report.received,
        total: report.total,
    });
}

/// What a call says when the loop holds no daemon: the sets are the daemon's.
fn fail_without_daemon(call: SpeakersCall, send: impl Fn(SpeakersDone)) {
    let why = NEEDS_DAEMON.to_string();
    match call {
        SpeakersCall::Catalogue => send(SpeakersDone::Catalogue(Err(why))),
        SpeakersCall::Fetch { id, .. } => send(SpeakersDone::Ended {
            id,
            result: Err(why),
        }),
        SpeakersCall::Import { path, .. } => send(SpeakersDone::Ended {
            id: path,
            result: Err(why),
        }),
        SpeakersCall::Forget { id } => send(SpeakersDone::Forgotten {
            id,
            result: Err(why),
        }),
    }
}

#[cfg(test)]
mod tests;
