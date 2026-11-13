// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Leaving a daemon's walk by a signal: Ctrl-C at the plain prompts, and SIGHUP or SIGTERM
//! anywhere (the terminal closed, an ssh session dropped).
//!
//! A process that dies with the walk's prompt up leaves the daemon's one walk busy until its idle
//! bound, so while this client runs a walk it counts those three signals, and the front end
//! cancels its own walk and leaves when one comes. The TUI goes on after a walk, and a signal it
//! then gets takes the default course, as it did before.

use std::sync::{Arc, Mutex, Once};

use crate::commands::interrupts::Presses;

/// The presses of the walk that is running now, if one is.
static ACTIVE: Mutex<Option<Arc<Presses>>> = Mutex::new(None);

/// The OS handlers are installed once per process (tokio installs each once and keeps it), so
/// they are never taken down again: with no walk running, a signal does what its default does.
static INSTALLED: Once = Once::new();

/// Count SIGINT, SIGTERM and SIGHUP for the one walk that holds a guard; with none, put the
/// default action back for that signal and raise it again, which is what the process did before
/// anything counted them.
fn install() {
    use tokio::signal::unix::{SignalKind, signal};

    INSTALLED.call_once(|| {
        for (kind, number) in [
            (SignalKind::interrupt(), libc::SIGINT),
            (SignalKind::terminate(), libc::SIGTERM),
            (SignalKind::hangup(), libc::SIGHUP),
        ] {
            let Ok(mut stream) = signal(kind) else {
                continue;
            };
            tokio::spawn(async move {
                while stream.recv().await.is_some() {
                    let walk = ACTIVE.lock().map_or(None, |active| active.clone());
                    match walk {
                        Some(presses) => presses.press(),
                        None => {
                            // SAFETY: the default action, raised again, ends the process as the
                            // signal always did.
                            unsafe {
                                libc::signal(number, libc::SIG_DFL);
                                libc::raise(number);
                            }
                        }
                    }
                }
            });
        }
    });
}

/// The signals a daemon's walk is left by, counted while this is held.
pub(crate) struct Signals {
    presses: Arc<Presses>,
}

impl Signals {
    /// Count the signals for a walk from now until this is dropped. Must run inside the tokio
    /// runtime that lives as long as the process does.
    pub(crate) fn listen() -> Signals {
        install();
        let presses = Arc::new(Presses::new());
        if let Ok(mut active) = ACTIVE.lock() {
            *active = Some(Arc::clone(&presses));
        }
        Signals { presses }
    }

    /// Presses a test makes itself: nothing is counted for it.
    #[cfg(test)]
    pub(crate) fn scripted(presses: Arc<Presses>) -> Signals {
        Signals { presses }
    }

    /// The presses, for a front end to wait on beside its own work.
    pub(crate) fn presses(&self) -> Arc<Presses> {
        Arc::clone(&self.presses)
    }
}

impl Drop for Signals {
    /// Nothing counts for this walk any more; a signal then takes its default course.
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE.lock()
            && active
                .as_ref()
                .is_some_and(|mine| Arc::ptr_eq(mine, &self.presses))
        {
            *active = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two walks in a row in one process are each left by a signal: the second `listen` must
    /// catch what the first did, though the OS handler is installed once per process. (This is
    /// the only case that raises a signal, and it never raises one while no walk holds a guard.)
    #[tokio::test]
    async fn a_signal_leaves_each_of_two_walks_in_a_row() {
        for _ in 0..2 {
            let held = Signals::listen();
            let presses = held.presses();
            // SAFETY: raising a signal this process handles.
            unsafe { libc::raise(libc::SIGHUP) };
            tokio::time::timeout(std::time::Duration::from_secs(10), presses.next())
                .await
                .expect("the signal was counted for the walk");
            drop(held);
        }
    }
}
