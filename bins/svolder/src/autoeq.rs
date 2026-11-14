// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a front end asks of the daemon's AutoEq service: search the index, import a measurement
//! as a preset, save a file's text as one.
//!
//! The daemon is the one process that reaches AutoEq's repository (see `docs/design/autoeq.md`);
//! this binary links no HTTP client. Each call here opens a connection of its own, so nothing
//! else the front end sends waits behind a slow fetch, and ends in the daemon's answer or its
//! refusal in words. `search` is a plain request. `import` is a job: the request
//! answers with the job's id, and the end comes on the family's follow stream, which is opened
//! after the start (the stream tells each job's last state first, so a job that has already
//! ended is read all the same). A call that is dropped while its job runs cancels the job.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;

use crate::commands::interrupts::Presses;
use svolder_ipc::socket::daemon_version::{RESTART_COMMAND, is_unknown_request};
use svolder_ipc::{
    AutoEqOp, AutoEqSaved, AutoEqSearch, AutoEqTarget, AutoEqVariant, Client, Event, IpcError,
    JobId, JobState, Reason, Request, Response, request_on_own_connection,
};

/// The longest a search waits: the daemon may be loading the index, which takes up to 60 s.
pub(crate) const SEARCH_WAIT: Duration = Duration::from_secs(90);
/// The longest the start of a job, or a plain request that reaches no network, waits.
const START_WAIT: Duration = Duration::from_secs(15);
/// The longest the follow stream takes to open.
const FOLLOW_WAIT: Duration = Duration::from_secs(10);
/// The longest a job is waited for: the index, two downloads of 20 s and the fit, with room.
const JOB_WAIT: Duration = Duration::from_secs(150);
/// The longest a cancel is waited for.
const CANCEL_WAIT: Duration = Duration::from_secs(3);
/// The longest the job's end is read after a cancel: a save that was under way runs to its end
/// (the file's lock wait, the write and the reload wait), and then says it saved.
const END_WAIT: Duration = Duration::from_secs(12);

/// What is said where there is no daemon to ask: the one process that reaches AutoEq.
pub(crate) const NEEDS_DAEMON: &str = "AutoEq needs svolderd, which fetches it";

/// A call the daemon did not carry out, in its own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refused {
    /// What went wrong.
    pub message: String,
    /// Why, where the daemon says: `not_listed` is a measurement whose file is not there.
    pub reason: Option<Reason>,
    /// Whether the daemon does not know the request: it is older than this svolder.
    pub too_old: bool,
    /// Whether the caller's interrupt ended the wait and the job was cancelled.
    pub interrupted: bool,
}

impl Refused {
    fn words(message: impl Into<String>) -> Refused {
        Refused {
            message: message.into(),
            reason: None,
            too_old: false,
            interrupted: false,
        }
    }

    /// The refusal for a front end with no `next` line of its own: a daemon too old is told how
    /// to restart.
    pub fn line(&self) -> String {
        match self.too_old {
            true => format!("{}; run {RESTART_COMMAND}", self.message),
            false => self.message.clone(),
        }
    }

    /// A reply that is not the answer the op asks for.
    fn of(reply: Result<Response, IpcError>) -> Refused {
        let reply = match reply {
            Err(IpcError::Refused(answer)) => Ok(*answer),
            other => other,
        };
        match reply {
            Ok(Response::Error {
                code,
                message,
                reason,
            }) => Refused::from_error(code, message, reason),
            Ok(other) => Refused::words(format!("the daemon answered {other:?}")),
            Err(e) => Refused::words(e.to_string()),
        }
    }

    /// The daemon's error answer: a daemon from before AutoEq says it does not know the request.
    fn from_error(code: u8, message: String, reason: Option<Reason>) -> Refused {
        match is_unknown_request(None, code, &message, "auto_eq") {
            true => Refused {
                too_old: true,
                ..Refused::words(crate::reach::too_old_words(None, "use AutoEq"))
            },
            false => Refused {
                message,
                reason,
                too_old: false,
                interrupted: false,
            },
        }
    }
}

/// Search the index on the daemon at `socket` for `query`; `refresh` fetches the index again
/// whatever the age of its copy.
pub(crate) async fn search(
    socket: &Path,
    query: &str,
    refresh: bool,
) -> Result<AutoEqSearch, Refused> {
    let reply =
        request_on_own_connection(socket, search_op(query, refresh), Some(SEARCH_WAIT)).await;
    search_answer(reply)
}

/// The op that searches for `query`.
pub(crate) fn search_op(query: &str, refresh: bool) -> Request {
    Request::AutoEq {
        op: AutoEqOp::Search {
            query: query.to_string(),
            refresh,
        },
    }
}

/// The search answer in a reply to [`search_op`], or the daemon's refusal.
pub(crate) fn search_answer(reply: Result<Response, IpcError>) -> Result<AutoEqSearch, Refused> {
    match reply {
        Ok(Response::AutoEqSearch(found)) => Ok(*found),
        other => Err(Refused::of(other)),
    }
}

/// Download `variant`'s correction and save it as an imported preset for `target`; `name` is the
/// preset's name (the headphone's own, shortened, when `None`), and `force` writes over the
/// user's preset of that name. Waits for the daemon's job to end.
///
/// `interrupt` is the presses of a signal the caller counts (the CLI's Ctrl-C): one cancels the
/// job on the daemon, once, and the call ends as refused, so nothing is saved after the user left.
pub(crate) async fn import(
    socket: &Path,
    variant: AutoEqVariant,
    name: Option<String>,
    force: bool,
    target: AutoEqTarget,
    interrupt: Option<Arc<Presses>>,
) -> Result<AutoEqSaved, Refused> {
    let op = AutoEqOp::Import {
        variant: Box::new(variant),
        name,
        force,
        target,
    };
    let payload = job(socket, op, interrupt).await?;
    serde_json::from_value(payload)
        .map_err(|e| Refused::words(format!("the daemon's answer is not a saved preset: {e}")))
}

/// Send `op` on a connection of its own.
async fn request(socket: &Path, op: AutoEqOp, within: Duration) -> Result<Response, IpcError> {
    request_on_own_connection(socket, Request::AutoEq { op }, Some(within)).await
}

/// Stops the job when dropped while armed: its caller went before it ended.
struct CancelOnDrop {
    socket: PathBuf,
    job: JobId,
    armed: bool,
}

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let (socket, job) = (self.socket.clone(), self.job);
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                let _ = request(&socket, AutoEqOp::Cancel { job }, START_WAIT).await;
            });
        }
    }
}

/// The start of a job, in a task of its own: a caller dropped before the daemon's answer is read
/// still has the job it started stopped, as soon as its id is known and once.
struct Starting {
    socket: PathBuf,
    start: Option<tokio::task::JoinHandle<Result<Response, IpcError>>>,
}

impl Starting {
    /// Send `op` to the daemon at `socket`.
    fn send(socket: &Path, op: AutoEqOp) -> Starting {
        let owned = socket.to_path_buf();
        Starting {
            socket: owned.clone(),
            start: Some(tokio::spawn(async move {
                request(&owned, op, START_WAIT).await
            })),
        }
    }

    /// The daemon's answer. The task is left to this guard until it is read, so a drop while it
    /// is awaited finds it still there.
    async fn answer(&mut self) -> Result<Response, Refused> {
        let Some(start) = self.start.as_mut() else {
            return Err(Refused::words("the start was read already"));
        };
        let answer = start.await;
        self.start = None;
        match answer {
            Ok(reply) => reply.map_err(|e| Refused::of(Err(e))),
            Err(e) => Err(Refused::words(e.to_string())),
        }
    }
}

impl Drop for Starting {
    fn drop(&mut self) {
        let Some(start) = self.start.take() else {
            return;
        };
        let socket = self.socket.clone();
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            runtime.spawn(async move {
                if let Ok(Ok(Response::Job { job })) = start.await {
                    let _ = request(&socket, AutoEqOp::Cancel { job }, START_WAIT).await;
                }
            });
        }
    }
}

/// Stop `job` before the caller returns: a CLI's runtime ends at once, and a cancel left to the
/// guard's spawned task would not go.
async fn cancel_here(guard: &mut CancelOnDrop, socket: &Path, job: JobId) {
    guard.armed = false;
    let _ = request(socket, AutoEqOp::Cancel { job }, CANCEL_WAIT).await;
}

/// Start the job `op` is, wait for its end and give its result.
async fn job(
    socket: &Path,
    op: AutoEqOp,
    interrupt: Option<Arc<Presses>>,
) -> Result<serde_json::Value, Refused> {
    let mut starting = Starting::send(socket, op);
    let job = match starting.answer().await {
        Ok(Response::Job { job }) => job,
        Ok(other) => return Err(Refused::of(Ok(other))),
        Err(refused) => return Err(refused),
    };
    let mut guard = CancelOnDrop {
        socket: socket.to_path_buf(),
        job,
        armed: true,
    };
    let left = async {
        match &interrupt {
            Some(presses) => presses.next().await,
            None => std::future::pending().await,
        }
    };
    tokio::pin!(left);
    let opened = tokio::time::timeout(FOLLOW_WAIT, async {
        Client::connect(socket)
            .await?
            .stream(Request::AutoEq {
                op: AutoEqOp::Follow,
            })
            .await
    })
    .await;
    let mut events = match opened {
        Ok(Ok(events)) => events,
        Ok(Err(e)) => {
            cancel_here(&mut guard, socket, job).await;
            return Err(Refused::of(Err(e)));
        }
        Err(_) => {
            cancel_here(&mut guard, socket, job).await;
            return Err(Refused::words("the daemon did not open the AutoEq stream"));
        }
    };
    let waiting = tokio::time::timeout(JOB_WAIT, async {
        while let Some(event) = events.next().await {
            if let Event::AutoEq(event) = event
                && event.job == job
                && event.state.is_final()
            {
                return Some(event.state);
            }
        }
        None
    });
    tokio::pin!(waiting);
    // The job's own end wins when both are ready: a job that finished just before the press has
    // saved its preset.
    let ended = tokio::select! {
        biased;
        ended = &mut waiting => ended,
        () = &mut left => {
            // Stopped here and now, before the process goes, so the daemon saves nothing. A cancel
            // that finds the job ended is refused: the end is then read, and says what happened.
            guard.armed = false;
            let _ = request(socket, AutoEqOp::Cancel { job }, CANCEL_WAIT).await;
            match tokio::time::timeout(END_WAIT, &mut waiting).await {
                Ok(Ok(Some(JobState::Done { payload }))) => return Ok(payload),
                _ => {
                    return Err(Refused {
                        interrupted: true,
                        ..Refused::words("interrupted; the import is cancelled")
                    });
                }
            }
        }
    };
    match ended {
        Ok(Some(JobState::Done { payload })) => {
            guard.armed = false;
            Ok(payload)
        }
        Ok(Some(JobState::Failed { message, reason })) => {
            guard.armed = false;
            Err(Refused {
                message,
                reason: Some(reason),
                too_old: false,
                interrupted: false,
            })
        }
        Ok(Some(_)) => {
            guard.armed = false;
            Err(Refused::words(
                "the job was cancelled: another client stopped it, or the daemon is stopping",
            ))
        }
        Ok(None) => {
            guard.armed = false;
            Err(Refused::words("the daemon's AutoEq stream ended"))
        }
        Err(_) => {
            // Cancelled before this returns: a CLI's runtime ends at once, and a cancel left to a
            // spawned task would not go.
            guard.armed = false;
            let _ = request(socket, AutoEqOp::Cancel { job }, CANCEL_WAIT).await;
            Err(Refused::words(
                "the AutoEq job took too long and was cancelled",
            ))
        }
    }
}

#[cfg(test)]
mod tests;
