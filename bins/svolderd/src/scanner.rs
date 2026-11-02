// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The adapter's scans, one at a time and shared. A device actor that looks for its device is
//! heard by the listen running when it asks, or by the next one: a listen hears every device
//! looked for while it runs, answers each the moment it is heard, and stops once nobody is left
//! to listen for. The `scan` verb and a listen after a refusal take their turns on the same
//! radio.

use std::panic::AssertUnwindSafe;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::FutureExt;
use svolder_ble::{Advertisement, Advertisements, BleError, Listening, Transport};
use svolder_proto::{Passphrase, UnitId};
use tokio::sync::{Notify, oneshot};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::locks::locked;

/// How long a listen goes on for a device it has not heard.
pub const SCAN_WINDOW: Duration = Duration::from_secs(5);

/// How soon after a listen begins an actor can ask and still count as having had all of it.
/// One that asks later and is not heard is answered at the end of the next listen, which it
/// then has whole.
const JOIN_GRACE: Duration = Duration::from_millis(250);

/// What an actor is told of a listen: its device's first advertisement, `None` when a listen it
/// had whole heard nothing of it, or why the listen failed.
pub type Heard = Result<Option<Advertisement>, String>;

/// One adapter's scans, shared by every device actor and the `scan` verb.
pub struct Scanner {
    transport: Arc<dyn Transport>,
    /// Held for the whole of one scan: a shared listen, a listen after a refusal, or the
    /// `scan` verb's.
    radio: Arc<tokio::sync::Mutex<()>>,
    /// The actors waiting, shared with the task that listens for them.
    shared: Arc<Shared>,
    /// The task that runs the shared listens, stopped with the scanner.
    task: JoinHandle<()>,
}

/// What the listening task and the actors share.
struct Shared {
    waiting: Mutex<Waiting>,
    /// Raised by every actor that starts waiting, so the task wakes for it.
    wake: Notify,
}

/// The actors waiting for their device, and the answers a listen holds back.
#[derive(Default)]
struct Waiting {
    list: Vec<Waiter>,
    /// Answers kept until the listen that heard them has stopped: see [`Round::heard`].
    held_back: Vec<(oneshot::Sender<Heard>, Advertisement)>,
    /// Set once the listening task has ended: nobody is listened for any more.
    closed: bool,
    /// Set once the running listen has said it is done: an actor that asks from here on has
    /// not had this listen and waits for the next.
    ending: bool,
}

/// One actor waiting for its device.
struct Waiter {
    unit: UnitId,
    /// When it asked.
    since: Instant,
    /// Whether it asked after the listen running then had said it was done.
    late: bool,
    answer: oneshot::Sender<Heard>,
}

impl Scanner {
    /// A scanner over `transport`, its listening task started on the current runtime.
    pub fn new(transport: Arc<dyn Transport>) -> Scanner {
        let radio = Arc::new(tokio::sync::Mutex::new(()));
        let shared = Arc::new(Shared {
            waiting: Mutex::new(Waiting::default()),
            wake: Notify::new(),
        });
        let task = tokio::spawn(listen_for_waiters(
            Arc::clone(&shared),
            Arc::clone(&transport),
            Arc::clone(&radio),
        ));
        Scanner {
            transport,
            radio,
            shared,
            task,
        }
    }

    /// The first advertisement `unit` sends while a listen runs for it: the listen running now,
    /// which hears it from this moment on, or the next one. `None` once a listen it had whole
    /// heard nothing of it.
    ///
    /// Cancel-safe: a caller that stops waiting is listened for no longer, and a listen left
    /// with nobody to listen for stops.
    pub async fn sight(&self, unit: UnitId) -> Heard {
        let (answer, heard) = oneshot::channel();
        {
            let mut waiting = locked(&self.shared.waiting);
            if waiting.closed {
                return Err("the scanner has stopped".to_string());
            }
            let late = waiting.ending;
            waiting.list.push(Waiter {
                unit,
                since: Instant::now(),
                late,
                answer,
            });
        }
        self.shared.wake.notify_one();
        heard
            .await
            .unwrap_or_else(|_| Err("the scanner stopped before it answered".to_string()))
    }

    /// [`svolder_ble::listen_afresh`] for `unit` after a refusal of `refused`, on the radio
    /// alone: what it hears has to be the device's own word on its code, so it shares nothing
    /// with the listens other actors wait on.
    pub async fn listen_afresh(&self, unit: UnitId, refused: Passphrase) -> Heard {
        let _radio = self.radio.lock().await;
        svolder_ble::listen_afresh(self.transport.as_ref(), unit, refused, SCAN_WINDOW)
            .await
            .map_err(|e| e.to_string())
    }

    /// Every device heard within `window`, for the `scan` verb, in its turn on the radio.
    pub async fn scan(&self, window: Duration) -> Result<Advertisements, BleError> {
        let _radio = self.radio.lock().await;
        self.transport.scan(window).await
    }
}

impl Drop for Scanner {
    fn drop(&mut self) {
        self.task.abort();
    }
}

/// The listening task: while anyone is waiting, one listen after another, each on the radio
/// alone.
async fn listen_for_waiters(
    shared: Arc<Shared>,
    transport: Arc<dyn Transport>,
    radio: Arc<tokio::sync::Mutex<()>>,
) {
    let _closing = Closing(Arc::clone(&shared));
    loop {
        if !anyone_waiting(&shared) {
            // An actor that asked since the check left a permit, so this returns at once.
            shared.wake.notified().await;
            continue;
        }
        let _radio = radio.lock().await;
        let round = Round {
            shared: &shared,
            started: Instant::now(),
        };
        // A listen that panics costs this round, not discovery for every device after it.
        let outcome = AssertUnwindSafe(transport.listen(SCAN_WINDOW, &round))
            .catch_unwind()
            .await
            .unwrap_or_else(|_| {
                Err(BleError::Connection(
                    "the scan stopped on an internal error".to_string(),
                ))
            });
        round.finish(outcome);
    }
}

/// Whether any actor is still waiting, forgetting the ones that stopped.
fn anyone_waiting(shared: &Shared) -> bool {
    let mut waiting = locked(&shared.waiting);
    waiting.list.retain(|waiter| !waiter.answer.is_closed());
    !waiting.list.is_empty()
}

/// Marks the scanner closed as the listening task ends, however it ends, and lets go of every
/// answer still owed, so no actor waits on a task that is gone.
struct Closing(Arc<Shared>);

impl Drop for Closing {
    fn drop(&mut self) {
        let mut waiting = locked(&self.0.waiting);
        waiting.closed = true;
        waiting.list.clear();
        waiting.held_back.clear();
    }
}

/// One listen, and who it is for.
struct Round<'a> {
    shared: &'a Shared,
    started: Instant,
}

impl Listening for Round<'_> {
    fn wants(&self, unit: UnitId) -> bool {
        locked(&self.shared.waiting)
            .list
            .iter()
            .any(|waiter| waiter.unit == unit && !waiter.answer.is_closed())
    }

    /// Answer every actor waiting for this device. The answer that leaves nobody waiting is
    /// held back until the listen has stopped, so the connect it lets go of runs on an adapter
    /// that is no longer scanning; the others go at once.
    ///
    /// An early answer's connect may pause or end the adapter's LE scan, so a device still
    /// wanted may hear nothing more from this window: its actor then gets `None` and falls back
    /// to the advertisement the adapter holds for it.
    fn heard(&self, advert: &Advertisement) {
        let mut waiting = locked(&self.shared.waiting);
        let unit = advert.unit_id();
        let (theirs, rest): (Vec<Waiter>, Vec<Waiter>) = std::mem::take(&mut waiting.list)
            .into_iter()
            .partition(|waiter| waiter.unit == unit);
        waiting.list = rest;
        let last = waiting.list.iter().all(|waiter| waiter.answer.is_closed());
        for waiter in theirs {
            match last {
                true => waiting.held_back.push((waiter.answer, advert.clone())),
                false => {
                    let _ = waiter.answer.send(Ok(Some(advert.clone())));
                }
            }
        }
    }

    /// Whether nobody is left to listen for, forgetting callers that stopped waiting. An actor
    /// that asked after an answer was held back keeps the listen going, so that answer goes
    /// out now rather than at its end.
    fn done(&self) -> bool {
        let mut waiting = locked(&self.shared.waiting);
        waiting.list.retain(|waiter| !waiter.answer.is_closed());
        if waiting.list.is_empty() {
            waiting.ending = true;
            return true;
        }
        for (answer, advert) in waiting.held_back.drain(..) {
            let _ = answer.send(Ok(Some(advert)));
        }
        false
    }
}

impl Round<'_> {
    /// Answer whoever this listen was for, once it has stopped: what it held back, then `None`,
    /// or why it failed, to every actor waiting by [`JOIN_GRACE`] after it began that it did
    /// not hear. An actor that asked later, or after the listen had said it was done, has not
    /// had the whole listen and waits for the next.
    fn finish(self, outcome: Result<(), BleError>) {
        let mut waiting = locked(&self.shared.waiting);
        waiting.ending = false;
        for (answer, advert) in waiting.held_back.drain(..) {
            let _ = answer.send(Ok(Some(advert)));
        }
        let failure = outcome.err().map(|e| e.to_string());
        let joined_by = self.started + JOIN_GRACE;
        let (had_it, later): (Vec<Waiter>, Vec<Waiter>) = std::mem::take(&mut waiting.list)
            .into_iter()
            .partition(|waiter| !waiter.late && waiter.since <= joined_by);
        waiting.list = later;
        for waiter in &mut waiting.list {
            waiter.late = false;
        }
        for waiter in had_it {
            let _ = waiter.answer.send(match &failure {
                Some(message) => Err(message.clone()),
                None => Ok(None),
            });
        }
    }
}

#[cfg(test)]
mod tests;
