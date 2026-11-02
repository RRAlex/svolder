// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `daemon restart` and `daemon stop`: start the daemon again, or stop it, whichever of the two
//! machines this is.
//!
//! Where systemd owns the daemon it is asked to; otherwise the daemon is asked over its socket
//! and waited for until it has let go of the socket and the lock beside it. Both waits are
//! shared with the install, which stops a daemon a front end started before the units take its
//! socket.

use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use svolder_ipc::checks::SystemdFacts;
use svolder_ipc::{Client, Request, Response};

use crate::backend::Globals;
use crate::backend::launch::{self, Host};
use crate::recovery::{Failure, Kind};

use super::{
    Ctl, SERVICE_UNIT, SOCKET_UNIT, resolved_socket, systemd_manages, trouble, units_stopped,
};

/// How long a daemon asked to stop is given to let the socket go.
const STOP_WAIT: Duration = Duration::from_secs(5);

/// Whether the daemon on `socket` is systemd's to start and stop.
///
/// It is systemd's when systemd has the units and the socket unit binds the path in hand. A path
/// the unit does not listen on is another daemon's (one a session with its own runtime directory
/// or `SVOLDER_SOCKET` started, which `daemon status` and `doctor` name), and stopping systemd's
/// own would leave that one running and still holding its devices. A socket unit that names no
/// path at all leaves nothing to tell the two apart, so the units are taken at their word.
pub(crate) fn systemd_owns(units: &SystemdFacts, socket: &Path) -> bool {
    systemd_manages(units)
        && units
            .listen_stream
            .as_deref()
            .is_none_or(|listen| listen == socket)
}

/// Start the daemon again, so it re-reads everything it read when it started.
///
/// With `if_running`, only a daemon that is running is restarted and none is started.
pub async fn restart(g: &Globals<'_>, if_running: bool) -> Result<(), Failure> {
    let socket = resolved_socket(g)?;
    restart_on(&Ctl::live(), &launch::Live, &socket, if_running).await
}

/// [`restart`] over given programs, a given host and a given socket.
async fn restart_on(
    ctl: &Ctl,
    host: &dyn Host,
    socket: &Path,
    if_running: bool,
) -> Result<(), Failure> {
    let units = ctl.units();
    // With both units stopped, systemd has nothing to restart and whatever answers on the
    // socket is another daemon's.
    if systemd_owns(&units, socket) && !(if_running && units_stopped(&units)) {
        if if_running && !units.service_unit.is_some_and(|u| u.running) {
            eprintln!("svolder: {SERVICE_UNIT} is not running; nothing to restart");
            return Ok(());
        }
        ctl.systemctl(&["restart", SERVICE_UNIT])
            .map_err(|why| trouble(why, "Run `svolder daemon status`"))?;
        eprintln!("svolder: restarted {SERVICE_UNIT}");
        return Ok(());
    }
    if svolder_ipc::is_running(socket).await {
        ask_to_stop(socket).await?;
        wait_for_the_socket_to_close(socket).await?;
        wait_for_the_old_daemon_to_exit(socket, LOCK_WAIT).await;
    } else if if_running {
        eprintln!(
            "svolder: no daemon answers on {}; nothing to restart",
            socket.display()
        );
        return Ok(());
    }
    launch::autospawn(host, socket).await?;
    eprintln!("svolder: started svolderd on {}", socket.display());
    Ok(())
}

/// Stop the daemon, whichever of the two is holding the socket.
pub async fn stop(g: &Globals<'_>) -> Result<(), Failure> {
    let socket = resolved_socket(g)?;
    stop_on(&Ctl::live(), &socket).await
}

/// [`stop`] over given programs and a given socket.
async fn stop_on(ctl: &Ctl, socket: &Path) -> Result<(), Failure> {
    if systemd_owns(&ctl.units(), socket) {
        ctl.systemctl(&["stop", SERVICE_UNIT, SOCKET_UNIT])
            .map_err(|why| trouble(why, "Run `svolder daemon status`"))?;
        eprintln!("svolder: stopped {SERVICE_UNIT} and {SOCKET_UNIT}");
        return Ok(());
    }
    if !svolder_ipc::is_running(socket).await {
        eprintln!(
            "svolder: no daemon answers on {}; nothing to stop",
            socket.display()
        );
        return Ok(());
    }
    ask_to_stop(socket).await?;
    wait_for_the_socket_to_close(socket).await?;
    eprintln!("svolder: stopped the daemon on {}", socket.display());
    Ok(())
}

/// Stop the daemon a front end started on `socket` and wait until it has exited: the socket is
/// free and so is the lock beside it. A daemon that outlives the wait is a failure.
pub(crate) async fn stop_spawned(socket: &Path) -> Result<(), Failure> {
    stop_spawned_within(socket, LOCK_WAIT).await
}

/// [`stop_spawned`] with the lock wait bounded by `bound`. The failure is the only report of a
/// daemon that outlives the wait.
async fn stop_spawned_within(socket: &Path, bound: Duration) -> Result<(), Failure> {
    ask_to_stop(socket).await?;
    wait_for_the_socket_to_close(socket).await?;
    if the_old_daemon_exits_within(socket, bound).await {
        Ok(())
    } else {
        Err(Failure::of(
            Kind::DaemonError,
            format!(
                "the daemon on {} had not exited {bound:?} after it was asked to stop",
                socket.display()
            ),
        ))
    }
}

/// Ask the daemon on `socket` to stop, as a termination signal would.
pub(super) async fn ask_to_stop(socket: &Path) -> Result<(), Failure> {
    let mut client = Client::connect(socket)
        .await
        .map_err(|e| Failure::of(Kind::DaemonError, format!("{}: {e}", socket.display())))?;
    match client.request(Request::Stop).await {
        Ok(Response::Stopping) => Ok(()),
        Ok(other) => Err(Failure::of(
            Kind::DaemonError,
            format!("the daemon answered {other:?} rather than stopping"),
        )),
        // The daemon that stops as it answers can close the connection first; that is the
        // request doing what it asked for, not a failure of it.
        Err(_) => Ok(()),
    }
}

/// Wait for the socket to stop answering, which is the daemon letting go of it.
pub(super) async fn wait_for_the_socket_to_close(socket: &Path) -> Result<(), Failure> {
    let deadline = Instant::now() + STOP_WAIT;
    while svolder_ipc::is_running(socket).await {
        if Instant::now() >= deadline {
            return Err(Failure::of(
                Kind::DaemonError,
                format!(
                    "{} still answers {STOP_WAIT:?} after the daemon was asked to stop",
                    socket.display()
                ),
            ));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    Ok(())
}

/// How long a restart waits for the old daemon to exit once its socket is gone: the daemon's own
/// shutdown deadline (its farewell bound and a margin of six seconds) and two seconds more.
pub(crate) const LOCK_WAIT: Duration =
    svolder_ble::FAREWELL_BOUND.saturating_add(Duration::from_secs(8));

/// [`the_old_daemon_exits_within`], saying on standard error when it gives up; the caller starts
/// the new daemon either way.
async fn wait_for_the_old_daemon_to_exit(socket: &Path, bound: Duration) -> bool {
    let exited = the_old_daemon_exits_within(socket, bound).await;
    if !exited {
        eprintln!(
            "svolder: the old daemon had not exited {bound:?} after it was asked to stop; \
             starting the new one anyway, which gives up if the old one still runs"
        );
    }
    exited
}

/// Wait until the lock the daemon on `socket` holds beside it can be taken, which is when that
/// daemon has exited: the socket is gone as soon as its accept loop ends, but the daemon still
/// closes its devices, and a new daemon started before then finds the lock taken and gives up.
/// Polls with a growing pause and gives up after `bound`, silently.
async fn the_old_daemon_exits_within(socket: &Path, bound: Duration) -> bool {
    let mut name = socket.as_os_str().to_owned();
    name.push(".lock");
    let Ok(lock) = std::fs::File::open(PathBuf::from(name)) else {
        return true;
    };
    let deadline = Instant::now() + bound;
    let mut pause = Duration::from_millis(10);
    loop {
        // SAFETY: the descriptor belongs to `lock`, which outlives the call; `flock` reads no
        // memory of ours. The lock is released when `lock` closes.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(pause).await;
        pause = (pause * 2).min(Duration::from_millis(250));
    }
}

#[cfg(test)]
mod tests;
