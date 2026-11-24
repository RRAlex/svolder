// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The panel model: everything the GNOME top-bar indicator draws, in one serialisable value.
//!
//! [`PanelView`] is what `svolder panel --json` prints, one object per line. The indicator draws
//! it and never re-judges anything: the Path strip's tones and its words (each segment's status
//! and the line at rest) come from the Path row's judge, the signal's bars from
//! [`Signal::bars`](crate::Signal::bars), and every choice list carries the CLI arguments that
//! select a choice. Every number a person reads is also given as text with the proper minus sign
//! (U+2212); the JSON numbers beside them stay numbers.
//!
//! The wire shape is versioned by [`PANEL_VERSION`]: a reader that meets another `v` stops
//! drawing instead of guessing.

use serde::{Deserialize, Serialize};

/// The version of the shape, carried in the `v` field of every object.
pub const PANEL_VERSION: u32 = 1;

/// One state of the indicator: the device, the computer's EQ, the Path, the scenes and the CLI
/// arguments that act on them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PanelView {
    /// The shape's version, [`PANEL_VERSION`].
    pub v: u32,
    /// The device the indicator shows; `None` when none is chosen or known.
    pub device: Option<PanelDevice>,
    /// The computer's EQ; `None` when the computer's controls are unavailable.
    pub computer_eq: Option<PanelComputerEq>,
    /// The Path from the player to the DAC; `None` when there is nothing to judge (no DAC, host
    /// audio off).
    pub path: Option<PanelPath>,
    /// The scenes and the one that is on.
    pub scenes: PanelScenes,
    /// The CLI arguments of the indicator's own actions.
    pub actions: PanelActions,
}

/// The device the indicator shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PanelDevice {
    /// The device's name as it is configured.
    pub name: String,
    /// Whether the DAC is there: its link is up. When false the bar shows the away state and
    /// the readings below are the last ones kept.
    pub present: bool,
    /// The sample rate in Hz; `None` when unknown.
    pub rate_hz: Option<u32>,
    /// The rate as the header says it, `44.1` (the unit is drawn apart as `kHz`); `None` when
    /// unknown.
    pub rate_text: Option<String>,
    /// The device's volume; `None` when its profile has none.
    pub volume: Option<PanelVolume>,
    /// The input the device listens to; `None` when its profile has none.
    pub input: Option<PanelChoices>,
    /// The device's presets, as the TUI's preset list for this device offers them; `None` when
    /// the device has none.
    pub preset: Option<PanelChoices>,
    /// The radio signal; `None` when it was never heard.
    pub signal: Option<PanelSignal>,
}

/// A device's volume.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PanelVolume {
    /// The volume in dB; `None` while it has not been read, or when the control is not in dB.
    pub db: Option<f64>,
    /// The lowest volume in dB; `None` when the profile gives none.
    pub min_db: Option<f64>,
    /// The highest volume in dB; `None` when the profile gives none.
    pub max_db: Option<f64>,
    /// Whether the device is muted; `None` when the profile has no mute.
    pub muted: Option<bool>,
    /// The volume as the popup says it, `−24.1` (the unit `dB` is drawn apart); `None` with
    /// `db`.
    pub text: Option<String>,
    /// The volume as the bar says it, rounded to whole dB, `−24`; `None` with `db`.
    pub bar_text: Option<String>,
}

/// A list of choices and the one that is on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PanelChoices {
    /// The name of the choice that is on; `None` when it is not known.
    pub on: Option<String>,
    /// The choices, in the order the TUI lists them.
    pub choices: Vec<PanelChoice>,
}

/// One choice and the CLI arguments that select it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PanelChoice {
    /// The choice's name as it is drawn.
    pub name: String,
    /// The arguments after the program name and the global flags, such as `["set", "input",
    /// "USB 1"]`.
    pub run: Vec<String>,
}

/// A device's radio signal as the indicator draws it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PanelSignal {
    /// The strength in dBm.
    pub dbm: i16,
    /// The strength as drawn, `−58`.
    pub dbm_text: String,
    /// When the daemon heard it, in unix milliseconds; the indicator turns it into an age.
    pub heard_at_ms: u64,
    /// How many of the four bars are filled, 1 to 4.
    pub bars: u8,
    /// Whether the signal is weak (one bar): drawn yellow.
    pub weak: bool,
    /// Whether the reading is old: drawn dim.
    pub old: bool,
}

/// The computer's EQ.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PanelComputerEq {
    /// Whether the EQ is on.
    pub on: bool,
    /// The preset on the computer; `None` when none, and `custom` once a hand changed it.
    pub preset: Option<String>,
    /// The presets that can be put on the computer.
    pub choices: Vec<PanelChoice>,
    /// What the chain plays at, in the daemon's words (`plays −14.8 dB in all`); `None` with no
    /// chain, or from a daemon that has no levels.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub level: Option<String>,
    /// The row's switch, "the computer changes the sound"; `None` from a daemon that has no
    /// bypass, and then the indicator drives `host eq on|off` with `on`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub switch: Option<PanelSwitch>,
}

/// The Computer EQ row's switch: whether the computer changes the sound now (any stage plays and
/// the bypass is off) and the CLI arguments that toggle it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PanelSwitch {
    /// Whether any stage (EQ, loudness, crossfeed, speakers) plays.
    pub on: bool,
    /// The arguments that flip it: `host bypass on` while a stage plays, `host bypass off` while
    /// the bypass holds a set, else `host eq on`.
    pub args: Vec<String>,
    /// The switch's accessible name: what it means, for a screen reader.
    pub name: String,
}

/// The Path from the player to the DAC, judged once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PanelPath {
    /// The verdict's stable key: `untouched`, `changed`, `not_this_dac`, `other_input` or
    /// `unknown`.
    pub end_to_end: String,
    /// How the Path row's label is drawn: `good` only for a path untouched end to end whose
    /// config check does not fail, `warn` when a hop or the DAC's stage changes the sound or the
    /// config check fails, `plain` for what could not be read and for another sink or input. The
    /// indicator's LED follows this, not `end_to_end`.
    pub tone: PanelLabelTone,
    /// The Path row's label in words for a screen reader, such as `Path: the DAC changes the
    /// sound`.
    pub label: String,
    /// Exactly three segments: the player, the computer and the DAC, in that order.
    pub segments: Vec<PanelSegment>,
    /// The words for what is off, each hop's changes prefixed with the hop's name
    /// (`DAC: volume −24.1 dB · tone control`); empty when the path is untouched end to end.
    /// A failing config check adds `config N`. For a sound that goes to another sink or input
    /// there are no hops: the row's short words (`→ Monitor HDMI`) stand here.
    pub off: String,
    /// The line the strip shows at rest: `untouched end to end` in `good` when the label is good,
    /// otherwise the `off` words in `warn`; empty (and `plain`) when there is nothing to say.
    pub rest: PanelRest,
}

/// The line beside the Path strip while no segment is hovered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PanelRest {
    /// The words, minus signs written as U+2212; empty when there is nothing to say.
    pub text: String,
    /// `good` for an untouched path, `warn` for words about what is off, `plain` for no words.
    pub tone: PanelLabelTone,
}

/// How the Path row's label is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelLabelTone {
    /// Green.
    Good,
    /// Yellow.
    Warn,
    /// Neither: not everything could be read, or the sound goes elsewhere.
    Plain,
}

/// One segment of the Path strip.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PanelSegment {
    /// Which hop the segment is.
    pub hop: PanelHop,
    /// How the segment is drawn.
    pub tone: PanelTone,
    /// The segment's status line for a hover or a screen reader, `<Name>: <status>`: the hop's
    /// changes joined by ` · ` (`Computer: resampled · sink filter`), `untouched` for a
    /// green hop, `not read` for a grey one. A path with no hops gives every segment the `off`
    /// words.
    pub words: String,
}

/// The hop of a [`PanelSegment`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelHop {
    /// The player.
    Player,
    /// The computer.
    Computer,
    /// The DAC.
    Dac,
}

/// How a segment is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PanelTone {
    /// Green: the hop leaves the sound alone.
    Good,
    /// Yellow: the hop changes the sound.
    Warn,
    /// Grey: the hop could not be read, or the sound does not go through it.
    Unknown,
}

/// The scenes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PanelScenes {
    /// The scene that is on; `None` when none is, or it is not known yet.
    pub on: Option<String>,
    /// The scenes in the file's order.
    pub choices: Vec<PanelChoice>,
}

/// The CLI arguments of the actions the bar and the popup run. Each is the arguments after the
/// program name and the global flags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct PanelActions {
    /// One volume step up; the indicator appends `--step N` for N notches.
    pub volume_up: Vec<String>,
    /// One volume step down; the indicator appends `--step N` for N notches.
    pub volume_down: Vec<String>,
    /// Mute on.
    pub mute_on: Vec<String>,
    /// Mute off.
    pub mute_off: Vec<String>,
    /// Set the volume: the literal `{db}` in the last argument stands for the number of dB.
    pub volume_set: Vec<String>,
    /// The computer's EQ on.
    pub eq_on: Vec<String>,
    /// The computer's EQ off.
    pub eq_off: Vec<String>,
}

/// The minus sign every number string of the panel is written with.
pub const MINUS: char = '\u{2212}';

/// `value` with `decimals` decimals and the proper minus sign.
pub fn number_text(value: f64, decimals: usize) -> String {
    let text = format!("{value:.decimals$}");
    match text.strip_prefix('-') {
        // A negative that rounds to zero reads `0`, never `−0`.
        Some(rest) if rest.bytes().any(|b| (b'1'..=b'9').contains(&b)) => format!("{MINUS}{rest}"),
        Some(rest) => rest.to_string(),
        None => text,
    }
}

mod build;

pub use build::{PanelInput, PanelLink};

#[cfg(test)]
mod tests;
