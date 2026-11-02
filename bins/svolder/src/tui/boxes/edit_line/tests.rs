// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use svolder_ipc::TuiConfig;
use svolder_profiles::tone::preset::SWITCHED_TO_EQ;
use svolder_proto::FunctionId;

use crate::backend::DeviceRow;
use crate::tui::app::{App, Intent, Pane, Screen, ToneRow};
use crate::tui::screens;
use crate::tui::test_support::{eq, hear_one as hear, press, profile};
use crate::tui::theme::Theme;
use ratatui::layout::Rect;
use svolder_profiles::tone::preset::Write;

use super::*;

/// A Gungnir 2 on the tone-control screen, the tone control in `mode` (1 flat, 2 EQ, 3
/// loudness) and every band flat on its own centre: raw 64 is the centre frequency, 98 is
/// 0 dB and 64 is Q 4.30. The cursor is on band 2, whose window is 82 to 312 Hz.
fn tone_app(mode: i64) -> App {
    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("gungnir")],
    );
    hear(&mut app, "12.50", mode);
    hear(&mut app, "12.52", 0);
    for band in 1..=5 {
        hear(&mut app, &format!("12.{band}.2"), 64);
        hear(&mut app, &format!("12.{band}.3"), 98);
        hear(&mut app, &format!("12.{band}.1"), 64);
    }
    app.show(Screen::Tone);
    press(&mut app, KeyCode::Char('2'));
    app
}

/// Type every character of `text`, and answer what the last of them asked for.
fn typed(app: &mut App, text: &str) -> Vec<Intent> {
    let mut intents = Vec::new();
    for letter in text.chars() {
        intents = press(app, KeyCode::Char(letter));
    }
    intents
}

/// The edit line, which the case expects to be open.
fn line(app: &App) -> &EditLine {
    app.tone.edit_line.as_ref().expect("the edit line is open")
}

/// The three fields' texts, in frequency, gain and Q order.
fn texts(app: &App) -> [String; 3] {
    line(app).fields.clone().map(|field| field.text)
}

/// One band write, as the edit line sends it: the value `raw` stands for on band `band`
/// (1-based), converted the way the device converts it back.
fn write(band: usize, param: EqParam, raw: u8) -> Intent {
    Intent::EqSet {
        band,
        param: param.word().into(),
        value: Write::Band {
            band: band - 1,
            param,
            raw,
        }
        .value(&eq())
        .expect("a step of the band"),
    }
}

/// `Enter` on a band's row opens the line on the three values the row shows, rounded as the
/// row rounds them, with the frequency focused and waiting to be typed over. A value the
/// device has not reported opens empty. It asks for nothing, and nothing opens from the mode
/// or the loudness row, nor from the device screen.
#[test]
fn enter_on_a_band_opens_its_line_on_what_its_row_shows() {
    let mut app = tone_app(2);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.modal_open());
    assert_eq!(line(&app).band, 1);
    assert_eq!(line(&app).focus, 0);
    assert_eq!(texts(&app), ["160", "+0.0", "4.30"]);
    assert!(line(&app).fields.iter().all(|field| field.fresh));

    // 85 is 199.9 Hz, 105 is +1.448 dB and 17 is Q 1.509.
    let mut app = tone_app(2);
    hear(&mut app, "12.2.2", 85);
    hear(&mut app, "12.2.3", 105);
    hear(&mut app, "12.2.1", 17);
    press(&mut app, KeyCode::Enter);
    assert_eq!(texts(&app), ["200", "+1.45", "1.51"]);

    let mut app = tone_app(2);
    press(&mut app, KeyCode::Char('3'));
    app.readings.values.remove(&FunctionId::three(12, 3, 3));
    press(&mut app, KeyCode::Enter);
    assert_eq!(texts(&app), ["632", "", "4.30"]);

    let mut app = tone_app(2);
    press(&mut app, KeyCode::Char('1'));
    for row in [ToneRow::Loudness, ToneRow::Mode] {
        press(&mut app, KeyCode::Up);
        assert_eq!(app.tone.row, row);
        assert!(press(&mut app, KeyCode::Enter).is_empty());
        assert!(app.tone.edit_line.is_none(), "{row:?} opened a line");
    }

    let mut app = tone_app(2);
    app.show(Screen::Device);
    app.pane = Pane::Tone;
    assert_eq!(app.tone.row, ToneRow::Band);
    press(&mut app, KeyCode::Enter);
    assert!(
        app.tone.edit_line.is_none(),
        "the device screen opened a line"
    );
}

/// `Tab` and `⇧Tab` move round the three fields, past either end, and every field they land
/// on is typed over by the next character.
#[test]
fn tab_and_shift_tab_move_round_the_fields() {
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    for wanted in [1, 2, 0, 1] {
        typed(&mut app, "1");
        assert!(press(&mut app, KeyCode::Tab).is_empty());
        assert_eq!(line(&app).focus, wanted);
        assert!(line(&app).fields[wanted].fresh, "Tab onto field {wanted}");
    }
    for wanted in [0, 2, 1] {
        assert!(press(&mut app, KeyCode::BackTab).is_empty());
        assert_eq!(line(&app).focus, wanted);
        assert!(line(&app).fields[wanted].fresh, "⇧Tab onto field {wanted}");
    }
}

/// `→` and `←` move round the three fields as `Tab` and `⇧Tab` do, past either end, and every
/// field they land on is typed over by the next character.
#[test]
fn the_side_arrows_move_round_the_fields() {
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    for wanted in [1, 2, 0, 1] {
        typed(&mut app, "1");
        assert!(press(&mut app, KeyCode::Right).is_empty());
        assert_eq!(line(&app).focus, wanted);
        assert!(line(&app).fields[wanted].fresh, "→ onto field {wanted}");
    }
    for wanted in [0, 2, 1] {
        assert!(press(&mut app, KeyCode::Left).is_empty());
        assert_eq!(line(&app).focus, wanted);
        assert!(line(&app).fields[wanted].fresh, "← onto field {wanted}");
    }
}

/// `↑` and `↓` step the focused field one of the device's own steps, the field reading the
/// new step as the band's row would: up from -0.20 dB (step 97) reads `+0.0`, and `Enter`
/// then writes 0 dB exactly, step 98. A frequency and a Q step one raw step too, each reading
/// as the row reads it, and a step back onto what the field opened on writes nothing.
#[test]
fn the_up_and_down_arrows_step_the_focused_field_one_device_step() {
    let eq = eq();
    let mut app = tone_app(2);
    hear(&mut app, "12.2.3", 97);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    assert_eq!(texts(&app)[1], "-0.20");
    assert!(press(&mut app, KeyCode::Up).is_empty());
    assert_eq!(texts(&app)[1], "+0.0");
    assert!(
        line(&app).fields[1].fresh,
        "the stepped value is not ready to be typed over"
    );
    let written = press(&mut app, KeyCode::Enter);
    assert_eq!(written, vec![write(2, EqParam::Gain, eq.gain_zero)]);
    let [Intent::EqSet { value, .. }] = written.as_slice() else {
        unreachable!("one gain write");
    };
    assert_eq!(*value, 0.0, "not 0 dB exactly");

    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Up);
    assert_eq!(
        texts(&app)[0],
        frequency_text(eq.frequency(1, 65).expect("a step"))
    );
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(texts(&app)[2], q_text(eq.q(62)));
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![write(2, EqParam::Frequency, 65), write(2, EqParam::Q, 62)]
    );

    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Up);
    assert_eq!(texts(&app)[0], "160");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(
        app.tone.edit_line.is_none(),
        "a field stepped back wrote something"
    );
}

/// At the top of a field's steps `↑` does nothing, and at the bottom `↓` does nothing: the
/// field keeps its text and nothing is refused.
#[test]
fn an_arrow_past_the_end_of_a_fields_steps_does_nothing() {
    for (raw, code) in [(127, KeyCode::Up), (0, KeyCode::Down)] {
        for (at, param) in PARAMS.iter().enumerate() {
            let mut app = tone_app(2);
            hear(&mut app, &format!("12.2.{}", *param as u8), raw);
            press(&mut app, KeyCode::Enter);
            for _ in 0..at {
                press(&mut app, KeyCode::Tab);
            }
            let before = line(&app).clone();
            assert!(press(&mut app, code).is_empty(), "{param:?} {code:?}");
            assert_eq!(line(&app), &before, "{param:?} at step {raw}: {code:?}");
        }
    }
}

/// A number typed in is stepped from the device step nearest it — `1.5` dB is step 105, so
/// `↑` reads step 106 — and a field that is no number the band takes is refused with the
/// words `Enter` would refuse it with, the field kept as it was.
#[test]
fn an_arrow_steps_a_typed_number_from_its_nearest_step() {
    let eq = eq();
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "1.5");
    press(&mut app, KeyCode::Up);
    assert_eq!(texts(&app)[1], gain_text(eq.gain_db(106)));
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![write(2, EqParam::Gain, 106)]
    );

    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "-");
    press(&mut app, KeyCode::Up);
    assert_eq!(texts(&app)[1], "-");
    assert_eq!(
        line(&app).refusal.as_deref(),
        Some("g must be -20.0–+6.0 dB")
    );
}

/// A double-click on one of a band row's values opens the band's line with that value's
/// field focused and ready to be typed over, and focuses the band, as a click on its row
/// does; it lets go of a drag on the curve, as any box opening over the screen does. A band
/// the tone control does not have opens nothing.
#[test]
fn open_at_opens_the_line_on_the_field_asked_for() {
    for field in 0..PARAMS.len() {
        let mut app = tone_app(2);
        app.tone.row = ToneRow::Mode;
        crate::tui::app::curve_drag::start(
            &mut app,
            0,
            crate::tui::app::curve_drag::Gesture::Move,
            (10, 6),
        );
        open_at(&mut app, 3, field);
        assert_eq!((app.tone.band, app.tone.row), (3, ToneRow::Band));
        assert_eq!(line(&app).band, 3);
        assert_eq!(line(&app).focus, field);
        assert!(line(&app).fields[field].fresh);
        assert_eq!(
            app.pointer.curve_drag, None,
            "the drag outlived the line opening"
        );
    }
    let mut app = tone_app(2);
    open_at(&mut app, 5, 0);
    assert!(app.tone.edit_line.is_none());
}

/// The first character typed into a field replaces what it shows and the next ones follow
/// it; `Backspace` edits the text instead of replacing it. Only a number's characters are
/// typed, up to [`FIELD_MAX`] of them, and every other key that is not one of the line's own
/// is swallowed while the line is up.
#[test]
fn typing_replaces_a_fresh_field_and_backspace_edits_it() {
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    typed(&mut app, "2");
    assert_eq!(texts(&app)[0], "2", "the first digit replaces 160");
    assert!(!line(&app).fields[0].fresh);
    typed(&mut app, "00");
    assert_eq!(texts(&app)[0], "200");

    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Backspace);
    assert_eq!(
        texts(&app)[1],
        "+0.",
        "Backspace edits +0.0 rather than replacing it"
    );
    typed(&mut app, "5");
    assert_eq!(texts(&app)[1], "+0.5");

    press(&mut app, KeyCode::Tab);
    typed(&mut app, "-+.9");
    assert_eq!(
        texts(&app)[2],
        "-+.9",
        "every character of a number is typed"
    );
    typed(&mut app, &"9".repeat(20));
    assert_eq!(texts(&app)[2].chars().count(), FIELD_MAX);
    assert_eq!(
        line(&app).refusal.as_deref(),
        Some("a number takes at most 12 characters"),
        "a character past the most a field takes went unsaid"
    );

    // A character no number has is refused on the line, and the field keeps its text.
    let fields = line(&app).fields.clone();
    let before = app.snapshot_for_test();
    for letter in ['q', '?', 'e', 'M', 'p', ' ', 'x'] {
        assert!(
            typed(&mut app, &letter.to_string()).is_empty(),
            "{letter:?}"
        );
        assert_eq!(app.snapshot_for_test(), before, "{letter:?}");
        assert_eq!(line(&app).fields, fields, "{letter:?} was typed");
        assert_eq!(
            line(&app).refusal.as_deref(),
            Some(NOT_A_NUMBER),
            "{letter:?} went unsaid"
        );
        assert_eq!(
            app.toast().map(|toast| toast.text.as_str()),
            Some(format!("band 2: {NOT_A_NUMBER}").as_str())
        );
    }
    // The keys that are no characters and none of the line's own are swallowed, and take
    // the refusal off the line.
    for code in [
        KeyCode::Home,
        KeyCode::End,
        KeyCode::PageUp,
        KeyCode::Delete,
    ] {
        assert!(
            press(&mut app, code).is_empty(),
            "{code:?} asked for a write"
        );
        assert_eq!(app.snapshot_for_test(), before, "{code:?}");
        assert_eq!(line(&app).fields, fields, "{code:?}");
        assert_eq!(line(&app).refusal, None, "{code:?}");
    }
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(app.key(ctrl_c), vec![Intent::Quit]);
}

/// `Enter` writes every field that was typed into at the device step nearest the number, as
/// the value that step stands for, and closes the line: `1.5` dB is step 105, +1.448 dB. The
/// frequency and Q go first, and a gain going up after them.
#[test]
fn enter_writes_each_changed_field_at_its_nearest_step() {
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    typed(&mut app, "200");
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "1.5");
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "1.5");
    let intents = press(&mut app, KeyCode::Enter);
    assert_eq!(
        intents,
        vec![
            write(2, EqParam::Frequency, 85),
            write(2, EqParam::Q, 17),
            write(2, EqParam::Gain, 105),
        ]
    );
    let Intent::EqSet { value, .. } = &intents[2] else {
        unreachable!("the gain is an EqSet");
    };
    assert!((value - 1.448).abs() < 0.001, "{value}");
    assert!(app.tone.edit_line.is_none(), "the line stayed open");
    assert!(
        app.toast().is_none(),
        "{:?}",
        app.toast().map(|toast| toast.text.clone())
    );
}

/// A gain coming down is written before the frequency and Q, so the band never passes
/// through a louder curve than either end of the edit.
#[test]
fn a_gain_coming_down_is_written_first() {
    let mut app = tone_app(2);
    hear(&mut app, "12.2.3", 110);
    press(&mut app, KeyCode::Enter);
    typed(&mut app, "200");
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "-3");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![
            write(2, EqParam::Gain, eq().gain_raw(-3.0)),
            write(2, EqParam::Frequency, 85),
        ]
    );
}

/// Only a field typed into is written, and only where it moves the band off the step it is
/// on: a frequency the row rounds for showing stays where it is when the gain beside it is
/// typed, `Enter` with nothing typed writes nothing, and a number on the band's own step
/// writes nothing either. The line closes all the same.
#[test]
fn only_a_field_typed_into_and_moved_is_written() {
    // Step 65 is 40.44 Hz, which the row shows as 40.4.
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Char('1'));
    hear(&mut app, "12.1.2", 65);
    press(&mut app, KeyCode::Enter);
    assert_eq!(texts(&app)[0], "40.4");
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "3");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![write(1, EqParam::Gain, eq().gain_raw(3.0))]
    );

    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.tone.edit_line.is_none());

    // Numbers on the band's own steps say so on the line, and the next `Enter` closes it.
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    typed(&mut app, "160.3");
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "0");
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "4.31");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(
        line(&app).already.as_deref(),
        Some("already at 160 Hz, +0.0 dB, Q 4.30")
    );
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.tone.edit_line.is_none());
}

/// A number that lands on the step the band is on already writes nothing and says so on
/// the line — `0.1` dB is nearest 0 dB — with the line kept up: `Enter` again, or `Esc`,
/// closes it, and a key typed in between takes the words off and lets the next `Enter`
/// write.
#[test]
fn a_number_on_the_bands_own_step_says_so() {
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "0.1");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(line(&app).already.as_deref(), Some("already at +0.0 dB"));
    assert_eq!(line(&app).refusal, None);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.tone.edit_line.is_none(), "Enter again left the line up");

    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "0.1");
    press(&mut app, KeyCode::Enter);
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert!(app.tone.edit_line.is_none(), "Esc left the line up");

    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "0.1");
    press(&mut app, KeyCode::Enter);
    typed(&mut app, "1");
    assert_eq!(line(&app).already, None);
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![write(2, EqParam::Gain, eq().gain_raw(0.11))]
    );
}

/// A number takes `,` as its decimal separator as well as `.`, as a keypad on a French or
/// German layout sends it: `0,5` dB writes what `0.5` does, and a Q or a frequency likewise.
/// A second separator, of either kind, is refused on the line and not typed.
#[test]
fn a_comma_is_a_decimal_separator_and_a_second_separator_is_refused() {
    let enter = |field: usize, text: &str| {
        let mut app = tone_app(2);
        press(&mut app, KeyCode::Char('1'));
        press(&mut app, KeyCode::Enter);
        for _ in 0..field {
            press(&mut app, KeyCode::Tab);
        }
        typed(&mut app, text);
        assert_eq!(texts(&app)[field], text, "the field shows what was typed");
        press(&mut app, KeyCode::Enter)
    };
    let half = vec![write(1, EqParam::Gain, eq().gain_raw(0.5))];
    assert_eq!(enter(1, "0,5"), half);
    assert_eq!(enter(1, "0.5"), half);
    assert_eq!(
        enter(1, "-1,5"),
        vec![write(1, EqParam::Gain, eq().gain_raw(-1.5))]
    );
    assert_eq!(enter(2, "0,7"), vec![write(1, EqParam::Q, eq().q_raw(0.7))]);
    let hz = eq().frequency_raw(0, 50.5).expect("a step");
    assert_eq!(enter(0, "50,5"), vec![write(1, EqParam::Frequency, hz)]);

    for (first, second) in [("0,5", ','), ("0,5", '.'), ("0.5", ','), ("0.5", '.')] {
        let mut app = tone_app(2);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Tab);
        typed(&mut app, first);
        typed(&mut app, &second.to_string());
        assert_eq!(texts(&app)[1], first, "{first} then {second:?}");
        assert_eq!(line(&app).refusal.as_deref(), Some(NOT_A_NUMBER));
    }
    // A separator typed over a fresh field is its first character, and taken.
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, ",5");
    assert_eq!(texts(&app)[1], ",5");
    assert_eq!(line(&app).refusal, None);
}

/// The line writes a band in the order a preset's recall does, from the one planner: a gain
/// the device has not reported yet, typed as a cut, goes before the frequency, as it would
/// in a recall, so the band never passes through whatever unknown boost it might hold.
#[test]
fn an_unread_gain_typed_as_a_cut_is_written_first() {
    let mut app = tone_app(2);
    app.readings.values.remove(&FunctionId::three(12, 2, 3));
    press(&mut app, KeyCode::Enter);
    typed(&mut app, "200");
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "-3");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![
            write(2, EqParam::Gain, eq().gain_raw(-3.0)),
            write(2, EqParam::Frequency, 85),
        ]
    );
}

/// A field that is not a number, or lies outside what the band takes, is refused with the
/// range it does take — on the line until the next key, and on the key line with the band's
/// number — the line kept up on that field with what was typed still in it and ready to be
/// typed over; nothing is written, the fields that were fine included. The ends of each
/// range as the refusal writes them are taken.
#[test]
fn a_refused_field_keeps_the_line_open_on_it() {
    let frequency = "f must be 82.0–312 Hz";
    let gain = "g must be -20.0–+6.0 dB";
    let q = "q must be 0.50–8.00";
    for (at, text, why) in [
        (0, "5", frequency),
        (0, "313", frequency),
        (0, "1-2", frequency),
        (1, "7", gain),
        (1, "-20.1", gain),
        (1, "-", gain),
        (1, "+", gain),
        (2, "9", q),
        (2, "0.49", q),
        (2, "+-1", q),
    ] {
        let mut app = tone_app(2);
        press(&mut app, KeyCode::Enter);
        // A good value in every other field is held back with the refused one.
        for (field, good) in ["200", "1", "1.5"].into_iter().enumerate() {
            typed(&mut app, if field == at { text } else { good });
            press(&mut app, KeyCode::Tab);
        }
        assert!(press(&mut app, KeyCode::Enter).is_empty(), "{text}");
        assert_eq!(
            app.toast().map(|toast| toast.text.as_str()),
            Some(format!("band 2: {why}").as_str()),
            "{text}"
        );
        let line = line(&app);
        assert_eq!(line.refusal.as_deref(), Some(why), "{text}");
        assert_eq!(line.focus, at, "{text}");
        assert!(line.fields[at].fresh, "{text}");
        assert_eq!(line.fields[at].text, text);
    }

    // The refusal stands until the next key, whatever that key is.
    for code in [KeyCode::Char('9'), KeyCode::Tab, KeyCode::Left] {
        let mut app = tone_app(2);
        press(&mut app, KeyCode::Enter);
        typed(&mut app, "5");
        press(&mut app, KeyCode::Enter);
        assert!(line(&app).refusal.is_some());
        press(&mut app, code);
        assert_eq!(line(&app).refusal, None, "{code:?} left the refusal up");
    }

    // A band whose window starts below 100 Hz is refused to the tenth of a hertz.
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Char('1'));
    press(&mut app, KeyCode::Enter);
    typed(&mut app, "90");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        line(&app).refusal.as_deref(),
        Some("f must be 20.0–78.0 Hz")
    );

    // A field emptied is refused too.
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    for _ in 0..3 {
        press(&mut app, KeyCode::Backspace);
    }
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(line(&app).refusal.as_deref(), Some(frequency));

    for (at, text, raw) in [
        (0, "82", 0),
        (0, "312", 127),
        (1, "-20", 0),
        (1, "+6", 127),
        (2, "0.5", 0),
        (2, "8", 127),
    ] {
        let mut app = tone_app(2);
        press(&mut app, KeyCode::Enter);
        for _ in 0..at {
            press(&mut app, KeyCode::Tab);
        }
        typed(&mut app, text);
        assert_eq!(
            press(&mut app, KeyCode::Enter),
            vec![write(2, PARAMS[at], raw)],
            "{text}"
        );
    }
}

/// `Esc` closes the line with nothing written, and the next line opens on the row again.
#[test]
fn esc_writes_nothing() {
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    typed(&mut app, "200");
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "3");
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert!(app.tone.edit_line.is_none());
    assert_eq!(app.screen, Screen::Tone, "Esc left the tone-control screen");
    press(&mut app, KeyCode::Enter);
    assert_eq!(texts(&app), ["160", "+0.0", "4.30"]);
}

/// The line's writes are a band edit like any other: in Flat mode the tone control goes to
/// EQ after them and the key line says so; in Loudness mode the mode is left alone; and a
/// line that writes nothing switches nothing.
#[test]
fn in_flat_mode_the_write_switches_the_tone_control_to_eq() {
    let three = write(2, EqParam::Gain, eq().gain_raw(3.0));
    let mut app = tone_app(1);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "3");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![three.clone(), Intent::EqMode("eq".into())]
    );
    assert_eq!(
        app.toast().map(|toast| toast.text.as_str()),
        Some(SWITCHED_TO_EQ)
    );

    let mut app = tone_app(3);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "3");
    assert_eq!(press(&mut app, KeyCode::Enter), vec![three]);
    assert!(app.toast().is_none());

    let mut app = tone_app(1);
    press(&mut app, KeyCode::Enter);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.toast().is_none());
}

/// While a preset's load is still being answered the line works from the steps the load
/// asked for, not from the readings it is about to replace: it opens on them, a number on
/// one of them says `already at`, and a number the reading already holds is written, so
/// the band ends where the line was told rather than where the load put it.
#[test]
fn during_a_load_the_line_works_from_the_steps_the_load_asked_for() {
    let eq = eq();
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Char('p'));
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    let recall = press(&mut app, KeyCode::Enter);
    assert!(
        app.readings
            .loading
            .as_ref()
            .is_some_and(|loading| loading.name == "Bass"),
        "{recall:?}"
    );
    // Bass asks band 2 for 160 Hz, +2.0 dB and Q 0.70; the device still reads
    // 160 Hz, +0.0 dB and Q 4.30.
    let (lifted, q) = (eq.gain_raw(2.0), eq.q_raw(0.7));
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        texts(&app),
        [
            "160".to_string(),
            gain_text(eq.gain_db(lifted)),
            q_text(eq.q(q))
        ]
    );

    press(&mut app, KeyCode::Tab);
    typed(&mut app, "2");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(
        line(&app).already.as_deref(),
        Some(format!("already at {} dB", gain_text(eq.gain_db(lifted))).as_str())
    );
    assert!(line(&app).refusal.is_none());
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "0");
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![write(2, EqParam::Gain, eq.gain_zero)],
        "a gain the reading holds but the load does not is written"
    );
    assert!(app.tone.edit_line.is_none());
}

/// The row's gain reads to the hundredth a step lands on, and a whole number of dB to the
/// tenth it always did.
#[test]
fn a_gain_reads_to_the_hundredth_and_a_whole_one_to_the_tenth() {
    let eq = eq();
    for (raw, wanted) in [
        (98, "+0.0"),
        (97, "-0.20"),
        (99, "+0.21"),
        (105, "+1.45"),
        (127, "+6.0"),
        (0, "-20.0"),
        (49, "-10.0"),
        (47, "-10.41"),
    ] {
        assert_eq!(gain_text(eq.gain_db(raw)), wanted, "raw {raw}");
    }
    assert_eq!(gain_text(-0.0), "+0.0");
}

/// The cells of the response curve in one frame of `app`, one string per row, with the edit
/// line's own row and the band table left out of it.
fn curve_cells(app: &App) -> Vec<String> {
    let area = Rect::new(0, 0, 80, 24);
    let curve = screens::tone::curve_of(area, app).expect("a curve is drawn");
    let buffer = crate::tui::test_support::frame_with_theme(app, 80, 24, &Theme::new(false));
    (curve.y..curve.y + curve.height)
        .map(|y| {
            (curve.x..curve.x + curve.width)
                .map(|x| buffer.cell((x, y)).expect("a curve cell").symbol())
                .collect()
        })
        .collect()
}

/// Where each band's point is drawn in a frame of `app`.
fn band_points(app: &App) -> Vec<(usize, (u16, u16))> {
    let curve = screens::tone::curve_of(Rect::new(0, 0, 80, 24), app).expect("a curve is drawn");
    screens::tone::points(app, curve)
}

/// The app with band 2's gain read at raw step `raw`, as a line previewing that step should
/// draw it.
fn read_at(mode: i64, raw: u8) -> App {
    let mut app = tone_app(mode);
    hear(&mut app, "12.2.3", i64::from(raw));
    app
}

/// `↑` in the gain field moves the curve and the band's point on it, with nothing asked of
/// the device: no intent, no write pending.
#[test]
fn an_arrow_in_the_line_moves_the_graph_and_asks_for_nothing() {
    let mut app = tone_app(2);
    let (cells, points) = (curve_cells(&app), band_points(&app));
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    for _ in 0..29 {
        assert!(press(&mut app, KeyCode::Up).is_empty());
    }
    assert_ne!(curve_cells(&app), cells, "the curve did not move");
    assert_ne!(band_points(&app), points, "the point did not move");
    assert_eq!(band_points(&app), band_points(&read_at(2, 127)));
    assert!(app.readings.pending.is_empty(), "something was written");
    assert_eq!(texts(&app)[1], "+6.0");
}

/// A number typed into a field is previewed at the step `Enter` would write for it.
#[test]
fn a_typed_number_previews_at_the_step_enter_writes() {
    let eq = eq();
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "3.3");
    let raw = eq.gain_raw(3.3);
    let reference = read_at(2, raw);
    assert_eq!(band_points(&app), band_points(&reference));
    assert_eq!(curve_cells(&app), curve_cells(&reference));
    assert_eq!(app.clip_peak(), reference.clip_peak());
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![write(2, EqParam::Gain, raw)]
    );
}

/// A text that is refused or incomplete leaves the field at its last good preview, or at what
/// it opened on: the drawn curve and the peak both stay.
#[test]
fn a_refused_or_incomplete_text_keeps_the_last_good_preview() {
    let eq = eq();
    let mut app = tone_app(2);
    let opened = (curve_cells(&app), app.clip_peak());
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "-");
    assert_eq!((curve_cells(&app), app.clip_peak()), opened, "incomplete");
    typed(&mut app, "x");
    assert_eq!((curve_cells(&app), app.clip_peak()), opened, "refused");

    let good = read_at(2, eq.gain_raw(4.0));
    let previewed = (curve_cells(&good), good.clip_peak());
    assert!(previewed.1.is_some());
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    typed(&mut app, "4");
    assert_eq!((curve_cells(&app), app.clip_peak()), previewed, "4");
    for (text, why) in [("-", "incomplete"), ("", "empty"), ("99", "out of range")] {
        for _ in 0..4 {
            press(&mut app, KeyCode::Backspace);
        }
        typed(&mut app, text);
        assert_eq!(
            (curve_cells(&app), app.clip_peak()),
            previewed,
            "{why}: {text:?}"
        );
    }
}

/// A field left as it opened previews nothing of its own, so a report for its band that
/// arrives while the line is open moves the graph; typed back to what it opened on, the same.
#[test]
fn an_untouched_field_does_not_freeze_the_graph() {
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    hear(&mut app, "12.2.3", 127);
    assert_eq!(curve_cells(&app), curve_cells(&read_at(2, 127)));
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "1");
    press(&mut app, KeyCode::Backspace);
    press(&mut app, KeyCode::Backspace);
    typed(&mut app, "+6.0");
    hear(&mut app, "12.2.3", 98);
    assert_eq!(curve_cells(&app), curve_cells(&read_at(2, 127)));
}

/// A write that never lands is dropped after its lifetime, and the peak is the readings' again
/// rather than the step the line wrote.
#[test]
fn a_write_the_device_never_answers_leaves_the_peak_on_the_readings() {
    let mut app = read_at(2, 127);
    let peak = app.clip_peak();
    assert!(peak.is_some());
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "0");
    assert!(!press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(app.clip_peak(), None, "the written step is drawn");
    app.tick(std::time::Instant::now() + crate::tui::app::ANSWER_LIFETIME * 2);
    assert!(app.readings.pending.is_empty());
    assert_eq!(app.clip_peak(), peak);
}

/// `Esc` puts the graph back exactly as it was drawn before the line opened.
#[test]
fn esc_puts_the_graph_back() {
    let mut app = read_at(2, 120);
    let before = crate::tui::test_support::frame_with_theme(&app, 80, 24, &Theme::new(false));
    let peak = app.clip_peak();
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "-8");
    assert_ne!(app.clip_peak(), peak);
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    let after = crate::tui::test_support::frame_with_theme(&app, 80, 24, &Theme::new(false));
    assert_eq!(after, before);
    assert_eq!(app.clip_peak(), peak);
}

/// `Enter` writes as it did, and the curve, the point and the peak are the same just before
/// the line closes and just after.
#[test]
fn enter_closes_the_line_with_no_jump_in_the_graph() {
    let mut app = tone_app(2);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "5");
    let eq = eq();
    let (cells, points, peak) = (curve_cells(&app), band_points(&app), app.clip_peak());
    assert!(peak.is_some());
    let written = press(&mut app, KeyCode::Enter);
    assert_eq!(written, vec![write(2, EqParam::Gain, eq.gain_raw(5.0))]);
    assert!(app.tone.edit_line.is_none());
    assert_eq!(curve_cells(&app), cells);
    assert_eq!(band_points(&app), points);
    assert_eq!(app.clip_peak(), peak);
}

/// The peak note under the band table follows the preview.
#[test]
fn the_peak_note_follows_the_preview() {
    let mut app = tone_app(2);
    let note = |app: &App| {
        crate::tui::test_support::text(&crate::tui::test_support::frame_with_theme(
            app,
            80,
            24,
            &Theme::new(false),
        ))
        .join("\n")
    };
    assert!(!note(&app).contains("peaks at"));
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "6");
    assert!(app.clip_peak().is_some_and(|peak| peak > 5.9));
    assert!(note(&app).contains("peaks at +6.0 dB"), "{}", note(&app));
    press(&mut app, KeyCode::Esc);
    assert!(app.clip_peak().is_none());
    assert!(!note(&app).contains("peaks at"));
}

/// In Flat mode the muted curve the stored bands describe moves too.
#[test]
fn in_flat_mode_the_muted_curve_moves() {
    let mut app = tone_app(1);
    let cells = curve_cells(&app);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Tab);
    typed(&mut app, "6");
    assert_ne!(curve_cells(&app), cells);
    assert_eq!(curve_cells(&app), curve_cells(&read_at(1, 127)));
    press(&mut app, KeyCode::Esc);
    assert_eq!(curve_cells(&app), cells);
}
