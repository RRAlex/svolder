// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A chain that dies is seen and started again once.
//!
//! One task waits on [`Chain::exited`] for as long as the controller lives and does nothing but
//! raise a flag; the controller's next tick puts the chain right, and only then is the wait
//! begun again, so a chain that is still gone is not waited on in a loop of the task's own. The
//! task is started once the chain has been seen to run (applied, or found in the graph): a saved
//! state that says on with no chain is the owner's start-up business, not a death.
//!
//! The first death restarts the chain, with the crossfeed off and its "chain restarted" note as
//! for any restart; one that runs for crossfeed alone is not started again, since the crossfeed
//! is off. A restart that fails (PipeWire is not back yet) is tried again after 1, 2, 4 and then
//! 8 seconds, and the view says so. A chain that dies within [`DEATH_WINDOW`] ticks of a
//! restart that went through, twice in a row, is a crash loop: the EQ is left off with a note
//! saying the chain stopped. A death that another svolder's change explains (the state file moved, a chain runs
//! again) or that an apply or a turn-off here already put right is not acted on. After a
//! crash loop whose turn-off failed, the retries wait longer than the window, so a retry that
//! comes late applies the chain again instead (one try every 8 s at most). A death while a drag
//! has values to send does not re-read the state file ([`EqControl::reload_if_changed`] waits
//! for the drag); `running()` still decides.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::Notify;
use tokio::task::JoinHandle;

use super::*;

/// How many ticks (10 s at 50 ms) a death after a restart still counts as one in a row with the
/// death before it.
pub(super) const DEATH_WINDOW: u64 = 200;

/// How many deaths in a row within the window of a restart are a crash loop: the first is ridden
/// out (PipeWire restarting stops the chain once more after a restart that went through).
const QUICK_DEATHS: u32 = 2;

/// What the note says when a chain was started again.
const RESTARTED_NOTE: &str = "the computer EQ chain stopped and was started again";

/// The flag the waiting task raises, and the task.
#[derive(Default)]
pub(super) struct ExitWatch {
    /// The chain ended and the controller has not put it right yet.
    died: Arc<AtomicBool>,
    /// The controller put the chain right: the task may wait again.
    handled: Arc<Notify>,
    task: Option<JoinHandle<()>>,
    /// The task waits for [`ExitWatch::done`]: the flag was taken and not answered yet.
    owes_ack: bool,
    /// The tick at which the task is let wait again (a back-off), when it is held.
    ack_at: Option<u64>,
    /// The chain is known to be down and has not been put right.
    down: bool,
    /// The tick of the last restart that went through.
    pub(super) restarted_at: Option<u64>,
    /// How many deaths in a row came within [`DEATH_WINDOW`] of a restart that went through.
    quick: u32,
    /// The death being answered is counted in `quick` (a retried restart is the same death).
    counted: bool,
    /// How many tries to start or remove the chain failed one after another.
    pub(super) retries: u32,
}

impl ExitWatch {
    /// Start the waiting task, once, when a runtime is there to run it.
    pub(super) fn arm(&mut self, chain: &Arc<dyn Chain>) {
        if self.task.is_some() {
            return;
        }
        let Ok(runtime) = tokio::runtime::Handle::try_current() else {
            return;
        };
        let (chain, died, handled) = (chain.clone(), self.died.clone(), self.handled.clone());
        self.task = Some(runtime.spawn(async move {
            loop {
                chain.exited().await;
                died.store(true, Ordering::SeqCst);
                handled.notified().await;
            }
        }));
    }

    /// Whether the chain ended and has not been put right yet.
    pub(super) fn pending(&self) -> bool {
        self.down || self.died.load(Ordering::SeqCst)
    }

    /// Take the flag the task raised, remembering that the task waits for an answer.
    fn take(&mut self) -> bool {
        let died = self.died.swap(false, Ordering::SeqCst);
        if died {
            self.owes_ack = true;
            self.down = true;
        }
        died
    }

    /// The task may wait again.
    fn done(&mut self) {
        self.ack_at = None;
        if std::mem::take(&mut self.owes_ack) {
            self.handled.notify_one();
        }
    }

    /// An apply or a remove went through: a death before it is out of date, and the chain that
    /// runs now is the one to wait on.
    pub(super) fn put_right(&mut self) {
        // A death the tick has not taken yet still has the task waiting for its answer.
        if self.died.swap(false, Ordering::SeqCst) {
            self.owes_ack = true;
        }
        self.down = false;
        self.counted = false;
        self.retries = 0;
        self.done();
    }
}

impl Drop for ExitWatch {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

impl EqControl {
    /// Put right a chain that ended, when it did.
    pub(super) async fn check_exit(&mut self) {
        if self.exit.ack_at.is_some_and(|at| self.ticks >= at) {
            self.exit.done();
        }
        if !self.exit.take() {
            return;
        }
        let hold = self.chain_died().await;
        match hold {
            0 => self.exit.done(),
            ticks => {
                if self.exit.owes_ack {
                    self.exit.ack_at = Some(self.ticks + ticks);
                }
            }
        }
    }

    /// The ticks to leave the chain alone before looking again after a try that did not go
    /// through: 1, 2, 4, then 8 seconds.
    fn back_off(&mut self) -> u64 {
        let ticks = (WATCH_EVERY << self.exit.retries.min(3)).min(8 * WATCH_EVERY);
        self.exit.retries += 1;
        ticks
    }

    /// Answer a death; the ticks to wait before the chain is waited on again (0: now).
    async fn chain_died(&mut self) -> u64 {
        // The EQ was turned off meanwhile: nothing is owed.
        if !self.runs {
            self.exit.down = false;
            return 0;
        }
        // Another svolder (the command line, a peer controller) may have replaced or removed the
        // chain: what its state file says wins, and a chain that runs is not a dead one.
        if self.reload() && !self.runs {
            self.exit.down = false;
            return 0;
        }
        match self.running().await {
            Some(true) => {
                self.exit.down = false;
                return 0;
            }
            None => return self.back_off(),
            Some(false) => {}
        }
        let quick = self
            .exit
            .restarted_at
            .is_some_and(|at| self.ticks < at + DEATH_WINDOW);
        if !std::mem::replace(&mut self.exit.counted, true) {
            self.exit.quick = if quick { self.exit.quick + 1 } else { 0 };
        }
        let crash_loop = self.exit.quick >= QUICK_DEATHS;
        if self.xf_on {
            // The restarted chain plays its config with the stage off.
            self.reset_xf(
                OffReason::ChainRestarted,
                "crossfeed turned off: the computer EQ chain restarted".to_string(),
                false,
            );
        }
        if self.flat {
            // The chain ran for crossfeed alone, which is off now (or was a pass-through): no
            // chain is wanted.
            if self.owed.is_none() {
                self.owed = Some(Owed {
                    plays: false,
                    tried: None,
                });
            }
            self.finish_reset().await;
            self.exit.down = false;
            return 0;
        }
        if crash_loop {
            return self.stop_for_good().await;
        }
        match self.apply().await {
            Ok(()) => {
                self.exit.restarted_at = Some(self.ticks);
                self.note = Some(RESTARTED_NOTE.to_string());
                0
            }
            Err(failure) => {
                let hold = self.back_off();
                self.note = Some(format!(
                    "the computer EQ chain stopped and could not be started again ({failure}); \
                     trying again in {} s",
                    hold / WATCH_EVERY
                ));
                hold
            }
        }
    }

    /// The chain keeps dying: the EQ is left off, with a note.
    async fn stop_for_good(&mut self) -> u64 {
        let why = "it stopped again right after it was started again";
        match self.turn_off().await {
            Ok(()) => {
                self.note = Some(format!("computer EQ turned off: the chain stopped ({why})"));
                0
            }
            Err(_) if !self.runs => {
                self.note = Some(format!(
                    "computer EQ turned off: the chain stopped ({why}); its state could not be saved"
                ));
                0
            }
            Err(error) => {
                self.note = Some(format!(
                    "computer EQ could not be turned off after its chain stopped: {error}"
                ));
                self.back_off()
            }
        }
    }
}
