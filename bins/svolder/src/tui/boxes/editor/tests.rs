// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::KeyCode;

use super::*;
use crate::tui::app::Screen;
use crate::tui::test_support::app_showing;
use crate::tui::test_support::{press_from as press, type_text};
use crate::tui::widgets::passphrase_row::Mode;

/// An app on the device list, with `gungnir` focused and the two devices the configuration
/// file names.
fn listing() -> App {
    app_showing(Screen::Devices)
}

/// An app with the editor open on the focused device.
fn editing() -> App {
    let mut app = listing();
    press(&mut app, KeyCode::Char('u'));
    assert!(app.devices.editor.is_some(), "the editor did not open");
    app
}

/// Take the whole of a row's line back.
fn clear(app: &mut App, len: usize) {
    for _ in 0..len {
        press(app, KeyCode::Backspace);
    }
}

/// Move the cursor to `row`, from the row the editor opens on.
fn focus(app: &mut App, row: EditorRow) {
    let at = EDITOR_ROWS
        .iter()
        .position(|other| *other == row)
        .expect("a row of the editor");
    for _ in 0..at {
        press(app, KeyCode::Down);
    }
    assert_eq!(
        app.devices
            .editor
            .as_ref()
            .expect("the editor is open")
            .focused(),
        row
    );
}

/// The device one save asked to be written.
fn saved(intents: &[Intent]) -> DeviceConfig {
    match intents.first() {
        Some(Intent::UpdateDevice(_, device)) => device.clone(),
        other => panic!("expected a device to be written, got {other:?}"),
    }
}

/// While the editor is open every letter is a letter: the keys that would switch screen or
/// leave are typed into the name, and `Esc` leaves the device as the file has it.
#[test]
fn the_editor_takes_every_key_and_esc_writes_nothing() {
    let mut app = editing();
    type_text(&mut app, "qest");
    assert_eq!(
        app.devices
            .editor
            .as_ref()
            .expect("the editor is open")
            .device
            .name,
        "gungnirqest"
    );
    assert_eq!(app.screen, Screen::Devices, "a letter switched screen");
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert!(app.devices.editor.is_none(), "Esc did not close the editor");
}

/// The editor refuses what the file would refuse, where it was typed: a name another device
/// carries, and a blank one.
#[test]
fn the_editor_refuses_a_taken_name_and_a_blank_one() {
    let mut app = editing();
    clear(&mut app, "gungnir".len());
    type_text(&mut app, "bifrost");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(
        app.devices.editor.is_some(),
        "the editor closed on a refused name"
    );
    assert_eq!(
        app.toast().map(|toast| toast.text.as_str()),
        Some("another device is called bifrost")
    );

    clear(&mut app, "bifrost".len());
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(
        app.toast().map(|toast| toast.text.as_str()),
        Some(NEEDS_A_NAME)
    );

    type_text(&mut app, "dac");
    let intents = press(&mut app, KeyCode::Enter);
    assert!(
        matches!(&intents[0], Intent::UpdateDevice(old, device)
                if old == "gungnir" && device.name == "dac"
                    && device.udid == "0102030405060708"),
        "got {intents:?}"
    );
}

/// The editor's passphrase row.
fn row_of(app: &App) -> &PassphraseRow {
    &app.devices
        .editor
        .as_ref()
        .expect("the editor is open")
        .passphrase
}

/// An app with the editor open on a device whose file says `passphrase`, and the cursor on its
/// passphrase row.
fn editing_passphrase(passphrase: &str) -> App {
    let mut app = listing();
    app.devices.configured[0].passphrase = passphrase.into();
    press(&mut app, KeyCode::Char('u'));
    focus(&mut app, EditorRow::Passphrase);
    app
}

/// The row opens on what the file says: `auto`, `none`, or the code with its digits.
#[test]
fn the_passphrase_row_opens_on_what_the_file_says() {
    for (file, shown) in [("auto", "auto"), ("none", "none"), ("4321", "4321")] {
        let app = editing_passphrase(file);
        assert_eq!(row_of(&app).shown(), shown, "file says {file}");
    }
    assert_eq!(row_of(&editing_passphrase("4321")).mode(), Mode::Code);
}

/// `←→` step round `none`, `auto`, the code and back to `none`, wrapping at both ends as the
/// Profile row does, and the file carries what the row is on when it is saved.
#[test]
fn the_arrows_cycle_the_passphrase_row_and_wrap() {
    let mut app = editing_passphrase("none");
    let mut seen = Vec::new();
    for _ in 0..4 {
        seen.push(row_of(&app).mode());
        press(&mut app, KeyCode::Right);
    }
    assert_eq!(seen, [Mode::None, Mode::Auto, Mode::Code, Mode::None]);
    // Four steps on from none is auto; two back is none again, and one more wraps to the code.
    press(&mut app, KeyCode::Left);
    assert_eq!(row_of(&app).mode(), Mode::None);
    press(&mut app, KeyCode::Left);
    assert_eq!(
        row_of(&app).mode(),
        Mode::Code,
        "back from none wraps round"
    );

    let mut app = editing_passphrase("auto");
    press(&mut app, KeyCode::Right);
    type_text(&mut app, "2468");
    assert_eq!(saved(&press(&mut app, KeyCode::Enter)).passphrase, "2468");
    let mut app = editing_passphrase("2468");
    press(&mut app, KeyCode::Right);
    assert_eq!(saved(&press(&mut app, KeyCode::Enter)).passphrase, "none");
    let mut app = editing_passphrase("2468");
    press(&mut app, KeyCode::Left);
    assert_eq!(saved(&press(&mut app, KeyCode::Enter)).passphrase, "auto");
}

/// In code mode only digits are typed, four of them in order, with no caret: `←→` stay free to
/// cycle, and `Backspace` takes back the last digit.
#[test]
fn the_code_takes_only_digits_in_order_and_backspace_takes_one_back() {
    let mut app = editing_passphrase("auto");
    press(&mut app, KeyCode::Right);
    assert_eq!(row_of(&app).shown(), "____");
    type_text(&mut app, "1a2-3 456");
    assert_eq!(
        row_of(&app).shown(),
        "1234",
        "letters and a fifth digit are ignored"
    );
    press(&mut app, KeyCode::Backspace);
    assert_eq!(row_of(&app).shown(), "123_");
    press(&mut app, KeyCode::Home);
    type_text(&mut app, "9");
    assert_eq!(
        row_of(&app).shown(),
        "1239",
        "Home placed no caret: digits go on at the end"
    );
}

/// A digit typed on `none` or `auto` switches to the code and starts it with that digit.
#[test]
fn a_digit_on_none_or_auto_starts_the_code() {
    for file in ["none", "auto"] {
        let mut app = editing_passphrase(file);
        type_text(&mut app, "7");
        assert_eq!(row_of(&app).mode(), Mode::Code, "from {file}");
        assert_eq!(row_of(&app).shown(), "7___", "from {file}");
        type_text(&mut app, "5");
        assert_eq!(row_of(&app).shown(), "75__");
    }
}

/// Stepping away from the code keeps the digits typed, until the editor saves or `Esc` cancels:
/// stepping back finds them again.
#[test]
fn stepping_away_from_the_code_keeps_its_digits() {
    let mut app = editing_passphrase("auto");
    press(&mut app, KeyCode::Right);
    type_text(&mut app, "12");
    press(&mut app, KeyCode::Right);
    assert_eq!(row_of(&app).shown(), "none");
    press(&mut app, KeyCode::Left);
    assert_eq!(row_of(&app).shown(), "12__");
    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    assert_eq!(row_of(&app).shown(), "auto");
    assert_eq!(saved(&press(&mut app, KeyCode::Enter)).passphrase, "auto");
    // The digits went with the editor: the next one opens on the file.
    let app = editing_passphrase("auto");
    assert_eq!(row_of(&app).digits(), "");
}

/// `Enter` with fewer than four digits in the code is refused in the Failure toast, with the
/// editor still open and nothing written.
#[test]
fn enter_with_a_partial_code_is_refused_where_it_was_typed() {
    let mut app = editing_passphrase("auto");
    press(&mut app, KeyCode::Right);
    type_text(&mut app, "12");
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(
        app.toast().map(|toast| toast.text.as_str()),
        Some("a code is four digits")
    );
    assert!(
        app.devices.editor.is_some(),
        "the editor closed on a refused row"
    );
    type_text(&mut app, "34");
    assert_eq!(saved(&press(&mut app, KeyCode::Enter)).passphrase, "1234");
}

/// The default-device row writes the key beside the device, and turning it off again for the
/// device the file names clears it. A rename carries the key itself, so the row asks for
/// nothing where it has not changed.
#[test]
fn the_default_device_row_writes_and_clears_the_key() {
    let mut app = editing();
    focus(&mut app, EditorRow::Default);
    press(&mut app, KeyCode::Char(' '));
    let intents = press(&mut app, KeyCode::Enter);
    assert_eq!(
        intents[1],
        Intent::DefaultDevice(Some("gungnir".into())),
        "got {intents:?}"
    );

    // The file names this device, spelled its own way: turning the row off clears the key.
    app.devices.default_device = Some("Gungnir".into());
    press(&mut app, KeyCode::Char('u'));
    focus(&mut app, EditorRow::Default);
    press(&mut app, KeyCode::Left);
    let intents = press(&mut app, KeyCode::Enter);
    assert_eq!(intents[1], Intent::DefaultDevice(None), "got {intents:?}");

    // Renaming the device the file defaults to asks for nothing of its own: the writer
    // carries the key along with the block.
    press(&mut app, KeyCode::Char('u'));
    clear(&mut app, "gungnir".len());
    type_text(&mut app, "dac");
    let intents = press(&mut app, KeyCode::Enter);
    assert_eq!(intents.len(), 1, "got {intents:?}");

    // And a device that is not the default and is not being made one leaves the key alone.
    app.devices.default_device = None;
    press(&mut app, KeyCode::Char('u'));
    assert_eq!(press(&mut app, KeyCode::Enter).len(), 1);
}

/// The profile row steps through the profiles that name the product the device advertises,
/// with the one svolder detects for itself first.
#[test]
fn the_profile_row_steps_through_the_profiles_that_match_the_product() {
    let mut app = listing();
    app.devices.rows[0].product = Some("Gungnir 2".into());
    press(&mut app, KeyCode::Char('u'));
    focus(&mut app, EditorRow::Profile);
    let editor = app.devices.editor.as_ref().expect("the editor is open");
    assert_eq!(editor.profile(), AUTOMATIC);
    assert!(
        editor.profiles.iter().any(|name| name == "gungnir2-v2"),
        "the profiles for a Gungnir 2 are not offered: {:?}",
        editor.profiles
    );
    press(&mut app, KeyCode::Right);
    let editor = app.devices.editor.as_ref().expect("the editor is open");
    assert_ne!(editor.profile(), AUTOMATIC, "the row did not step");
    assert!(saved(&press(&mut app, KeyCode::Enter)).profile.is_some());
}

/// A profile the list does not hold is the row's own entry, and stepping starts from it
/// rather than jumping to the first choice.
#[test]
fn a_pinned_profile_the_list_does_not_hold_is_offered_as_it_stands() {
    let mut app = listing();
    app.devices.configured[0].profile = Some("mine".into());
    press(&mut app, KeyCode::Char('u'));
    focus(&mut app, EditorRow::Profile);
    let editor = app.devices.editor.as_ref().expect("the editor is open");
    assert_eq!(editor.profile(), "mine");
    assert_eq!(
        editor.profiles.last().map(String::as_str),
        Some("mine"),
        "the pinned profile is not among the choices: {:?}",
        editor.profiles
    );

    // The list runs round, so one step back from the last entry is the one before it and
    // one step on is the first.
    press(&mut app, KeyCode::Right);
    assert_eq!(
        app.devices
            .editor
            .as_ref()
            .expect("the editor is open")
            .profile(),
        AUTOMATIC
    );
}
