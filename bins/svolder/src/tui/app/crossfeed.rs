// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's crossfeed in the TUI: its rows, their keys and the edit line.
//!
//! The controller owns crossfeed (the daemon's, or this TUI's own when no daemon runs); this
//! side reads the view ([`CrossfeedView`]) and sends [`HostEqOp::Crossfeed`]. Every range, step,
//! preset and word comes from the view, so nothing here knows a number of any model.
//!
//! The tone screen's Crossfeed section is folded to one line until the keys are in it: the rows
//! are [`Xf`] and the section is unfolded exactly while [`ToneRow::Crossfeed`] is the focused
//! row. The device screen's Tone pane has the one [`Xf::Preset`] row, whose arrows switch
//! crossfeed on and off. A key that changes a value sends the value that step stands for, held
//! to the model's range and rounded to its step ([`Range::snap`]), and sends nothing at the
//! end of the range.

use crossterm::event::KeyCode;
use svolder_host::eq::crossfeed::{CrossfeedView, ModelView, Range};
use svolder_ipc::HostEqOp;

use super::{App, Intent, Screen, ToneRow};

/// The most characters the edit line takes.
const LINE_MAX: usize = 10;

/// One row of the Crossfeed section, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Xf {
    /// The space choice (Off, Crossfeed, Virtual speakers): the section's first row where the
    /// view carries virtual speakers.
    Space,
    /// On or off, the model and its preset: the section's own line, which is also the folded
    /// section's one line.
    Preset,
    /// The share of the other side, with its level in dB beside it.
    Level,
    /// The crossover.
    Crossover,
    /// The total lag of the other side.
    Delay,
    /// The virtual speakers' set, while they are the choice.
    Set,
}

impl Xf {
    /// Every row of the crossfeed, in the order the section draws them.
    pub const ALL: [Xf; 4] = [Xf::Preset, Xf::Level, Xf::Crossover, Xf::Delay];

    /// The row's label.
    pub fn label(self) -> &'static str {
        match self {
            Xf::Space => "Space",
            Xf::Set => "Set",
            Xf::Preset => "Crossfeed",
            Xf::Level => "Level",
            Xf::Crossover => "Crossover",
            Xf::Delay => "Delay",
        }
    }

    /// The unit its edit line shows after the number.
    pub fn unit(self) -> &'static str {
        match self {
            Xf::Space | Xf::Set | Xf::Preset => "",
            Xf::Level => "%",
            Xf::Crossover => "Hz",
            Xf::Delay => "ms",
        }
    }
}

/// The edit line of a value row, while it is open: the number typed in place of the row's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XfLine {
    /// The row it edits.
    pub row: Xf,
    /// The text as typed.
    pub text: String,
    /// Whether the next character typed replaces the text.
    pub fresh: bool,
    /// Why the last key was refused, shown in place of the line's keys until the next key.
    pub refusal: Option<String>,
}

/// A number's text: the comma a keypad sends is a decimal point.
fn number(text: &str) -> Option<f64> {
    text.trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
}

/// The text of `value` with the digits its `step` needs: `37`, `0.27`.
pub(crate) fn plain(value: f64, step: f64) -> String {
    let digits = match step {
        s if s >= 1.0 || (s - s.round()).abs() < 1e-9 => 0,
        s if s >= 0.1 => 1,
        _ => 2,
    };
    format!("{value:.digits$}")
}

impl App {
    /// The crossfeed as the controller last said it: `None` where the controller serves none.
    pub(crate) fn crossfeed(&self) -> Option<&CrossfeedView> {
        self.computer_eq().and_then(|eq| eq.crossfeed.as_ref())
    }

    /// Whether the computer side is shown and has crossfeed to show.
    pub(crate) fn crossfeed_shown(&self) -> bool {
        self.on_computer() && self.crossfeed().is_some()
    }

    /// The row of the section the keys are in, if they are in it.
    pub(crate) fn xf_row(&self) -> Option<Xf> {
        // The tone screen has all the rows; the device screen's Tone pane has the one; no other
        // screen has any.
        let here = match (self.screen, self.tone.row) {
            (Screen::Tone, ToneRow::Crossfeed(row)) => Some(row),
            (Screen::Device, ToneRow::Crossfeed(row))
                if self.pane == super::Pane::Tone && row == self.xf_first() =>
            {
                Some(row)
            }
            _ => None,
        };
        here.filter(|_| self.crossfeed_shown())
    }

    /// Whether the section is unfolded: the keys are in it, on the tone screen.
    pub(crate) fn xf_unfolded(&self) -> bool {
        self.screen == Screen::Tone && self.xf_row().is_some()
    }

    /// The view's model row for the model crossfeed is on.
    pub(crate) fn xf_model(&self) -> Option<&ModelView> {
        let view = self.crossfeed()?;
        view.models.iter().find(|m| m.word == view.model)
    }

    /// Put the keys on `row` of the section, the way a click on it does. On the tone screen that
    /// unfolds the section; on the device screen it takes the keys to the Tone pane.
    pub(crate) fn focus_crossfeed(&mut self, row: Xf) {
        if !self.crossfeed_shown() {
            return;
        }
        let from = self.cursor_id();
        if self.screen == Screen::Device {
            self.frame_flags.pane_changed |= self.pane != super::Pane::Tone;
            self.pane = super::Pane::Tone;
        }
        self.tone.xf_line = None;
        self.tone.row = ToneRow::Crossfeed(row);
        self.cursor_moved(from);
    }

    /// A click on `row` of the section (`step` 0: it is focused, and a double-click on a value
    /// opens its edit line on the tone screen), or on its `‹` or `›`, or the wheel over it
    /// (`step` -1 or 1: the row is stepped, the focus stays where it is).
    pub(crate) fn click_crossfeed(&mut self, row: Xf, step: i64) -> Vec<Intent> {
        if step != 0 {
            return self.xf_arrow_any(row, step);
        }
        let was = self.xf_row() == Some(row);
        self.focus_crossfeed(row);
        if was && self.pointer.double_click && self.screen == Screen::Tone {
            // The choice and the set open the set list; a value opens its line.
            if matches!(row, Xf::Space | Xf::Set) {
                return self.open_sets();
            }
            self.open_xf_line(row);
        }
        Vec::new()
    }

    /// [`App::xf_arrow`] for a row the keys may not be on.
    fn xf_arrow_any(&mut self, row: Xf, dir: i64) -> Vec<Intent> {
        match self.crossfeed_shown() {
            true => self.xf_arrow(row, dir),
            false => Vec::new(),
        }
    }

    /// `X`: the section's first row, on the tone screen, from wherever the keys are; from
    /// inside the unfolded section it folds it.
    pub(crate) fn jump_to_crossfeed(&mut self) -> Vec<Intent> {
        if !self.crossfeed_shown() {
            return Vec::new();
        }
        // Pressed with the keys already in the unfolded section, it folds it.
        if self.xf_unfolded() {
            self.leave_crossfeed();
            return Vec::new();
        }
        if self.screen != Screen::Tone {
            self.show(Screen::Tone);
        }
        self.focus_crossfeed(self.xf_first());
        self.frame_flags.pane_changed = true;
        Vec::new()
    }

    /// One change of the crossfeed, with the view's refusal in words when it cannot be changed.
    fn xf_send(&mut self, op: HostEqOp) -> Vec<Intent> {
        if let Some(why) = self.crossfeed().and_then(|view| view.unavailable.clone()) {
            self.warn(why);
            return Vec::new();
        }
        vec![Intent::eq(op)]
    }

    /// The op that only names these fields; `done` ends a change that stands.
    fn xf_op(
        on: Option<bool>,
        model: Option<String>,
        preset: Option<String>,
        values: [Option<f64>; 3],
    ) -> HostEqOp {
        HostEqOp::Crossfeed {
            on,
            model,
            preset,
            amount_pct: values[0],
            crossover_hz: values[1],
            delay_ms: values[2],
            done: true,
        }
    }

    /// Turn crossfeed on or off, or the other way from now when `on` is `None`.
    pub(crate) fn xf_toggle(&mut self, on: Option<bool>) -> Vec<Intent> {
        let Some(view) = self.crossfeed() else {
            return Vec::new();
        };
        let on = on.unwrap_or(!view.on);
        if on == view.on {
            return Vec::new();
        }
        self.xf_send(Self::xf_op(Some(on), None, None, [None; 3]))
    }

    /// The preset `dir` steps from the one crossfeed is on, within its model: nothing at either
    /// end. From Custom the first step goes to the nearest end of the list in that direction.
    pub(crate) fn xf_preset(&mut self, dir: i64) -> Vec<Intent> {
        let (Some(view), Some(model)) = (self.crossfeed(), self.xf_model()) else {
            return Vec::new();
        };
        let count = model.presets.len();
        let at = view
            .preset
            .as_ref()
            .and_then(|name| model.presets.iter().position(|p| &p.name == name));
        let next = match (at, dir > 0) {
            (Some(at), true) => at + 1,
            (Some(at), false) => at.wrapping_sub(1),
            (None, true) => 0,
            (None, false) => count.wrapping_sub(1),
        };
        let Some(preset) = model.presets.get(next).map(|p| p.name.clone()) else {
            return Vec::new();
        };
        self.xf_send(Self::xf_op(None, None, Some(preset), [None; 3]))
    }

    /// The model `dir` steps from the one crossfeed is on, each loading its default preset:
    /// nothing at either end.
    pub(crate) fn xf_model_step(&mut self, dir: i64) -> Vec<Intent> {
        let Some(view) = self.crossfeed() else {
            return Vec::new();
        };
        let Some(at) = view.models.iter().position(|m| m.word == view.model) else {
            return Vec::new();
        };
        let next = match dir > 0 {
            true => at + 1,
            false => at.wrapping_sub(1),
        };
        let Some(word) = view.models.get(next).map(|m| m.word.clone()) else {
            return Vec::new();
        };
        self.xf_send(Self::xf_op(None, Some(word), None, [None; 3]))
    }

    /// The range of a value row and the number it stands at, from the view.
    fn xf_range(&self, row: Xf) -> Option<(Range, f64)> {
        let view = self.crossfeed()?;
        let model = self.xf_model()?;
        match row {
            Xf::Space | Xf::Set | Xf::Preset => None,
            Xf::Level => Some((model.amount_pct, view.amount_pct)),
            Xf::Crossover => Some((model.crossover_hz, view.crossover_hz)),
            Xf::Delay => model.delay_settable.then_some((
                Range {
                    min: view.delay_floor_ms,
                    max: model.delay_max_ms,
                    step: model.delay_step_ms,
                },
                view.delay_ms,
            )),
        }
    }

    /// The write that sets value row `row` to `value`.
    fn xf_set(&mut self, row: Xf, value: f64) -> Vec<Intent> {
        let mut values = [None; 3];
        match row {
            Xf::Space | Xf::Set | Xf::Preset => return Vec::new(),
            Xf::Level => values[0] = Some(value),
            Xf::Crossover => values[1] = Some(value),
            Xf::Delay => values[2] = Some(value),
        }
        self.xf_send(Self::xf_op(None, None, None, values))
    }

    /// Step a value row one of the view's steps, held to its range: nothing at either end. A
    /// delay stepped down to the model's own goes back to it.
    pub(crate) fn xf_value_step(&mut self, row: Xf, dir: i64) -> Vec<Intent> {
        let Some((range, now)) = self.xf_range(row) else {
            if row == Xf::Delay && self.crossfeed().is_some() {
                self.warn("this model plays no delay");
            }
            return Vec::new();
        };
        let next = range.snap(now + range.step * dir.signum() as f64);
        if (next - now).abs() < range.step / 2.0 {
            return Vec::new();
        }
        self.xf_set(row, next)
    }

    /// What the arrows do on `row`: the Preset row's are the preset on the tone screen and on/off
    /// on the device screen's pane; a value row's step it.
    pub(crate) fn xf_arrow(&mut self, row: Xf, dir: i64) -> Vec<Intent> {
        match (row, self.screen) {
            (Xf::Space, _) => self.space_step(dir),
            (Xf::Set, _) => self.set_step(dir),
            (Xf::Preset, Screen::Tone) => self.xf_preset(dir),
            (Xf::Preset, _) => self.xf_toggle(Some(dir > 0)),
            (row, _) => self.xf_value_step(row, dir),
        }
    }

    /// The keys of the section while they are in it. `None` for any other key.
    pub(crate) fn xf_key(&mut self, code: KeyCode) -> Option<Vec<Intent>> {
        let row = self.xf_row()?;
        if self.tone.xf_line.is_some() {
            return Some(self.xf_line_key(code));
        }
        let tone = self.screen == Screen::Tone;
        // The choice and the set: the arrows and `, .` step them, `␣` turns the space off or on,
        // `Enter` opens the set list (on the device screen it first goes to the tone screen).
        if matches!(row, Xf::Space | Xf::Set) {
            return Some(match code {
                KeyCode::Left | KeyCode::Char(',') => self.xf_arrow(row, -1),
                KeyCode::Right | KeyCode::Char('.') => self.xf_arrow(row, 1),
                KeyCode::Char(' ') => self.space_toggle(),
                KeyCode::Enter if !tone => {
                    self.show(Screen::Tone);
                    self.focus_crossfeed(row);
                    Vec::new()
                }
                KeyCode::Enter => self.open_sets(),
                KeyCode::Esc if tone => {
                    self.leave_crossfeed();
                    Vec::new()
                }
                _ => return None,
            });
        }
        Some(match code {
            KeyCode::Left => self.xf_arrow(row, -1),
            KeyCode::Right => self.xf_arrow(row, 1),
            KeyCode::Char(' ') => self.xf_toggle(None),
            KeyCode::Char(c @ (',' | '.')) => {
                let dir = if c == ',' { -1 } else { 1 };
                match (row, tone) {
                    (Xf::Preset, true) => self.xf_model_step(dir),
                    (Xf::Preset, false) => self.xf_preset(dir),
                    (row, _) => self.xf_value_step(row, dir),
                }
            }
            KeyCode::Enter if !tone => {
                self.show(Screen::Tone);
                self.focus_crossfeed(row);
                Vec::new()
            }
            // Enter on the model and preset row has no number to type: it turns it on or off.
            KeyCode::Enter if row == Xf::Preset => self.xf_toggle(None),
            KeyCode::Enter => {
                self.open_xf_line(row);
                Vec::new()
            }
            KeyCode::Esc if tone => {
                self.leave_crossfeed();
                Vec::new()
            }
            _ => return None,
        })
    }

    /// Fold the section: the keys go back to the Mode row, where `Tab` leaves the header.
    pub(crate) fn leave_crossfeed(&mut self) {
        self.tone.xf_line = None;
        self.tone.row = ToneRow::Mode;
        self.frame_flags.pane_changed = true;
    }

    /// Open the edit line on value row `row`, on the number it stands at.
    pub(crate) fn open_xf_line(&mut self, row: Xf) {
        let Some((range, now)) = self.xf_range(row) else {
            if row == Xf::Delay && self.crossfeed().is_some() {
                self.warn("this model plays no delay");
            }
            return;
        };
        self.focus_crossfeed(row);
        self.tone.xf_line = Some(XfLine {
            row,
            text: plain(now, range.step),
            fresh: true,
            refusal: None,
        });
    }

    /// What a key does while the line is open.
    fn xf_line_key(&mut self, code: KeyCode) -> Vec<Intent> {
        let Some(line) = self.tone.xf_line.as_mut() else {
            return Vec::new();
        };
        line.refusal = None;
        match code {
            KeyCode::Esc => self.tone.xf_line = None,
            KeyCode::Enter => return self.write_xf_line(),
            KeyCode::Backspace => {
                if std::mem::take(&mut line.fresh) {
                    line.text.clear();
                } else {
                    line.text.pop();
                }
            }
            KeyCode::Char(c) if c.is_ascii_digit() || matches!(c, '.' | ',') => {
                if std::mem::take(&mut line.fresh) {
                    line.text.clear();
                }
                match line.text.chars().count() >= LINE_MAX {
                    true => line.refusal = Some(format!("at most {LINE_MAX} characters")),
                    false => line.text.push(c),
                }
            }
            KeyCode::Char(_) => line.refusal = Some("only digits and one . or ,".to_string()),
            _ => {}
        }
        Vec::new()
    }

    /// `Enter`: write the typed number, which the controller holds to the model's range.
    fn write_xf_line(&mut self) -> Vec<Intent> {
        let Some(line) = self.tone.xf_line.clone() else {
            return Vec::new();
        };
        let Some(value) = number(&line.text) else {
            if let Some(open) = self.tone.xf_line.as_mut() {
                open.refusal = Some("not a number".to_string());
            }
            return Vec::new();
        };
        self.tone.xf_line = None;
        let same = self
            .xf_range(line.row)
            .is_some_and(|(range, now)| plain(now, range.step) == line.text);
        match same {
            true => Vec::new(),
            false => self.xf_set(line.row, value),
        }
    }

    /// The rows the Tone pane and the tone screen move through on the computer side, in order:
    /// the Crossfeed rows go between the mode and the points. On the device screen only the
    /// pane's one row is a stop, and it sits under the points.
    pub(crate) fn xf_stops(&self) -> Vec<Xf> {
        let speakers = self.space_shown() && self.space_now() == super::Space::Speakers;
        match (self.crossfeed_shown(), self.screen, self.space_shown()) {
            (false, ..) => Vec::new(),
            (true, Screen::Tone, false) => Xf::ALL.to_vec(),
            (true, Screen::Tone, true) if speakers => vec![Xf::Space, Xf::Set],
            (true, Screen::Tone, true) => [&[Xf::Space][..], &Xf::ALL].concat(),
            (true, _, true) => vec![Xf::Space],
            (true, _, false) => vec![Xf::Preset],
        }
    }
}

/// A number with a true minus sign.
fn signed(text: String) -> String {
    text.replace('-', "\u{2212}")
}

/// The model and preset in words: `Bauer · Chu Moy`, `Bauer · Custom`.
pub(crate) fn words_of(view: &CrossfeedView) -> String {
    let model = view
        .models
        .iter()
        .find(|m| m.word == view.model)
        .map_or(view.model.as_str(), |m| m.name.as_str());
    let preset = view.preset.as_deref().unwrap_or("Custom");
    format!("{model} \u{b7} {preset}")
}

/// The level with its dB beside it: `37 % · −8.6 dB`.
pub(crate) fn amount_text(view: &CrossfeedView) -> String {
    signed(format!(
        "{:.0} % \u{b7} {:.1} dB",
        view.amount_pct, view.amount_db
    ))
}

/// The crossover: `700 Hz`.
pub(crate) fn crossover_text(view: &CrossfeedView) -> String {
    format!("{:.0} Hz", view.crossover_hz)
}

/// The delay: `0.27 ms`, with `(own)` while it is the model's own, `none` for a model that plays
/// none.
pub(crate) fn delay_text(view: &CrossfeedView) -> String {
    match (view.delay_settable, view.delay_own) {
        (false, _) => "none".to_string(),
        (true, true) => format!("{:.2} ms (own)", view.delay_ms),
        (true, false) => format!("{:.2} ms", view.delay_ms),
    }
}

/// The numbers in one line: `37 % · −8.6 dB · 700 Hz · 0.27 ms`, `no delay` for a model with none.
pub(crate) fn summary_text(view: &CrossfeedView) -> String {
    let delay = match view.delay_settable {
        true => format!("{:.2} ms", view.delay_ms),
        false => "no delay".to_string(),
    };
    format!(
        "{} \u{b7} {} \u{b7} {delay}",
        amount_text(view),
        crossover_text(view)
    )
}

#[cfg(test)]
mod tests;
