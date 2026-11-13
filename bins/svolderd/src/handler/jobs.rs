// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Daemon-owned jobs: the one mechanism the families of long work (the device walk, AutoEq)
//! stand on.
//!
//! A request that starts long work asks a family's [`Jobs`] to [`Jobs::start`] it, and gets the
//! job's id at once ([`Jobs::start_answer`] builds the answer); the work runs as a task the
//! registry owns. Each change of the job goes out
//! as a [`JobEvent`] to whoever [`Jobs::follow`]s, and a job is stopped by its id
//! ([`Jobs::cancel`]) or, all of them, by the daemon's own stop ([`Jobs::stop_all`]).
//!
//! An event is a job's state, not a delta: a state may repeat (a subscriber that fell behind is
//! given where the jobs stand again), so a reader keeps the last state of each id and never
//! counts events.
//!
//! The rules the mechanism keeps, whatever the family:
//! - ids never repeat within a daemon run;
//! - a job that finishes ends with a result the family defines (the value its work returns, which
//!   rides in [`JobState::Done`]); it is part of the job's last state, so a subscriber that comes
//!   late or reconnects reads it from there, and one larger than [`JOB_RESULT_MAX_BYTES`] ends the
//!   job as failed instead;
//! - a family's slot policy ([`Slots`]) bounds how many jobs run at once, and a start past it is
//!   refused ([`Refusal::Busy`]), naming whose job is in the way;
//! - the last state of the newest [`KEPT_FINISHED`] ended jobs, as many of them as fit in
//!   [`REPLAY_BYTES_MAX`] (the newest one is always kept), and where each running job stands, is
//!   what a subscriber that comes late reads first; a progress payload larger than
//!   [`PROGRESS_MAX_BYTES`] is not sent, so a running job's last state is small too;
//! - a subscriber that reads slowly loses events and is told where the jobs stand instead
//!   ([`EVENT_BACKLOG`] each): it never holds a job or another subscriber up;
//! - work that blocks (AutoEq's index load, download and fit) runs on a blocking thread that the
//!   job's cancel is raced against: the job ends at once, the thread at its own timeouts or the
//!   runtime's stop bound, and stopping waits only as long as the bound it is given.
//!   [`JobCtx::blocking`] would add a stop flag for such work to check between its steps; no job
//!   family uses it.

use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt};
use svolder_ipc::{
    CODE_NOT_FOUND, CODE_USAGE, JOB_RESULT_MAX_BYTES, JobEvent, JobId, JobState, Reason, Response,
};
use tokio::sync::{Notify, broadcast};
use tokio_util::sync::CancellationToken;

use crate::locks::locked;

/// How many ended jobs a family remembers for a subscriber that comes late: the newest of them.
pub(super) const KEPT_FINISHED: usize = 16;

/// The most bytes of results the ended jobs a family remembers hold together: what a follower
/// that opens or lags is replayed beyond the running jobs. Two results of the largest size
/// ([`JOB_RESULT_MAX_BYTES`]), so a follower is never sent more than about 1 MiB it did not ask
/// for, instead of the 8 MiB that sixteen full results would be. The oldest ended job leaves
/// first; the newest stays whatever its size.
pub(super) const REPLAY_BYTES_MAX: usize = 2 * JOB_RESULT_MAX_BYTES;

/// The most bytes of one progress payload a job may say. A progress says where the work stands
/// (a count, a name); the result is what carries data. A larger one is dropped, not cut, and
/// logged.
pub(super) const PROGRESS_MAX_BYTES: usize = 64 * 1024;

/// How many events a subscriber may be behind by before it is told where the jobs stand rather
/// than every step it missed.
pub(super) const EVENT_BACKLOG: usize = 64;

/// How many jobs of a family run at once.
#[derive(Debug, Clone, Copy)]
pub(super) enum Slots {
    /// One in the whole daemon: a second start is refused while it runs.
    One,
    /// Up to this many; the next is refused.
    Up(usize),
}

impl Slots {
    fn cap(self) -> usize {
        match self {
            Slots::One => 1,
            Slots::Up(cap) => cap.max(1),
        }
    }
}

/// Why a job was not started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Refusal {
    /// The family is at its cap. `origin` is who started the job that is in the way, and `job`
    /// is its id.
    Busy {
        /// who started it, as the family's start gave it
        origin: String,
        /// its id
        job: JobId,
        /// how many jobs the family runs at once
        slots: usize,
    },
    /// The daemon is stopping; nothing starts.
    Stopping,
}

impl Refusal {
    /// The answer a request that asked for the job gets: the daemon's words, and for a family
    /// that is full the reason `busy`.
    pub(super) fn answer(&self, family: &str) -> Response {
        match self {
            Refusal::Busy { origin, job, slots } => Response::error(
                CODE_USAGE,
                match slots {
                    0 | 1 => format!(
                        "{} {family} is already running (job {job}, started by {origin})",
                        article(family)
                    ),
                    many => format!(
                        "{} {family}s are running; try again when one ends",
                        count_words(*many)
                    ),
                },
            )
            .with_reason(Reason::Busy),
            Refusal::Stopping => {
                Response::error(CODE_USAGE, "the daemon is stopping").with_reason(Reason::Stopping)
            }
        }
    }
}

/// `count` in words up to twelve, in digits past that.
fn count_words(count: usize) -> String {
    const WORDS: [&str; 11] = [
        "two", "three", "four", "five", "six", "seven", "eight", "nine", "ten", "eleven", "twelve",
    ];
    match count.checked_sub(2).and_then(|at| WORDS.get(at)) {
        Some(word) => (*word).to_string(),
        None => count.to_string(),
    }
}

/// `a` or `an`, for the word that follows.
fn article(word: &str) -> &'static str {
    match word.chars().next() {
        Some('A' | 'E' | 'I' | 'O' | 'U' | 'a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}

/// There is no job by that id: the daemon never ran it or has forgotten it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct NoSuchJob;

impl NoSuchJob {
    /// The answer a request that named the job gets.
    pub(super) fn answer(self, job: JobId) -> Response {
        Response::error(CODE_NOT_FOUND, format!("no job {job}")).with_reason(Reason::NoSuchJob)
    }
}

/// How a job's work ended when it did not finish.
#[derive(Debug)]
pub(super) enum JobError {
    /// It stopped because it was asked to.
    Cancelled,
    /// It could not finish.
    Failed {
        /// what went wrong
        message: String,
        /// why, as a front end matches on it
        reason: Reason,
    },
}

impl JobError {
    /// A failure with its words and its reason.
    pub(super) fn failed(message: impl Into<String>, reason: Reason) -> JobError {
        JobError::Failed {
            message: message.into(),
            reason,
        }
    }
}

/// The ask to stop, in the two forms work reads: a flag to look at between steps, and a future
/// to wait on.
#[derive(Clone, Default)]
struct Stop {
    flag: Arc<AtomicBool>,
    token: CancellationToken,
}

impl Stop {
    fn raise(&self) {
        self.flag.store(true, Ordering::SeqCst);
        self.token.cancel();
    }
}

/// What a job's work holds: its id, the way to say how it goes, and the ask to stop.
pub(super) struct JobCtx {
    id: JobId,
    stop: Stop,
    shared: Arc<Shared>,
}

impl JobCtx {
    /// The job's id.
    pub(super) fn id(&self) -> JobId {
        self.id
    }

    /// Say something of the work, in the family's own words. A payload larger than
    /// [`PROGRESS_MAX_BYTES`] is dropped (and logged): progress is small, the result carries
    /// data.
    pub(super) fn progress(&self, payload: serde_json::Value) {
        let size = serde_json::to_string(&payload).map_or(usize::MAX, |text| text.len());
        if size > PROGRESS_MAX_BYTES {
            crate::log::warn(&format!(
                "job {}: a progress of {size} bytes is too large to send; dropped",
                self.id
            ));
            return;
        }
        self.shared.publish(JobEvent {
            job: self.id,
            state: JobState::Progress { payload },
        });
    }

    /// Whether the job was asked to stop. Work looks between its steps.
    pub(super) fn stopped(&self) -> bool {
        self.stop.flag.load(Ordering::SeqCst)
    }

    /// Resolves once the job is asked to stop.
    pub(super) async fn cancelled(&self) {
        self.stop.token.cancelled().await;
    }

    /// Run `work` on a blocking thread, handing it the stop flag to look at between its steps.
    /// Dropping the future leaves the thread running to its next look at the flag.
    #[allow(
        dead_code,
        reason = "no job family uses the stop flag (AutoEq's blocking threads end at their own timeouts); the tests use it"
    )]
    pub(super) async fn blocking<T: Send + 'static>(
        &self,
        work: impl FnOnce(&AtomicBool) -> T + Send + 'static,
    ) -> Result<T, JobError> {
        let flag = Arc::clone(&self.stop.flag);
        tokio::task::spawn_blocking(move || work(&flag))
            .await
            .map_err(|_| JobError::failed("the job's work stopped unexpectedly", Reason::Internal))
    }
}

/// The state of a job that finished with `result`: done, or failed when the result is too large
/// to send.
fn done(payload: serde_json::Value) -> JobState {
    match serde_json::to_string(&payload).map_or(usize::MAX, |text| text.len()) {
        size if size > JOB_RESULT_MAX_BYTES => JobState::Failed {
            message: format!("the job's result is larger than {JOB_RESULT_MAX_BYTES} bytes"),
            reason: Reason::Internal,
        },
        _ => JobState::Done { payload },
    }
}

/// The bytes an event's payload takes on the wire.
fn event_size(event: &JobEvent) -> usize {
    match &event.state {
        JobState::Done { payload } | JobState::Progress { payload } => {
            serde_json::to_string(payload).map_or(usize::MAX, |text| text.len())
        }
        _ => 0,
    }
}

/// One job that runs.
struct Running {
    origin: String,
    /// The last event of the job: where it stands for a subscriber that comes late.
    last: JobEvent,
    stop: Stop,
    task: tokio::task::AbortHandle,
}

#[derive(Default)]
struct Inner {
    next: JobId,
    running: BTreeMap<JobId, Running>,
    /// The last event of each ended job, oldest first, at most [`KEPT_FINISHED`] of them and
    /// [`REPLAY_BYTES_MAX`] bytes (the newest excepted), each with its size.
    finished: VecDeque<(JobEvent, usize)>,
    stopping: bool,
}

struct Shared {
    inner: Mutex<Inner>,
    events: broadcast::Sender<JobEvent>,
    /// Woken when a job ends.
    ended: Notify,
}

impl Shared {
    /// Send `event` to every subscriber and keep it as the job's last, as one step: a
    /// subscriber that joins sees either the state with it or the event after it, never
    /// neither.
    fn publish(&self, event: JobEvent) {
        let mut inner = locked(&self.inner);
        if let Some(job) = inner.running.get_mut(&event.job) {
            job.last = event.clone();
            let _ = self.events.send(event);
        }
    }

    /// The job's work ended with `state`: it leaves the running jobs and joins the remembered
    /// ones. Does nothing for a job that was already ended.
    fn finish(&self, id: JobId, state: JobState) {
        let mut inner = locked(&self.inner);
        if inner.running.remove(&id).is_none() {
            return;
        }
        let event = JobEvent { job: id, state };
        let size = event_size(&event);
        inner.finished.push_back((event.clone(), size));
        let mut total: usize = inner
            .finished
            .iter()
            .fold(0, |sum, (_, size)| usize::saturating_add(sum, *size));
        while inner.finished.len() > KEPT_FINISHED
            || (inner.finished.len() > 1 && total > REPLAY_BYTES_MAX)
        {
            if let Some((_, size)) = inner.finished.pop_front() {
                total = total.saturating_sub(size);
            }
        }
        let _ = self.events.send(event);
        drop(inner);
        self.ended.notify_waiters();
    }

    /// Where every job stands: the ended ones, oldest first, then the running ones by id.
    fn snapshot(inner: &Inner) -> VecDeque<JobEvent> {
        inner
            .finished
            .iter()
            .map(|(event, _)| event.clone())
            .chain(inner.running.values().map(|job| job.last.clone()))
            .collect()
    }
}

/// The jobs of one family.
pub(super) struct Jobs {
    slots: Slots,
    shared: Arc<Shared>,
}

impl Jobs {
    /// A registry with no jobs that runs as many at once as `slots` says.
    pub(super) fn new(slots: Slots) -> Jobs {
        Jobs::with_backlog(slots, EVENT_BACKLOG)
    }

    /// [`Jobs::new`] with a backlog of its own for each subscriber.
    pub(super) fn with_backlog(slots: Slots, backlog: usize) -> Jobs {
        let (events, _) = broadcast::channel(backlog.max(1));
        Jobs {
            slots,
            shared: Arc::new(Shared {
                inner: Mutex::new(Inner {
                    next: 1,
                    ..Inner::default()
                }),
                events,
                ended: Notify::new(),
            }),
        }
    }

    /// How many jobs run.
    #[cfg(test)]
    pub(super) fn running(&self) -> usize {
        locked(&self.shared.inner).running.len()
    }

    /// [`Jobs::start`] as the answer to the request that asked for the job: the job's id as
    /// [`Response::Job`], or the refusal's answer, which names the job `family`.
    pub(super) fn start_answer<W, F, T>(&self, family: &str, origin: &str, work: W) -> Response
    where
        W: FnOnce(JobCtx) -> F + Send + 'static,
        F: Future<Output = Result<T, JobError>> + Send + 'static,
        T: Into<serde_json::Value>,
    {
        match self.start(origin, work) {
            Ok(job) => Response::Job { job },
            Err(refusal) => refusal.answer(family),
        }
    }

    /// Start `work` as a job of `origin` (who asked, in the words a refusal names them by) and
    /// answer its id at once. The job runs as a task this registry owns; its first event,
    /// `started`, is out before this returns. A start past the slot policy, or after the
    /// daemon's stop, is refused and runs nothing. What the work returns is the job's result
    /// (`()` for none), carried by `done`.
    pub(super) fn start<W, F, T>(&self, origin: &str, work: W) -> Result<JobId, Refusal>
    where
        W: FnOnce(JobCtx) -> F + Send + 'static,
        F: Future<Output = Result<T, JobError>> + Send + 'static,
        T: Into<serde_json::Value>,
    {
        let mut inner = locked(&self.shared.inner);
        if inner.stopping {
            return Err(Refusal::Stopping);
        }
        if inner.running.len() >= self.slots.cap() {
            let (job, oldest) = inner
                .running
                .iter()
                .next()
                .map(|(id, job)| (*id, job.origin.clone()))
                .unwrap_or_default();
            return Err(Refusal::Busy {
                origin: oldest,
                job,
                slots: self.slots.cap(),
            });
        }
        let id = inner.next;
        inner.next += 1;
        let stop = Stop::default();
        let ctx = JobCtx {
            id,
            stop: stop.clone(),
            shared: Arc::clone(&self.shared),
        };
        let shared = Arc::clone(&self.shared);
        let task = tokio::spawn(async move {
            let ended = AssertUnwindSafe(async move { work(ctx).await })
                .catch_unwind()
                .await;
            let state = match ended {
                Ok(Ok(result)) => done(result.into()),
                Ok(Err(JobError::Cancelled)) => JobState::Cancelled,
                Ok(Err(JobError::Failed { message, reason })) => {
                    JobState::Failed { message, reason }
                }
                Err(_) => JobState::Failed {
                    message: "the job stopped unexpectedly".to_string(),
                    reason: Reason::Internal,
                },
            };
            shared.finish(id, state);
        });
        let started = JobEvent {
            job: id,
            state: JobState::Started,
        };
        inner.running.insert(
            id,
            Running {
                origin: origin.to_string(),
                last: started.clone(),
                stop,
                task: task.abort_handle(),
            },
        );
        let _ = self.shared.events.send(started);
        Ok(id)
    }

    /// Ask job `id` to stop; its work ends as cancelled when it next looks. The answer says
    /// only that nothing is running under that id now, not that it was cancelled: a job that
    /// has ended needs none, so that is not an error (one still remembered, the newest
    /// [`KEPT_FINISHED`], reads the same as one just stopped), and an id the registry does not
    /// hold is.
    pub(super) fn cancel(&self, id: JobId) -> Result<(), NoSuchJob> {
        let inner = locked(&self.shared.inner);
        if let Some(job) = inner.running.get(&id) {
            job.stop.raise();
            return Ok(());
        }
        match inner.finished.iter().any(|(event, _)| event.job == id) {
            true => Ok(()),
            false => Err(NoSuchJob),
        }
    }

    /// Whether the registry holds job `id`: running, or ended and still remembered.
    pub(super) fn knows(&self, id: JobId) -> bool {
        let inner = locked(&self.shared.inner);
        inner.running.contains_key(&id) || inner.finished.iter().any(|event| event.0.job == id)
    }

    /// The events of this family's jobs: first where each job stands (the ended ones the
    /// registry still remembers, oldest first, then the running ones), then each change as it
    /// happens, for as long as the stream is held.
    ///
    /// A reader that falls more than [`EVENT_BACKLOG`] events behind skips what it missed and
    /// is given where each job stands again; it never holds a job or another reader up.
    pub(super) fn follow(&self) -> BoxStream<'static, JobEvent> {
        let (snapshot, receiver) = {
            let inner = locked(&self.shared.inner);
            (Shared::snapshot(&inner), self.shared.events.subscribe())
        };
        let shared = Arc::clone(&self.shared);
        futures::stream::unfold(
            (snapshot, receiver, shared),
            |(mut queue, mut receiver, shared)| async move {
                loop {
                    if let Some(event) = queue.pop_front() {
                        return Some((event, (queue, receiver, shared)));
                    }
                    match receiver.recv().await {
                        Ok(event) => return Some((event, (queue, receiver, shared))),
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            // Taken together, so no event falls between the state and the
                            // events that follow it.
                            let inner = locked(&shared.inner);
                            queue = Shared::snapshot(&inner);
                            receiver = shared.events.subscribe();
                        }
                        Err(broadcast::error::RecvError::Closed) => return None,
                    }
                }
            },
        )
        .boxed()
    }

    /// The answer to a request to cancel job `id`: an acknowledgement, or why not.
    pub(super) fn cancel_answer(&self, id: JobId) -> Response {
        match self.cancel(id) {
            Ok(()) => Response::Ack,
            Err(none) => none.answer(id),
        }
    }

    /// The refusal of a stream op (`follow`) that reaches `handle_from`: it has no response.
    pub(super) fn follow_needs_a_stream(cmd: &str) -> Response {
        Response::error(
            CODE_USAGE,
            format!("{cmd} follow is not a request/response verb"),
        )
    }

    /// The refusal of an op that is not a stream (`cancel`) asked of `stream`.
    pub(super) fn cancel_is_no_stream(cmd: &str) -> Response {
        Response::error(CODE_USAGE, format!("{cmd} cancel is not a stream"))
    }

    /// Stop every job, as the daemon's own stop does: ask each to stop, refuse any later start,
    /// and wait for the jobs to end for no longer than `bound`. A job still running then is
    /// given up on (its task is aborted and it is reported cancelled); a thread of blocking
    /// work it left runs to its next look at its stop flag on its own. Answers how many jobs
    /// were given up on.
    pub(super) async fn stop_all(&self, bound: Duration) -> usize {
        {
            let mut inner = locked(&self.shared.inner);
            inner.stopping = true;
            for job in inner.running.values() {
                job.stop.raise();
            }
        }
        let wait = async {
            loop {
                let ended = self.shared.ended.notified();
                tokio::pin!(ended);
                ended.as_mut().enable();
                if locked(&self.shared.inner).running.is_empty() {
                    return;
                }
                ended.await;
            }
        };
        if tokio::time::timeout(bound, wait).await.is_ok() {
            return 0;
        }
        let left: Vec<(JobId, tokio::task::AbortHandle)> = locked(&self.shared.inner)
            .running
            .iter()
            .map(|(id, job)| (*id, job.task.clone()))
            .collect();
        for (id, task) in &left {
            task.abort();
            self.shared.finish(*id, JobState::Cancelled);
        }
        left.len()
    }
}

#[cfg(test)]
mod tests;
