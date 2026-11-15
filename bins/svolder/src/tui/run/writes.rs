// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The writes the loop makes to the configuration file for a Settings change, off the loop.
//!
//! A write waits up to two seconds for the file's lock and then syncs the file, which while the
//! daemon holds the lock (a web preset save, a bridge setting, a server switch) would freeze keys
//! and frames. Each write runs on a thread of its own; one is in flight at a time and the rest
//! wait in the order they were asked, so the file ends as the last change left it. The outcome
//! lands on [`Writes`] and the loop takes it in with [`Writes::recv`].

use std::collections::VecDeque;

use svolder_ipc::ConfigError;

/// One write: what it did to the file (`true` when the file changed) or why it could not.
pub(in crate::tui) type Job = Box<dyn FnOnce() -> Result<bool, ConfigError> + Send + 'static>;

/// What a write came to: whether the file changed, or the words of its failure.
pub(in crate::tui) type Done = Result<bool, String>;

/// The writes of a session: one in flight, the rest in the order asked.
#[derive(Default)]
pub(in crate::tui) struct Writes {
    running: Option<tokio::sync::oneshot::Receiver<Done>>,
    queued: VecDeque<Job>,
}

impl Writes {
    /// Ask for `job` to run after the ones asked before it, and start it when none is in flight.
    pub(in crate::tui) fn push(&mut self, job: Job) {
        self.queued.push_back(job);
        self.start();
    }

    /// Whether a write is in flight, so the loop is to read [`Writes::recv`].
    pub(in crate::tui) fn busy(&self) -> bool {
        self.running.is_some()
    }

    /// Whether anything is in flight or waiting.
    pub(in crate::tui) fn owes(&self) -> bool {
        self.running.is_some() || !self.queued.is_empty()
    }

    /// Start the next write when none is in flight.
    fn start(&mut self) {
        if self.running.is_some() {
            return;
        }
        let Some(job) = self.queued.pop_front() else {
            return;
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        tokio::task::spawn_blocking(move || {
            let _ = tx.send(job().map_err(|e| e.to_string()));
        });
        self.running = Some(rx);
    }

    /// The outcome of the write in flight, once it ends; pending while none is in flight.
    pub(in crate::tui) async fn recv(&mut self) -> Done {
        let Some(running) = self.running.as_mut() else {
            return std::future::pending().await;
        };
        running
            .await
            .unwrap_or_else(|_| Err("the write stopped".to_string()))
    }

    /// Take a write's outcome in: the next one starts. Call it after [`Writes::recv`] answered.
    pub(in crate::tui) fn landed(&mut self) {
        self.running = None;
        self.start();
    }

    /// Run what is still owed, in order, and answer each outcome; for a loop that is ending,
    /// which waits for the write in flight and does the rest where it stands.
    pub(in crate::tui) async fn drain(&mut self) -> Vec<Done> {
        let mut done = Vec::new();
        while self.owes() {
            let outcome = self.recv().await;
            let failed = outcome.is_err();
            done.push(outcome);
            // A write that failed (a lock held past its wait, most likely) has the rest fail the
            // same way, each after its own wait: they are given up on, so a quit waits once.
            if failed {
                let busy = "not saved: the file is busy";
                done.extend(self.queued.drain(..).map(|_| Err(busy.to_string())));
            }
            self.landed();
        }
        done
    }
}
