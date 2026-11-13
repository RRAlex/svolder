// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device walk as a daemon a case scripts: the stub daemon's `walk` family.
//!
//! A case says what a start publishes and what each answer or step back that follows publishes
//! (states of the one job it starts, and of foreign jobs the replay carries), and reads back
//! the ops that arrived. Nothing here walks anything: it is what a front end's side of the
//! socket is tested against.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;

use futures::stream::BoxStream;
use futures::{StreamExt, stream};
use svolder_ipc::{CODE_USAGE, Event, JobEvent, JobId, JobState, Reason, Response, WalkOp};
use svolder_walk::{Phase, Prompt, Step, Takes, WalkState};
use tokio::sync::broadcast;

/// The id the scripted daemon gives the walk a client starts.
pub(crate) const JOB: JobId = 7;

/// What the scripted walk family has been asked and what it will say.
struct Inner {
    /// What a start is refused with while a walk runs elsewhere.
    busy: Option<String>,
    /// The states a start publishes.
    started: Vec<JobState>,
    /// What each answer or step back publishes, in order.
    steps: VecDeque<Vec<JobState>>,
    /// The last state of each job, which a follower is given first.
    last: BTreeMap<JobId, JobState>,
    /// The ops that arrived, in the words `{:?}` gives them (an answer's text hidden).
    ops: Vec<String>,
    /// The answers that arrived: the choice and the text.
    answers: Vec<(usize, Option<String>)>,
    /// The jobs a cancel named.
    cancelled: Vec<JobId>,
    /// How many follow streams were opened.
    follows: usize,
}

/// The walk family of the stub daemon.
pub(crate) struct WalkScript {
    inner: Mutex<Inner>,
    events: broadcast::Sender<Event>,
    /// Signalled by each cancel that arrives, so a case waits for one without sleeping.
    cancels: tokio::sync::Notify,
    /// Raised to end every follow stream, as a daemon that stops does.
    hangup: tokio::sync::watch::Sender<bool>,
}

impl Default for WalkScript {
    fn default() -> WalkScript {
        WalkScript {
            inner: Mutex::new(Inner {
                busy: None,
                started: Vec::new(),
                steps: VecDeque::new(),
                last: BTreeMap::new(),
                ops: Vec::new(),
                answers: Vec::new(),
                cancelled: Vec::new(),
                follows: 0,
            }),
            events: broadcast::channel(64).0,
            cancels: tokio::sync::Notify::new(),
            hangup: tokio::sync::watch::channel(false).0,
        }
    }
}

/// A prompt that takes a choice.
pub(crate) fn choice_prompt(step: Step, title: &str, choices: &[&str]) -> Prompt {
    Prompt {
        step,
        title: title.into(),
        body: Vec::new(),
        choices: choices.iter().map(|c| (*c).to_string()).collect(),
        default: 0,
        takes: Takes::Choice,
    }
}

/// A prompt that takes a line, whose last body line is the default.
pub(crate) fn text_prompt(step: Step, title: &str, default: &str, takes: Takes) -> Prompt {
    Prompt {
        step,
        title: title.into(),
        body: vec![default.into()],
        choices: Vec::new(),
        default: 0,
        takes,
    }
}

/// The state of a walk that waits at `prompt`.
pub(crate) fn waiting(prompt: Prompt, can_go_back: bool) -> JobState {
    progress(WalkState {
        origin: "socket".into(),
        phase: Phase::Waiting,
        working: None,
        prompt,
        rows: Vec::new(),
        can_go_back,
        refusal: None,
        connect: None,
    })
}

/// The state of a walk that waits at `prompt` and says why the last answer was turned away.
pub(crate) fn refusing(prompt: Prompt, kind: svolder_walk::RefusalKind, why: &str) -> JobState {
    progress(WalkState {
        origin: "socket".into(),
        phase: Phase::Waiting,
        working: None,
        prompt,
        rows: Vec::new(),
        can_go_back: true,
        refusal: Some(svolder_walk::WalkRefusal {
            message: why.into(),
            kind,
        }),
        connect: None,
    })
}

/// The state of a walk that works on an answer given at `prompt`.
pub(crate) fn working(prompt: Prompt) -> JobState {
    progress(WalkState {
        origin: "socket".into(),
        phase: Phase::Working,
        working: Some(svolder_walk::Working::Scanning),
        prompt,
        rows: Vec::new(),
        can_go_back: false,
        refusal: None,
        connect: None,
    })
}

/// The state of a walk that scans for the answer given at `prompt` and has heard `rows` so far.
pub(crate) fn scanning(prompt: Prompt, rows: Vec<svolder_walk::Row>) -> JobState {
    progress(WalkState {
        origin: "socket".into(),
        phase: Phase::Working,
        working: Some(svolder_walk::Working::Scanning),
        prompt,
        rows,
        can_go_back: false,
        refusal: None,
        connect: None,
    })
}

/// A progress carrying `state`.
fn progress(state: WalkState) -> JobState {
    JobState::Progress {
        payload: serde_json::to_value(state).expect("a state is JSON"),
    }
}

/// The end of a walk that wrote `name`.
pub(crate) fn done(name: &str, unit: &str) -> JobState {
    JobState::Done {
        payload: serde_json::json!({
            "unit": unit,
            "name": name,
            "profile": null,
            "passphrase": "auto",
            "default": false,
            "connected": false,
        }),
    }
}

impl WalkScript {
    /// Say what a start publishes.
    pub(crate) fn starts_with(&self, states: Vec<JobState>) {
        self.inner.lock().expect("the walk script").started = states;
    }

    /// Say what the next answer or step back publishes, after those already queued.
    pub(crate) fn then(&self, states: Vec<JobState>) {
        self.inner
            .lock()
            .expect("the walk script")
            .steps
            .push_back(states);
    }

    /// Refuse every start as `busy`, in words that say it was started by `origin`.
    pub(crate) fn busy_with(&self, origin: &str) {
        self.inner.lock().expect("the walk script").busy = Some(origin.to_string());
    }

    /// Publish `state` as the state of a job no client of this case started.
    pub(crate) fn foreign(&self, job: JobId, state: JobState) {
        self.publish(&mut self.inner.lock().expect("the walk script"), job, state);
    }

    /// Every op that arrived, as `{:?}` prints it.
    pub(crate) fn ops(&self) -> Vec<String> {
        self.inner.lock().expect("the walk script").ops.clone()
    }

    /// The answers that arrived, with the text each carried.
    pub(crate) fn answers(&self) -> Vec<(usize, Option<String>)> {
        self.inner.lock().expect("the walk script").answers.clone()
    }

    /// The jobs a cancel named.
    pub(crate) fn cancelled(&self) -> Vec<JobId> {
        self.inner
            .lock()
            .expect("the walk script")
            .cancelled
            .clone()
    }

    /// End every follow stream.
    pub(crate) fn hang_up(&self) {
        self.hangup.send_replace(true);
    }

    /// Wait until a cancel has arrived (one that came before is not missed).
    pub(crate) async fn cancel_arrived(&self) {
        self.cancels.notified().await;
    }

    /// How many follow streams were opened.
    pub(crate) fn follows(&self) -> usize {
        self.inner.lock().expect("the walk script").follows
    }

    /// Publish `state` of `job`, keeping it as the job's last.
    fn publish(&self, inner: &mut Inner, job: JobId, state: JobState) {
        inner.last.insert(job, state.clone());
        let _ = self.events.send(Event::Walk(JobEvent { job, state }));
    }

    /// Answer a walk op that is not the follow.
    pub(crate) fn handle(&self, op: WalkOp) -> Response {
        let mut inner = self.inner.lock().expect("the walk script");
        inner.ops.push(format!("{op:?}"));
        match op {
            WalkOp::Start => {
                if let Some(origin) = inner.busy.clone() {
                    return Response::error(
                        CODE_USAGE,
                        format!("a walk is already running (job 3, started by {origin})"),
                    )
                    .with_reason(Reason::Busy);
                }
                for state in std::mem::take(&mut inner.started) {
                    self.publish(&mut inner, JOB, state);
                }
                Response::Job { job: JOB }
            }
            WalkOp::Answer { job, choice, text } => {
                inner.answers.push((choice, text));
                self.step(&mut inner, job)
            }
            WalkOp::Back { job } => self.step(&mut inner, job),
            WalkOp::Cancel { job } => {
                inner.cancelled.push(job);
                self.publish(&mut inner, job, JobState::Cancelled);
                self.cancels.notify_one();
                Response::Ack
            }
            WalkOp::Follow => Response::error(CODE_USAGE, "follow is a stream"),
        }
    }

    /// Publish what the next scripted step says, for an op naming `job`.
    fn step(&self, inner: &mut Inner, job: JobId) -> Response {
        if job != JOB {
            return Response::error(CODE_USAGE, "no such job").with_reason(Reason::NoSuchJob);
        }
        // As the daemon's inbox does: an ended job takes no answer.
        if inner.last.get(&job).is_some_and(JobState::is_final) {
            return Response::error(
                CODE_USAGE,
                format!("walk {job} is not waiting for an answer"),
            )
            .with_reason(Reason::NotWaiting);
        }
        for state in inner.steps.pop_front().unwrap_or_default() {
            self.publish(inner, job, state);
        }
        Response::Ack
    }

    /// The follow stream: the last state of every job, then each change.
    pub(crate) fn follow(&self) -> BoxStream<'static, Event> {
        let mut inner = self.inner.lock().expect("the walk script");
        inner.follows += 1;
        let rx = self.events.subscribe();
        let replay: Vec<Event> = inner
            .last
            .iter()
            .map(|(job, state)| {
                Event::Walk(JobEvent {
                    job: *job,
                    state: state.clone(),
                })
            })
            .collect();
        let mut hung_up = self.hangup.subscribe();
        Box::pin(
            stream::iter(replay)
                .chain(stream::unfold(rx, |mut rx| async move {
                    rx.recv().await.ok().map(|event| (event, rx))
                }))
                .take_until(async move {
                    let _ = hung_up.wait_for(|up| *up).await;
                }),
        )
    }
}
