// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The configuration a verb loads, the failures that name what is wrong with it, and the
//! `--transport` words.

use super::*;

/// The configuration for a verb that acts on the device `selector` names (or the default, with
/// none), or a failure naming what is wrong with it. A `default_device` naming no configured
/// device fails a verb that needs the default, in its own words and without the warning that
/// says the same; what the file only deserves a warning for is printed to stderr here.
pub(super) fn load_config(path: &Path, selector: Option<&str>) -> Result<Config, Failure> {
    let config = Config::load_from(path).map_err(|e| config_error(e.to_string()))?;
    if let Some(failure) = dangling_default_failure(&config, selector) {
        return Err(failure);
    }
    print_warnings(&config);
    Ok(config)
}

/// Print each of the config's warnings to stderr as `svolder: <warning>`, each at most once
/// per process: a verb that reads the configuration in two places says it once.
pub(crate) fn print_warnings(config: &Config) {
    static SAID: Mutex<Vec<String>> = Mutex::new(Vec::new());
    for warning in config.warnings() {
        let mut said = SAID.lock().unwrap_or_else(|e| e.into_inner());
        if !said.contains(&warning) {
            eprintln!("svolder: {warning}");
            said.push(warning);
        }
    }
}

/// The failure of a verb that would fall back on a `default_device` naming no configured
/// device: it names the device rather than reaching for another one.
fn dangling_default_failure(config: &Config, selector: Option<&str>) -> Option<Failure> {
    if selector.is_some() {
        return None;
    }
    config.dangling_default_failure().map(config_error)
}

/// Say under `-v` which daemon a verb is talking to.
pub(crate) fn daemon_note(g: &Globals<'_>, socket: &std::path::Path) {
    if g.verbose {
        eprintln!("svolder: daemon at {}", socket.display());
    }
}

/// The configured devices as rows, in the order the file lists them.
pub(crate) fn rows_from_config(config: &Config) -> Vec<DeviceRow> {
    config
        .devices
        .iter()
        .map(|device| DeviceRow {
            name: device.name.clone(),
            ..DeviceRow::default()
        })
        .collect()
}

/// A failure the user can fix by typing the command differently: exit 1.
///
/// What `--transport` and the fake device's environment knobs are given is the command line's
/// to say, so a value neither of them can use is answered by the help rather than by the file.
pub(super) fn usage(message: String) -> Failure {
    Failure::of(Kind::Usage, message)
}

/// A failure the user can fix in the configuration file: exit 1.
pub(super) fn config_error(message: String) -> Failure {
    Failure::of(Kind::ConfigUnreadable, message)
}

/// The profile registry, or a failure pointing at the profiles it could not read.
pub(crate) fn registry() -> Result<svolder_profiles::Registry, Failure> {
    crate::registry().map_err(|e| Failure::of(Kind::UnknownProduct, e))
}

/// The transport `--no-start` stands for: a daemon already answering on the socket, and nothing
/// else. `--no-start` sets it; `main` refuses it as a `--transport` value, so it is an
/// internal name.
pub const RUNNING: &str = "running";

/// Whether `--transport` names somewhere this build can run.
///
/// This is the one list of them: [`Backend::open`] and [`Backend::devices_if_running`] ask it
/// before they dispatch, and a front end with no verb to open a backend for asks it on its own,
/// so an unusable value is refused in the same words whichever caller was given it.
pub fn known_transport(name: &str) -> Result<(), Failure> {
    match name {
        "auto" | "daemon" | "ble" | "fake" | RUNNING => Ok(()),
        other => Err(unknown_transport(other)),
    }
}

/// Whether `--transport` lets a verb consult a running daemon: `auto` and `daemon` do, `ble`
/// and `fake` run without one.
pub fn asks_a_daemon(transport: &str) -> bool {
    matches!(transport, "auto" | "daemon" | RUNNING)
}

/// A `--transport` value that names nowhere to run, in the one wording every caller of it
/// uses.
pub(crate) fn unknown_transport(name: &str) -> Failure {
    usage(format!(
        "unknown transport {name:?}; use auto, daemon, ble, or fake"
    ))
}
