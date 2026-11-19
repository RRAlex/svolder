// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's loudness: what it is set to, the maths that turns the computer's volume into
//! the coefficients of the chain's loudness stage, and the view a front end draws from.
//!
//! Everything here is pure. Below a reference volume the ear hears the bass and the treble less
//! than the middle of the band (the equal-loudness contours of ISO 226:2003); the stage gives
//! that back by taking the middle and the top down instead, so it never raises any frequency
//! and the preamp never moves for it. With `v` the volume on the cubic scale (0 to 1), `c = v³`
//! the gain on the samples, `r` the reference and `s` the strength (both 0 to 1):
//!
//! ```text
//! V = 20·log10(c) = 60·log10(v)    R = 60·log10(r)    d = clamp(R − V, 0, 40) dB    d_e = s·d
//! G_lo = LOW_GAIN_PER_DB · d_e       G_hi = min(HIGH_GAIN_PER_DB · d_e, 6) dB
//! ```
//!
//! The stage is two first-order sections per channel, a low shelf with its zero at
//! [`LOW_CORNER_HZ`] and `G_lo` of gain at DC, and a high shelf with its zero at
//! [`HIGH_CORNER_HZ`] and `G_hi` of gain at Nyquist. Each is `k·(s + Kz)/(s + Kp)` in the
//! bilinear domain (`s` the prewarped Laplace variable, `Kz = tan(π·f/rate)`):
//!
//! ```text
//! low:  Kz = K(289 Hz)      Kp = Kz / 10^(G_lo/20)    k = 1
//! high: Kz = K(7.76 kHz)      Kp = Kz · 10^(G_hi/20)    k = Kp / Kz
//! b0 = k (1 + Kz)/(1 + Kp)   b1 = k (Kz − 1)/(1 + Kp)   b2 = 0   a1 = (Kp − 1)/(1 + Kp)   a2 = 0
//! ```
//!
//! Setting the pole from the gain, not from a second prewarped frequency, makes the gain at
//! Nyquist exactly `G_hi` at every rate; prewarping the pole as well would let a 44.1 or 48 kHz
//! stage rise 14 to 18 dB at Nyquist and move the maximum there. Each section's magnitude is
//! monotonic from DC to Nyquist.
//!
//! **Cut-only.** `M`, the largest gain of the two sections together, measured on the
//! f32-rounded coefficients with the preamp's own evaluator, scales the low section down
//! (`b0` and `b1` by `1/M`); an excess the rounding leaves is trimmed one f32 step at a time. So
//! the stage's f32 maximum over DC to Nyquist is at most 0 dB at every rate and every depth, DC
//! sits at 0 dB and the middle at about `−G_lo`. A depth of 0 is the exact identity.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};
use svolder_profiles::tone::response::played_peak_db_of;

use super::EqFailure;
use super::crossfeed::Range;

/// The corner of the low shelf (its zero), in Hz.
pub const LOW_CORNER_HZ: f64 = 289.0;
/// The low shelf's gain at DC per dB of effective depth.
pub const LOW_GAIN_PER_DB: f64 = 0.53;
/// The corner of the high shelf (its zero), in Hz.
pub const HIGH_CORNER_HZ: f64 = 7_760.0;
/// The high shelf's gain at Nyquist per dB of effective depth, before [`HIGH_GAIN_CAP_DB`].
pub const HIGH_GAIN_PER_DB: f64 = 0.15;
/// The most the high shelf gains, in dB.
pub const HIGH_GAIN_CAP_DB: f64 = 6.0;
/// The deepest the depth counts, in dB below the reference (the volume at 0 gives this).
pub const MAX_DEPTH_DB: f64 = 40.0;
/// The step the played depth is held to, in dB.
pub const DEPTH_STEP_DB: f64 = 0.1;
/// How many points a curve has: 20 per decade from 20 Hz to 20 kHz, ends included.
pub const CURVE_POINTS: usize = 61;

/// The most a coefficient port of the chain can hold, either way (PipeWire clamps to it).
const COEFFICIENT_LIMIT: f64 = 10.0;
/// The most one rounding trim may take before the stage falls back to the identity.
const TRIM_LIMIT: usize = 4096;

/// The strength range in percent.
const STRENGTH_RANGE: Range = Range {
    min: 0.0,
    max: 100.0,
    step: 5.0,
};
/// The reference range in percent of the volume scale.
const REFERENCE_RANGE: Range = Range {
    min: 1.0,
    max: 100.0,
    step: 1.0,
};

/// The computer's volume as the controller reads it: the gain on the samples (the volume on the
/// cubic scale, cubed) and whether the sink is muted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Level {
    /// The gain on the samples, 1.0 at full volume.
    pub linear: f64,
    /// Whether the sink is muted (hard or soft).
    pub muted: bool,
}

impl Level {
    /// The volume as the percent a volume control shows (the cube root of the gain), or `None`
    /// for a gain that is not a number.
    pub fn volume_pct(&self) -> Option<f64> {
        match self.linear.is_finite() {
            true => Some(self.linear.max(0.0).cbrt() * 100.0),
            false => None,
        }
    }
}

/// What the loudness is set to. The numbers are kept apart from whether it is on, so turning it
/// off keeps them.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoudnessSettings {
    /// Whether it plays.
    pub on: bool,
    /// How much of the equal-loudness correction is given back, in percent (step 5).
    pub strength_pct: f64,
    /// The volume at and above which the stage is flat, in percent of the volume scale.
    pub reference_pct: f64,
}

impl Default for LoudnessSettings {
    /// Off, half strength, flat only at full volume.
    fn default() -> Self {
        LoudnessSettings {
            on: false,
            strength_pct: 50.0,
            reference_pct: 100.0,
        }
    }
}

/// What a loudness change from outside asks for: the fields of `HostEqOp::Loudness` but
/// `reference_now` and `done`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LoudnessChange {
    /// Turn it on or off.
    pub on: Option<bool>,
    /// The strength in percent.
    pub strength_pct: Option<f64>,
    /// The reference in percent of the volume scale.
    pub reference_pct: Option<f64>,
}

impl LoudnessSettings {
    /// These settings with every number snapped to its step and held to its range (a number that
    /// is not one goes back to the default).
    pub fn held(&self) -> LoudnessSettings {
        let default = LoudnessSettings::default();
        let snap = |range: &Range, value: f64, fallback: f64| match value.is_finite() {
            true => range.snap(value),
            false => fallback,
        };
        LoudnessSettings {
            on: self.on,
            strength_pct: snap(&STRENGTH_RANGE, self.strength_pct, default.strength_pct),
            reference_pct: snap(&REFERENCE_RANGE, self.reference_pct, default.reference_pct),
        }
    }

    /// The settings after `change`: a number that is not finite is refused in words and nothing
    /// changes; a strength is snapped to its step (5) and held to 0-100, a reference to its step
    /// (1) and held to 1-100. A change that asks for nothing returns the settings held.
    pub fn change(&self, change: LoudnessChange) -> Result<LoudnessSettings, EqFailure> {
        let mut next = self.held();
        if let Some(value) = change.strength_pct {
            next.strength_pct = checked(value, "strength", &STRENGTH_RANGE)?;
        }
        if let Some(value) = change.reference_pct {
            next.reference_pct = checked(value, "reference", &REFERENCE_RANGE)?;
        }
        if let Some(on) = change.on {
            next.on = on;
        }
        Ok(next)
    }

    /// The settings with the reference at the volume `level` reads (rounded to a whole percent and
    /// held to the range). Refused in words while the volume has not been read.
    pub fn reference_now(&self, level: Option<Level>) -> Result<LoudnessSettings, EqFailure> {
        let pct = level
            .and_then(|level| level.volume_pct())
            .ok_or_else(|| EqFailure {
                what: "the computer's volume has not been read yet".to_string(),
                next: "Try again in a moment, or give the reference as a number.".to_string(),
            })?;
        let mut next = self.held();
        next.reference_pct = REFERENCE_RANGE.snap(pct);
        Ok(next)
    }
}

/// `value` snapped and held for the control `name` with `range`, or a refusal in words.
fn checked(value: f64, name: &str, range: &Range) -> Result<f64, EqFailure> {
    match value.is_finite() {
        true => Ok(range.snap(value)),
        false => Err(EqFailure {
            what: format!("the {name} {value} is not a number"),
            next: format!(
                "Give the {name} as a number from {} to {} percent.",
                range.min, range.max
            ),
        }),
    }
}

/// The effective depth `d_e` in dB of the volume whose gain on the samples is `linear` under
/// `settings`: `s · clamp(R − V, 0, 40)`. A volume of 0 (or a gain that is not a number) gives
/// the full depth; the reference and anything above it give 0.
pub fn depth_db(linear: f64, settings: &LoudnessSettings) -> f64 {
    let settings = settings.held();
    let reference_db = 60.0 * (settings.reference_pct / 100.0).log10();
    let volume_db = match linear > 0.0 && linear.is_finite() {
        true => 20.0 * linear.log10(),
        false => f64::NEG_INFINITY,
    };
    let depth = (reference_db - volume_db).clamp(0.0, MAX_DEPTH_DB);
    settings.strength_pct / 100.0 * depth
}

/// `db` rounded to [`DEPTH_STEP_DB`].
pub fn quantise_db(db: f64) -> f64 {
    (db / DEPTH_STEP_DB).round() / (1.0 / DEPTH_STEP_DB)
}

/// The low and the high shelf's gains in dB for the effective depth `d_e` (held to 0-40).
fn shelf_gains_db(d_e: f64) -> (f64, f64) {
    let d_e = match d_e.is_nan() {
        true => 0.0,
        false => d_e.clamp(0.0, MAX_DEPTH_DB),
    };
    (
        LOW_GAIN_PER_DB * d_e,
        (HIGH_GAIN_PER_DB * d_e).min(HIGH_GAIN_CAP_DB),
    )
}

/// The section `k (s + kz)/(s + kp)` of the bilinear domain as `[b0, b1, 0, a1, 0]`.
fn section(kz: f64, kp: f64, k: f64) -> [f64; 5] {
    [
        k * (1.0 + kz) / (1.0 + kp),
        k * (kz - 1.0) / (1.0 + kp),
        0.0,
        (kp - 1.0) / (1.0 + kp),
        0.0,
    ]
}

/// The prewarp constant `tan(π·f/rate)` of a corner, the corner held under Nyquist.
fn prewarp(corner_hz: f64, rate: f64) -> f64 {
    (PI * corner_hz.min(rate * 0.45) / rate).tan()
}

/// The two sections, low then high, as the f64 design gives them, with the low section not yet
/// scaled to cut only.
fn design(d_e: f64, rate_hz: u32) -> [[f64; 5]; 2] {
    let rate = f64::from(rate_hz.max(8_000));
    let (g_lo, g_hi) = shelf_gains_db(d_e);
    let low_zero = prewarp(LOW_CORNER_HZ, rate);
    let high_zero = prewarp(HIGH_CORNER_HZ, rate);
    let high_pole = high_zero * 10f64.powf(g_hi / 20.0);
    [
        section(low_zero, low_zero / 10f64.powf(g_lo / 20.0), 1.0),
        section(high_zero, high_pole, high_pole / high_zero),
    ]
}

/// `value` rounded to the f32 the chain's port holds.
fn f32_of(value: f64) -> f64 {
    f64::from(value as f32)
}

/// The sections with every coefficient rounded to f32.
fn rounded(sections: &[[f64; 5]; 2]) -> [[f64; 5]; 2] {
    sections.map(|set| set.map(f32_of))
}

/// The summed response of `sections` at `f` Hz at `rate`, in dB. Worked in `sin²(w/2)` so a
/// response near DC is not lost to cancellation.
fn sections_db(sections: &[[f64; 5]; 2], f: f64, rate: f64) -> f64 {
    let w = 2.0 * PI * f / rate;
    let x = (w / 2.0).sin().powi(2);
    let power = |p0: f64, p1: f64| (p0 + p1) * (p0 + p1) - 4.0 * p0 * p1 * x;
    sections
        .iter()
        .map(|c| 10.0 * (power(c[0], c[1]) / power(1.0, c[3])).log10())
        .sum()
}

/// The loudness stage of the chain: two first-order sections, each `[b0, b1, b2, a1, a2]` with
/// `a0` 1, the same on both channels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stage {
    /// The low shelf, then the high shelf, as the f32 values the chain's ports hold.
    pub sections: [[f64; 5]; 2],
}

/// The identity section `[1, 0, 0, 0, 0]`.
const IDENTITY: [f64; 5] = [1.0, 0.0, 0.0, 0.0, 0.0];

impl Stage {
    /// The stage that changes nothing.
    pub const fn identity() -> Stage {
        Stage {
            sections: [IDENTITY; 2],
        }
    }

    /// Whether this is the exact identity.
    pub fn is_identity(&self) -> bool {
        self.sections == [IDENTITY; 2]
    }

    /// The response of the stage as the chain plays it at `f` Hz at `rate_hz`, in dB.
    pub fn response_db(&self, f: f64, rate_hz: u32) -> f64 {
        sections_db(&self.sections, f, f64::from(rate_hz.max(1)))
    }
}

/// The stage for the effective depth `d_e` (dB, held to 0-40) at `rate_hz`, cut-only: its f32
/// maximum over DC to Nyquist is at most 0 dB. `d_e` of 0 is the exact identity.
pub fn stage(d_e: f64, rate_hz: u32) -> Stage {
    if d_e.is_nan() || d_e <= 0.0 {
        return Stage::identity();
    }
    let rate = rate_hz.max(8_000);
    let sets = rounded(&design(d_e, rate));
    let peak = played_peak_db_of(&sets, &[], rate).max(0.0);
    let scale = 10f64.powf(-peak / 20.0);
    let mut low = sets[0];
    low[0] = f32_of(low[0] * scale);
    low[1] = f32_of(low[1] * scale);
    let mut sets = [low, sets[1]];
    for _ in 0..TRIM_LIMIT {
        if played_peak_db_of(&sets, &[], rate) <= 0.0 {
            return Stage { sections: sets }.checked();
        }
        sets[0][0] = f64::from((sets[0][0] as f32).next_down());
    }
    Stage::identity()
}

impl Stage {
    /// The stage itself when every coefficient is a number the chain can hold and the poles are
    /// inside the unit circle, else the identity.
    fn checked(self) -> Stage {
        let holds = self
            .sections
            .iter()
            .flatten()
            .all(|c| c.abs() <= COEFFICIENT_LIMIT);
        let stable = self.sections.iter().all(|c| c[3].abs() < 1.0 - 1e-7);
        match holds && stable {
            true => self,
            false => Stage::identity(),
        }
    }
}

/// The response of the stage for the effective depth `d_e` at `f` Hz at `rate_hz`, in dB, as
/// the f64 design gives it (the oracle the played stage is measured against; the f32 stage is
/// within 0.05 dB of it from 20 Hz to 20 kHz).
pub fn response(d_e: f64, f: f64, rate_hz: u32) -> f64 {
    if d_e.is_nan() || d_e <= 0.0 {
        return 0.0;
    }
    let rate = rate_hz.max(8_000);
    let sets = design(d_e, rate);
    let peak = played_peak_db_of(&sets, &[], rate).max(0.0);
    sections_db(&sets, f, f64::from(rate)) - peak
}

/// The frequency of curve point `i`, in Hz.
fn curve_hz(i: usize) -> f64 {
    20.0 * 10f64.powf(i as f64 / 20.0)
}

/// The stage as played for the depth `d_e` at `rate_hz`: [`CURVE_POINTS`] pairs of frequency
/// (Hz, log-spaced from 20 Hz to 20 kHz) and level (dB).
pub fn curve(d_e: f64, rate_hz: u32) -> Vec<(f64, f64)> {
    let played = stage(d_e, rate_hz);
    (0..CURVE_POINTS)
        .map(|i| {
            let hz = curve_hz(i);
            (hz, played.response_db(hz, rate_hz))
        })
        .collect()
}

/// The ranges a control moves the loudness numbers within, so a front end hard-codes none.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoudnessRanges {
    /// The strength in percent.
    pub strength_pct: Range,
    /// The reference in percent of the volume scale.
    pub reference_pct: Range,
}

impl Default for LoudnessRanges {
    fn default() -> Self {
        LoudnessRanges {
            strength_pct: STRENGTH_RANGE,
            reference_pct: REFERENCE_RANGE,
        }
    }
}

/// The stage's response for a picture: `hz` and `db` pairwise, [`CURVE_POINTS`] of each, the
/// level to 0.01 dB.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoudnessCurve {
    /// The frequencies in Hz, log-spaced from 20 Hz to 20 kHz.
    pub hz: Vec<f64>,
    /// The level at each, in dB (at most 0).
    pub db: Vec<f64>,
}

/// The loudness as a screen shows it. Every field has a default, so a view from an older daemon
/// still parses.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LoudnessView {
    /// Whether it is on.
    pub on: bool,
    /// The strength in percent.
    pub strength_pct: f64,
    /// The reference in percent of the volume scale.
    pub reference_pct: f64,
    /// The computer's volume in percent, to 0.1; absent until it has been read.
    pub volume_pct: Option<f64>,
    /// The effective depth the stage plays now, in dB below the reference, to 0.1.
    pub depth_db: f64,
    /// How far the bass stands above the middle, in dB (`G_lo`), to 0.1.
    pub lift_db: f64,
    /// How far the middle sits under the volume's own cut, in dB (`−G_lo`, at most 0), to 0.1.
    pub mids_db: f64,
    /// How far the top stands above the middle, in dB (`G_hi`), to 0.1.
    pub treble_db: f64,
    /// Whether it follows the volume now: false while it is off, muted, unread or unavailable.
    pub following: bool,
    /// A sentence about what the controller is doing or waiting for.
    pub note: Option<String>,
    /// Why loudness cannot follow the volume now. Front ends show it; a change is still taken and
    /// saved, and acts once it can follow again.
    pub unavailable: Option<String>,
    /// The stage as played, for the picture.
    pub curve: LoudnessCurve,
    /// The ranges of the controls.
    pub ranges: LoudnessRanges,
}

impl LoudnessView {
    /// The view of `settings` (held to their ranges) with the volume `level` the controller last
    /// read, the effective depth `played_d_e` the stage plays now (0 while no stage plays), at
    /// `rate_hz`.
    pub fn of(
        settings: &LoudnessSettings,
        level: Option<Level>,
        played_d_e: f64,
        rate_hz: u32,
        note: Option<String>,
        unavailable: Option<String>,
    ) -> LoudnessView {
        let settings = settings.held();
        let played = quantise_db(match played_d_e.is_finite() {
            true => played_d_e.clamp(0.0, MAX_DEPTH_DB),
            false => 0.0,
        });
        let (g_lo, g_hi) = shelf_gains_db(played);
        let tenth = |db: f64| (db * 10.0).round() / 10.0;
        let curve = curve(played, rate_hz);
        LoudnessView {
            on: settings.on,
            strength_pct: settings.strength_pct,
            reference_pct: settings.reference_pct,
            volume_pct: level.and_then(|level| level.volume_pct()).map(tenth),
            depth_db: tenth(played),
            lift_db: tenth(g_lo),
            // `+ 0.0` turns -0.0 (depth 0) into 0.0, so no screen or JSON shows a minus sign.
            mids_db: tenth(-g_lo) + 0.0,
            treble_db: tenth(g_hi),
            following: settings.on
                && unavailable.is_none()
                && level.is_some_and(|level| !level.muted && level.volume_pct().is_some()),
            note,
            unavailable,
            curve: LoudnessCurve {
                hz: curve
                    .iter()
                    .map(|point| (point.0 * 100.0).round() / 100.0)
                    .collect(),
                db: curve
                    .iter()
                    .map(|point| (point.1 * 100.0).round() / 100.0)
                    .collect(),
            },
            ranges: LoudnessRanges::default(),
        }
    }
}

#[cfg(test)]
mod tests;
