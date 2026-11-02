// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Ctrl-C and its kin, counted: a verb that must clean up before it leaves listens here.

use std::sync::Arc;

use tokio::signal::unix::{SignalKind, signal};

/// The Ctrl-C presses the command has had that no wait has taken yet, counted: each wait takes
/// one, so two presses made before anything waits are two, where a `Notify` keeps one.
pub(crate) struct Presses(tokio::sync::Semaphore);

impl Presses {
    /// No presses yet.
    pub(crate) fn new() -> Presses {
        Presses(tokio::sync::Semaphore::new(0))
    }

    /// One press, kept until a wait takes it.
    pub(crate) fn press(&self) {
        self.0.add_permits(1);
    }

    /// Wait for a press and take it.
    pub(crate) async fn next(&self) {
        if let Ok(permit) = self.0.acquire().await {
            permit.forget();
        }
    }
}

/// Count every SIGINT, SIGTERM and SIGHUP this process receives from now on, one press each.
///
/// The handlers are in place when this returns, so a signal that arrives before anything waits
/// is kept. A signal that cannot be listened for is not counted; the default action for it is
/// gone either way once another one is. It must run inside the tokio runtime.
pub(crate) fn listen() -> Arc<Presses> {
    let presses = Arc::new(Presses::new());
    for kind in [
        SignalKind::interrupt(),
        SignalKind::terminate(),
        SignalKind::hangup(),
    ] {
        let Ok(mut stream) = signal(kind) else {
            continue;
        };
        let presses = Arc::clone(&presses);
        tokio::spawn(async move {
            while stream.recv().await.is_some() {
                presses.press();
            }
        });
    }
    presses
}

#[cfg(test)]
mod tests;
