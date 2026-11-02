// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The edit line: one band's frequency, gain and Q typed as numbers, in place of the band's row
//! on the tone-control screen.
//!
//! `Enter` on a band's row opens it on what the row shows, or, where a write of the band is
//! still unanswered, on the step that write asked for; a double-click on one of the row's values
//! opens it with that value's field focused. `Tab`/`⇧Tab` and `←`/`→` move between its three
//! fields; `↑`/`↓` step the focused one a device step up or down; the first character typed into
//! a field replaces what it shows, and `Backspace` edits that text instead. A number takes `.`
//! or `,` as its decimal separator, so a keypad that sends either one types it. `Enter` writes
//! every field whose text has changed, each at the device step nearest the number typed (or the
//! very step `↑↓` put it on), as the band edit every other band key makes.
//!
//! Whatever the line refuses (a character no number has, a second separator, a field that is
//! not a number or lies outside what the band takes) it says on itself, in place of its keys
//! until the next key, and in a toast, and the line stays up. A number that lands on the
//! step the band is on already, or on its way to, says so the same way, and `Enter` again closes
//! the line. `Esc` closes it with nothing written.
//!
//! While the line is open the tone-control graph previews its fields ([`preview_raw`]): each
//! field is drawn at the step `Enter` would write for its text, or at the last step it had
//! while the text is refused or incomplete; a field as it opened is drawn at the band's own
//! step. Nothing is sent before `Enter`.

use crossterm::event::KeyCode;
use svolder_profiles::convert::frequency_text;
use svolder_profiles::tone::preset::{self, Plan};
use svolder_profiles::{EqParam, EqSpec};
use svolder_proto::FunctionId;

use super::presets;
use crate::tui::app::{App, Intent, ToneRow, cycled};

/// The parameters the three fields hold, in the order the line draws them and `Tab` moves
/// through them.
pub const PARAMS: [EqParam; 3] = [EqParam::Frequency, EqParam::Gain, EqParam::Q];

/// The most characters a field takes: more than any value a band holds is written with.
const FIELD_MAX: usize = 12;

/// What a character that is no part of a number, or a second decimal separator, is refused
/// with.
const NOT_A_NUMBER: &str = "only digits, one . or , and a sign";

/// What a character past [`FIELD_MAX`] is refused with.
fn too_long() -> String {
    format!("a number takes at most {FIELD_MAX} characters")
}

/// Whether `letter` separates a number's whole part from its decimals: `.`, or the `,` a
/// French or German keyboard's keypad sends.
fn separator(letter: char) -> bool {
    matches!(letter, '.' | ',')
}

/// The characters a field takes: a number's digits, its decimal separator and its sign.
fn typable(letter: char) -> bool {
    letter.is_ascii_digit() || separator(letter) || matches!(letter, '-' | '+')
}

pub use svolder_profiles::convert::{gain_text, q_text};

/// The edit line, while it is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditLine {
    /// The band it edits, 0-based.
    pub band: usize,
    /// The focused field, as an index into [`PARAMS`].
    pub focus: usize,
    /// The three fields, in [`PARAMS`] order.
    pub fields: [Field; 3],
    /// Why the last key was refused — `f must be 82.0–312 Hz` — which the line shows in place
    /// of its keys, in the alert role, until the next key.
    pub refusal: Option<String>,
    /// What the last `Enter` found nothing to write for — `already at +0.0 dB` — which the line
    /// shows in place of its keys until the next key, an `Enter` then closing the line.
    pub already: Option<String>,
}

/// One field of the edit line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Field {
    /// The number as it has been typed.
    pub text: String,
    /// What the field opened on: the band's row as it read then, or nothing for a value the
    /// device had not reported.
    opened: String,
    /// Whether the next character typed replaces the whole of the text rather than adding to
    /// it: true for a field the focus has just landed on.
    pub fresh: bool,
    /// The device step the text reads, while it reads one: the step the field opened on, or the
    /// one `↑↓` last stepped it to. `None` once anything has been typed into it, and for a value
    /// the device had not reported.
    step: Option<u8>,
    /// The step the graph previews the field at: the one the text reads while it reads one `Enter`
    /// would write, and otherwise the last step it did. `None` while the text is as the field
    /// opened, which `Enter` writes nothing for, and the graph draws the band's own step.
    pub(crate) preview: Option<u8>,
}

impl Field {
    /// A field opened on `text`, the reading of device step `step`.
    fn new(text: String, step: Option<u8>) -> Field {
        Field {
            opened: text.clone(),
            text,
            fresh: true,
            step,
            preview: None,
        }
    }

    /// Take the step the text now reads as the preview, if it reads one: the step `↑↓` put it
    /// on, or the step nearest the number typed (the step `Enter` would write). A text that reads
    /// none leaves the preview at the last step it had, and a text back to what the field opened
    /// on has none.
    fn settle(&mut self, eq: &EqSpec, band: usize, param: EqParam) {
        if !self.changed() {
            self.preview = None;
            return;
        }
        let reads = match self.step {
            Some(step) => Some(step),
            None => step_of(eq, band, param, &self.text).ok(),
        };
        if reads.is_some() {
            self.preview = reads;
        }
    }

    /// Whether the text differs from what the field opened on: the one thing that has `Enter`
    /// write it.
    pub fn changed(&self) -> bool {
        self.text != self.opened
    }
}

impl EditLine {
    /// A line on band `band` of `app`'s tone control `eq`, its fields holding each value as the
    /// band's row writes it, at the step the band is on its way to ([`App::planned_raw`]) —
    /// the one the row shows once the device has answered every write before — the frequency
    /// focused.
    fn open(app: &App, eq: &EqSpec, band: usize) -> EditLine {
        let raw = |param| {
            eq.band_id(band, param)
                .and_then(|id| app.planned_raw(id))
                .and_then(|raw| u8::try_from(raw).ok())
        };
        let field = |param| {
            let step = raw(param);
            let text = step.and_then(|step| step_text(eq, band, param, step));
            Field::new(text.unwrap_or_default(), step)
        };
        EditLine {
            band,
            focus: 0,
            fields: PARAMS.map(field),
            refusal: None,
            already: None,
        }
    }

    /// Put the focus on field `at`, whose text the next character typed replaces.
    fn focus_on(&mut self, at: usize) {
        self.focus = at.min(PARAMS.len() - 1);
        self.fields[self.focus].fresh = true;
    }

    /// Type `letter` into the focused field: in place of its text while it is fresh, and after
    /// it otherwise, up to [`FIELD_MAX`] characters — or the words that refuse it, with the
    /// field left as it was: a character no number has, a second decimal separator, or one
    /// character too many.
    fn type_letter(&mut self, letter: char) -> Result<(), String> {
        let field = &mut self.fields[self.focus.min(PARAMS.len() - 1)];
        let kept = match field.fresh {
            true => "",
            false => field.text.as_str(),
        };
        if !typable(letter) || (separator(letter) && kept.chars().any(separator)) {
            return Err(NOT_A_NUMBER.to_string());
        }
        if kept.chars().count() >= FIELD_MAX {
            return Err(too_long());
        }
        if field.fresh {
            field.text.clear();
            field.fresh = false;
        }
        field.text.push(letter);
        field.step = None;
        Ok(())
    }

    /// Take the last character off the focused field, which from then on is edited rather than
    /// replaced.
    fn erase(&mut self) {
        let field = &mut self.fields[self.focus.min(PARAMS.len() - 1)];
        field.fresh = false;
        field.text.pop();
        field.step = None;
    }

    /// Step the focused field one device step, up for `dir > 0` and down otherwise, of band
    /// `band` of the tone control `eq`: from the step its text reads, or from the step nearest the
    /// number typed into it, its text then reading the new step as the band's row would, ready to
    /// be typed over. At either end of the steps it stays as it is. A field whose text is no
    /// number the band takes is refused with the words `Enter` would refuse it with.
    fn step(&mut self, eq: &EqSpec, dir: i64) -> Result<(), String> {
        let at = self.focus.min(PARAMS.len() - 1);
        let (band, param) = (self.band, PARAMS[at]);
        let field = &mut self.fields[at];
        let from = match field.step {
            Some(step) => step,
            None => step_of(eq, band, param, &field.text)?,
        };
        let Some(next) = i64::from(from)
            .checked_add(dir.signum())
            .filter(|next| (0..=i64::from(EqSpec::STEP_MAX)).contains(next))
            .and_then(|next| u8::try_from(next).ok())
        else {
            return Ok(());
        };
        if let Some(text) = step_text(eq, band, param, next) {
            field.text = text;
            field.step = Some(next);
            field.fresh = true;
        }
        Ok(())
    }
}

/// Step `step` of `param` on band `band` of `eq` as the band's row writes it — `160`, `+1.45`,
/// `4.30` — or `None` for a band the tone control does not have.
fn step_text(eq: &EqSpec, band: usize, param: EqParam, step: u8) -> Option<String> {
    match param {
        EqParam::Frequency => eq.frequency(band, step).map(frequency_text),
        EqParam::Gain => Some(gain_text(eq.gain_db(step))),
        EqParam::Q => Some(q_text(eq.q(step))),
    }
}

/// Open the edit line on the band the tone-control screen's cursor is on, and ask for nothing;
/// nothing opens while the cursor is on the mode or the loudness, or on a device with no tone
/// control.
pub(in crate::tui) fn open(app: &mut App) -> Vec<Intent> {
    if app.tone.row != ToneRow::Band {
        return Vec::new();
    }
    let Some(eq) = app.profile.eq.as_ref() else {
        return Vec::new();
    };
    if app.tone.band < eq.bands.len() {
        app.tone.edit_line = Some(EditLine::open(app, eq, app.tone.band));
    }
    Vec::new()
}

/// Open the edit line on band `band` with field `field` focused — an index into [`PARAMS`] — the
/// way a double-click on that value of the band's row does: the band is focused as a click on
/// its row focuses it, and a drag on the curve is let go of, as it is when any box opens over
/// the screen. Nothing opens on a band the tone control does not have.
pub(in crate::tui) fn open_at(app: &mut App, band: usize, field: usize) {
    if app
        .profile
        .eq
        .as_ref()
        .is_none_or(|eq| band >= eq.bands.len())
    {
        return;
    }
    app.focus_band(band);
    app.pointer.curve_drag = None;
    open(app);
    if let Some(line) = app.tone.edit_line.as_mut() {
        line.focus_on(field);
    }
}

/// What a key does while the edit line is open: `Tab` and `⇧Tab`, and `→` and `←`, move between
/// the fields round the ends, `↑` and `↓` step the focused field one device step up or down, a
/// number's characters are typed and any other character is refused on the line, `Backspace`
/// takes the last character off, `Enter` writes — or closes the line, right after one that found
/// nothing to write — and `Esc` closes it with nothing written. The other keys that are no
/// characters are swallowed: the line is answered, and what it covers is not reached through it.
/// Any key takes what the line last said off it, which shows its keys again.
pub(in crate::tui) fn key(app: &mut App, code: KeyCode) -> Vec<Intent> {
    let intents = handle(app, code);
    if let (Some(line), Some(eq)) = (app.tone.edit_line.as_mut(), app.profile.eq.as_ref()) {
        let (band, at) = (line.band, line.focus.min(PARAMS.len() - 1));
        line.fields[at].settle(eq, band, PARAMS[at]);
    }
    intents
}

/// [`key`] without the work after it: the line's own answer to `code`.
fn handle(app: &mut App, code: KeyCode) -> Vec<Intent> {
    let Some(line) = app.tone.edit_line.as_mut() else {
        return Vec::new();
    };
    line.refusal = None;
    let already = line.already.take().is_some();
    match code {
        KeyCode::Enter if already => {
            app.tone.edit_line = None;
            return Vec::new();
        }
        KeyCode::Enter => return write(app),
        KeyCode::Esc => {
            app.tone.edit_line = None;
            return Vec::new();
        }
        KeyCode::Tab | KeyCode::Right => line.focus_on(cycled(line.focus, 1, PARAMS.len())),
        KeyCode::BackTab | KeyCode::Left => line.focus_on(cycled(line.focus, -1, PARAMS.len())),
        KeyCode::Up | KeyCode::Down => {
            let dir = if code == KeyCode::Up { 1 } else { -1 };
            let stepped = match app.profile.eq.as_ref() {
                Some(eq) => line.step(eq, dir),
                None => Ok(()),
            };
            if let Err(why) = stepped {
                refuse(app, why);
            }
        }
        KeyCode::Backspace => line.erase(),
        KeyCode::Char(letter) => {
            if let Err(why) = line.type_letter(letter) {
                refuse(app, why);
            }
        }
        _ => {}
    }
    Vec::new()
}

/// The raw step the edit line previews function `id` at: the step of the field that holds `id`
/// on the band the line is open on, or `None` for any other function and with no line open.
/// The graph draws the band from it
/// ([`curve_drag::drawn_raw`](crate::tui::app::curve_drag::drawn_raw)); nothing is written from
/// it.
pub(crate) fn preview_raw(app: &App, id: FunctionId) -> Option<i64> {
    let line = app.tone.edit_line.as_ref()?;
    let eq = app.profile.eq.as_ref()?;
    let slot = PARAMS
        .iter()
        .position(|&param| eq.band_id(line.band, param) == Some(id))?;
    line.fields[slot].preview.map(i64::from)
}

/// Say on the line why it refused what was asked of it, until the next key, and on the key
/// line with the band it is about.
fn refuse(app: &mut App, why: String) {
    let Some(line) = app.tone.edit_line.as_mut() else {
        return;
    };
    let band = line.band + 1;
    line.refusal = Some(why.clone());
    app.warn(format!("band {band}: {why}"));
}

/// Write every field whose text has changed and close the line, or say what stops it with the
/// line still up: on the line itself, and in a toast with the band it is about.
///
/// A field left as it opened is not written, so a value the row rounds for showing is never
/// moved by a line opened over it. A changed field is written at the device step nearest the
/// number typed (at the very step it reads where `↑↓` stepped it there), and only where that is
/// a step the band is not on already, in the order a preset's recall writes a band in
/// ([`preset::band_plan`]): a gain coming down before the frequency and Q, one going up after
/// them, so the band never passes through a louder curve than either end of the edit. The
/// writes are a band edit like any other: in Flat mode they switch the tone control to EQ.
///
/// The steps a typed number is compared with are the ones the band is on its way to
/// ([`App::planned_raw`]), so a line written while a recall or a band key is still being
/// answered follows on from what they asked for. Typed numbers that all land on those steps
/// write nothing and say so (`already at +0.0 dB`) with the line still up, the next `Enter`
/// closing it.
fn write(app: &mut App) -> Vec<Intent> {
    let (Some(line), Some(eq)) = (app.tone.edit_line.clone(), app.profile.eq.clone()) else {
        app.tone.edit_line = None;
        return Vec::new();
    };
    let band = line.band;
    let mut targets = [None; 3];
    for (at, field) in line.fields.iter().enumerate() {
        if !field.changed() {
            continue;
        }
        if let Some(step) = field.step {
            targets[at] = Some(step);
            continue;
        }
        match step_of(&eq, band, PARAMS[at], &field.text) {
            Ok(raw) => targets[at] = Some(raw),
            Err(why) => {
                if let Some(line) = app.tone.edit_line.as_mut() {
                    line.focus_on(at);
                }
                refuse(app, why);
                return Vec::new();
            }
        }
    }
    let plan = Plan {
        writes: preset::band_plan(&eq, |id| app.planned_raw(id), band, targets),
        clamped: Vec::new(),
    };
    if plan.writes.is_empty() && targets.iter().any(Option::is_some) {
        let already = already_at(&eq, band, targets);
        if let Some(line) = app.tone.edit_line.as_mut() {
            line.already = Some(already);
        }
        return Vec::new();
    }
    app.tone.edit_line = None;
    let writes = presets::plan_intents(app, &eq, &plan);
    app.band_edit(writes)
}

/// What a line whose typed numbers all landed on the band's own steps says: `already at` and
/// each of those steps as the row reads it — `already at 160 Hz, +0.0 dB`.
fn already_at(eq: &EqSpec, band: usize, targets: [Option<u8>; 3]) -> String {
    let steps: Vec<String> = PARAMS
        .iter()
        .zip(targets)
        .filter_map(|(&param, raw)| {
            let raw = raw?;
            Some(match param {
                EqParam::Frequency => format!("{} Hz", frequency_text(eq.frequency(band, raw)?)),
                EqParam::Gain => format!("{} dB", gain_text(eq.gain_db(raw))),
                EqParam::Q => format!("Q {}", q_text(eq.q(raw))),
            })
        })
        .collect();
    format!("already at {}", steps.join(", "))
}

/// The raw step of band `band` nearest `text` typed for `param`, or the words that refuse it:
/// the range the band takes — `f must be 82.0–312 Hz`, `g must be -20.0–+6.0 dB`, `q must be
/// 0.50–8.00` — for a text that is not a number or a number outside that range.
///
/// A frequency is taken anywhere from the band's lowest step to its highest as the row writes
/// them (see [`frequency_text`]), and one between a step written that way and the step itself
/// goes to the step; a gain and a Q are taken inside the range the profile gives them.
fn step_of(eq: &EqSpec, band: usize, param: EqParam, text: &str) -> Result<u8, String> {
    let typed = text
        .trim()
        .replace(',', ".")
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite());
    match param {
        EqParam::Frequency => {
            let edge = |raw| {
                eq.frequency(band, raw)
                    .map(frequency_text)
                    .unwrap_or_default()
            };
            let (low, high) = (edge(0), edge(EqSpec::STEP_MAX));
            let within = |value: f64| {
                let (Ok(low), Ok(high)) = (low.parse::<f64>(), high.parse::<f64>()) else {
                    return false;
                };
                (low..=high).contains(&value)
            };
            typed
                .filter(|&hz| within(hz))
                .and_then(|hz| eq.frequency_raw(band, hz))
                .ok_or_else(|| format!("f must be {low}–{high} Hz"))
        }
        EqParam::Gain => {
            let (low, high) = eq.gain_range;
            typed
                .filter(|db| (low..=high).contains(db))
                .map(|db| eq.gain_raw(db))
                .ok_or_else(|| format!("g must be {low:+.1}–{high:+.1} dB"))
        }
        EqParam::Q => {
            let (low, high) = eq.q_range;
            typed
                .filter(|q| (low..=high).contains(q))
                .map(|q| eq.q_raw(q))
                .ok_or_else(|| format!("q must be {low:.2}–{high:.2}"))
        }
    }
}

#[cfg(test)]
mod tests;
