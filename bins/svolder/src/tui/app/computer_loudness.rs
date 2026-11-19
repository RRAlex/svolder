// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's loudness in the TUI: its rows and their keys.
//!
//! The controller owns loudness (the daemon's, or this TUI's own when no daemon runs); this side
//! reads the view ([`LoudnessView`]) and sends [`HostEqOp::Loudness`]. Every number the screens
//! show (strength, reference, volume, depth, the shelf gains, the ranges and steps) comes from the
//! view, so nothing here works one out. The section is absent while the view is.
//!
//! The device screen's Tone pane has the one [`Ld::Strength`] row. The tone screen folds the
//! section to that row and its dim line, and unfolds it exactly while [`ToneRow::Ld`] is the
//! focused row: the [`Ld::Reference`] row and the readout come under the strength row. A key
//! that changes a value sends the value that step stands for, held to the view's range and
//! rounded to its step, and sends nothing at the end of the range.

use crossterm::event::KeyCode;
use svolder_host::eq::loudness::LoudnessView;
use svolder_ipc::HostEqOp;
use svolder_profiles::EqMode;

use super::{App, Intent, Screen, ToneRow};

/// One row of the Loudness section, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ld {
    /// On or off with the strength: `Loudness ‹ on › ████░░ 50 %`.
    Strength,
    /// The volume at and above which the stage is flat.
    Reference,
}

impl Ld {
    /// The rows of the unfolded section, in the order it draws them.
    pub const ALL: [Ld; 2] = [Ld::Strength, Ld::Reference];

    /// The row's label.
    pub fn label(self) -> &'static str {
        match self {
            Ld::Strength => "Loudness",
            Ld::Reference => "Reference",
        }
    }
}

/// A minus sign where a number has a hyphen.
fn signed(text: String) -> String {
    text.replace('-', "\u{2212}")
}

/// A dB value with its sign, a true minus: `+7.3`, `−7.3`, `0.0`.
fn db(value: f64) -> String {
    match value.abs() < 0.05 {
        true => "0.0".to_string(),
        false => signed(format!("{value:+.1}")),
    }
}

/// The strength as the bar's value text: `50 %`.
pub(crate) fn strength_text(view: &LoudnessView) -> String {
    format!("{:.0} %", view.strength_pct)
}

/// The reference as a number: `100 %`.
pub(crate) fn reference_text(view: &LoudnessView) -> String {
    format!("{:.0} %", view.reference_pct)
}

/// The numbers of the readout, as the view's shelf gains say them: `bass +7.3 · mids −7.3 ·
/// treble +2.1 dB`.
pub(crate) fn gains_text(view: &LoudnessView) -> String {
    format!(
        "bass {} \u{b7} mids {} \u{b7} treble {} dB",
        db(view.lift_db),
        db(view.mids_db),
        db(view.treble_db)
    )
}

/// What a line says while the device's own loudness plays too.
pub(crate) const DAC_HINT: &str = "the DAC's loudness is on too";

/// The readout that fits in `room` cells, the longest form first: what the stage does at the
/// volume the view read, or why it does nothing. Every number is the view's. The forms are
/// `ref 100 % · 35 %: bass +7.3 · mids −7.3 dB`, `35 %: bass … · mids … dB` and `bass … · mids …
/// dB`; `treble` puts the treble gain after the mids in the longest ones, and `dac_on` a first
/// form that ends with [`DAC_HINT`].
pub(crate) fn readout(view: &LoudnessView, dac_on: bool, treble: bool, room: usize) -> String {
    let cells = crate::tui::widgets::text::cells;
    let mut options: Vec<String> = Vec::new();
    match (&view.unavailable, view.on) {
        (Some(why), _) => options.push(format!("unavailable: {why}")),
        (None, false) => {
            if dac_on {
                options.push(format!("off \u{b7} {DAC_HINT}"));
            }
            options.push("off: the sound plays as the EQ made it".to_string());
            options.push("off".to_string());
        }
        (None, true) if !view.following => {
            let why = view
                .note
                .clone()
                .unwrap_or_else(|| "waiting for the volume".to_string());
            options.push(why);
        }
        (None, true) => {
            let volume = view
                .volume_pct
                .map(|v| format!("{v:.0} %"))
                .unwrap_or_else(|| "\u{2014}".to_string());
            let reference = format!("ref {:.0} %", view.reference_pct);
            let flat = view.depth_db <= 0.0;
            let gains = |with_treble: bool| match (flat, with_treble) {
                (true, _) => "flat".to_string(),
                (false, false) => format!(
                    "bass {} \u{b7} mids {} dB",
                    db(view.lift_db),
                    db(view.mids_db)
                ),
                (false, true) => gains_text(view),
            };
            let full = format!("{reference} \u{b7} {volume}: {}", gains(false));
            if treble {
                let long = format!("{reference} \u{b7} {volume}: {}", gains(true));
                if dac_on {
                    options.push(format!("{long} \u{b7} {DAC_HINT}"));
                }
                options.push(long);
            }
            if dac_on {
                options.push(format!("{full} \u{b7} {DAC_HINT}"));
            }
            options.extend([full, format!("{volume}: {}", gains(false)), gains(false)]);
        }
    }
    options
        .iter()
        .find(|text| cells(text) <= room)
        .or(options.last())
        .map(|text| crate::tui::widgets::text::cut_to_cells(text, room))
        .unwrap_or_default()
}

/// How far along a bar drawn across `bar` the middle of the cell at column `col` is, from 0 to 1:
/// a column left of the bar is 0 and one right of it 1.
pub(crate) fn bar_fraction(bar: ratatui::layout::Rect, col: u16) -> f64 {
    if bar.width == 0 || col < bar.x {
        return 0.0;
    }
    let at = f64::from(col - bar.x);
    ((2.0 * at + 1.0) / (2.0 * f64::from(bar.width))).clamp(0.0, 1.0)
}

impl App {
    /// The loudness as the controller last said it: `None` where the controller serves none.
    pub(crate) fn loudness_view(&self) -> Option<&LoudnessView> {
        self.computer_eq().and_then(|eq| eq.loudness.as_ref())
    }

    /// Whether the computer side is shown and has loudness to show.
    pub(crate) fn ld_shown(&self) -> bool {
        self.on_computer() && self.loudness_view().is_some()
    }

    /// Whether the device's own loudness mode is on: the hint that both play.
    pub(crate) fn dac_loudness_on(&self) -> bool {
        self.eq_mode() == Some(EqMode::Loudness)
    }

    /// The row of the section the keys are in, if they are in it.
    pub(crate) fn ld_row(&self) -> Option<Ld> {
        let here = match (self.screen, self.tone.row) {
            (Screen::Tone, ToneRow::Ld(row)) => Some(row),
            (Screen::Device, ToneRow::Ld(Ld::Strength)) if self.pane == super::Pane::Tone => {
                Some(Ld::Strength)
            }
            _ => None,
        };
        here.filter(|_| self.ld_shown())
    }

    /// Whether the section is unfolded: the keys are in it, on the tone screen.
    pub(crate) fn ld_unfolded(&self) -> bool {
        self.screen == Screen::Tone && self.ld_row().is_some()
    }

    /// Put the keys on `row` of the section, the way a click on it does.
    pub(crate) fn focus_ld(&mut self, row: Ld) {
        if !self.ld_shown() {
            return;
        }
        let from = self.cursor_id();
        if self.screen == Screen::Device {
            self.frame_flags.pane_changed |= self.pane != super::Pane::Tone;
            self.pane = super::Pane::Tone;
        }
        self.tone.xf_line = None;
        self.tone.row = ToneRow::Ld(row);
        self.cursor_moved(from);
    }

    /// A click on `row` of the section (`step` 0: it is focused), or on its `‹` or `›`, or the
    /// wheel over it (`step` -1 or 1: the row is stepped, the focus stays where it is).
    pub(crate) fn click_ld(&mut self, row: Ld, step: i64) -> Vec<Intent> {
        if !self.ld_shown() {
            return Vec::new();
        }
        match step {
            0 => {
                self.focus_ld(row);
                Vec::new()
            }
            dir => self.ld_arrow(row, dir),
        }
    }

    /// The strength the bar's cell `fraction` of the way along (0 to 1, past either end the
    /// end) stands for, held to the view's range and step.
    pub(crate) fn ld_strength_at(&self, fraction: f64) -> Option<f64> {
        let range = self.loudness_view()?.ranges.strength_pct;
        Some(range.snap(range.min + (range.max - range.min) * fraction.clamp(0.0, 1.0)))
    }

    /// The strength the view says it is at.
    pub(crate) fn ld_strength_now(&self) -> Option<f64> {
        self.loudness_view().map(|view| view.strength_pct)
    }

    /// One strength set from the bar: `done` false while the pointer is still dragging it (the
    /// value is heard, not saved), true for the last one.
    pub(crate) fn ld_strength_drag(&mut self, value: f64, done: bool) -> Vec<Intent> {
        self.ld_send(HostEqOp::Loudness {
            on: None,
            strength_pct: Some(value),
            reference_pct: None,
            reference_now: None,
            done,
        })
    }

    /// One change of the loudness. The view's `unavailable` note is shown but does not stop it: the
    /// controller saves the change and applies it once loudness can follow the volume again.
    fn ld_send(&mut self, op: HostEqOp) -> Vec<Intent> {
        vec![Intent::eq(op)]
    }

    /// The op that only names these fields.
    fn ld_op(
        on: Option<bool>,
        strength_pct: Option<f64>,
        reference_pct: Option<f64>,
        reference_now: bool,
    ) -> HostEqOp {
        HostEqOp::Loudness {
            on,
            strength_pct,
            reference_pct,
            reference_now: reference_now.then_some(true),
            done: true,
        }
    }

    /// Turn loudness on or off, or the other way from now when `on` is `None`.
    pub(crate) fn ld_toggle(&mut self, on: Option<bool>) -> Vec<Intent> {
        let Some(view) = self.loudness_view() else {
            return Vec::new();
        };
        let on = on.unwrap_or(!view.on);
        if on == view.on {
            return Vec::new();
        }
        self.ld_send(Self::ld_op(Some(on), None, None, false))
    }

    /// Step the strength or the reference one of the view's steps: nothing at either end.
    pub(crate) fn ld_value_step(&mut self, row: Ld, dir: i64) -> Vec<Intent> {
        let Some(view) = self.loudness_view() else {
            return Vec::new();
        };
        let (range, now) = match row {
            Ld::Strength => (view.ranges.strength_pct, view.strength_pct),
            Ld::Reference => (view.ranges.reference_pct, view.reference_pct),
        };
        let next = range.snap(now + range.step * dir.signum() as f64);
        if (next - now).abs() < range.step / 2.0 {
            return Vec::new();
        }
        match row {
            Ld::Strength => self.ld_send(Self::ld_op(None, Some(next), None, false)),
            Ld::Reference => self.ld_send(Self::ld_op(None, None, Some(next), false)),
        }
    }

    /// `r`: the reference is the computer's volume now, as the view read it.
    pub(crate) fn ld_reference_now(&mut self) -> Vec<Intent> {
        match self.loudness_view().map(|view| view.volume_pct.is_some()) {
            None => Vec::new(),
            Some(false) => {
                self.warn("the computer's volume has not been read yet");
                Vec::new()
            }
            Some(true) => self.ld_send(Self::ld_op(None, None, None, true)),
        }
    }

    /// What the arrows do on `row`: step its value.
    pub(crate) fn ld_arrow(&mut self, row: Ld, dir: i64) -> Vec<Intent> {
        self.ld_value_step(row, dir)
    }

    /// The keys of the section while they are in it. `None` for any other key.
    pub(crate) fn ld_key(&mut self, code: KeyCode) -> Option<Vec<Intent>> {
        let row = self.ld_row()?;
        let tone = self.screen == Screen::Tone;
        Some(match code {
            KeyCode::Left | KeyCode::Char(',') => self.ld_arrow(row, -1),
            KeyCode::Right | KeyCode::Char('.') => self.ld_arrow(row, 1),
            KeyCode::Char(' ') | KeyCode::Enter => self.ld_toggle(None),
            KeyCode::Char('r') => self.ld_reference_now(),
            KeyCode::Esc if tone => {
                self.leave_ld();
                Vec::new()
            }
            _ => return None,
        })
    }

    /// Fold the section: the keys go back to the Mode row.
    pub(crate) fn leave_ld(&mut self) {
        self.tone.row = ToneRow::Mode;
        self.frame_flags.pane_changed = true;
    }

    /// The rows the Tone pane and the tone screen move through on the computer side, in order.
    /// The device screen's pane has only the strength row as a stop.
    pub(crate) fn ld_stops(&self) -> Vec<Ld> {
        match (self.ld_shown(), self.screen) {
            (false, _) => Vec::new(),
            (true, Screen::Tone) => Ld::ALL.to_vec(),
            (true, _) => vec![Ld::Strength],
        }
    }
}

#[cfg(test)]
mod tests;
