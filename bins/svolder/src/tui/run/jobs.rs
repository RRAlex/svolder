// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The slow work the preset list asks of the loop: fetching the AutoEq index and downloading one
//! file.
//!
//! Each runs on a task of its own, so no frame and no key waits on a network or a service
//! manager; what it ends with arrives on a channel the loop reads, as a
//! [`Done`]. What the jobs reach is behind [`Hands`], so a test puts a
//! fake where the network and PipeWire would be.

use std::sync::Arc;

use svolder_autoeq::{AutoEqError, Client, Entry, Index, Refresh};
use svolder_ipc::config::{RefreshWeeks, cache_home};
use tokio::sync::mpsc;

use crate::tui::boxes::presets::Done;

/// What the jobs reach: AutoEq's repository.
pub(in crate::tui) trait Hands: Send + Sync {
    /// The index, fetched or read from the cache as `refresh` says. Blocks.
    fn index(&self, refresh: Refresh) -> Result<Index, String>;
    /// One measurement's parametric EQ text. Blocks.
    fn download(&self, entry: &Entry) -> Result<String, String>;
}

/// The network.
struct Live;

/// The client the TUI uses: `SVOLDER_AUTOEQ_URL` or AutoEq's repository, cached where the CLI
/// caches it.
fn client() -> Client {
    Client::from_env(cache_home().join("svolder").join("autoeq"))
}

impl Hands for Live {
    fn index(&self, refresh: Refresh) -> Result<Index, String> {
        client().index(refresh).map_err(|e| e.to_string())
    }

    fn download(&self, entry: &Entry) -> Result<String, String> {
        client().download(entry).map_err(|e| match e {
            AutoEqError::Status { status: 404, .. } => format!(
                "{} is listed but its file is not there: pick another measurement",
                entry.name
            ),
            other => other.to_string(),
        })
    }
}

/// Nothing: what a loop that reaches no network is left with, which says so rather than ever
/// reaching for one.
struct Idle;

/// What [`Idle`] says to every job.
const NOTHING: &str = "this session reaches no network";

impl Hands for Idle {
    fn index(&self, _: Refresh) -> Result<Index, String> {
        Err(NOTHING.to_string())
    }

    fn download(&self, _: &Entry) -> Result<String, String> {
        Err(NOTHING.to_string())
    }
}

/// The jobs in flight and the channel their ends come back on.
pub(in crate::tui) struct Jobs {
    /// What the jobs reach.
    hands: Arc<dyn Hands>,
    /// Where a job sends what it ended with.
    done: mpsc::UnboundedSender<Done>,
    /// What the jobs ended with, in the order they ended.
    pub(super) ended: mpsc::UnboundedReceiver<Done>,
}

impl Default for Jobs {
    /// Jobs that reach nothing.
    fn default() -> Jobs {
        Jobs::over(Arc::new(Idle))
    }
}

/// When the index is fetched again: at once for `refresh`, else as `weeks` say.
pub(super) fn policy(refresh: bool, weeks: RefreshWeeks) -> Refresh {
    match (refresh, weeks) {
        (true, _) => Refresh::Now,
        (false, RefreshWeeks::Never) => Refresh::Never,
        (false, RefreshWeeks::Weeks(weeks)) => Refresh::AfterWeeks(weeks),
    }
}

impl Jobs {
    /// Jobs that reach AutoEq's repository and this machine.
    pub(in crate::tui) fn live() -> Jobs {
        Jobs::over(Arc::new(Live))
    }

    /// Jobs that reach `hands`.
    pub(in crate::tui) fn over(hands: Arc<dyn Hands>) -> Jobs {
        let (done, ended) = mpsc::unbounded_channel();
        Jobs { hands, done, ended }
    }

    /// Fetch the index.
    pub(super) fn index(&self, refresh: Refresh) {
        let (hands, done) = (self.hands.clone(), self.done.clone());
        tokio::spawn(async move {
            let result = tokio::task::spawn_blocking(move || hands.index(refresh))
                .await
                .unwrap_or_else(|e| Err(format!("the fetch stopped: {e}")));
            let _ = done.send(Done::Index(result));
        });
    }

    /// Download one measurement's file.
    pub(super) fn download(&self, entry: Entry) {
        let (hands, done) = (self.hands.clone(), self.done.clone());
        tokio::spawn(async move {
            let wanted = entry.clone();
            let result = tokio::task::spawn_blocking(move || hands.download(&wanted))
                .await
                .unwrap_or_else(|e| Err(format!("the download stopped: {e}")));
            let _ = done.send(Done::Download(entry, result));
        });
    }
}

/// What the jobs ended with next; never, while none is in flight.
pub(super) async fn next(ended: &mut mpsc::UnboundedReceiver<Done>) -> Done {
    match ended.recv().await {
        Some(done) => done,
        None => std::future::pending().await,
    }
}

#[cfg(test)]
mod tests;
