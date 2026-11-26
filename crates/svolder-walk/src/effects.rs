// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the walk can do: listen, test a passphrase, write the device, and read what is
//! configured already. The front end's first run implements it over its own radio and file;
//! the daemon implements it over its scanner, a probe actor and its config writer.

use std::future::Future;
use std::time::Duration;

use svolder_ipc::{Connect, DeviceConfig, ScanEntry};
use svolder_profiles::Registry;

/// What one scan heard, and what opening the radio for it had to say.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Heard {
    /// one row per unit heard, first sighting each
    pub entries: Vec<ScanEntry>,
    /// what opening the radio had to say, as lines to show above the rows
    pub notices: Vec<String>,
    /// what every adapter heard, when the machine has more than one and the walk listened on
    /// all of them; `None` otherwise, which leaves the walk as it was
    pub adapters: Option<crate::Listened>,
}

/// Why a write of the device was refused.
#[derive(Debug)]
pub enum WriteError<F> {
    /// Another device already carries the name: the walk goes back to its name step.
    NameTaken(String),
    /// The name is none the file takes (blank, too long, a control character), in the writer's
    /// words: the walk goes back to its name step.
    BadName(String),
    /// Another block already configures the unit, in these words (which name the device): the
    /// walk goes back to its pick step.
    UnitConfigured(String),
    /// The file cannot take the write (read-only, held by another writer, unreadable): the walk
    /// stops on it.
    Stop(F),
}

/// What the walk asks of the machine it runs on.
///
/// The methods that take time are written as futures without a `Send` bound: the front end's
/// effects run on one thread, while the daemon's, run in a job, are `Send` because its own
/// types are, which the compiler sees through a concrete [`crate::DeviceWalk`].
pub trait WalkEffects {
    /// What an effect that did not happen hands back; the walk passes it through untouched.
    type Failure;

    /// Listen for devices in range.
    fn scan(&self) -> impl Future<Output = Result<Heard, Self::Failure>>;

    /// The longest a scan listens, which the scan step says before it starts.
    fn scan_window(&self) -> Duration;

    /// Open a session to the device `seen` names with `passphrase` (`auto`, `none` or four
    /// digits), and let it go again.
    fn try_connect(
        &self,
        seen: &ScanEntry,
        passphrase: &str,
    ) -> impl Future<Output = Result<Connect, Self::Failure>>;

    /// Write the device into the configuration.
    fn write_device(
        &self,
        device: &DeviceConfig,
    ) -> impl Future<Output = Result<(), WriteError<Self::Failure>>>;

    /// The names the configured devices already carry.
    fn taken_names(&self) -> Vec<String>;

    /// The configured devices' names with the unit ids their blocks carry.
    fn taken_units(&self) -> Vec<(String, String)>;

    /// The profiles devices are read through, or nothing when they could not be read.
    fn registry(&self) -> Option<&Registry>;

    /// Make `adapter` the one svolder connects through: written into the config as `adapter`
    /// (`address` is what to name there, `name` its kernel name), and followed by whatever holds
    /// the links. The walk asks for it before the device's passphrase is tested, so the test runs
    /// on the adapter that hears the device.
    fn switch_adapter(
        &self,
        address: &str,
        name: &str,
    ) -> impl Future<Output = Result<(), Self::Failure>>;

    /// The names of the configured devices that hold a link now, which the scan and the
    /// passphrase test may disturb. None by default: a walk with nothing linked starts at its
    /// scan.
    fn linked(&self) -> Vec<String> {
        Vec::new()
    }
}
