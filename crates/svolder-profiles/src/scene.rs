// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device side of a scene: which controls a scene holds, a device's part captured from its
//! readings, the plan that recalls it and whether it is on. Pure functions over a profile and
//! readings; no I/O and no clock.

use std::fmt;

use svolder_proto::{FunctionId, Message, Value};

use crate::device_presets::{self, SettingWrite};
use crate::meter;
use crate::model::{Control, ControlKind, Profile};
use crate::roles;
use crate::tone::preset::{self, On, Preset};

/// One device's part of a scene: its tone and its plain sound settings at save, by unit id.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DevicePart {
    /// The unit id the part belongs to, as the device's configuration holds it.
    pub udid: String,
    /// The device's configured name at save, for "X is not connected".
    pub name: String,
    /// A device with a tone control: its curve with no source, named after the preset that was
    /// on, or "Custom".
    pub tone: Option<Preset>,
    /// A device with built-in settings and no tone control: each setting's key and raw step.
    pub raw_bands: Option<Vec<(String, i64)>>,
    /// The held controls' keys, each with its raw step, in profile order.
    pub controls: Vec<(String, i64)>,
}

/// The reader of a device's raw steps a scene is made with: `readings` as the daemon holds them,
/// a function's step as an integer. A toggle reads as `1` or `0` (a device reports it as a
/// boolean, which has no integer form of its own), every other control as its integer step;
/// `None` while the function has not been read. [`capture`], [`plan`] and [`fn@matches`] judge
/// toggles by these steps, so every caller reads through this.
pub fn raw_of(readings: &[Message]) -> impl Fn(FunctionId) -> Option<i64> + '_ {
    move |id| {
        readings
            .iter()
            .find(|reading| reading.id == id)
            .and_then(|reading| match &reading.value {
                Value::Bool(on) => Some(i64::from(*on)),
                other => other.as_i64(),
            })
    }
}

/// What a capture could not read: the part is not taken while a reading is missing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unread {
    /// What has not been read: `the tone control`, or a control's key.
    pub what: String,
}

impl fmt::Display for Unread {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} has not been read yet", self.what)
    }
}

impl std::error::Error for Unread {}

/// What a recall of a [`DevicePart`] does to one device now.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct DevicePlan {
    /// The tone curve for the writer's recall, which orders its writes; `None` when the device
    /// is already on it, has none, or the part's curve does not fit.
    pub tone: Option<Preset>,
    /// The built-in settings' bands that change, as key and raw step, cuts first; `None` when
    /// none changes.
    pub raw: Option<Vec<(String, i64)>>,
    /// The held controls that change, as key and raw step, in profile order.
    pub sets: Vec<(String, i64)>,
    /// What was left as it is, each in words naming the setting.
    pub skipped: Vec<String>,
}

/// The controls of `profile` a scene holds, in profile order: every select, toggle or level
/// that is not the volume, an output gain, a power or mute switch, a control that asks before a
/// change or turns itself off, not the amps' meter switch (reporting, not sound), not one of
/// the tone control's own and not one of the built-in settings' bands (a part keeps those
/// apart). A readout, meter or fault is never held.
///
/// This is the one rule for which settings a scene holds; no front end lists keys itself.
pub fn held_controls(profile: &Profile) -> Vec<&Control> {
    profile
        .controls
        .iter()
        .filter(|control| {
            control.kind.writable()
                && !roles::is_volume(control)
                && !control.output_gain
                && !control.powers_output
                && !control.mutes_output
                && !control.asks_on_change
                && !control.auto_off
                && !control.key.eq_ignore_ascii_case(meter::POWER_METERS)
                && !profile.eq.as_ref().is_some_and(|eq| eq.owns(control.id))
                && !is_raw_band(profile, &control.key)
        })
        .collect()
}

/// Whether `key` is one of the built-in settings' controls.
fn is_raw_band(profile: &Profile, key: &str) -> bool {
    profile
        .builtin_settings
        .as_ref()
        .is_some_and(|table| table.keys.iter().any(|own| own.eq_ignore_ascii_case(key)))
}

/// The part of a scene for the device `profile` describes, read through `raw`: the tone curve
/// when the profile has a tone control, named after the preset of `fitting` that is on
/// (`recalled`, the one last recalled, first when it still matches) and `Custom` when none is;
/// the built-in settings' bands when it has those and no tone control; and every held control's
/// raw step (a toggle's is 0 or 1, as [`raw_of`] reads it). `udid` and `name` are the unit id and
/// the configured name. A missing reading is [`Unread`], naming it.
pub fn capture(
    profile: &Profile,
    udid: &str,
    name: &str,
    fitting: &[Preset],
    recalled: Option<&str>,
    raw: impl Fn(FunctionId) -> Option<i64>,
) -> Result<DevicePart, Unread> {
    let unread = |what: &str| Unread {
        what: what.to_string(),
    };
    let tone = match &profile.eq {
        Some(eq) => {
            let mut curve = preset::from_readings(eq, &raw, preset::CUSTOM)
                .ok_or_else(|| unread("the tone control"))?;
            curve.name = match preset::on(eq, &raw, fitting, recalled) {
                On::Preset(on) => on.name.clone(),
                On::Unread | On::Custom => "Custom".to_string(),
            };
            Some(curve)
        }
        None => None,
    };
    let raw_bands = match (&profile.eq, &profile.builtin_settings) {
        (None, Some(table)) => Some(
            table
                .keys
                .iter()
                .map(|key| {
                    let step = profile
                        .control_by_key(key)
                        .and_then(|control| raw(control.id))
                        .ok_or_else(|| unread(key))?;
                    Ok((key.clone(), step))
                })
                .collect::<Result<Vec<_>, Unread>>()?,
        ),
        _ => None,
    };
    let controls = held_controls(profile)
        .into_iter()
        .map(|control| {
            let step = raw(control.id).ok_or_else(|| unread(&control.key))?;
            Ok((control.key.clone(), step))
        })
        .collect::<Result<Vec<_>, Unread>>()?;
    Ok(DevicePart {
        udid: udid.to_string(),
        name: name.to_string(),
        tone,
        raw_bands,
        controls,
    })
}

/// Why `step` is not a value `control` takes, in words naming it; `None` when it is one.
fn refusal(control: &Control, step: i64) -> Option<String> {
    let key = &control.key;
    match control.kind {
        ControlKind::Select => u8::try_from(step)
            .ok()
            .is_none_or(|value| !control.values.iter().any(|(own, _)| *own == value))
            .then(|| format!("{key}: {step} is not one of its values")),
        ControlKind::Toggle => {
            (step != 0 && step != 1).then(|| format!("{key}: {step} is not on or off"))
        }
        _ => {
            let (low, high) = control.range?;
            (!(low..=high).contains(&step))
                .then(|| format!("{key}: {step} is outside its range {low} to {high}"))
        }
    }
}

/// What recalling `part` onto the device `profile` describes does, given its readings `raw`:
/// the tone curve for the writer's recall, the built-in bands that change (cuts first, as
/// [`device_presets::plan_values`] orders them), the held controls that change, and what was left
/// as it is and why. `raw` reads a toggle as 0 or 1 ([`raw_of`]). A setting the profile no longer
/// holds, a value it does not take, a level outside its range, a curve of another band count:
/// skipped and named, never clamped.
pub fn plan(
    profile: &Profile,
    part: &DevicePart,
    raw: impl Fn(FunctionId) -> Option<i64>,
) -> DevicePlan {
    let mut plan = DevicePlan::default();
    match (&part.tone, &profile.eq) {
        (Some(tone), Some(eq)) => match preset::plan_recall(eq, &raw, tone) {
            Ok(writes) if writes.writes.is_empty() => {}
            Ok(_) => plan.tone = Some(tone.clone()),
            Err(why) => plan.skipped.push(why.to_string()),
        },
        (Some(_), None) => plan
            .skipped
            .push("tone: this device has no tone control now".to_string()),
        _ => {}
    }
    if let Some(bands) = &part.raw_bands {
        let mut targets = Vec::new();
        for (key, step) in bands {
            match profile
                .control_by_key(key)
                .filter(|_| is_raw_band(profile, key))
            {
                None => plan
                    .skipped
                    .push(format!("{key}: this device no longer has it")),
                Some(control) => match refusal(control, *step) {
                    Some(why) => plan.skipped.push(why),
                    None => targets.push((key.clone(), *step)),
                },
            }
        }
        let writes: Vec<SettingWrite> = device_presets::plan_values(profile, &targets, &raw);
        if !writes.is_empty() {
            plan.raw = Some(writes.into_iter().map(|w| (w.key, w.raw)).collect());
        }
    }
    let held = held_controls(profile);
    for control in &held {
        let Some((_, step)) = part
            .controls
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(&control.key))
        else {
            continue;
        };
        if let Some(why) = refusal(control, *step) {
            plan.skipped.push(why);
        } else if raw(control.id) != Some(*step) {
            plan.sets.push((control.key.clone(), *step));
        }
    }
    for (key, _) in &part.controls {
        if !held
            .iter()
            .any(|control| control.key.eq_ignore_ascii_case(key))
        {
            plan.skipped
                .push(format!("{key}: this device no longer holds it in a scene"));
        }
    }
    plan
}

/// Whether the device is on `part`: the tone curve by [`preset::matches`], and the built-in bands
/// and every held control by raw step (a toggle's is 0 or 1, [`raw_of`]). `None` while a reading it
/// needs is missing. A part whose
/// curve does not fit, or whose setting the profile no longer holds, is not on.
pub fn matches(
    profile: &Profile,
    part: &DevicePart,
    raw: impl Fn(FunctionId) -> Option<i64>,
) -> Option<bool> {
    let held = held_controls(profile);
    let mut on = true;
    if let Some(tone) = &part.tone {
        match &profile.eq {
            Some(eq) => {
                preset::from_readings(eq, &raw, preset::CUSTOM)?;
                on &= preset::matches(eq, &raw, tone);
            }
            None => on = false,
        }
    }
    for (key, step) in part.raw_bands.iter().flatten() {
        match profile
            .control_by_key(key)
            .filter(|_| is_raw_band(profile, key))
        {
            Some(control) => on &= raw(control.id)? == *step,
            None => on = false,
        }
    }
    for (key, step) in &part.controls {
        match held.iter().find(|c| c.key.eq_ignore_ascii_case(key)) {
            Some(control) => on &= raw(control.id)? == *step,
            None => on = false,
        }
    }
    Some(on)
}

#[cfg(test)]
mod tests;
