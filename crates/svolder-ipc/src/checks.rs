// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Health checks over a snapshot of the host.
//!
//! [`run`] turns a [`Probes`] snapshot — what BlueZ, systemd, the environment and the config
//! file say — into one [`Outcome`] per check, in the order they are meant to be read. Nothing
//! here reaches the system itself: [`live`](live::live) takes the snapshot, so a front end can
//! print the outcomes and the daemon can log them without either one growing its own probes,
//! and the tests drive every branch from a fixture.
//!
//! The checks that need a radio session — grading the adapters against a device, and scanning
//! for the configured devices — belong to the front end that has one.
//!
//! - [`live`]: taking the snapshot from the real host.
//! - [`processes`]: which svolderd processes this user is running.
//!
//! The checks themselves sit in `bluetooth` (service, adapters, advertising, plugins), `daemon`
//! (socket location, other daemons) and `environment` (config, container, WirePlumber); the
//! snapshot's types are in `facts`.

mod bluetooth;
mod daemon;
mod environment;
mod facts;
pub mod live;
pub mod processes;

use std::path::Path;

use crate::config::Config;

use bluetooth::{adapters, bluetooth_service, bluetoothd_plugins, extended_advertising};
use daemon::{daemon, other_daemons, runtime};
use environment::{config, container, wireplumber};
pub use facts::{AdapterFacts, BluezFacts, EnvFacts, Probes, RfkillFacts, SystemdFacts, UnitFacts};

/// The service that must answer before anything else can work.
pub const BLUETOOTH_SERVICE: &str = "Bluetooth service";
/// The adapters BlueZ reports.
pub const ADAPTERS: &str = "Adapters";
/// Where the socket lives.
pub const RUNTIME: &str = "Runtime";
/// The units that start the daemon.
pub const DAEMON: &str = "Daemon";
/// A svolderd of this user's that listens somewhere else than this session's socket.
pub(crate) const OTHER_DAEMONS: &str = "Other daemons";
/// The config file and what it names.
pub const CONFIG: &str = "Config";
/// What a container needs from its host.
pub const CONTAINER: &str = "Container";
/// Whether two adapters can advertise beyond the legacy 31 bytes.
pub(crate) const EXTENDED_ADVERTISING: &str = "Extended advertising";
/// Whether bluetoothd runs without the plugins that fight for the same devices.
pub(crate) const BLUETOOTHD_PLUGINS: &str = "bluetoothd plugins";
/// Whether WirePlumber leaves BLE MIDI devices alone.
pub const WIREPLUMBER: &str = "WirePlumber";

/// The command the daemon check's fix names: the verb that writes the units and enables them.
///
/// A front end that applies a fix itself recognises it by the command it carries, so the
/// command is written once, here, rather than in the check and again in the front end.
pub const DAEMON_INSTALL: &str = "svolder daemon install";

/// The command the config check's fix names: the verb that walks a first run.
pub const SETUP: &str = "svolder setup";

/// How a check came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum State {
    /// The host has what this check looks for.
    Pass,
    /// Something works but is not what it should be.
    Warn,
    /// Something this check looks for is missing or broken.
    Fail,
    /// A fact that is neither good nor bad, recorded so the reader knows it was looked at.
    Note,
}

/// One check's result.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    /// The check's name, as it is printed.
    pub check: &'static str,
    /// How it came out.
    pub state: State,
    /// One line saying what was found.
    pub reason: String,
    /// What to do about it, when there is something to do.
    pub fix: Option<Fix>,
}

/// What to do about a check that did not pass.
#[derive(Debug, Clone, PartialEq)]
pub struct Fix {
    /// The command to run, exactly as it should be typed.
    pub command: String,
    /// Whether a front end may run this itself, without asking the user to type it. A fix that
    /// needs root, or that changes the state of a radio, never may.
    pub applies_itself: bool,
}

/// An outcome is one JSON object of `check`, `state`, `reason`, `fix` and `applies_itself`:
/// the fix is flattened to its command, or `null` when there is none, and `applies_itself`
/// stands beside it so a reader never has to look inside a nested object.
impl serde::Serialize for Outcome {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut out = serializer.serialize_struct("Outcome", 5)?;
        out.serialize_field("check", self.check)?;
        out.serialize_field("state", &self.state)?;
        out.serialize_field("reason", &self.reason)?;
        out.serialize_field("fix", &self.fix.as_ref().map(|f| f.command.as_str()))?;
        out.serialize_field(
            "applies_itself",
            &self.fix.as_ref().is_some_and(|f| f.applies_itself),
        )?;
        out.end()
    }
}

/// Every check but the ones that need a radio session, in the order they are read.
///
/// `socket` is the socket path the caller resolved, which the daemon check holds the socket
/// unit to. `dev` adds the three checks only someone running the simulator or the end-to-end
/// script cares about.
pub fn run(probes: &dyn Probes, socket: &Path, dev: bool) -> Vec<Outcome> {
    let bluez = probes.bluez();
    let systemd = probes.systemd();
    let env = probes.env();
    let config = probes.config_text().map(|text| match text {
        Ok(text) => Config::parse(&text).map_err(|e| first_line(&e.to_string())),
        Err(message) => Err(first_line(&message)),
    });

    let mut outcomes = vec![
        bluetooth_service(&bluez),
        adapters(&bluez, config.as_ref(), &probes.rfkill()),
        runtime(&env, socket),
        daemon(&systemd, socket),
    ];
    outcomes.extend(other_daemons(
        &probes.daemons(),
        socket,
        systemd.listen_stream.as_deref(),
    ));
    outcomes.push(self::config(
        config.as_ref(),
        &probes.profiles(),
        probes.profiles_error().as_deref(),
    ));
    if env.container {
        outcomes.push(container(&bluez, &systemd));
    }
    if dev {
        outcomes.push(extended_advertising(&bluez));
        outcomes.push(bluetoothd_plugins(&systemd));
        outcomes.push(wireplumber(&env));
    }
    outcomes
}

/// The first line of a message, for a reason that is read as one line.
fn first_line(message: &str) -> String {
    message.lines().next().unwrap_or_default().trim_end().into()
}

/// A check that found what it looks for.
fn pass(check: &'static str, reason: impl Into<String>) -> Outcome {
    Outcome {
        check,
        state: State::Pass,
        reason: reason.into(),
        fix: None,
    }
}

/// The exit code a set of outcomes asks for: 2 when anything failed, 1 when anything warned,
/// 0 when every check passed or only recorded a fact.
pub fn exit_code(outcomes: &[Outcome]) -> u8 {
    if outcomes.iter().any(|o| o.state == State::Fail) {
        2
    } else if outcomes.iter().any(|o| o.state == State::Warn) {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests;
