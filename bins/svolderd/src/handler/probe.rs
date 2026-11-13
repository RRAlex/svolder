// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The walk's passphrase test: a short-lived device actor the daemon owns.
//!
//! The probe is [`device::spawn_probe`] with a temporary [`DeviceConfig`] for the unit being
//! tested: the same discovery, profile, passphrase and connect an actor of a configured device
//! runs, which says how its first attempt ended. It lives in the device set's `probe` slot beside
//! `starting`, entered under the set's lock, so the daemon's stop sees it:
//! - it is never started once the stop's mark is set;
//! - [`Daemon::shutdown`]'s actor step asks it to close with the other actors, at once, under the
//!   same deadline, so the restart wait (`LOCK_WAIT`) holds;
//! - [`Daemon::reconcile`] starts no actor for the unit it holds, and it is refused a unit an
//!   actor holds, so the two never fight over one device.
//!
//! It is asked to close once its answer is in (accepted, refused, or why no answer came), and
//! leaves the slot once it has closed, so the walk's write, and the reload the write sets off,
//! come after the radio is free. A test cut short by the daemon's stop leaves the probe in the
//! slot for the stop to close.

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::{Arc, RwLock};
use std::time::Duration;

use svolder_ipc::config::DeviceConfig;
use svolder_ipc::{Connect, ConnectFailure};
use svolder_proto::UnitId;
use tokio::sync::oneshot;

use super::{Daemon, DeviceSet};
use crate::device::{self, DeviceHandle, FirstAttempt};
use crate::locks::written;
use crate::log;

/// The longest a passphrase test waits for its first attempt to end: a listen
/// ([`crate::scanner::SCAN_WINDOW`]) and a connect, then for a device on `auto` that refused
/// the code its advertisement carried, a fresh listen and a second connect, with the radio's
/// turns between them.
pub(crate) const PROBE_BOUND: Duration = Duration::from_secs(75);

/// Whether two unit ids name one unit, however each is spelled.
pub(super) fn same_unit(a: &str, b: &str) -> bool {
    match (a.parse::<UnitId>(), b.parse::<UnitId>()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a.eq_ignore_ascii_case(b),
    }
}

/// The probe in the device set's slot, for as long as the test that started it runs. Letting go
/// of it empties the slot unless the daemon's stop has begun: the stop then closes it.
struct HeldProbe<'a> {
    set: &'a RwLock<DeviceSet>,
    handle: Arc<DeviceHandle>,
}

impl Drop for HeldProbe<'_> {
    fn drop(&mut self) {
        let mut set = written(self.set);
        if !set.stopping
            && set
                .probe
                .as_ref()
                .is_some_and(|probe| Arc::ptr_eq(probe, &self.handle))
        {
            set.probe = None;
        }
    }
}

impl Daemon {
    /// Start an actor for every unit the configuration names that has none: a reload that met
    /// the probe's unit left it out, and nothing else would start it before the next reload. It
    /// waits its turn behind a reload under way, which reads the running set and writes the next
    /// one, and starts nothing while another probe holds a unit.
    async fn start_unwatched(&self) {
        let _reloading = self.reloading.lock().await;
        let config = self.config_snapshot();
        let registry = self.registry();
        let started = {
            let mut set = written(&self.devices);
            if set.stopping {
                return;
            }
            let mut started = false;
            for wanted in &config.devices {
                let held = set
                    .devices
                    .iter()
                    .chain(&set.starting)
                    .any(|actor| actor.name.eq_ignore_ascii_case(&wanted.name));
                let probed = set
                    .probe
                    .as_ref()
                    .is_some_and(|probe| same_unit(&probe.udid, &wanted.udid));
                if held || probed {
                    continue;
                }
                if let Some(handle) = self.start(wanted, &registry) {
                    set.devices.push(handle);
                    started = true;
                }
            }
            started
        };
        if started {
            self.bridge_to(&config.mqtt, &config.presets).await;
        }
    }

    /// Start a probe for `config`'s unit and enter it in the set's slot, in one step under the
    /// set's lock. Refused once the stop has begun, for a unit an actor holds (a configured
    /// device's, or one a reload is starting), and while another probe runs.
    pub(super) fn start_probe(
        &self,
        config: DeviceConfig,
    ) -> Result<(Arc<DeviceHandle>, oneshot::Receiver<FirstAttempt>), ConnectFailure> {
        let mut set = written(&self.devices);
        if set.stopping {
            return Err(ConnectFailure::Stopping);
        }
        let held = set
            .devices
            .iter()
            .chain(&set.starting)
            .any(|actor| same_unit(&actor.udid, &config.udid));
        if held || set.probe.is_some() {
            return Err(ConnectFailure::Held);
        }
        let (handle, first) = device::spawn_probe(
            Arc::clone(&self.transport),
            self.registry(),
            config,
            Arc::clone(&self.scanner),
        )
        .map_err(|message| {
            log::error(&message);
            ConnectFailure::Unknown
        })?;
        let handle = Arc::new(handle);
        set.probe = Some(Arc::clone(&handle));
        Ok((handle, first))
    }

    /// Test `passphrase` (`auto`, `none` or four digits) on `unit`: a probe actor's first attempt
    /// says whether the device let the session in or turned the code away, or why no answer came.
    /// The profile is the one its advertisement selects, as for any connect: the connect does not
    /// depend on it. `None` when `cancelled` resolved first.
    ///
    /// The probe is asked to close before this answers (unless the daemon's stop has begun,
    /// which closes it), so the device is free for whatever connects next.
    pub(crate) async fn probe(
        &self,
        unit: &str,
        passphrase: &str,
        cancelled: impl Future<Output = ()>,
    ) -> Option<Connect> {
        let config = DeviceConfig {
            name: format!("probe {unit}"),
            udid: unit.to_string(),
            profile: None,
            passphrase: passphrase.to_string(),
            inputs: BTreeMap::new(),
        };
        let (handle, first) = match self.start_probe(config) {
            Ok(started) => started,
            Err(kind) => return Some(Connect::Failed { kind }),
        };
        let held = HeldProbe {
            set: &self.devices,
            handle: Arc::clone(&handle),
        };
        let answered = tokio::select! {
            first = tokio::time::timeout(PROBE_BOUND, first) => Some(match first {
                Ok(Ok(FirstAttempt::Connected { refused: false })) => Connect::Accepted,
                Ok(Ok(FirstAttempt::Connected { refused: true })) => Connect::Refused,
                Ok(Ok(FirstAttempt::Failed { heard: false })) => Connect::Failed {
                    kind: ConnectFailure::NotInRange,
                },
                Ok(Ok(FirstAttempt::Failed { heard: true })) | Err(_) => Connect::Failed {
                    kind: ConnectFailure::Link,
                },
                // The actor ended before its first attempt did: only the stop ends it so.
                Ok(Err(_)) => Connect::Failed {
                    kind: ConnectFailure::Stopping,
                },
            }),
            () = cancelled => None,
        };
        if !self.is_stopping() {
            self.stop(&handle).await;
        }
        drop(held);
        self.start_unwatched().await;
        answered
    }
}

#[cfg(test)]
mod tests;
