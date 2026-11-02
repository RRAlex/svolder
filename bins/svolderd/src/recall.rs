// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Recalling a tone-control preset, or applying a built-in setting, on a device the daemon holds:
//! the part the MCP server's `load_preset` and the MQTT bridge's preset select share. Both find
//! the preset among the ones that fit the device (or the setting among the device's own) and plan
//! the writes from the device's current readings; this module holds the finding, the planning and
//! the words, and the device's writer ([`crate::writer`]) sends the commands in order, stopping at
//! the first that fails.

use svolder_profiles::device_presets::{self, SettingWrite};
use svolder_profiles::tone::preset::{self, EqCommand, Preset, Recall, RecallError};
use svolder_profiles::{Profile, ProfileError};
use svolder_proto::FunctionId;

use crate::device::Command;

/// A preset found among the ones that fit a device, and the writes that recall it.
pub struct Recalled {
    /// The preset, under the name the configuration gives it.
    pub preset: Preset,
    /// What recalling it sends, and what it had to move to fit.
    pub recall: Recall,
}

/// Find the preset `name` names (blanks and case aside) among `fitting`, the presets that fit
/// `device`, and plan its recall from the raw readings `raw` gives. The error is the sentence
/// a front end reports: no such preset, listing the ones that fit, or why the plan cannot be
/// made.
pub fn plan(
    device: &str,
    profile: &Profile,
    fitting: &[Preset],
    name: &str,
    raw: impl Fn(FunctionId) -> Option<i64>,
) -> Result<Recalled, String> {
    preset::recall_named(profile, fitting, name, raw)
        .map(|(preset, recall)| Recalled { preset, recall })
        .map_err(|why| refusal(device, &why))
}

/// A built-in setting found on a device, and the writes that put the device on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The setting, under the name the device lists it by.
    pub name: String,
    /// What applying it sends, in order; empty when the device is already on it.
    pub writes: Vec<SettingWrite>,
}

/// Find the built-in setting `name` names (blanks and case aside) on `device` and plan the writes
/// that put it there from the raw readings `raw` gives. The error is the sentence a front end
/// reports: the device has no built-in settings, or none of that name, listing the ones it has.
pub fn plan_setting(
    device: &str,
    profile: &Profile,
    name: &str,
    raw: impl Fn(FunctionId) -> Option<i64>,
) -> Result<Applied, String> {
    device_presets::plan(profile, name, raw)
        .map(|plan| Applied {
            name: plan.name,
            writes: plan.writes,
        })
        .map_err(|why| match why {
            ProfileError::NoDevicePresets { tone_control, .. } => ProfileError::NoDevicePresets {
                device: device.to_string(),
                tone_control,
            }
            .to_string(),
            other => other.to_string(),
        })
}

/// A recall that cannot be made, in a front end's words.
fn refusal(device: &str, why: &RecallError) -> String {
    match why {
        RecallError::NotFound { name, fitting } => format!(
            "no preset named {name:?} fits {device}; the presets are: {}",
            fitting.join(", ")
        ),
        RecallError::Plan(e) => e.to_string(),
    }
}

/// What a recall that went wrong partway reports: `sent` of its `total` writes went through
/// before `why`.
pub fn stopped_after(sent: usize, total: usize, why: &str) -> String {
    format!("stopped after {sent} of {total} writes: {why}")
}

/// The actor command that carries one recall command out, as a `set`, an `eq set`, an `eq mode`
/// or, for a loudness strength the profile names no control for, a raw write.
pub fn command(command: &EqCommand) -> Command {
    match command {
        EqCommand::Band { band, param, value } => Command::EqSet {
            band: *band,
            param: param.to_string(),
            value: *value,
            no_wait: false,
        },
        EqCommand::Mode(mode) => Command::EqMode {
            mode: mode.to_string(),
            no_wait: false,
        },
        EqCommand::Loudness {
            key: Some(key),
            raw,
            ..
        } => Command::Set {
            key: key.clone(),
            value: raw.to_string(),
            step: 0,
            no_wait: false,
        },
        EqCommand::Loudness { id, raw, .. } => Command::Raw {
            id: id.to_string(),
            value_type: "u8".to_string(),
            value: raw.to_string(),
            no_wait: false,
        },
    }
}

/// The actor command that carries one write of a built-in setting out: the step, written to the
/// id the setting's table names, and waited for.
pub fn setting_command(write: &SettingWrite) -> Command {
    Command::Raw {
        id: write.id.to_string(),
        value_type: "u8".to_string(),
        value: write.raw.to_string(),
        no_wait: false,
    }
}

#[cfg(test)]
mod tests;
