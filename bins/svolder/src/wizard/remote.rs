// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device walk as a client of the daemon's: the same steps, run where the daemon runs them.
//!
//! While a daemon runs, the walk is its job (`svolder_ipc::WalkOp`): the daemon listens, tests
//! the passphrase and writes the device, and this side only draws the prompt it is shown and
//! sends the answer. [`RemoteWalk`] starts one, follows its states on a connection of its own
//! and answers, steps back and cancels on a connection per request, so a request that is given
//! up on never leaves a half-written line on a connection the next one needs.
//!
//! A client acts only on the walk it started, from the moment the start answer gives the job:
//! it reads the follow stream's states of that job alone, never adopts another walk, and leaves
//! by cancelling its own, never another's. A walk that is already running refuses the start
//! (`busy`) in words that say where it was started from, and this side shows them.
//!
//! A state is the whole of where the walk stands, so a reader keeps the last one of its job. An
//! answer is settled when a state arrives that shows it has been taken: the walk working, a new
//! prompt, or the end; the state the answer was given against, sent again, settles nothing.

use std::path::PathBuf;
use std::time::Duration;

use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt};
use svolder_ipc::{
    Client, ConfiguredDevice, DeviceConfig, Event, IpcError, JobEvent, JobId, JobState,
    PassphraseKind, Reason, Request, Response, WalkOp, request_on_own_connection,
};
use svolder_walk::{Phase, Prompt, RefusalKind, Row, Step, WalkRefusal, WalkState, Working};
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::recovery::{Failure, Kind};

/// How long one op's answer is waited for: the daemon acknowledges an op as soon as the walk has
/// it, so an answer this late is a daemon that is wedged.
const OP_BOUND: Duration = Duration::from_secs(10);

/// How long the follow stream's opening is waited for.
const FOLLOW_BOUND: Duration = Duration::from_secs(5);

/// How long a walk being left waits for the daemon to take its cancel.
const CANCEL_BOUND: Duration = Duration::from_secs(3);

/// How long a refused op waits for the stream to say the walk ended.
const REFUSAL_WAIT: Duration = Duration::from_secs(2);

/// What a walk whose stream is gone says.
const WENT_AWAY: &str = "the daemon went away while it walked the device";

/// How a walk ended.
#[derive(Debug, Clone, PartialEq)]
enum Ended {
    /// The device was written (`None`: a quiet scan was given up on, nothing written).
    Done(Option<ConfiguredDevice>),
    /// Cancelled: by a cancel, the linked step's Cancel, the idle bound or the daemon's stop.
    Cancelled,
    /// The daemon could not finish it, in its words.
    Failed(String),
}

/// What an answer left the walk at.
#[derive(Debug, PartialEq)]
pub(crate) enum Answered {
    /// On to another step, or the same one asked again.
    On,
    /// The step refused the answer, in the daemon's words, and is asked again.
    Refused(WalkRefusal),
    /// The device was written.
    Added(DeviceConfig),
    /// A quiet scan was given up on: nothing was written.
    Skipped,
    /// The walk was cancelled at its linked step, as asked.
    Cancelled,
}

/// A walk this client started on the daemon, and what it has read of it.
pub(crate) struct RemoteWalk {
    /// The daemon's socket.
    socket: PathBuf,
    /// The start, while its answer is not in: a task of its own, so a wait that is given up on
    /// does not lose the job the start gives.
    start: Option<JoinHandle<Result<JobId, Failure>>>,
    /// The job this client started: the only one it reads or acts on.
    job: Option<JobId>,
    /// The follow stream, on a connection of its own.
    follow: Option<BoxStream<'static, Event>>,
    /// The last state of the job.
    state: Option<WalkState>,
    /// How the job ended, once it has.
    ended: Option<Ended>,
    /// The state the last answer was given against, until a state shows it was taken.
    awaiting: Option<WalkState>,
    /// Whether a state of the walk working has come since that answer.
    moved: bool,
    /// The devices heard so far, one line each, while the walk scans; empty otherwise.
    heard: watch::Sender<Vec<String>>,
}

impl RemoteWalk {
    /// A walk to start on the daemon at `socket`; nothing is asked of it yet.
    pub(crate) fn new(socket: PathBuf) -> RemoteWalk {
        RemoteWalk {
            socket,
            start: None,
            job: None,
            follow: None,
            state: None,
            ended: None,
            awaiting: None,
            moved: false,
            heard: watch::channel(Vec::new()).0,
        }
    }

    /// The lines of the devices heard so far, as the scan hears them.
    pub(crate) fn heard(&self) -> watch::Receiver<Vec<String>> {
        self.heard.subscribe()
    }

    /// The daemon's words for a reply that is not what an op asked for.
    fn failure(&self, reply: Result<Response, IpcError>) -> Failure {
        match reply {
            Ok(Response::Error {
                code,
                message,
                reason,
            }) => Failure::from_daemon_on(&self.socket, code, message, reason),
            Ok(other) => Failure::of(
                Kind::DaemonError,
                format!("the daemon sent an unexpected reply: {other:?}"),
            ),
            Err(IpcError::Refused(answer)) => self.failure(Ok(*answer)),
            Err(e) => Failure::from_ipc(&e),
        }
    }

    /// The daemon's socket.
    pub(crate) fn socket(&self) -> &std::path::Path {
        &self.socket
    }

    /// Start the walk, unless it has been, and follow it: the first states are read by
    /// [`RemoteWalk::prompt`]. A walk already running on the daemon refuses, in words that name
    /// where it was started from.
    pub(crate) async fn begin(&mut self) -> Result<(), Failure> {
        if self.job.is_none() && self.start.is_none() {
            let socket = self.socket.clone();
            self.start = Some(tokio::spawn(async move {
                let reply = request_on_own_connection(
                    &socket,
                    Request::Walk { op: WalkOp::Start },
                    Some(OP_BOUND),
                )
                .await;
                match reply {
                    Ok(Response::Job { job }) => Ok(job),
                    other => Err(started_badly(&socket, other)),
                }
            }));
        }
        if let Some(start) = self.start.as_mut() {
            let started = start.await;
            self.start = None;
            match started {
                Ok(Ok(job)) => self.job = Some(job),
                Ok(Err(failure)) => return Err(failure),
                Err(e) => return Err(Failure::of(Kind::DaemonError, e.to_string())),
            }
        }
        if self.follow.is_none() {
            let opened = tokio::time::timeout(FOLLOW_BOUND, async {
                Client::connect(&self.socket)
                    .await?
                    .stream(Request::Walk { op: WalkOp::Follow })
                    .await
            })
            .await;
            match opened {
                Ok(Ok(stream)) => self.follow = Some(stream),
                Ok(Err(e)) => return Err(self.failure(Err(e))),
                Err(_) => {
                    return Err(Failure::of(
                        Kind::DaemonError,
                        "the daemon did not open the walk's stream in time",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Keep what an event says of this client's job, and nothing of any other job.
    fn take(&mut self, event: Event) {
        let Event::Walk(JobEvent { job, state }) = event else {
            return;
        };
        if Some(job) != self.job || self.ended.is_some() {
            return;
        }
        match state {
            JobState::Progress { payload } => {
                match serde_json::from_value::<WalkState>(payload) {
                    Ok(state) => {
                        if state.phase == Phase::Working {
                            self.moved = true;
                        }
                        let scanning = state.working == Some(Working::Scanning);
                        self.heard.send_replace(match scanning {
                            true => state.rows.iter().map(heard_line).collect(),
                            false => Vec::new(),
                        });
                        self.state = Some(state);
                    }
                    // A state this build cannot read is a daemon that walks further than it
                    // does: it is said, rather than waited on until the idle bound.
                    Err(_) => {
                        self.ended = Some(Ended::Failed(
                            "the daemon's walk says more than this svolder can read: the daemon \
                             is newer than svolder"
                                .into(),
                        ));
                    }
                }
            }
            JobState::Done { payload } => {
                let device = serde_json::from_value::<ConfiguredDevice>(payload).ok();
                self.ended = Some(Ended::Done(device));
            }
            JobState::Failed { message, .. } => self.ended = Some(Ended::Failed(message)),
            JobState::Cancelled => self.ended = Some(Ended::Cancelled),
            JobState::Started | JobState::Unknown => {}
        }
    }

    /// Take every event already waiting on the stream, and a stream that has gone as the end of
    /// the walk: the daemon ends a walk by itself (the idle bound, a restart, a stop), and an
    /// op sent after that is refused as not waiting, whose next state is what says why.
    fn drain(&mut self) {
        while let Some(follow) = self.follow.as_mut() {
            match follow.next().now_or_never() {
                Some(Some(event)) => self.take(event),
                Some(None) => {
                    self.follow = None;
                    if self.ended.is_none() {
                        self.ended = Some(Ended::Failed(WENT_AWAY.into()));
                    }
                }
                None => break,
            }
        }
    }

    /// After an op the daemon refused, read the stream for a short while for the walk's end: a
    /// refusal that is not one (the walk merely works) finds none, and the refusal stands.
    async fn read_end(&mut self) {
        self.drain();
        let waited = tokio::time::timeout(REFUSAL_WAIT, async {
            while self.ended.is_none() {
                let Some(follow) = self.follow.as_mut() else {
                    break;
                };
                match follow.next().await {
                    Some(event) => self.take(event),
                    None => {
                        self.follow = None;
                        self.ended = Some(Ended::Failed(WENT_AWAY.into()));
                    }
                }
            }
        })
        .await;
        let _ = waited;
    }

    /// Whether what has been read is a state to act on: the walk has ended, or waits at a state
    /// that is not the one the last answer was given against (unless it was seen working since).
    fn settled(&self) -> bool {
        if self.ended.is_some() {
            return true;
        }
        let Some(state) = &self.state else {
            return false;
        };
        if state.phase != Phase::Waiting {
            return false;
        }
        match &self.awaiting {
            None => true,
            Some(before) => self.moved || state != before,
        }
    }

    /// Read states until the walk waits for an answer or has ended. Safe to drop and call
    /// again: the stream keeps its place.
    async fn ready(&mut self) -> Result<(), Failure> {
        self.begin().await?;
        while !self.settled() {
            let Some(follow) = self.follow.as_mut() else {
                break;
            };
            match follow.next().await {
                Some(event) => self.take(event),
                None => {
                    self.follow = None;
                    let ended = Ended::Failed(WENT_AWAY.into());
                    let failure = ended_words(&ended);
                    self.ended = Some(ended);
                    return Err(failure);
                }
            }
        }
        self.awaiting = None;
        self.moved = false;
        Ok(())
    }

    /// What the walk asks now, once it waits for an answer. A walk that has ended asks nothing,
    /// and says how it ended.
    pub(crate) async fn prompt(&mut self) -> Result<Prompt, Failure> {
        self.ready().await?;
        if let Some(ended) = &self.ended {
            return Err(ended_words(ended));
        }
        self.state
            .as_ref()
            .map(|state| state.prompt.clone())
            .ok_or_else(|| Failure::of(Kind::DaemonError, "the daemon's walk showed no prompt"))
    }

    /// The prompt's step the walk waits at, when it does.
    pub(crate) fn step(&self) -> Option<Step> {
        self.state.as_ref().map(|state| state.prompt.step)
    }

    /// Whether going back would change the step.
    pub(crate) fn can_go_back(&self) -> bool {
        self.ended.is_none() && self.state.as_ref().is_some_and(|state| state.can_go_back)
    }

    /// Whether the walk is over, however it ended.
    pub(crate) fn is_over(&self) -> bool {
        self.ended.is_some()
    }

    /// The op sent for this walk's job, answered with the daemon's acknowledgement.
    async fn send(&self, op: WalkOp) -> Result<(), Failure> {
        let reply =
            request_on_own_connection(&self.socket, Request::Walk { op }, Some(OP_BOUND)).await;
        match reply {
            Ok(Response::Ack) => Ok(()),
            other => Err(self.failure(other)),
        }
    }

    /// Answer the prompt the walk waits at: a choice, or the line typed where it takes text.
    /// Returns once the answer has been taken and the walk has moved, refused it or ended.
    pub(crate) async fn answer(
        &mut self,
        choice: usize,
        text: Option<String>,
    ) -> Result<Answered, Failure> {
        let Some(job) = self.job else {
            return Err(Failure::of(Kind::DaemonError, "the walk has not started"));
        };
        let at_linked_cancel = self.step() == Some(Step::Linked) && choice == 1;
        // A walk the daemon ended while this sat at its prompt is read before anything is sent.
        self.drain();
        if self.ended.is_some() {
            return self.outcome(false);
        }
        self.awaiting = self.state.clone();
        self.moved = false;
        if let Err(failure) = self.send(WalkOp::Answer { job, choice, text }).await {
            self.awaiting = None;
            // A refusal may be the walk's end, which its stream says a moment later at most.
            self.read_end().await;
            if self.ended.is_some() {
                return self.outcome(false);
            }
            return Err(failure);
        }
        self.ready().await?;
        self.outcome(at_linked_cancel)
    }

    /// Go back a step; returns once the walk shows the step it went back to.
    pub(crate) async fn back(&mut self) -> Result<(), Failure> {
        let Some(job) = self.job else {
            return Ok(());
        };
        self.drain();
        if self.ended.is_some() {
            return self.outcome(false).map(|_| ());
        }
        self.awaiting = self.state.clone();
        self.moved = false;
        if let Err(failure) = self.send(WalkOp::Back { job }).await {
            self.awaiting = None;
            self.read_end().await;
            if self.ended.is_some() {
                return self.outcome(false).map(|_| ());
            }
            return Err(failure);
        }
        self.ready().await?;
        // A step that could not be gone back from says so in a refusal, which is dropped: the
        // keys offer a Back only where the state says one exists.
        if let Some(state) = self.state.as_mut() {
            state.refusal = None;
        }
        Ok(())
    }

    /// What the walk's last state says an answer came to.
    fn outcome(&mut self, at_linked_cancel: bool) -> Result<Answered, Failure> {
        match self.ended.clone() {
            Some(Ended::Done(Some(device))) => Ok(Answered::Added(as_config(&device))),
            Some(Ended::Done(None)) => Ok(Answered::Skipped),
            Some(Ended::Cancelled) if at_linked_cancel => Ok(Answered::Cancelled),
            Some(other) => Err(ended_words(&other)),
            None => {
                let refusal = self.state.as_mut().and_then(|state| state.refusal.take());
                Ok(match refusal {
                    Some(refusal) => Answered::Refused(refusal),
                    None => Answered::On,
                })
            }
        }
    }

    /// Leave: cancel the walk this client started, if it is still running, and nothing else.
    /// Cancelling a job that has ended is acknowledged and harmless; a start whose answer was
    /// not read yet is waited for a moment, so the job it gave is not left to idle out.
    pub(crate) async fn leave(&mut self) {
        if let Some(start) = self.start.take()
            && let Ok(Ok(Ok(job))) = tokio::time::timeout(CANCEL_BOUND, start).await
        {
            self.job = Some(job);
        }
        let Some(job) = self.job else {
            return;
        };
        if self.ended.is_some() {
            return;
        }
        let cancel = Request::Walk {
            op: WalkOp::Cancel { job },
        };
        let _ = request_on_own_connection(&self.socket, cancel, Some(CANCEL_BOUND)).await;
        self.ended = Some(Ended::Cancelled);
    }
}

impl Drop for RemoteWalk {
    /// A walk dropped while it runs is cancelled all the same: the job this client started, and
    /// the job a start not yet read will give, never another. The explicit [`RemoteWalk::leave`]
    /// is awaited by the front ends; this is the net under every other way out.
    fn drop(&mut self) {
        let start = self.start.take();
        if self.ended.is_some() || (start.is_none() && self.job.is_none()) {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let (socket, known) = (self.socket.clone(), self.job);
        runtime.spawn(async move {
            let job = match start {
                Some(start) => start.await.ok().and_then(Result::ok).or(known),
                None => known,
            };
            if let Some(job) = job {
                let cancel = Request::Walk {
                    op: WalkOp::Cancel { job },
                };
                let _ = request_on_own_connection(&socket, cancel, Some(CANCEL_BOUND)).await;
            }
        });
    }
}

/// One device heard, as the scan's screen lists it.
fn heard_line(row: &Row) -> String {
    let entry = &row.entry;
    let product = entry.product.as_deref().unwrap_or("unknown");
    let signal = entry
        .rssi
        .map(|rssi| format!("  {rssi} dBm"))
        .unwrap_or_default();
    let configured = row
        .configured
        .as_ref()
        .map(|name| format!("  configured as {name}"))
        .unwrap_or_default();
    format!("{product}  {}{signal}{configured}", entry.unit)
}

/// The failure a start that was not answered with a job is, in the daemon's words.
fn started_badly(socket: &std::path::Path, reply: Result<Response, IpcError>) -> Failure {
    match reply {
        Ok(Response::Error {
            code,
            message,
            reason,
        }) => {
            let mut failure = Failure::from_daemon_on(socket, code, message, reason);
            if reason == Some(Reason::Busy) {
                failure.next = "That walk ends by itself within 10 minutes of its last answer, or at once with `svolder daemon restart`; then try again".to_string();
            }
            failure
        }
        Ok(other) => Failure::of(
            Kind::DaemonError,
            format!("the daemon sent an unexpected reply: {other:?}"),
        ),
        Err(IpcError::Refused(answer)) => started_badly(socket, Ok(*answer)),
        Err(e) => Failure::from_ipc(&e),
    }
}

/// What a walk that ended says when nobody asked it to.
fn ended_words(ended: &Ended) -> Failure {
    match ended {
        Ended::Failed(message) => Failure::of(Kind::DaemonError, message.clone()),
        Ended::Cancelled => Failure::of(
            Kind::DaemonError,
            "the walk was cancelled: it waited too long for an answer, or the daemon stopped",
        ),
        Ended::Done(_) => Failure::of(Kind::DaemonError, "the walk is over"),
    }
}

/// The device a walk wrote, as the block this side shows: the daemon never reads a passphrase
/// back, so the block carries its kind (`auto`, `none`, or `set` for a code).
fn as_config(device: &ConfiguredDevice) -> DeviceConfig {
    DeviceConfig {
        name: device.name.clone(),
        udid: device.unit.clone(),
        profile: device.profile.clone(),
        passphrase: match device.passphrase {
            PassphraseKind::Auto => "auto",
            PassphraseKind::None => "none",
            _ => "set",
        }
        .to_string(),
        inputs: std::collections::BTreeMap::new(),
    }
}

/// How a refusal is worded as a failure: a code the device turned away is the refusal it always
/// was, the rest are things to answer differently.
pub(crate) fn refusal_failure(refusal: &WalkRefusal) -> Failure {
    let kind = match refusal.kind {
        RefusalKind::Passphrase => Kind::Refused,
        RefusalKind::Connect => Kind::NothingInRange,
        RefusalKind::Failed => Kind::DaemonError,
        RefusalKind::Answer | RefusalKind::Taken | RefusalKind::Configured => Kind::Usage,
    };
    Failure::of(kind, refusal.message.clone())
}

#[cfg(test)]
mod tests;
