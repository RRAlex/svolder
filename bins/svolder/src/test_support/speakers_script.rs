// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon's virtual-speakers sets service as a daemon a case scripts: the stub daemon's
//! `speakers` family.
//!
//! A case says what the catalogue lists, how a fetch or an import ends (and what progress a fetch
//! reports first), and whether the jobs are held until the case lets them go; it reads back what
//! was asked. Nothing here reaches a network.

use std::collections::BTreeMap;
use std::sync::Mutex;

use futures::stream::BoxStream;
use futures::{StreamExt, stream};
use svolder_ipc::{
    CODE_USAGE, Event, JobEvent, JobId, JobState, Reason, Response, SpeakersCatalogue, SpeakersOp,
    SpeakersProgress, SpeakersSaved, SpeakersSet,
};
use tokio::sync::broadcast;

/// The id the scripted daemon gives the first job it starts.
pub(crate) const FIRST_JOB: JobId = 31;

/// How a job ends.
#[derive(Clone)]
enum Ending {
    Saved(Box<SpeakersSaved>),
    Fails(String, Reason),
}

struct Inner {
    catalogue: SpeakersCatalogue,
    ops: Vec<SpeakersOp>,
    ending: Ending,
    progress: Vec<(u64, u64)>,
    /// The set each job fetches, for its progress reports.
    fetching: BTreeMap<JobId, String>,
    waiting: Vec<JobId>,
    /// The jobs that end when a follower comes, so its stream tells their progress as it happens.
    on_follow: Vec<JobId>,
    last: BTreeMap<JobId, JobState>,
    cancelled: Vec<JobId>,
    forget_refusal: Option<(String, Reason)>,
    next: JobId,
}

/// The `speakers` family of the stub daemon.
pub(crate) struct SpeakersScript {
    inner: Mutex<Inner>,
    events: broadcast::Sender<Event>,
    hold_jobs: std::sync::atomic::AtomicBool,
    /// Whether a job's start waits for the case to let its answer go.
    start_held: tokio::sync::watch::Sender<bool>,
    /// Signalled when a job is made, and when a cancel arrives.
    started: tokio::sync::Notify,
    cancel_seen: tokio::sync::Notify,
}

impl Default for SpeakersScript {
    fn default() -> SpeakersScript {
        SpeakersScript {
            inner: Mutex::new(Inner {
                catalogue: SpeakersCatalogue {
                    sets: Vec::new(),
                    fetching: true,
                    note: None,
                    max_sets: 32,
                },
                ops: Vec::new(),
                ending: Ending::Fails("no ending was scripted".into(), Reason::Internal),
                progress: Vec::new(),
                fetching: BTreeMap::new(),
                waiting: Vec::new(),
                on_follow: Vec::new(),
                last: BTreeMap::new(),
                cancelled: Vec::new(),
                forget_refusal: None,
                next: FIRST_JOB,
            }),
            events: broadcast::channel(64).0,
            hold_jobs: std::sync::atomic::AtomicBool::new(false),
            start_held: tokio::sync::watch::channel(false).0,
            started: tokio::sync::Notify::new(),
            cancel_seen: tokio::sync::Notify::new(),
        }
    }
}

impl SpeakersScript {
    /// The catalogue answers with these sets.
    pub(crate) fn lists(&self, sets: Vec<SpeakersSet>) {
        self.inner.lock().expect("the script").catalogue.sets = sets;
    }

    /// The catalogue's note, as the daemon words it.
    pub(crate) fn notes(&self, note: &str) {
        self.inner.lock().expect("the script").catalogue.note = Some(note.to_string());
    }

    /// Hold the answer to every job's start, the job already made, until
    /// [`SpeakersScript::lets_starts_go`].
    pub(crate) fn holds_starts(&self) {
        self.start_held.send_replace(true);
    }

    /// Answer the starts held, and every one after.
    pub(crate) fn lets_starts_go(&self) {
        self.start_held.send_replace(false);
    }

    /// Wait until a job has been made.
    pub(crate) async fn job_started(&self) {
        self.started.notified().await;
    }

    /// Wait until a cancel has arrived.
    pub(crate) async fn cancel_arrived(&self) {
        self.cancel_seen.notified().await;
    }

    /// The catalogue says whether the daemon can fetch.
    pub(crate) fn can_fetch(&self, fetching: bool) {
        self.inner.lock().expect("the script").catalogue.fetching = fetching;
    }

    /// Every job from now on saves `set`.
    pub(crate) fn saves(&self, set: SpeakersSet) {
        self.inner.lock().expect("the script").ending = Ending::Saved(Box::new(SpeakersSaved {
            set,
            evicted: Vec::new(),
        }));
    }

    /// Every job from now on fails with these words.
    pub(crate) fn fails(&self, message: &str, reason: Reason) {
        self.inner.lock().expect("the script").ending = Ending::Fails(message.to_string(), reason);
    }

    /// A job reports these `(received, total)` byte counts before it ends.
    pub(crate) fn reports(&self, progress: &[(u64, u64)]) {
        self.inner.lock().expect("the script").progress = progress.to_vec();
    }

    /// A forget is refused with these words.
    pub(crate) fn refuses_forget(&self, message: &str, reason: Reason) {
        self.inner.lock().expect("the script").forget_refusal = Some((message.to_string(), reason));
    }

    /// Start every job from now on and leave it running (until a cancel).
    pub(crate) fn holds_jobs(&self) {
        self.hold_jobs
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// The ops that arrived, in order.
    pub(crate) fn ops(&self) -> Vec<SpeakersOp> {
        self.inner.lock().expect("the script").ops.clone()
    }

    /// The jobs a cancel named.
    pub(crate) fn cancelled(&self) -> Vec<JobId> {
        self.inner.lock().expect("the script").cancelled.clone()
    }

    fn publish(&self, inner: &mut Inner, job: JobId, state: JobState) {
        inner.last.insert(job, state.clone());
        let _ = self.events.send(Event::Speakers(JobEvent { job, state }));
    }

    fn end(&self, inner: &mut Inner, job: JobId) {
        for (received, total) in inner.progress.clone() {
            let payload = serde_json::to_value(SpeakersProgress {
                id: inner.fetching.get(&job).cloned().unwrap_or_default(),
                received,
                total,
            })
            .expect("a progress is JSON");
            self.publish(inner, job, JobState::Progress { payload });
        }
        let state = match inner.ending.clone() {
            Ending::Saved(saved) => JobState::Done {
                payload: serde_json::to_value(saved).expect("a saved set is JSON"),
            },
            Ending::Fails(message, reason) => JobState::Failed { message, reason },
        };
        self.publish(inner, job, state);
    }

    /// Answer an op that is not the follow.
    pub(crate) async fn handle(&self, op: SpeakersOp) -> Response {
        let mut held = self.start_held.subscribe();
        let (response, job) = self.answer(op);
        if job {
            self.started.notify_one();
            let _ = held.wait_for(|held| !*held).await;
        }
        response
    }

    /// The answer to `op`, and whether it made a job.
    fn answer(&self, op: SpeakersOp) -> (Response, bool) {
        let mut inner = self.inner.lock().expect("the script");
        inner.ops.push(op.clone());
        let response = match op {
            SpeakersOp::Catalogue => Response::SpeakersCatalogue(Box::new(inner.catalogue.clone())),
            SpeakersOp::Fetch { .. } | SpeakersOp::Import { .. } => {
                let job = inner.next;
                inner.next += 1;
                if let SpeakersOp::Fetch { id } = &op {
                    inner.fetching.insert(job, id.clone());
                }
                self.publish(&mut inner, job, JobState::Started);
                match self.hold_jobs.load(std::sync::atomic::Ordering::SeqCst) {
                    true => inner.waiting.push(job),
                    false => inner.on_follow.push(job),
                }
                return (Response::Job { job }, true);
            }
            SpeakersOp::Forget { .. } => match inner.forget_refusal.clone() {
                Some((message, reason)) => Response::error(CODE_USAGE, message).with_reason(reason),
                None => Response::Ack,
            },
            SpeakersOp::Cancel { job } => {
                inner.cancelled.push(job);
                if let Some(at) = inner.waiting.iter().position(|held| *held == job) {
                    inner.waiting.remove(at);
                    self.publish(&mut inner, job, JobState::Cancelled);
                }
                self.cancel_seen.notify_one();
                Response::Ack
            }
            SpeakersOp::ImportDir | SpeakersOp::Follow => {
                Response::error(CODE_USAGE, "the script does not run this op")
            }
        };
        (response, false)
    }

    /// The follow stream: the last state of each job, then each change.
    pub(crate) fn follow(&self) -> BoxStream<'static, Event> {
        let mut inner = self.inner.lock().expect("the script");
        let rx = self.events.subscribe();
        let replay: Vec<Event> = inner
            .last
            .iter()
            .map(|(job, state)| {
                Event::Speakers(JobEvent {
                    job: *job,
                    state: state.clone(),
                })
            })
            .collect();
        for job in std::mem::take(&mut inner.on_follow) {
            self.end(&mut inner, job);
        }
        Box::pin(
            stream::iter(replay).chain(stream::unfold(rx, |mut rx| async move {
                rx.recv().await.ok().map(|event| (event, rx))
            })),
        )
    }
}

/// A fetched set of invented words: `id` under `room`, labelled "Label of <id>".
pub(crate) fn set(id: &str, room: svolder_ipc::SpeakersRoom) -> SpeakersSet {
    SpeakersSet {
        id: id.to_string(),
        room,
        label: format!("Label of {id}"),
        licence: "CC-BY-4.0".to_string(),
        credit: String::new(),
        source_url: String::new(),
        bytes: 0,
        own: false,
        fetched: true,
        damaged: None,
        angles: None,
        taps_ms: 0.0,
        rate: 48_000,
        loudness_db: 0.0,
        cut: false,
    }
}
