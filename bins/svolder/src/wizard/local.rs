// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device walk's effects over the wizard's own: the scan, the passphrase test and the write
//! go through this machine's radio and configuration file, as [`super::Effects`] does them.

use std::time::Duration;

use svolder_ipc::{Connect, DeviceConfig, ScanEntry};
use svolder_profiles::Registry;
use svolder_walk::{Heard, WalkEffects, WriteError};

use super::Effects;
use crate::recovery::{Failure, Kind};

/// The wizard's effects, as the device walk asks for them.
pub(crate) struct Local<E>(pub(crate) E);

impl<E: Effects> WalkEffects for Local<E> {
    type Failure = Failure;

    async fn scan(&self) -> Result<Heard, Failure> {
        let scanned = self.0.scan().await?;
        let units: Vec<String> = scanned.entries.iter().map(|e| e.unit.clone()).collect();
        let adapters = self.0.listen_on_all(&units).await;
        Ok(Heard {
            entries: scanned.entries,
            notices: scanned.notices,
            adapters,
        })
    }

    fn scan_window(&self) -> Duration {
        crate::scanning::WINDOW.saturating_mul(u32::from(crate::scanning::WINDOWS))
    }

    /// The adapter is written into the config before the passphrase test, which opens its session
    /// through the config's adapter, so it runs on the adapter that hears the device.
    async fn switch_adapter(&self, _address: &str, name: &str) -> Result<(), Failure> {
        self.0
            .write_adapter(name)
            .map_err(|why| Failure::of(Kind::ConfigUnreadable, why))
    }

    async fn try_connect(&self, seen: &ScanEntry, passphrase: &str) -> Result<Connect, Failure> {
        self.0.try_connect(seen, passphrase).await
    }

    /// A write the file refuses stops the walk: the writer's words are kept, under the exit
    /// code of a configuration that cannot be read or written.
    async fn write_device(&self, device: &DeviceConfig) -> Result<(), WriteError<Failure>> {
        self.0
            .write_device(device)
            .await
            .map_err(|why| WriteError::Stop(Failure::of(Kind::ConfigUnreadable, why)))
    }

    fn taken_names(&self) -> Vec<String> {
        self.0.taken_names()
    }

    fn taken_units(&self) -> Vec<(String, String)> {
        self.0.taken_units()
    }

    fn registry(&self) -> Option<&Registry> {
        self.0.registry()
    }
}
