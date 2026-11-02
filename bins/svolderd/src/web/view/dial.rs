// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The volume's knob: its range and the words of every step, the step from which the curve can
//! clip, the step the cap stops at, the dB marks at their true fractional steps, and its notes.
//! The theme draws the dial from these (`bins/svolderd/web/themes/ember/knob.js`).

use svolder_profiles::{Control, Profile, roles, tone::response, units};

use super::{Snapshot, knob_controls, minus, risk_step};

/// The marks between the ends, in dB, where they fall inside the range.
const MARKS_DB: [f64; 4] = [-60.0, -40.0, -20.0, -10.0];

/// The knob.
#[derive(Debug)]
pub(crate) struct Knob {
    pub(crate) key: String,
    pub(crate) lo: i64,
    pub(crate) hi: i64,
    pub(crate) raw: Option<i64>,
    pub(crate) target: Option<i64>,
    pub(crate) muted: bool,
    /// Whether the bottom step is silence: the detent sits between it and the next.
    pub(crate) silence: bool,
    /// The first step the curve can clip from.
    pub(crate) risk: Option<i64>,
    /// The highest step `[web] max_volume_db` allows.
    pub(crate) cap: Option<i64>,
    /// The candidate marks before any spacing: the fractional step and the words, bottom first.
    pub(crate) marks: Vec<(f64, String)>,
    /// The words of every step from `lo` to `hi`.
    pub(crate) words: Vec<String>,
    /// The dB of every step from `lo` to `hi`: a number, or `None` for silence and for every step
    /// of a control whose unit is not dB.
    pub(crate) levels: Vec<Option<f64>>,
    /// The number the readout shows: `−24.1`, `silence`, or `–` before a reading.
    pub(crate) readout: String,
    /// Its unit, beside a number.
    pub(crate) unit: Option<String>,
    /// The line under it: `to −1.0 dB` while climbing, `muted`.
    pub(crate) sub: Option<String>,
    /// The notes, the climb's first: the lasting ones go under the buttons, the climb's is a popup.
    pub(crate) notes: Vec<Note>,
    /// Whether the EQ's note can appear (the device has a tone control): its room under the
    /// buttons is kept, so the note's coming does not move what is below.
    pub(crate) room: bool,
    /// The words a screen reader says of it.
    pub(crate) aria: String,
    /// The mute toggle's key, for the Mute pill.
    pub(crate) mute: Option<String>,
}

impl Knob {
    /// The volume's place in its range, 0 to 1.
    pub(crate) fn fraction(&self) -> Option<f64> {
        let raw = self.raw?;
        (self.hi > self.lo).then(|| (raw - self.lo) as f64 / (self.hi - self.lo) as f64)
    }
}

/// A note under the knob.
#[derive(Debug)]
pub(crate) struct Note {
    pub(crate) text: String,
    /// Whether it is said in red: the level is in the zone that can clip.
    pub(crate) red: bool,
    /// Whether it comes and goes with what the dial is doing, so the page says it as a popup
    /// and leaves the dial's layout alone.
    pub(crate) popup: bool,
    /// Whether it is the EQ's: it has a slot of its own under the buttons, so its coming does
    /// not move the cap's note below it.
    pub(crate) eq: bool,
}

/// The knob for `profile`'s volume, when it has one.
pub(crate) fn knob(s: &Snapshot, profile: &Profile) -> Option<Knob> {
    let (volume, mute) = knob_controls(profile);
    let control = volume?;
    let (lo, hi) = control.range?;
    let raw = s.raw(control.id);
    let muted = mute.is_some_and(|m| s.message(m.id).is_some_and(|r| m.reads_true(&r.value)));
    let silence = matches!(units::amount(control, lo), Some(units::Amount::Silence));
    let words = |raw: i64| minus(&units::text(control, raw).unwrap_or_else(|| raw.to_string()));
    let texts: Vec<String> = (lo..=hi).map(words).collect();
    let in_db = control.unit.as_deref() == Some("dB");
    let db: Vec<Option<f64>> = (lo..=hi)
        .map(|raw| match units::amount(control, raw) {
            Some(units::Amount::Value(v)) if in_db && v.is_finite() => Some(v),
            _ => None,
        })
        .collect();
    let peak = profile
        .eq
        .as_ref()
        .and_then(|eq| response::clipping_peak(eq, |id| s.raw(id)));
    let risk = peak
        .and_then(|peak| risk_step(control, peak))
        .filter(|first| *first <= hi);
    let cap_db = roles::volume_cap(control, s.cap_db);
    let cap = cap_db.and_then(|cap| roles::capped_step(control, cap));
    let target = s
        .climb
        .filter(|c| c.key.eq_ignore_ascii_case(&control.key))
        .map(|c| c.target);
    let climbing = target.zip(raw).is_some_and(|(t, r)| t > r);
    let (readout, unit) = match raw.and_then(|r| units::amount(control, r)) {
        None => ("–".to_string(), None),
        Some(units::Amount::Silence) => (units::SILENCE.to_string(), None),
        Some(amount) => (
            minus(&units::amount_text(amount, None)),
            control.unit.clone(),
        ),
    };
    let sub = match (climbing, muted) {
        (true, _) => target.map(|t| format!("to {}", words(t))),
        (false, true) => Some("muted".to_string()),
        (false, false) => None,
    };
    let mut notes = Vec::new();
    if climbing && let Some(t) = target {
        notes.push(Note {
            text: format!(
                "Climbing to {}, a tenth of the range a second. Touch the dial or press Mute to \
                 stop.",
                words(t)
            ),
            red: false,
            popup: true,
            eq: false,
        });
    }
    if let Some(peak) = peak {
        notes.push(Note {
            text: minus(&format!(
                "This EQ peaks at {peak:+.1} dB. Above {:.1} dB, loud tracks may clip.",
                -peak
            )),
            red: !muted && raw.zip(risk).is_some_and(|(r, risk)| r >= risk),
            popup: false,
            eq: true,
        });
    }
    if let Some(db) = cap_db {
        let text = match cap {
            Some(_) => format!("Capped at {db:.1} dB by [web] max_volume_db"),
            None => format!(
                "[web] max_volume_db ({db:.1} dB) is below every step: the quietest step is the \
                 most it allows"
            ),
        };
        notes.push(Note {
            text: minus(&text),
            red: false,
            popup: false,
            eq: false,
        });
    }
    let mut aria = match &unit {
        Some(unit) => format!("{readout} {unit}"),
        None => readout.clone(),
    };
    if climbing && let Some(t) = target {
        aria.push_str(&format!(", climbing to {}", words(t)));
    }
    if muted {
        aria.push_str(", muted");
    }
    let marks = marks(control, lo, hi, silence);
    Some(Knob {
        key: control.key.clone(),
        lo,
        hi,
        raw,
        target,
        muted,
        silence,
        risk,
        cap,
        marks,
        words: texts,
        levels: db,
        readout,
        unit,
        sub,
        notes,
        room: profile.eq.is_some(),
        aria,
        mute: mute.map(|m| m.key.clone()),
    })
}

/// The candidate dB marks: the bottom (`−∞` at silence, else its number), the top, then −60,
/// −40, −20 and −10 where they fall strictly between, each at its true fractional step.
fn marks(control: &Control, lo: i64, hi: i64, silence: bool) -> Vec<(f64, String)> {
    let value = |raw: i64| roles::step_value(control, raw);
    let number = |db: f64| match db.abs() < 0.5 {
        true => "0".to_string(),
        false => minus(&format!("{db:.0}")),
    };
    let bottom = match silence {
        true => "−∞".to_string(),
        false => value(lo).map_or_else(|| lo.to_string(), number),
    };
    let top = value(hi).map_or_else(|| hi.to_string(), number);
    let mut wanted: Vec<(f64, String)> = vec![(lo as f64, bottom), (hi as f64, top)];
    let first = if silence { lo + 1 } else { lo };
    for db in MARKS_DB {
        // The fractional step whose value is `db`, between the two steps either side of it.
        let found = (first..hi).find_map(|raw| {
            let (a, b) = (value(raw)?, value(raw + 1)?);
            (a <= db && db <= b && b > a).then(|| raw as f64 + (db - a) / (b - a))
        });
        if let Some(found) = found {
            wanted.push((found, number(db)));
        }
    }
    wanted
}
