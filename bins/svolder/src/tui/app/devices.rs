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
    /// The device a removal is waiting to be confirmed for, while one is.
    pub confirm: Option<String>,
}
