// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the AutoEq ops share: the index, loaded once at a time and kept for the run; the
//! downloaded corrections, kept by measurement; and the fits, a couple at a time.

use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use svolder_autoeq::{AutoEqError, Client, Entry, Index, Refresh, missing_file_words};
use svolder_ipc::{AutoEqVariant, CODE_USAGE, Reason, Response};
use svolder_profiles::EqSpec;
use svolder_profiles::tone::apo::{self, Correction, Parsed};
use svolder_profiles::tone::fit::Preview;
use svolder_profiles::tone::preset::{Imported, Preset, host_correction};
use svolder_profiles::tone::response::log_freqs;
use tokio::sync::{Semaphore, watch};

use super::search;
use crate::locks::locked;

/// How many downloaded corrections are kept for the run, so a preview and the import after it
/// fetch the file once. A file is at most 64 KiB; when the list is full the oldest goes.
const FILES_KEPT: usize = 32;

/// How many fits run at once: a fit is a second or so of one core, and a page asks for one per
/// preview.
const FITS_AT_ONCE: usize = 2;

/// How many points a preview has: the web's curve grid.
pub(super) const PREVIEW_POINTS: usize = crate::web::view::POINTS;

/// Why an op did not finish: the daemon's words and the reason a front end matches on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Failure {
    pub(super) message: String,
    pub(super) reason: Reason,
}

impl Failure {
    pub(super) fn new(message: impl Into<String>, reason: Reason) -> Failure {
        Failure {
            message: message.into(),
            reason,
        }
    }

    /// The answer to a request that failed this way.
    pub(super) fn answer(&self) -> Response {
        Response::error(CODE_USAGE, self.message.clone()).with_reason(self.reason)
    }

    /// The end of a job that failed this way.
    pub(super) fn job(self) -> super::super::jobs::JobError {
        super::super::jobs::JobError::failed(self.message, self.reason)
    }

    /// A failure of the network side: a 404 is a measurement that is listed and not there.
    fn of_fetch(error: &AutoEqError, subject: &str) -> Failure {
        match error.is_missing_file() {
            true => Failure::new(missing_file_words(subject), Reason::NotListed),
            false => Failure::new(error.to_string(), Reason::Unreachable),
        }
    }
}

/// The side a preview is made for.
#[derive(Clone)]
pub(super) enum Side {
    /// A device's tone control.
    Device(EqSpec),
    /// The computer.
    Computer,
}

/// A load under way, as its waiters see it: empty until it ends, then its outcome.
type Loading = watch::Receiver<Option<Result<Arc<Index>, Failure>>>;

/// What the index loader keeps between requests.
#[derive(Default)]
struct Held {
    /// The index as last loaded: a fresh copy, or a stale one a failed refresh fell back to.
    copy: Option<Arc<Index>>,
    /// When the last load ended, whatever its outcome.
    tried: Option<SystemTime>,
    /// How the last load failed, while there is no copy to show for it.
    failed: Option<Failure>,
    /// The load under way: waiters subscribe to it rather than start another.
    loading: Option<Loading>,
}

/// How long a stale copy that a refresh just failed to replace is served as it is before the
/// next request tries again: a network that is down answers every key of a search box otherwise.
const STALE_BACKOFF: Duration = Duration::from_secs(10 * 60);

/// How long a failed load with no copy to fall back on is answered with its failure before the
/// next request tries again.
const FAILURE_BACKOFF: Duration = Duration::from_secs(30);

/// The shared state of the AutoEq ops. Without a client (a daemon that was not given one) every
/// op that needs the repository answers that AutoEq is not set up.
pub(super) struct Core {
    client: Option<Client>,
    index: Arc<Mutex<Held>>,
    files: Mutex<Vec<(String, Arc<Parsed>)>>,
    fits: Semaphore,
}

impl Core {
    pub(super) fn new(client: Option<Client>) -> Core {
        Core {
            client,
            index: Arc::new(Mutex::new(Held::default())),
            files: Mutex::new(Vec::new()),
            fits: Semaphore::new(FITS_AT_ONCE),
        }
    }

    fn client(&self) -> Result<Client, Failure> {
        self.client.clone().ok_or_else(|| {
            Failure::new(
                "this daemon was not set up to reach AutoEq",
                Reason::Internal,
            )
        })
    }

    /// The index, as `policy` says. One load runs at a time, as a task of its own in a blocking
    /// thread; every request that arrives while it runs waits for that load and shares its
    /// outcome, and a waiter that goes away neither stops it nor leaves a second one to start. A
    /// copy the policy still takes is returned as it is. After a failed refresh the stale copy is
    /// served for [`STALE_BACKOFF`] (with no copy, the failure for [`FAILURE_BACKOFF`]); only a
    /// request for the index `Now` goes round them.
    pub(super) async fn index(&self, policy: Refresh) -> Result<Arc<Index>, Failure> {
        let client = self.client()?;
        let mut waiting = {
            let mut held = locked(&self.index);
            let now = SystemTime::now();
            let since = |at: Option<SystemTime>, bound: Duration| {
                at.is_some_and(|at| now.duration_since(at).is_ok_and(|age| age < bound))
            };
            if let Some(copy) = &held.copy {
                let backed_off = copy.note.is_some()
                    && policy != Refresh::Now
                    && since(held.tried, STALE_BACKOFF);
                if backed_off || !policy.is_stale(copy.fetched, now) {
                    return Ok(Arc::clone(copy));
                }
            } else if let Some(failure) = &held.failed
                && policy != Refresh::Now
                && since(held.tried, FAILURE_BACKOFF)
            {
                return Err(failure.clone());
            }
            match &held.loading {
                Some(under_way) => under_way.clone(),
                None => {
                    let (done, waiting) = watch::channel(None);
                    held.loading = Some(waiting.clone());
                    let state = Arc::clone(&self.index);
                    tokio::spawn(async move {
                        let loaded =
                            tokio::task::spawn_blocking(move || client.index(policy)).await;
                        let outcome = match loaded {
                            Ok(Ok(index)) => Ok(Arc::new(index)),
                            // A copy already held is served, with a note, rather than the
                            // failure: the back-off then runs as it does for a noted copy.
                            Ok(Err(error)) => match locked(&state).copy.clone() {
                                Some(old) => Ok(Arc::new(Index {
                                    note: Some(format!(
                                        "using the index already loaded (refresh failed: {error})"
                                    )),
                                    ..(*old).clone()
                                })),
                                None => Err(Failure::new(error.to_string(), Reason::Unreachable)),
                            },
                            Err(_) => Err(Failure::new(
                                "the index load stopped unexpectedly",
                                Reason::Internal,
                            )),
                        };
                        {
                            let mut held = locked(&state);
                            held.loading = None;
                            held.tried = Some(SystemTime::now());
                            match &outcome {
                                Ok(index) => {
                                    held.copy = Some(Arc::clone(index));
                                    held.failed = None;
                                }
                                Err(failure) => held.failed = Some(failure.clone()),
                            }
                        }
                        let _ = done.send(Some(outcome));
                    });
                    waiting
                }
            }
        };
        let ended = waiting
            .wait_for(Option::is_some)
            .await
            .map_err(|_| Failure::new("the index load stopped unexpectedly", Reason::Internal))?;
        ended.clone().unwrap_or_else(|| {
            Err(Failure::new(
                "the index load stopped unexpectedly",
                Reason::Internal,
            ))
        })
    }

    /// The entry of the index that `variant` names: only a listed measurement is ever fetched.
    pub(super) async fn entry(
        &self,
        variant: &AutoEqVariant,
        policy: Refresh,
    ) -> Result<Entry, Failure> {
        let index = self.index(policy).await?;
        index
            .entries
            .iter()
            .find(|entry| search::is(entry, variant))
            .cloned()
            .ok_or_else(|| {
                Failure::new(
                    format!("{} is not in the AutoEq index", variant.name),
                    Reason::NotListed,
                )
            })
    }

    /// The correction `entry`'s file holds: the copy kept, or the file downloaded in a blocking
    /// thread (at most 20 s) and read.
    pub(super) async fn correction(&self, entry: &Entry) -> Result<Arc<Parsed>, Failure> {
        let key = format!(
            "{}\u{0}{}\u{0}{}\u{0}{:?}",
            entry.name, entry.source, entry.form, entry.rig
        );
        if let Some((_, kept)) = locked(&self.files).iter().find(|(k, _)| *k == key) {
            return Ok(Arc::clone(kept));
        }
        let client = self.client()?;
        let wanted = entry.clone();
        let text = tokio::task::spawn_blocking(move || client.download(&wanted))
            .await
            .map_err(|_| Failure::new("the download stopped unexpectedly", Reason::Internal))?
            .map_err(|error| Failure::of_fetch(&error, &entry.name))?;
        let parsed = apo::parse(&text).map_err(|error| {
            Failure::new(
                format!(
                    "the file for {} is not a parametric EQ: {error}",
                    entry.name
                ),
                Reason::Invalid,
            )
        })?;
        let parsed = Arc::new(parsed);
        let mut files = locked(&self.files);
        if files.len() >= FILES_KEPT {
            files.remove(0);
        }
        files.push((key, Arc::clone(&parsed)));
        Ok(parsed)
    }

    /// The preview of `correction` for `side` on the web's grid. A device's is fitted, in a
    /// blocking thread behind the fit semaphore; the computer's plays the correction as it is,
    /// and is refused when it has more filters than the computer EQ holds.
    pub(super) async fn preview(
        &self,
        side: Side,
        correction: Correction,
    ) -> Result<Preview, Failure> {
        let grid = log_freqs(PREVIEW_POINTS);
        match side {
            Side::Computer => {
                let preset = Preset::imported(
                    "preview",
                    Imported {
                        origin: None,
                        correction,
                    },
                );
                let played =
                    host_correction(&preset).map_err(|why| Failure::new(why, Reason::Invalid))?;
                Ok(Preview::computer(&played, &grid))
            }
            Side::Device(eq) => {
                let _permit = self
                    .fits
                    .acquire()
                    .await
                    .map_err(|_| Failure::new("the daemon is stopping", Reason::Stopping))?;
                tokio::task::spawn_blocking(move || Preview::device(&correction, &eq, &grid))
                    .await
                    .map_err(|_| Failure::new("the fit stopped unexpectedly", Reason::Internal))
            }
        }
    }
}

#[cfg(test)]
mod tests;
