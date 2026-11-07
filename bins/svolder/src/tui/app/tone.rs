// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone control's keys: the Tone pane's rows and the tone-control screen's, the mode and
//! loudness steps, the flat mode and the flat curve, and each band's nudges.

use crossterm::event::KeyCode;
use svolder_profiles::{EqMode, EqParam, EqSpec, tone::preset};
use svolder_proto::{FunctionId, ValueType};

use crate::tui::boxes::edit_line;
use crate::tui::boxes::presets;

use super::intent::set;
use super::{App, Intent, stepped};

/// What one key press multiplies or divides a band's Q by.
const Q_FACTOR: f64 = 1.1;
/// The strongest loudness setting the function takes.
pub const LOUDNESS_MAX: i64 = 127;

/// Which row of the device screen's Tone pane the keys act on.
///
/// The pane's rows are the tone control's own: its mode, how strong its loudness curve is, and
/// one per band. `↑↓` move between them and `←→` change whichever one they are on, exactly as
/// they do in the Controls pane beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToneRow {
    /// The mode, which is a select over flat, EQ and loudness.
    Mode,
    /// How strong the loudness curve is, which is a level.
    Loudness,
    /// The band [`ToneEdit::band`](super::ToneEdit::band) has focused, which is also the band the
    /// tone-control screen's own keys act on.
    Band,
    /// A point of the computer's table, 0-based, on the computer side.
    Point(usize),
}

impl App {
    /// Toggle the tone control's flat mode, the device's own bypass: from EQ or Loudness it
    /// asks for Flat; pressed again from Flat it asks for the mode before ([`App::mode_before`]),
    /// EQ by default. The bands keep what they hold either way, so a mode back to EQ brings every
    /// one of them back. Which way it goes is read from the mode the tone
    /// control is on its way to ([`App::planned_mode`]) rather than from its last reading, so a
    /// quick second `f` toggles back before the device has answered the first. Asks for nothing
    /// on a device with no tone control.
    pub(super) fn flat(&mut self) -> Vec<Intent> {
        if self.on_computer() {
            return self.computer_mode(None);
        }
        if self.profile.eq.is_none() {
            return Vec::new();
        }
        let mode = match self.planned_mode() {
            Some(EqMode::Flat) => self.mode_before(),
            _ => EqMode::Flat,
        };
        self.note_mode(mode);
        let writes = vec![Intent::EqMode(mode.name().to_string())];
        self.note_pending(&writes);
        writes
    }

    /// Toggle the tone control's loudness mode, the way a click on the Loudness half but its
    /// bar does: from Flat or EQ it asks for Loudness, remembering the mode it leaves; asked again
    /// from Loudness it asks for the mode before ([`App::mode_before_loudness`]), EQ by default.
    /// Read from the mode the tone control is on its way to ([`App::planned_mode`]) rather than
    /// from its last reading, so a quick second click toggles back before the device has
    /// answered the first. Asks for nothing on a device with no tone control.
    pub(super) fn loudness(&mut self) -> Vec<Intent> {
        if self.profile.eq.is_none() || self.on_computer() {
            return Vec::new();
        }
        let mode = match self.planned_mode() {
            Some(EqMode::Loudness) => self.mode_before_loudness(),
            _ => EqMode::Loudness,
        };
        self.note_mode(mode);
        let writes = vec![Intent::EqMode(mode.name().to_string())];
        self.note_pending(&writes);
        writes
    }

    /// Put every band at 0 dB on its centre frequency and centre Q, the Flat preset's values: a
    /// flat curve, which is not the flat mode. Only the steps that differ are written, in a
    /// recall's order: the boosts come down, then the frequencies and Q move, then the cuts come
    /// up. Each step is compared with what it is on its way to be ([`App::planned_raw`]), so a
    /// flatten made while a recall or a band key is still being answered flattens what they
    /// asked for. Asks for nothing on a device with no tone control.
    pub(crate) fn flatten(&mut self) -> Vec<Intent> {
        match self.on_computer() {
            true => self.flatten_computer(),
            false => self.flatten_dac(),
        }
    }

    /// [`App::flatten`] for the device's own tone control, whichever side is shown.
    pub(crate) fn flatten_dac(&mut self) -> Vec<Intent> {
        let Some(eq) = self.profile.eq.as_ref() else {
            return Vec::new();
        };
        let plan = preset::flatten(eq, |id| self.planned_raw(id));
        let writes = presets::plan_intents(self, eq, &plan);
        self.band_edit(writes)
    }

    /// Move the Tone pane's cursor `delta` rows: its mode, its loudness, and then a row per
    /// band, in the order the pane draws them. Landing on the mode or the loudness notes which
    /// of the two it is, for `Tab` to come back to on the tone screen.
    pub(super) fn move_tone_row(&mut self, delta: isize) {
        if self.on_computer() {
            // The mode, then a row per point.
            let at = match self.tone.row {
                ToneRow::Point(point) => 1 + point,
                _ => 0,
            };
            match stepped(at, delta, 1 + self.computer_points().len()) {
                0 => self.tone.row = ToneRow::Mode,
                row => self.focus_point(row - 1),
            }
            return;
        }
        let at = match self.tone.row {
            ToneRow::Mode => 0,
            ToneRow::Loudness => 1,
            ToneRow::Band | ToneRow::Point(_) => 2 + self.tone.band,
        };
        match stepped(at, delta, 2 + self.bands()) {
            0 => self.tone.row = ToneRow::Mode,
            1 => self.tone.row = ToneRow::Loudness,
            row => {
                self.tone.row = ToneRow::Band;
                self.tone.band = row - 2;
            }
        }
        self.note_tone_header(self.tone.row);
    }

    /// Note which half of the header `row` is, for `Tab` to bring the cursor back to on the
    /// tone screen; does nothing for the band table, which is the other stop.
    pub(super) fn note_tone_header(&mut self, row: ToneRow) {
        match row {
            ToneRow::Mode => self.tone.header_loudness = false,
            ToneRow::Loudness => self.tone.header_loudness = true,
            ToneRow::Band | ToneRow::Point(_) => {}
        }
    }

    /// How many bands the tone control has, none without one.
    fn bands(&self) -> usize {
        self.profile.eq.as_ref().map_or(0, |eq| eq.bands.len())
    }

    /// Step the Tone pane's focused row one value: its mode through the modes the tone control
    /// has, its loudness one step, and a band's row by the gain its own bar shows — the same
    /// step the wheel over that row takes, and a band edit like it.
    pub(super) fn step_tone(&mut self, dir: i64) -> Vec<Intent> {
        if self.on_computer() {
            return match self.tone.row {
                ToneRow::Point(at) => self.nudge_point_gain(at, dir),
                _ => self.step_computer_mode(dir),
            };
        }
        let Some(mode_id) = self.profile.eq.as_ref().map(EqSpec::mode_id) else {
            return Vec::new();
        };
        match self.tone.row {
            ToneRow::Mode => self.step_mode(mode_id, dir),
            ToneRow::Loudness => {
                let target = self.loudness_target(dir);
                let writes = self.step_loudness(dir);
                if let Some(value) = target {
                    self.note_loudness(value);
                }
                writes
            }
            ToneRow::Band | ToneRow::Point(_) => {
                let writes = self.nudge_band(self.tone.band, dir);
                self.band_edit(writes)
            }
        }
    }

    /// Step the tone control's mode one value, the way a click on the Mode select's `‹` (`dir <
    /// 0`) or `›` (`dir > 0`) does, like every select's arrows. Asks for nothing on
    /// a device with no tone control.
    pub(crate) fn step_tone_mode(&mut self, dir: i64) -> Vec<Intent> {
        if self.on_computer() {
            return self.step_computer_mode(dir);
        }
        let Some(mode_id) = self.profile.eq.as_ref().map(EqSpec::mode_id) else {
            return Vec::new();
        };
        self.step_mode(mode_id, dir)
    }

    /// The tone-control screen's keys.
    ///
    /// Its rows are the tone control's own, as the device screen's Tone pane has them: the mode,
    /// how strong the loudness curve is, and one per band. The arrows move between them and
    /// change the one they are on — the mode through the modes there are, the loudness and a
    /// band's gain one step — while the keys this screen has of its own reach the two values a
    /// band carries beside its gain, the digits pick the band to act on, `Enter` opens the line
    /// the focused band's values are typed into, `0` puts every band back on its centre at 0 dB,
    /// `f` toggles the whole tone control's flat mode on and off and `p` opens the preset list.
    pub(super) fn tone_key(&mut self, code: KeyCode) -> Vec<Intent> {
        if code == KeyCode::Char('C') {
            return self.flip_side();
        }
        if self.on_computer()
            && let Some(intents) = self.computer_tone_key(code)
        {
            return intents;
        }
        match code {
            KeyCode::Enter => return edit_line::open(self),
            KeyCode::Char('f') => return self.flat(),
            KeyCode::Char('0') => return self.flatten(),
            KeyCode::Char('p') => return self.open_presets(),
            KeyCode::Char(digit @ '1'..='9') => {
                let band = digit as usize - '1' as usize;
                if band < self.bands() {
                    self.focus_band(band);
                }
                return Vec::new();
            }
            KeyCode::Up => return self.move_selection(-1),
            KeyCode::Down => return self.move_selection(1),
            KeyCode::Left => return self.step_tone(-1),
            KeyCode::Right => return self.step_tone(1),
            _ => {}
        }
        let Some(eq) = self.profile.eq.as_ref() else {
            return Vec::new();
        };
        let writes = match code {
            // One semitone either way, on the band the cursor is on.
            KeyCode::Char(',') => self.step_frequency(eq, -1.0 / 12.0),
            KeyCode::Char('.') => self.step_frequency(eq, 1.0 / 12.0),
            KeyCode::Char('[') => self.step_q(eq, 1.0 / Q_FACTOR),
            KeyCode::Char(']') => self.step_q(eq, Q_FACTOR),
            _ => Vec::new(),
        };
        self.band_edit(writes)
    }

    /// Move the focused band's centre frequency by `octaves`.
    fn step_frequency(&self, eq: &EqSpec, octaves: f64) -> Vec<Intent> {
        let Some(hz) = self
            .band_raw(eq, self.tone.band, EqParam::Frequency)
            .and_then(|raw| eq.frequency(self.tone.band, raw))
        else {
            return Vec::new();
        };
        let target = hz * 2f64.powf(octaves);
        if eq.frequency_raw(self.tone.band, target)
            == self.band_raw(eq, self.tone.band, EqParam::Frequency)
        {
            return Vec::new();
        }
        self.eq_set(self.tone.band, "f", target)
    }

    /// Multiply the focused band's Q by `factor`.
    fn step_q(&self, eq: &EqSpec, factor: f64) -> Vec<Intent> {
        let Some(raw) = self.band_raw(eq, self.tone.band, EqParam::Q) else {
            return Vec::new();
        };
        let target = eq.q(raw) * factor;
        if eq.q_raw(target) == raw {
            return Vec::new();
        }
        self.eq_set(self.tone.band, "q", target)
    }

    /// Nudge `band`'s gain (0-based) one raw step up or down, whatever that step is in dB on this
    /// profile, the way the wheel over its row on the tone-control screen does — the same step the
    /// left and right keys take on whichever band is focused, but reaching for `band` directly
    /// rather than `self.tone.band`. The value sent is the one that step stands for, so the device
    /// lands on it rather than on a neighbour a round number is nearer. Asks for nothing without a
    /// tone control, before `band`'s gain has a reading to step from, or at either end of the
    /// gain's steps.
    pub(crate) fn nudge_band(&self, band: usize, dir: i64) -> Vec<Intent> {
        if self.on_computer() {
            return self.nudge_point_gain(band, dir);
        }
        self.nudge_step(band, EqParam::Gain, dir)
    }

    /// Nudge `band`'s Q (0-based) one raw step, the way the wheel over the tone-control screen's
    /// curve does: up (`dir > 0`) narrows the bell and down widens it. The value sent is the one
    /// that step stands for. Asks for nothing without a tone control, before `band`'s Q has a
    /// reading to step from, or at either end of the Q's steps.
    pub(crate) fn nudge_q(&self, band: usize, dir: i64) -> Vec<Intent> {
        if self.on_computer() {
            return self.step_point_q(band, Self::q_factor(dir > 0));
        }
        self.nudge_step(band, EqParam::Q, dir)
    }

    /// Nudge `param` of `band` (0-based) one raw step up or down from the step it is on its way
    /// to, as the value that step stands for: nothing without a tone control, before there is a
    /// step to move from, or at either end of the steps.
    fn nudge_step(&self, band: usize, param: EqParam, dir: i64) -> Vec<Intent> {
        let Some(eq) = self.profile.eq.as_ref() else {
            return Vec::new();
        };
        let Some(raw) = self.band_raw(eq, band, param) else {
            return Vec::new();
        };
        let Some(next) = i64::from(raw)
            .checked_add(dir.signum())
            .filter(|next| (0..=i64::from(EqSpec::STEP_MAX)).contains(next))
            .and_then(|next| u8::try_from(next).ok())
        else {
            return Vec::new();
        };
        let value = match param {
            EqParam::Frequency => eq.frequency(band, next),
            EqParam::Gain => Some(eq.gain_db(next)),
            EqParam::Q => Some(eq.q(next)),
        };
        value.map_or_else(Vec::new, |value| self.eq_set(band, param.word(), value))
    }

    /// One `band`'s write, `band` 0-based as the app tracks it and 1-based as it is typed and
    /// sent.
    fn eq_set(&self, band: usize, param: &str, value: f64) -> Vec<Intent> {
        vec![Intent::EqSet {
            band: band + 1,
            param: param.to_string(),
            value,
        }]
    }

    /// The raw step one parameter of `band` is stepped from: the one it is on its way to
    /// ([`App::planned_raw`]), once there is a write or a reading to say.
    fn band_raw(&self, eq: &EqSpec, band: usize, param: EqParam) -> Option<u8> {
        u8::try_from(self.planned_raw(eq.band_id(band, param)?)?).ok()
    }

    /// The mode next to the one the tone control is on its way to, or nothing at either end:
    /// the mode row is a select, and it is stepped the way the Controls pane's selects are
    /// rather than spun round a ring. With no reading yet, the first press names a mode rather
    /// than guessing at one. The mode asked for is noted as the mode before Flat too
    /// ([`App::note_mode`]), and as pending so a step made right after this one — `f`'s own
    /// toggle among them — reads it back from [`App::planned_mode`] rather than waiting on the
    /// device.
    fn step_mode(&mut self, mode_id: FunctionId, dir: i64) -> Vec<Intent> {
        let current = self
            .planned_raw(mode_id)
            .and_then(|raw| u8::try_from(raw).ok())
            .and_then(EqMode::from_raw);
        let next = match (current, dir > 0) {
            (None, _) => EqMode::Flat,
            (Some(EqMode::Flat), true) => EqMode::Eq,
            (Some(EqMode::Eq), true) => EqMode::Loudness,
            (Some(EqMode::Loudness), false) => EqMode::Eq,
            (Some(EqMode::Eq), false) => EqMode::Flat,
            (Some(EqMode::Loudness), true) | (Some(EqMode::Flat), false) => return Vec::new(),
        };
        self.note_mode(next);
        let writes = vec![Intent::EqMode(next.name().to_string())];
        self.note_pending(&writes);
        writes
    }

    /// Move the loudness strength one raw step, held inside its range, from the loudness row's
    /// own arrows or from the wheel over the loudness bar wherever it is drawn. A
    /// profile that names a control on that function is set by its key; otherwise the function
    /// is set directly. Asks for nothing without a tone control at all.
    pub(crate) fn step_loudness(&self, delta: i64) -> Vec<Intent> {
        let (Some(eq), Some(value)) = (self.profile.eq.as_ref(), self.loudness_target(delta))
        else {
            return Vec::new();
        };
        vec![self.loudness_intent(eq, value)]
    }

    /// The strength a `delta` step of the loudness moves to, from the one it is on its way to
    /// ([`App::planned_raw`]): nothing without a tone control or at either end of the range.
    fn loudness_target(&self, delta: i64) -> Option<i64> {
        let eq = self.profile.eq.as_ref()?;
        let current = self.planned_raw(eq.loudness_id()).unwrap_or(0);
        let value = current.saturating_add(delta).clamp(0, LOUDNESS_MAX);
        (value != current).then_some(value)
    }

    /// The write that sets the loudness strength of the tone control `eq` to `value`: by key
    /// where the profile names a control on that function, and straight to the function where
    /// it does not.
    pub(crate) fn loudness_intent(&self, eq: &EqSpec, value: i64) -> Intent {
        let id = eq.loudness_id();
        match self.profile.control_by_id(id) {
            Some(control) => set(&control.key, value.to_string()),
            None => Intent::Raw {
                id: id.to_string(),
                value_type: ValueType::U8.to_string(),
                value: value.to_string(),
            },
        }
    }
}
