// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the wizard can do to the machine it runs on, in three seams.
//!
//! [`Host`] is what the machine says about itself, [`Radio`] what it hears and reaches over the
//! air, [`Files`] what the wizard writes or installs. [`Effects`] is the three together, which
//! is what a walk needs; a reader that only looks, like the summary, asks for the seams it
//! uses.

use async_trait::async_trait;
use svolder_ipc::checks::{Fix, Outcome};
use svolder_ipc::{DeviceConfig, ScanEntry};
use svolder_profiles::Registry;

use super::{Connect, Platform};
use crate::recovery::Failure;
use crate::scanning::{Grade, Scanned};

/// What the machine says about itself.
#[async_trait(?Send)]
pub trait Host {
    /// What the host checks say, as the rows `doctor` prints.
    async fn checks(&self) -> Vec<Outcome>;

    /// Apply one of the fixes svolder may apply itself, answering with what happened.
    async fn apply(&self, fix: &Fix) -> Result<String, String>;

    /// What this machine makes of a background service.
    fn platform(&self) -> Platform;

    /// The home-manager block that stands in for the user units on a machine that generates
    /// them, as lines.
    fn nixos_snippet(&self) -> Vec<String>;

    /// The profiles this build reads devices through, or nothing when they could not be read.
    fn registry(&self) -> Option<&Registry>;
}

/// What the machine hears and reaches over the air.
#[async_trait(?Send)]
pub trait Radio {
    /// Listen for devices in range, with whatever opening a backend had to say.
    async fn scan(&self) -> Result<Scanned, Failure>;

    /// The machine's adapters, as BlueZ names them: what says whether there is a choice to
    /// offer, before the grading that takes seconds is run.
    fn adapters(&self) -> Vec<String>;

    /// How well each of the machine's adapters hears this device.
    async fn grade(&self, udid: &str) -> Vec<Grade>;

    /// Open a session to this device with `passphrase` and let it go again.
    async fn try_connect(&self, seen: &ScanEntry, passphrase: &str) -> Result<Connect, Failure>;
}

/// What the wizard writes or installs.
#[async_trait(?Send)]
pub trait Files {
    /// Write this device into the configuration file.
    async fn write_device(&self, device: &DeviceConfig) -> Result<(), String>;

    /// Tell a daemon that is running to read the configuration file again.
    async fn reload_daemon(&self);

    /// Write the adapter to open sessions on into the configuration file.
    fn write_adapter(&self, adapter: &str) -> Result<(), String>;

    /// Install the daemon's user units, answering with what happened.
    async fn install_daemon(&self) -> Result<String, String>;

    /// The names the configured devices already carry.
    fn taken_names(&self) -> Vec<String>;
}

/// Everything the wizard can do to the machine it runs on: the three seams together.
///
/// The core holds one of these and nothing else that touches the host, so a test drives every
/// step and branch against a fake and no radio, no configuration file and no daemon are
/// involved. [`super::live::LiveEffects`] is the real one.
pub trait Effects: Host + Radio + Files {}

impl<T: Host + Radio + Files> Effects for T {}
