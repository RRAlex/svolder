// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The devices screen's state: the rows to switch between, what the configuration file names,
//! the editor over the list and the removal waiting to be confirmed.

use svolder_ipc::DeviceConfig;

use crate::backend::DeviceRow;
use crate::tui::boxes::editor::DeviceEditor;

/// The devices screen's state: the rows to switch between, what the configuration file names,
/// the editor over the list and the removal waiting to be confirmed.
pub struct DevicesScreen {
    /// The devices to switch between.
    pub rows: Vec<DeviceRow>,
    /// The devices the configuration file names, as it names them, which the loop re-reads
    /// after every write. This is what the editor edits; the rows above are what the list
    /// shows, link states and all.
    pub configured: Vec<DeviceConfig>,
    /// The device the configuration file names as the one a command acts on when it names
    /// none, if it names one.
    pub default_device: Option<String>,
    /// The device being edited over the list, while one is.
    pub editor: Option<DeviceEditor>,
    /// The record the editor last opened on: what a save is measured against, whatever the list
    /// has been refreshed to since.
    pub opened: Option<DeviceConfig>,
    /// The device a removal is waiting to be confirmed for, while one is.
    pub confirm: Option<String>,
    /// The adapter hint a direct session's failed open carried: the daemon's rows say theirs
    /// themselves, and a direct session has no rows that do.
    pub landed_hint: Option<svolder_ipc::AdapterHint>,
    /// The device whose adapter step `Esc` put away. It stays away while the hint lasts and is
    /// forgotten once the hint is gone, so the step comes back with the next one.
    pub dismissed: Option<String>,
}

impl DevicesScreen {
    /// The adapter hint the Devices screen offers a switch from: a device's row that carries
    /// one, else the one a failed open of this session carried, unless `Esc` put its device's
    /// step away.
    pub fn hint(&self) -> Option<&svolder_ipc::AdapterHint> {
        self.rows
            .iter()
            .filter_map(|row| row.adapter_hint.as_ref())
            .chain(self.landed_hint.as_ref())
            .find(|hint| self.dismissed.as_deref() != Some(hint.device.as_str()))
    }

    /// The adapter a row's device is being tried on, while its connects keep failing there and
    /// another adapter hears it: its own row says so under a daemon, and in a direct session the
    /// failed open's hint names the device it was for.
    pub fn tried_on<'a>(&'a self, row: &'a DeviceRow) -> Option<&'a str> {
        row.adapter_hint
            .as_ref()
            .or(self
                .landed_hint
                .as_ref()
                .filter(|hint| hint.device == row.name))
            .map(|hint| hint.failing.as_str())
    }

    /// The hint the row under the cursor carries, unless `Esc` put its step away.
    pub fn selected_hint(&self, selected: usize) -> Option<&svolder_ipc::AdapterHint> {
        self.rows
            .get(selected)?
            .adapter_hint
            .as_ref()
            .filter(|hint| self.dismissed.as_deref() != Some(hint.device.as_str()))
    }

    /// Forget a dismissal whose hint has gone, so the step is offered again when it comes back.
    pub fn forget_cleared_dismissal(&mut self) {
        let Some(device) = self.dismissed.as_deref() else {
            return;
        };
        let held = self
            .rows
            .iter()
            .filter_map(|row| row.adapter_hint.as_ref())
            .chain(self.landed_hint.as_ref())
            .any(|hint| hint.device == device);
        if !held {
            self.dismissed = None;
        }
    }
}
