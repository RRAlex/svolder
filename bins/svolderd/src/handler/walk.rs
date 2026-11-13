// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device walk in the daemon: one service that owns the walk's jobs.
//!
//! Placement rule: everything of this family lives here and in the module of the work it runs.
//! [`Daemon`] holds the service in one field and routes `Request::Walk` to it in
//! one line of `handle_from` and one of `stream`. The web and MCP reach the family through
//! `Fleet::request` and `Fleet::stream` only: `Fleet` has no method per feature, and a stub
//! answers each in one arm.
//!
//! The service runs one walk for the whole daemon (a second start is refused as busy and names
//! whose walk is running; that walk's id and origin are in the follow stream's states). A walk
//! is a [`DeviceWalk`] run as a job, over [`effects::DaemonEffects`]: the shared scanner for its
//! scan, a probe actor for its passphrase test, and the `device_config` add for its write.
//!
//! A job has no inbox of its own, so the service keeps one channel per walk: `answer` and `back`
//! go down it while the walk waits for one, and are refused `not_waiting` while it works on the
//! last one or once it has ended. Each state the walk publishes is the whole of where it stands
//! ([`WalkState`]: its origin, its prompt, the devices heard so far, why the last answer did not
//! move it on), so a follower that comes late reads everything from the last one. A walk nobody
//! answers for [`WALK_IDLE_BOUND`] ends as cancelled. It ends done with the device it wrote, as
//! `device_config` answers it, or with nothing when a quiet scan was given up on.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::StreamExt;
use futures::stream::BoxStream;
use svolder_ipc::{
    CODE_USAGE, ConfiguredDevice, Event, JobId, PassphraseKind, Reason, Response, WALK_IDLE_BOUND,
    WalkOp,
};
use svolder_walk::{
    Configured, DeviceWalk, Given, Phase, RefusalKind, Step, WalkError, WalkRefusal, WalkState,
    Walked, Working,
};
use tokio::sync::mpsc;

use super::Daemon;
use super::jobs::{JobCtx, JobError, Jobs, NoSuchJob, Slots};
use crate::locks::locked;
use crate::log;
use crate::writer::Origin;

mod effects;

use effects::{Board, DaemonEffects, DaemonFailure};

/// What a walk is handed through its inbox.
enum Input {
    /// An answer to the prompt it shows.
    Answer { choice: usize, text: Option<String> },
    /// Go back a step.
    Back,
}

/// One walk's inbox: the channel it reads, and whether it waits for an answer now.
struct Inbox {
    tx: mpsc::Sender<Input>,
    waiting: Arc<AtomicBool>,
}

/// The inboxes by job id, shared with the walks, which take theirs out as they end.
type Inboxes = Arc<Mutex<BTreeMap<JobId, Inbox>>>;

/// The walk: its job slot and the inbox of each walk that runs.
pub(super) struct WalkService {
    jobs: Jobs,
    inboxes: Inboxes,
}

impl WalkService {
    /// A service with no job, running one at a time.
    pub(super) fn new() -> WalkService {
        WalkService {
            jobs: Jobs::new(Slots::One),
            inboxes: Arc::default(),
        }
    }

    /// Answer one op that has an answer. `origin` is who asked, which a start names its walk
    /// by; `daemon` is the daemon as a shared handle, which a walk's work holds while it runs
    /// (none for a daemon that was not made shared, which runs no walk). The follow op is a
    /// stream, which this refuses as `handle_from` refuses every stream.
    pub(super) fn handle(
        &self,
        op: WalkOp,
        origin: Origin,
        daemon: Option<Arc<Daemon>>,
    ) -> Response {
        match op {
            WalkOp::Start => self.start(origin, daemon),
            WalkOp::Answer { job, choice, text } => self.hand(job, Input::Answer { choice, text }),
            WalkOp::Back { job } => self.hand(job, Input::Back),
            WalkOp::Cancel { job } => self.jobs.cancel_answer(job),
            WalkOp::Follow => Jobs::follow_needs_a_stream("walk"),
        }
    }

    /// The stream `op` turns its connection into, or the answer that refuses it.
    pub(super) fn stream(&self, op: WalkOp) -> Result<BoxStream<'static, Event>, Box<Response>> {
        match op {
            WalkOp::Follow => Ok(self.jobs.follow().map(Event::Walk).boxed()),
            WalkOp::Cancel { .. } => Err(Box::new(Jobs::cancel_is_no_stream("walk"))),
            WalkOp::Start | WalkOp::Answer { .. } | WalkOp::Back { .. } => Err(Box::new(
                Response::error(CODE_USAGE, "only walk follow is a stream"),
            )),
        }
    }

    /// Cancel the walk's jobs and wait for them no longer than `bound`; how many were given up
    /// on.
    pub(super) async fn shutdown(&self, bound: Duration) -> usize {
        self.jobs.stop_all(bound).await
    }

    /// The walk's jobs, for the tests that start one.
    #[cfg(test)]
    pub(super) fn jobs(&self) -> &Jobs {
        &self.jobs
    }

    /// Start a walk for `origin`, with its inbox in place before anyone can know its id.
    fn start(&self, origin: Origin, daemon: Option<Arc<Daemon>>) -> Response {
        let Some(daemon) = daemon else {
            return Response::error(CODE_USAGE, "this daemon runs no walk")
                .with_reason(Reason::Internal);
        };
        let (tx, rx) = mpsc::channel(1);
        let waiting = Arc::new(AtomicBool::new(false));
        // Held across the start, so a walk that ends at once takes its inbox out after it is in.
        let mut inboxes = locked(&self.inboxes);
        let theirs = Arc::clone(&self.inboxes);
        let answer = {
            let waiting = Arc::clone(&waiting);
            self.jobs
                .start_answer("walk", origin.who(), move |ctx| async move {
                    let _inbox = InboxOut {
                        inboxes: theirs,
                        job: ctx.id(),
                    };
                    walk(daemon, ctx, origin, rx, waiting).await
                })
        };
        if let Response::Job { job } = answer {
            inboxes.insert(job, Inbox { tx, waiting });
        }
        answer
    }

    /// Hand `input` to walk `job`, when it waits for one.
    fn hand(&self, job: JobId, input: Input) -> Response {
        let inboxes = locked(&self.inboxes);
        match inboxes.get(&job) {
            Some(inbox) if inbox.waiting.swap(false, Ordering::SeqCst) => {
                match inbox.tx.try_send(input) {
                    Ok(()) => Response::Ack,
                    Err(_) => not_waiting(job),
                }
            }
            Some(_) => not_waiting(job),
            None if self.jobs.knows(job) => not_waiting(job),
            None => NoSuchJob.answer(job),
        }
    }
}

/// The refusal of an answer to a walk that does not wait for one.
fn not_waiting(job: JobId) -> Response {
    Response::error(
        CODE_USAGE,
        format!("walk {job} is not waiting for an answer: its next state says when it is"),
    )
    .with_reason(Reason::NotWaiting)
}

/// Takes a walk's inbox out as the walk ends, however it ends.
struct InboxOut {
    inboxes: Inboxes,
    job: JobId,
}

impl Drop for InboxOut {
    fn drop(&mut self) {
        locked(&self.inboxes).remove(&self.job);
    }
}

/// What the walk works on while it answers the prompt it showed.
fn working_on(step: Step) -> Working {
    match step {
        Step::Scan => Working::Scanning,
        Step::Name | Step::Passphrase => Working::Testing,
        _ => Working::Answering,
    }
}

/// One walk, from its first prompt to its end: show the prompt, wait for an answer (no longer
/// than [`WALK_IDLE_BOUND`]), work on it, and again, until the device is written, the walk is
/// given up on or cancelled, or an effect stops it.
async fn walk(
    daemon: Arc<Daemon>,
    ctx: JobCtx,
    origin: Origin,
    mut inbox: mpsc::Receiver<Input>,
    waiting: Arc<AtomicBool>,
) -> Result<serde_json::Value, JobError> {
    let ctx = Arc::new(ctx);
    let board = Arc::new(Board::new(Arc::clone(&ctx)));
    let effects = DaemonEffects::new(daemon, Arc::clone(&ctx), Arc::clone(&board));
    let mut walk = DeviceWalk::new(effects, Given::default(), Configured::Mark);
    let mut refusal: Option<WalkRefusal> = None;
    loop {
        // Open to an answer before the state that asks for it is out, so a front end that answers
        // the moment it reads the state is never refused; the inbox holds that one answer.
        waiting.store(true, Ordering::SeqCst);
        board.show(state_of(
            &walk,
            origin,
            Phase::Waiting,
            None,
            refusal.take(),
        ));
        let input = tokio::select! {
            input = inbox.recv() => input,
            () = ctx.cancelled() => None,
            () = tokio::time::sleep(WALK_IDLE_BOUND) => {
                log::info(&format!(
                    "walk {}: nobody answered for {} s; it ends",
                    ctx.id(),
                    WALK_IDLE_BOUND.as_secs()
                ));
                None
            }
        };
        waiting.store(false, Ordering::SeqCst);
        let Some(input) = input else {
            return Err(JobError::Cancelled);
        };
        if ctx.stopped() {
            return Err(JobError::Cancelled);
        }
        let (choice, text) = match input {
            Input::Back => {
                if !walk.back() {
                    refusal = Some(WalkRefusal {
                        message: "there is no step to go back to".into(),
                        kind: RefusalKind::Answer,
                    });
                }
                continue;
            }
            Input::Answer { choice, text } => (choice, text),
        };
        let working = working_on(walk.step());
        board.show(state_of(&walk, origin, Phase::Working, Some(working), None));
        let answered = walk.answer(choice, text).await;
        if ctx.stopped() {
            return Err(JobError::Cancelled);
        }
        match answered {
            Ok(Walked::On) => {}
            Ok(Walked::Added) => {
                let device = walk
                    .effects()
                    .written()
                    .or_else(|| walk.added().map(as_written));
                return Ok(device
                    .and_then(|device| serde_json::to_value(device).ok())
                    .unwrap_or_default());
            }
            Ok(Walked::Skipped) => return Ok(serde_json::Value::Null),
            Ok(Walked::Cancelled) | Err(WalkError::Effect(DaemonFailure::Cancelled)) => {
                return Err(JobError::Cancelled);
            }
            Err(WalkError::Effect(DaemonFailure::Failed { message, reason })) if walk.stopped() => {
                return Err(JobError::failed(message, reason));
            }
            Err(error) => refusal = Some(WalkRefusal::of(&error, DaemonFailure::words)),
        }
    }
}

/// The device a walk wrote as `device_config` answers one, for a write whose answer did not list
/// it: what the walk wrote, the passphrase as its kind only, not the default, not connected yet.
fn as_written(device: &svolder_ipc::DeviceConfig) -> ConfiguredDevice {
    ConfiguredDevice {
        unit: device.udid.clone(),
        name: device.name.clone(),
        profile: device.profile.clone(),
        passphrase: PassphraseKind::of(&device.passphrase),
        default: false,
        connected: false,
        not_connected_since_ms: None,
    }
}

/// The state a walk publishes, from where it stands.
fn state_of(
    walk: &DeviceWalk<DaemonEffects>,
    origin: Origin,
    phase: Phase,
    working: Option<Working>,
    refusal: Option<WalkRefusal>,
) -> WalkState {
    WalkState {
        origin: origin.word().to_string(),
        phase,
        working,
        prompt: walk.prompt(),
        rows: walk.rows().to_vec(),
        can_go_back: walk.can_go_back(),
        refusal,
        connect: walk.connect(),
    }
}

#[cfg(test)]
mod tests;
