// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The daemon that holds the device, stopped for a capture and brought back.
//!
//! One central holds a Forkbeard at a time, so a capture over the radio needs the daemon that
//! holds it out of the way. [`holder`] finds who that is without waking anything: where systemd
//! owns the socket and a unit is running, a connect would start the service, so only the units
//! are read there. [`away`] stops the holder, runs a body and puts back what was there whatever
//! the body did, a panic included.

use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use futures::FutureExt;
use svolder_ipc::checks::SystemdFacts;
use svolder_ipc::{Config, socket};

use crate::backend::Globals;
use crate::backend::launch::{self, Host};
use crate::commands::daemon::lifecycle::{LOCK_WAIT, stop_spawned, systemd_owns};
use crate::commands::daemon::{Ctl, SERVICE_UNIT, SOCKET_UNIT};
use crate::recovery::{Failure, Kind};

/// Who holds the device.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Holder {
    /// Nobody does.
    Nobody,
    /// systemd's units do, whichever of them were running.
    Systemd {
        /// Whether `svolderd.socket` was running.
        socket: bool,
        /// Whether `svolderd.service` was running.
        service: bool,
    },
    /// A daemon a front end started answers on this socket.
    Spawned {
        /// The socket it answers on.
        socket: PathBuf,
    },
}

/// Who holds the device on the socket `g` and `config` name.
///
/// With the fake transport the device is this process's own and nobody holds it. Where systemd's
/// units own the socket and one of them is running, they hold it, and the socket itself is never
/// connected to: that would start the service. With both units idle nothing can start, so the
/// socket is asked like any other: a daemon a front end or the owner started by hand may answer.
pub(crate) async fn holder(g: &Globals<'_>, config: &Config, ctl: &Ctl) -> Holder {
    if g.transport == "fake" {
        return Holder::Nobody;
    }
    let path = socket::path::resolve(g.socket, config.socket.as_deref());
    let units = ctl.units();
    if systemd_owns(&units, &path) {
        let (socket, service) = (running(units.socket_unit), running(units.service_unit));
        if socket || service {
            return Holder::Systemd { socket, service };
        }
    }
    if svolder_ipc::is_running(&path).await {
        Holder::Spawned { socket: path }
    } else {
        Holder::Nobody
    }
}

/// Whether a unit is running, or on its way there.
fn running(unit: Option<svolder_ipc::checks::UnitFacts>) -> bool {
    unit.is_some_and(|unit| unit.running)
}

/// The marker a `systemctl` that ran out of time leaves in its failure.
const TIMED_OUT: &str = "no answer within";

/// Wait up to `bound` for the units to satisfy `want`, which a `systemctl` that was given up on
/// may yet bring about: systemd goes on with a job after the program that asked for it is gone.
async fn settled(ctl: &Ctl, bound: Duration, want: impl Fn(&SystemdFacts) -> bool) -> bool {
    let deadline = Instant::now() + bound;
    loop {
        if want(&ctl.units()) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// A daemon failure whose next step is the exact command that undoes it.
fn daemon_failure(what: String, next: String) -> Failure {
    Failure {
        code: Kind::DaemonError.code(),
        what,
        next,
    }
}

/// Stop `holder` and wait until it has let go of the device.
///
/// systemd's units are stopped together, the socket unit with the service so nothing starts it
/// again, and a `systemctl` that ran out of time is followed by waiting for the units to report
/// inactive, as a daemon standing down can take longer than the program is given; a spawned
/// daemon is asked over its socket and waited for until the socket and the lock beside it are
/// free.
pub(crate) async fn stop(_g: &Globals<'_>, ctl: &Ctl, holder: &Holder) -> Result<(), Failure> {
    match holder {
        Holder::Nobody => Ok(()),
        Holder::Systemd { .. } => match ctl.systemctl(&["stop", SERVICE_UNIT, SOCKET_UNIT]) {
            Ok(()) => Ok(()),
            Err(why)
                if why.contains(TIMED_OUT)
                    && settled(ctl, LOCK_WAIT, |units| {
                        !running(units.socket_unit) && !running(units.service_unit)
                    })
                    .await =>
            {
                Ok(())
            }
            Err(why) => Err(Failure::of(
                Kind::DaemonError,
                format!("the daemon could not be stopped for the capture: {why}"),
            )),
        },
        Holder::Spawned { socket } => stop_spawned(socket).await,
    }
}

/// The command that starts a daemon again for the socket it was on, with `--socket` when that is
/// not the default one.
fn restart_command(socket: &Path) -> String {
    if socket == socket::path::resolve(None, None) {
        "svolder daemon restart".to_string()
    } else {
        format!("svolder --socket {} daemon restart", socket.display())
    }
}

/// Put back what `holder` was: the units that were running, the socket first, or a daemon
/// started the way svolder starts one. A failure says the command that brings it back.
pub(crate) async fn restore(ctl: &Ctl, host: &dyn Host, holder: &Holder) -> Result<(), Failure> {
    match holder {
        Holder::Nobody => Ok(()),
        Holder::Systemd { socket, service } => {
            let mut failed = Vec::new();
            for (was_running, unit) in [(*socket, SOCKET_UNIT), (*service, SERVICE_UNIT)] {
                if !was_running {
                    continue;
                }
                match ctl.systemctl(&["start", unit]) {
                    Ok(()) => {}
                    Err(why)
                        if why.contains(TIMED_OUT)
                            && settled(ctl, LOCK_WAIT, |units| {
                                running(if unit == SOCKET_UNIT {
                                    units.socket_unit
                                } else {
                                    units.service_unit
                                })
                            })
                            .await => {}
                    Err(why) => failed.push((unit, why)),
                }
            }
            if failed.is_empty() {
                return Ok(());
            }
            let said: Vec<String> = failed.iter().map(|(_, why)| why.clone()).collect();
            let commands: Vec<String> = failed
                .iter()
                .map(|(unit, _)| format!("`systemctl --user start {unit}`"))
                .collect();
            Err(daemon_failure(
                format!("the daemon was not started again: {}", said.join("; ")),
                format!("Run {}", commands.join(", then ")),
            ))
        }
        Holder::Spawned { socket } => launch::autospawn(host, socket).await.map_err(|why| {
            daemon_failure(
                format!("the daemon was not started again: {}", why.what),
                format!("Run `{}`", restart_command(socket)),
            )
        }),
    }
}

/// Stop `holder`, run `body`, and put `holder` back whatever happened.
///
/// The restore runs whenever the stop was tried, even one that failed part way (one unit
/// stopped, the other not): putting back what was there is always right. A body that returns
/// early, with a failure or because it was interrupted, is still followed by the restore, and so
/// is a stop or a body that panics, which is then resumed. The body does not run behind a stop
/// that failed.
///
/// The returned future must be driven to its end: raced in a `select!` or dropped, the restore
/// never runs. An interrupt (Ctrl-C) is therefore handled inside `body`, which returns when it
/// arrives.
pub(crate) async fn away<T>(
    g: &Globals<'_>,
    ctl: &Ctl,
    host: &dyn Host,
    holder: &Holder,
    body: impl Future<Output = T>,
) -> (Result<T, Failure>, Result<(), Failure>) {
    away_after(stop(g, ctl, holder), ctl, host, holder, body).await
}

/// [`away`] with the stop passed in as the future that does it.
async fn away_after<T>(
    stopping: impl Future<Output = Result<(), Failure>>,
    ctl: &Ctl,
    host: &dyn Host,
    holder: &Holder,
    body: impl Future<Output = T>,
) -> (Result<T, Failure>, Result<(), Failure>) {
    // The stop is inside the catch too: one that panics after a unit went down still gets its
    // restore.
    let ran = AssertUnwindSafe(async {
        match stopping.await {
            Ok(()) => Ok(body.await),
            Err(failure) => Err(failure),
        }
    })
    .catch_unwind()
    .await;
    let back = restore(ctl, host, holder).await;
    match ran {
        Ok(ran) => (ran, back),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

#[cfg(test)]
mod tests;
