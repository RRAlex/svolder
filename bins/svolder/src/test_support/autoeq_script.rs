// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon's AutoEq service as a daemon a case scripts: the stub daemon's `auto_eq` family.
//!
//! A case says what a search answers (an index of its own lines, grouped as the daemon groups),
//! how an import ends, and whether a search or a job is held until the case lets it go; it reads
//! back what was asked. Nothing here reaches a network: it is what a front end's side of the
//! socket is tested against.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use futures::stream::BoxStream;
use futures::{StreamExt, stream};
use svolder_autoeq::{Entry, parse_index, search};
use svolder_ipc::{
    AutoEqGroup, AutoEqMeasurement, AutoEqOp, AutoEqSaved, AutoEqSearch, AutoEqVariant, CODE_USAGE,
    Event, JobEvent, JobId, JobState, Reason, Response,
};
use tokio::sync::broadcast;

/// The id the scripted daemon gives the first job it starts.
pub(crate) const FIRST_JOB: JobId = 11;

/// The search answer for `query` over `entries`, grouped by headphone as the daemon groups: one
/// group per name as lowercase letters and digits, its measurements in the index's own order.
pub(crate) fn answer_of(entries: &[Entry], query: &str) -> AutoEqSearch {
    let mut groups: Vec<AutoEqGroup> = Vec::new();
    for entry in search(entries, query) {
        let key = entry.group_key();
        let measurement = AutoEqMeasurement {
            variant: AutoEqVariant {
                name: entry.name.clone(),
                source: entry.source.clone(),
                form: entry.form.clone(),
                rig: entry.rig.clone(),
            },
            words: entry.measurement(),
        };
        match groups.iter_mut().find(|group| group.key == key) {
            Some(group) => group.variants.push(measurement),
            None => groups.push(AutoEqGroup {
                key,
                name: entry.name.clone(),
                variants: vec![measurement],
            }),
        }
    }
    AutoEqSearch {
        groups,
        more: 0,
        measurements: entries.len(),
        fetched_unix: 1_782_864_000,
        note: None,
    }
}

/// How the scripted import ends.
#[derive(Clone)]
enum Ending {
    /// It saves this preset (and writes it to the file a case named, when it did).
    Saved(AutoEqSaved),
    /// It fails with these words and this reason.
    Fails(String, Reason),
}

/// What the scripted family has been asked and what it will say.
struct Inner {
    /// The index lines a search runs over.
    entries: Vec<Entry>,
    /// The `(query, refresh)` of each search that arrived, in order.
    searches: Vec<(String, bool)>,
    /// The import ops that arrived, in order.
    imports: Vec<AutoEqOp>,
    /// How an import ends.
    ending: Ending,
    /// The jobs still waiting for the case to finish them.
    waiting: Vec<JobId>,
    /// The last state of each job, which a follower is given first.
    last: BTreeMap<JobId, JobState>,
    /// The jobs a cancel named.
    cancelled: Vec<JobId>,
    /// The origin each import job gives its preset.
    origins: BTreeMap<JobId, String>,
    /// The next job's id.
    next: JobId,
    /// A file the import adds the preset to, with the text of its correction.
    saves_into: Option<(PathBuf, String)>,
}

/// The `auto_eq` family of the stub daemon.
pub(crate) struct AutoEqScript {
    inner: Mutex<Inner>,
    events: broadcast::Sender<Event>,
    /// Whether a search waits for the case to let it go.
    held: tokio::sync::watch::Sender<bool>,
    /// Whether an import's start waits for the case to let its answer go.
    start_held: tokio::sync::watch::Sender<bool>,
    /// Whether the follow stream is refused.
    refuse_follow: std::sync::atomic::AtomicBool,
    /// Whether an import starts and then waits for the case to finish it.
    hold_jobs: std::sync::atomic::AtomicBool,
    /// How many follow streams were opened.
    follows: std::sync::atomic::AtomicUsize,
}

impl Default for AutoEqScript {
    fn default() -> AutoEqScript {
        AutoEqScript {
            inner: Mutex::new(Inner {
                entries: Vec::new(),
                searches: Vec::new(),
                imports: Vec::new(),
                ending: Ending::Fails("no ending was scripted".into(), Reason::Internal),
                waiting: Vec::new(),
                last: BTreeMap::new(),
                cancelled: Vec::new(),
                origins: BTreeMap::new(),
                next: FIRST_JOB,
                saves_into: None,
            }),
            events: broadcast::channel(64).0,
            held: tokio::sync::watch::channel(false).0,
            start_held: tokio::sync::watch::channel(false).0,
            refuse_follow: std::sync::atomic::AtomicBool::new(false),
            hold_jobs: std::sync::atomic::AtomicBool::new(false),
            follows: std::sync::atomic::AtomicUsize::new(0),
        }
    }
}

impl AutoEqScript {
    /// Search over these index lines from now on.
    pub(crate) fn index(&self, lines: &[&str]) {
        self.inner.lock().expect("the script").entries = parse_index(&lines.join("\n"));
    }

    /// Every import from now on saves a preset named `name` with `filters` filters.
    pub(crate) fn saves(&self, name: &str, filters: usize) {
        self.inner.lock().expect("the script").ending = Ending::Saved(AutoEqSaved {
            name: name.to_string(),
            filters,
            skipped: 0,
        });
    }

    /// Every import from now on adds its preset to the configuration file at `path`, with the
    /// correction `text` says.
    pub(crate) fn writes_into(&self, path: PathBuf, text: &str) {
        self.inner.lock().expect("the script").saves_into = Some((path, text.to_string()));
    }

    /// Every import from now on fails with `message` and `reason`.
    pub(crate) fn fails(&self, message: &str, reason: Reason) {
        self.inner.lock().expect("the script").ending = Ending::Fails(message.to_string(), reason);
    }

    /// Hold every search from now on until [`AutoEqScript::lets_searches_go`].
    pub(crate) fn holds_searches(&self) {
        self.held.send_replace(true);
    }

    /// Answer the searches held, and every one after.
    pub(crate) fn lets_searches_go(&self) {
        self.held.send_replace(false);
    }

    /// Hold the answer to every import's start from now on, the job already made, until
    /// [`AutoEqScript::lets_starts_go`].
    pub(crate) fn holds_starts(&self) {
        self.start_held.send_replace(true);
    }

    /// Answer the starts held, and every one after.
    pub(crate) fn lets_starts_go(&self) {
        self.start_held.send_replace(false);
    }

    /// Refuse the follow stream from now on, as a daemon that cannot open one does.
    pub(crate) fn refuses_follow(&self) {
        self.refuse_follow
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Whether the follow stream is refused.
    pub(crate) fn follow_refused(&self) -> bool {
        self.refuse_follow.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Start every import from now on and leave it running until [`AutoEqScript::finishes`].
    pub(crate) fn holds_jobs(&self) {
        self.hold_jobs
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// End the jobs held, as the scripted ending says.
    pub(crate) fn finishes(&self) {
        let mut inner = self.inner.lock().expect("the script");
        for job in std::mem::take(&mut inner.waiting) {
            self.end(&mut inner, job);
        }
    }

    /// The `(query, refresh)` of each search that arrived, in order.
    pub(crate) fn searches(&self) -> Vec<(String, bool)> {
        self.inner.lock().expect("the script").searches.clone()
    }

    /// The import ops that arrived, in order.
    pub(crate) fn imports(&self) -> Vec<AutoEqOp> {
        self.inner.lock().expect("the script").imports.clone()
    }

    /// How many follow streams were opened.
    pub(crate) fn follows(&self) -> usize {
        self.follows.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The jobs a cancel named.
    pub(crate) fn cancelled(&self) -> Vec<JobId> {
        self.inner.lock().expect("the script").cancelled.clone()
    }

    /// Publish `state` of `job`, keeping it as the job's last.
    fn publish(&self, inner: &mut Inner, job: JobId, state: JobState) {
        inner.last.insert(job, state.clone());
        let _ = self.events.send(Event::AutoEq(JobEvent { job, state }));
    }

    /// End `job` the way the script says.
    fn end(&self, inner: &mut Inner, job: JobId) {
        let state = match inner.ending.clone() {
            Ending::Saved(saved) => {
                if let Some((path, text)) = inner.saves_into.clone() {
                    let origin = inner.origins.get(&job).cloned();
                    write_preset(&path, &saved.name, &text, origin);
                }
                JobState::Done {
                    payload: serde_json::to_value(saved).expect("a saved preset is JSON"),
                }
            }
            Ending::Fails(message, reason) => JobState::Failed { message, reason },
        };
        self.publish(inner, job, state);
    }

    /// Answer an op that is not the follow.
    pub(crate) async fn handle(&self, op: AutoEqOp) -> Response {
        match op {
            AutoEqOp::Search { query, refresh } => {
                let mut held = self.held.subscribe();
                let answer = {
                    let mut inner = self.inner.lock().expect("the script");
                    inner.searches.push((query.clone(), refresh));
                    answer_of(&inner.entries, &query)
                };
                let _ = held.wait_for(|held| !*held).await;
                Response::AutoEqSearch(Box::new(answer))
            }
            AutoEqOp::Import { ref variant, .. } => {
                let job = {
                    let mut inner = self.inner.lock().expect("the script");
                    let job = inner.next;
                    inner.next += 1;
                    let origin = Entry {
                        name: variant.name.clone(),
                        source: variant.source.clone(),
                        form: variant.form.clone(),
                        rig: variant.rig.clone(),
                    }
                    .origin();
                    inner.origins.insert(job, origin);
                    inner.imports.push(op);
                    self.publish(&mut inner, job, JobState::Started);
                    match self.hold_jobs.load(std::sync::atomic::Ordering::SeqCst) {
                        true => inner.waiting.push(job),
                        false => self.end(&mut inner, job),
                    }
                    job
                };
                let mut held = self.start_held.subscribe();
                let _ = held.wait_for(|held| !*held).await;
                Response::Job { job }
            }
            AutoEqOp::Upload { .. } => {
                let inner = self.inner.lock().expect("the script");
                match inner.ending.clone() {
                    Ending::Saved(saved) => Response::AutoEqSaved(saved),
                    Ending::Fails(message, reason) => {
                        Response::error(CODE_USAGE, message).with_reason(reason)
                    }
                }
            }
            AutoEqOp::Cancel { job } => {
                let mut inner = self.inner.lock().expect("the script");
                inner.cancelled.push(job);
                // A job still running ends cancelled, as the daemon's does.
                if let Some(at) = inner.waiting.iter().position(|held| *held == job) {
                    inner.waiting.remove(at);
                    self.publish(&mut inner, job, JobState::Cancelled);
                }
                Response::Ack
            }
            AutoEqOp::Preview { .. } | AutoEqOp::Follow => {
                Response::error(CODE_USAGE, "the script does not run this op")
            }
        }
    }

    /// The follow stream: the last state of each job, then each change.
    pub(crate) fn follow(&self) -> BoxStream<'static, Event> {
        let inner = self.inner.lock().expect("the script");
        let rx = self.events.subscribe();
        self.follows
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let replay: Vec<Event> = inner
            .last
            .iter()
            .map(|(job, state)| {
                Event::AutoEq(JobEvent {
                    job: *job,
                    state: state.clone(),
                })
            })
            .collect();
        Box::pin(
            stream::iter(replay).chain(stream::unfold(rx, |mut rx| async move {
                rx.recv().await.ok().map(|event| (event, rx))
            })),
        )
    }
}

/// Add the imported preset `name` to the configuration file at `path`, as the daemon's writer
/// does.
fn write_preset(path: &std::path::Path, name: &str, text: &str, origin: Option<String>) {
    use svolder_ipc::Config;
    use svolder_profiles::tone::apo;
    use svolder_profiles::tone::preset::{Imported, Preset};
    let parsed = apo::parse(text).expect("the scripted correction parses");
    let preset = Preset::imported(
        name,
        Imported {
            origin,
            correction: parsed.correction,
        },
    );
    Config::add_preset(path, &preset).expect("the scripted import writes the file");
}
