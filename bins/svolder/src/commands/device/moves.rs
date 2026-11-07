// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Moving a correction between the DAC and the computer: the question that asks, the devices
//! a running daemon serves that have a preset on, and the flattening of them.
//!
//! The same preset is never left on both. `svolder host eq apply NAME` asks before it flattens
//! the devices NAME is on, and `svolder eq load NAME` asks before it takes NAME off the
//! computer. Only a daemon that is already running is asked about its devices: with none
//! reachable there is nothing to check, and no scan or connection is started to find out.

use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use svolder_ipc::Config;
use svolder_profiles::tone::preset::{self, Preset};

use crate::backend::{ASK_WAIT, Backend, Globals, asks_a_daemon, launch};
use crate::recovery::{Failure, Kind};

use super::eq::{current_raw, do_flatten};

/// A device of a running daemon that has a preset on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Holder {
    /// The daemon's socket.
    pub(crate) socket: PathBuf,
    /// The device's configured name, which is what a request selects it by.
    pub(crate) device: String,
}

/// Whether the user said yes to `question`: `yes` is an answer already, a terminal is asked,
/// and anything else is refused with the flag that answers without one.
pub(crate) fn confirm(question: &str, yes: bool) -> Result<bool, Failure> {
    decide(question, yes, std::io::stdin().is_terminal(), super::ask)
}

/// [`confirm`] with the terminal and the asking given, for the tests.
pub(crate) fn decide(
    question: &str,
    yes: bool,
    terminal: bool,
    ask: impl FnOnce(&str) -> std::io::Result<bool>,
) -> Result<bool, Failure> {
    if yes {
        return Ok(true);
    }
    if !terminal {
        return Err(Failure::of(
            Kind::Usage,
            format!("{question} needs an answer and there is no terminal; pass --yes to say yes"),
        ));
    }
    ask(question).map_err(|e| Failure::of(Kind::Usage, e.to_string()))
}

/// What a no, or an Enter, leaves on stderr.
pub(crate) fn nothing_changed() -> ExitCode {
    eprintln!("svolder: nothing changed");
    ExitCode::SUCCESS
}

/// The devices of the running daemon that have `preset`, built-in or saved, on, which is to say
/// the device's mode and bands read what the preset gives. Empty without a daemon, and for a
/// device that is not connected or does not answer in time.
pub(crate) async fn holders(g: &Globals<'_>, config: &Config, preset: &Preset) -> Vec<Holder> {
    if !asks_a_daemon(g.transport) {
        return Vec::new();
    }
    let Some(socket) = launch::probe(g, config).await else {
        return Vec::new();
    };
    let Some(held) = Backend::hold(g, Some(&socket), None, &Config::path()).await else {
        return Vec::new();
    };
    held.backend.close().await;
    let mut found = Vec::new();
    for row in held.rows.iter().filter(|row| row.connected == Some(true)) {
        let asked = tokio::time::timeout(ASK_WAIT, async {
            let (mut backend, _) = Backend::open_on(&socket, Some(&row.name)).await.ok()?;
            let on = match backend.state().await {
                Ok(values) => backend.profile().eq.as_ref().map(|eq| {
                    let raw = current_raw(&values);
                    preset::matches(eq, raw, preset)
                }),
                Err(_) => None,
            };
            backend.close().await;
            on
        })
        .await;
        if let Ok(Some(true)) = asked {
            found.push(Holder {
                socket: socket.clone(),
                device: row.name.clone(),
            });
        }
    }
    found
}

/// Flatten every device in `holders`. The first failure ends it, with the exit code it earned.
pub(crate) async fn flatten(holders: &[Holder]) -> Result<(), ExitCode> {
    for holder in holders {
        let mut backend = match Backend::open_on(&holder.socket, Some(&holder.device)).await {
            Ok((backend, _)) => backend,
            Err(failure) => return Err(failure.report()),
        };
        let outcome = do_flatten(&mut backend).await;
        backend.close().await;
        match outcome {
            Ok(code) if code == ExitCode::SUCCESS => {}
            Ok(code) => return Err(code),
            Err(message) => return Err(Failure::of(Kind::Usage, message).report()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
