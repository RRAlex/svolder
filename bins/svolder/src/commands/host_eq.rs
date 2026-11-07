// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder host eq`: an equalizer on what the computer plays, as a PipeWire filter chain in
//! front of the sink that feeds the DAC.
//!
//! The manager of the chain, [`Host`], is in `svolder-host`; this module is the command that
//! drives it and the entry points the front ends call, each on this machine's own paths.
//!
//! - [`verbs`]: the verbs of `svolder host eq`, and the question `eq load` asks.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use svolder_host::eq::{ApplyRequest, EqView, Host, HostEqState, Status};
use svolder_ipc::{Client, Config, HostEqOp, Request, Response};

use crate::backend::{ASK_WAIT, Globals, asks_a_daemon, launch};
use crate::cli::{HostCommand, HostEqCommand};
use crate::recovery::Failure;
use crate::which;

mod verbs;

pub(super) use verbs::move_to_dac;
use verbs::{host_apply, host_remove, host_status};

/// Carry out one of the `host` sub-commands.
pub(crate) async fn run(g: &Globals<'_>, command: HostCommand) -> ExitCode {
    match command {
        HostCommand::Eq {
            command: HostEqCommand::Apply { target, sink, yes },
        } => host_apply(g, &target, sink.as_deref(), yes).await,
        HostCommand::Eq {
            command: HostEqCommand::Remove,
        } => host_remove(g).await,
        HostCommand::Eq {
            command: HostEqCommand::Status { json },
        } => host_status(g, json).await,
    }
}

/// This machine's computer EQ: its files where this installation keeps them, and whether this
/// process runs in a container.
fn live() -> Host {
    Host::live(
        svolder_ipc::config::host_eq_paths(),
        svolder_ipc::checks::live::in_container(),
    )
}

/// Where this machine's computer EQ keeps its saved state, made absolute.
fn state_path() -> std::path::PathBuf {
    which::absolute(&svolder_ipc::config::host_eq_state_path())
}

/// The socket of a running daemon that serves the computer EQ, and the EQ it shows. A daemon
/// that does not serve it (an older one, or one whose host is off) is no daemon for this, and
/// neither is none at all: the command then does the work itself. Nothing is started to find
/// out.
pub(crate) async fn served(g: &Globals<'_>, config: &Config) -> Option<(PathBuf, EqView)> {
    if !asks_a_daemon(g.transport) {
        return None;
    }
    let socket = launch::probe(g, config).await?;
    let eq = eq_view(&socket).await?;
    Some((socket, eq))
}

/// The computer EQ the daemon on `socket` shows; `None` when it shows none or does not answer
/// in time.
async fn eq_view(socket: &Path) -> Option<EqView> {
    tokio::time::timeout(ASK_WAIT, async {
        let mut client = Client::connect(socket).await.ok()?;
        match client.request(Request::Host).await.ok()? {
            Response::Host(view) => view.eq,
            _ => None,
        }
    })
    .await
    .ok()
    .flatten()
}

/// Ask the daemon on `socket` to change the computer EQ: its refusal comes back in its words.
async fn eq_op(socket: &Path, op: HostEqOp) -> Result<(), Failure> {
    let mut client = Client::connect(socket)
        .await
        .map_err(|e| Failure::of(crate::recovery::Kind::NoDaemon, e.to_string()))?;
    match client.request(Request::HostEq { op }).await {
        Ok(Response::Ack) => Ok(()),
        Ok(Response::Error { code, message }) => {
            Err(Failure::from_daemon_on(socket, code, message))
        }
        Ok(other) => Err(Failure::of(
            crate::recovery::Kind::DaemonError,
            format!("the daemon answered {other:?}"),
        )),
        Err(e) => Err(Failure::of(
            crate::recovery::Kind::DaemonError,
            e.to_string(),
        )),
    }
}

/// The refusal of a one-shot command under the child manager: the chain would end with the
/// command, so it is left to a program that keeps running.
fn child_needs_a_daemon() -> Failure {
    Failure {
        code: 1,
        what: "in a container the computer EQ runs with the daemon".to_string(),
        next: "start svolderd, then try again".to_string(),
    }
}

/// `host`, or the refusal when it keeps its chain as a child of the program that applied it.
fn one_shot(host: Host) -> Result<Host, Failure> {
    match host.runs_as_child() {
        true => Err(child_needs_a_daemon()),
        false => Ok(host),
    }
}

/// Apply on `host` for one command; nothing is written when the chain would not outlive it.
async fn apply_once(host: Host, request: ApplyRequest) -> Result<HostEqState, Failure> {
    one_shot(host)?.apply(request).await.map_err(Failure::from)
}

/// Remove on `host` for one command.
async fn remove_once(host: Host) -> Result<(), Failure> {
    one_shot(host)?.remove().await.map_err(Failure::from)
}

/// Apply on this machine, for one command.
pub async fn apply(request: ApplyRequest) -> Result<HostEqState, Failure> {
    apply_once(live(), request).await
}

/// Remove on this machine, for one command.
pub async fn remove() -> Result<(), Failure> {
    remove_once(live()).await
}

/// The one host of this process's session: a child it starts lives until the process ends.
fn session() -> std::sync::Arc<Host> {
    static HELD: std::sync::OnceLock<std::sync::Arc<Host>> = std::sync::OnceLock::new();
    std::sync::Arc::clone(HELD.get_or_init(|| std::sync::Arc::new(live())))
}

/// A controller of this machine's computer EQ for a session that keeps running (the TUI with no
/// daemon): over the session's one host, so a child it starts lives as long as the session, and
/// over `pw-cli`, which the live updates go through. Says why there is none where `pw-cli` is
/// missing. Nothing is applied or written by making it.
pub fn session_control() -> Result<svolder_host::eq::EqControl, String> {
    let params = svolder_host::eq::PwCliSession::from_env()
        .ok_or_else(|| "computer controls unavailable: no pw-cli on this computer".to_string())?;
    Ok(svolder_host::eq::EqControl::new(
        session(),
        Box::new(params),
        svolder_host::eq::Keep::File(state_path()),
    ))
}

/// The saved state of this machine's computer EQ.
pub fn state() -> Option<HostEqState> {
    live().state()
}

/// This machine's computer EQ, and whether it runs.
pub async fn status() -> Status {
    live().status().await
}

/// Whether the computer EQ can be used here: for a front end that offers it or not.
pub fn available() -> Result<(), Failure> {
    match live().unavailable() {
        Some(failure) => Err(failure.into()),
        None => Ok(()),
    }
}

#[cfg(test)]
mod tests;
