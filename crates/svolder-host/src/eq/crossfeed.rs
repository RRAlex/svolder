// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's crossfeed: what it is set to, the maths that turns the settings into the
//! coefficients of the chain's crossfeed stage, and the view a front end draws from.
//!
//! Everything here is pure. A crossfeed feeds a low-passed, delayed copy of each
//! channel into the other channel at a set level. The stage sits after the
//! EQ in the one filter chain (see [`live`](super::live)) and has the same shape for every
//! model: for each output channel `C`, with `O` the other channel,
//!
//! ```text
//! out_C = H_dir · C  +  D · (H_s · C  +  H_x · O)
//! ```
//!
//! where `H_dir` is the direct path, `H_x` the cross path, `H_s` a side-subtract path on the
//! channel's own signal and `D` the added delay (a pure phase). A section is `[b0, b1, b2, a1,
//! a2]` with `a0` 1, the transfer function `(b0 + b1 z⁻¹ + b2 z⁻²) / (1 + a1 z⁻¹ + a2 z⁻²)`,
//! the convention of `svolder_profiles::tone::response::coefficients` and of the chain's
//! `bq_raw` nodes. A first-order section has `b2 = a2 = 0`. The mixer cannot take a negative
//! gain, so a subtracting path carries its sign in its numerator.
//!
//! - **Bauer** is the Bauer stereophonic-to-binaural crossfeed as the bs2b library sets it:
//!   a high-boosted direct path and a low-passed cross path whose feed level, in dB, is how
//!   far the cross path sits below the direct one at low frequencies.
//! - **Linkwitz**: `H_dir = 1 − A`, `H_x = A · LP1(fc)`.
//! - **Meier**: `H_dir = 1`, `H_x = A · LP1(fc)`, `H_s = −A · LP1(fc)`: only what differs
//!   between the channels is crossed.
//! - **Mid/Side bass**: `H_dir = 1`, `H_x = A · LP2(fc)`, `H_s = −A · LP2(fc)` with a second-order
//!   Butterworth low pass: the bass below `fc` is blended toward mono, at 50 % fully, and the
//!   centre and the highs pass untouched.
//!
//! `A` is the share of the other channel at low frequencies, from 0 to 0.5 (0.5 is mono).
//! `LP1(fc) = (1 − p) / (1 − p z⁻¹)` with `p = exp(−2π fc / rate)`.
//!
//! The table [`SPECS`] is the one place that holds the models' names, presets and ranges; a
//! front end reads them from [`CrossfeedView::models`] and hard-codes none.

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

/// The most a coefficient port of the chain can hold, either way (PipeWire clamps to it).
const COEFFICIENT_LIMIT: f64 = 10.0;

/// The margin of the stability test, the same as the EQ's: the second Jury condition
/// `1 + a2 − |a1|` and `1 − |a2|` must stay above it in the f32 coefficients the chain runs.
const STABLE_MARGIN: f64 = 1e-7;

/// The frequency, in Hz, the model's own lag is read at.
const LAG_PROBE_HZ: f64 = 20.0;

/// The ceiling of the stage's added delay, in seconds: the chain's delay node is built for it
/// (`max-delay`, see [`live`](super::live)).
pub const DELAY_BUFFER_S: f64 = 0.002;

/// The crossfeed model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Model {
    /// The bs2b maths of Bauer's crossfeed.
    #[default]
    Bauer,
    /// Linkwitz's passive RC crossfeed.
    Linkwitz,
    /// Jan Meier's natural crossfeed: only the side signal is crossed.
    Meier,
    /// The bass blended toward mono below a crossover.
    MidSide,
}

impl Model {
    /// Every model, in the order a front end lists them.
    pub const ALL: [Model; 4] = [Model::Bauer, Model::Linkwitz, Model::Meier, Model::MidSide];

    /// The word the wire, the state file and the command line use.
    pub fn word(self) -> &'static str {
        match self {
            Model::Bauer => "bauer",
            Model::Linkwitz => "linkwitz",
            Model::Meier => "meier",
            Model::MidSide => "mid-side",
        }
    }

    /// The name a screen shows.
    pub fn name(self) -> &'static str {
        match self {
            Model::Bauer => "Bauer",
            Model::Linkwitz => "Linkwitz",
            Model::Meier => "Meier",
            Model::MidSide => "Mid/Side bass",
        }
    }

    /// The model a [`word`](Model::word) names.
    pub fn from_word(word: &str) -> Option<Model> {
        Model::ALL.into_iter().find(|model| model.word() == word)
    }

    /// The model's row of the table.
    pub fn spec(self) -> &'static ModelSpec {
        &SPECS[self as usize]
    }
}

/// A range a value is held to, with the step a control moves it by.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Range {
    /// The least.
    pub min: f64,
    /// The most.
    pub max: f64,
    /// The step of a control.
    pub step: f64,
}

impl Range {
    /// `value` rounded to the nearest step from the least, then held between the ends.
    pub fn snap(&self, value: f64) -> f64 {
        let steps = ((value - self.min) / self.step).round();
        let snapped = ((self.min + steps * self.step) * 1e6).round() / 1e6;
        snapped.clamp(self.min, self.max)
    }

    /// `value` held between the ends; `fallback` for a number that is not one.
    fn hold(&self, value: f64, fallback: f64) -> f64 {
        match value.is_nan() {
            true => fallback,
            false => value.clamp(self.min, self.max),
        }
    }
}

/// How a preset states its amount.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Amount {
    /// The share of the other channel at low frequencies, from 0 to 0.5.
    Share(f64),
    /// bs2b's feed level in dB, kept as its authors wrote it so it stays exact.
    FeedDb(f64),
}

/// One preset of a model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PresetSpec {
    /// The name a screen shows and the wire carries.
    pub name: &'static str,
    /// The crossover in Hz.
    pub crossover_hz: f64,
    /// The amount.
    pub amount: Amount,
}

impl PresetSpec {
    /// The share of the other channel at low frequencies.
    pub fn share(&self) -> f64 {
        match self.amount {
            Amount::Share(share) => share,
            Amount::FeedDb(db) => amount_of_feed_db(db),
        }
    }

    /// The preset's bs2b feed level in dB, for a Bauer preset.
    pub fn feed_db(&self) -> Option<f64> {
        match self.amount {
            Amount::FeedDb(db) => Some(db),
            Amount::Share(_) => None,
        }
    }
}

/// One model's row: its presets (mildest first) and its ranges.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelSpec {
    /// The model.
    pub model: Model,
    /// One line about the model, for a tooltip or the line under the model picker.
    pub description: &'static str,
    /// The presets, from the mildest to the strongest.
    pub presets: &'static [PresetSpec],
    /// The crossover range in Hz.
    pub crossover_hz: Range,
    /// The amount range in percent.
    pub amount_pct: Range,
    /// The most the total delay can be, in ms.
    pub delay_max_ms: f64,
    /// The step of the delay control, in ms.
    pub delay_step_ms: f64,
    /// Whether the delay can be set (false: the model plays none).
    pub delay_settable: bool,
    /// The index of the preset the model loads when it is chosen.
    pub default_preset: usize,
}

impl ModelSpec {
    /// The preset the model loads when it is chosen.
    pub fn default_preset(&self) -> &'static PresetSpec {
        &self.presets[self.default_preset]
    }
}

/// The table of models, indexed by `Model as usize`. The Linkwitz, Meier and Mid/Side bass numbers
/// are placeholders until they are checked against their sources.
pub static SPECS: [ModelSpec; 4] = [
    ModelSpec {
        model: Model::Bauer,
        description: "Feeds a low-passed, delayed copy of each channel into the other channel at a set level, as the bs2b library does.",
        presets: &[
            PresetSpec {
                name: "Jan Meier",
                crossover_hz: 650.0,
                amount: Amount::FeedDb(9.5),
            },
            PresetSpec {
                name: "Chu Moy",
                crossover_hz: 700.0,
                amount: Amount::FeedDb(6.0),
            },
            PresetSpec {
                name: "bs2b default",
                crossover_hz: 700.0,
                amount: Amount::FeedDb(4.5),
            },
        ],
        crossover_hz: Range {
            min: 300.0,
            max: 2000.0,
            step: 10.0,
        },
        amount_pct: Range {
            min: 15.0,
            max: 47.0,
            step: 1.0,
        },
        delay_max_ms: 1.0,
        delay_step_ms: 0.01,
        delay_settable: true,
        default_preset: 2,
    },
    ModelSpec {
        model: Model::Linkwitz,
        description: "A first-order low-pass cross path: the low frequencies of each channel are added to the other channel.",
        presets: &[PresetSpec {
            name: "Default",
            crossover_hz: 700.0,
            amount: Amount::Share(0.30),
        }],
        crossover_hz: Range {
            min: 300.0,
            max: 2000.0,
            step: 10.0,
        },
        amount_pct: Range {
            min: 5.0,
            max: 50.0,
            step: 1.0,
        },
        delay_max_ms: 1.0,
        delay_step_ms: 0.01,
        delay_settable: true,
        default_preset: 0,
    },
    ModelSpec {
        model: Model::Meier,
        description: "Crosses only what differs between the channels, so sound in the centre stays untouched.",
        presets: &[
            PresetSpec {
                name: "Default",
                crossover_hz: 650.0,
                amount: Amount::Share(0.25),
            },
            PresetSpec {
                name: "Rockbox",
                crossover_hz: 660.0,
                amount: Amount::Share(0.24),
            },
        ],
        crossover_hz: Range {
            min: 300.0,
            max: 2000.0,
            step: 10.0,
        },
        amount_pct: Range {
            min: 5.0,
            max: 50.0,
            step: 1.0,
        },
        delay_max_ms: 1.0,
        delay_step_ms: 0.01,
        delay_settable: true,
        default_preset: 0,
    },
    ModelSpec {
        model: Model::MidSide,
        description: "Blends the bass below the crossover toward mono; the centre and the highs stay as they are.",
        presets: &[PresetSpec {
            name: "Default",
            crossover_hz: 120.0,
            amount: Amount::Share(0.5),
        }],
        crossover_hz: Range {
            min: 40.0,
            max: 500.0,
            step: 10.0,
        },
        amount_pct: Range {
            min: 5.0,
            max: 50.0,
            step: 1.0,
        },
        delay_max_ms: 1.0,
        delay_step_ms: 0.01,
        delay_settable: false,
        default_preset: 0,
    },
];

/// bs2b's feed level in dB for the share `amount` of the other channel: how far the crossed
/// signal sits below the direct one at low frequencies, `20·log10((1 − A) / A)`.
pub fn feed_db(amount: f64) -> f64 {
    20.0 * ((1.0 - amount) / amount).log10()
}

/// The share of the other channel for bs2b's feed level `db`; the inverse of [`feed_db`].
pub fn amount_of_feed_db(db: f64) -> f64 {
    1.0 / (1.0 + 10f64.powf(db / 20.0))
}

/// What the crossfeed is set to. The model and the numbers are kept apart from whether it is
/// on (that is the state's `crossfeed_on`), so turning it off keeps them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CrossfeedSettings {
    /// The model.
    #[serde(default)]
    pub model: Model,
    /// The preset the numbers came from; `None` is Custom (the wire's word for it is the absence
    /// of a preset, see [`CUSTOM_PRESET`]).
    #[serde(default)]
    pub preset: Option<String>,
    /// For Custom values, the preset they were edited from, so a screen can say "edited from
    /// bs2b default". `None` for a preset's own values and once the model has been switched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_from: Option<String>,
    /// The share of the other channel at low frequencies, from 0 to 0.5. Stored exactly; a
    /// screen shows it rounded to 1 %.
    #[serde(default = "default_amount")]
    pub amount: f64,
    /// The crossover in Hz.
    #[serde(default = "default_crossover")]
    pub crossover_hz: f64,
    /// The total lag of the other side in ms; `None` is the model's own, which follows the
    /// crossover. A value at or below the model's own lag is stored as `None`
    /// ([`CrossfeedSettings::change`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<f64>,
}

fn default_amount() -> f64 {
    Model::Bauer.spec().default_preset().share()
}

fn default_crossover() -> f64 {
    Model::Bauer.spec().default_preset().crossover_hz
}

impl Default for CrossfeedSettings {
    /// Bauer with bs2b's default preset.
    fn default() -> Self {
        CrossfeedSettings::for_model(Model::Bauer)
    }
}

impl CrossfeedSettings {
    /// The settings of `model`'s preset `name`; `None` when it has no such preset.
    pub fn from_preset(model: Model, name: &str) -> Option<CrossfeedSettings> {
        let preset = model.spec().presets.iter().find(|p| p.name == name)?;
        Some(CrossfeedSettings {
            model,
            preset: Some(preset.name.to_string()),
            edited_from: None,
            amount: preset.share(),
            crossover_hz: preset.crossover_hz,
            delay_ms: None,
        })
    }

    /// The settings of `model`'s default preset: what switching to the model loads.
    pub fn for_model(model: Model) -> CrossfeedSettings {
        CrossfeedSettings::from_preset(model, model.spec().default_preset().name)
            .expect("every model has a default preset")
    }
}

/// The word a screen shows for values that came from no preset. On the wire a Custom state is
/// `preset: None`, so no preset can clash with it; `EqView.preset` uses the lower-case `custom`.
pub const CUSTOM_PRESET: &str = "Custom";

/// What a crossfeed change from outside asks for: the fields of `HostEqOp::Crossfeed` but `on`
/// and `done`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CrossfeedChange {
    /// The model's wire word.
    pub model: Option<String>,
    /// A preset of the model (the new one when `model` also changes).
    pub preset: Option<String>,
    /// The share of the other channel in percent.
    pub amount_pct: Option<f64>,
    /// The crossover in Hz.
    pub crossover_hz: Option<f64>,
    /// The total lag in ms.
    pub delay_ms: Option<f64>,
}

impl CrossfeedSettings {
    /// These values as Custom: the preset, if any, becomes `edited_from`.
    pub fn edited(mut self) -> CrossfeedSettings {
        if let Some(name) = self.preset.take() {
            self.edited_from = Some(name);
        }
        self
    }

    /// The settings after `change`, at a sink running at `rate_hz`; an `Err` in words and no
    /// change when it cannot be made. The rules, in order:
    ///
    /// 1. an unknown model word or preset name is refused, so is a number that is not finite,
    ///    and a delay for a model that plays none;
    /// 2. a `model` other than the current one loads that model's default preset; the same one
    ///    changes nothing (it does not wipe Custom values);
    /// 3. a `preset` loads that preset's exact numbers (and the model's own delay);
    /// 4. any amount, crossover or delay makes the values Custom, each rounded to its model's
    ///    step and held to its range; a delay at or below the model's own lag is "own", stored
    ///    as `None`.
    ///
    /// A change that asks for nothing returns the settings as they are.
    pub fn change(
        &self,
        change: &CrossfeedChange,
        rate_hz: u32,
    ) -> Result<CrossfeedSettings, String> {
        let mut next = held(self);
        if let Some(word) = &change.model {
            let model = Model::from_word(word).ok_or_else(|| {
                let words: Vec<&str> = Model::ALL.iter().map(|m| m.word()).collect();
                format!(
                    "no crossfeed model {word:?}; the models are {}",
                    words.join(", ")
                )
            })?;
            if model != next.model {
                next = CrossfeedSettings::for_model(model);
            }
        }
        if let Some(name) = &change.preset {
            next = CrossfeedSettings::from_preset(next.model, name).ok_or_else(|| {
                let names: Vec<&str> = next.model.spec().presets.iter().map(|p| p.name).collect();
                format!(
                    "{} has no preset {name:?}; its presets are {}",
                    next.model.name(),
                    names.join(", ")
                )
            })?;
        }
        for (what, value) in [
            ("amount", change.amount_pct),
            ("crossover", change.crossover_hz),
            ("delay", change.delay_ms),
        ] {
            if value.is_some_and(|v| !v.is_finite()) {
                return Err(format!("the {what} is not a number"));
            }
        }
        let spec = next.model.spec();
        if change.delay_ms.is_some() && !spec.delay_settable {
            return Err(format!("{} plays no delay", next.model.name()));
        }
        if change.amount_pct.is_some() || change.crossover_hz.is_some() || change.delay_ms.is_some()
        {
            next = next.edited();
            if let Some(pct) = change.amount_pct {
                next.amount = spec.amount_pct.snap(pct) / 100.0;
            }
            if let Some(hz) = change.crossover_hz {
                next.crossover_hz = spec.crossover_hz.snap(hz);
            }
            if let Some(ms) = change.delay_ms {
                let own = own_lag_ms(&next, rate_hz);
                next.delay_ms = match ms <= own {
                    true => None,
                    false => Some(
                        Range {
                            min: 0.0,
                            max: spec.delay_max_ms,
                            step: spec.delay_step_ms,
                        }
                        .snap(ms),
                    ),
                };
            } else if let Some(delay) = next.delay_ms {
                // The crossover moved the model's own lag: a delay that fell to it is "own".
                if delay <= own_lag_ms(&next, rate_hz) {
                    next.delay_ms = None;
                }
            }
        }
        Ok(held(&next))
    }
}

/// `settings` with every value held to its model's range: the crossover and the amount between
/// the ends, the delay between 0 and the model's most (none for a model that plays none, and
/// none for a number that is not one), an unknown preset name as custom.
pub fn held(settings: &CrossfeedSettings) -> CrossfeedSettings {
    let spec = settings.model.spec();
    let first = spec.default_preset();
    CrossfeedSettings {
        model: settings.model,
        preset: settings
            .preset
            .clone()
            .filter(|name| spec.presets.iter().any(|p| p.name == name)),
        edited_from: settings
            .edited_from
            .clone()
            .filter(|name| spec.presets.iter().any(|p| p.name == name))
            .filter(|_| {
                settings
                    .preset
                    .as_ref()
                    .is_none_or(|name| !spec.presets.iter().any(|p| p.name == name))
            }),
        amount: match settings.amount.is_nan() {
            true => first.share(),
            false => settings
                .amount
                .clamp(spec.amount_pct.min / 100.0, spec.amount_pct.max / 100.0),
        },
        crossover_hz: spec
            .crossover_hz
            .hold(settings.crossover_hz, first.crossover_hz),
        delay_ms: settings
            .delay_ms
            .filter(|ms| spec.delay_settable && !ms.is_nan())
            .map(|ms| ms.clamp(0.0, spec.delay_max_ms)),
    }
}

/// bs2b's derived numbers for a feed level and a lowpass corner.
#[derive(Debug, Clone, Copy)]
struct BauerMaths {
    /// The cross path's DC gain before the common gain.
    g_lo: f64,
    /// How much the direct path's highs are boosted over its DC gain.
    g_hi: f64,
    /// The direct path's high-boost corner in Hz.
    fc_hi: f64,
    /// The common gain that makes direct plus cross 1 at DC.
    gain: f64,
}

/// The numbers bs2b derives from a feed level `feed_db` (dB) and the low-pass corner `fc_lo`
/// (Hz): the cross path sits `feed_db` below the direct one at DC, the direct path's highs
/// are 6 dB-per-octave lifted so the two paths together stay level, and the whole is scaled
/// so direct plus cross is 1 at DC.
fn bauer_maths(feed_db: f64, fc_lo: f64) -> BauerMaths {
    let gb_lo = feed_db * (-5.0 / 6.0) - 3.0;
    let gb_hi = feed_db / 6.0 - 3.0;
    let g_lo = 10f64.powf(gb_lo / 20.0);
    let g_hi = 1.0 - 10f64.powf(gb_hi / 20.0);
    let fc_hi = fc_lo * 2f64.powf((gb_lo - 20.0 * g_hi.log10()) / 12.0);
    BauerMaths {
        g_lo,
        g_hi,
        fc_hi,
        gain: 1.0 / (1.0 - g_hi + g_lo),
    }
}

/// A complex number, for the responses.
#[derive(Debug, Clone, Copy)]
struct Complex {
    re: f64,
    im: f64,
}

impl Complex {
    fn abs(self) -> f64 {
        self.re.hypot(self.im)
    }

    fn arg(self) -> f64 {
        self.im.atan2(self.re)
    }

    fn mul(self, other: Complex) -> Complex {
        Complex {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }

    fn div(self, other: Complex) -> Complex {
        let d = other.re * other.re + other.im * other.im;
        Complex {
            re: (self.re * other.re + self.im * other.im) / d,
            im: (self.im * other.re - self.re * other.im) / d,
        }
    }

    fn add(self, other: Complex) -> Complex {
        Complex {
            re: self.re + other.re,
            im: self.im + other.im,
        }
    }
}

/// The response of the section `[b0, b1, b2, a1, a2]` at the angular frequency `w` (radians
/// per sample).
fn response(section: &[f64; 5], w: f64) -> Complex {
    let z1 = Complex {
        re: w.cos(),
        im: -w.sin(),
    };
    section_at(section, z1, z1.mul(z1))
}

/// The response of the section `[b0, b1, b2, a1, a2]` where `z^-1` is `z1` and `z^-2` is `z2`.
fn section_at(section: &[f64; 5], z1: Complex, z2: Complex) -> Complex {
    let scale = |z: Complex, k: f64| Complex {
        re: z.re * k,
        im: z.im * k,
    };
    let [b0, b1, b2, a1, a2] = *section;
    let num = Complex { re: b0, im: 0.0 }
        .add(scale(z1, b1))
        .add(scale(z2, b2));
    let den = Complex { re: 1.0, im: 0.0 }
        .add(scale(z1, a1))
        .add(scale(z2, a2));
    num.div(den)
}

/// A first-order low pass at `fc` Hz of DC gain 1: `(1 − p) / (1 − p z⁻¹)`.
fn lowpass1(fc: f64, rate: f64) -> [f64; 5] {
    let p = (-2.0 * PI * fc / rate).exp();
    [1.0 - p, 0.0, 0.0, -p, 0.0]
}

/// The cookbook second-order low pass at `fc` Hz with Q 1/√2 (Butterworth), DC gain 1.
fn lowpass2(fc: f64, rate: f64) -> [f64; 5] {
    let w0 = 2.0 * PI * fc / rate;
    let alpha = w0.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
    let a0 = 1.0 + alpha;
    let b = (1.0 - w0.cos()) / 2.0 / a0;
    [b, 2.0 * b, b, -2.0 * w0.cos() / a0, (1.0 - alpha) / a0]
}

fn scaled(section: [f64; 5], k: f64) -> [f64; 5] {
    [
        section[0] * k,
        section[1] * k,
        section[2] * k,
        section[3],
        section[4],
    ]
}

/// A section as the chain runs it: its coefficients rounded to f32 (PipeWire's control type).
fn as_run(section: [f64; 5]) -> [f64; 5] {
    section.map(|c| f64::from(c as f32))
}

/// The crossfeed stage as the chain holds it: the coefficients of its three paths, the gains of
/// the sum, the added delay and the mix.
///
/// For each channel `C` (the cross path is fed by the other one) the graph is
/// `dir(C)` and `del(sum(x(O), s(C)))`, added by a mixer: see the module doc.
#[derive(Debug, Clone, PartialEq)]
pub struct Stage {
    /// The direct path, `[b0, b1, b2, a1, a2]`.
    pub dir: [f64; 5],
    /// The cross path, fed by the other channel.
    pub x: [f64; 5],
    /// The side-subtract path, fed by the channel's own signal; zero for a model without one.
    pub s: [f64; 5],
    /// The gains `Gain 1` (the cross path) and `Gain 2` (the side path) of the sum; the side
    /// gain is 0 for a model without one.
    pub sum: [f64; 2],
    /// The value sent to the delay node, in seconds: the middle of the whole-sample step the
    /// delay plays at ([`Stage::delay_samples`]), so that PipeWire's truncation to whole samples
    /// cannot land a step short; 0 for no added delay.
    pub delay_s: f64,
    /// The whole samples of delay that play on top of the model's own lag: what the curves and
    /// the worst gain are worked out for.
    pub delay_samples: u32,
    /// The gain of the delayed path into the final mixer: 1 when the stage plays, 0 for the
    /// bypass.
    pub mix: f64,
}

impl Stage {
    /// The stage that changes nothing: the direct path the identity, the others silent, the
    /// delayed path off the mixer (a 0-gain input is skipped, so the output is the input).
    pub fn bypass() -> Stage {
        Stage {
            dir: [1.0, 0.0, 0.0, 0.0, 0.0],
            x: [0.0; 5],
            s: [0.0; 5],
            sum: [1.0, 0.0],
            delay_s: 0.0,
            delay_samples: 0,
            mix: 0.0,
        }
    }

    /// The stage `settings` makes at `rate_hz`, after holding the settings to their ranges
    /// ([`held`]), refused with words when its coefficients are out of the chain's range or a
    /// section would not be stable.
    pub fn of(settings: &CrossfeedSettings, rate_hz: u32) -> Result<Stage, String> {
        let stage = Stage::unchecked(settings, rate_hz);
        stage.check()?;
        Ok(stage)
    }

    /// [`Stage::of`] without the check of the coefficients.
    fn unchecked(settings: &CrossfeedSettings, rate_hz: u32) -> Stage {
        let settings = held(settings);
        let rate = f64::from(rate_hz.max(1));
        let a = settings.amount;
        let fc = settings.crossover_hz.min(rate * 0.499);
        let none = [0.0; 5];
        let (dir, x, s, side) = match settings.model {
            Model::Bauer => {
                let m = bauer_maths(feed_db(a), fc);
                let hi = {
                    // bs2b's high boost: O[n] = a0 I[n] + a1 I[n-1] + b1 O[n-1] with
                    // a0 = 1 − G_hi (1 − x), a1 = −x, b1 = x; the chain's section negates b1.
                    let p = (-2.0 * PI * m.fc_hi / rate).exp();
                    [1.0 - m.g_hi * (1.0 - p), -p, 0.0, -p, 0.0]
                };
                let lo = lowpass1(fc, rate);
                (scaled(hi, m.gain), scaled(lo, m.gain * m.g_lo), none, 0.0)
            }
            Model::Linkwitz => (
                [1.0 - a, 0.0, 0.0, 0.0, 0.0],
                scaled(lowpass1(fc, rate), a),
                none,
                0.0,
            ),
            Model::Meier => {
                let lp = lowpass1(fc, rate);
                (
                    [1.0, 0.0, 0.0, 0.0, 0.0],
                    scaled(lp, a),
                    scaled(lp, -a),
                    1.0,
                )
            }
            Model::MidSide => {
                let lp = lowpass2(fc, rate);
                (
                    [1.0, 0.0, 0.0, 0.0, 0.0],
                    scaled(lp, a),
                    scaled(lp, -a),
                    1.0,
                )
            }
        };
        let mut stage = Stage {
            dir: as_run(dir),
            x: as_run(x),
            s: as_run(s),
            sum: [1.0, side],
            delay_s: 0.0,
            delay_samples: 0,
            mix: 1.0,
        };
        let spec = settings.model.spec();
        if spec.delay_settable {
            let own = lag_ms(&stage, rate_hz);
            let total = settings.delay_ms.unwrap_or(0.0).max(own);
            // PipeWire plays whole samples, truncated: the delay that plays is the asked one
            // rounded down to a sample (21 µs at 48 kHz), and that is what is measured.
            let added = (total - own).max(0.0) / 1000.0;
            stage.delay_samples = (added * rate + 1e-9).floor() as u32;
            stage.delay_s = match stage.delay_samples {
                0 => 0.0,
                n => (f64::from(n) + 0.5) / rate,
            };
        }
        stage
    }

    /// Whether the chain can run this stage: every coefficient within the port range and every
    /// section strictly stable in the f32 coefficients the chain runs. The refusal says which.
    pub fn check(&self) -> Result<(), String> {
        for (name, section) in [("direct", &self.dir), ("cross", &self.x), ("side", &self.s)] {
            if section
                .iter()
                .any(|c| !c.is_finite() || c.abs() > COEFFICIENT_LIMIT)
            {
                return Err(format!(
                    "the crossfeed's {name} path has a coefficient past the chain's limit of {COEFFICIENT_LIMIT}"
                ));
            }
            let [_, _, _, a1, a2] = *section;
            let stable = a2.abs() < 1.0 - STABLE_MARGIN && a1.abs() < 1.0 + a2 - STABLE_MARGIN;
            if !stable {
                return Err(format!(
                    "the crossfeed's {name} path would not be stable at this sample rate: lower its crossover"
                ));
            }
        }
        Ok(())
    }
}

/// The lag of the stage's cross path behind its direct path at [`LAG_PROBE_HZ`], in ms: the
/// difference of their group delays, read from the phase of `H_x / H_dir` around the probe.
fn lag_ms(stage: &Stage, rate_hz: u32) -> f64 {
    let rate = f64::from(rate_hz.max(1));
    let w = |f: f64| 2.0 * PI * f / rate;
    let phase = |f: f64| {
        let ratio = response(&stage.x, w(f)).div(response(&stage.dir, w(f)));
        ratio.arg()
    };
    let (low, high) = (LAG_PROBE_HZ - 1.0, LAG_PROBE_HZ + 1.0);
    let tau = -(phase(high) - phase(low)) / (2.0 * PI * (high - low));
    match tau.is_finite() {
        true => tau * 1000.0,
        false => 0.0,
    }
}

/// The lag of the cross path behind the direct one at 20 Hz in ms, from the coefficients:
/// the delay the model has before any is added (about 0.26 ms for Bauer at 700 Hz, which is
/// where bs2b's own figure comes from). The lag of a model
/// that plays no delay is still this number; the view shows 0 for it.
pub fn own_lag_ms(settings: &CrossfeedSettings, rate_hz: u32) -> f64 {
    lag_ms(&Stage::unchecked(settings, rate_hz), rate_hz)
}

/// The most the stage can give for any input, in dB (never below 0): one output is
/// `H_dir·C + D·(H_s·C + H_x·O)`, so for the two inputs aligned to add up it is
/// `|H_dir + D·H_s| + |H_x|` at each frequency; this is the maximum of that over a grid of
/// 1 Hz steps from 1 Hz to Nyquist, for the stage `settings` makes at `rate_hz`. It depends on
/// the played delay `D` (in whole samples), so it changes with the asked delay for Meier.
///
/// The rule it serves, "nothing plays above unity", holds per frequency, as the EQ's preamp
/// rule does; it is not a bound on the samples: the time-domain bound `Σ|h_dir| + Σ|h_x|`
/// of Bauer's default is +2.45 dB at 48 kHz.
pub fn worst_gain_db(settings: &CrossfeedSettings, rate_hz: u32) -> f64 {
    stage_worst_db(&Stage::unchecked(settings, rate_hz), rate_hz)
}

/// [`worst_gain_db`] of a stage as it is (its coefficients and its delay in seconds), played at
/// `play_hz`: the same stage at another rate than the one it was worked out for, which is what
/// a config holding the stage plays when the sink's rate changes, and what a line sent before the
/// next look carries. The delay plays as whole samples of the played rate (`played_samples`).
pub fn stage_worst_db(stage: &Stage, play_hz: u32) -> f64 {
    stage_worst_over_db(stage, &[play_hz])
}

/// How close to a whole sample, in samples, a delay times the played rate must come for both
/// neighbouring samples to count: PipeWire works the product out in f32, this in f64.
const ON_A_SAMPLE: f64 = 1e-3;

/// The whole samples a delay of `delay_s` seconds (sent as an f32) plays at `rate_hz`, as the
/// lower and the higher candidate. PipeWire's delay truncates `delay * rate` worked out in f32;
/// a delay worked out at half or a quarter of the rate lands on a whole sample there, where the
/// f32 product may fall on either side of it, so both count. Away from a whole sample the two
/// are the same truncated count.
fn played_samples(delay_s: f64, rate_hz: u32) -> [f64; 2] {
    if delay_s <= 0.0 {
        return [0.0; 2];
    }
    let at = f64::from(delay_s as f32) * f64::from(rate_hz);
    [
        (at - ON_A_SAMPLE).floor().max(0.0),
        (at + ON_A_SAMPLE).floor().max(0.0),
    ]
}

/// How many steps of a rotating phasor go before it is worked out again from its angle, so the
/// rounding of the products stays far under what the worst gain is read to.
const RESYNC_STEPS: u32 = 1024;

/// `e^(-i angle)`.
fn phasor(angle: f64) -> Complex {
    let (sin, cos) = angle.sin_cos();
    Complex { re: cos, im: -sin }
}

/// The most of [`stage_worst_db`] over `rates`, in dB. The three paths depend on the angular
/// frequency alone, so they are worked out once on the 1 Hz grid of the highest rate of each
/// family whose grids nest (44.1, 88.2, 176.4 and 352.8 kHz all sit on the grid of 352.8 kHz), and
/// each rate of the family adds its own delay at its own points.
pub fn stage_worst_over_db(stage: &Stage, rates: &[u32]) -> f64 {
    let mut rates: Vec<u32> = rates.iter().map(|rate| (*rate).max(1)).collect();
    rates.sort_unstable_by_key(|rate| std::cmp::Reverse(*rate));
    rates.dedup();
    // Each base takes the rates its grid holds: (rate, its step in base points, its delays).
    let mut families: Vec<(u32, Vec<Member>)> = Vec::new();
    for rate in rates {
        let delays = played_samples(stage.delay_s, rate);
        match families
            .iter_mut()
            .find(|(base, _)| base.is_multiple_of(rate))
        {
            Some((base, members)) => members.push((rate, *base / rate, delays)),
            None => families.push((rate, vec![(rate, 1, delays)])),
        }
    }
    let worst = families
        .iter()
        .map(|(base, members)| family_worst(stage, *base, members))
        .fold(1.0, f64::max);
    20.0 * worst.log10()
}

/// A rate of a family: the rate, its step in the base's points, its delays in whole samples.
type Member = (u32, u32, [f64; 2]);

/// The worst linear gain over the points of every member of one family (see
/// [`stage_worst_over_db`]): `(rate, step, delays)`, the member's grid being every `step`-th
/// point of the base's.
fn family_worst(stage: &Stage, base: u32, members: &[Member]) -> f64 {
    let rate = f64::from(base);
    let dw = 2.0 * PI / rate;
    let top = (base / 2).max(1);
    let one_delay = |delays: &[f64; 2]| delays[0] == delays[1];
    // The delay phasors of each member and candidate, and their rotation per member point.
    let turns: Vec<[Complex; 2]> = members
        .iter()
        .map(|(_, step, delays)| delays.map(|n| phasor(dw * f64::from(*step) * n)))
        .collect();
    let mut delays: Vec<[Complex; 2]> = turns.clone();
    let mut visits = vec![0u32; members.len()];
    let z_turn = phasor(dw);
    let mut z1 = z_turn;
    // A section's numerator and denominator at `z1`, `z2`: `b0 + b1 z1 + b2 z2`, `1 + a1 z1 + a2 z2`.
    let poly = |c0: f64, c1: f64, c2: f64, z1: Complex, z2: Complex| Complex {
        re: c0 + c1 * z1.re + c2 * z2.re,
        im: c1 * z1.im + c2 * z2.im,
    };
    let norm = |z: Complex| z.re * z.re + z.im * z.im;
    let [db0, db1, db2, da1, da2] = stage.dir;
    let [sb0, sb1, sb2, sa1, sa2] = stage.s;
    let [xb0, xb1, xb2, xa1, xa2] = stage.x;
    let mut worst = 1.0f64;
    for f in 1..=top {
        if f.is_multiple_of(RESYNC_STEPS) {
            z1 = phasor(dw * f64::from(f));
        }
        let z2 = z1.mul(z1);
        // `dir + D·s` is `(nd·ds + D·ns·dd) / (dd·ds)`: two products here, no division, and one
        // magnitude for each delay below.
        let (nd, dd) = (poly(db0, db1, db2, z1, z2), poly(1.0, da1, da2, z1, z2));
        let (ns, ds) = (poly(sb0, sb1, sb2, z1, z2), poly(1.0, sa1, sa2, z1, z2));
        let (nx, dx) = (poly(xb0, xb1, xb2, z1, z2), poly(1.0, xa1, xa2, z1, z2));
        let (own, delayed) = (nd.mul(ds), ns.mul(dd));
        let scale = 1.0 / (norm(dd) * norm(ds)).sqrt();
        let cross = (norm(nx) / norm(dx)).sqrt();
        for (at, (member, step, samples)) in members.iter().enumerate() {
            if !f.is_multiple_of(*step) || f / step > (member / 2).max(1) {
                continue;
            }
            visits[at] += 1;
            if visits[at].is_multiple_of(RESYNC_STEPS) {
                let w = dw * f64::from(f);
                delays[at] = samples.map(|n| phasor(w * n));
            }
            let mut direct = norm(own.add(delayed.mul(delays[at][0])));
            if !one_delay(samples) {
                direct = direct.max(norm(own.add(delayed.mul(delays[at][1]))));
            }
            worst = worst.max(direct.sqrt() * scale + cross);
            for (delay, turn) in delays[at].iter_mut().zip(&turns[at]) {
                *delay = delay.mul(*turn);
            }
        }
        z1 = z1.mul(z_turn);
    }
    worst
}

/// What the stage does to a tone at one frequency, as complex gains `[re, im]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageResponse {
    /// An output channel from its own channel's tone: the direct path plus the side path, the
    /// side path delayed.
    pub same: [f64; 2],
    /// An output channel from the other channel's tone: the cross path, delayed.
    pub other: [f64; 2],
}

/// The response of `stage` at `hz`, played at `rate_hz` with its delay in whole samples (the
/// numbers of [`CurveView`] with the phase kept): a left-only tone `T` comes out as `same · T`
/// on the left and `other · T` on the right; the same tone on both channels as `same + other`.
pub fn stage_response(stage: &Stage, rate_hz: u32, hz: f64) -> StageResponse {
    let rate = f64::from(rate_hz.max(1));
    let w = 2.0 * PI * hz.min(rate * 0.499) / rate;
    let d = Complex {
        re: (w * f64::from(stage.delay_samples)).cos(),
        im: -(w * f64::from(stage.delay_samples)).sin(),
    };
    let same = response(&stage.dir, w).add(response(&stage.s, w).mul(d));
    let other = response(&stage.x, w).mul(d);
    StageResponse {
        same: [same.re, same.im],
        other: [other.re, other.im],
    }
}

/// One preset as a front end lists it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PresetView {
    /// Its name.
    pub name: String,
    /// The crossover in Hz.
    pub crossover_hz: f64,
    /// The amount in percent, exact.
    pub amount_pct: f64,
    /// bs2b's feed level in dB for a Bauer preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed_db: Option<f64>,
}

/// One model as a front end lists it: its words, presets and ranges.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelView {
    /// The wire word.
    pub word: String,
    /// The name to show.
    pub name: String,
    /// One line about the model, for a tooltip or the line under the picker.
    pub description: String,
    /// The presets, from the mildest to the strongest (Custom is no preset).
    pub presets: Vec<PresetView>,
    /// The name of the preset the model loads when it is chosen.
    pub default_preset: String,
    /// The crossover range in Hz.
    pub crossover_hz: Range,
    /// The amount range in percent.
    pub amount_pct: Range,
    /// The most the total delay can be in ms.
    pub delay_max_ms: f64,
    /// The delay control's step in ms.
    pub delay_step_ms: f64,
    /// Whether the delay can be set.
    pub delay_settable: bool,
}

impl ModelView {
    /// `model`'s row of the table as a view.
    pub fn of(model: Model) -> ModelView {
        let spec = model.spec();
        ModelView {
            word: model.word().to_string(),
            name: model.name().to_string(),
            description: spec.description.to_string(),
            presets: spec
                .presets
                .iter()
                .map(|p| PresetView {
                    name: p.name.to_string(),
                    crossover_hz: p.crossover_hz,
                    amount_pct: p.share() * 100.0,
                    feed_db: p.feed_db(),
                })
                .collect(),
            crossover_hz: spec.crossover_hz,
            amount_pct: spec.amount_pct,
            delay_max_ms: spec.delay_max_ms,
            delay_step_ms: spec.delay_step_ms,
            delay_settable: spec.delay_settable,
            default_preset: spec.default_preset().name.to_string(),
        }
    }
}

/// Why the crossfeed turned itself off. The view keeps the reason (and its note) until the next
/// crossfeed change or turn-on clears it; a front end that shows it once (a toast) shows it when
/// the pair `off_reason` and `note` changes, not on every view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffReason {
    /// The output the computer plays into changed.
    OutputChanged,
    /// The owner (daemon or TUI) started and found it on.
    Restart,
    /// The filter chain restarted.
    ChainRestarted,
    /// The owner stopped.
    Stopped,
}

impl OffReason {
    /// The word the view carries.
    pub fn word(self) -> &'static str {
        match self {
            OffReason::OutputChanged => "output-changed",
            OffReason::Restart => "restart",
            OffReason::ChainRestarted => "chain-restarted",
            OffReason::Stopped => "stopped",
        }
    }
}

/// The two curves a screen draws, in dB on a fixed log grid: what the stage does to the sound
/// of the same ear (its own channel: direct path plus any side path) and to the other ear's.
/// Both follow every setting, at the running rate with the played delay, so no front end works
/// the maths out itself.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CurveView {
    /// The frequencies in Hz: [`CURVE_POINTS`] log-spaced from 20 Hz to 20 kHz.
    pub hz: Vec<f64>,
    /// The same-ear level in dB (floored at −120), to 0.01 dB.
    pub same_db: Vec<f64>,
    /// The other-ear level in dB (floored at −120), to 0.01 dB.
    pub other_db: Vec<f64>,
}

/// How many points the curve has: 20 per decade from 20 Hz to 20 kHz, ends included.
pub const CURVE_POINTS: usize = 61;

impl CurveView {
    /// The curves of `stage` at `rate_hz`.
    fn of(stage: &Stage, rate_hz: u32) -> CurveView {
        let rate = f64::from(rate_hz.max(1));
        let db = |gain: f64| ((20.0 * gain.max(1e-6).log10()) * 100.0).round() / 100.0;
        let mut curve = CurveView::default();
        for i in 0..CURVE_POINTS {
            let hz = 20.0 * 10f64.powf(i as f64 / 20.0);
            let w = 2.0 * PI * hz.min(rate * 0.499) / rate;
            let d = Complex {
                re: (w * f64::from(stage.delay_samples)).cos(),
                im: -(w * f64::from(stage.delay_samples)).sin(),
            };
            let same = response(&stage.dir, w).add(response(&stage.s, w).mul(d));
            curve.hz.push((hz * 100.0).round() / 100.0);
            curve.same_db.push(db(same.abs()));
            curve.other_db.push(db(response(&stage.x, w).abs()));
        }
        curve
    }
}

/// The crossfeed as a screen shows it. Every field has a default, so a view from an older
/// daemon (without the newer fields) still parses.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CrossfeedView {
    /// Whether it plays.
    pub on: bool,
    /// The model's wire word.
    pub model: String,
    /// The preset the numbers came from; `None` is Custom ([`CUSTOM_PRESET`]; `EqView.preset`
    /// itself says `custom`).
    pub preset: Option<String>,
    /// For Custom, the preset the values were edited from ("Edited from bs2b default").
    pub edited_from: Option<String>,
    /// The amount in percent, exact (a screen rounds it to 1 %).
    pub amount_pct: f64,
    /// The level of the crossed signal against the input, `20·log10(amount)`, shown beside the
    /// amount (more percent, a higher number).
    pub amount_db: f64,
    /// bs2b's feed level for the amount: how far the crossed signal sits below the direct one.
    pub feed_db: f64,
    /// The crossover in Hz.
    pub crossover_hz: f64,
    /// The total lag of the other side in ms: what was asked, never below the floor. It plays in
    /// whole samples, truncated (21 µs at 48 kHz): within one sample under this number.
    pub delay_ms: f64,
    /// The least total lag: the model's own (0 for a model that plays no delay).
    pub delay_floor_ms: f64,
    /// Whether the delay can be set.
    pub delay_settable: bool,
    /// Whether the delay is the model's own (nothing asked above it, or none to ask for): a
    /// screen says "(own)". To go back to it, send a `delay_ms` at or below `delay_floor_ms`.
    /// For Mid/Side bass `delay_ms` and the floor are 0 and mean "none shown": its low-pass cross
    /// path does lag, about 1.9 ms at 120 Hz, which no control changes.
    pub delay_own: bool,
    /// The name of the output it plays into, when known.
    pub output: Option<String>,
    /// A sentence about why it turned itself off, kept with `off_reason` until the next
    /// crossfeed change or turn-on.
    pub note: Option<String>,
    /// Why it turned itself off, as an [`OffReason`] word: `output-changed`, `restart`,
    /// `chain-restarted` or `stopped`.
    pub off_reason: Option<String>,
    /// Why crossfeed cannot be changed now (a chain from an older svolder that is rebuilt on
    /// the first turn-on is not this; a curve the chain cannot hold is): front ends dim the
    /// controls and show it instead of failing a press. The controller sets it.
    pub unavailable: Option<String>,
    /// The two curves for the picture.
    pub curve: CurveView,
    /// Every model with its presets and ranges, so a front end hard-codes none.
    pub models: Vec<ModelView>,
}

impl CrossfeedView {
    /// The view of `settings` (held to their ranges) for a sink running at `rate_hz`.
    pub fn of(
        settings: &CrossfeedSettings,
        on: bool,
        rate_hz: u32,
        output: Option<String>,
        note: Option<String>,
    ) -> CrossfeedView {
        let settings = held(settings);
        let spec = settings.model.spec();
        let floor = match spec.delay_settable {
            true => own_lag_ms(&settings, rate_hz),
            false => 0.0,
        };
        CrossfeedView {
            on,
            model: settings.model.word().to_string(),
            preset: settings.preset.clone(),
            edited_from: settings.edited_from.clone(),
            amount_pct: settings.amount * 100.0,
            amount_db: 20.0 * settings.amount.log10(),
            feed_db: feed_db(settings.amount),
            crossover_hz: settings.crossover_hz,
            delay_ms: settings.delay_ms.unwrap_or(0.0).max(floor),
            delay_floor_ms: floor,
            delay_settable: spec.delay_settable,
            delay_own: settings.delay_ms.is_none_or(|ms| ms <= floor),
            output,
            note,
            off_reason: None,
            unavailable: None,
            curve: CurveView::of(&Stage::unchecked(&settings, rate_hz), rate_hz),
            models: Model::ALL.into_iter().map(ModelView::of).collect(),
        }
    }
}

#[cfg(test)]
mod tests;
