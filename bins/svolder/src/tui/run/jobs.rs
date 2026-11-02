// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The slow work the preset list asks of the loop: fetching the AutoEq index, downloading one
//! file, and putting the computer EQ on or taking it off.
//!
//! Each runs on a task of its own, so no frame and no key waits on a network or a service
//! manager; what it ends with arrives on a channel the loop reads, as a
//! [`Done`]. What the jobs reach is behind [`Hands`], so a test puts a
//! fake where the network and PipeWire would be.

use std::sync::Arc;
use std::time::SystemTime;

use futures::future::BoxFuture;
use svolder_autoeq::{AutoEqError, Client, Entry, Index, Refresh};
use svolder_host::eq::HostEqState;
use svolder_ipc::config::{RefreshWeeks, cache_home};
use tokio::sync::mpsc;

use crate::commands::host_eq::{self, ApplyRequest};
use crate::recovery::Failure;
use crate::tui::boxes::presets::Done;

/// What the jobs reach: AutoEq's repository and this machine's computer EQ.
pub(in crate::tui) trait Hands: Send + Sync {
    /// The index, fetched or read from the cache as `refresh` says. Blocks.
    fn index(&self, refresh: Refresh) -> Result<Index, String>;
    /// One measurement's parametric EQ text. Blocks.
    fn download(&self, entry: &Entry) -> Result<String, String>;
    /// Put `request` in front of the sink.
    fn apply(&self, request: ApplyRequest) -> BoxFuture<'static, Result<HostEqState, Failure>>;
    /// Take the computer EQ off.
    fn remove(&self) -> BoxFuture<'static, Result<(), Failure>>;
    /// The computer EQ's state file, `None` when there is none.
    fn state(&self) -> Option<HostEqState>;
    /// When the state file last changed, `None` when there is none. A fake with no file keeps
    /// the answer it has.
    fn stamp(&self) -> Option<SystemTime> {
        None
    }
    /// Why the computer EQ cannot run here, when it cannot.
    fn available(&self) -> Result<(), Failure>;
}

/// The network and this machine.
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

    fn apply(&self, request: ApplyRequest) -> BoxFuture<'static, Result<HostEqState, Failure>> {
        Box::pin(host_eq::apply(request))
    }

    fn remove(&self) -> BoxFuture<'static, Result<(), Failure>> {
        Box::pin(host_eq::remove())
    }

    fn state(&self) -> Option<HostEqState> {
        host_eq::state()
    }

    fn stamp(&self) -> Option<SystemTime> {
        host_eq::state_stamp()
    }

    fn available(&self) -> Result<(), Failure> {
        host_eq::available()
    }
}

/// Nothing: what a loop that reaches neither the network nor the host is left with, which says
/// so rather than ever reaching for either.
struct Idle;

/// What [`Idle`] says to every job.
const NOTHING: &str = "this session reaches neither the network nor the host";

impl Hands for Idle {
    fn index(&self, _: Refresh) -> Result<Index, String> {
        Err(NOTHING.to_string())
    }

    fn download(&self, _: &Entry) -> Result<String, String> {
        Err(NOTHING.to_string())
    }

    fn apply(&self, _: ApplyRequest) -> BoxFuture<'static, Result<HostEqState, Failure>> {
        Box::pin(async {
            Err(Failure::of(
                crate::recovery::Kind::Usage,
                NOTHING.to_string(),
            ))
        })
    }

    fn remove(&self) -> BoxFuture<'static, Result<(), Failure>> {
        Box::pin(async {
            Err(Failure::of(
                crate::recovery::Kind::Usage,
                NOTHING.to_string(),
            ))
        })
    }

    fn state(&self) -> Option<HostEqState> {
        None
    }

    fn available(&self) -> Result<(), Failure> {
        Err(Failure::of(
            crate::recovery::Kind::Usage,
            NOTHING.to_string(),
        ))
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
    /// When the state file last changed as [`Jobs::host_eq_changed`] last saw it; `None` before
    /// the first look.
    seen: Option<Option<SystemTime>>,
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
        Jobs {
            hands,
            done,
            ended,
            seen: None,
        }
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

    /// Put `request` on the computer.
    pub(super) fn apply(&self, request: ApplyRequest) {
        let (hands, done) = (self.hands.clone(), self.done.clone());
        let name = request.name.clone();
        let stated = request.correction.preamp_db;
        tokio::spawn(async move {
            let result = hands.apply(request).await;
            let _ = done.send(Done::Applied(name, stated, result));
        });
    }

    /// Take the computer EQ off, for the preset `then_load` to be loaded after.
    pub(super) fn remove(&self, then_load: Option<String>) {
        let (hands, done) = (self.hands.clone(), self.done.clone());
        tokio::spawn(async move {
            let result = hands.remove().await;
            let _ = done.send(Done::Removed { then_load, result });
        });
    }

    /// Forget what the state file looked like, so the next look reads it again: for an app that
    /// has not read it yet.
    pub(super) fn look_again(&mut self) {
        self.seen = None;
    }

    /// The computer EQ's saved state, when the file changed since the last look, or this is the
    /// first: `Some(None)` for a file that is gone. One stat of the file otherwise.
    pub(super) fn host_eq_changed(&mut self) -> Option<Option<HostEqState>> {
        let stamp = self.hands.stamp();
        if self.seen == Some(stamp) {
            return None;
        }
        self.seen = Some(stamp);
        Some(self.hands.state())
    }

    /// The preset on the computer, and why the computer EQ cannot run here: what the preset list
    /// reads each time it opens.
    pub(super) fn computer(&self) -> (Option<String>, Option<String>) {
        let on = self
            .hands
            .state()
            .filter(|state| state.on)
            .map(|state| state.name);
        (on, self.hands.available().err().map(|failure| failure.what))
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
