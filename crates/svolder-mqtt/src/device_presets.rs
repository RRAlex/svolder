// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A device's own presets over MQTT: one preset select on the key `eq_preset`, the key a tone
//! control's select uses, so a dashboard finds the device's presets under one key whichever kind
//! the device has. A device with a tone control keeps that select ([`crate::eq`]); a device with
//! slots and built-in settings of its own, a Loki Max, gets this one instead.
//!
//! The select offers the slots by name (`Preset 1`), then the built-in settings, then
//! [`CUSTOM`]. Its state is the built-in setting the device's bands read, [`CUSTOM`] when they
//! match none, and nothing while a band is unread; a slot is never the state, because the device
//! does not say which it was last set from. Saving to a slot is not offered.

use serde_json::{Value as Json, json};

use svolder_profiles::device_presets;
use svolder_profiles::tone::preset::same_name;
use svolder_profiles::{Profile, ProfileError, roles};
use svolder_proto::Message;

use crate::discovery::{Device, DiscoveryEntry, entity_envelope};
use crate::eq::{self, raw_of};
use crate::topics;

/// The key the preset select is exposed under, the tone control's own.
pub const PRESET_KEY: &str = eq::PRESET_KEY;
/// The select's option, and state, for bands no built-in setting matches.
pub const CUSTOM: &str = eq::CUSTOM;

/// The select's label.
const LABEL: &str = "Preset";

/// A preset command parsed from an MQTT payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Recall this slot.
    Slot(u8),
    /// Apply the built-in setting of this name, as the profile spells it.
    Setting(String),
}

/// Whether `key` is the preset select of `profile`, matched without regard to case. A profile
/// with a tone control has none of its own: that control's select owns the key.
pub fn target(profile: &Profile, key: &str) -> bool {
    offers(profile) && key.eq_ignore_ascii_case(PRESET_KEY)
}

/// Whether the select is the profile's: it has presets of its own and no tone control.
fn offers(profile: &Profile) -> bool {
    profile.eq.is_none() && device_presets::offered(profile)
}

/// The slots the profile recalls from, numbered from 1.
fn slots(profile: &Profile) -> u8 {
    roles::recall_action(profile).map_or(0, |action| action.slots)
}

/// The names of the slots then the built-in settings, in the order the select lists them.
fn names(profile: &Profile) -> Vec<String> {
    (1..=slots(profile))
        .map(device_presets::slot_name)
        .chain(
            profile
                .builtin_settings
                .iter()
                .flat_map(|table| table.settings.iter().map(|setting| setting.name.clone())),
        )
        .collect()
}

/// The select's options: each slot, each built-in setting, then [`CUSTOM`]. A setting named
/// `custom` in any case is left out, since the option stands for no preset at all. Empty for a
/// profile that has no select of this kind.
pub fn options(profile: &Profile) -> Vec<String> {
    if !offers(profile) {
        return Vec::new();
    }
    names(profile)
        .into_iter()
        .filter(|name| !same_name(name, CUSTOM))
        .chain(std::iter::once(CUSTOM.to_string()))
        .collect()
}

/// The discovery entry of the preset select: a `select` with `commands`, a `sensor` without.
/// Nothing for a profile that has no select of this kind.
pub(crate) fn entries(
    base_topic: &str,
    discovery_prefix: &str,
    profile: &Profile,
    device: &Device<'_>,
    commands: bool,
) -> Vec<DiscoveryEntry> {
    if !offers(profile) {
        return Vec::new();
    }
    let component = if commands { "select" } else { "sensor" };
    let topic = topics::discovery_config(discovery_prefix, component, device.unit_id, PRESET_KEY);
    let mut payload = entity_envelope(base_topic, profile, device, LABEL, PRESET_KEY);
    payload.insert(
        "state_topic".into(),
        topics::state(base_topic, device.unit_id).into(),
    );
    payload.insert(
        "value_template".into(),
        format!("{{{{ value_json.{PRESET_KEY} }}}}").into(),
    );
    if commands {
        payload.insert(
            "command_topic".into(),
            topics::command(base_topic, device.unit_id, PRESET_KEY).into(),
        );
        payload.insert(
            "options".into(),
            Json::Array(options(profile).into_iter().map(Json::from).collect()),
        );
    }
    vec![DiscoveryEntry {
        topic,
        payload: Json::Object(payload),
    }]
}

/// The select's state for the device: `None` until every band a built-in setting sets has been
/// read; then the built-in setting those readings match, else [`CUSTOM`]. A setting named
/// `custom` never stands for itself. `None` too for a profile that has no select of this kind.
pub fn state(profile: &Profile, readings: &[Message]) -> Option<String> {
    if !offers(profile) {
        return None;
    }
    // A profile with slots and no table has no setting to match: nothing to say either way.
    let table = profile.builtin_settings.as_ref()?;
    let raw = raw_of(readings);
    let unread = table.keys.iter().any(|key| {
        profile
            .control_by_key(key)
            .is_none_or(|control| raw(control.id).is_none())
    });
    if unread {
        return None;
    }
    Some(match device_presets::matching(profile, raw) {
        Some(setting) if !same_name(&setting.name, CUSTOM) => setting.name.clone(),
        _ => CUSTOM.to_string(),
    })
}

/// Add the select's state to `map` once it has one, as the published state carries it.
pub(crate) fn add_state(
    profile: &Profile,
    readings: &[Message],
    map: &mut serde_json::Map<String, Json>,
) {
    if let Some(chosen) = state(profile, readings) {
        map.insert(PRESET_KEY.to_string(), json!(chosen));
    }
}

/// Parse `payload` as a pick: a slot by name or number, or a built-in setting by name, blanks
/// and case aside. [`CUSTOM`] names nothing to send and is refused, as is anything else the
/// device does not list.
pub fn parse(profile: &Profile, payload: &str) -> Result<Command, ProfileError> {
    let text = payload.trim();
    let refused = || ProfileError::NoSetting {
        name: text.to_string(),
        names: names(profile),
    };
    if !offers(profile) || same_name(text, CUSTOM) {
        return Err(refused());
    }
    if let Some(slot) = device_presets::parse_slot(text, slots(profile)) {
        return Ok(Command::Slot(slot));
    }
    device_presets::find(profile, text)
        .map(|setting| Command::Setting(setting.name.clone()))
        .ok_or_else(refused)
}

#[cfg(test)]
mod tests;
