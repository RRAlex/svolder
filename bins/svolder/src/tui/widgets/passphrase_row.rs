// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Passphrase row as a three-way stepper, which the device editor and the setup wizard share.
//!
//! The row is one of three answers: `none`, for a device with no passphrase, `auto`, for the
//! code the device's advertisement carries, or a code of four digits. `←→` step round them,
//! `none` then `auto` then the code and back to `none`, and a digit typed on any of them starts
//! or goes on with the code. The digits are typed in order, one after another, so there is no
//! caret and the arrows stay free for stepping; `Backspace` takes back the last one. Stepping
//! away from the code keeps the digits typed, so stepping back finds them again.

use crossterm::event::KeyCode;

use crate::tui::app::cycled;

/// How many digits a code is.
pub const CODE_DIGITS: usize = 4;

/// What a row with a code of fewer digits than that is refused with.
pub const A_CODE_IS_FOUR_DIGITS: &str = "a code is four digits";

/// What a device that asks for no passphrase carries in the file.
pub const NONE: &str = "none";

/// What a device that takes the code its advertisement carries has in the file.
pub const AUTO: &str = "auto";

/// Which of its three answers the row is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// A device with no passphrase.
    None,
    /// The code the advertisement carries.
    Auto,
    /// A code of four digits, typed.
    Code,
}

/// The modes in the order `←→` step through them.
const MODES: [Mode; 3] = [Mode::None, Mode::Auto, Mode::Code];

/// The row: its mode, and the digits typed so far, which stay when the mode steps away from
/// the code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PassphraseRow {
    /// Which of its three answers the row is on.
    mode: Mode,
    /// The digits typed so far, kept while the row is on another answer.
    digits: String,
}

impl PassphraseRow {
    /// The row a configuration file's `passphrase` value opens: `auto`, `none`, or the code the
    /// four digits are. A value that is none of those opens on `auto`, which is what the file
    /// means by anything it does not say.
    pub fn from_file(text: &str) -> PassphraseRow {
        match text {
            NONE => PassphraseRow::on(Mode::None),
            AUTO => PassphraseRow::on(Mode::Auto),
            code if code.len() == CODE_DIGITS && code.bytes().all(|b| b.is_ascii_digit()) => {
                PassphraseRow {
                    mode: Mode::Code,
                    digits: code.to_string(),
                }
            }
            _ => PassphraseRow::on(Mode::Auto),
        }
    }

    /// The row on a code with none of its digits typed yet.
    pub fn code() -> PassphraseRow {
        PassphraseRow::on(Mode::Code)
    }

    /// The row on `mode`, with no digits.
    fn on(mode: Mode) -> PassphraseRow {
        PassphraseRow {
            mode,
            digits: String::new(),
        }
    }

    /// Which answer the row is on.
    #[cfg(test)]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// The digits typed so far, whichever mode the row is on.
    #[cfg(test)]
    pub fn digits(&self) -> &str {
        &self.digits
    }

    /// What the row reads between its arrows: `none`, `auto`, or the digits typed with a `_` for
    /// each still to come.
    pub fn shown(&self) -> String {
        match self.mode {
            Mode::None => NONE.to_string(),
            Mode::Auto => AUTO.to_string(),
            Mode::Code => {
                let missing = CODE_DIGITS.saturating_sub(self.digits.len());
                format!("{}{}", self.digits, "_".repeat(missing))
            }
        }
    }

    /// Step the mode `dir` places round `none`, `auto`, the code, wrapping at each end.
    pub fn step(&mut self, dir: isize) {
        let at = MODES
            .iter()
            .position(|mode| *mode == self.mode)
            .unwrap_or(0);
        self.mode = MODES[cycled(at, dir, MODES.len())];
    }

    /// Type one digit. A row on `none` or `auto` goes to the code and starts it with this digit,
    /// whatever digits an earlier visit left; a row on the code takes it after the others, up to
    /// four. Anything that is not a digit is ignored.
    pub fn type_digit(&mut self, digit: char) {
        if !digit.is_ascii_digit() {
            return;
        }
        if self.mode != Mode::Code {
            self.mode = Mode::Code;
            self.digits.clear();
        }
        if self.digits.len() < CODE_DIGITS {
            self.digits.push(digit);
        }
    }

    /// Take back the last digit of the code. A row on `none` or `auto` has none to take back.
    pub fn erase(&mut self) {
        if self.mode == Mode::Code {
            self.digits.pop();
        }
    }

    /// What `code` does to the row, and whether the row took it: `←` and `→` step, a digit is
    /// typed, `Backspace` takes one back. Any other key is not the row's.
    pub fn key(&mut self, code: KeyCode) -> bool {
        match code {
            KeyCode::Left => self.step(-1),
            KeyCode::Right => self.step(1),
            KeyCode::Char(c) if c.is_ascii_digit() => self.type_digit(c),
            KeyCode::Backspace => self.erase(),
            _ => return false,
        }
        true
    }

    /// What the row asks the file to carry: `none`, `auto`, or the four digits, or the words that
    /// say why it cannot yet be answered, which is a code with fewer digits than four.
    pub fn text(&self) -> Result<String, &'static str> {
        match self.mode {
            Mode::None => Ok(NONE.to_string()),
            Mode::Auto => Ok(AUTO.to_string()),
            Mode::Code if self.digits.len() == CODE_DIGITS => Ok(self.digits.clone()),
            Mode::Code => Err(A_CODE_IS_FOUR_DIGITS),
        }
    }
}

#[cfg(test)]
mod tests;
