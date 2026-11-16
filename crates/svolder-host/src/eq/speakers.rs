// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The virtual speakers: what a chosen set is, the oracle for what the chain does to a tone,
//! and the view a front end draws from.
//!
//! A set is one 4-channel float WAV (channels LL, LR, RL, RR: the speaker, then the ear it is
//! heard in) and a sidecar that says what it is. The chain's convolvers play it (see
//! [`live`](super::live)); an ear hears `LL` and `RL` on the left, `LR` and `RR` on the right:
//!
//! ```text
//! left ear  = LL · L  +  RL · R
//! right ear = LR · L  +  RR · R
//! ```
//!
//! **The set's WAV must stay on disk for as long as any running chain was started with it.** A
//! sink rate change makes the running chain re-open the file to resample it, and a convolver
//! that cannot open it leaves the chain alive with its graph never started: all sound stops,
//! dry included, with no exit code and nothing for a supervisor to see. The chain also keeps
//! its convolvers, idle, after the stage is turned off until it next restarts. So forgetting or
//! evicting a set refuses or defers while a running chain holds it (the store and the
//! controller enforce this); the chain's readback cannot say which WAV it holds, because the
//! convolver's file is config and not a parameter.
//!
//! Everything here is pure. The words of the two choices and their technical terms (BRIR for a
//! measured room, HRTF for the free field) are svolder-speakers' table ([`Room`] reads them).

use std::f64::consts::PI;

use serde::{Deserialize, Serialize};

use super::crossfeed::CURVE_POINTS;

/// Whether a set is the speakers in a measured room or in the free field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Room {
    /// A measured room: a BRIR.
    With,
    /// The free field: an HRTF.
    No,
}

impl Room {
    /// The same choice in the catalogue's table, which holds its words.
    pub fn catalogue(self) -> svolder_speakers::Room {
        match self {
            Room::With => svolder_speakers::Room::With,
            Room::No => svolder_speakers::Room::No,
        }
    }

    /// The wire word.
    pub fn word(self) -> &'static str {
        self.catalogue().wire()
    }

    /// The choice's name for a person.
    pub fn name(self) -> &'static str {
        self.catalogue().label()
    }

    /// The technical term for the choice, shown beside its name.
    pub fn term(self) -> &'static str {
        self.catalogue().term()
    }
}

/// Why the set called `label` cannot be forgotten now: it plays. The daemon refuses a forget
/// with these words, and a front end that sees the set play says them before it asks.
pub fn plays_now_words(label: &str) -> String {
    format!("{label} plays now; turn virtual speakers off or choose another set first")
}

/// What a set's sidecar (`<id>.toml`) says that the engine and the view use. Fields the sidecar
/// holds beyond these (checksums, the source's size) are the store's and are not read here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SetInfo {
    /// The set's id, the WAV's file stem.
    pub id: String,
    /// Whether it is a room or the free field.
    pub room: Room,
    /// The dataset and subject as a person reads them.
    pub label: String,
    /// The licence's SPDX id.
    pub licence: String,
    /// The dataset's own credit words, where its licence asks for them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credit: Option<String>,
    /// The source's page.
    pub source_url: String,
    /// The WAV's sample rate in Hz.
    pub rate: u32,
    /// The length of each impulse response in samples.
    pub taps: u32,
    /// The measurement angles found, as a sentence ("left 30.0 deg, right 330.0 deg, 1.5 m").
    pub angles: String,
    /// The loudness change against Off, in dB (negative is quieter).
    pub loudness_db: f64,
    /// Whether the impulse responses were cut to the length cap at import.
    #[serde(default)]
    pub cut: bool,
}

/// The four impulse responses of a set, as the WAV holds them.
#[derive(Debug, Clone, PartialEq)]
pub struct Irs {
    /// The WAV's sample rate in Hz.
    pub rate_hz: u32,
    /// The samples of LL, LR, RL and RR.
    pub taps: [Vec<f32>; 4],
}

impl Irs {
    /// The impulse responses in a 4-channel WAV, read by the store's own reader
    /// ([`svolder_speakers::wav`], whose cap of [`svolder_speakers::wav::MAX_BYTES`] it keeps),
    /// or why the file is not one.
    pub fn from_wav(bytes: &[u8]) -> Result<Irs, String> {
        if bytes.len() > svolder_speakers::wav::MAX_BYTES {
            return Err("the WAV is over 64 MiB".to_string());
        }
        let wav = svolder_speakers::wav::read_channels(bytes, &[4]).map_err(|e| e.to_string())?;
        if wav.rate == 0 {
            return Err("the WAV has no sample rate".to_string());
        }
        let Ok([ll, lr, rl, rr]) = <[Vec<f32>; 4]>::try_from(wav.channels) else {
            return Err("the WAV does not have 4 channels".to_string());
        };
        if ll.is_empty() {
            return Err("the WAV has no samples".to_string());
        }
        Ok(Irs {
            rate_hz: wav.rate,
            taps: [ll, lr, rl, rr],
        })
    }
}

/// The four transfer functions at one frequency, as complex gains `[re, im]`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpeakersResponse {
    /// Left speaker to left ear.
    pub ll: [f64; 2],
    /// Left speaker to right ear.
    pub lr: [f64; 2],
    /// Right speaker to left ear.
    pub rl: [f64; 2],
    /// Right speaker to right ear.
    pub rr: [f64; 2],
}

impl SpeakersResponse {
    /// A tone on the left speaker alone: what the left and the right ear hear.
    pub fn left_only(&self) -> ([f64; 2], [f64; 2]) {
        (self.ll, self.lr)
    }

    /// A tone on the right speaker alone: what the left and the right ear hear.
    pub fn right_only(&self) -> ([f64; 2], [f64; 2]) {
        (self.rl, self.rr)
    }

    /// The same tone on both speakers: what the left and the right ear hear.
    pub fn mono(&self) -> ([f64; 2], [f64; 2]) {
        let add = |a: [f64; 2], b: [f64; 2]| [a[0] + b[0], a[1] + b[1]];
        (add(self.ll, self.rl), add(self.lr, self.rr))
    }
}

/// One impulse response's transfer function at `hz` for a file at `rate_hz`: `Σ h[n] e^(-jωn)`.
fn transfer(taps: &[f32], rate_hz: u32, hz: f64) -> [f64; 2] {
    let w = 2.0 * PI * hz / f64::from(rate_hz);
    let (mut re, mut im) = (0.0, 0.0);
    for (n, h) in taps.iter().enumerate() {
        let angle = w * n as f64;
        re += f64::from(*h) * angle.cos();
        im -= f64::from(*h) * angle.sin();
    }
    [re, im]
}

/// What the stage does to a tone at `hz` on a sink running at `rate_hz`: each impulse response's
/// transfer function, worked out directly from the samples (the oracle the chain is measured
/// against). When the sink's rate is not the file's, the engine resamples the file and that adds
/// one source sample of delay (`1 / irs.rate_hz` seconds, measured on the running chain): the
/// oracle applies it as a phase of `-2 pi hz / irs.rate_hz` on all four responses, magnitude
/// untouched. A tone at or above half the file's rate or the sink's has no response (zeros):
/// the file holds nothing there and the sink cannot play it.
pub fn speakers_response(irs: &Irs, rate_hz: u32, hz: f64) -> SpeakersResponse {
    let top = f64::from(irs.rate_hz.min(rate_hz)) / 2.0;
    let resampled = rate_hz != irs.rate_hz;
    let lag = -2.0 * PI * hz / f64::from(irs.rate_hz);
    let at = |channel: usize| match hz < top {
        true => {
            let [re, im] = transfer(&irs.taps[channel], irs.rate_hz, hz);
            match resampled {
                true => [
                    re * lag.cos() - im * lag.sin(),
                    re * lag.sin() + im * lag.cos(),
                ],
                false => [re, im],
            }
        }
        false => [0.0, 0.0],
    };
    SpeakersResponse {
        ll: at(0),
        lr: at(1),
        rl: at(2),
        rr: at(3),
    }
}

/// The four magnitudes a screen draws, in dB on crossfeed's log grid ([`CURVE_POINTS`] points
/// from 20 Hz to 20 kHz): what each speaker does to each ear.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpeakersCurve {
    /// The frequencies in Hz.
    pub hz: Vec<f64>,
    /// Left speaker to left ear in dB (floored at -120), to 0.01 dB.
    pub ll_db: Vec<f64>,
    /// Left speaker to right ear.
    pub lr_db: Vec<f64>,
    /// Right speaker to left ear.
    pub rl_db: Vec<f64>,
    /// Right speaker to right ear.
    pub rr_db: Vec<f64>,
}

impl SpeakersCurve {
    /// The curves of `irs` for a sink running at `rate_hz`. A DFT per point and impulse
    /// response: keep the result ([`CurveCache`]) rather than work it out per view.
    pub fn of(irs: &Irs, rate_hz: u32) -> SpeakersCurve {
        let db = |[re, im]: [f64; 2]| {
            let gain = re.hypot(im);
            ((20.0 * gain.max(1e-6).log10()) * 100.0).round() / 100.0
        };
        let mut curve = SpeakersCurve::default();
        for i in 0..CURVE_POINTS {
            let hz = 20.0 * 10f64.powf(i as f64 / 20.0);
            let r = speakers_response(irs, rate_hz, hz);
            curve.hz.push((hz * 100.0).round() / 100.0);
            curve.ll_db.push(db(r.ll));
            curve.lr_db.push(db(r.lr));
            curve.rl_db.push(db(r.rl));
            curve.rr_db.push(db(r.rr));
        }
        curve
    }
}

/// The curves worked out so far, by set id and sink rate, so a view build (every publish) never
/// repeats the DFT.
#[derive(Debug, Default)]
pub struct CurveCache {
    curves: std::collections::HashMap<(String, u32), SpeakersCurve>,
    computed: usize,
}

impl CurveCache {
    /// The curve of set `id` (whose impulse responses are `irs`) at `rate_hz`, worked out on
    /// first use.
    pub fn get(&mut self, id: &str, irs: &Irs, rate_hz: u32) -> &SpeakersCurve {
        let computed = &mut self.computed;
        self.curves
            .entry((id.to_string(), rate_hz))
            .or_insert_with(|| {
                *computed += 1;
                SpeakersCurve::of(irs, rate_hz)
            })
    }

    /// Drop every curve of set `id` (it was forgotten or fetched again).
    pub fn forget(&mut self, id: &str) {
        self.curves.retain(|(held, _), _| held != id);
    }

    /// How many curves were worked out (not served from the cache).
    pub fn computed(&self) -> usize {
        self.computed
    }
}

/// Why the virtual speakers turned themselves off. The view keeps the reason (and its note)
/// until the next change or turn-on clears it, as crossfeed's does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffReason {
    /// The output the computer plays into changed.
    OutputChanged,
    /// The owner (daemon or TUI) started and found them on.
    Restart,
    /// The filter chain restarted.
    ChainRestarted,
    /// The owner stopped.
    Stopped,
    /// The chosen set's WAV is missing or damaged.
    SetMissing,
}

impl OffReason {
    /// The word the view carries.
    pub fn word(self) -> &'static str {
        match self {
            OffReason::OutputChanged => "output-changed",
            OffReason::Restart => "restart",
            OffReason::ChainRestarted => "chain-restarted",
            OffReason::Stopped => "stopped",
            OffReason::SetMissing => "set-missing",
        }
    }
}

/// The note that goes with [`OffReason::SetMissing`].
pub fn missing_note(label: &str) -> String {
    format!("virtual speakers off: {label} is missing or damaged, fetch it again")
}

/// One fetched set as a list shows it.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SetView {
    /// Its id.
    pub id: String,
    /// `with` or `no`.
    pub room: String,
    /// "With room" or "No room".
    pub room_name: String,
    /// "BRIR" or "HRTF".
    pub term: String,
    /// The dataset and subject.
    pub label: String,
    /// The licence's SPDX id.
    pub licence: String,
    /// The dataset's credit words, where its licence asks for them.
    pub credit: Option<String>,
    /// The source's page.
    pub source_url: String,
}

impl SetView {
    /// `set` as a list row.
    pub fn of(set: &SetInfo) -> SetView {
        SetView {
            id: set.id.clone(),
            room: set.room.word().to_string(),
            room_name: set.room.name().to_string(),
            term: set.room.term().to_string(),
            label: set.label.clone(),
            licence: set.licence.clone(),
            credit: set.credit.clone(),
            source_url: set.source_url.clone(),
        }
    }
}

/// The virtual speakers as a screen shows it. Every field has a default, so a view from an older
/// daemon still parses.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SpeakersView {
    /// Whether they play.
    pub on: bool,
    /// The chosen set's id; `None` before one is chosen.
    pub set: Option<String>,
    /// `with` or `no`.
    pub room: Option<String>,
    /// "With room" or "No room".
    pub room_name: Option<String>,
    /// "BRIR" or "HRTF".
    pub term: Option<String>,
    /// The chosen set's dataset and subject.
    pub label: String,
    /// Its licence's SPDX id.
    pub licence: String,
    /// The dataset's credit words, where its licence asks for them.
    pub credit: Option<String>,
    /// The source's page.
    pub source_url: String,
    /// The measurement angles found.
    pub angles: String,
    /// The length of the impulse responses in ms.
    pub taps_ms: f64,
    /// The loudness change against Off in dB (negative is quieter).
    pub loudness_db: f64,
    /// Whether the impulse responses were cut to the length cap ("cut to 0.5 s").
    pub cut: bool,
    /// A sentence about why they turned themselves off, kept with `off_reason`.
    pub note: Option<String>,
    /// Why they turned themselves off, as an [`OffReason`] word.
    pub off_reason: Option<String>,
    /// Why they cannot be changed now; the controller sets it.
    pub unavailable: Option<String>,
    /// The four magnitudes for the picture; empty with no set chosen.
    pub curve: SpeakersCurve,
    /// The chosen set's arrivals and decay for the pictures; `None` from an older daemon or
    /// before the set is analysed.
    pub timing: Option<svolder_speakers::analysis::Timing>,
    /// The fetched sets.
    pub sets: Vec<SetView>,
}

impl SpeakersView {
    /// The view of `chosen` (its sidecar and its curve from a [`CurveCache`]; `None` for no
    /// choice), playing when `on`, beside the fetched `sets`.
    pub fn of(
        chosen: Option<(&SetInfo, &SpeakersCurve)>,
        on: bool,
        sets: &[SetInfo],
        note: Option<String>,
    ) -> SpeakersView {
        let mut view = SpeakersView {
            on,
            note,
            sets: sets.iter().map(SetView::of).collect(),
            ..SpeakersView::default()
        };
        if let Some((set, curve)) = chosen {
            view.set = Some(set.id.clone());
            view.room = Some(set.room.word().to_string());
            view.room_name = Some(set.room.name().to_string());
            view.term = Some(set.room.term().to_string());
            view.label = set.label.clone();
            view.licence = set.licence.clone();
            view.credit = set.credit.clone();
            view.source_url = set.source_url.clone();
            view.angles = set.angles.clone();
            view.taps_ms = f64::from(set.taps) / f64::from(set.rate.max(1)) * 1000.0;
            view.loudness_db = set.loudness_db;
            view.cut = set.cut;
            view.curve = curve.clone();
        }
        view
    }
}

#[cfg(test)]
mod tests;
