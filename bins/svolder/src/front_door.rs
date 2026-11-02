// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What `svolder` with no verb does: choose between the interface and the guided first run, or
//! print the summary when nothing is watching.
//!
//! The choice looks at the daemon before it makes it, so a daemon holding devices the file no
//! longer lists is told first and cannot decide it. `main` calls [`front_door`] when the command
//! line names no verb.

use std::io::IsTerminal;
use std::process::ExitCode;

use crate::backend::{self, Globals};
use crate::recovery::{Failure, Kind};
use crate::{monochrome, summary, tui, wizard};
use svolder_ipc::Config;

#[cfg(test)]
mod tests;

/// What `svolder` with no verb does.
///
/// On a terminal it opens the TUI on the configured devices, or the guided first run when
/// there are none to open it on. Off one — a script, a shell over SSH — it prints the summary
/// [`summary::print`] draws and exits 0, because a program whose output is being read has no
/// business taking the screen or asking a question.
pub(crate) async fn front_door(g: &Globals<'_>) -> Result<ExitCode, String> {
    // A verb refuses a `--transport` naming nowhere to run before doing anything; with no verb
    // the refusal has to come first too, or a mistyped transport gets a summary instead.
    if let Err(failure) = backend::known_transport(g.transport) {
        return Ok(failure.report());
    }
    let config = match Config::load() {
        Ok(config) => config,
        Err(e) => return Ok(Failure::of(Kind::ConfigUnreadable, e.to_string()).report()),
    };
    backend::print_warnings(&config);
    if !std::io::stdout().is_terminal() {
        let reached = summary::reached(g, &config).await;
        let health = summary::health(g, &config).await;
        summary::print(&config, reached.as_deref(), &health);
        return Ok(ExitCode::SUCCESS);
    }
    match choose_door(
        &config,
        backend::reload_if_stale(g, &config, &Config::path()),
    )
    .await
    {
        Door::Wizard(note) => Ok(wizard::setup(g, note).await),
        Door::Tui(note) => Ok(tui::run(
            g,
            tui::TuiOptions {
                no_color: monochrome(false),
                demo: None,
                join: None,
                reloaded: note,
                checked: true,
            },
        )
        .await),
    }
}

/// Where the front door leads on a terminal, with the note the daemon check left.
#[derive(Debug, PartialEq, Eq)]
enum Door {
    /// The setup wizard: nothing is configured.
    Wizard(Option<String>),
    /// The interface.
    Tui(Option<String>),
}

/// Look at the daemon with `probe` first, and only then choose between the wizard and the
/// interface, so a daemon holding devices the file no longer lists is told before the choice
/// and cannot decide it.
async fn choose_door(
    config: &Config,
    probe: impl std::future::Future<Output = Option<String>>,
) -> Door {
    let note = probe.await;
    match config.devices.is_empty() {
        true => Door::Wizard(note),
        false => Door::Tui(note),
    }
}
