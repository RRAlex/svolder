// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone control: its mode, its curve (display only, drawn here from the response maths),
//! the preset that is on and the band table, in the one band wording.
//!
//! `curve_fixture` (tests only) writes the numbers the page's `tone-curve.js` is held to.

use svolder_profiles::tone::preset::{self, On, Preset};
use svolder_profiles::{EqMode, EqParam, EqSpec, tone::response, units};

use svolder_proto::FunctionId;

use super::{Snapshot, minus};

#[cfg(test)]
mod curve_fixture;

/// How many points the curve has: log-spaced, 20 Hz to 20 kHz.
const POINTS: usize = 181;
/// The curve's y range, ± dB.
const RANGE_DB: f64 = 13.0;

/// The tone control.
#[derive(Debug)]
pub(crate) struct Tone {
    pub(crate) modes: Vec<ModeChoice>,
    /// The mode as the device reads it: `flat`, `eq` or `loudness`; `None` before a reading.
    pub(crate) mode: Option<&'static str>,
    pub(crate) curve: Curve,
    pub(crate) note: Option<&'static str>,
    /// Whether the mode is Loudness: its button then carries the round - and +.
    pub(crate) loud: bool,
    /// The loudness strength's reading, while the mode is Loudness and it has been read.
    pub(crate) strength: Option<String>,
}

/// One value of the mode track.
#[derive(Debug)]
pub(crate) struct ModeChoice {
    /// What a write sends: `flat`, `eq`, `loudness`.
    pub(crate) name: &'static str,
    /// What the track says: `Flat`, `EQ`, `Loudness`.
    pub(crate) label: &'static str,
    pub(crate) on: bool,
}

/// The curve: the bands' response, its peak and a marker per band.
#[derive(Debug)]
pub(crate) struct Curve {
    /// The bands' curve in dB at the curve's 181 frequencies, while every band is read.
    pub(crate) response: Option<Vec<f64>>,
    /// The summed peak's line: its y and its words.
    pub(crate) peak: Option<(String, String)>,
    /// The summed peak in dB.
    pub(crate) peak_db: Option<f64>,
    /// A marker per band that has reported in full, at its own frequency and gain.
    pub(crate) markers: Vec<Marker>,
    /// Each band's frequency, gain and Q as raw steps, `None` where not read.
    pub(crate) band_readings: Vec<[Option<i64>; 3]>,
}

/// One band's marker.
#[derive(Debug)]
pub(crate) struct Marker {
    pub(crate) band: usize,
    /// Its frequency in Hz.
    pub(crate) hz: f64,
    /// The level it sits at, in dB.
    pub(crate) db: f64,
}

/// The tone control's mode as `raw` reads it.
fn mode_of(eq: &EqSpec, raw: impl Fn(FunctionId) -> Option<i64>) -> Option<EqMode> {
    raw(eq.mode_id())
        .and_then(|r| u8::try_from(r).ok())
        .and_then(EqMode::from_raw)
}

/// y of `db`.
fn y(db: f64) -> f64 {
    10.0 * (RANGE_DB - db.clamp(-RANGE_DB, RANGE_DB))
}

/// The dB a band's marker sits at: its own gain in EQ and in Flat, 0 in Loudness and while the
/// mode is unread.
fn marker_db(mode: Option<EqMode>, gain: f64) -> f64 {
    match mode {
        Some(EqMode::Eq | EqMode::Flat) => gain,
        _ => 0.0,
    }
}

/// The words of a peak of `peak` dB, with the page's minus.
fn peak_words(peak: f64) -> String {
    minus(&format!("peak {peak:+.1} dB"))
}

/// Band `band`'s own frequency in Hz and gain in dB, when it has reported all three of its
/// values and each is a step the band holds.
fn own(eq: &EqSpec, raw: impl Fn(FunctionId) -> Option<i64>, band: usize) -> Option<(f64, f64)> {
    let step = |param: EqParam| u8::try_from(raw(eq.band_id(band, param)?)?).ok();
    step(EqParam::Q)?;
    Some((
        eq.frequency(band, step(EqParam::Frequency)?)?,
        eq.gain_db(step(EqParam::Gain)?),
    ))
}

/// Every step of every band's frequency, gain and Q with the core's words, as JSON for the page's
/// drag: `{"bands":[{"f":[[hz,"words"],…]},…],"gain":[[db,"words"],…],"q":[[q,"words"],…]}`, the
/// index of an entry being its raw step. It holds no `<`, so it cannot close its `<script>`.
pub(crate) fn steps_json(eq: &EqSpec) -> String {
    steps(eq).to_string().replace('<', "\\u003c")
}

/// [`steps_json`]'s value.
pub(crate) fn steps(eq: &EqSpec) -> serde_json::Value {
    let steps = 0..=EqSpec::STEP_MAX;
    let words = |band: usize, param: EqParam, raw: u8| {
        units::band_text(eq, band, param, i64::from(raw)).map_or_else(String::new, |t| minus(&t))
    };
    let bands: Vec<_> = (0..eq.bands.len())
        .map(|band| {
            let f: Vec<_> = steps
                .clone()
                .map(|raw| {
                    serde_json::json!([
                        eq.frequency(band, raw),
                        words(band, EqParam::Frequency, raw)
                    ])
                })
                .collect();
            serde_json::json!({ "f": f })
        })
        .collect();
    // Gain and Q do not depend on the band; band 0 speaks for them.
    let by = |value: fn(&EqSpec, u8) -> f64, param: EqParam| -> Vec<_> {
        steps
            .clone()
            .map(|raw| serde_json::json!([value(eq, raw), words(0, param, raw)]))
            .collect()
    };
    serde_json::json!({
        "bands": bands,
        "gain": by(EqSpec::gain_db, EqParam::Gain),
        "q": by(EqSpec::q, EqParam::Q),
    })
}

/// The tone control's view.
pub(crate) fn tone(s: &Snapshot, eq: &EqSpec) -> Tone {
    let raw = |id| s.raw(id);
    let mode = mode_of(eq, raw);
    let modes = [EqMode::Flat, EqMode::Eq, EqMode::Loudness]
        .into_iter()
        .map(|m| ModeChoice {
            name: m.name(),
            label: m.display(),
            on: mode == Some(m),
        })
        .collect();
    let freqs = response::log_freqs(POINTS);
    let bands = response::bands_of(eq, raw);
    let response_db: Option<Vec<f64>> = bands.as_ref().map(|b| response::response_db(b, &freqs));
    let peak_db = response::clipping_peak(eq, raw);
    let peak = peak_db.map(|peak| (format!("{:.1}", y(peak)), peak_words(peak)));
    // A band's point: its own frequency and gain in EQ and in Flat, 0 dB in Loudness and while
    // the mode is unread. A band that has not reported in full draws none.
    let markers = (0..eq.bands.len())
        .filter_map(|band| {
            let (hz, gain) = own(eq, raw, band)?;
            let db = marker_db(mode, gain);
            Some(Marker {
                band: band + 1,
                hz,
                db,
            })
        })
        .collect();
    let band_readings: Vec<[Option<i64>; 3]> = (0..eq.bands.len())
        .map(|band| {
            [EqParam::Frequency, EqParam::Gain, EqParam::Q]
                .map(|param| eq.band_id(band, param).and_then(raw))
        })
        .collect();
    let note = match mode {
        Some(EqMode::Flat) => Some(
            "Flat bypasses the bands; the faint line is what EQ would do. Step a band and the \
             mode switches to EQ.",
        ),
        Some(EqMode::Loudness) => Some(
            "Loudness follows the volume: it adds more the lower you listen, and nothing at full \
             volume.",
        ),
        _ => None,
    };
    let loud = mode == Some(EqMode::Loudness);
    let strength = loud
        .then(|| raw(eq.loudness_id()))
        .flatten()
        .map(|r| r.to_string());
    Tone {
        modes,
        mode: mode.map(EqMode::name),
        curve: Curve {
            response: response_db,
            peak,
            peak_db,
            markers,
            band_readings,
        },
        note,
        loud,
        strength,
    }
}

/// The presets' part.
#[derive(Debug)]
pub(crate) struct Presets {
    pub(crate) status: PresetStatus,
    pub(crate) rows: Vec<PresetRow>,
    /// Whether save, rename and delete show: the viewer may write and the configuration can be.
    pub(crate) editable: bool,
    /// Whether a recall may be made from here.
    pub(crate) recall: bool,
    /// Why save, rename and delete are hidden, for a viewer who may write.
    pub(crate) note: Option<String>,
}

/// The status line: `lead`, the preset's name in bold, `tail`.
#[derive(Debug)]
pub(crate) struct PresetStatus {
    pub(crate) lead: &'static str,
    pub(crate) name: Option<String>,
    pub(crate) tail: &'static str,
}

/// One preset's row.
#[derive(Debug)]
pub(crate) struct PresetRow {
    pub(crate) name: String,
    pub(crate) built_in: bool,
    pub(crate) on: bool,
    /// What its recall and its delete post: `{"name": …}`.
    pub(crate) json: String,
    /// What its rename posts beside the new name: `{"from": …}`.
    pub(crate) rename_json: String,
}

/// The presets that fit, the one that is on ([`preset::on`] with the daemon's memory), and what
/// the viewer may do with them.
pub(crate) fn presets(s: &Snapshot, eq: &EqSpec) -> Presets {
    let raw = |id| s.raw(id);
    let listed: Vec<Preset> = s.presets.iter().map(|(p, _)| p.clone()).collect();
    let recalled = s.info.recalled.as_deref();
    let mode = mode_of(eq, raw);
    let on = preset::on(eq, raw, &listed, recalled);
    let on_name = match on {
        On::Preset(p) => Some(p.name.clone()),
        On::Unread | On::Custom => None,
    };
    let status = match on {
        On::Unread => PresetStatus {
            lead: "Reading the tone control…",
            name: None,
            tail: "",
        },
        On::Preset(p) => PresetStatus {
            lead: "On: ",
            name: Some(p.name.clone()),
            tail: "",
        },
        On::Custom => match recalled.and_then(|name| preset::find(&listed, name)) {
            Some(edited) => PresetStatus {
                lead: "",
                name: Some(edited.name.clone()),
                tail: ", edited. Save the bands to keep them.",
            },
            None if mode == Some(EqMode::Flat) => PresetStatus {
                lead: "Mode Flat: no preset is on.",
                name: None,
                tail: "",
            },
            None => PresetStatus {
                lead: "No preset matches these bands.",
                name: None,
                tail: "",
            },
        },
    };
    let rows = s
        .presets
        .iter()
        .map(|(p, built_in)| PresetRow {
            name: p.name.clone(),
            built_in: *built_in,
            on: on_name
                .as_deref()
                .is_some_and(|n| preset::same_name(n, &p.name)),
            json: serde_json::json!({ "name": p.name }).to_string(),
            rename_json: serde_json::json!({ "from": p.name }).to_string(),
        })
        .collect();
    let writer = s.access.writes && !s.access.guest;
    Presets {
        status,
        rows,
        editable: writer && s.writable.is_ok(),
        recall: writer,
        note: match (writer, s.writable) {
            (true, Err(why)) => Some(format!("Saving presets is off: {why}")),
            _ => None,
        },
    }
}

/// The band table.
#[derive(Debug)]
pub(crate) struct Bands {
    pub(crate) rows: Vec<BandRow>,
    /// Whether the rows are faint: the tone control is in Flat.
    pub(crate) faint: bool,
}

/// One band's row, in the one band wording with the page's minus.
#[derive(Debug)]
pub(crate) struct BandRow {
    pub(crate) band: usize,
    pub(crate) f: String,
    pub(crate) g: String,
    pub(crate) q: String,
}

/// Every band's frequency, gain and Q.
pub(crate) fn bands(s: &Snapshot, eq: &EqSpec) -> Bands {
    let text = |band, param| {
        eq.band_id(band, param)
            .and_then(|id| s.raw(id))
            .and_then(|raw| units::band_text(eq, band, param, raw))
            .map_or_else(|| "–".to_string(), |t| minus(&t))
    };
    let mode = mode_of(eq, |id| s.raw(id));
    Bands {
        rows: (0..eq.bands.len())
            .map(|band| BandRow {
                band: band + 1,
                f: text(band, EqParam::Frequency),
                g: text(band, EqParam::Gain),
                q: text(band, EqParam::Q),
            })
            .collect(),
        faint: mode == Some(EqMode::Flat),
    }
}
