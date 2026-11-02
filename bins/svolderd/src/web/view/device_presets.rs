// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A device's own presets: the slots it keeps, which the page recalls and (for a viewer who may
//! write) saves the bands to, and the built-in settings it ships with, the one the bands read
//! marked on. A slot is never marked: the device does not say which one it holds.

use svolder_profiles::{Profile, device_presets, roles};

use super::Snapshot;

/// The device presets' part.
#[derive(Debug)]
pub(crate) struct DevicePresets {
    pub(crate) slots: Vec<SlotRow>,
    pub(crate) builtins: Vec<SettingRow>,
    /// Whether a slot may be recalled and a built-in setting applied from here: the viewer may
    /// write.
    pub(crate) recall: bool,
    /// Whether the slots offer a save: the viewer may write and the device has a save command.
    pub(crate) save: bool,
}

/// One slot's row.
#[derive(Debug)]
pub(crate) struct SlotRow {
    /// Its number, from 1.
    pub(crate) slot: u8,
    /// `Preset 2`.
    pub(crate) name: String,
    /// What its recall posts: `{"slot": 2}`.
    pub(crate) recall_json: String,
    /// What its save posts, once confirmed: `{"confirm": true, "slot": 2}`.
    pub(crate) save_json: String,
}

/// One built-in setting's row.
#[derive(Debug)]
pub(crate) struct SettingRow {
    pub(crate) name: String,
    /// Whether the bands read as this setting.
    pub(crate) on: bool,
    /// What its apply posts: `{"name": …}`.
    pub(crate) json: String,
}

/// The device's presets, when it has any of its own.
pub(crate) fn device_presets(s: &Snapshot, profile: &Profile) -> Option<DevicePresets> {
    if !device_presets::offered(profile) {
        return None;
    }
    let on = device_presets::matching(profile, |id| s.raw(id)).map(|setting| setting.name.as_str());
    let slots = roles::recall_action(profile).map_or(0, |action| action.slots);
    let writer = s.access.writes && !s.access.guest;
    Some(DevicePresets {
        slots: (1..=slots)
            .map(|slot| SlotRow {
                slot,
                name: device_presets::slot_name(slot),
                recall_json: serde_json::json!({ "slot": slot }).to_string(),
                save_json: serde_json::json!({ "slot": slot, "confirm": true }).to_string(),
            })
            .collect(),
        builtins: profile
            .builtin_settings
            .iter()
            .flat_map(|table| &table.settings)
            .map(|setting| SettingRow {
                name: setting.name.clone(),
                on: on == Some(setting.name.as_str()),
                json: serde_json::json!({ "name": setting.name }).to_string(),
            })
            .collect(),
        recall: writer,
        save: writer && roles::save_action(profile).is_some(),
    })
}
