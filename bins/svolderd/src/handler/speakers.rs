// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Virtual speakers' sets in the daemon: the one service that reaches the sets' sources, keeps the
//! converted sets, and owns the jobs that fetch and import them.
//!
//! Placement rule: as AutoEq's. [`Daemon`] holds the service in one field and routes
//! `Request::Speakers` to it in one line of `handle_from` and one of `stream`; the web and MCP
//! reach the family through `Fleet::request` and `Fleet::stream` only. A front end tells whether
//! a daemon has the ops by the feature `speakers`.
//!
//! The service owns the speakers job slot ([`Slots::One`]: a fetch or an import at a time), the
//! shared state of the ops ([`core`]: one store, one client, the write gate), serves the stream
//! that follows its jobs, and is stopped first when the daemon stops. `catalogue` and `forget`
//! are plain requests; `fetch` and `import` are jobs: the request answers with the job's id, a
//! download's progress rides in the job's progress events and the saved set in its `done`.
//!
//! The store and the sets' base address are the ones the daemon is given
//! ([`Daemon::with_speakers`]): main reads the environment variable and the state directory once,
//! and a test hands in a temporary directory and a loopback server. A daemon given none answers
//! every op with `internal`, so a test daemon never touches the person's sets.
//!
//! The store is one `Arc` in the daemon: the service holds it, and the host service hands the
//! same one to every real computer's EQ controller, which pins the sets its chain holds in it
//! (two stores over one directory would each have a pin of their own, and a forget through one
//! would remove a WAV that plays through the other). [`Daemon::link_speakers_to_host`] links the
//! service to the computer that runs: the chosen set, and the call that says the sets changed.
//!
//! - `core`: the store, the client, the gate, and the blocking work of each op.

mod core;

pub use self::core::SpeakersLinks;
pub(crate) use self::core::room_of;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use futures::stream::BoxStream;
use svolder_ipc::{
    CODE_USAGE, Event, Reason, Response, SpeakersOp, SpeakersProgress, SpeakersRoom,
};
use svolder_speakers::{Cancel, Client, Store};
use tokio::sync::mpsc;

use self::core::{Core, Failure};
use super::Daemon;
use super::jobs::{JobCtx, JobError, Jobs, Slots};
use crate::writer::Origin;

/// Pin in `store` the set a computer EQ chain left running holds, as `state` (the computer
/// EQ's state file) records it, until a controller starts and pins for itself: a daemon whose
/// host is off or fake runs no controller, and the chain (a unit or a process) outlives the
/// daemon that applied it. The set is `speakers_loaded` whenever the state names one (every
/// apply writes it and a remove clears it, and a chain that runs for the stage alone has the
/// EQ's own `on` false), or for a state from before it the chosen set when the speakers were on
/// with the chain (the config then holds their convolvers).
pub fn pin_from_state(store: &Store, state: Option<&svolder_host::eq::HostEqState>) {
    let held = state.and_then(|state| {
        state.speakers_loaded.as_deref().or(state
            .speakers
            .as_deref()
            .filter(|_| state.on && state.speakers_on))
    });
    store.set_in_use(&held.into_iter().collect::<Vec<_>>());
}

/// Where a file the web sends waits to be read, under the store's own directory so one sweep at
/// start finds what a crash left.
fn import_dir_of(store: &Store) -> PathBuf {
    store.dir().join(".import")
}

/// The speakers' jobs and the state their ops share.
pub(super) struct SpeakersService {
    jobs: Jobs,
    core: Option<Arc<Core>>,
}

impl SpeakersService {
    /// A service with no job, keeping sets in `store` and fetching through `client` (none: it
    /// keeps and reaches nothing).
    pub(super) fn new(setup: Option<(Client, Arc<Store>)>) -> SpeakersService {
        SpeakersService {
            jobs: Jobs::new(Slots::One),
            core: setup.map(|(client, store)| Arc::new(Core::new(client, store))),
        }
    }

    /// The stream `op` turns its connection into, or the answer that refuses it.
    pub(super) fn stream(
        &self,
        op: SpeakersOp,
    ) -> Result<BoxStream<'static, Event>, Box<Response>> {
        match op {
            SpeakersOp::Follow => Ok(self.jobs.follow().map(Event::Speakers).boxed()),
            SpeakersOp::Cancel { .. } => Err(Box::new(Jobs::cancel_is_no_stream("speakers"))),
            _ => Err(Box::new(Response::error(
                CODE_USAGE,
                "that speakers op is not a stream",
            ))),
        }
    }

    /// Cancel the speakers jobs and wait for them no longer than `bound`; how many were given up
    /// on.
    pub(super) async fn shutdown(&self, bound: Duration) -> usize {
        self.jobs.stop_all(bound).await
    }

    /// The one store of converted sets, which the controller of the computer EQ shares (it pins
    /// the sets its chain holds and checks one before every apply).
    pub(super) fn store(&self) -> Option<&Arc<Store>> {
        self.core.as_ref().map(|core| &core.store)
    }

    /// `answered`, a change of the virtual speakers, with the reason `missing` when it is a
    /// refusal and its set (the one it names, else the chosen one) cannot be played from the
    /// store: a front end then reads the sets again rather than match the words.
    pub(super) fn missing_reason(&self, answered: Response, named: Option<&str>) -> Response {
        let (Response::Error { .. }, Some(core)) = (&answered, &self.core) else {
            return answered;
        };
        match named.map(str::to_string).or_else(|| core.chosen()) {
            Some(id) if core.store.check(&id).is_err() => answered.with_reason(Reason::Missing),
            _ => answered,
        }
    }

    /// The speakers' jobs, for the tests that start one.
    #[cfg(test)]
    pub(super) fn jobs(&self) -> &Jobs {
        &self.jobs
    }

    #[allow(
        clippy::result_large_err,
        reason = "the refusal is the answer the handler returns as it is"
    )]
    fn core(&self) -> Result<Arc<Core>, Response> {
        self.core.clone().ok_or_else(|| {
            Failure::new(
                "this daemon was not set up to keep virtual speakers' sets",
                Reason::Internal,
            )
            .answer()
        })
    }
}

impl Daemon {
    /// The daemon keeping virtual speakers' sets in `store` and fetching them through `client`.
    /// The host service serves the speakers from the same store in every real computer it
    /// starts.
    pub fn with_speakers(mut self, client: Client, store: impl Into<Arc<Store>>) -> Self {
        let store = store.into();
        self.host.serve_speakers(Arc::clone(&store));
        self.speakers = SpeakersService::new(Some((client, store)));
        self
    }

    /// The daemon's one store of converted sets, the very one the computer EQ's controller pins
    /// the sets its chain holds in, for the tests that look at it.
    #[cfg(test)]
    pub fn speakers_store(&self) -> Option<Arc<Store>> {
        self.speakers.store().cloned()
    }

    /// Link the computer EQ's controller: the chosen set (never evicted) and the call that says
    /// the sets changed. The first link stays; a daemon with no speakers ignores it.
    pub fn link_speakers(&self, links: SpeakersLinks) {
        if let Some(core) = &self.speakers.core {
            core.link(links);
        }
    }

    /// [`Daemon::link_speakers`] to whichever computer the host service runs, now and after any
    /// restart of it: the set its controller keeps chosen, and its controller reading the sets
    /// again (on this runtime, whatever thread the service calls from). Needs a tokio runtime.
    pub fn link_speakers_to_host(&self) {
        self.link_speakers(SpeakersLinks {
            chosen: self.host.chosen_speakers_fn(),
            changed: self.host.sets_changed_fn(),
        });
    }

    /// Remove what a daemon that was killed mid-import left under the store's import directory.
    /// Run at start, when no import can be running.
    pub fn sweep_speakers_imports(&self) {
        let Some(store) = self.speakers.store() else {
            return;
        };
        if let Ok(entries) = std::fs::read_dir(import_dir_of(store)) {
            for entry in entries.flatten() {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    /// [`Daemon::with_speakers`] for the pair main builds.
    pub fn with_speakers_setup(self, (client, store): (Client, Store)) -> Self {
        self.with_speakers(client, store)
    }

    /// Answer one `speakers` op for `origin`. The follow op is a stream, which this refuses as
    /// `handle_from` refuses every stream.
    pub(super) async fn speakers_op(&self, origin: Origin, op: SpeakersOp) -> Response {
        match op {
            SpeakersOp::Cancel { job } => return self.speakers.jobs.cancel_answer(job),
            SpeakersOp::Follow => return Jobs::follow_needs_a_stream("speakers"),
            _ => {}
        }
        let core = match self.speakers.core() {
            Ok(core) => core,
            Err(answer) => return answer,
        };
        match op {
            SpeakersOp::Catalogue => {
                match tokio::task::spawn_blocking(move || core.catalogue()).await {
                    Ok(catalogue) => Response::SpeakersCatalogue(Box::new(catalogue)),
                    Err(_) => stopped_unexpectedly().answer(),
                }
            }
            SpeakersOp::Forget { id } => {
                match tokio::task::spawn_blocking(move || core.forget(&id)).await {
                    Ok(Ok(())) => Response::Ack,
                    Ok(Err(failure)) => failure.answer(),
                    Err(_) => stopped_unexpectedly().answer(),
                }
            }
            SpeakersOp::ImportDir => Response::SpeakersImportDir {
                path: import_dir_of(&core.store).display().to_string(),
            },
            SpeakersOp::Fetch { id } => self.speakers_fetch(origin, core, id),
            SpeakersOp::Import { path, label, room } => {
                self.speakers_import(origin, core, PathBuf::from(path), label, room)
            }
            SpeakersOp::Cancel { .. } | SpeakersOp::Follow => unreachable!("answered above"),
        }
    }

    fn speakers_fetch(&self, origin: Origin, core: Arc<Core>, id: String) -> Response {
        if svolder_speakers::find(&id).is_none() {
            return Failure::new(format!("{id} is not in the catalogue"), Reason::NotListed)
                .answer();
        }
        if core.client.is_off() {
            return Failure::new(svolder_speakers::OFF_WORDS, Reason::Unreachable).answer();
        }
        self.speakers
            .jobs
            .start_answer("speakers job", origin.who(), move |ctx| async move {
                let (told, mut heard) = mpsc::unbounded_channel::<(u64, u64)>();
                let cancel = Cancel::new();
                let work = {
                    let (cancel, id) = (cancel.clone(), id.clone());
                    tokio::task::spawn_blocking(move || {
                        // A progress goes out when the whole percent moves, so a download is a
                        // hundred events at most.
                        let mut last = u64::MAX;
                        core.fetch(&id, &cancel, &mut |received, total| {
                            let percent = received.saturating_mul(100) / total.max(1);
                            if percent != last {
                                last = percent;
                                let _ = told.send((received, total));
                            }
                        })
                    })
                };
                tokio::pin!(work);
                let done = loop {
                    tokio::select! {
                        Some((received, total)) = heard.recv() => {
                            say(&ctx, &id, received, total);
                        }
                        done = &mut work => break done,
                        () = ctx.cancelled() => {
                            // The download stops at its next chunk; the job ends now.
                            cancel.cancel();
                            return Err(JobError::Cancelled);
                        }
                    }
                };
                result_of(done)
            })
    }

    fn speakers_import(
        &self,
        origin: Origin,
        core: Arc<Core>,
        path: PathBuf,
        label: Option<String>,
        room: Option<SpeakersRoom>,
    ) -> Response {
        self.speakers
            .jobs
            .start_answer("speakers job", origin.who(), move |ctx| async move {
                let cancel = Cancel::new();
                let work = {
                    let cancel = cancel.clone();
                    tokio::task::spawn_blocking(move || {
                        core.import(&path, label.as_deref(), room, &cancel)
                    })
                };
                tokio::pin!(work);
                // The conversion cannot be interrupted: a cancel is noted, and the job ends when
                // the thread does, with the set it made removed.
                let mut asked = false;
                let done = loop {
                    tokio::select! {
                        done = &mut work => break done,
                        () = ctx.cancelled(), if !asked => {
                            asked = true;
                            cancel.cancel();
                        }
                    }
                };
                result_of(done)
            })
    }
}

fn say(ctx: &JobCtx, id: &str, received: u64, total: u64) {
    if let Ok(payload) = serde_json::to_value(SpeakersProgress {
        id: id.to_string(),
        received,
        total,
    }) {
        ctx.progress(payload);
    }
}

fn stopped_unexpectedly() -> Failure {
    Failure::new("the work stopped unexpectedly", Reason::Internal)
}

/// A blocking op's outcome as the job's result.
fn result_of(
    done: Result<Result<svolder_ipc::SpeakersSaved, Failure>, tokio::task::JoinError>,
) -> Result<serde_json::Value, JobError> {
    let saved = match done {
        Ok(Ok(saved)) => saved,
        Ok(Err(failure)) => return Err(failure.job()),
        Err(_) => return Err(stopped_unexpectedly().job()),
    };
    serde_json::to_value(saved)
        .map_err(|error| JobError::failed(error.to_string(), Reason::Internal))
}

#[cfg(test)]
mod tests;
