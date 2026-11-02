// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `watch`: the device's values, then every status it sends until Ctrl-C.

use std::future::Future;
use std::process::ExitCode;

use futures::future::BoxFuture;
use svolder_ipc::ValueObject;

use crate::backend::{Backend, Failure, Globals, OnlyWhile, Update};
use crate::commands::interrupts::Presses;
use crate::format::print_value;
use crate::recovery::Kind;

use super::{exit_for, open_watched, print_values};

/// Reach the device, print the snapshot it holds, then print every status it sends until
/// Ctrl-C or the device stops coming back.
///
/// The snapshot is printed here because a refresh burst is otherwise drained silently; without
/// it, `watch` would show nothing until the device's next spontaneous status.
///
/// Ctrl-C exits 0. A dropped link is reconnected (by the session on the direct path, by the
/// daemon on the remote one), and either way the link going down and coming back is printed to
/// stderr; the snapshot is reprinted after a reconnect so a value that changed while the device
/// was away is visible. Whatever ends the loop, the backend is closed before returning, so a
/// direct session releases its BLE connection; a release that takes over half a second says
/// `svolder: releasing NAME…` on stderr, once, and is still waited for. The Ctrl-C future is
/// created once and polled across every iteration, so a signal arriving between two statuses is
/// still seen.
///
/// The reprint after a reconnect runs beside the loop rather than inside it, so Ctrl-C and the
/// next status are answered while it is still in flight. The loop's arms are polled in order:
/// Ctrl-C, the reprint, then the next status. A reprint that is ready at once (the direct path's
/// is) prints before a buffered status; on the remote path a status arriving during a slow
/// reprint prints first, since it is not held back for the reprint.
///
/// A Ctrl-C during the release (a second one when Ctrl-C ended the loop) stops waiting for it:
/// `svolder: not waiting for the release; the device may stay held for a moment` on stderr, and
/// exit 130, whatever code the loop ended with.
///
/// A Ctrl-C while the device is still being reached ends the process by the signal; the handler
/// is in place from the moment the first reading can be printed.
///
/// SIGINT is received by one task for the whole command, which counts each press ([`Presses`]),
/// so presses made while nothing waits are kept, one for each wait that follows.
pub async fn watch(g: &Globals<'_>, json: bool) -> Result<ExitCode, String> {
    let mut backend = match open_watched(g).await {
        Ok(backend) => backend,
        Err(code) => return Ok(code),
    };
    // The process-wide SIGINT handler `ctrl_c()` installs, held for the whole command.
    let mut sigint = match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
    {
        Ok(sigint) => sigint,
        Err(e) => {
            // Release the link before the error ends the command.
            backend.close().await;
            return Err(format!("cannot listen for Ctrl-C: {e}"));
        }
    };
    let presses = std::sync::Arc::new(Presses::new());
    let receiver = {
        let presses = std::sync::Arc::clone(&presses);
        tokio::spawn(async move {
            while sigint.recv().await.is_some() {
                presses.press();
            }
        })
    };
    let interrupt = || presses.next();
    let code = watch_loop(
        &mut backend,
        &interrupt,
        Backend::snapshot_request,
        |shown| match shown {
            Shown::Reading(vo) => print_value(vo, json),
            Shown::Values(values) => print_values(values, json),
        },
    )
    .await;
    let name = backend
        .configured_name()
        .or_else(|| g.device.map(str::to_string))
        .unwrap_or_else(|| backend.profile().display.clone());
    // Unlocked: a lock held across the release would block the trace lines other threads
    // print to stderr while the release waits.
    let code = release_or_give_up(backend, &name, code, &interrupt, &mut std::io::stderr()).await;
    receiver.abort();
    Ok(code)
}

/// What the loop puts on stdout.
enum Shown<'a> {
    /// One status the device sent.
    Reading(&'a ValueObject),
    /// The snapshot read at the start, or again after a reconnect.
    Values(&'a [ValueObject]),
}

/// A snapshot read that runs on its own, holding nothing of the backend.
type Reprint = BoxFuture<'static, Result<Vec<ValueObject>, Failure>>;

/// Release `backend`, and give up waiting for it when `interrupt` fires again.
///
/// The release is announced by `svolder: releasing NAME…` when it runs long. A further Ctrl-C
/// drops the release future and leaves with 130: the process is exiting, so what the release had
/// not finished is abandoned to the operating system and the device may stay held for a moment.
/// The line is written to `err`. Otherwise the code the loop ended with is returned.
async fn release_or_give_up<F: Future<Output = ()>>(
    backend: Backend,
    name: &str,
    code: ExitCode,
    interrupt: &impl Fn() -> F,
    err: &mut impl std::io::Write,
) -> ExitCode {
    tokio::select! {
        () = backend.release(name) => code,
        () = interrupt() => {
            let _ = writeln!(
                err,
                "svolder: not waiting for the release; the device may stay held for a moment"
            );
            ExitCode::from(130)
        }
    }
}

/// Print the snapshot, then every update until `interrupt` fires or the updates end.
///
/// `reprint` starts the snapshot read after a reconnect. It is polled as its own arm of the
/// `select!`, so a status or Ctrl-C arriving while it is in flight is handled at once; a read
/// still running when the link drops again is dropped.
async fn watch_loop<F: Future<Output = ()>>(
    backend: &mut Backend,
    interrupt: &impl Fn() -> F,
    reprint: impl Fn(&Backend) -> Reprint,
    mut print: impl FnMut(Shown<'_>),
) -> ExitCode {
    // Once, before anything reads it: subscribing consumes the daemon connection, and the
    // `select!` below drops whichever arm did not win.
    if let Err(e) = backend.open_feed().await {
        eprintln!("svolder: {e}");
        return exit_for(&e);
    }
    let mut gates = OnlyWhile::default();
    match backend.state().await {
        Ok(values) => {
            gates.note_all(backend.profile(), &values);
            print(Shown::Values(&values));
        }
        Err(failure) => return failure.report(),
    }
    // The snapshot was read successfully, so the link is up as of now: a first update
    // repeating that is not a transition and prints no marker.
    let mut connected = true;
    let mut reading: Option<Reprint> = None;
    let first = interrupt();
    tokio::pin!(first);
    loop {
        tokio::select! {
            biased;
            () = &mut first => return ExitCode::SUCCESS,
            snapshot = async { reading.as_mut().expect("guarded").await }, if reading.is_some() => {
                reading = None;
                match snapshot {
                    Ok(values) => {
                        gates.note_all(backend.profile(), &values);
                        print(Shown::Values(&values));
                    }
                    Err(failure) => return failure.report(),
                }
            }
            update = backend.next_status() => match update {
                Ok(Some(Update::Status(mut vo))) => {
                    let dependents = gates.apply(backend.profile(), &mut vo);
                    print(Shown::Reading(&vo));
                    for dependent in &dependents {
                        print(Shown::Reading(dependent));
                    }
                }
                Ok(Some(Update::Connection { connected: now })) => {
                    if now == connected {
                        continue;
                    }
                    connected = now;
                    if !now {
                        eprintln!("svolder: device disconnected");
                        reading = None;
                        continue;
                    }
                    eprintln!("svolder: reconnected");
                    reading = Some(reprint(backend));
                }
                // Watch prints readings, not recalls, and never asks for the host's view.
                Ok(Some(Update::Recalled(_) | Update::Host(_))) => continue,
                // The link is gone for good: the session spent its reconnect deadline and the
                // grace after it, or the daemon's stream ended.
                Ok(None) => {
                    let what = match backend.connect_deadline() {
                        Some(deadline) => format!(
                            "connection failed: device did not reconnect within {deadline:?}"
                        ),
                        None => "connection failed: the daemon stopped sending events".to_string(),
                    };
                    return Failure::of(Kind::Dropped, what).report();
                }
                Err(e) => {
                    eprintln!("svolder: {e}");
                    return exit_for(&e);
                }
            },
        }
    }
}

#[cfg(test)]
mod tests;
