// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The walk's effects in the daemon: its scan is a listen of the shared scanner's, its
//! passphrase test a probe actor, and its write the `device_config` add, through the same
//! writer and reload as every device write.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use svolder_ble::Advertisement;
use svolder_ipc::{
    ConfiguredDevice, Connect, DeviceConfig, DeviceConfigOp, Reason, Response, ScanEntry,
};
use svolder_profiles::Registry;
use svolder_proto::UnitId;
use svolder_walk::{Heard, ROWS_MAX, Row, WalkEffects, WalkState, Working, WriteError};

use super::super::Daemon;
use super::super::jobs::JobCtx;
use crate::locks::locked;

/// How long a walk's scan listens.
pub(crate) const WALK_SCAN_WINDOW: Duration = Duration::from_secs(10);

/// What a walk's effect that did not happen hands back.
#[derive(Debug)]
pub(crate) enum DaemonFailure {
    /// The walk was cancelled while the effect ran.
    Cancelled,
    /// The effect failed, in the daemon's words and with the reason a front end matches on.
    Failed {
        /// what went wrong
        message: String,
        /// why
        reason: Reason,
    },
}

impl DaemonFailure {
    /// The words a refusal shows for this failure.
    pub(crate) fn words(&self) -> String {
        match self {
            DaemonFailure::Cancelled => "the walk was cancelled".to_string(),
            DaemonFailure::Failed { message, .. } => message.clone(),
        }
    }
}

/// Where a walk last said it stands, which its effects add to as they work: the rows a scan
/// hears, and what it works on.
pub(crate) struct Board {
    ctx: Arc<JobCtx>,
    state: Mutex<Option<WalkState>>,
}

impl Board {
    /// A board with nothing said yet.
    pub(crate) fn new(ctx: Arc<JobCtx>) -> Board {
        Board {
            ctx,
            state: Mutex::new(None),
        }
    }

    /// Say `state`, whole.
    pub(crate) fn show(&self, state: WalkState) {
        let mut held = locked(&self.state);
        self.ctx
            .progress(serde_json::to_value(&state).unwrap_or_default());
        *held = Some(state);
    }

    /// Change what was last said and say it again.
    fn change(&self, change: impl FnOnce(&mut WalkState)) {
        let mut held = locked(&self.state);
        if let Some(state) = held.as_mut() {
            change(state);
            self.ctx
                .progress(serde_json::to_value(&*state).unwrap_or_default());
        }
    }
}

/// The daemon as a walk's effects.
pub(crate) struct DaemonEffects {
    daemon: Arc<Daemon>,
    ctx: Arc<JobCtx>,
    board: Arc<Board>,
    /// The profiles as they stood when the walk started.
    registry: Arc<Registry>,
    /// The device the write configured, as `device_config` answered it.
    written: Mutex<Option<ConfiguredDevice>>,
}

impl DaemonEffects {
    /// The effects of a walk run as the job `ctx`, saying where it stands on `board`.
    pub(crate) fn new(daemon: Arc<Daemon>, ctx: Arc<JobCtx>, board: Arc<Board>) -> DaemonEffects {
        DaemonEffects {
            registry: daemon.registry(),
            daemon,
            ctx,
            board,
            written: Mutex::new(None),
        }
    }

    /// The device the walk wrote, once it has, as `device_config` answered it.
    pub(crate) fn written(&self) -> Option<ConfiguredDevice> {
        locked(&self.written).clone()
    }

    /// The configured units, parsed, with their devices' names.
    fn units(&self) -> Vec<(String, UnitId)> {
        self.taken_units()
            .into_iter()
            .filter_map(|(name, unit)| Some((name, unit.parse().ok()?)))
            .collect()
    }

    /// The row an advertisement makes, marked with the configured name of its unit.
    fn row(&self, advert: &Advertisement, units: &[(String, UnitId)]) -> Row {
        Row {
            entry: ScanEntry::from_advert(&advert.manufacturer, advert.rssi, &self.registry),
            configured: units
                .iter()
                .find(|(_, unit)| *unit == advert.unit_id())
                .map(|(name, _)| name.clone()),
        }
    }
}

impl WalkEffects for DaemonEffects {
    type Failure = DaemonFailure;

    /// One listen of the shared scanner's that hears every device, each row going on the board
    /// as it is heard; the first [`ROWS_MAX`] units are offered.
    async fn scan(&self) -> Result<Heard, DaemonFailure> {
        let units = self.units();
        self.board.change(|state| state.rows.clear());
        let each = |advert: &Advertisement| {
            let row = self.row(advert, &units);
            self.board.change(|state| {
                if state.rows.len() < ROWS_MAX {
                    state.rows.push(row);
                }
            });
        };
        let heard = tokio::select! {
            heard = self.daemon.listen_all(WALK_SCAN_WINDOW, &each) => heard,
            () = self.ctx.cancelled() => return Err(DaemonFailure::Cancelled),
        };
        let adverts = heard.map_err(|e| DaemonFailure::Failed {
            message: e.to_string(),
            reason: Reason::Internal,
        })?;
        Ok(Heard {
            entries: adverts
                .iter()
                .take(ROWS_MAX)
                .map(|advert| self.row(advert, &units).entry)
                .collect(),
            notices: Vec::new(),
        })
    }

    fn scan_window(&self) -> Duration {
        WALK_SCAN_WINDOW
    }

    /// A probe actor's first attempt on the unit `seen` names.
    async fn try_connect(
        &self,
        seen: &ScanEntry,
        passphrase: &str,
    ) -> Result<Connect, DaemonFailure> {
        self.board
            .change(|state| state.working = Some(Working::Testing));
        self.daemon
            .probe(&seen.unit, passphrase, self.ctx.cancelled())
            .await
            .ok_or(DaemonFailure::Cancelled)
    }

    /// The `device_config` add: a name taken sends the walk back to its name, a unit another
    /// block took (the writer's strict parse, `invalid`, while the snapshot now holds the unit)
    /// to its pick; any other refusal (`invalid` of the file, `read_only`, `file_busy`,
    /// `stopping`) stops it.
    async fn write_device(&self, device: &DeviceConfig) -> Result<(), WriteError<DaemonFailure>> {
        self.board
            .change(|state| state.working = Some(Working::Writing));
        let op = DeviceConfigOp::Add {
            unit: device.udid.clone(),
            name: device.name.clone(),
            profile: device.profile.clone(),
            passphrase: Some(device.passphrase.clone()),
        };
        match self.daemon.device_config(op).await {
            Response::DeviceConfig { devices, .. } => {
                *locked(&self.written) = devices
                    .into_iter()
                    .find(|written| super::super::probe::same_unit(&written.unit, &device.udid));
                Ok(())
            }
            Response::Error {
                reason: Some(Reason::Taken),
                ..
            } => Err(WriteError::NameTaken(device.name.clone())),
            Response::Error {
                message,
                reason: Some(Reason::BadName),
                ..
            } => Err(WriteError::BadName(message)),
            // The walk judged the name by the writers' own rule before the write, so `invalid` is
            // the unit (another block took it) or the file itself (one unit twice, a profile that
            // went away), which stops the walk.
            Response::Error {
                message,
                reason: Some(Reason::Invalid),
                ..
            } if self
                .taken_units()
                .iter()
                .any(|(_, unit)| super::super::probe::same_unit(unit, &device.udid)) =>
            {
                Err(WriteError::UnitConfigured(message))
            }
            Response::Error {
                message, reason, ..
            } => Err(WriteError::Stop(DaemonFailure::Failed {
                message,
                reason: reason.unwrap_or(Reason::Internal),
            })),
            other => Err(WriteError::Stop(DaemonFailure::Failed {
                message: format!("the write answered {other:?}"),
                reason: Reason::Internal,
            })),
        }
    }

    fn taken_names(&self) -> Vec<String> {
        self.daemon
            .config_snapshot()
            .devices
            .iter()
            .map(|device| device.name.clone())
            .collect()
    }

    fn taken_units(&self) -> Vec<(String, String)> {
        self.daemon
            .config_snapshot()
            .devices
            .iter()
            .map(|device| (device.name.clone(), device.udid.clone()))
            .collect()
    }

    fn registry(&self) -> Option<&Registry> {
        Some(&self.registry)
    }

    /// The configured devices whose actor holds a link now.
    fn linked(&self) -> Vec<String> {
        self.daemon
            .devices()
            .iter()
            .filter(|device| device.describe().connected)
            .map(|device| device.name.clone())
            .collect()
    }
}
