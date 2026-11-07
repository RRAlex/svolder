// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Named tone curves: the built-in ones, a device's current curve read back as one, and the
//! writes that recall one onto a device.
//!
//! A preset holds physical units — Hz, dB and Q — rather than raw steps, because the same
//! curve converts differently on different profiles. Recall converts each value through the
//! device's own [`EqSpec`] and writes only the steps that differ, in an order that keeps a
//! boost from sounding anywhere it is not meant to on the way.

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};

use svolder_proto::FunctionId;

use crate::convert::{EqMode, EqParam, frequency_text, gain_text};
use crate::error::ProfileError;
use crate::model::{EqSpec, Profile};
use crate::tone::apo::{Correction, FilterKind, SourceFilter};
use crate::tone::fit::{self, Fit};
use crate::tone::response::{self, BandParams};

/// The strongest loudness setting a tone control takes.
pub const LOUDNESS_MAX: u8 = 127;

/// The most characters a preset's name has: enough for a headphone and a word about it, few
/// enough that every place a name is shown has room for it.
pub const NAME_MAX: usize = 40;

/// What a front end says when a band edit made while the tone control is in Flat mode, the
/// device's bypass, also writes the mode to EQ so the edit is heard: the TUI's key line and
/// `svolder eq flatten` say the same words.
pub const SWITCHED_TO_EQ: &str = "switched to EQ";

/// The built-in presets' names, in the order they are offered. A user preset may take none
/// of them, in any case.
pub const BUILTIN_NAMES: [&str; 4] = ["Flat", "Warm", "Bass", "Presence"];

/// A named tone curve: for every tone control with `bands` bands, or, when `source` is set, an
/// imported correction that fits every tone control that has bands.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Preset {
    /// The name a user picks it by, matched without regard to case.
    pub name: String,
    /// How many bands the tone controls it fits have; 0 for an imported preset, which fits
    /// them all.
    pub bands: u8,
    /// The mode it puts the tone control in; `flat`, `eq` or `loudness` in a file.
    #[cfg_attr(feature = "serde", serde(with = "mode_word"))]
    pub mode: EqMode,
    /// The loudness strength, 0 to 127.
    pub loudness: u8,
    /// One entry per band, in band order; empty for an imported preset.
    pub band: Vec<PresetBand>,
    /// The correction an imported preset is fitted from, `None` for a preset of bands.
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub source: Option<Imported>,
}

/// A headphone correction kept as it was imported, to be fitted to each tone control it is
/// recalled on.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Imported {
    /// Where it came from, as free text for a person to read.
    pub origin: Option<String>,
    /// The preamp and filters.
    pub correction: Correction,
}

impl Preset {
    /// Whether this preset is an imported correction rather than a curve of bands.
    pub fn is_imported(&self) -> bool {
        self.source.is_some()
    }

    /// An imported preset called `name`: no bands of its own, since it is fitted to each tone
    /// control it is recalled on. The name is taken as it is; the caller has checked it.
    pub fn imported(name: &str, source: Imported) -> Preset {
        Preset {
            name: name.to_string(),
            bands: 0,
            mode: EqMode::Eq,
            loudness: 0,
            band: Vec::new(),
            source: Some(source),
        }
    }

    /// The preset to recall or compare on a device whose tone control is `eq`: for an imported
    /// preset, its own filters when [`as_bands`] says they fit the tone control as they are,
    /// else the bands [`fit_for`] gives, in EQ mode with loudness 0 and the same name; any
    /// other preset as it is. An imported preset stays as it is for a tone control with no
    /// bands, which nothing fits.
    pub fn for_device(&self, eq: &EqSpec) -> Cow<'_, Preset> {
        let (Some(source), Ok(bands)) = (&self.source, u8::try_from(eq.bands.len())) else {
            return Cow::Borrowed(self);
        };
        if bands == 0 {
            return Cow::Borrowed(self);
        }
        Cow::Owned(Preset {
            name: self.name.clone(),
            bands,
            mode: EqMode::Eq,
            loudness: 0,
            band: as_bands(eq, &source.correction)
                .unwrap_or_else(|| fit_for(eq, source).bands.clone()),
            source: None,
        })
    }
}

/// The most filters the computer EQ holds.
pub const COMPUTER_POINTS: usize = 16;

/// Why `preset` cannot be put on the computer for its filter count: `17 filters; the computer
/// EQ holds 16`. `None` for a preset the computer holds; a count is never cut down.
/// [`host_correction`] says it too, so no path to the computer has to ask first.
pub fn computer_refusal(preset: &Preset) -> Option<String> {
    let count = match &preset.source {
        Some(source) => source.correction.filters.len(),
        None => preset.band.len(),
    };
    (count > COMPUTER_POINTS)
        .then(|| format!("{count} filters; the computer EQ holds {COMPUTER_POINTS}"))
}

/// Whether `preset` changes nothing: every band or filter at 0 dB. Such a curve cannot play on
/// the DAC and the computer at once, so it is never moved between them.
pub fn is_neutral(preset: &Preset) -> bool {
    match &preset.source {
        Some(source) => source.correction.filters.iter().all(|f| f.g == 0.0),
        None => preset.band.iter().all(|band| band.g == 0.0),
    }
}

/// What moving a preset to the other side (the DAC or the computer) asks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BothSides {
    /// Nothing: the preset is not on the other side, or changes nothing.
    Free,
    /// The preset is already on the other side: the move asks first, so one correction is
    /// never left playing on both.
    Ask {
        /// The preset being moved.
        name: String,
        /// The preset that is on the other side, as that side calls it.
        other: String,
    },
}

/// Whether putting `preset` on one side while `other_side` (the name of what is on the other
/// side, if anything) is the same preset needs the question. The one decision every front end
/// and both directions make: a neutral curve ([`is_neutral`]) is free, since it cannot play twice.
pub fn both_sides(preset: &Preset, other_side: Option<&str>) -> BothSides {
    match other_side {
        Some(other) if same_name(&preset.name, other) && !is_neutral(preset) => BothSides::Ask {
            name: preset.name.clone(),
            other: other.to_string(),
        },
        _ => BothSides::Free,
    }
}

/// A computer curve as the bands of the tone control `eq`, when it fits as it is: exactly one
/// peaking filter per band, and each one's frequency, gain and Q inside the band's own step
/// table, from its first step to its last (the table a write converts through). The filters are matched to the bands in order of
/// frequency, and keep their values.
pub fn as_bands(eq: &EqSpec, correction: &Correction) -> Option<Vec<PresetBand>> {
    if eq.bands.is_empty()
        || correction.filters.len() != eq.bands.len()
        || correction
            .filters
            .iter()
            .any(|filter| filter.kind != FilterKind::Peaking)
    {
        return None;
    }
    let mut sorted = correction.filters.clone();
    sorted.sort_by(|a, b| a.f.total_cmp(&b.f));
    let between = |value: f64, a: f64, b: f64| value >= a.min(b) && value <= a.max(b);
    let (gain_low, gain_high) = (eq.gain_db(0), eq.gain_db(EqSpec::STEP_MAX));
    let (q_low, q_high) = (eq.q(0), eq.q(EqSpec::STEP_MAX));
    sorted
        .iter()
        .enumerate()
        .all(|(index, filter)| {
            let (Some(low), Some(high)) = (
                eq.frequency(index, 0),
                eq.frequency(index, EqSpec::STEP_MAX),
            ) else {
                return false;
            };
            between(filter.f, low, high)
                && between(filter.g, gain_low, gain_high)
                && between(filter.q, q_low, q_high)
        })
        .then(|| {
            sorted
                .iter()
                .map(|filter| PresetBand {
                    f: filter.f,
                    g: filter.g,
                    q: filter.q,
                })
                .collect()
        })
}

/// The preset a computer curve is saved as: an imported one called `name` with no origin, which
/// every device fits as [`Preset::for_device`] says.
pub fn from_computer(name: &str, correction: Correction) -> Preset {
    Preset::imported(
        name,
        Imported {
            origin: None,
            correction,
        },
    )
}

/// The correction a preset applies to the computer: an imported preset's filters and stated
/// preamp, a band preset's bands as peaking filters with a preamp of minus the curve's peak,
/// rounded up to 0.1 dB, so the correction never clips. A built-in is a band preset.
///
/// A band preset the DAC keeps flat or in loudness mode has no curve of its own to play, and
/// is refused with the reason, and so is a preset of more filters than the computer holds
/// ([`computer_refusal`]). The numbers are not checked here.
pub fn host_correction(preset: &Preset) -> Result<Correction, String> {
    if let Some(why) = computer_refusal(preset) {
        return Err(why);
    }
    if let Some(imported) = &preset.source {
        return Ok(imported.correction.clone());
    }
    if preset.mode != EqMode::Eq {
        let mode = match preset.mode {
            EqMode::Flat => "flat",
            _ => "loudness",
        };
        return Err(format!(
            "{} is a {mode} setting of the DAC's tone control, with no curve to put on the computer",
            preset.name
        ));
    }
    let bands: Vec<BandParams> = preset
        .band
        .iter()
        .map(|band| BandParams {
            f0: band.f,
            gain_db: band.g,
            q: band.q,
        })
        .collect();
    let peak = response::peak_db(&bands);
    // The nudge keeps a peak that is a whole step plus rounding noise from rounding up again.
    let preamp_db = match peak > 1e-9 {
        true => -((peak * 10.0 - 1e-9).ceil() / 10.0),
        false => 0.0,
    };
    Ok(Correction {
        preamp_db,
        filters: preset
            .band
            .iter()
            .map(|band| SourceFilter {
                kind: FilterKind::Peaking,
                f: band.f,
                g: band.g,
                q: band.q,
            })
            .collect(),
    })
}

/// What updating an imported preset with the device's curve is refused with: the curve would
/// replace the correction it was fitted from.
pub fn imported_update_refusal(name: &str) -> String {
    format!("{name} is imported: save the curve under a new name")
}

/// The most fits [`fit_for`] keeps: a key is about 2 KB, and every row of a list asks for its
/// fit on every frame, so the cap is well above the longest list.
const FIT_CACHE_MAX: usize = 4096;

/// The fit of `imported` to the tone control `eq`, computed once per process for the same
/// correction and layout, so lists, headers and a recall never fit in a loop.
pub fn fit_for(eq: &EqSpec, imported: &Imported) -> Arc<Fit> {
    static CACHE: Mutex<Option<HashMap<Vec<u64>, Arc<Fit>>>> = Mutex::new(None);
    let key = fit_key(eq, &imported.correction);
    let cached = |cache: &Option<HashMap<Vec<u64>, Arc<Fit>>>| {
        cache.as_ref().and_then(|map| map.get(&key)).cloned()
    };
    if let Some(found) = cached(&CACHE.lock().unwrap_or_else(PoisonError::into_inner)) {
        return found;
    }
    // Fitted outside the lock; a second caller racing for the same key keeps the first result.
    let fitted = Arc::new(fit::fit(eq, &imported.correction));
    let mut guard = CACHE.lock().unwrap_or_else(PoisonError::into_inner);
    let map = guard.get_or_insert_with(HashMap::new);
    if map.len() >= FIT_CACHE_MAX && !map.contains_key(&key) {
        map.clear();
    }
    Arc::clone(map.entry(key).or_insert(fitted))
}

/// What a fit depends on, as bits: the tone control's layout and ranges, then the correction.
///
/// `EqSpec.id`, `mode_sub` and `loudness_sub` are left out on purpose: fitted bands are
/// physical values, the same on every device with the same layout. The preamp is in the key
/// although it does not change the fit, so one key stands for one whole correction.
fn fit_key(eq: &EqSpec, correction: &Correction) -> Vec<u64> {
    let mut key = vec![
        eq.range.to_bits(),
        eq.gain_range.0.to_bits(),
        eq.gain_range.1.to_bits(),
        u64::from(eq.gain_zero),
        eq.q_range.0.to_bits(),
        eq.q_range.1.to_bits(),
        eq.q_center.to_bits(),
        eq.bands.len() as u64,
    ];
    for band in &eq.bands {
        key.extend([
            band.center.to_bits(),
            band.fscale_low.to_bits(),
            band.fscale_high.to_bits(),
        ]);
    }
    key.push(correction.preamp_db.to_bits());
    for filter in &correction.filters {
        key.extend([
            filter.kind as u64,
            filter.f.to_bits(),
            filter.g.to_bits(),
            filter.q.to_bits(),
        ]);
    }
    key
}

/// The name a file import takes: the file name without its extension, without a trailing
/// ` ParametricEQ` in any case, cut at [`NAME_MAX`] characters and trimmed.
pub fn default_name(file_stem: &str) -> String {
    const SUFFIX: &str = " parametriceq";
    let stem = file_stem.trim();
    let keep = match stem.len().checked_sub(SUFFIX.len()) {
        Some(at) if stem.is_char_boundary(at) && stem[at..].eq_ignore_ascii_case(SUFFIX) => at,
        _ => stem.len(),
    };
    let stem = stem[..keep].trim();
    let cut: String = stem.chars().take(NAME_MAX).collect();
    cut.trim().to_string()
}

/// One band of a preset, in physical units.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct PresetBand {
    /// Centre frequency in Hz.
    pub f: f64,
    /// Gain in dB.
    pub g: f64,
    /// Q.
    pub q: f64,
}

/// One write a recall sends, as a raw step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Write {
    /// One parameter of one band.
    Band {
        /// The band, counted from 0.
        band: usize,
        /// Which of its parameters.
        param: EqParam,
        /// The raw step it goes to.
        raw: u8,
    },
    /// The tone-control mode.
    Mode(EqMode),
    /// The loudness strength.
    Loudness(u8),
}

impl Write {
    /// The function this write addresses on a device whose tone control is `eq`; `None` for
    /// a band that tone control does not have.
    pub fn id(&self, eq: &EqSpec) -> Option<FunctionId> {
        match *self {
            Write::Band { band, param, .. } => eq.band_id(band, param),
            Write::Mode(_) => Some(eq.mode_id()),
            Write::Loudness(_) => Some(eq.loudness_id()),
        }
    }

    /// The raw value it writes.
    pub fn raw(&self) -> u8 {
        match *self {
            Write::Band { raw, .. } => raw,
            Write::Mode(mode) => mode as u8,
            Write::Loudness(raw) => raw,
        }
    }

    /// What a band write's raw step stands for on the tone control `eq`, in the units a band
    /// parameter is typed and sent in: Hz, dB or Q. `None` for a mode or loudness write, which
    /// carries no such unit, and for a frequency on a band `eq` does not have.
    pub fn value(&self, eq: &EqSpec) -> Option<f64> {
        match *self {
            Write::Band { band, param, raw } => match param {
                EqParam::Frequency => eq.frequency(band, raw),
                EqParam::Gain => Some(eq.gain_db(raw)),
                EqParam::Q => Some(eq.q(raw)),
            },
            Write::Mode(_) | Write::Loudness(_) => None,
        }
    }
}

/// A value a recall moved to the nearest step this tone control has, because it lay outside
/// the band's window or the device's range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Clamp {
    /// A band parameter.
    Band {
        /// The band, counted from 0.
        band: usize,
        /// Which of its parameters.
        param: EqParam,
        /// The value the preset asks for, in Hz, dB or Q.
        asked: f64,
        /// The value the step it was moved to stands for.
        got: f64,
    },
    /// The loudness strength, above [`LOUDNESS_MAX`].
    Loudness {
        /// The strength the preset asks for.
        asked: u8,
        /// The strength written.
        got: u8,
    },
}

impl fmt::Display for Clamp {
    /// `band 5 frequency 22000 Hz clamped to 19500 Hz`, with the band counted from 1.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Clamp::Band {
                band,
                param,
                asked,
                got,
            } => {
                let band = band + 1;
                match param {
                    EqParam::Frequency => write!(
                        f,
                        "band {band} frequency {asked} Hz clamped to {} Hz",
                        frequency_text(got)
                    ),
                    EqParam::Gain => {
                        write!(
                            f,
                            "band {band} gain {asked} dB clamped to {} dB",
                            gain_text(got)
                        )
                    }
                    EqParam::Q => write!(f, "band {band} Q {asked} clamped to {got:.1}"),
                }
            }
            Clamp::Loudness { asked, got } => write!(f, "loudness {asked} clamped to {got}"),
        }
    }
}

/// What a recall sends, in the order it sends it, and what it had to move to fit.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Plan {
    /// The writes, first to last. Empty when the device already has the curve.
    pub writes: Vec<Write>,
    /// Every value moved to the nearest step, in band order, then a Loudness preset's
    /// strength.
    pub clamped: Vec<Clamp>,
}

/// One command that carries a [`Write`] out on a device, in the terms every front end sends:
/// a band parameter as the value its raw step stands for, the mode by name, and the loudness
/// strength through the profile's own control when one names that function.
#[derive(Debug, Clone, PartialEq)]
pub enum EqCommand {
    /// One parameter of one band.
    Band {
        /// The band, counted from 1, as on the wire.
        band: usize,
        /// `f`, `g` or `q`.
        param: &'static str,
        /// Hz, dB or Q.
        value: f64,
    },
    /// The tone-control mode, by name.
    Mode(&'static str),
    /// The loudness strength.
    Loudness {
        /// The key of the profile's control on the loudness function, when it has one.
        key: Option<String>,
        /// The loudness function.
        id: FunctionId,
        /// The strength, 0 to 127.
        raw: u8,
    },
}

impl Plan {
    /// The commands that send this plan's writes, in its order, on `profile`'s device. Empty
    /// for a profile with no tone control.
    pub fn commands(&self, profile: &Profile) -> Vec<EqCommand> {
        let Some(eq) = profile.eq.as_ref() else {
            return Vec::new();
        };
        self.writes
            .iter()
            .filter_map(|write| match *write {
                Write::Band { band, param, .. } => Some(EqCommand::Band {
                    band: band + 1,
                    param: param.word(),
                    value: write.value(eq)?,
                }),
                Write::Mode(mode) => Some(EqCommand::Mode(mode.name())),
                Write::Loudness(raw) => {
                    let id = eq.loudness_id();
                    Some(EqCommand::Loudness {
                        key: profile.control_by_id(id).map(|c| c.key.clone()),
                        id,
                        raw,
                    })
                }
            })
            .collect()
    }
}

/// What recalling a preset sends: the plan, and the commands that carry it out.
#[derive(Debug, Clone, PartialEq)]
pub struct Recall {
    /// The writes and the values moved to fit.
    pub plan: Plan,
    /// The plan's writes as commands.
    pub commands: Vec<EqCommand>,
}

impl Preset {
    /// The writes that recall this preset onto a device with `profile`, given the raw
    /// readings `raw` returns; see [`plan_recall`].
    pub fn eq_writes(
        &self,
        profile: &Profile,
        raw: impl Fn(FunctionId) -> Option<i64>,
    ) -> Result<Recall, ProfileError> {
        let eq = profile.eq.as_ref().ok_or(ProfileError::NoEq)?;
        let plan = plan_recall(eq, raw, self)?;
        let commands = plan.commands(profile);
        Ok(Recall { plan, commands })
    }
}

/// The presets that fit the tone control `eq`: the built-ins on its own centres, then the
/// `user` presets with its band count and every imported one, in the order given. Each comes
/// with whether it is a built-in.
pub fn fitting(eq: &EqSpec, user: &[Preset]) -> Vec<(Preset, bool)> {
    builtins(eq)
        .into_iter()
        .map(|preset| (preset, true))
        .chain(
            user.iter()
                .filter(|preset| {
                    preset.is_imported() || usize::from(preset.bands) == eq.bands.len()
                })
                .map(|preset| (preset.clone(), false)),
        )
        .collect()
}

/// The built-in presets for the tone control `eq`, in [`BUILTIN_NAMES`] order, every one in
/// EQ mode with loudness 0.
///
/// Flat puts every band at 0 dB on this tone control's own centre frequency and centre Q, raw
/// 64 of each, for any band count. Warm, Bass and Presence exist for five and
/// three bands: each is Flat with the bands it shapes set to fixed Hz, dB and Q.
pub fn builtins(eq: &EqSpec) -> Vec<Preset> {
    let Ok(bands) = u8::try_from(eq.bands.len()) else {
        return Vec::new();
    };
    let flat: Vec<PresetBand> = eq
        .bands
        .iter()
        .map(|b| PresetBand {
            f: b.center,
            g: 0.0,
            q: eq.q_center,
        })
        .collect();
    let curve = |name: &str, changes: &[(usize, PresetBand)]| {
        let mut band = flat.clone();
        for &(index, values) in changes {
            if let Some(slot) = band.get_mut(index) {
                *slot = values;
            }
        }
        Preset {
            name: name.to_string(),
            bands,
            mode: EqMode::Eq,
            loudness: 0,
            band,
            source: None,
        }
    };
    std::iter::once(curve(BUILTIN_NAMES[0], &[]))
        .chain(
            shaped(eq.bands.len())
                .iter()
                .map(|&(name, changes)| curve(name, changes)),
        )
        .collect()
}

/// The names of the built-ins a tone control with `bands` bands is offered, for a list drawn
/// without a device: all of [`BUILTIN_NAMES`] for five and three bands, Flat alone otherwise.
pub fn builtin_names(bands: usize) -> &'static [&'static str] {
    match shaped(bands).is_empty() {
        true => &BUILTIN_NAMES[..1],
        false => &BUILTIN_NAMES,
    }
}

/// Whether `name` is a built-in preset's, in any case and with blanks round it or not.
pub fn is_builtin(name: &str) -> bool {
    BUILTIN_NAMES.iter().any(|builtin| same_name(builtin, name))
}

/// Why a text cannot be a preset's name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameError {
    /// Nothing but blanks, or nothing at all.
    Blank,
    /// More characters than [`NAME_MAX`]: how many it has, the blanks round it left out.
    TooLong(usize),
    /// A control character inside it: a newline, a tab or another.
    Control,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            NameError::Blank => write!(f, "a preset needs a name"),
            NameError::TooLong(length) => write!(
                f,
                "a preset's name is at most {NAME_MAX} characters, not {length}"
            ),
            NameError::Control => write!(
                f,
                "a preset's name cannot hold a control character, such as a newline or a tab"
            ),
        }
    }
}

impl std::error::Error for NameError {}

/// `text` as a preset's name: without the blanks round it, or why it cannot be one — blank,
/// longer than [`NAME_MAX`] characters, or holding a control character.
///
/// Every writer of a name goes through here, the configuration file's reader and each front
/// end's before it asks one, so a name is kept, shown and looked up in the one spelling.
pub fn checked_name(text: &str) -> Result<&str, NameError> {
    let name = text.trim();
    if name.is_empty() {
        return Err(NameError::Blank);
    }
    if name.chars().any(char::is_control) {
        return Err(NameError::Control);
    }
    match name.chars().count() {
        length if length > NAME_MAX => Err(NameError::TooLong(length)),
        _ => Ok(name),
    }
}

/// Whether `a` and `b` name the same preset: the same text once the blanks round each are
/// left out, case aside. Every lookup of a preset by name compares this way.
pub fn same_name(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// The preset select's option, and state, for a curve no listed preset matches. No preset is
/// ever on under this name, and none can be given it.
pub const CUSTOM: &str = "custom";

/// Which preset a device's tone control is on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum On<'a> {
    /// The mode, or a band's frequency, gain or Q, has not read a step yet.
    Unread,
    /// This preset.
    Preset(&'a Preset),
    /// None of the listed presets.
    Custom,
}

/// Which of `fitting` the tone control `eq` is on, given the raw readings `raw` returns: the one
/// rule every front end reads. [`On::Unread`] until the mode and every band's frequency, gain
/// and Q have read a step; then `recalled`, the preset last recalled, while it is listed and
/// still [`matches()`]; else the first listed preset that matches; else [`On::Custom`]. A preset
/// named [`CUSTOM`] (a file's old block) is never listed, so it is skipped for the next preset
/// that matches.
pub fn on<'a>(
    eq: &EqSpec,
    raw: impl Fn(FunctionId) -> Option<i64>,
    fitting: impl IntoIterator<Item = &'a Preset> + Clone,
    recalled: Option<&str>,
) -> On<'a> {
    let step = |id| current(&raw, id).filter(|step| *step <= EqSpec::STEP_MAX);
    let read = step(eq.mode_id()).is_some()
        && (0..eq.bands.len()).all(|band| {
            [EqParam::Gain, EqParam::Frequency, EqParam::Q]
                .into_iter()
                .all(|param| eq.band_id(band, param).and_then(&step).is_some())
        });
    if !read {
        return On::Unread;
    }
    let listed = || {
        fitting
            .clone()
            .into_iter()
            .filter(|preset| !same_name(&preset.name, CUSTOM))
    };
    let matching = |preset: &&'a Preset| matches(eq, &raw, preset);
    recalled
        .and_then(|name| listed().find(|preset| same_name(&preset.name, name)))
        .filter(matching)
        .or_else(|| listed().find(matching))
        .map_or(On::Custom, On::Preset)
}

/// Why a text cannot be a new preset's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameRefusal {
    /// What [`checked_name`] refuses: blank, too long, a control character.
    Name(NameError),
    /// A built-in's name, in any case.
    BuiltIn(String),
    /// [`CUSTOM`], in any case, which stands for a curve no preset matches.
    Reserved(String),
    /// Another preset's name, as that preset spells it.
    Taken(String),
}

impl fmt::Display for NameRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NameRefusal::Name(why) => write!(f, "{why}"),
            NameRefusal::BuiltIn(name) => write!(f, "{name:?} is a built-in preset's name"),
            NameRefusal::Reserved(name) => write!(
                f,
                "{name:?} stands for a curve no preset matches, so no preset can take it"
            ),
            NameRefusal::Taken(name) => write!(f, "a preset named {name:?} already exists"),
        }
    }
}

impl std::error::Error for NameRefusal {}

/// `text` as a new preset's name, without the blanks round it, or why it cannot be: what
/// [`checked_name`] refuses, a built-in's name, [`CUSTOM`], or the name of one of `existing`
/// other than `renaming` (the preset being renamed, which may take its own name in another
/// spelling, and a file's old preset named [`CUSTOM`] keeps that name). Every comparison is
/// [`same_name`]'s. Every writer of a name asks this, and each
/// front end adds its own hint to the words.
pub fn new_name<'t>(
    text: &'t str,
    existing: impl IntoIterator<Item = impl AsRef<str>>,
    renaming: Option<&str>,
) -> Result<&'t str, NameRefusal> {
    let name = checked_name(text).map_err(NameRefusal::Name)?;
    if is_builtin(name) {
        return Err(NameRefusal::BuiltIn(name.to_string()));
    }
    if same_name(name, CUSTOM) && !renaming.is_some_and(|old| same_name(old, CUSTOM)) {
        return Err(NameRefusal::Reserved(name.to_string()));
    }
    let taken = existing.into_iter().find(|other| {
        let other = other.as_ref();
        same_name(other, name) && !renaming.is_some_and(|old| same_name(other, old))
    });
    match taken {
        Some(other) => Err(NameRefusal::Taken(other.as_ref().trim().to_string())),
        None => Ok(name),
    }
}

/// The preset of `fitting` named `name`, the blanks round it and case aside.
pub fn find<'a>(fitting: &'a [Preset], name: &str) -> Option<&'a Preset> {
    fitting.iter().find(|preset| same_name(&preset.name, name))
}

/// Why a preset cannot be recalled by name.
#[derive(Debug)]
pub enum RecallError {
    /// No preset of the ones that fit is named so.
    NotFound {
        /// The name asked for.
        name: String,
        /// The names of the presets that fit, in their order.
        fitting: Vec<String>,
    },
    /// The preset's writes cannot be planned.
    Plan(ProfileError),
}

/// Find `name` among `fitting`, the presets that fit the device `profile` describes, and plan
/// its recall from the raw readings `raw` returns ([`Preset::eq_writes`]).
pub fn recall_named(
    profile: &Profile,
    fitting: &[Preset],
    name: &str,
    raw: impl Fn(FunctionId) -> Option<i64>,
) -> Result<(Preset, Recall), RecallError> {
    let Some(found) = find(fitting, name) else {
        return Err(RecallError::NotFound {
            name: name.to_string(),
            fitting: fitting.iter().map(|preset| preset.name.clone()).collect(),
        });
    };
    let recall = found.eq_writes(profile, raw).map_err(RecallError::Plan)?;
    Ok((found.clone(), recall))
}

/// The curve a device has now, as a preset named `name`: its mode, its loudness strength and
/// every band's frequency, gain and Q, read through `raw`.
///
/// Frequencies are rounded to 1 Hz, gains to 0.1 dB and Q to 0.01, each to as many more
/// decimals as it takes to recall the same raw step, which the lowest band's sub-hertz steps
/// need. `None` while any of those readings is missing or is not a step the tone control has.
pub fn from_readings(
    eq: &EqSpec,
    raw: impl Fn(FunctionId) -> Option<i64>,
    name: impl Into<String>,
) -> Option<Preset> {
    let step = |id: FunctionId| {
        raw(id)
            .and_then(|value| u8::try_from(value).ok())
            .filter(|&value| value <= 127)
    };
    let mode = EqMode::from_raw(step(eq.mode_id())?)?;
    let loudness = step(eq.loudness_id())?;
    let band = (0..eq.bands.len())
        .map(|band| {
            let read = |param| step(eq.band_id(band, param)?);
            let (f, g, q) = (
                read(EqParam::Frequency)?,
                read(EqParam::Gain)?,
                read(EqParam::Q)?,
            );
            Some(PresetBand {
                f: rounded(eq.frequency(band, f)?, 0, f, |hz| {
                    eq.frequency_raw(band, hz)
                }),
                g: rounded(eq.gain_db(g), 1, g, |db| Some(eq.gain_raw(db))),
                q: rounded(eq.q(q), 2, q, |value| Some(eq.q_raw(value))),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Preset {
        name: name.into(),
        bands: u8::try_from(eq.bands.len()).ok()?,
        mode,
        loudness,
        band,
        source: None,
    })
}

/// The writes that put `preset` on a device whose tone control is `eq` and whose current
/// readings `raw` gives.
///
/// Only a step that differs from its reading is written; one with no reading is always
/// written. The order keeps a boost from sounding anywhere it is not meant to: every gain
/// going down, then the frequency and Q changes, then every gain going up (an unread gain
/// goes with the down group when its target is at or under 0 dB, the up group otherwise). The
/// mode goes last in EQ mode, so the curve is complete before the device applies it, and first
/// otherwise.
///
/// The loudness strength is written only for a Loudness preset, just before the mode, so the
/// device enters Loudness already at that strength; in EQ or Flat mode the strength write is
/// skipped and the device's own is left alone.
///
/// A value outside its band's window or the device's range goes to the nearest step and is
/// reported in [`Plan::clamped`]. A preset with another band count, or a non-finite value, is
/// an error. An imported preset is fitted first ([`Preset::for_device`]), and its bands, on the
/// device's own steps, are recalled with no clamp.
pub fn plan_recall(
    eq: &EqSpec,
    raw: impl Fn(FunctionId) -> Option<i64>,
    preset: &Preset,
) -> Result<Plan, ProfileError> {
    let preset = &*preset.for_device(eq);
    fits(eq, preset)?;
    let mut clamped = Vec::new();
    let targets = preset
        .band
        .iter()
        .enumerate()
        .map(|(band, values)| band_steps(eq, preset, band, values, &mut clamped))
        .collect::<Result<Vec<_>, _>>()?;

    let bands = band_writes(eq, &raw, &targets);
    let mode = (current(&raw, eq.mode_id()).and_then(EqMode::from_raw) != Some(preset.mode))
        .then_some(Write::Mode(preset.mode));
    let writes = match preset.mode {
        EqMode::Eq => bands.into_iter().chain(mode).collect(),
        EqMode::Flat => mode.into_iter().chain(bands).collect(),
        EqMode::Loudness => {
            let loudness = preset.loudness.min(LOUDNESS_MAX);
            if loudness != preset.loudness {
                clamped.push(Clamp::Loudness {
                    asked: preset.loudness,
                    got: loudness,
                });
            }
            let strength = (current(&raw, eq.loudness_id()) != Some(loudness))
                .then_some(Write::Loudness(loudness));
            strength.into_iter().chain(mode).chain(bands).collect()
        }
    };
    Ok(Plan { writes, clamped })
}

/// Whether `preset` is on, decided by what can be heard: the device is in the preset's mode,
/// every band's gain reads the step the preset's gain converts to through `eq`, and so do its
/// frequency and Q, except for a band whose preset gain is the zero step, which shapes nothing
/// whatever its frequency and Q. For a preset in Loudness mode only, the loudness strength is
/// the preset's too, since the strength does nothing in EQ or Flat mode. A missing reading
/// counts as a difference, and a preset that does not fit is never on. An imported preset is
/// compared by its fit ([`Preset::for_device`]).
///
/// This is not "a recall would write nothing": [`plan_recall`] still writes a zero-gain band's
/// frequency and Q back to the preset's.
pub fn matches(eq: &EqSpec, raw: impl Fn(FunctionId) -> Option<i64>, preset: &Preset) -> bool {
    let preset = &*preset.for_device(eq);
    if fits(eq, preset).is_err() {
        return false;
    }
    if current(&raw, eq.mode_id()).and_then(EqMode::from_raw) != Some(preset.mode) {
        return false;
    }
    if preset.mode == EqMode::Loudness
        && current(&raw, eq.loudness_id()) != Some(preset.loudness.min(LOUDNESS_MAX))
    {
        return false;
    }
    let mut clamped = Vec::new();
    preset.band.iter().enumerate().all(|(band, values)| {
        let Ok([frequency, gain, q]) = band_steps(eq, preset, band, values, &mut clamped) else {
            return false;
        };
        let now = |param| current(&raw, eq.band_id(band, param)?);
        if gain.is_none() || now(EqParam::Gain) != gain {
            return false;
        }
        gain == Some(eq.gain_zero) || (now(EqParam::Frequency) == frequency && now(EqParam::Q) == q)
    })
}

/// The writes that put every band on Flat's values, 0 dB on its own centre frequency and the
/// centre Q (the built-in Flat's raw steps), leaving the mode and the loudness strength alone;
/// ordered as [`plan_recall`] orders its band writes: gains down, then frequencies and Q, then
/// gains up. Only a step that differs from its reading is written.
pub fn flatten(eq: &EqSpec, raw: impl Fn(FunctionId) -> Option<i64>) -> Plan {
    let Some(flat) = builtins(eq).into_iter().next() else {
        return Plan::default();
    };
    let Ok(mut plan) = plan_recall(eq, raw, &flat) else {
        return Plan::default();
    };
    plan.writes
        .retain(|write| matches!(write, Write::Band { .. }));
    plan
}

/// One band of a built-in.
const fn band(f: f64, g: f64, q: f64) -> PresetBand {
    PresetBand { f, g, q }
}

/// The built-ins beyond Flat that a tone control with `bands` bands has, each with the bands it
/// moves off Flat, counted from 0.
fn shaped(bands: usize) -> &'static [(&'static str, &'static [(usize, PresetBand)])] {
    match bands {
        5 => &FIVE_BANDS,
        3 => &THREE_BANDS,
        _ => &[],
    }
}

/// The five-band built-ins beyond Flat.
const FIVE_BANDS: [(&str, &[(usize, PresetBand)]); 3] = [
    (
        "Warm",
        &[
            (0, band(40.0, 3.0, 0.7)),
            (1, band(160.0, 1.5, 0.7)),
            (4, band(10_000.0, 2.0, 0.7)),
        ],
    ),
    (
        "Bass",
        &[(0, band(40.0, 4.0, 0.7)), (1, band(160.0, 2.0, 0.7))],
    ),
    ("Presence", &[(3, band(3000.0, 2.5, 1.4))]),
];

/// The three-band built-ins beyond Flat.
const THREE_BANDS: [(&str, &[(usize, PresetBand)]); 3] = [
    (
        "Warm",
        &[(0, band(63.0, 3.0, 0.7)), (2, band(6300.0, 2.0, 0.7))],
    ),
    ("Bass", &[(0, band(63.0, 4.0, 0.7))]),
    ("Presence", &[(2, band(3000.0, 2.5, 1.0))]),
];

/// How many decimals beyond its first choice [`rounded`] tries before it keeps a value as
/// it is.
const EXTRA_PLACES: i32 = 6;

/// `value` rounded to `places` decimals, or to as many more as it takes for `step` to give
/// `raw` back, so a curve saved from a device recalls to the steps it was read from.
///
/// A small cut never comes back as `-0.0`: that rounds to the step for 0 dB, not to the cut's
/// own, so a longer value is taken instead.
fn rounded(value: f64, places: i32, raw: u8, step: impl Fn(f64) -> Option<u8>) -> f64 {
    (places..=places + EXTRA_PLACES)
        .map(|places| {
            let scale = 10f64.powi(places);
            (value * scale).round() / scale
        })
        .find(|&candidate| step(candidate) == Some(raw))
        .unwrap_or(value)
}

/// An error unless `preset` has as many bands as `eq`, and says so. An imported preset that
/// reaches here was not fitted, because `eq` has no bands.
fn fits(eq: &EqSpec, preset: &Preset) -> Result<(), ProfileError> {
    let device = eq.bands.len();
    let declared = usize::from(preset.bands);
    let reason = if preset.is_imported() {
        "it is imported and this tone control has no bands".to_string()
    } else if declared != device {
        format!("it is for {declared} bands and this tone control has {device}")
    } else if preset.band.len() != device {
        format!(
            "it lists {} bands under bands = {declared}",
            preset.band.len()
        )
    } else {
        return Ok(());
    };
    Err(ProfileError::Preset {
        name: preset.name.clone(),
        reason,
    })
}

/// The raw frequency, gain and Q one preset band recalls to, each clamped to the nearest step
/// and reported in `clamped` when it lies outside the band's window or the device's range.
fn band_steps(
    eq: &EqSpec,
    preset: &Preset,
    band: usize,
    values: &PresetBand,
    clamped: &mut Vec<Clamp>,
) -> Result<[Option<u8>; 3], ProfileError> {
    for (param, value) in [
        (EqParam::Frequency, values.f),
        (EqParam::Gain, values.g),
        (EqParam::Q, values.q),
    ] {
        if !value.is_finite() {
            return Err(ProfileError::Preset {
                name: preset.name.clone(),
                reason: format!(
                    "band {} {} is not a finite number ({value})",
                    band + 1,
                    param_name(param)
                ),
            });
        }
    }
    let mut report = |param, asked, got: Option<f64>| {
        if let Some(got) = got {
            clamped.push(Clamp::Band {
                band,
                param,
                asked,
                got,
            });
        }
    };

    // A frequency is clamped when its nearest step lies outside the window, not merely when
    // it falls past the edge, since a window's edges are not round numbers and a frequency
    // saved at one is rounded. One at or under 0 Hz sits below every step.
    let steps = eq
        .frequency_steps(band, values.f)
        .unwrap_or(f64::NEG_INFINITY);
    let top = f64::from(EqSpec::STEP_MAX);
    let frequency = steps.round().clamp(0.0, top) as u8;
    if !(0.0..=top).contains(&steps.round()) {
        report(EqParam::Frequency, values.f, eq.frequency(band, frequency));
    }

    let (gain_min, gain_max) = eq.gain_range;
    let gain = eq.gain_raw(values.g);
    if !(gain_min..=gain_max).contains(&values.g) {
        report(EqParam::Gain, values.g, Some(eq.gain_db(gain)));
    }

    let (q_min, q_max) = eq.q_range;
    let q = eq.q_raw(values.q);
    if !(q_min..=q_max).contains(&values.q) {
        report(EqParam::Q, values.q, Some(eq.q(q)));
    }

    Ok([Some(frequency), Some(gain), Some(q)])
}

/// The writes that take band `band` (0-based) of a device from its readings to `targets` —
/// `[frequency, gain, Q]` raw steps, `None` for a parameter left alone — in the same order a
/// recall writes them (see [`plan_recall`]): gain down first, then frequency and Q, then gain
/// up, a parameter already on target left out. An unread gain counts as going down to a cut or
/// 0 dB, up to a boost. Nothing for a band the tone control does not have.
///
/// A front end's own band edit (the TUI's edit line) writes through this, so it and a recall
/// never order the same writes differently.
pub fn band_plan(
    eq: &EqSpec,
    raw: impl Fn(FunctionId) -> Option<i64>,
    band: usize,
    targets: [Option<u8>; 3],
) -> Vec<Write> {
    let mut all = vec![[None; 3]; eq.bands.len()];
    match all.get_mut(band) {
        Some(slot) => *slot = targets,
        None => return Vec::new(),
    }
    band_writes(eq, &raw, &all)
}

/// The band writes that take a device from its readings to `targets`, one `[frequency, gain,
/// Q]` per band with `None` for a parameter left alone: gains going down, then frequencies
/// and Q, then gains going up, each group in band order.
fn band_writes(
    eq: &EqSpec,
    raw: &impl Fn(FunctionId) -> Option<i64>,
    targets: &[[Option<u8>; 3]],
) -> Vec<Write> {
    let (mut down, mut shape, mut up) = (Vec::new(), Vec::new(), Vec::new());
    for (band, &[frequency, gain, q]) in targets.iter().enumerate() {
        let now = |param| current(raw, eq.band_id(band, param)?);
        for (param, target) in [(EqParam::Frequency, frequency), (EqParam::Q, q)] {
            if let Some(target) = target.filter(|&target| now(param) != Some(target)) {
                shape.push(Write::Band {
                    band,
                    param,
                    raw: target,
                });
            }
        }
        let Some(target) = gain else {
            continue;
        };
        let write = Write::Band {
            band,
            param: EqParam::Gain,
            raw: target,
        };
        match now(EqParam::Gain) {
            Some(reading) if reading == target => {}
            Some(reading) if target < reading => down.push(write),
            Some(_) => up.push(write),
            None if target <= eq.gain_zero => down.push(write),
            None => up.push(write),
        }
    }
    down.into_iter().chain(shape).chain(up).collect()
}

/// The raw step one function reads now, when it reads one a byte can hold.
fn current(raw: &impl Fn(FunctionId) -> Option<i64>, id: FunctionId) -> Option<u8> {
    raw(id).and_then(|value| u8::try_from(value).ok())
}

/// A band parameter's name, as a sentence says it.
fn param_name(param: EqParam) -> &'static str {
    match param {
        EqParam::Frequency => "frequency",
        EqParam::Gain => "gain",
        EqParam::Q => "Q",
    }
}

/// A mode as the word a file carries it by: `flat`, `eq` or `loudness`, read in any case.
#[cfg(feature = "serde")]
mod mode_word {
    use crate::convert::EqMode;

    pub(super) fn serialize<S: serde::Serializer>(
        mode: &EqMode,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(mode.name())
    }

    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> Result<EqMode, D::Error> {
        let word = <String as serde::Deserialize>::deserialize(deserializer)?;
        word.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests;
