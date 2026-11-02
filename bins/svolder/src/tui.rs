// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! `svolder tui`: an interactive terminal UI over a [`Backend`](crate::backend::Backend).
//!
//! This claims the terminal, opens the same backend every other verb uses, seeds an
//! [`App`](app::App) from the readings the device holds, and hands both to the event loop in
//! [`run()`].
//!
//! - [`app`]: the state: the readings heard, what is focused, and the intents keys turn into.
//! - [`audio_level`]: how loud what is playing is, for everything that follows a level.
//! - [`boxes`]: the boxes opened over a screen, each one's state and logic.
//! - [`clipboard`]: copying text out of the TUI by every way that reaches the user's clipboard.
//! - [`decoration`]: the animation and ornament drawn around the content.
//! - [`host`]: the host's own audio as the TUI shows it.
//! - [`keys`]: the one table of the keys every screen listens for.
//! - [`layout_tier`]: the two size tiers a screen is laid out in.
//! - [`mouse`]: where the last frame drew what, so a click can ask for what a key does.
//! - [`palette`]: the xterm colour tables the theme reduces through.
//! - [`render`]: which screen one frame shows.
//! - [`mod@run`]: the event loop.
//! - [`screens`]: the screens the TUI draws, one module each.
//! - `screenshots`: the README's pictures, drawn by this renderer (compiled for tests only).
//! - [`start`]: the start of a run: claim the terminal, play the intro, hand over to the loop.
//! - [`terminal`]: raw-mode setup and teardown.
//! - [`theme`]: what the pieces of a screen mean, and the styles a theme gives them.
//! - [`waiting`]: waiting on work with the alternate screen up.
//! - [`widgets`]: the row widgets a control screen is made of.
//! - [`wizard`]: the setup wizard on the terminal.

pub mod app;
mod audio_level;
mod boxes;
mod clipboard;
mod decoration;
mod host;
mod keys;
mod layout_tier;
mod mouse;
mod palette;
mod render;
mod run;
mod screens;
#[cfg(test)]
mod screenshots;
mod start;
mod terminal;
#[cfg(test)]
mod test_support;
mod theme;
pub(crate) mod waiting;
mod widgets;
pub(crate) mod wizard;

use std::process::ExitCode;

use svolder_ipc::Config;

use crate::backend::Globals;
use terminal::Stdout;

/// Options for `svolder tui`, gathered from its own flags and the environment.
pub struct TuiOptions {
    /// Force monochrome rendering: set by `--no-color` or the `NO_COLOR` environment
    /// variable.
    pub no_color: bool,
    /// The socket of the daemon `svolder tui --demo` started for this run. Every open the TUI
    /// makes then connects to that socket and nothing else: no launcher chain, no daemon
    /// started, no direct session, and no device added through the machine's own scan.
    pub demo: Option<std::path::PathBuf>,
    /// The socket of a daemon `svolder tui --join` was told to join. Every open is the same
    /// connect-only open the demo's is, and no device is added.
    pub join: Option<std::path::PathBuf>,
    /// What the start already said about a daemon it asked to read the configuration again,
    /// shown as a toast once the intro is over.
    pub reloaded: Option<String>,
    /// Whether the start already looked at the daemon, so [`run()`] does not look again.
    pub checked: bool,
}

/// The one daemon's socket every open goes to, when the run is the demo or a joined daemon.
pub(crate) fn only_socket(opts: &TuiOptions) -> Option<std::path::PathBuf> {
    opts.demo.clone().or_else(|| opts.join.clone())
}

/// Run the TUI until `q`: claim the terminal, open a backend, and run the event loop on it.
///
/// Exits 1 before opening anything if stdout is not a terminal, or if the configuration fails to
/// load; otherwise the exit code is the loop's own. An open that finds no device ends nothing: the
/// loop starts on the waiting panel or the Devices screen the failure earns.
///
/// The terminal is claimed before the device is opened, so the seconds a connect takes are spent
/// on the wordmark rather than an empty screen: the open and the intro run side by side, and
/// whichever finishes first waits for the other. Ctrl-C or `q`, pressed over the mark or the
/// wait after it, gives up on a device that has not answered and leaves with 0 as the loop's own
/// `q` does. The terminal is given back before that line is printed, so it lands on the
/// user's own screen.
///
/// The loop opens a backend again whenever the user switches device, so what it opens under is
/// kept rather than borrowed from the command line, except `-v`. Every open this makes happens
/// with the alternate screen up, so [`Opener::reopen`](run::Opener::reopen) drops `-v` from
/// all of them; what `-v` says about a TUI run is the one line below, said before the screen is
/// claimed.
pub async fn run(g: &Globals<'_>, opts: TuiOptions) -> ExitCode {
    start::run_on(&Stdout, Config::load(), Config::path(), g, opts).await
}
