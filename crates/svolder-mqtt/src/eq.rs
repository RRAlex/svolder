// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone control over MQTT beyond its mode: a preset select, the loudness strength and, for
//! every band, a gain, a frequency and a Q. A profile with a tone control exposes
//! `eq_preset`, `eq_loudness` and `eq_band_<b>_gain|frequency|q` for `b` from 1 to its band
//! count; every other profile exposes none of them.
//!
//! The entities are modeled here rather than as profile controls because a band parameter has
//! no control of its own: its value and its steps come from the [`EqSpec`] conversions.

use serde_json::{Map, Value as Json};

use svolder_profiles::tone::preset::{self, On, Preset};
use svolder_profiles::{EqParam, EqSpec, Profile, ProfileError};
use svolder_proto::{FunctionId, Message};

use crate::discovery::{Device, DiscoveryEntry, entity_envelope, round3, table_bounds};
use crate::{json_number, topics};

/// The key the preset select is exposed under.
pub const PRESET_KEY: &str = "eq_preset";
/// The key the loudness strength is exposed under.
pub const LOUDNESS_KEY: &str = "eq_loudness";
/// The preset select's option, and state, for a curve no listed preset matches.
pub const CUSTOM: &str = preset::CUSTOM;

/// What a tone-control key addresses on one profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The preset select.
    Preset,
    /// The loudness strength.
    Loudness,
    /// One parameter of one band.
    Band {
        /// The band, counted from 0.
        band: usize,
        /// Which of its parameters.
        param: EqParam,
    },
}

/// The suffix of a band parameter's key.
fn param_word(param: EqParam) -> &'static str {
    match param {
        EqParam::Gain => "gain",
        EqParam::Frequency => "frequency",
        EqParam::Q => "q",
    }
}

/// The key of one band parameter; `band` counts from 0, the key from 1.
pub fn band_key(band: usize, param: EqParam) -> String {
    format!("eq_band_{}_{}", band + 1, param_word(param))
}

/// The key of the entity `target` names.
pub fn key(target: Target) -> String {
    match target {
        Target::Preset => PRESET_KEY.to_string(),
        Target::Loudness => LOUDNESS_KEY.to_string(),
        Target::Band { band, param } => band_key(band, param),
    }
}

/// What `key` addresses on `profile`, matched without regard to case, or `None` for a key that
/// is no tone-control entity of it.
pub fn target(profile: &Profile, key: &str) -> Option<Target> {
    let eq = profile.eq.as_ref()?;
    keys(eq)
        .into_iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, target)| target)
}

/// Every tone-control entity of a tone control, in the order discovery lists them: the preset,
/// the loudness, then each band's gain, frequency and Q.
fn keys(eq: &EqSpec) -> Vec<(String, Target)> {
    let mut out = vec![
        (PRESET_KEY.to_string(), Target::Preset),
        (LOUDNESS_KEY.to_string(), Target::Loudness),
    ];
    for band in 0..eq.bands.len() {
        for param in [EqParam::Gain, EqParam::Frequency, EqParam::Q] {
            out.push((band_key(band, param), Target::Band { band, param }));
        }
    }
    out
}

/// The keys of every tone-control entity `profile` exposes, empty for a profile with no tone
/// control.
#[cfg(test)]
fn exposed_keys(profile: &Profile) -> Vec<String> {
    profile
        .eq
        .as_ref()
        .map(|eq| keys(eq).into_iter().map(|(key, _)| key).collect())
        .unwrap_or_default()
}

/// The preset select's options for `names`, the fitting presets in order: each name, then
/// [`CUSTOM`] last. A preset whose name is `custom` in any case is left out, since the option
/// stands for no preset at all.
pub fn preset_options(names: &[String]) -> Vec<String> {
    names
        .iter()
        .filter(|name| !preset::same_name(name, CUSTOM))
        .cloned()
        .chain(std::iter::once(CUSTOM.to_string()))
        .collect()
}

/// The discovery entries of every tone-control entity of `profile`: components `select` or
/// `number` with `commands`, `sensor` without. `presets` names the fitting presets, in the
/// order the select offers them.
pub(crate) fn entries(
    base_topic: &str,
    discovery_prefix: &str,
    profile: &Profile,
    device: &Device<'_>,
    commands: bool,
    presets: &[String],
) -> Vec<DiscoveryEntry> {
    let Some(eq) = profile.eq.as_ref() else {
        return Vec::new();
    };
    keys(eq)
        .into_iter()
        .map(|(key, target)| {
            let (label, unit, bounds, mode) = describe(eq, target);
            let component = match (commands, target) {
                (true, Target::Preset) => "select",
                (true, _) => "number",
                (false, _) => "sensor",
            };
            let topic = topics::discovery_config(discovery_prefix, component, device.unit_id, &key);
            let mut payload = entity_envelope(base_topic, profile, device, &label, &key);
            payload.insert(
                "state_topic".into(),
                topics::state(base_topic, device.unit_id).into(),
            );
            payload.insert(
                "value_template".into(),
                format!("{{{{ value_json.{key} }}}}").into(),
            );
            if commands {
                payload.insert(
                    "command_topic".into(),
                    topics::command(base_topic, device.unit_id, &key).into(),
                );
                match target {
                    Target::Preset => {
                        let options = preset_options(presets).into_iter().map(Json::from);
                        payload.insert("options".into(), Json::Array(options.collect()));
                    }
                    _ => {
                        if let Some((min, max, step)) = bounds {
                            payload.insert("min".into(), json_number(min));
                            payload.insert("max".into(), json_number(max));
                            payload.insert("step".into(), json_number(step));
                        }
                        if let Some(mode) = mode {
                            payload.insert("mode".into(), mode.into());
                        }
                    }
                }
            }
            if let Some(unit) = unit {
                payload.insert("unit_of_measurement".into(), unit.into());
            }
            DiscoveryEntry {
                topic,
                payload: Json::Object(payload),
            }
        })
        .collect()
}

/// An entity's label, unit, `number` bounds (minimum, maximum, step) and `number` mode.
type Description = (
    String,
    Option<&'static str>,
    Option<(f64, f64, f64)>,
    Option<&'static str>,
);

fn describe(eq: &EqSpec, target: Target) -> Description {
    match target {
        Target::Preset => ("EQ preset".to_string(), None, None, None),
        Target::Loudness => (
            "Loudness strength".to_string(),
            None,
            Some((0.0, f64::from(EqSpec::STEP_MAX), 1.0)),
            Some("slider"),
        ),
        Target::Band { band, param } => {
            let n = band + 1;
            match param {
                EqParam::Gain => (
                    format!("Band {n} gain"),
                    Some("dB"),
                    table_bounds((0..=EqSpec::STEP_MAX).map(|raw| eq.gain_db(raw)).collect()),
                    Some("slider"),
                ),
                EqParam::Frequency => {
                    let low = eq.frequency(band, 0);
                    let high = eq.frequency(band, EqSpec::STEP_MAX);
                    let bounds = low.zip(high).map(|(low, high)| {
                        // The window's edges are rounded outward, so every value it reaches,
                        // itself rounded to three decimals, lies inside what is announced.
                        (
                            (low * 1000.0).floor() / 1000.0,
                            (high * 1000.0).ceil() / 1000.0,
                            1.0,
                        )
                    });
                    (
                        format!("Band {n} frequency"),
                        Some("Hz"),
                        bounds,
                        Some("box"),
                    )
                }
                EqParam::Q => (
                    format!("Band {n} Q"),
                    None,
                    Some((round3(eq.q_range.0), round3(eq.q_range.1), 0.01)),
                    Some("box"),
                ),
            }
        }
    }
}

/// Add every tone-control reading `readings` holds to `map` in human units: the loudness
/// strength as its raw number, each band's gain in dB, frequency in Hz and Q rounded to three
/// decimals. A parameter with no reading, or one that is no step, is left out.
pub(crate) fn add_readings(profile: &Profile, readings: &[Message], map: &mut Map<String, Json>) {
    let Some(eq) = profile.eq.as_ref() else {
        return;
    };
    let step = step_reader(readings);
    if let Some(raw) = step(eq.loudness_id()) {
        map.insert(LOUDNESS_KEY.to_string(), Json::from(raw));
    }
    for band in 0..eq.bands.len() {
        for param in [EqParam::Gain, EqParam::Frequency, EqParam::Q] {
            let Some(raw) = eq.band_id(band, param).and_then(&step) else {
                continue;
            };
            let value = match param {
                EqParam::Gain => Some(eq.gain_db(raw)),
                EqParam::Frequency => eq.frequency(band, raw),
                EqParam::Q => Some(eq.q(raw)),
            };
            if let Some(value) = value {
                map.insert(band_key(band, param), json_number(round3(value)));
            }
        }
    }
}

/// A reader of the raw reading of one function in `readings`, a step 0 to 127 or nothing.
fn step_reader(readings: &[Message]) -> impl Fn(FunctionId) -> Option<u8> + '_ {
    move |id| {
        readings
            .iter()
            .find(|m| m.id == id)
            .and_then(|m| m.value.as_i64())
            .and_then(|raw| u8::try_from(raw).ok())
            .filter(|raw| *raw <= EqSpec::STEP_MAX)
    }
}

/// The raw readings of `readings` as [`svolder_profiles::tone::preset`] takes them.
pub fn raw_of(readings: &[Message]) -> impl Fn(FunctionId) -> Option<i64> + '_ {
    move |id| {
        readings
            .iter()
            .find(|m| m.id == id)
            .and_then(|m| m.value.as_i64())
    }
}

/// The preset select's state for a device: `None` until the device has reported its mode and
/// every parameter of every band; then the name of `recalled`, the preset last recalled on this
/// device, while it is among `fitting` and still matches; else the first preset of `fitting`
/// that matches; else [`CUSTOM`]. A preset named `custom` never stands for itself.
pub fn preset_state(
    profile: &Profile,
    readings: &[Message],
    fitting: &[Preset],
    recalled: Option<&str>,
) -> Option<String> {
    let eq = profile.eq.as_ref()?;
    match preset::on(eq, raw_of(readings), fitting, recalled) {
        On::Unread => None,
        On::Preset(preset) => Some(preset.name.clone()),
        On::Custom => Some(CUSTOM.to_string()),
    }
}

/// A tone-control command parsed from an MQTT payload.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// Set one band parameter, as `svolder eq set` sends it.
    Band {
        /// The band, counted from 1.
        band: usize,
        /// `f`, `g` or `q`.
        param: &'static str,
        /// The value in Hz, dB or Q, which the device's actor snaps to the nearest step.
        value: f64,
    },
    /// Set the loudness strength, 0 to 127.
    Loudness(u8),
    /// Recall the preset of this name; whether it exists is for the caller to say.
    Preset(String),
}

/// Parse `payload` for `target`. A band value is any finite number, as `svolder eq set`
/// takes one: a value outside the band's window goes to the nearest step it has, and the
/// conversion is the one the device's actor applies. The loudness strength is a number rounded
/// to a whole step, refused outside 0 to 127. A preset is its name, blanks round it aside.
pub fn parse(profile: &Profile, target: Target, payload: &str) -> Result<Command, ProfileError> {
    let eq = profile.eq.as_ref().ok_or(ProfileError::NoEq)?;
    let text = payload.trim();
    match target {
        Target::Preset => Ok(Command::Preset(text.to_string())),
        Target::Loudness => {
            let bad = || ProfileError::BadInput {
                key: LOUDNESS_KEY.to_string(),
                input: payload.to_string(),
            };
            let value: f64 = text.parse().map_err(|_| bad())?;
            let rounded = value.round();
            if !value.is_finite() || !(0.0..=f64::from(EqSpec::STEP_MAX)).contains(&rounded) {
                return Err(bad());
            }
            Ok(Command::Loudness(rounded as u8))
        }
        Target::Band { band, param } => {
            let value: f64 = text.parse().map_err(|_| ProfileError::BadInput {
                key: band_key(band, param),
                input: payload.to_string(),
            })?;
            if param == EqParam::Frequency && value.is_finite() && value <= 0.0 {
                let window = |raw| eq.frequency(band, raw).unwrap_or_default();
                return Err(ProfileError::BadEqWord(format!(
                    "band {} frequency {value} is not above 0 Hz; its window is {} to {} Hz",
                    band + 1,
                    crate::discovery::round3(window(0)),
                    crate::discovery::round3(window(EqSpec::STEP_MAX)),
                )));
            }
            // The same build and refusal `svolder eq set` goes through.
            svolder_profiles::eq_set(profile, band, param, value)?;
            Ok(Command::Band {
                band: band + 1,
                param: param.word(),
                value,
            })
        }
    }
}

#[cfg(test)]
mod tests;
