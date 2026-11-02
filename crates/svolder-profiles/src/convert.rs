// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Raw wire values to human units and back, for `Level` controls and the EQ.

use std::cmp::Ordering;
use std::str::FromStr;

use crate::db_tables;
use crate::error::ProfileError;
use crate::model::{Control, Convert, EqSpec};
use svolder_proto::FunctionId;

/// Human value for a raw wire value, or `None` when the control has no conversion.
pub fn raw_to_display(control: &Control, raw: i64) -> Option<f64> {
    match control.convert.as_ref()? {
        Convert::Table(name) => {
            let (lo, hi) = control.range?;
            if raw < lo || raw > hi {
                return None;
            }
            let index = usize::try_from(raw).ok()?;
            db_tables::table(name)?.get(index).copied()
        }
        Convert::Linear { min, max } => {
            let (lo, hi) = control.range?;
            if hi == lo {
                return Some(*min);
            }
            let fraction = (i128::from(raw) - i128::from(lo)) as f64
                / (i128::from(hi) - i128::from(lo)) as f64;
            Some(min + (max - min) * fraction)
        }
    }
}

/// Nearest raw wire value for a human value, clamped to the control's range.
pub fn display_to_raw(control: &Control, value: f64) -> Option<i64> {
    let (lo, hi) = control.range?;
    match control.convert.as_ref()? {
        Convert::Table(name) => {
            let table = db_tables::table(name)?;
            // Distance that treats an exact match (including infinities) as zero.
            let distance = |entry: f64| {
                if entry == value {
                    0.0
                } else {
                    (entry - value).abs()
                }
            };
            let (index, _) = table.iter().enumerate().min_by(|a, b| {
                distance(*a.1)
                    .partial_cmp(&distance(*b.1))
                    .unwrap_or(Ordering::Equal)
            })?;
            let raw = i64::try_from(index).ok()?;
            (raw >= lo && raw <= hi).then_some(raw)
        }
        Convert::Linear { min, max } => {
            if max == min {
                return Some(lo);
            }
            let fraction = (value - min) / (max - min);
            let raw = lo as f64 + fraction * (i128::from(hi) - i128::from(lo)) as f64;
            Some(raw.round().clamp(lo as f64, hi as f64) as i64)
        }
    }
}

/// Which of a band's three parameters a message addresses; the value is the third id byte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EqParam {
    /// The band's Q.
    Q = 1,
    /// The band's centre frequency.
    Frequency = 2,
    /// The band's gain.
    Gain = 3,
}

impl EqParam {
    /// The parameter a third id byte addresses, or `None` for any other byte.
    pub fn from_sub(sub: u8) -> Option<Self> {
        match sub {
            1 => Some(EqParam::Q),
            2 => Some(EqParam::Frequency),
            3 => Some(EqParam::Gain),
            _ => None,
        }
    }

    /// The word the parameter is typed and sent as: `f`, `g` or `q`.
    pub fn word(self) -> &'static str {
        match self {
            EqParam::Frequency => "f",
            EqParam::Gain => "g",
            EqParam::Q => "q",
        }
    }
}

impl FromStr for EqParam {
    type Err = ProfileError;

    /// `f`/`frequency`, `g`/`gain` or `q`, in any case.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.trim().to_ascii_lowercase().as_str() {
            "f" | "frequency" => Ok(EqParam::Frequency),
            "g" | "gain" => Ok(EqParam::Gain),
            "q" => Ok(EqParam::Q),
            other => Err(ProfileError::BadEqWord(format!(
                "{other:?} is not a band parameter; use f, g, or q"
            ))),
        }
    }
}

/// Tone-control mode, the value of the `E.mode_sub` function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum EqMode {
    /// Tone control bypassed.
    Flat = 1,
    /// The band settings applied.
    Eq = 2,
    /// The loudness curve applied.
    Loudness = 3,
}

impl EqMode {
    /// The mode a raw value names, or `None` for any other value.
    pub fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            1 => Some(EqMode::Flat),
            2 => Some(EqMode::Eq),
            3 => Some(EqMode::Loudness),
            _ => None,
        }
    }

    /// The lowercase name a user types and reads.
    pub fn name(self) -> &'static str {
        match self {
            EqMode::Flat => "flat",
            EqMode::Eq => "eq",
            EqMode::Loudness => "loudness",
        }
    }

    /// The name the mode is shown under in a front end: `Flat`, `EQ`, `Loudness`.
    pub fn display(self) -> &'static str {
        match self {
            EqMode::Flat => "Flat",
            EqMode::Eq => "EQ",
            EqMode::Loudness => "Loudness",
        }
    }
}

impl FromStr for EqMode {
    type Err = ProfileError;

    /// `flat`, `eq` or `loudness`, in any case.
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.trim().to_ascii_lowercase().as_str() {
            "flat" => Ok(EqMode::Flat),
            "eq" => Ok(EqMode::Eq),
            "loudness" => Ok(EqMode::Loudness),
            other => Err(ProfileError::BadEqWord(format!(
                "{other:?} is not a tone-control mode; use flat, eq, or loudness"
            ))),
        }
    }
}

/// A band's centre frequency as a user reads it: to the tenth of a hertz below 100 Hz, where one
/// of a band's frequency steps is less than a hertz (`40.4`), and in whole hertz from 100 Hz up
/// (`160`, `10000`), where a step is more than one. Two neighbouring steps never read the same.
pub fn frequency_text(hz: f64) -> String {
    let tenths = format!("{hz:.1}");
    match tenths.parse::<f64>().is_ok_and(|shown| shown < 100.0) {
        true => tenths,
        false => format!("{hz:.0}"),
    }
}

/// A band's gain as its row shows it: signed, to the hundredth of a dB a device step lands on —
/// `+1.45`, `-0.20` — and a whole number of dB to the tenth, `+0.0`, `+6.0`.
pub fn gain_text(db: f64) -> String {
    // A gain of exactly 0 dB reads `+0.0` whichever zero the conversion handed over.
    let db = if db == 0.0 { 0.0 } else { db };
    let text = format!("{db:+.2}");
    match text.strip_suffix("00") {
        Some(whole) => format!("{whole}0"),
        None => text,
    }
}

/// A band's Q as its row shows it, to the hundredth.
pub fn q_text(q: f64) -> String {
    format!("{q:.2}")
}

impl EqSpec {
    /// The highest raw step a band's frequency, gain and Q take, the lowest being 0: the top of
    /// the frequency window, the highest gain and the narrowest Q.
    pub const STEP_MAX: u8 = 127;

    /// The raw step nearest `value` for one parameter of band `band` — a frequency in Hz, a gain
    /// in dB or a Q — clamped to the band's steps, as a write converts it. `None` for a band
    /// that does not exist, or a frequency that is not finite and positive.
    pub fn step(&self, band: usize, param: EqParam, value: f64) -> Option<u8> {
        match param {
            EqParam::Frequency => self.frequency_raw(band, value),
            EqParam::Gain => (band < self.bands.len()).then(|| self.gain_raw(value)),
            EqParam::Q => (band < self.bands.len()).then(|| self.q_raw(value)),
        }
    }

    /// Centre frequency in Hz for a band's raw frequency value.
    pub fn frequency(&self, band: usize, raw: u8) -> Option<f64> {
        let b = self.bands.get(band)?;
        let fscale = if raw < 64 {
            b.fscale_low
        } else {
            b.fscale_high
        };
        let steps = f64::from(raw.min(Self::STEP_MAX)) - 64.0;
        Some(b.center * (steps * fscale * self.range).exp())
    }

    /// Raw frequency value nearest a frequency in Hz, clamped to the band's range.
    /// `None` for a band that does not exist or a frequency that is not finite and positive.
    pub fn frequency_raw(&self, band: usize, hz: f64) -> Option<u8> {
        let steps = self.frequency_steps(band, hz)?.round();
        Some(steps.clamp(0.0, f64::from(Self::STEP_MAX)) as u8)
    }

    /// Where a frequency in Hz falls on a band's raw scale, neither rounded nor clamped: 64 at
    /// the centre, and below 0 or above [`STEP_MAX`](Self::STEP_MAX) outside the band's window.
    /// `None` as for [`frequency_raw`](Self::frequency_raw).
    pub(crate) fn frequency_steps(&self, band: usize, hz: f64) -> Option<f64> {
        let b = self.bands.get(band)?;
        if !(hz > 0.0 && hz.is_finite()) {
            return None;
        }
        let fscale = if hz >= b.center {
            b.fscale_high
        } else {
            b.fscale_low
        };
        Some((hz / b.center).ln() / self.range / fscale + 64.0)
    }

    /// Gain in dB for a raw gain value, `gain_zero` mapping to 0 dB.
    pub fn gain_db(&self, raw: u8) -> f64 {
        let (min, max) = self.gain_range;
        let zero = f64::from(self.gain_zero);
        let top = f64::from(Self::STEP_MAX);
        let v = f64::from(raw);
        if raw >= Self::STEP_MAX {
            max
        } else if raw == 0 {
            min
        } else if v < zero {
            min + v * (min.abs() / zero)
        } else {
            (v - zero) * (max / (top - zero))
        }
    }

    /// Raw gain value nearest a gain in dB, clamped to `gain_range`.
    pub fn gain_raw(&self, db: f64) -> u8 {
        let (min, max) = self.gain_range;
        let zero = f64::from(self.gain_zero);
        if db <= min {
            0
        } else if db >= max {
            Self::STEP_MAX
        } else if db < 0.0 {
            (zero / min.abs() * (db + min.abs())).round() as u8
        } else {
            (zero + (f64::from(Self::STEP_MAX) - zero) / max * db).round() as u8
        }
    }

    /// Q for a raw Q value, `q_center` sitting at raw 64.
    pub fn q(&self, raw: u8) -> f64 {
        let (qmin, qmax) = self.q_range;
        if raw < 64 {
            qmin + f64::from(raw) * ((self.q_center - qmin) / 64.0)
        } else {
            self.q_center
                + f64::from(raw.min(Self::STEP_MAX) - 64)
                    * ((qmax - self.q_center) / f64::from(Self::STEP_MAX - 64))
        }
    }

    /// Raw Q value nearest a Q, clamped to `q_range`.
    pub fn q_raw(&self, q: f64) -> u8 {
        let (qmin, qmax) = self.q_range;
        if q <= qmin {
            0
        } else if q >= qmax {
            Self::STEP_MAX
        } else if q < self.q_center {
            ((q - qmin) / ((self.q_center - qmin) / 64.0)).round() as u8
        } else {
            let above = (qmax - self.q_center) / f64::from(Self::STEP_MAX - 64);
            64 + ((q - self.q_center) / above).round() as u8
        }
    }

    /// Function id of one band parameter, `E.<band+1>.<param>`.
    pub fn band_id(&self, band: usize, param: EqParam) -> Option<FunctionId> {
        (band < self.bands.len()).then(|| FunctionId::three(self.id, (band + 1) as u8, param as u8))
    }

    /// Function id of the tone-control mode, `E.mode_sub`.
    pub fn mode_id(&self) -> FunctionId {
        FunctionId::two(self.id, self.mode_sub)
    }

    /// Function id of the loudness strength, `E.loudness_sub`.
    pub fn loudness_id(&self) -> FunctionId {
        FunctionId::two(self.id, self.loudness_sub)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ControlKind, EqBand};
    use svolder_proto::ValueType;

    /// A mode shows under a name of its own, and that name reads as the typed one in any case.
    #[test]
    fn a_mode_displays_as_its_typed_name_in_the_case_a_user_reads() {
        for (mode, shown) in [
            (EqMode::Flat, "Flat"),
            (EqMode::Eq, "EQ"),
            (EqMode::Loudness, "Loudness"),
        ] {
            assert_eq!(mode.display(), shown);
            assert_eq!(mode.display().to_lowercase(), mode.name());
            assert_eq!(shown.parse::<EqMode>().unwrap(), mode);
        }
    }

    fn level(range: (i64, i64), convert: Convert) -> Control {
        Control {
            id: FunctionId::one(10),
            key: "volume".into(),
            label: "Volume".into(),
            kind: ControlKind::Level,
            value_type: ValueType::U8,
            values: vec![],
            range: Some(range),
            center: None,
            unit: Some("dB".into()),
            convert: Some(convert),
            channel: None,
            floor: None,
            tick: None,
            log_scale: None,
            advice: None,
            only_while: None,
            auto_off: false,
            output_gain: false,
            mutes_output: false,
            powers_output: false,
            asks_on_change: false,
            reads: Vec::new(),
        }
    }

    fn ygg_eq() -> EqSpec {
        EqSpec {
            id: 12,
            mode_sub: 50,
            loudness_sub: 52,
            range: 1.95,
            bands: vec![
                EqBand {
                    center: 40.0,
                    fscale_low: 0.005554,
                    fscale_high: 0.00544,
                },
                EqBand {
                    center: 160.0,
                    fscale_low: 0.00536,
                    fscale_high: 0.005435,
                },
                EqBand {
                    center: 632.0,
                    fscale_low: 0.005435,
                    fscale_high: 0.005435,
                },
                EqBand {
                    center: 2515.0,
                    fscale_low: 0.005435,
                    fscale_high: 0.005435,
                },
                EqBand {
                    center: 10000.0,
                    fscale_low: 0.005435,
                    fscale_high: 0.005435,
                },
            ],
            gain_range: (-20.0, 6.0),
            gain_zero: 98,
            q_range: (0.5, 8.0),
            q_center: 4.3,
        }
    }

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    #[test]
    fn table_lookup_and_nearest() {
        let c = level((0, 127), Convert::Table("dac-attenuation-db".into()));
        assert_eq!(raw_to_display(&c, 127), Some(0.0));
        assert_eq!(raw_to_display(&c, 109), Some(-6.021));
        assert_eq!(raw_to_display(&c, 0), Some(f64::NEG_INFINITY));
        assert_eq!(raw_to_display(&c, 128), None);
        assert_eq!(raw_to_display(&c, 200), None);
        assert_eq!(raw_to_display(&c, -1), None);
        assert_eq!(display_to_raw(&c, 0.0), Some(127));
        assert_eq!(display_to_raw(&c, -6.0), Some(109));
        assert_eq!(display_to_raw(&c, -6.2), Some(108));
        assert_eq!(display_to_raw(&c, -200.0), Some(1));
        assert_eq!(display_to_raw(&c, f64::NEG_INFINITY), Some(0));
    }

    #[test]
    fn linear_map_and_inverse() {
        let c = level(
            (0, 127),
            Convert::Linear {
                min: -80.0,
                max: 0.0,
            },
        );
        assert_eq!(raw_to_display(&c, 127), Some(0.0));
        assert_eq!(raw_to_display(&c, 0), Some(-80.0));
        assert!(close(raw_to_display(&c, 64).unwrap(), -39.685, 0.001));
        assert_eq!(display_to_raw(&c, 0.0), Some(127));
        assert_eq!(display_to_raw(&c, -80.0), Some(0));
        assert_eq!(display_to_raw(&c, -40.0), Some(64));
        assert_eq!(display_to_raw(&c, 5.0), Some(127));
        assert_eq!(display_to_raw(&c, -100.0), Some(0));
    }

    #[test]
    fn no_conversion_gives_none() {
        let mut c = level((0, 127), Convert::Linear { min: 0.0, max: 1.0 });
        c.convert = None;
        assert_eq!(raw_to_display(&c, 5), None);
        assert_eq!(display_to_raw(&c, 5.0), None);
    }

    #[test]
    fn eq_frequency_matches_the_app_band_edges() {
        let eq = ygg_eq();
        assert_eq!(eq.frequency(0, 64), Some(40.0));
        assert!(close(eq.frequency(0, 0).unwrap(), 20.0, 0.05));
        assert!(close(eq.frequency(0, 127).unwrap(), 78.0, 0.1));
        assert_eq!(eq.frequency(1, 64), Some(160.0));
        assert_eq!(eq.frequency(5, 64), None);
        assert_eq!(eq.frequency_raw(0, 40.0), Some(64));
        assert_eq!(eq.frequency_raw(0, 20.0), Some(0));
        assert_eq!(eq.frequency_raw(0, 78.0), Some(127));
        assert_eq!(eq.frequency_raw(0, 5.0), Some(0));
        assert_eq!(eq.frequency_raw(0, 5000.0), Some(127));
        assert_eq!(eq.frequency_raw(1, 160.0), Some(64));
        assert_eq!(eq.frequency_raw(9, 160.0), None);
        assert_eq!(eq.frequency_raw(0, 0.0), None);
        assert_eq!(eq.frequency_raw(0, -5.0), None);
        assert_eq!(eq.frequency_raw(0, f64::NAN), None);
        assert_eq!(eq.frequency_raw(0, f64::INFINITY), None);
    }

    #[test]
    fn eq_gain_uses_the_zero_point() {
        let eq = ygg_eq();
        assert_eq!(eq.gain_db(98), 0.0);
        assert_eq!(eq.gain_db(0), -20.0);
        assert_eq!(eq.gain_db(127), 6.0);
        assert!(close(eq.gain_db(49), -10.0, 1e-9));
        assert!(close(eq.gain_db(112), 14.0 * 6.0 / 29.0, 1e-9));
        assert_eq!(eq.gain_raw(0.0), 98);
        assert_eq!(eq.gain_raw(-10.0), 49);
        assert_eq!(eq.gain_raw(-20.0), 0);
        assert_eq!(eq.gain_raw(-30.0), 0);
        assert_eq!(eq.gain_raw(6.0), 127);
        assert_eq!(eq.gain_raw(9.0), 127);
    }

    #[test]
    fn eq_q_is_piecewise_linear_around_the_centre() {
        let eq = ygg_eq();
        assert_eq!(eq.q(0), 0.5);
        assert_eq!(eq.q(64), 4.3);
        assert!(close(eq.q(127), 8.0, 1e-9));
        assert_eq!(eq.q_raw(0.5), 0);
        assert_eq!(eq.q_raw(4.3), 64);
        assert_eq!(eq.q_raw(8.0), 127);
        assert_eq!(eq.q_raw(2.1), 27);
        assert_eq!(eq.q_raw(0.1), 0);
        assert_eq!(eq.q_raw(9.0), 127);
    }

    #[test]
    fn eq_ids() {
        let eq = ygg_eq();
        assert_eq!(eq.band_id(0, EqParam::Q), Some(FunctionId::three(12, 1, 1)));
        assert_eq!(
            eq.band_id(0, EqParam::Frequency),
            Some(FunctionId::three(12, 1, 2))
        );
        assert_eq!(
            eq.band_id(4, EqParam::Gain),
            Some(FunctionId::three(12, 5, 3))
        );
        assert_eq!(eq.band_id(5, EqParam::Gain), None);
        assert_eq!(eq.mode_id(), FunctionId::two(12, 50));
        assert_eq!(eq.loudness_id(), FunctionId::two(12, 52));
        assert_eq!(EqParam::from_sub(1), Some(EqParam::Q));
        assert_eq!(EqParam::from_sub(2), Some(EqParam::Frequency));
        assert_eq!(EqParam::from_sub(3), Some(EqParam::Gain));
        assert_eq!(EqParam::from_sub(4), None);
        assert_eq!(EqMode::from_raw(3), Some(EqMode::Loudness));
        assert_eq!(EqMode::Eq.name(), "eq");
    }

    #[test]
    fn band_parameters_parse_from_their_names() {
        for text in ["f", "F", "frequency", "Frequency"] {
            assert_eq!(
                text.parse::<EqParam>().unwrap(),
                EqParam::Frequency,
                "{text}"
            );
        }
        for text in ["g", "gain", "GAIN"] {
            assert_eq!(text.parse::<EqParam>().unwrap(), EqParam::Gain, "{text}");
        }
        for text in ["q", "Q"] {
            assert_eq!(text.parse::<EqParam>().unwrap(), EqParam::Q, "{text}");
        }
        let err = "wobble".parse::<EqParam>().unwrap_err();
        assert!(matches!(err, ProfileError::BadEqWord(_)), "got {err:?}");
        assert!(err.to_string().contains("wobble"), "got {err}");
    }

    #[test]
    fn tone_control_modes_parse_from_their_names() {
        assert_eq!("flat".parse::<EqMode>().unwrap(), EqMode::Flat);
        assert_eq!("EQ".parse::<EqMode>().unwrap(), EqMode::Eq);
        assert_eq!("Loudness".parse::<EqMode>().unwrap(), EqMode::Loudness);
        let err = "boomy".parse::<EqMode>().unwrap_err();
        assert!(matches!(err, ProfileError::BadEqWord(_)), "got {err:?}");
        assert!(err.to_string().contains("boomy"), "got {err}");
    }

    /// `step` is the one conversion a write goes through: every raw step of every parameter of
    /// a built-in tone control comes back as itself from the value it stands for, a value past
    /// either end goes to the end, and a band the tone control does not have has no step.
    #[test]
    fn a_value_goes_to_its_nearest_step_and_every_step_comes_back_as_itself() {
        let registry = crate::Registry::builtin().expect("the built-in profiles load");
        let eq = registry
            .get("mesh-eq-5")
            .and_then(|profile| profile.eq.clone())
            .expect("a tone control");
        for band in 0..eq.bands.len() {
            for raw in 0..=EqSpec::STEP_MAX {
                let hz = eq.frequency(band, raw).expect("a band");
                assert_eq!(eq.step(band, EqParam::Frequency, hz), Some(raw));
                assert_eq!(eq.step(band, EqParam::Gain, eq.gain_db(raw)), Some(raw));
                assert_eq!(eq.step(band, EqParam::Q, eq.q(raw)), Some(raw));
            }
        }
        assert_eq!(eq.step(0, EqParam::Gain, 40.0), Some(EqSpec::STEP_MAX));
        assert_eq!(eq.step(0, EqParam::Q, 0.0), Some(0));
        assert_eq!(eq.step(9, EqParam::Gain, 0.0), None);
        assert_eq!(eq.step(0, EqParam::Frequency, -1.0), None);
    }

    /// A frequency reads to the tenth of a hertz below 100 Hz and in whole hertz from there up,
    /// a value that rounds to 100 reading as the whole number; and no two steps of any band of
    /// any built-in tone control read the same.
    #[test]
    fn a_frequency_reads_finely_enough_to_tell_every_step_apart() {
        for (hz, wanted) in [
            (40.0, "40.0"),
            (40.44, "40.4"),
            (20.69, "20.7"),
            (99.94, "99.9"),
            (99.96, "100"),
            (160.0, "160"),
            (311.95, "312"),
            (10000.0, "10000"),
        ] {
            assert_eq!(frequency_text(hz), wanted, "{hz}");
        }
        let registry = crate::Registry::builtin().expect("the built-in profiles load");
        let mut bands = 0;
        for name in registry.names() {
            let Some(eq) = registry.get(name).and_then(|profile| profile.eq.clone()) else {
                continue;
            };
            for band in 0..eq.bands.len() {
                let texts: Vec<String> = (0u8..=127)
                    .map(|raw| frequency_text(eq.frequency(band, raw).expect("a band")))
                    .collect();
                for pair in texts.windows(2) {
                    assert_ne!(pair[0], pair[1], "{name} band {}", band + 1);
                }
                bands += 1;
            }
        }
        assert!(bands >= 8, "only {bands} bands checked");
    }
}
