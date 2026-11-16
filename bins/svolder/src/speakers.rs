// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a front end asks of the daemon's virtual-speakers sets service: list the catalogue, fetch
//! a set, import a file as one, forget one.
//!
//! The daemon is the one process that reaches the sets' sources (this binary links no HTTP
//! client). Each call opens a connection of its own, so nothing else the front end sends waits
//! behind a slow download, and ends in the daemon's answer or its refusal in words. `catalogue`
//! and `forget` are plain requests. `fetch` and `import` are jobs: the request answers with the
//! job's id and the end comes on the family's follow stream, which is opened after the start (the
//! stream tells each job's last state first, so a job that has already ended is read all the
//! same). A call interrupted while its job runs cancels the job.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use svolder_ipc::socket::daemon_version::is_unknown_request;
use svolder_ipc::{
    Client, Event, IpcError, JobId, JobState, Reason, Request, Response, SpeakersCatalogue,
    SpeakersOp, SpeakersProgress, SpeakersRoom, SpeakersSaved, request_on_own_connection,
};

use crate::commands::interrupts::Presses;

/// The longest a plain request, or the start of a job, waits.
const START_WAIT: Duration = Duration::from_secs(15);
/// The longest the follow stream takes to open.
const FOLLOW_WAIT: Duration = Duration::from_secs(10);
/// The longest a job is waited for: the download's own limit is 120 s, then the conversion.
const JOB_WAIT: Duration = Duration::from_secs(180);
/// The longest a cancel is waited for.
const CANCEL_WAIT: Duration = Duration::from_secs(3);
/// The longest the job's end is read after a cancel.
const END_WAIT: Duration = Duration::from_secs(12);

/// What is said where there is no daemon to ask: the one process that reaches the sets' sources.
pub(crate) const NEEDS_DAEMON: &str = "the sets are fetched and kept by svolderd";

/// A call the daemon did not carry out, in its own words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Refused {
    /// What went wrong.
    pub message: String,
    /// Why, where the daemon says.
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
            }) => match is_unknown_request(None, code, &message, "speakers") {
                true => Refused {
                    too_old: true,
                    ..Refused::words(crate::reach::too_old_words(
                        None,
                        "list or fetch virtual speakers' sets",
                    ))
                },
                false => Refused {
                    message,
                    reason,
                    too_old: false,
                    interrupted: false,
                },
            },
            Ok(other) => Refused::words(format!("the daemon answered {other:?}")),
            Err(e) => Refused::words(e.to_string()),
        }
    }
}

/// Send `op` on a connection of its own.
async fn request(socket: &Path, op: SpeakersOp, within: Duration) -> Result<Response, IpcError> {
    request_on_own_connection(socket, Request::Speakers { op }, Some(within)).await
}

/// The catalogue's sets, and the ones the person imported, as the daemon at `socket` lists them.
pub(crate) async fn catalogue(socket: &Path) -> Result<SpeakersCatalogue, Refused> {
    match request(socket, SpeakersOp::Catalogue, START_WAIT).await {
        Ok(Response::SpeakersCatalogue(found)) => Ok(*found),
        other => Err(Refused::of(other)),
    }
}

/// Remove the fetched or imported set `id`.
pub(crate) async fn forget(socket: &Path, id: &str) -> Result<(), Refused> {
    match request(
        socket,
        SpeakersOp::Forget { id: id.to_string() },
        START_WAIT,
    )
    .await
    {
        Ok(Response::Ack) => Ok(()),
        other => Err(Refused::of(other)),
    }
}

/// Download the catalogue set `id`, check, convert and keep it. `progress` hears each report of
/// the download. `interrupt` is the presses of a signal the caller counts (the CLI's Ctrl-C):
/// one cancels the job on the daemon and the call ends as refused, so nothing is kept after the
/// person left.
pub(crate) async fn fetch(
    socket: &Path,
    id: &str,
    interrupt: Option<Arc<Presses>>,
    progress: &mut (dyn FnMut(&SpeakersProgress) + Send),
) -> Result<SpeakersSaved, Refused> {
    saved(
        job(
            socket,
            SpeakersOp::Fetch { id: id.to_string() },
            interrupt,
            progress,
        )
        .await?,
    )
}

/// Convert the file at `path` (on the daemon's machine) and keep it as the person's own set.
pub(crate) async fn import(
    socket: &Path,
    path: &Path,
    label: Option<String>,
    room: Option<SpeakersRoom>,
    interrupt: Option<Arc<Presses>>,
) -> Result<SpeakersSaved, Refused> {
    let op = SpeakersOp::Import {
        path: path.display().to_string(),
        label,
        room,
    };
    saved(job(socket, op, interrupt, &mut |_| {}).await?)
}

fn saved(payload: serde_json::Value) -> Result<SpeakersSaved, Refused> {
    serde_json::from_value(payload)
        .map_err(|e| Refused::words(format!("the daemon's answer is not a saved set: {e}")))
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
                let _ = request(&socket, SpeakersOp::Cancel { job }, START_WAIT).await;
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
    fn send(socket: &Path, op: SpeakersOp) -> Starting {
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
                    let _ = request(&socket, SpeakersOp::Cancel { job }, START_WAIT).await;
                }
            });
        }
    }
}

/// Stop `job` before the caller returns: a CLI's runtime ends at once, and a cancel left to the
/// guard's spawned task would not go.
async fn cancel_here(guard: &mut CancelOnDrop) {
    guard.armed = false;
    let _ = request(
        &guard.socket,
        SpeakersOp::Cancel { job: guard.job },
        CANCEL_WAIT,
    )
    .await;
}

/// Start the job `op` is, wait for its end and give its result.
async fn job(
    socket: &Path,
    op: SpeakersOp,
    interrupt: Option<Arc<Presses>>,
    progress: &mut (dyn FnMut(&SpeakersProgress) + Send),
) -> Result<serde_json::Value, Refused> {
    let mut starting = Starting::send(socket, op);
    let job: JobId = match starting.answer().await? {
        Response::Job { job } => job,
        other => return Err(Refused::of(Ok(other))),
    };
    let mut guard = CancelOnDrop {
        socket: socket.to_path_buf(),
        job,
        armed: true,
    };
    let opened = tokio::time::timeout(FOLLOW_WAIT, async {
        Client::connect(socket)
            .await?
            .stream(Request::Speakers {
                op: SpeakersOp::Follow,
            })
            .await
    })
    .await;
    let mut events = match opened {
        Ok(Ok(events)) => events,
        Ok(Err(e)) => {
            cancel_here(&mut guard).await;
            return Err(Refused::of(Err(e)));
        }
        Err(_) => {
            cancel_here(&mut guard).await;
            return Err(Refused::words(
                "the daemon did not open the sets' job stream",
            ));
        }
    };
    let left = async {
        match &interrupt {
            Some(presses) => presses.next().await,
            None => std::future::pending().await,
        }
    };
    tokio::pin!(left);
    // Reads the stream up to the job's end, telling `progress` what a report says.
    let mut waiting = Box::pin(tokio::time::timeout(JOB_WAIT, async {
        while let Some(event) = events.next().await {
            let Event::Speakers(event) = event else {
                continue;
            };
            if event.job != job {
                continue;
            }
            match event.state {
                JobState::Progress { payload } => {
                    if let Ok(report) = serde_json::from_value::<SpeakersProgress>(payload) {
                        progress(&report);
                    }
                }
                state if state.is_final() => return Some(state),
                _ => {}
            }
        }
        None
    }));
    // The job's own end wins when both are ready: a job that finished just before the press has
    // kept its set.
    let ended = tokio::select! {
        biased;
        ended = &mut waiting => ended,
        () = &mut left => {
            cancel_here(&mut guard).await;
            return match tokio::time::timeout(END_WAIT, &mut waiting).await {
                Ok(Ok(Some(JobState::Done { payload }))) => Ok(payload),
                _ => Err(Refused {
                    interrupted: true,
                    ..Refused::words("interrupted; the job is cancelled")
                }),
            };
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
            Err(Refused::words("the daemon's sets stream ended"))
        }
        Err(_) => {
            cancel_here(&mut guard).await;
            Err(Refused::words("the job took too long and was cancelled"))
        }
    }
}

#[cfg(test)]
mod tests;
