// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Serde mirror of a profile TOML file. Raw and unvalidated; `registry` turns it into `model`.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProfileFile {
    pub schema: u32,
    pub name: String,
    pub display: Option<String>,
    pub product_id: Option<u16>,
    pub min_firmware: Option<u16>,
    pub extends: Option<String>,
    /// "dac", "amp" or "preamp"; unset inherits the parent's, or defaults to `dac` at a root.
    pub family: Option<String>,
    /// The firmware subsystems asked for their version after each connect; unset inherits the
    /// parent's, and a list replaces it.
    pub firmware_subsystems: Option<Vec<u8>>,
    /// Whether svolder offers the identify cue; unset inherits the parent's, and a root offers it.
    pub identify: Option<bool>,
    #[serde(default, rename = "control")]
    pub controls: Vec<ControlDef>,
    pub eq: Option<EqDef>,
    pub builtin_settings: Option<BuiltinSettingsDef>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlDef {
    pub id: Vec<u8>,
    pub key: String,
    pub label: String,
    pub kind: KindDef,
    /// Wire type override: "u8", "u16", "u32", "i8", "i16", "i32", "bool", "data".
    pub value_type: Option<String>,
    /// TOML keys are strings; they are parsed to u8 during resolution.
    pub values: Option<BTreeMap<String, String>>,
    pub range: Option<[i64; 2]>,
    pub center: Option<i64>,
    pub unit: Option<String>,
    pub convert: Option<ConvertDef>,
    pub channel: Option<String>,
    /// Meter only: the raw value a linear meter (one with `range`) reads at idle.
    pub floor: Option<i64>,
    /// Meter only: the raw value where it crosses into class-A operation. Belongs to a linear
    /// meter (one with `range`); a power amp's meter uses `scale`.
    pub tick: Option<i64>,
    /// Meter only: the amp's logarithmic scale, in place of `range`, `floor` and `tick`.
    pub scale: Option<ScaleDef>,
    pub advice: Option<String>,
    /// Boolean readout only: the key of a boolean control; the readout reads true only while
    /// that control also reads true.
    pub only_while: Option<String>,
    /// Boolean readout only: whenever the device reports it true, the session writes it false.
    #[serde(default)]
    pub auto_off: bool,
    /// Select only: the values, ordered low to high, set how loud the output is (a gain or a
    /// mode with gain); a change to a higher one is a rise.
    #[serde(default)]
    pub output_gain: bool,
    /// Toggle only: true silences the output, so setting it false lets sound through.
    #[serde(default)]
    pub mutes_output: bool,
    /// Toggle only: true powers the device on and lets sound through, so setting it true is
    /// as much a rise as an unmute.
    #[serde(default)]
    pub powers_output: bool,
    /// Select or toggle only: any change to it raises or may raise the output in a way no
    /// order or polarity describes, so a change asks first.
    #[serde(default)]
    pub asks_on_change: bool,
    /// Action only: how many slots it has, numbered from 1. Wider than the byte it ends up in,
    /// so a file that asks for 256 is told so.
    pub slots: Option<u16>,
    /// Action only: the action overwrites what the device holds, so a front end asks first.
    #[serde(default)]
    pub overwrites: bool,
    /// Action only: the keys of the controls whose readings the device sets after the action.
    pub changes: Option<Vec<String>>,
    /// Further function ids a status is read as this control from, each of one to three bytes;
    /// a status whose id equals an entry or lies under it reads as this control.
    pub reads: Option<Vec<Vec<u8>>>,
}

/// A meter's logarithmic scale: `zero` or `zero_from` (exactly one), `full`, `class_a`.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScaleDef {
    /// The raw reading the live part of the bar is empty at.
    pub zero: Option<i64>,
    /// The key of a `u16` readout whose floor the zero follows.
    pub zero_from: Option<String>,
    /// The raw reading the live part is full at.
    pub full: i64,
    /// How much of the bar the class-A block covers, from its left end.
    #[serde(default)]
    pub class_a: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KindDef {
    Select,
    Toggle,
    Level,
    Readout,
    Meter,
    Fault,
    Action,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged, deny_unknown_fields)]
pub enum ConvertDef {
    Table { table: String },
    Linear { linear: [f64; 2] },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EqDef {
    pub id: u8,
    #[serde(default = "default_mode_sub")]
    pub mode_sub: u8,
    #[serde(default = "default_loudness_sub")]
    pub loudness_sub: u8,
    /// Band centre frequencies in Hz, in band order.
    pub bands: Vec<f64>,
    /// One `[below_centre, above_centre]` frequency scale pair per band.
    pub fscale: Vec<[f64; 2]>,
    /// Frequency range factor shared by all bands.
    pub range: f64,
    pub gain: EqGainDef,
    pub q: EqQDef,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EqGainDef {
    pub range: [f64; 2],
    /// Raw value that means 0 dB.
    pub zero: u8,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EqQDef {
    pub range: [f64; 2],
    /// Q at raw value 64.
    pub center: f64,
}

fn default_mode_sub() -> u8 {
    50
}

fn default_loudness_sub() -> u8 {
    52
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinSettingsDef {
    /// The keys of the controls a setting sets.
    pub keys: Vec<String>,
    /// The id each is written to, in the order of `keys`.
    pub ids: Vec<Vec<u8>>,
    pub settings: Vec<BuiltinSettingDef>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuiltinSettingDef {
    pub name: String,
    /// One raw step per key, in the order of `keys`.
    pub raw: Vec<i64>,
}
