// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How the tools describe a control and its reading: labels, ranges, values and the reply a set
//! gives.

use serde_json::{Map, Value, json};
use svolder_ipc::{DeviceInfo, ValueObject};
use svolder_profiles::units::{self, Amount};
use svolder_profiles::{Control, ControlKind, Profile, roles};

/// What a set reports: the reading the device confirmed, a level counted the way `get_state`
/// counts it.
pub(super) fn reply(info: &DeviceInfo, control: &Control, vo: &ValueObject) -> Value {
    let origin = roles::origin(control);
    let display = match vo.raw.as_i64() {
        Some(raw) if control.kind == ControlKind::Level && origin != 0 => {
            units::text(control, raw).unwrap_or_else(|| vo.display.clone())
        }
        _ => vo.display.clone(),
    };
    let value = match control.kind {
        ControlKind::Level => value_of(control, vo),
        _ => json!(vo.value),
    };
    json!({
        "device": info.name,
        "key": vo.key,
        "label": vo.label,
        "value": value,
        "unit": vo.unit,
        "display": display,
    })
}

/// A control's label inside a sentence: lower case, except a label that starts with a number
/// (`20 Hz`), whose unit keeps its capitals.
pub(super) fn label_words(control: &Control) -> String {
    match control.label.starts_with(|c: char| c.is_ascii_digit()) {
        true => control.label.clone(),
        false => control.label.to_ascii_lowercase(),
    }
}

/// A level in dB for a sentence: `-6.0 dB`, and `silence` at the lowest step.
pub(super) fn db_text(db: f64) -> String {
    units::amount_text(Amount::of(db), Some("dB"))
}

/// The keys of the controls `keep` accepts, for a message that names the valid ones.
pub(super) fn keys_of(profile: &Profile, keep: impl Fn(&Control) -> bool) -> String {
    profile
        .controls
        .iter()
        .filter(|c| keep(c))
        .map(|c| c.key.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// One control as `get_state` shows it: its key, label and kind, its reading, and what it can
/// be set to.
pub(super) fn describe_control(
    profile: &Profile,
    control: &Control,
    values: &[ValueObject],
) -> Value {
    let id = control.id.to_string();
    let reading = values.iter().find(|v| v.id == id);
    let mut out = Map::new();
    out.insert("key".into(), json!(control.key));
    out.insert("label".into(), json!(control.label));
    out.insert("kind".into(), json!(kind_word(control.kind)));
    out.insert("writable".into(), json!(control.kind.writable()));
    out.insert(
        "value".into(),
        reading.map_or(Value::Null, |r| match profile.gate_of(control) {
            Some(_) => json!(gated(profile, control, r, values)),
            None => value_of(control, r),
        }),
    );
    if let Some(unit) = &control.unit {
        out.insert("unit".into(), json!(unit));
    }
    if !control.values.is_empty() {
        let names: Vec<&str> = control.values.iter().map(|(_, n)| n.as_str()).collect();
        out.insert("values".into(), json!(names));
    }
    if control.kind == ControlKind::Level
        && let Some(range) = level_range(control)
    {
        out.insert("range".into(), range);
    }
    Value::Object(out)
}

/// A level's range as `get_state` shows it: in its unit when it has a conversion, the silent step
/// of a volume left out, else in whole steps counted from its centre, if it has one. `None` for a
/// level with no range.
pub(super) fn level_range(control: &Control) -> Option<Value> {
    let bounds = units::bounds(control)?;
    Some(match control.convert {
        None => json!({ "min": bounds.min as i64, "max": bounds.max as i64 }),
        Some(_) => json!({ "min": bounds.min, "max": bounds.max }),
    })
}

fn kind_word(kind: ControlKind) -> &'static str {
    match kind {
        ControlKind::Select => "select",
        ControlKind::Toggle => "toggle",
        ControlKind::Level => "level",
        ControlKind::Readout => "readout",
        ControlKind::Meter => "meter",
        ControlKind::Fault => "fault",
    }
}

/// Whether a readout an `only_while` gates reads true: it and its gate both read true in
/// `values`, a gate not reported counting as closed ([`units::shows_true`]).
fn gated(
    profile: &Profile,
    control: &Control,
    reading: &ValueObject,
    values: &[ValueObject],
) -> bool {
    units::shows_true(profile, control, reads_true(&reading.raw), |gate| {
        let id = gate.id.to_string();
        values
            .iter()
            .find(|v| v.id == id)
            .map(|v| reads_true(&v.raw))
    })
}

/// Whether a raw reading reads true: a boolean's own bit, or an integer 1.
fn reads_true(raw: &Value) -> bool {
    match raw {
        Value::Bool(on) => *on,
        other => other.as_i64() == Some(1),
    }
}

/// A reading's value in the shape the control's kind suggests: a boolean for a toggle or
/// fault, for a level the value built from its raw step (the converted number for a level with a
/// conversion, `null` at silence, the step counted from the centre for a centred one), the name
/// for a select. A meter is read as its raw value: it is never converted here.
pub(super) fn value_of(control: &Control, reading: &ValueObject) -> Value {
    match control.kind {
        ControlKind::Toggle | ControlKind::Fault => json!(reads_true(&reading.raw)),
        ControlKind::Level | ControlKind::Meter => {
            let origin = units::origin(control);
            let amount = reading
                .raw
                .as_i64()
                .and_then(|raw| units::amount(control, raw).map(|amount| (raw, amount)));
            match amount {
                Some((_, Amount::Value(value))) if control.convert.is_some() => json!(value),
                Some((_, Amount::Silence)) => Value::Null,
                Some((raw, Amount::Value(_))) if origin != 0 => json!(raw - origin),
                _ => reading.raw.clone(),
            }
        }
        ControlKind::Select | ControlKind::Readout => reading
            .raw
            .as_u64()
            .and_then(|r| u8::try_from(r).ok())
            .and_then(|r| control.value_name(r))
            .map_or_else(|| reading.raw.clone(), |name| json!(name)),
    }
}
