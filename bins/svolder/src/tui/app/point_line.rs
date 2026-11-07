// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The edit line of the computer's side: one point's frequency, gain and Q typed as numbers in
//! place of the point's row, as the band's edit line does on the device's side.
//!
//! `Enter` on a point's row, or a double-click on one of its values, opens it. `Tab`/`⇧Tab` and
//! `←`/`→` move between the fields, the first character typed into a field replaces what it
//! shows, `Backspace` edits it, `Enter` writes every field whose text changed in one change of
//! the point, and `Esc` closes the line with nothing written. A number outside what a hand may
//! set is held to the end of its range, the other fields as they were; text that is no number is
//! refused on the line, which stays up.

use crossterm::event::KeyCode;
use svolder_profiles::convert::{frequency_text, gain_text, q_text};

use super::side::{F_RANGE, G_RANGE, Q_RANGE, set_point};
use super::{App, Intent};

/// The most characters a field takes.
const FIELD_MAX: usize = 12;

/// What the fields are called, in the order the line draws them.
pub(crate) const NAMES: [(&str, &str); 3] = [("f", "Hz"), ("g", "dB"), ("q", "")];

/// The edit line, while it is open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PointLine {
    /// The point it edits, 0-based.
    pub point: usize,
    /// The slot the point held when the line opened (0: none was given): the edit goes to the
    /// point in that slot, wherever the table lists it by then.
    pub slot: u8,
    /// The focused field: frequency, gain or Q.
    pub focus: usize,
    /// The three texts, as typed.
    pub fields: [String; 3],
    /// What they opened on.
    opened: [String; 3],
    /// Whether the next character typed replaces the focused text.
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

impl App {
    /// Open the line on point `point`, focused on field `field`.
    pub(crate) fn open_point_line(&mut self, point: usize, field: usize) {
        let Some(p) = self.computer_points().get(point).cloned() else {
            return;
        };
        let fields = [frequency_text(p.f), gain_text(p.g), q_text(p.q)];
        self.focus_point(point);
        self.tone.point_line = Some(PointLine {
            point,
            slot: p.slot,
            focus: field.min(2),
            opened: fields.clone(),
            fields,
            fresh: true,
            refusal: None,
        });
    }

    /// A click on one of a point's cells: the point is focused, the type cell cycles its type, and
    /// a double-click on a value opens the line on it.
    pub(crate) fn click_point_field(&mut self, point: usize, field: usize) -> Vec<Intent> {
        self.focus_point(point);
        match field {
            // The first click of a double-click has cycled it already.
            0 if self.pointer.double_click => Vec::new(),
            0 => self.cycle_point_type(point),
            _ if self.pointer.double_click && self.screen == super::Screen::Tone => {
                self.open_point_line(point, field - 1);
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// What a key does while the line is open.
    pub(super) fn point_line_key(&mut self, code: KeyCode) -> Vec<Intent> {
        let Some(line) = self.tone.point_line.as_mut() else {
            return Vec::new();
        };
        line.refusal = None;
        match code {
            KeyCode::Esc => self.tone.point_line = None,
            KeyCode::Enter => return self.write_point_line(),
            KeyCode::Tab | KeyCode::Right => {
                line.focus = (line.focus + 1) % 3;
                line.fresh = true;
            }
            KeyCode::BackTab | KeyCode::Left => {
                line.focus = (line.focus + 2) % 3;
                line.fresh = true;
            }
            KeyCode::Backspace => {
                if std::mem::take(&mut line.fresh) {
                    line.fields[line.focus].clear();
                } else {
                    line.fields[line.focus].pop();
                }
            }
            KeyCode::Char(letter) => {
                if !(letter.is_ascii_digit() || matches!(letter, '.' | ',' | '-' | '+')) {
                    line.refusal = Some("only digits, one . or , and a sign".to_string());
                } else {
                    if std::mem::take(&mut line.fresh) {
                        line.fields[line.focus].clear();
                    }
                    if line.fields[line.focus].chars().count() >= FIELD_MAX {
                        line.refusal =
                            Some(format!("a number takes at most {FIELD_MAX} characters"));
                    } else {
                        line.fields[line.focus].push(letter);
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// `Enter`: write the fields whose text changed, each held to its range, as one change.
    fn write_point_line(&mut self) -> Vec<Intent> {
        let Some(line) = self.tone.point_line.clone() else {
            return Vec::new();
        };
        let ranges = [F_RANGE, G_RANGE, Q_RANGE];
        let mut values = [None; 3];
        for at in 0..3 {
            if line.fields[at] == line.opened[at] {
                continue;
            }
            match number(&line.fields[at]) {
                Some(n) => values[at] = Some(n.clamp(ranges[at].0, ranges[at].1)),
                None => {
                    let name = NAMES[at].0;
                    if let Some(open) = self.tone.point_line.as_mut() {
                        open.refusal = Some(format!("{name} is not a number"));
                        open.focus = at;
                    }
                    return Vec::new();
                }
            }
        }
        self.tone.point_line = None;
        match values {
            [None, None, None] => Vec::new(),
            values => vec![set_point(line.point, line.slot, values, true)],
        }
    }
}
