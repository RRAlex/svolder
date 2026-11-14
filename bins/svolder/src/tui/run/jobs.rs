// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The slow work the preset list asks of the loop: AutoEq's searches and imports, which the
//! daemon does, and the writes of the preset library.
//!
//! Each runs on a task of its own, so no frame and no key waits on the daemon, which may be
//! fetching AutoEq's index (10 to 60 s) or a file; what a task ends with arrives on a channel the
//! loop reads, as a [`Done`]. A search question waits out a pause in the typing first
//! ([`Jobs::due`]): the loop wakes at that moment and sends the last question asked, and a newer
//! question replaces the one waiting. A question that is sent stops the one still in flight,
//! whose answer would be dropped anyway.

use std::path::PathBuf;

use futures::future::BoxFuture;
use svolder_ipc::socket::daemon_version::RESTART_COMMAND;
use svolder_ipc::{AutoEqTarget, AutoEqVariant, IpcError, PresetOp, Response};
use tokio::sync::mpsc;
use tokio::task::AbortHandle;
use tokio::time::Instant;

use crate::autoeq::{self, NEEDS_DAEMON};
use crate::tui::boxes::presets::Done;
use crate::tui::boxes::presets::search::{Ask, DEBOUNCE};

/// A request to the daemon that has not been sent: its future does nothing until it is awaited.
pub(in crate::tui) type Call = BoxFuture<'static, Result<Response, IpcError>>;

/// A search question waiting out the typing.
pub(super) struct Due {
    /// When it is sent.
    at: Instant,
    /// The request that asks it, or nothing when the session holds no daemon.
    call: Option<Call>,
    /// The question.
    ask: Ask,
}

/// One write of the preset library, and where it goes: the daemon on `socket` when one runs, the
/// configuration file `file` itself when none does.
pub(super) struct PresetJob {
    /// The socket of the daemon that would write it.
    pub socket: PathBuf,
    /// The file this session reads and writes without a daemon.
    pub file: PathBuf,
    /// The write.
    pub op: PresetOp,
}

/// The jobs in flight and the channel their ends come back on.
pub(in crate::tui) struct Jobs {
    /// Where a job sends what it ended with.
    done: mpsc::UnboundedSender<Done>,
    /// What the jobs ended with, in the order they ended.
    pub(super) ended: mpsc::UnboundedReceiver<Done>,
    /// The search question that waits for the typing to pause.
    pub(super) due: Option<Box<Due>>,
    /// The search question in flight, which the next one stops.
    searching: Option<AbortHandle>,
    /// The task that carries out the preset writes one after the other, so two keys in a row
    /// reach the file in the order they were pressed.
    writes: Option<mpsc::UnboundedSender<PresetJob>>,
}

impl Default for Jobs {
    fn default() -> Jobs {
        let (done, ended) = mpsc::unbounded_channel();
        Jobs {
            done,
            ended,
            due: None,
            searching: None,
            writes: None,
        }
    }
}

/// What a job says when the loop holds no daemon: AutoEq is the daemon's.
fn no_daemon() -> String {
    format!("{NEEDS_DAEMON}: none is running; {RESTART_COMMAND} starts one")
}

impl Jobs {
    /// Ask the daemon the question `ask` by sending `call` (none: this session holds no daemon),
    /// after the typing pauses when `soon`.
    pub(super) fn search(&mut self, call: Option<Call>, ask: Ask, soon: bool) {
        match soon {
            true => {
                self.due = Some(Box::new(Due {
                    at: Instant::now() + DEBOUNCE,
                    call,
                    ask,
                }));
            }
            false => {
                self.due = None;
                self.send(call, ask);
            }
        }
    }

    /// When the waiting question is sent, if one waits.
    pub(super) fn due_at(&self) -> Option<Instant> {
        self.due.as_ref().map(|due| due.at)
    }

    /// Whether a search question is in flight.
    #[cfg(test)]
    pub(super) fn is_searching(&self) -> bool {
        self.searching
            .as_ref()
            .is_some_and(|task| !task.is_finished())
    }

    /// Stop the questions of a search box that is gone: the one waiting for the typing is never
    /// sent, and the one in flight is dropped with its connection.
    pub(super) fn stop_search(&mut self) {
        self.due = None;
        if let Some(earlier) = self.searching.take() {
            earlier.abort();
        }
    }

    /// Send the question that was waiting for the typing to pause.
    pub(super) fn send_due(&mut self) {
        if let Some(due) = self.due.take() {
            self.send(due.call, due.ask);
        }
    }

    /// Ask now, on a task of its own, in place of the question in flight.
    fn send(&mut self, call: Option<Call>, ask: Ask) {
        if let Some(earlier) = self.searching.take() {
            earlier.abort();
        }
        let done = self.done.clone();
        let task = tokio::spawn(async move {
            let result = match call {
                Some(call) => autoeq::search_answer(call.await).map_err(|refused| refused.line()),
                None => Err(no_daemon()),
            };
            let _ = done.send(Done::Searched(ask, result));
        });
        self.searching = Some(task.abort_handle());
    }

    /// Have the daemon at `socket` import `variant` as the preset `name` for `target`. The import
    /// is not stopped by the next question: it ends in a saved preset or a refusal.
    pub(super) fn import(
        &self,
        socket: Option<PathBuf>,
        variant: AutoEqVariant,
        name: String,
        target: AutoEqTarget,
    ) {
        let done = self.done.clone();
        let asked = name.clone();
        tokio::spawn(async move {
            let result = match socket {
                Some(socket) => autoeq::import(&socket, variant, Some(name), false, target, None)
                    .await
                    .map_err(|refused| refused.line()),
                None => Err(no_daemon()),
            };
            let _ = done.send(Done::Imported(asked, result));
        });
    }
}

impl Jobs {
    /// Queue a write of the preset library: the loop goes on at once, and what the write ended
    /// with comes back as [`Done::PresetWritten`], in the order the writes were queued.
    pub(super) fn write_preset(&mut self, job: PresetJob) {
        let job = match self.writes.as_ref() {
            Some(queue) => match queue.send(job) {
                Ok(()) => return,
                Err(returned) => returned.0,
            },
            None => job,
        };
        let (queue, mut waiting) = mpsc::unbounded_channel::<PresetJob>();
        let done = self.done.clone();
        tokio::spawn(async move {
            while let Some(job) = waiting.recv().await {
                let result = crate::reach::preset_write(&job.socket, &job.file, job.op)
                    .await
                    .map_err(|failure| match crate::reach::is_too_old(&failure) {
                        true => failure.to_string(),
                        false => failure.what,
                    });
                let _ = done.send(Done::PresetWritten(result));
            }
        });
        let _ = queue.send(job);
        self.writes = Some(queue);
    }
}

/// What the jobs ended with next; never, while none is in flight.
pub(super) async fn next(ended: &mut mpsc::UnboundedReceiver<Done>) -> Done {
    match ended.recv().await {
        Some(done) => done,
        None => std::future::pending().await,
    }
}

/// The moment the waiting question is sent; never, while none waits.
pub(super) async fn due(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests;
