// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// `←→` step round `none`, `auto`, the code and back to `none`, and wrap at both ends the way
/// the Profile row does.
#[test]
fn the_arrows_step_round_none_auto_and_the_code_and_wrap() {
    let mut row = PassphraseRow::from_file("none");
    let mut seen = Vec::new();
    for _ in 0..4 {
        seen.push(row.mode());
        row.step(1);
    }
    assert_eq!(seen, [Mode::None, Mode::Auto, Mode::Code, Mode::None]);
    // Four steps on from none is auto; two back is none again, and one more wraps to the code.
    row.step(-1);
    assert_eq!(row.mode(), Mode::None);
    row.step(-1);
    assert_eq!(row.mode(), Mode::Code, "back from none wraps to the code");
}

/// Only digits are typed, four of them in order, and `Backspace` takes back the last.
#[test]
fn only_digits_are_typed_in_order_and_backspace_takes_one_back() {
    let mut row = PassphraseRow::code();
    for key in "1a2-3 45".chars() {
        row.key(KeyCode::Char(key));
    }
    assert_eq!(
        row.digits(),
        "1234",
        "letters and a fifth digit are ignored"
    );
    assert_eq!(row.shown(), "1234");
    row.key(KeyCode::Backspace);
    assert_eq!(row.shown(), "123_");
    assert!(!row.key(KeyCode::Home), "a key that is not the row's");
}

/// A digit typed on `none` or `auto` starts the code with that digit, and digits kept from an
/// earlier visit are not carried into it.
#[test]
fn a_digit_on_none_or_auto_starts_the_code_afresh() {
    let mut row = PassphraseRow::from_file("auto");
    row.type_digit('7');
    assert_eq!((row.mode(), row.digits()), (Mode::Code, "7"));
    row.type_digit('8');
    row.step(1);
    assert_eq!(row.mode(), Mode::None);
    assert_eq!(row.digits(), "78", "stepping away keeps the digits");
    row.type_digit('5');
    assert_eq!((row.mode(), row.digits()), (Mode::Code, "5"));
}

/// Stepping away from the code keeps the digits, and stepping back shows them again.
#[test]
fn stepping_away_from_the_code_keeps_its_digits() {
    let mut row = PassphraseRow::code();
    row.type_digit('1');
    row.type_digit('2');
    row.step(1);
    assert_eq!(row.mode(), Mode::None);
    assert_eq!(row.shown(), "none");
    row.step(-1);
    assert_eq!(row.shown(), "12__");
}

/// The row opens on what the file says, and answers with what the file carries; a code short
/// of four digits is refused in the words the editor's toast uses.
#[test]
fn the_row_opens_on_the_file_and_answers_with_it() {
    for (file, mode, digits) in [
        ("auto", Mode::Auto, ""),
        ("none", Mode::None, ""),
        ("4321", Mode::Code, "4321"),
        ("garbled", Mode::Auto, ""),
    ] {
        let row = PassphraseRow::from_file(file);
        assert_eq!((row.mode(), row.digits()), (mode, digits), "{file}");
    }
    assert_eq!(PassphraseRow::from_file("4321").text(), Ok("4321".into()));
    assert_eq!(PassphraseRow::from_file("auto").text(), Ok("auto".into()));
    assert_eq!(PassphraseRow::from_file("none").text(), Ok("none".into()));
    let mut short = PassphraseRow::code();
    short.type_digit('1');
    assert_eq!(short.text(), Err(A_CODE_IS_FOUR_DIGITS));
}

/// `Backspace` takes back a digit of the code only: on `none` and `auto` it leaves the digits an
/// earlier visit kept alone.
#[test]
fn backspace_off_the_code_leaves_the_kept_digits_alone() {
    let mut row = PassphraseRow::code();
    row.type_digit('1');
    row.type_digit('2');
    row.step(1);
    row.erase();
    row.step(-1);
    assert_eq!(row.shown(), "12__");
}
