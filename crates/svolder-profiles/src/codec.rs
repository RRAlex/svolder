// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Names for incoming statuses, and commands from a user's words.

use std::collections::BTreeMap;

use svolder_proto::{FunctionId, Message, Value, ValueType};

use crate::convert::{EqMode, EqParam, raw_to_display};
use crate::device_presets;
use crate::error::ProfileError;
use crate::model::{Control, ControlKind, Profile};
use crate::units::{self, Outside, band_text};

/// A message explained through a profile.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Reading {
    /// The function address the message carried.
    pub id: FunctionId,
    /// `None` when no control matches the id.
    pub key: Option<String>,
    /// The control's label, an EQ description, or `?`.
    pub label: String,
    /// The matching control's kind; `None` for an EQ parameter and an unknown id.
    pub kind: Option<ControlKind>,
    /// The value exactly as the message carried it.
    pub raw: Value,
    /// The value written for a person to read.
    pub display: String,
    /// The converted numeric value: the human units of a `Level` control that has a
    /// conversion, or the Hz, dB or Q of an EQ band parameter. `None` for everything else.
    pub value: Option<f64>,
    /// The unit `value` is expressed in, such as `dB`.
    pub unit: Option<String>,
}

/// Explain one message through a profile: what it addresses and what it says.
pub fn describe(profile: &Profile, message: &Message) -> Reading {
    let id = message.id;
    let raw = message.value.clone();
    if let Some(control) = profile.control_by_id(id) {
        // A value of the wrong wire type is reported as it arrived rather than interpreted,
        // except an integer on a boolean readout, which the device sends as 1 for true.
        let (display, value) = if control.kind == ControlKind::Readout
            && control.value_type == ValueType::Bool
            && raw.value_type() != ValueType::Bool
            && raw.as_i64().is_some()
        {
            (control.reads_true(&raw).to_string(), None)
        } else if raw.value_type() != control.value_type {
            (raw.to_string(), None)
        } else {
            match control.kind {
                ControlKind::Toggle => (
                    if raw.bool_value() { "on" } else { "off" }.to_string(),
                    None,
                ),
                ControlKind::Fault => (
                    if raw.bool_value() { "active" } else { "clear" }.to_string(),
                    None,
                ),
                ControlKind::Select | ControlKind::Readout => {
                    let display = match raw.as_u64().and_then(|v| u8::try_from(v).ok()) {
                        Some(v) => match control.value_name(v) {
                            Some(name) => format!("{v} ({name})"),
                            None => v.to_string(),
                        },
                        None => raw.to_string(),
                    };
                    (display, None)
                }
                ControlKind::Level => match raw.as_i64() {
                    Some(v) => (format_level(control, v), raw_to_display(control, v)),
                    None => (raw.to_string(), None),
                },
                ControlKind::Meter => (raw.to_string(), None),
            }
        };
        return Reading {
            id,
            key: Some(control.key.clone()),
            label: control.label.clone(),
            kind: Some(control.kind),
            raw,
            display,
            value,
            unit: control.unit.clone(),
        };
    }
    if let Some(eq) = &profile.eq
        && id.first() == eq.id
        && let Some(sub) = id.second()
    {
        let raw_u8 = raw.as_u64().and_then(|v| u8::try_from(v).ok());
        let eq_reading =
            |label: String, display: String, value: Option<f64>, unit: Option<&str>| Reading {
                id,
                key: None,
                label,
                kind: None,
                raw: raw.clone(),
                display,
                value,
                unit: unit.map(str::to_string),
            };
        if sub == eq.mode_sub {
            let display = raw_u8
                .and_then(EqMode::from_raw)
                .map(|m| m.name().to_string())
                .unwrap_or_else(|| raw.to_string());
            return eq_reading("Tone control mode".to_string(), display, None, None);
        }
        if sub == eq.loudness_sub {
            return eq_reading("Loudness".to_string(), raw.to_string(), None, None);
        }
        if let Some(param_sub) = id.third()
            && let Some(param) = EqParam::from_sub(param_sub)
            && (1..=eq.bands.len()).contains(&usize::from(sub))
        {
            let band = usize::from(sub) - 1;
            let (label, display, value, unit) = match (param, raw_u8) {
                (EqParam::Frequency, Some(v)) => {
                    let hz = eq.frequency(band, v).unwrap_or(0.0);
                    (
                        format!("EQ band {sub} frequency"),
                        band_text(eq, band, param, i64::from(v)).unwrap_or_else(|| raw.to_string()),
                        Some(hz),
                        Some("Hz"),
                    )
                }
                (EqParam::Gain, Some(v)) => (
                    format!("EQ band {sub} gain"),
                    band_text(eq, band, param, i64::from(v)).unwrap_or_else(|| raw.to_string()),
                    Some(eq.gain_db(v)),
                    Some("dB"),
                ),
                (EqParam::Q, Some(v)) => (
                    format!("EQ band {sub} Q"),
                    band_text(eq, band, param, i64::from(v)).unwrap_or_else(|| raw.to_string()),
                    Some(eq.q(v)),
                    None,
                ),
                (EqParam::Frequency, None) => (
                    format!("EQ band {sub} frequency"),
                    raw.to_string(),
                    None,
                    Some("Hz"),
                ),
                (EqParam::Gain, None) => (
                    format!("EQ band {sub} gain"),
                    raw.to_string(),
                    None,
                    Some("dB"),
                ),
                (EqParam::Q, None) => (format!("EQ band {sub} Q"), raw.to_string(), None, None),
            };
            return eq_reading(label, display, value, unit);
        }
    }
    Reading {
        id,
        key: None,
        label: "?".to_string(),
        kind: None,
        display: raw.to_string(),
        value: None,
        raw,
        unit: None,
    }
}

/// Build the command that sets `key` from forgiving user input.
///
/// A key no control has may name an action: its input is then a slot, `2` or `Preset 2`, and the
/// command is the action's on that slot. Anything that is not one of the action's slots (`0`, a
/// number past the last slot, `up`, `max`) is refused as a bad slot.
pub fn intent(profile: &Profile, key: &str, input: &str) -> Result<Message, ProfileError> {
    let Some(control) = profile.control_by_key(key) else {
        let action = profile
            .action_by_key(key)
            .ok_or_else(|| profile.unknown_key(key))?;
        let slot = device_presets::parse_slot(input, action.slots).ok_or_else(|| {
            ProfileError::BadSlot {
                key: action.key.clone(),
                slots: action.slots,
            }
        })?;
        return action.command(slot);
    };
    if !control.kind.writable() {
        return Err(ProfileError::NotWritable(control.key.clone()));
    }
    let bad = || ProfileError::BadInput {
        key: control.key.clone(),
        input: input.to_string(),
    };
    let text = input.trim();
    let lower = text.to_ascii_lowercase();
    let raw: i64 = match control.kind {
        ControlKind::Toggle => {
            let on = match lower.as_str() {
                "on" | "true" | "1" | "yes" => true,
                "off" | "false" | "0" | "no" => false,
                _ => return Err(bad()),
            };
            return Ok(Message::command(control.id, Value::Bool(on)));
        }
        ControlKind::Select => {
            if let Ok(n) = lower.parse::<u8>() {
                if control.values.is_empty() || control.value_name(n).is_some() {
                    i64::from(n)
                } else {
                    return Err(bad());
                }
            } else {
                let exact: Vec<u8> = control
                    .values
                    .iter()
                    .filter(|(_, name)| name.eq_ignore_ascii_case(&lower))
                    .map(|(v, _)| *v)
                    .collect();
                let prefix: Vec<u8> = control
                    .values
                    .iter()
                    .filter(|(_, name)| name.to_ascii_lowercase().starts_with(&lower))
                    .map(|(v, _)| *v)
                    .collect();
                match (exact.as_slice(), prefix.as_slice()) {
                    ([v], _) | (_, [v]) => i64::from(*v),
                    _ => return Err(bad()),
                }
            }
        }
        // Level. `writable` above has already rejected readouts, meters and faults.
        _ => {
            let (lo, hi) = control.range.ok_or_else(bad)?;
            match lower.as_str() {
                "max" => hi,
                "min" => lo,
                _ => {
                    let suffixed = control
                        .unit
                        .as_deref()
                        .is_some_and(|unit| lower.ends_with(&unit.to_ascii_lowercase()));
                    if suffixed {
                        let value =
                            units::number_in(&lower, control.unit.as_deref()).ok_or_else(bad)?;
                        units::raw_for(control, value, Outside::Clamp).map_err(|_| bad())?
                    } else {
                        let n: i64 = lower.parse().map_err(|_| bad())?;
                        if n < lo || n > hi {
                            return Err(bad());
                        }
                        n
                    }
                }
            }
        }
    };
    let value = typed(control.value_type, raw).ok_or_else(bad)?;
    Ok(Message::command(control.id, value))
}

/// Build the command that nudges a `Level` control from `current_raw` by `delta`, clamped
/// to the control's range.
///
/// The caller supplies the current raw value because only it knows what the device last
/// reported. A key naming no control, or one that is not a level with a range (an action is
/// not, with words of its own), is refused without a command being built.
pub fn relative(
    profile: &Profile,
    key: &str,
    current_raw: i64,
    delta: i64,
) -> Result<Message, ProfileError> {
    let control = steppable_control(profile, key)?;
    if control.kind != ControlKind::Level {
        return Err(ProfileError::NotSteppable(control.key.clone()));
    }
    let (lo, hi) = control
        .range
        .ok_or_else(|| ProfileError::NotSteppable(control.key.clone()))?;
    let target = current_raw.saturating_add(delta).clamp(lo, hi);
    intent(profile, key, &target.to_string())
}

/// The control `key` names, for a step: an action is refused as not steppable, and a key that is
/// neither a control nor an action is unknown.
pub(crate) fn steppable_control<'a>(
    profile: &'a Profile,
    key: &str,
) -> Result<&'a Control, ProfileError> {
    match profile.control_by_key(key) {
        Some(control) => Ok(control),
        None => match profile.action_by_key(key) {
            Some(action) => Err(ProfileError::ActionNotSteppable(action.key.clone())),
            None => Err(profile.unknown_key(key)),
        },
    }
}

/// Build the command that sets one band parameter to a value in Hz, dB or Q.
pub fn eq_set(
    profile: &Profile,
    band: usize,
    param: EqParam,
    value: f64,
) -> Result<Message, ProfileError> {
    let eq = profile.eq.as_ref().ok_or(ProfileError::NoEq)?;
    let id = eq.band_id(band, param).ok_or(ProfileError::BadBand(band))?;
    if !value.is_finite() {
        return Err(ProfileError::BadInput {
            key: format!("eq band {}", band + 1),
            input: value.to_string(),
        });
    }
    // The band exists, so a value with no step is a frequency that is not positive.
    let raw = eq
        .step(band, param, value)
        .ok_or_else(|| ProfileError::BadInput {
            key: format!("eq band {} {}", band + 1, param.word()),
            input: value.to_string(),
        })?;
    Ok(Message::command(id, Value::U8(raw)))
}

/// Build the command that selects a tone-control mode.
pub fn eq_mode(profile: &Profile, mode: EqMode) -> Result<Message, ProfileError> {
    let eq = profile.eq.as_ref().ok_or(ProfileError::NoEq)?;
    Ok(Message::command(eq.mode_id(), Value::U8(mode as u8)))
}

/// Lay a device's own input names over the profile's `input` control.
///
/// A name replaces the label of a value the profile already lists and adds one it does not,
/// leaving the values in ascending raw order. A profile without an `input` control, and an
/// empty table, both leave the profile untouched.
pub fn apply_input_labels(profile: &mut Profile, inputs: &BTreeMap<u8, String>) {
    if inputs.is_empty() {
        return;
    }
    let Some(control) = profile.controls.iter_mut().find(|c| c.key == "input") else {
        return;
    };
    for (raw, name) in inputs {
        match control.values.iter_mut().find(|(v, _)| v == raw) {
            Some((_, existing)) => existing.clone_from(name),
            None => control.values.push((*raw, name.clone())),
        }
    }
    control.values.sort_by_key(|(raw, _)| *raw);
}

/// Build the command that sets the loudness strength, 0 to 127.
pub fn eq_loudness(profile: &Profile, raw: u8) -> Result<Message, ProfileError> {
    let eq = profile.eq.as_ref().ok_or(ProfileError::NoEq)?;
    Ok(Message::command(eq.loudness_id(), Value::U8(raw)))
}

fn typed(value_type: ValueType, n: i64) -> Option<Value> {
    Some(match value_type {
        ValueType::U8 => Value::U8(u8::try_from(n).ok()?),
        ValueType::U16 => Value::U16(u16::try_from(n).ok()?),
        ValueType::U32 => Value::U32(u32::try_from(n).ok()?),
        ValueType::U64 => Value::U64(u64::try_from(n).ok()?),
        ValueType::I8 => Value::I8(i8::try_from(n).ok()?),
        ValueType::I16 => Value::I16(i16::try_from(n).ok()?),
        ValueType::I32 => Value::I32(i32::try_from(n).ok()?),
        ValueType::I64 => Value::I64(n),
        ValueType::Bool => Value::Bool(n != 0),
        ValueType::Data => return None,
    })
}

/// A level's `display`: a converted one's raw step and its wording (`109 (-6.0 dB)`,
/// `0 (silence)`), a centred one's distance from its centre (`0`, `6`, `-5`), any other's raw
/// step.
fn format_level(control: &Control, raw: i64) -> String {
    match (&control.convert, units::text(control, raw)) {
        (Some(_), Some(text)) => format!("{raw} ({text})"),
        (None, Some(text)) if units::origin(control) != 0 => text,
        _ => raw.to_string(),
    }
}

#[cfg(test)]
mod tests;
