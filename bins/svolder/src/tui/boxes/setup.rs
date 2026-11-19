// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The PipeWire config box: the seven lines of the config check over the device screen, `k` or a
//! click on the Path row's chip opens it, `k` or `Esc` closes it.
//!
//! Every word in it is the judge's ([`setup_check`](svolder_ipc::host::setup_check)): this module
//! only picks the route the judge is given and holds what the box needs between frames. With a
//! daemon the route is the daemon's; without one the loop reads `pw-dump` itself when the box
//! opens and every [`READ_PERIOD`] while it is open, and the answer is held here.

use std::time::Duration;

use svolder_host::RouteFacts;
use svolder_ipc::host::{SetupCheck, SetupOutcome};

/// How often the loop reads the graph again while the box is open and no daemon speaks for the
/// host.
pub const READ_PERIOD: Duration = Duration::from_secs(2);

/// What the box says while its own first read is on its way.
pub const READING: &str = "reading the PipeWire graph…";
/// What the box says when its own read found no graph.
pub const UNREAD: &str = "svolder could not read the PipeWire graph (pw-dump)";

/// The PipeWire config box while it is open.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetupBox {
    /// The route the loop read itself, the latest one that answered: used only where no daemon
    /// speaks for the host.
    pub own: Option<Box<RouteFacts>>,
    /// Whether the loop's latest read of its own found no graph.
    pub failed: bool,
    /// Whether the loop was asked for the box's first read.
    pub asked: bool,
}

/// The mark a line wears: `✓` for a pass, `✗` for a failure and `?` for one not read.
pub fn mark(outcome: SetupOutcome) -> &'static str {
    match outcome {
        SetupOutcome::Pass => "✓",
        SetupOutcome::Fail => "✗",
        SetupOutcome::Unknown => "?",
    }
}

/// How many of the check's lines pass, and how many it has.
pub fn passes(check: &SetupCheck) -> (usize, usize) {
    let pass = check
        .lines
        .iter()
        .filter(|line| line.outcome == SetupOutcome::Pass)
        .count();
    (pass, check.lines.len())
}

#[cfg(test)]
mod tests;
