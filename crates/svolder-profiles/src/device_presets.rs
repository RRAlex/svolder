// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A device's own presets: its slots, and the named settings it ships with.
//!
//! These differ from the tone-control presets in [`crate::tone::preset`]: a slot lives on the
//! device and is recalled or saved by a command with no value, and a built-in setting is a row of
//! raw steps from the profile, not a curve in physical units. Every front end plans a built-in
//! setting, finds the one a device is on and words a slot here, so none of them does it itself.

use svolder_proto::FunctionId;

use crate::error::ProfileError;
use crate::model::{BuiltinSetting, Control, ControlKind, Profile};
use crate::roles;
use crate::tone::preset::same_name;
use crate::units;

/// One write of a built-in setting: a control's raw step, sent to the id the table names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingWrite {
    /// The key of the control the write sets.
    pub key: String,
    /// The id the step is written to.
    pub id: FunctionId,
    /// The raw step.
    pub raw: i64,
}

/// The writes that put a device on a built-in setting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingPlan {
    /// The setting's name as the device lists it.
    pub name: String,
    /// The writes, in the order to send them; empty when the device is already on the setting.
    pub writes: Vec<SettingWrite>,
}

/// Whether the profile describes presets of the device's own: built-in settings, or slots to
/// recall.
pub fn offered(profile: &Profile) -> bool {
    profile.builtin_settings.is_some() || roles::recall_action(profile).is_some()
}

/// What a slot's name starts with, before its number.
const PREFIX: &str = "Preset ";

/// The name of slot `slot`: `Preset 2`.
pub fn slot_name(slot: u8) -> String {
    format!("{PREFIX}{slot}")
}

/// The slot `text` names, `2` or `Preset 2` (blanks and case aside), when it is one of
/// `1..=slots`.
pub fn parse_slot(text: &str, slots: u8) -> Option<u8> {
    let text = text.trim();
    let number = match text.get(..PREFIX.len()) {
        Some(head) if head.eq_ignore_ascii_case(PREFIX) => text[PREFIX.len()..].trim_start(),
        _ => text,
    };
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    number
        .parse::<u8>()
        .ok()
        .filter(|slot| (1..=slots).contains(slot))
}

/// The built-in setting called `name`, blanks and case aside.
pub fn find<'a>(profile: &'a Profile, name: &str) -> Option<&'a BuiltinSetting> {
    profile
        .builtin_settings
        .as_ref()?
        .settings
        .iter()
        .find(|setting| same_name(&setting.name, name))
}

/// The writes that put the device on the built-in setting `name`, given `raw`, the raw step a
/// control id last read (`None` while it has not been read).
///
/// The writes go in an order that never sounds a boost first: every control going down, in the
/// table's order, then every control going up. A control already on its target is left out; one
/// that has not been read is written, with the cuts when its target is at or below the step that
/// reads zero (a device shows zero before it reports) and with the boosts otherwise.
pub fn plan(
    profile: &Profile,
    name: &str,
    raw: impl Fn(FunctionId) -> Option<i64>,
) -> Result<SettingPlan, ProfileError> {
    let table = profile
        .builtin_settings
        .as_ref()
        .ok_or_else(|| ProfileError::NoDevicePresets {
            device: profile.display.clone(),
            tone_control: profile.eq.is_some(),
        })?;
    let setting = find(profile, name).ok_or_else(|| ProfileError::NoSetting {
        name: name.to_string(),
        names: table.settings.iter().map(|s| s.name.clone()).collect(),
    })?;
    let (mut down, mut up) = (Vec::new(), Vec::new());
    for ((key, id), &target) in table.keys.iter().zip(&table.ids).zip(&setting.raw) {
        let Some(control) = profile.control_by_key(key) else {
            continue;
        };
        let current = raw(control.id);
        if current == Some(target) {
            continue;
        }
        let lowers = match current {
            Some(current) => target < current,
            None => zero_step(control).is_none_or(|zero| target <= zero),
        };
        let write = SettingWrite {
            key: control.key.clone(),
            id: *id,
            raw: target,
        };
        if lowers { &mut down } else { &mut up }.push(write);
    }
    down.append(&mut up);
    Ok(SettingPlan {
        name: setting.name.clone(),
        writes: down,
    })
}

/// The built-in setting every control it sets reads, the first in the table's order when two do.
/// `None` while any of those controls has not been read.
pub fn matching(
    profile: &Profile,
    raw: impl Fn(FunctionId) -> Option<i64>,
) -> Option<&BuiltinSetting> {
    let table = profile.builtin_settings.as_ref()?;
    let readings: Vec<i64> = table
        .keys
        .iter()
        .map(|key| profile.control_by_key(key).and_then(|c| raw(c.id)))
        .collect::<Option<_>>()?;
    table
        .settings
        .iter()
        .find(|setting| setting.raw == readings)
}

/// The highest value, in dB, any level the recall action changes can reach: what a recall can
/// raise a band to. Only levels whose unit is dB are considered; a level in any other unit, or
/// with none, is ignored. `None` when the profile has no recall action, or it changes no dB level.
pub fn recall_ceiling_db(profile: &Profile) -> Option<f64> {
    let recall = roles::recall_action(profile)?;
    recall
        .changes
        .iter()
        .filter_map(|key| profile.control_by_key(key))
        .filter(|control| {
            control.kind == ControlKind::Level
                && control
                    .unit
                    .as_deref()
                    .is_some_and(|unit| unit.eq_ignore_ascii_case("dB"))
        })
        .filter_map(|control| {
            let (lo, hi) = control.range?;
            (lo..=hi)
                .filter_map(|raw| units::amount(control, raw)?.number())
                .max_by(f64::total_cmp)
        })
        .max_by(f64::total_cmp)
}

/// Whether `id` is a control a built-in setting sets, or an id it writes to.
pub fn rewrites(profile: &Profile, id: FunctionId) -> bool {
    let Some(table) = &profile.builtin_settings else {
        return false;
    };
    table.ids.contains(&id)
        || table
            .keys
            .iter()
            .filter_map(|key| profile.control_by_key(key))
            .any(|control| control.id == id)
}

/// The raw step of `control` whose value is nearest zero; the lower raw when two are equally near.
fn zero_step(control: &Control) -> Option<i64> {
    let (lo, hi) = control.range?;
    (lo..=hi)
        .filter_map(|raw| roles::step_value(control, raw).map(|v| (raw, v.abs())))
        .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)))
        .map(|(raw, _)| raw)
}

#[cfg(test)]
mod tests;
