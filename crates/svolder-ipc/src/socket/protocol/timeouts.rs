// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How long the daemon and the front ends wait: command deadlines, stop bounds, the default
//! request timeout and the longest scan window.

/// The longest the daemon waits for a device on one step of a command: to hand it to the device's
/// actor, and then for the actor's answer, each; a fleet recall waits it for its plan and again for
/// its end. The daemon's own deadlines are this number, and [`DEFAULT_REQUEST_TIMEOUT`]'s is
/// derived from it.
pub const COMMAND_DEADLINE: std::time::Duration = std::time::Duration::from_secs(60);

/// The longest a daemon takes to answer one request, which it answers in its own words when a
/// device is silent: two [`COMMAND_DEADLINE`]s (hand-over and answer, or a recall's plan and end).
pub(crate) const COMMAND_WORST_CASE: std::time::Duration = COMMAND_DEADLINE.saturating_mul(2);

/// How long a front end waits for the reply to a `reload`. A reload reconciles the devices one at
/// a time, each of which may take a [`COMMAND_DEADLINE`] with a stuck actor, so it can take that
/// many deadlines plus one; this covers nine stuck devices.
pub const RELOAD_TIMEOUT: std::time::Duration = COMMAND_DEADLINE.saturating_mul(10);

/// The longest the daemon waits for its jobs to end as it stops: each looks at its stop flag
/// between steps, so this only runs out for work stuck inside one step, and the stop goes on.
pub const JOB_STOP_BOUND: std::time::Duration = std::time::Duration::from_secs(3);

/// What the daemon gives each actor beyond the farewell bound to close its session as it stops.
pub const ACTOR_STOP_MARGIN: std::time::Duration = std::time::Duration::from_secs(6);

/// The longest the daemon waits, as it stops, for the MQTT bridge to say `offline` and
/// disconnect: the broker's own wait and a moment.
pub const BRIDGE_STOP_BOUND: std::time::Duration = std::time::Duration::from_secs(4);

/// The longest the daemon waits, as it stops, for a configuration write already under way to
/// finish: a write waits at most two seconds for the file's lock and then writes.
pub const WRITE_SETTLE_BOUND: std::time::Duration = std::time::Duration::from_secs(3);

/// The longest the daemon waits for threads of blocking work as its runtime ends.
pub const RUNTIME_STOP_BOUND: std::time::Duration = std::time::Duration::from_secs(3);

/// How long [`crate::Client::request`] waits for a reply: `COMMAND_WORST_CASE` and a margin, so
/// the daemon's words always arrive before this bound, which is only the net against a daemon
/// that says nothing.
pub const DEFAULT_REQUEST_TIMEOUT: std::time::Duration =
    COMMAND_WORST_CASE.saturating_add(std::time::Duration::from_secs(5));

/// The longest window a daemon listens for on a `scan` request. A longer one asked for is
/// listened for this long, and the front end that asks says so before it does.
pub const MAX_SCAN_WINDOW: std::time::Duration = std::time::Duration::from_secs(30);
