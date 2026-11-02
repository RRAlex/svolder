// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How a meter's raw reading lands on its bar.
//!
//! A power amp reports each meter as a raw `u16` that reads on a logarithmic scale of the amp's
//! own: a zero at and below which the bar's live part is empty, a full-scale reading at and
//! above which it is full, and a class-A block over the start of the bar that no reading moves.
//! On the amps that report a meter floor, the zero follows that floor. A control with a `range`
//! instead reads on a straight line, which is also how the graphs scale a level.
//!
//! [`Profile::placed`] settles a control's scale against the readings a front end holds, and
//! every front end reads a meter through the [`Placed`] it returns rather than doing the sums
//! itself.

use svolder_proto::FunctionId;

use crate::model::{Control, Profile};

#[cfg(test)]
mod tests;

/// The key of the switch that turns an amp's meters on and off.
pub const POWER_METERS: &str = "power_meters";
/// The key of the switch that wakes a device from standby.
const POWER: &str = "power";
/// The key of the readout that says an awake amp is still warming up.
const WARMING_UP: &str = "warming_up";
/// The least a zero that follows a floor sits above it.
const FLOOR_MARGIN: i64 = 10;

/// Where a logarithmic meter's zero comes from.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Zero {
    /// A fixed raw reading.
    Fixed(i64),
    /// The floor the `u16` readout with this key reports, through [`zero_from_floor`].
    Floor(String),
}

/// A power amp's logarithmic meter scale, as a profile's `scale` table gives it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct LogScale {
    /// The raw reading at and below which the bar's live part is empty. A [`Zero::Floor`] key
    /// is checked on a concrete profile once it is fully resolved, so a base profile may name a
    /// floor only its children define.
    pub zero: Zero,
    /// The raw reading at and above which the live part is full.
    pub full: i64,
    /// How much of the bar, from its left end, the class-A block covers: `0.0..1.0`.
    pub class_a: f64,
}

/// The scale a control's readings are drawn against.
#[derive(Debug, Clone, PartialEq)]
pub enum Scale {
    /// A straight line over `min..=max`, filled from `floor` where the control names one, with
    /// a class-A `tick`.
    Linear {
        /// The bottom of the control's range.
        min: i64,
        /// The top of it.
        max: i64,
        /// The raw value a meter reads at idle.
        floor: Option<i64>,
        /// The raw value where a meter crosses into class-A operation.
        tick: Option<i64>,
    },
    /// A power amp's logarithmic scale.
    Log(LogScale),
}

/// A scale with its zero settled: what a reading is drawn against this moment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Placed {
    /// A straight line from `from` (empty) to `to` (full), with a class-A `tick`.
    Linear {
        /// The raw value the bar is empty at.
        from: i64,
        /// The raw value it is full at.
        to: i64,
        /// The raw value the class-A tick marks.
        tick: Option<i64>,
    },
    /// A logarithmic scale from `zero` (empty) to `full`, after a class-A block of `class_a`.
    Log {
        /// The raw value the live part is empty at.
        zero: i64,
        /// The raw value it is full at.
        full: i64,
        /// How much of the bar the class-A block covers.
        class_a: f64,
    },
}

impl Placed {
    /// How far up the bar's live part `raw` reaches, `0.0..=1.0`: on a log scale
    /// `log10(max(0, raw - zero) + 1) / log10(full - zero + 1)`, on a linear one the distance
    /// above `from` over the span. Nothing at all where the scale has no span (a zero that a
    /// floor has pushed to full scale or past it).
    pub fn fraction(&self, raw: f64) -> f64 {
        if raw.is_nan() {
            return 0.0;
        }
        match *self {
            Placed::Linear { from, to, .. } => {
                let span = (to - from) as f64;
                match span > 0.0 {
                    true => ((raw - from as f64) / span).clamp(0.0, 1.0),
                    false => 0.0,
                }
            }
            Placed::Log { zero, full, .. } if full <= zero => 0.0,
            Placed::Log { zero, full, .. } => {
                let above = (raw - zero as f64).max(0.0);
                ((above + 1.0).log10() / ((full - zero + 1) as f64).log10()).clamp(0.0, 1.0)
            }
        }
    }

    /// The raw reading that reaches `p` of the live part, the inverse of [`Placed::fraction`]:
    /// `zero + (full - zero + 1)^p - 1` rounded on a log scale, `from + p * span` truncated on a
    /// linear one. `p` is clamped to `0.0..=1.0`, and no level (`NaN`) is the bottom.
    pub fn raw_at(&self, p: f64) -> i64 {
        let p = match p.is_nan() {
            true => 0.0,
            false => p.clamp(0.0, 1.0),
        };
        match *self {
            Placed::Linear { from, to, .. } => from + (p * (to - from) as f64) as i64,
            Placed::Log { zero, full, .. } if full <= zero => zero,
            Placed::Log { zero, full, .. } => {
                zero + (((full - zero + 1) as f64).powf(p) - 1.0).round() as i64
            }
        }
    }

    /// How much of the bar, from its left end, the class-A block covers: none on a linear
    /// scale, whose class-A mark is its [`Placed::tick`].
    pub fn class_a(&self) -> f64 {
        match *self {
            Placed::Log { class_a, .. } => class_a,
            Placed::Linear { .. } => 0.0,
        }
    }

    /// Where a linear scale's class-A tick falls, as a fraction of the bar.
    pub fn tick(&self) -> Option<f64> {
        match *self {
            Placed::Linear {
                tick: Some(tick), ..
            } => Some(self.fraction(tick as f64)),
            _ => None,
        }
    }
}

/// The zero a meter that follows a floor reading `m` reads from: the floor plus half again,
/// and at least 10 above it, so the amp's idle level stays off the bar. 0 before
/// the floor has been reported.
pub fn zero_from_floor(m: Option<i64>) -> i64 {
    m.map_or(0, |m| m + m.div_euclid(2).max(FLOOR_MARGIN))
}

impl Control {
    /// The scale this control's readings are drawn against: its `scale` table, else a straight
    /// line over its `range` (from its `floor`, with its `tick`), else none.
    pub fn scale(&self) -> Option<Scale> {
        if let Some(log) = &self.log_scale {
            return Some(Scale::Log(log.clone()));
        }
        self.range.map(|(min, max)| Scale::Linear {
            min,
            max,
            floor: self.floor,
            tick: self.tick,
        })
    }
}

impl Profile {
    /// `control`'s scale with its zero settled, a floor it follows read through `read` (the
    /// raw value a front end holds for a function, if any). `None` for a control with no scale.
    pub fn placed(
        &self,
        control: &Control,
        read: impl Fn(FunctionId) -> Option<i64>,
    ) -> Option<Placed> {
        Some(match control.scale()? {
            Scale::Linear {
                min,
                max,
                floor,
                tick,
            } => Placed::Linear {
                from: floor.unwrap_or(min),
                to: max,
                tick,
            },
            Scale::Log(LogScale {
                zero,
                full,
                class_a,
            }) => Placed::Log {
                zero: match zero {
                    Zero::Fixed(raw) => raw,
                    Zero::Floor(key) => {
                        zero_from_floor(self.control_by_key(&key).and_then(|floor| read(floor.id)))
                    }
                },
                full,
                class_a,
            },
        })
    }

    /// The readout whose floor a meter's zero follows, when it follows one.
    pub fn zero_source(&self, control: &Control) -> Option<&Control> {
        match &control.log_scale.as_ref()?.zero {
            Zero::Floor(key) => self.control_by_key(key),
            Zero::Fixed(_) => None,
        }
    }

    /// Whether `readout` is a floor some meter's zero follows: it places a scale rather than
    /// saying something on its own.
    pub fn sets_a_zero(&self, readout: &Control) -> bool {
        self.controls.iter().any(|meter| {
            self.zero_source(meter)
                .is_some_and(|floor| floor.id == readout.id)
        })
    }

    /// Whether the meters are live: the meter switch does not read off (a profile without one
    /// has nothing to switch off), and the amp neither reads asleep nor reads warming up. `on`
    /// gives how a function reads as on or off, `None` before it has reported. Only a reported
    /// state counts: meters whose switch has not reported yet are live.
    pub fn meters_live(&self, on: impl Fn(FunctionId) -> Option<bool>) -> bool {
        let reads = |key: &str| self.control_by_key(key).and_then(|control| on(control.id));
        reads(POWER_METERS) != Some(false)
            && reads(POWER) != Some(false)
            && reads(WARMING_UP) != Some(true)
    }
}
