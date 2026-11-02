// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the tone control is on its way to: the steps asked for that the device has not
//! answered, the band edit that plans from them, the mode `f` and the Loudness half come
//! back to.

use std::sync::Arc;
use std::time::Instant;

use svolder_profiles::convert::{frequency_text, gain_text};
use svolder_profiles::tone::fit::Fit;
use svolder_profiles::tone::preset::{self, SWITCHED_TO_EQ, Write};
use svolder_profiles::tone::response::{self, F_MAX, F_MIN};
use svolder_profiles::{Convert, EqMode, EqParam, EqSpec, db_tables};
use svolder_proto::FunctionId;

use crate::tui::app::curve_drag;
use crate::tui::boxes::edit_line::EditLine;

use super::{ANSWER_LIFETIME, App, Intent, ToneRow};

/// How many points the target curve is kept at, log-spaced from 20 Hz to 20 kHz: fine enough
/// that a column of the widest terminal reads the line between two of them.
const TARGET_POINTS: usize = 1024;

/// The tone control's edit: the focused band and row, the edit line, the mode `f` and the
/// Loudness half come back to, and the curve's clip and target.
pub struct ToneEdit {
    /// The focused band, 0-based.
    pub band: usize,
    /// Which row of the device screen's Tone pane the keys act on.
    pub row: ToneRow,
    /// Which half of the tone screen's header the cursor was last on: `Loudness` when set, else
    /// `Mode` (where the header opens). `Tab` reads this to return the cursor to the half it
    /// left rather than always opening on Mode; [`App::move_tone_row`] and a click on either
    /// half keep it current whenever the cursor rests on the header.
    pub(super) header_loudness: bool,
    /// The last non-Flat mode the tone control was seen in or asked for this device session (a
    /// reading, a Mode-select step, a preset recall's mode, `f` itself, or a band edit's switch
    /// to EQ — [`App::note_mode`]), for `f` to come back to when it toggles off Flat
    /// ([`App::mode_before`]). `None` with none seen yet, which is EQ; forgotten on a device
    /// switch, since this app is rebuilt fresh for one.
    pub(super) mode_before_flat: Option<EqMode>,
    /// The same memory as [`ToneEdit::mode_before_flat`], kept for Loudness instead: the last mode
    /// other than Loudness this device session noted, for a click on the Loudness half to come
    /// back to when it toggles the strength off.
    pub(super) mode_before_loudness: Option<EqMode>,
    /// The line a band's frequency, gain and Q are typed into on the tone-control screen, in
    /// place of the band's row, while it is open.
    pub edit_line: Option<EditLine>,
    /// The imported preset the tone screen draws against, kept between frames.
    pub(crate) target_cache: TargetCache,
}

/// The imported correction the tone screen draws a preset against, worked out once when the
/// preset on the device, or loaded with changes, is an imported one, so no frame fits or
/// evaluates a filter.
#[derive(Debug, PartialEq)]
pub struct TargetView {
    /// The correction's response in dB, without its preamp, at [`TARGET_POINTS`] log-spaced
    /// frequencies.
    grid: Vec<f64>,
    /// What the header says of the fit: `fit 0.9 dB`.
    pub header: String,
    /// The side pane's heading: where the correction came from and how many filters it has.
    pub source: String,
    /// One line per filter: `LSC 105 +6.0 0.70`.
    pub filters: Vec<String>,
    /// The preamp line.
    pub preamp: String,
    /// The fit as one line ([`Fit::line`]).
    pub fit: String,
}

impl TargetView {
    /// The view of the imported preset `name` for the tone control `eq`, or `None` where the
    /// preset is not imported or there is no such preset.
    fn of(presets: &[preset::Preset], eq: &EqSpec, name: &str) -> Option<TargetView> {
        let imported = presets
            .iter()
            .find(|preset| preset::same_name(&preset.name, name))?
            .source
            .as_ref()?;
        let fit: Arc<Fit> = preset::fit_for(eq, imported);
        let correction = &imported.correction;
        let grid = response::correction_db(correction, &response::log_freqs(TARGET_POINTS));
        let count = correction.filters.len();
        Some(TargetView {
            grid,
            header: format!("fit {:.1} dB", fit.rms_db),
            source: format!(
                "{} · {count} filter{}",
                imported.origin.as_deref().unwrap_or("Imported"),
                if count == 1 { "" } else { "s" }
            ),
            filters: correction
                .filters
                .iter()
                .map(|filter| {
                    format!(
                        "{:<3} {:>5} {:>5} {:>4.2}",
                        filter.kind.code(),
                        frequency_text(filter.f),
                        gain_text(filter.g),
                        filter.q
                    )
                })
                .collect(),
            preamp: format!("Preamp {} dB", gain_text(correction.preamp_db)),
            fit: fit.line(),
        })
    }

    /// The correction's response in dB at `hz`, read off the kept points, or `None` outside
    /// 20 Hz to 20 kHz.
    pub fn db_at(&self, hz: f64) -> Option<f64> {
        if !(F_MIN..=F_MAX).contains(&hz) {
            return None;
        }
        let at = (hz / F_MIN).log10() / (F_MAX / F_MIN).log10() * (self.grid.len() - 1) as f64;
        let low = (at.floor() as usize).min(self.grid.len() - 1);
        let high = (low + 1).min(self.grid.len() - 1);
        let along = at - low as f64;
        Some(self.grid[low] + (self.grid[high] - self.grid[low]) * along)
    }
}

/// What a [`TargetView`] was worked out for: the preset's name and the tone control's layout.
#[derive(Debug, Clone, PartialEq)]
struct TargetKey {
    /// The preset's name.
    name: String,
    /// The tone control's bands, and the bits of its frequency window.
    layout: (usize, u64),
    /// The correction the view was worked out from, so a preset rewritten under the same name
    /// misses.
    source: Option<preset::Imported>,
}

/// The last [`TargetView`] asked for, with what it was asked for, and whether the last frame
/// drew the side pane. Interior mutability since a frame draws from a shared `&App`.
#[derive(Debug, Default)]
pub struct TargetCache {
    /// The view and the key it answers.
    held: std::cell::RefCell<Option<(TargetKey, Option<Arc<TargetView>>)>>,
    /// Whether the frame drawn last had an imported preset's target, which with the wide tier
    /// decides the side pane the layout the mouse and the animation layer read back has.
    target: std::cell::Cell<bool>,
}

impl TargetCache {
    /// Forget the view: the presets were read again, so the correction may have changed.
    pub fn clear(&self) {
        self.held.borrow_mut().take();
    }

    /// Note whether the frame being drawn has a target.
    pub fn set_target(&self, drawn: bool) {
        self.target.set(drawn);
    }

    /// Whether the frame drawn last had a target.
    pub fn target(&self) -> bool {
        self.target.get()
    }
}

/// A tone-control step asked for — a band parameter, the mode or the loudness strength — that
/// the device has not answered every write of yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pending {
    /// The raw step asked for last.
    pub step: u8,
    /// How many of the writes asking for this function have not been answered.
    pub outstanding: u32,
    /// When the step was asked for last, which [`ANSWER_LIFETIME`] is measured from.
    pub since: Instant,
}

impl Pending {
    /// Whether the device has left this step unanswered for [`ANSWER_LIFETIME`], as of `now`.
    pub(super) fn expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.since) >= ANSWER_LIFETIME
    }

    /// Take one reading of the function as an answer, and say whether that ends the wait: the
    /// device reads the step asked for last, or this reading answers the last write
    /// outstanding.
    pub(super) fn answered(&mut self, reading: Option<i64>) -> bool {
        self.outstanding = self.outstanding.saturating_sub(1);
        self.outstanding == 0 || reading == Some(i64::from(self.step))
    }
}

impl App {
    /// The imported preset `name` as the tone screen draws it against the device: its target
    /// curve, its fit and its filters. Kept until the preset or the tone control changes, so a
    /// frame only looks it up; `None` for a preset that is not imported.
    pub(crate) fn target_view(&self, name: &str) -> Option<Arc<TargetView>> {
        let eq = self.profile.eq.as_ref()?;
        let source = self
            .presets
            .saved
            .iter()
            .find(|preset| preset::same_name(&preset.name, name))
            .and_then(|preset| preset.source.clone());
        let key = TargetKey {
            name: name.to_string(),
            layout: (eq.bands.len(), eq.range.to_bits()),
            source,
        };
        let mut held = self.tone.target_cache.held.borrow_mut();
        if let Some((_, view)) = held.as_ref().filter(|(had, _)| *had == key) {
            return view.clone();
        }
        let view = TargetView::of(&self.presets.saved, eq, name).map(Arc::new);
        *held = Some((key, view.clone()));
        view
    }

    /// Note the mode the device has just reported as the mode before Flat too
    /// ([`App::note_mode`]), once it names one.
    pub(super) fn tone_read(&mut self) {
        if let Some(mode) = self.eq_mode() {
            self.note_mode(mode);
        }
    }

    /// Note that the tone control's mode is, or is on its way to being, `mode`: a reading that
    /// names one ([`App::tone_read`]), a Mode-select step, a preset recall's own mode, `f`
    /// itself, a click on the Loudness half, and a band edit's switch to EQ all call this. Kept
    /// as the mode before Flat ([`App::mode_before`]) for `f` to come back to and as the mode
    /// before Loudness ([`App::mode_before_loudness`]) for a click on the Loudness half to come
    /// back to; a mode of `Flat` itself is not the mode before Flat, nor is `Loudness` the mode
    /// before Loudness, so either leaves its own memory untouched.
    pub(super) fn note_mode(&mut self, mode: EqMode) {
        if mode != EqMode::Flat {
            self.tone.mode_before_flat = Some(mode);
        }
        if mode != EqMode::Loudness {
            self.tone.mode_before_loudness = Some(mode);
        }
    }

    /// The mode `f` asks for on its way back from Flat: the last non-Flat mode this device
    /// session noted ([`App::note_mode`]), or EQ with none noted.
    pub(super) fn mode_before(&self) -> EqMode {
        self.tone.mode_before_flat.unwrap_or(EqMode::Eq)
    }

    /// The mode a click on the Loudness half asks for on its way back from Loudness: the last
    /// mode other than Loudness this device session noted ([`App::note_mode`]), or EQ with none
    /// noted — the same rule [`App::mode_before`] follows for Flat.
    pub(super) fn mode_before_loudness(&self) -> EqMode {
        self.tone.mode_before_loudness.unwrap_or(EqMode::Eq)
    }

    /// How far the tone control's curve boosts, in dB, while that boost can clip: the peak
    /// [`response::clipping_peak`] finds in the curve as the tone screen draws it, each band at the
    /// step [`curve_drag::drawn_raw`] gives (the edit line's preview, a drag's held step, the step
    /// it is on its way to, else its reading). Worked out when asked for. `None` outside EQ mode,
    /// while a band has not reported in full, and for a curve that does not rise above 0 dB.
    pub fn clip_peak(&self) -> Option<f64> {
        let eq = self.profile.eq.as_ref()?;
        response::clipping_peak(eq, |id| curve_drag::drawn_raw(self, id))
    }

    /// The highest volume step a curve that boosts stays clear of clipping at, on a full-scale
    /// master: the step [`response::threshold_step`] finds for [`App::clip_peak`] in the
    /// attenuation table the volume control converts through. Every step above it can clip.
    ///
    /// `None` wherever there is nothing to warn about, and on a device whose volume names no
    /// table to measure a step's attenuation by.
    pub fn clip_step(&self) -> Option<i64> {
        let peak = self.clip_peak()?;
        let Some(Convert::Table(name)) = self.volume()?.convert.as_ref() else {
            return None;
        };
        let step = response::threshold_step(peak, db_tables::table(name)?)?;
        i64::try_from(step).ok()
    }

    /// The mode the tone control last reported, once it has reported one it has a name for.
    pub fn eq_mode(&self) -> Option<EqMode> {
        let eq = self.profile.eq.as_ref()?;
        u8::try_from(self.raw_of(eq.mode_id())?)
            .ok()
            .and_then(EqMode::from_raw)
    }

    /// Finish a band edit: `writes` are what the gain, frequency or Q keys, the wheel over a
    /// band's row or `0` ask of the bands. On a tone control in Flat mode — the device's
    /// bypass, under which no band applies — the mode then goes to EQ so the edit is heard,
    /// written after the bands so the curve is whole when the device applies it, and the key
    /// line says [`SWITCHED_TO_EQ`]. In Loudness mode, and before the mode has been read, the
    /// mode is left alone. An edit that writes no band switches nothing.
    ///
    /// Every path that edits a band comes through here, and each band write, and the switch to
    /// EQ, is noted as pending until the device answers it, for the next band key to step
    /// from. The mode it switches from is the one the tone control is on its way to
    /// ([`App::planned_mode`]), so a band edit made while a recall is still being answered
    /// switches the mode that recall asked for. A recall notes its own writes
    /// ([`App::note_writes`]): a preset writes its own mode.
    pub(crate) fn band_edit(&mut self, mut writes: Vec<Intent>) -> Vec<Intent> {
        let edits = writes
            .iter()
            .any(|write| matches!(write, Intent::EqSet { .. }));
        if edits && self.planned_mode() == Some(EqMode::Flat) {
            writes.push(Intent::EqMode(EqMode::Eq.name().to_string()));
            self.notify(SWITCHED_TO_EQ);
            self.note_mode(EqMode::Eq);
        }
        self.note_pending(&writes);
        writes
    }

    /// Note every band write and mode write of `writes` as pending, at the step its value
    /// converts to: the step the next band key, flatten or recall plans from until the device
    /// has answered.
    pub(super) fn note_pending(&mut self, writes: &[Intent]) {
        let Some(eq) = self.profile.eq.as_ref() else {
            return;
        };
        let steps: Vec<(FunctionId, u8)> = writes
            .iter()
            .filter_map(|write| match write {
                Intent::EqSet { band, param, value } => {
                    let (band, param) = band.checked_sub(1).zip(param.parse::<EqParam>().ok())?;
                    Some((eq.band_id(band, param)?, eq.step(band, param, *value)?))
                }
                Intent::EqMode(mode) => Some((eq.mode_id(), mode.parse::<EqMode>().ok()? as u8)),
                _ => None,
            })
            .collect();
        for (id, step) in steps {
            self.expect(id, step);
        }
    }

    /// Note every write of a recall's plan as pending — each band parameter, the mode and the
    /// loudness strength, at the raw step the plan writes — as it is sent to the tone control
    /// `eq`: what every band key, flatten, edit line and recall plans from until the device has
    /// answered, so none of them works from the readings the recall is about to replace. A mode
    /// the plan writes is noted as the mode before Flat too ([`App::note_mode`]).
    pub(crate) fn note_writes(&mut self, eq: &EqSpec, writes: &[Write]) {
        for write in writes {
            if let Some(id) = write.id(eq) {
                self.expect(id, write.raw());
            }
            if let Write::Mode(mode) = write {
                self.note_mode(*mode);
            }
        }
    }

    /// Note a write of the loudness strength `raw` as pending, the step the next loudness step
    /// plans from until the device has answered.
    pub(super) fn note_loudness(&mut self, raw: i64) {
        let Some(id) = self.profile.eq.as_ref().map(EqSpec::loudness_id) else {
            return;
        };
        self.expect_raw(id, raw);
    }

    /// Note that a write of `raw` to function `id` was asked for, as a band step's is, when the
    /// value fits the step a pending write counts in; a value that does not is not tracked.
    pub(crate) fn expect_raw(&mut self, id: FunctionId, raw: i64) {
        if let Ok(step) = u8::try_from(raw) {
            self.expect(id, step);
        }
    }

    /// Note one more write asking function `id` for raw step `step`, the step asked for last, as
    /// of this app's clock.
    fn expect(&mut self, id: FunctionId, step: u8) {
        let outstanding = self
            .readings
            .pending
            .get(&id)
            .map_or(0, |pending| pending.outstanding);
        self.readings.pending.insert(
            id,
            Pending {
                step,
                outstanding: outstanding + 1,
                since: self.now,
            },
        );
    }

    /// The raw value one function is on its way to: the step asked for last while the device has
    /// not answered every write asking for it (see [`super::Readings::pending`]), and otherwise the
    /// one it reads. Every plan of a band edit, a flatten or a recall is made from this, as is a
    /// step of the tone control's mode or loudness, so a write made before the device has answered
    /// the ones before it follows on from what they asked for rather than from the readings they
    /// are about to replace.
    pub(crate) fn planned_raw(&self, id: FunctionId) -> Option<i64> {
        match self.readings.pending.get(&id) {
            Some(pending) => Some(i64::from(pending.step)),
            None => self.raw_of(id),
        }
    }

    /// The mode the tone control is on its way to ([`App::planned_raw`]), once a write or a
    /// reading names one.
    pub(crate) fn planned_mode(&self) -> Option<EqMode> {
        let eq = self.profile.eq.as_ref()?;
        u8::try_from(self.planned_raw(eq.mode_id())?)
            .ok()
            .and_then(EqMode::from_raw)
    }
}
