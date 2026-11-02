// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A level's raw steps in the units a person reads and types: the value a front end shows, the
//! one wording a level is written in, the bounds a slider is given and the raw step a typed
//! value lands on; and the one rule for a readout an `only_while` gates.
//!
//! Every front end reads a level through here, so the TUI, the CLI, MQTT and MCP agree on what a
//! step means. Text is built from the raw value, never from a converted value a transport
//! carried, so silence reads the same through the daemon and on a direct session.

use crate::convert::{self, EqParam, frequency_text, gain_text, q_text};
use crate::model::{Control, ControlKind, Convert, EqSpec, Profile};

/// Raw step `raw` of band `band`'s parameter `param` in the one wording every front end shows
/// a band value in: a frequency through [`frequency_text`] with ` Hz` (`40.4 Hz`, `1232 Hz`), a
/// gain through [`gain_text`] with ` dB` (`+1.45 dB`, `-0.20 dB`, `+6.0 dB`), and Q through
/// [`q_text`] with no unit (`0.50`; a line of prose writes `Q 0.50`). `None` for a raw value no
/// byte holds and for a band `eq` does not have.
pub fn band_text(eq: &EqSpec, band: usize, param: EqParam, raw: i64) -> Option<String> {
    let raw = u8::try_from(raw).ok()?;
    if band >= eq.bands.len() {
        return None;
    }
    Some(match param {
        EqParam::Frequency => format!("{} Hz", frequency_text(eq.frequency(band, raw)?)),
        EqParam::Gain => format!("{} dB", gain_text(eq.gain_db(raw))),
        EqParam::Q => q_text(eq.q(raw)),
    })
}

/// The word the bottom step of a level that has silence reads.
pub const SILENCE: &str = "silence";

/// A level's value in human terms.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Amount {
    /// The bottom of a level whose conversion makes it silence (`-inf` in its table).
    Silence,
    /// The converted value, or, for a level without a conversion, the raw step counted from its
    /// centre.
    Value(f64),
}

impl Amount {
    /// `value` as an amount: negative infinity is silence.
    pub fn of(value: f64) -> Amount {
        if value == f64::NEG_INFINITY {
            Amount::Silence
        } else {
            Amount::Value(value)
        }
    }

    /// The number a program reads: `None` for silence, which JSON has no number for.
    pub fn number(self) -> Option<f64> {
        match self {
            Amount::Silence => None,
            Amount::Value(value) => Some(value),
        }
    }
}

/// Where a level with no conversion of its own counts from: its centre for a balance-like
/// control, so a front end can carry 0 for centred, below 0 for the left and above 0 for the
/// right, and 0 (the raw number itself) for any other.
pub fn origin(control: &Control) -> i64 {
    match control.convert {
        Some(_) => 0,
        None => control.center.unwrap_or(0),
    }
}

/// What raw step `raw` of the level `control` amounts to: its converted value, silence, or the
/// step counted from the centre. `None` for a control that is not a level, and for a converted
/// raw outside its table.
pub fn amount(control: &Control, raw: i64) -> Option<Amount> {
    if control.kind != ControlKind::Level {
        return None;
    }
    match control.convert {
        Some(_) => convert::raw_to_display(control, raw).map(Amount::of),
        None => Some(Amount::Value(raw.saturating_sub(origin(control)) as f64)),
    }
}

/// The one wording of raw step `raw` of the level `control`: `-6.0 dB`, a space before the unit
/// and `-0.0` written `0.0`; `silence` at the bottom of a level that has it; a centred level's
/// signed distance from its centre (`-5`, `0`, `6`); a converted level without a unit to a tenth
/// (`-6.0`); and any other level's raw step. `None` for a control that is not a level and for a
/// converted raw outside its table, where the caller shows the raw value.
pub fn text(control: &Control, raw: i64) -> Option<String> {
    match control.convert {
        Some(_) => amount(control, raw).map(|amount| amount_text(amount, control.unit.as_deref())),
        None if control.kind == ControlKind::Level => {
            Some(raw.saturating_sub(origin(control)).to_string())
        }
        None => None,
    }
}

/// An amount in words: [`SILENCE`], or the value to a tenth with `unit` after a space; `-0.0` is
/// written `0.0`.
pub fn amount_text(amount: Amount, unit: Option<&str>) -> String {
    let Amount::Value(value) = amount else {
        return SILENCE.to_string();
    };
    let number = match format!("{value:.1}") {
        zero if zero == "-0.0" => "0.0".to_string(),
        number => number,
    };
    match unit {
        Some(unit) => format!("{number} {unit}"),
        None => number,
    }
}

/// What a level reaches, in the units a slider shows and a typed value is read in.
#[derive(Debug, Clone, PartialEq)]
pub struct Bounds {
    /// The lowest and highest raw step: what a key or a `−`/`+` button moves through.
    pub raw: (i64, i64),
    /// The lowest value, silence left out.
    pub min: f64,
    /// The highest value.
    pub max: f64,
    /// The finest gap between two neighbouring steps: 1 for a level without a conversion, 0 for a
    /// one-step range.
    pub step: f64,
    /// The unit the values are in.
    pub unit: Option<String>,
    /// Whether the level counts from a centre.
    pub centred: bool,
    /// The raw step that reads silence, for a level that has one.
    pub silence: Option<i64>,
}

/// The bounds of the level `control`, or `None` without a range (or with a table that has no
/// finite value). A table's values are rounded to three decimals, the precision the tables
/// carry; a level without a conversion counts from its centre, one raw step a step.
pub fn bounds(control: &Control) -> Option<Bounds> {
    let (lo, hi) = control.range?;
    let origin = origin(control);
    let (min, max, step, silence) = match &control.convert {
        None => (
            lo.saturating_sub(origin) as f64,
            hi.saturating_sub(origin) as f64,
            1.0,
            None,
        ),
        Some(Convert::Linear { min, max }) => {
            let step = if hi == lo {
                0.0
            } else {
                round3((max - min).abs() / (hi - lo) as f64)
            };
            (min.min(*max), min.max(*max), step, None)
        }
        Some(Convert::Table(_)) => {
            let steps: Vec<(i64, f64)> = (lo..=hi)
                .filter_map(|raw| convert::raw_to_display(control, raw).map(|v| (raw, v)))
                .collect();
            let silence = steps
                .iter()
                .find(|(_, value)| *value == f64::NEG_INFINITY)
                .map(|(raw, _)| *raw);
            let (min, max, step) = spread(steps.into_iter().map(|(_, value)| value).collect())?;
            (min, max, step, silence)
        }
    };
    Some(Bounds {
        raw: (lo, hi),
        min,
        max,
        step,
        unit: control.unit.clone(),
        centred: origin != 0,
        silence,
    })
}

/// The lowest and highest finite value of `values` and the finest gap between two neighbouring
/// ones, all rounded to three decimals; a gap of 1 when no two differ. `None` when none is
/// finite.
pub fn spread(values: Vec<f64>) -> Option<(f64, f64, f64)> {
    let mut finite: Vec<f64> = values.into_iter().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        return None;
    }
    finite.sort_by(f64::total_cmp);
    let min = finite[0];
    let max = *finite.last().expect("checked not empty");
    let step = finite
        .windows(2)
        .map(|w| w[1] - w[0])
        .filter(|d| *d > 0.0)
        .fold(f64::INFINITY, f64::min);
    let step = if step.is_finite() { step } else { 1.0 };
    Some((round3(min), round3(max), round3(step)))
}

/// `v` rounded to three decimals, the precision the conversion tables carry, so a float's own
/// rounding noise never reaches what a front end publishes.
pub fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// What a value past a converted level's ends does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outside {
    /// It lands on the nearest end, as a slider dragged past the end should.
    Clamp,
    /// It is refused, as an agent should be told no.
    Refuse,
}

/// Why a value has no raw step.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Refusal {
    /// Not a number, or an infinity.
    NotFinite,
    /// The control has no range to step through.
    NoRange,
    /// Past the level's ends.
    Outside {
        /// The lowest value the level takes.
        min: f64,
        /// The highest value the level takes.
        max: f64,
    },
}

/// The raw step `value`, in the level's own units, lands on.
///
/// Not-a-number and the infinities are refused first. A converted level snaps to its nearest
/// step, and past its ends clamps or refuses as `outside` says (with 1e-9 of slack); a level
/// without a conversion counts from its centre, rounds half away from zero, and refuses past its
/// ends whatever `outside` says, as every front end has done for a balance.
pub fn raw_for(control: &Control, value: f64, outside: Outside) -> Result<i64, Refusal> {
    if !value.is_finite() {
        return Err(Refusal::NotFinite);
    }
    let bounds = bounds(control).ok_or(Refusal::NoRange)?;
    let past = Refusal::Outside {
        min: bounds.min,
        max: bounds.max,
    };
    if control.convert.is_none() {
        let (lo, hi) = bounds.raw;
        let raw = value.round() + origin(control) as f64;
        if raw < lo as f64 || raw > hi as f64 {
            return Err(past);
        }
        return Ok(raw as i64);
    }
    if outside == Outside::Refuse && (value < bounds.min - 1e-9 || value > bounds.max + 1e-9) {
        return Err(past);
    }
    convert::display_to_raw(control, value).ok_or(past)
}

/// A number written as text, with `unit` after it in any case and blanks allowed, or without it:
/// `-6`, `-6dB`, `-6 dB`, `-6 DB`. `None` for anything else, an infinity or not-a-number
/// included.
pub fn number_in(text: &str, unit: Option<&str>) -> Option<f64> {
    let lower = text.trim().to_ascii_lowercase();
    let unit = unit.map(str::to_ascii_lowercase);
    let number = unit
        .as_deref()
        .and_then(|unit| lower.strip_suffix(unit))
        .map_or(lower.as_str(), str::trim);
    number.parse::<f64>().ok().filter(|n| n.is_finite())
}

/// The one `only_while` rule: a readout that names a gate reads true only while it reads true
/// itself (`own`) and its gate reads true too; a gate that has not reported counts as closed. A
/// control with no gate reads `own`. `gate_reads` says whether the gate reads true, `None` while
/// it has not reported.
pub fn shows_true(
    profile: &Profile,
    control: &Control,
    own: bool,
    gate_reads: impl Fn(&Control) -> Option<bool>,
) -> bool {
    own && profile
        .gate_of(control)
        .is_none_or(|gate| gate_reads(gate) == Some(true))
}

#[cfg(test)]
mod tests;
