// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The work the BlueZ transport keeps on the radio for callers that may stop waiting for it — a
//! connect opening a link, and the release that takes a link back down — recorded per device,
//! so a caller about to stop driving its runtime waits for its own device's work, and never
//! for, or instead of, another device's.

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use futures::FutureExt;
use futures::future::BoxFuture;
use svolder_proto::UnitId;
use tokio::sync::{oneshot, watch};
use tokio::time::error::Elapsed;

use crate::error::BleError;

/// The tasks recorded per device until they end. Clones share one record.
#[derive(Clone, Default)]
pub(crate) struct Releases {
    running: Arc<Mutex<Running>>,
}

/// Every task still running, and the number the next one gets.
#[derive(Default)]
struct Running {
    next: u64,
    tasks: Vec<Task>,
}

/// One recorded task: the device it works for, and a receiver whose channel closes as it ends.
struct Task {
    id: u64,
    unit: UnitId,
    ended: watch::Receiver<()>,
}

/// Carried by a recorded task. Dropped as the task ends — finished, panicked or aborted — it
/// takes the task off the record and closes the channel every wait for it watches.
struct Ended {
    running: Arc<Mutex<Running>>,
    id: u64,
    _closes: watch::Sender<()>,
}

impl Drop for Ended {
    fn drop(&mut self) {
        locked(&self.running)
            .tasks
            .retain(|task| task.id != self.id);
    }
}

/// The record, locked whatever a holder that panicked left behind: a drop that runs while a
/// panic unwinds must not panic again.
fn locked(running: &Mutex<Running>) -> MutexGuard<'_, Running> {
    running.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Releases {
    /// Run `work` for `unit` on a task of its own, on the record until it ends. Outside a Tokio
    /// runtime, or on one that is shutting down, there is nowhere to run it, and it is dropped
    /// unrun.
    pub(crate) fn spawn(&self, unit: UnitId, work: impl Future<Output = ()> + Send + 'static) {
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let (closes, ended) = watch::channel(());
        let id = {
            let mut running = locked(&self.running);
            let id = running.next;
            running.next += 1;
            running.tasks.push(Task { id, unit, ended });
            id
        };
        let carried = Ended {
            running: Arc::clone(&self.running),
            id,
            _closes: closes,
        };
        runtime.spawn(async move {
            let _ended = carried;
            work.await;
        });
    }

    /// Wait, at most `bound`, for the recorded work of `unit` — of every device for `None` —
    /// and for any more such work that starts while this waits. `None` when there was none to
    /// wait for, else whether it all ended inside the bound.
    ///
    /// Waiting takes nothing off the record: work still running when the bound passes, or when
    /// the caller drops this wait, is there for the next wait, and two waits at once both wait
    /// for it.
    pub(crate) async fn settle(
        &self,
        unit: Option<UnitId>,
        bound: Duration,
    ) -> Option<Result<(), Elapsed>> {
        if self.running_for(unit).is_empty() {
            return None;
        }
        let all = async {
            loop {
                let waiting = self.running_for(unit);
                if waiting.is_empty() {
                    return;
                }
                for mut ended in waiting {
                    // The channel is never sent on: it only closes, as its task ends.
                    let _ = ended.changed().await;
                }
            }
        };
        Some(tokio::time::timeout(bound, all).await)
    }

    /// A receiver for each recorded task of `unit`, or of every device for `None`, that has
    /// not ended: a task whose channel has closed is over whether or not its record has gone
    /// yet, so a wait never loops on it.
    fn running_for(&self, unit: Option<UnitId>) -> Vec<watch::Receiver<()>> {
        locked(&self.running)
            .tasks
            .iter()
            .filter(|task| task.ended.has_changed().is_ok())
            .filter(|task| unit.is_none_or(|unit| task.unit == unit))
            .map(|task| task.ended.clone())
            .collect()
    }

    /// How many tasks are on the record.
    #[cfg(test)]
    fn len(&self) -> usize {
        locked(&self.running).tasks.len()
    }
}

/// A connection the transport can ask the adapter to take down.
pub(crate) trait Peer: Clone + Send + Sync + 'static {
    /// Ask for the connection to go, bounded like every other radio operation.
    fn take_down(&self) -> BoxFuture<'static, Result<(), BleError>>;
}

/// One device's connection, held by a link: taken down by [`Hold::release`], and, when no
/// release of it succeeded, by the drop, on a task [`Releases`] records for the device.
pub(crate) struct Hold<P: Peer> {
    peer: P,
    unit: UnitId,
    releases: Releases,
    /// Whether a release has succeeded, which leaves the drop nothing to ask.
    released: AtomicBool,
}

impl<P: Peer> Hold<P> {
    /// The connection to `peer`, the device `unit`, whose drop records its ask in `releases`.
    pub(crate) fn new(peer: P, unit: UnitId, releases: Releases) -> Self {
        Self {
            peer,
            unit,
            releases,
            released: AtomicBool::new(false),
        }
    }

    /// The peer the connection is to.
    pub(crate) fn peer(&self) -> &P {
        &self.peer
    }

    /// Ask for the connection to go now. Only an ask that succeeded spares the drop its own:
    /// one that failed or timed out is asked again when the link is dropped.
    pub(crate) async fn release(&self) -> Result<(), BleError> {
        let asked = self.peer.take_down().await;
        if asked.is_ok() {
            self.released.store(true, Ordering::SeqCst);
        }
        asked
    }
}

impl<P: Peer> Drop for Hold<P> {
    /// A destructor cannot wait, so the ask goes to a task the transport records for the
    /// device, which a settle for that device waits for. This covers a link no caller took and
    /// a session dropped with its link still in it. Best effort: with no runtime to run it on,
    /// or one shutting down, the ask is dropped unrun.
    fn drop(&mut self) {
        if self.released.load(Ordering::SeqCst) {
            return;
        }
        let peer = self.peer.clone();
        self.releases.spawn(self.unit, async move {
            let _ = peer.take_down().await;
        });
    }
}

/// Run `work` for `unit` on a task [`Releases`] records, and hand its result to the caller over
/// a channel, so a caller that stops awaiting cannot leave the work half done. `release` runs
/// on the same task, once, whenever what the work builds has nobody to go to:
///
/// - the caller left before the work was done: the work is stopped where it stands, and
///   `release(None)` takes down whatever it had half built;
/// - the work failed or panicked: `release(None)`, after the caller is told;
/// - the work succeeded after the caller left: `release(Some(value))`, so the value is taken
///   down through itself and nothing asks twice.
///
/// The task is on the record before the caller starts waiting, so a settle made the instant a
/// caller gives up waits for it.
pub(crate) async fn deliver_or_release<T, W, R, F>(
    unit: UnitId,
    work: W,
    release: R,
    releases: &Releases,
) -> Result<T, BleError>
where
    T: Send + 'static,
    W: Future<Output = Result<T, BleError>> + Send + 'static,
    R: FnOnce(Option<T>) -> F + Send + 'static,
    F: Future<Output = ()> + Send + 'static,
{
    let (tx, rx) = oneshot::channel();
    releases.spawn(unit, async move {
        let mut tx = tx;
        let work = AssertUnwindSafe(work).catch_unwind();
        tokio::pin!(work);
        let done = tokio::select! {
            biased;
            done = &mut work => Some(done),
            () = tx.closed() => None,
        };
        match done {
            None => release(None).await,
            Some(Err(_panicked)) => {
                drop(tx);
                release(None).await;
            }
            Some(Ok(Err(e))) => {
                let _ = tx.send(Err(e));
                release(None).await;
            }
            Some(Ok(Ok(value))) => {
                if let Err(Ok(value)) = tx.send(Ok(value)) {
                    release(Some(value)).await;
                }
            }
        }
    });
    match rx.await {
        Ok(result) => result,
        Err(_) => Err(BleError::Connection(
            "the connection task ended without a result".into(),
        )),
    }
}

#[cfg(test)]
mod tests;
