// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's crossfeed in the TUI: its rows, their keys and the edit line.
//!
//! The controller owns crossfeed (the daemon's, or this TUI's own when no daemon runs); this
//! side reads the view ([`CrossfeedView`]) and sends [`HostEqOp::Crossfeed`]. Every range, step,
//! preset and word comes from the view, so nothing here knows a number of any model.
//!
//! The tone screen's Space panel is these rows ([`Xf`], [`ToneRow::Crossfeed`] while the keys are
//! in one) and the stage keys of [`stages`](super::stages) reach them. The device screen's Tone
//! pane has the Plays row, and under it the Model and Preset rows while crossfeed plays. A key
//! that changes a value sends the value that step stands for, held to the model's range and
//! rounded to its step ([`Range::snap`]), and sends nothing at the end of the range.

use crossterm::event::KeyCode;
use svolder_host::eq::crossfeed::{CrossfeedView, ModelView, Range};
use svolder_ipc::HostEqOp;

use super::{App, Intent, Ld, Screen, Space, ToneRow};

/// The most characters the edit line takes.
const LINE_MAX: usize = 10;

/// One row of the Space panel, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Xf {
    /// What plays (Off, Crossfeed, Virtual speakers) where the view carries virtual speakers;
    /// crossfeed's own on/off row where it does not. The panel's first row either way.
    Space,
    /// The crossfeed model: Bauer, Chu Moy and the rest.
    Model,
    /// The model's preset.
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
    pub const ALL: [Xf; 5] = [Xf::Model, Xf::Preset, Xf::Level, Xf::Crossover, Xf::Delay];

    /// The row's label.
    pub fn label(self) -> &'static str {
        match self {
            Xf::Space => "Plays",
            Xf::Set => "Set",
            Xf::Model => "Model",
            Xf::Preset => "Preset",
            Xf::Level => "Level",
            Xf::Crossover => "Crossover",
            Xf::Delay => "Delay",
        }
    }

    /// The unit its edit line shows after the number.
    pub fn unit(self) -> &'static str {
        match self {
            Xf::Space | Xf::Set | Xf::Model | Xf::Preset => "",
            Xf::Level => "%",
            Xf::Crossover => "Hz",
            Xf::Delay => "ms",
        }
    }
}

/// The row an edit line is typed into: a Space panel row or a Loudness panel row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineRow {
    /// A value row of the Space panel.
    Xf(Xf),
    /// A value row of the Loudness panel.
    Ld(Ld),
}

impl LineRow {
    /// The unit its edit line shows after the number.
    pub fn unit(self) -> &'static str {
        match self {
            LineRow::Xf(row) => row.unit(),
            LineRow::Ld(_) => "%",
        }
    }
}

/// The edit line of a value row, while it is open: the number typed in place of the row's value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XfLine {
    /// The row it edits.
    pub row: LineRow,
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
        // The tone screen has all the rows; the device screen's Tone pane has its own few; no
        // other screen has any.
        let here = match (self.screen, self.tone.row) {
            (Screen::Tone, ToneRow::Crossfeed(row)) => Some(row),
            (Screen::Device, ToneRow::Crossfeed(row))
                if self.pane == super::Pane::Tone && self.xf_stops().contains(&row) =>
            {
                Some(row)
            }
            _ => None,
        };
        here.filter(|_| self.crossfeed_shown())
    }

    /// The view's model row for the model crossfeed is on.
    pub(crate) fn xf_model(&self) -> Option<&ModelView> {
        let view = self.crossfeed()?;
        view.models.iter().find(|m| m.word == view.model)
    }

    /// Put the keys on `row` of the section, the way a click on it does. On the tone screen that
    /// opens the Space panel on it; on the device screen it takes the keys to the Tone pane.
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
        self.tone.plays = None;
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

    /// `X`: the Space panel, on the tone screen, from wherever the keys are; pressed in the
    /// panel, back to the strip.
    pub(crate) fn jump_to_crossfeed(&mut self) -> Vec<Intent> {
        if !self.crossfeed_shown() {
            return Vec::new();
        }
        self.jump_to_stage(super::Stage::Space);
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

    /// The preset `dir` steps from the one crossfeed is on, within its model, round the ends: a
    /// model of one preset steps to itself, which sends nothing. From Custom the first step goes
    /// to the nearest end of the list in that direction.
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
            (Some(at), true) => (at + 1) % count.max(1),
            (Some(at), false) => (at + count.max(1) - 1) % count.max(1),
            (None, true) => 0,
            (None, false) => count.wrapping_sub(1),
        };
        if at == Some(next) {
            return Vec::new();
        }
        let Some(preset) = model.presets.get(next).map(|p| p.name.clone()) else {
            return Vec::new();
        };
        self.xf_send(Self::xf_op(None, None, Some(preset), [None; 3]))
    }

    /// The model `dir` steps from the one crossfeed is on, round the ends, each loading its
    /// default preset.
    pub(crate) fn xf_model_step(&mut self, dir: i64) -> Vec<Intent> {
        let Some(view) = self.crossfeed() else {
            return Vec::new();
        };
        let count = view.models.len();
        let Some(at) = view.models.iter().position(|m| m.word == view.model) else {
            return Vec::new();
        };
        let next = match dir > 0 {
            true => (at + 1) % count,
            false => (at + count - 1) % count,
        };
        if next == at {
            return Vec::new();
        }
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
            Xf::Space | Xf::Set | Xf::Model | Xf::Preset => None,
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
            Xf::Space | Xf::Set | Xf::Model | Xf::Preset => return Vec::new(),
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

    /// What the arrows do on `row` of the device pane's rows or a click on the tone screen's:
    /// the choice and the set step, the model and the preset step round, a value steps.
    pub(crate) fn xf_arrow(&mut self, row: Xf, dir: i64) -> Vec<Intent> {
        match row {
            Xf::Space if !self.space_shown() => self.xf_toggle(Some(dir > 0)),
            // On the choice the arrows only highlight; `Enter` plays the highlighted one.
            Xf::Space => {
                self.plays_move(dir);
                Vec::new()
            }
            Xf::Set => self.set_step(dir),
            Xf::Model => self.xf_model_step(dir),
            Xf::Preset => self.xf_preset(dir),
            row => self.xf_value_step(row, dir),
        }
    }

    /// The keys of the device pane's Space rows while the keys are in them, and of an open edit
    /// line. `None` for any other key; the tone screen's panel has its own
    /// ([`stages`](super::stages)).
    pub(crate) fn xf_key(&mut self, code: KeyCode) -> Option<Vec<Intent>> {
        let row = self.xf_row()?;
        if self.tone.xf_line.is_some() {
            return Some(self.xf_line_key(code));
        }
        if self.screen == Screen::Tone {
            return None;
        }
        Some(match code {
            // `,` and `.` never change the crossfeed model: only `←→` do.
            KeyCode::Char(',' | '.') if row == Xf::Model => Vec::new(),
            KeyCode::Left | KeyCode::Char(',') => self.xf_arrow(row, -1),
            KeyCode::Right | KeyCode::Char('.') => self.xf_arrow(row, 1),
            KeyCode::Char(' ') => match row {
                Xf::Space | Xf::Set if self.space_shown() => self.space_toggle(),
                _ => self.xf_toggle(None),
            },
            // Plays with a highlight not yet played: `Enter` plays it.
            KeyCode::Enter
                if row == Xf::Space
                    && self.space_shown()
                    && self.plays_highlight() != self.space_now() =>
            {
                self.plays_apply()
            }
            // The panel on the tone screen has the row's own keys.
            KeyCode::Enter => {
                self.show(Screen::Tone);
                self.focus_crossfeed(row);
                Vec::new()
            }
            _ => return None,
        })
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
            row: LineRow::Xf(row),
            text: plain(now, range.step),
            fresh: true,
            refusal: None,
        });
    }

    /// Open the edit line on value row `row` of the Loudness panel, on the number it stands at.
    pub(crate) fn open_ld_line(&mut self, row: Ld) {
        let Some((range, now)) = self.ld_range(row) else {
            return;
        };
        self.focus_ld(row);
        self.tone.xf_line = Some(XfLine {
            row: LineRow::Ld(row),
            text: plain(now, range.step),
            fresh: true,
            refusal: None,
        });
    }

    /// Whether the row an open edit line types into is the one the keys are on.
    pub(crate) fn line_row_held(&self) -> bool {
        match self.tone.xf_line.as_ref().map(|line| line.row) {
            Some(LineRow::Xf(row)) => self.xf_row() == Some(row),
            Some(LineRow::Ld(row)) => self.ld_row() == Some(row),
            None => false,
        }
    }

    /// What a key does while the line is open.
    pub(crate) fn xf_line_key(&mut self, code: KeyCode) -> Vec<Intent> {
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

    /// `Enter`: write the typed number, which the controller holds to the row's range.
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
        match line.row {
            LineRow::Xf(row) => {
                let same = self
                    .xf_range(row)
                    .is_some_and(|(range, now)| plain(now, range.step) == line.text);
                match same {
                    true => Vec::new(),
                    false => self.xf_set(row, value),
                }
            }
            LineRow::Ld(row) => {
                let same = self
                    .ld_range(row)
                    .is_some_and(|(range, now)| plain(now, range.step) == line.text);
                match same {
                    true => Vec::new(),
                    false => self.ld_set(row, value),
                }
            }
        }
    }

    /// The rows the Space panel and the device pane move through on the computer side, in order.
    /// The panel on the tone screen has every row of what plays; the device pane has the Plays
    /// row, and the Model and Preset rows under it while crossfeed plays.
    pub(crate) fn xf_stops(&self) -> Vec<Xf> {
        if !self.crossfeed_shown() {
            return Vec::new();
        }
        let speakers = self.space_shown() && self.space_now() == Space::Speakers;
        match (self.screen, speakers) {
            (Screen::Tone, true) => vec![Xf::Space, Xf::Set],
            (Screen::Tone, false) => [&[Xf::Space][..], &Xf::ALL].concat(),
            (_, true) => vec![Xf::Space],
            (_, false) if self.space_now() == Space::Crossfeed => {
                vec![Xf::Space, Xf::Model, Xf::Preset]
            }
            (_, false) => vec![Xf::Space],
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

/// The delay: `0.27 ms`, with `the model's own` while it is the model's own, `none` for a model
/// that plays none.
pub(crate) fn delay_text(view: &CrossfeedView) -> String {
    match (view.delay_settable, view.delay_own) {
        (false, _) => "none".to_string(),
        (true, true) => format!("{:.2} ms \u{b7} the model's own", view.delay_ms),
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
