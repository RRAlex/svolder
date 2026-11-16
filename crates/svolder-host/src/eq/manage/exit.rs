// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! An exit signal for the chain: [`Host::exited`] resolves when the process that runs it ends.

use super::*;

impl Host {
    /// Resolves when the chain's process ends on its own: killed, crashed, or taken away by
    /// something else. The wait is on the process itself (a pidfd), never a poll.
    ///
    /// It follows the chain: an apply or a remove that stops the chain, or replaces it, is not
    /// an exit, and the wait moves on to the chain that is running after it. It resolves when
    /// a computer EQ is on and no process runs the chain: at once when it died before the wait
    /// began, and for a unit only when systemd answers that it is inactive or failed. It does
    /// not resolve while no computer EQ is on, nor when the chain cannot be looked at (systemd
    /// does not answer, the unit is `activating` or `reloading`, no pidfd could be opened): the
    /// wait then lasts until the next apply or remove, which looks again. A unit that systemd
    /// restarts by itself (`Restart=on-failure`) resolves at the death of each main process
    /// that is seen while the unit is active.
    ///
    /// How to drive it: one task of its own for the chain's life, which only waits and then
    /// tells the control (a watch or a notify); never recreate it per tick (each call asks
    /// systemd once) and never pin it in a `select!` whose other arms apply or remove, which
    /// is the one task that must not wait on the gate while this future is parked. After it
    /// resolves, apply (or remove) before waiting again: a chain that is still gone resolves it
    /// again at once.
    pub async fn exited(&self) {
        let mut epoch = self.epoch.subscribe();
        loop {
            // Looked at under the gate, so no apply is half done: what is found is what runs.
            let located = {
                let _gate = self.gate.lock().await;
                epoch.borrow_and_update();
                self.locate()
            };
            let watched = match located {
                Located::Idle => Watched::Idle,
                Located::Gone => Watched::Gone,
                Located::Held(held) => Watched::of(held),
                // Systemd is asked with the gate released: an apply waits for no answer of it.
                Located::AskUnit => match self.unit_main_pid().await {
                    // An apply or remove ran meanwhile: what was answered is out of date.
                    _ if epoch.has_changed().unwrap_or(false) => continue,
                    Unit::Running(pid) => Watched::of(Held::of(pid)),
                    Unit::Stopped => Watched::Gone,
                    Unit::Unknown => Watched::Idle,
                },
            };
            match watched {
                Watched::Gone => {
                    // An apply or remove that is still under way may be what stopped it: its
                    // end, which moves the epoch, decides.
                    let _gate = self.gate.lock().await;
                    if !epoch.has_changed().unwrap_or(false) {
                        return;
                    }
                }
                // Nothing to wait on, or nothing that can be: the next apply looks again.
                Watched::Idle => {
                    if epoch.changed().await.is_err() {
                        return std::future::pending().await;
                    }
                }
                Watched::Running(fd) => {
                    tokio::select! {
                        ready = fd.readable() => {
                            drop(ready);
                            // An apply or remove that is under way stopped it, or finishes
                            // first: wait for the gate, then see whether the epoch moved.
                            let _gate = self.gate.lock().await;
                            if !epoch.has_changed().unwrap_or(false) {
                                return;
                            }
                        }
                        _ = epoch.changed() => {}
                    }
                }
            }
        }
    }

    /// What the chain that should be running is, as far as the gate alone tells.
    fn locate(&self) -> Located {
        let Some(state) = self.state().filter(|state| state.on && !self.off) else {
            return Located::Idle;
        };
        match state.manager {
            Manager::Child => match locked(&self.child).as_ref().and_then(|child| child.id()) {
                Some(pid) => Located::of(Held::of(pid)),
                None => Located::Gone,
            },
            Manager::Process => {
                // The hold is opened before the command line is checked, and kept.
                match self
                    .chains()
                    .into_iter()
                    .find_map(|(pid, argv)| Held::open(pid, &argv))
                {
                    Some(held) => Located::Held(Some(held)),
                    None => Located::Gone,
                }
            }
            Manager::Systemd => Located::AskUnit,
        }
    }

    /// What systemd says of the unit's main process.
    async fn unit_main_pid(&self) -> Unit {
        let Some(text) = self
            .ctl
            .show(&["show", UNIT, "-p", "ActiveState", "-p", "MainPID"])
            .await
        else {
            return Unit::Unknown;
        };
        let state = text
            .lines()
            .find_map(|line| line.trim().strip_prefix("ActiveState="));
        let pid = text
            .lines()
            .find_map(|line| line.trim().strip_prefix("MainPID="))
            .and_then(|pid| pid.parse::<u32>().ok())
            .filter(|pid| *pid > 1);
        match (state, pid) {
            (Some("active"), Some(pid)) => Unit::Running(pid),
            (Some("inactive" | "failed"), _) => Unit::Stopped,
            _ => Unit::Unknown,
        }
    }
}

/// What the gate alone says of the chain.
enum Located {
    /// No computer EQ is on.
    Idle,
    /// A computer EQ is on and no process runs it.
    Gone,
    /// The process that runs it, held (`None`: it is gone already).
    Held(Option<Held>),
    /// A unit runs it: systemd is to be asked.
    AskUnit,
}

impl Located {
    fn of(held: Option<Held>) -> Located {
        match held {
            Some(held) => Located::Held(Some(held)),
            None => Located::Gone,
        }
    }
}

/// What systemd says of the unit.
enum Unit {
    /// Active, with this main process.
    Running(u32),
    /// Inactive or failed.
    Stopped,
    /// No answer, or a state that says nothing about the process (`activating`, `reloading`).
    Unknown,
}

/// What [`Host::exited`] waits on.
enum Watched {
    /// Nothing that can be waited on: the next apply looks again.
    Idle,
    /// A computer EQ is on and no process runs it.
    Gone,
    /// The process that runs it, readable when it ends.
    Running(tokio::io::unix::AsyncFd<std::os::fd::OwnedFd>),
}

impl Watched {
    /// The wait on `held`: gone when there is no such process, idle when no pidfd could be had
    /// (a refused `pidfd_open`, a full descriptor table), which a later look may get.
    fn of(held: Option<Held>) -> Watched {
        let Some(held) = held else {
            return Watched::Gone;
        };
        let Some(fd) = held.into_fd() else {
            return Watched::Idle;
        };
        // SAFETY: the pidfd is open, owned by the value registered, and closed only when that
        // drops; `register` is the form of `AsyncFd::new` that tokio 1.54 does not deprecate.
        match unsafe { tokio::io::unix::AsyncFd::register(fd) } {
            Ok(fd) => Watched::Running(fd),
            Err(_) => Watched::Idle,
        }
    }
}
