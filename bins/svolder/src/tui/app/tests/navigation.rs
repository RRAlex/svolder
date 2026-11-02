// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Moving between panes, screens and rows, the devices screen, and what each screen remembers
//! across a visit.

use super::*;

/// Type `text` into the editor, letter by letter.
fn type_into_editor(app: &mut App, text: &str) {
    for letter in text.chars() {
        ch(app, letter);
    }
}

#[test]
fn tab_cycles_panes_and_e_t_d_switch_screens_and_esc_returns() {
    let mut app = app();
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.pane, Pane::Tone);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.tone.band, 1, "the Tone pane moves through the bands");
    press(&mut app, KeyCode::Tab);
    assert_eq!(
        app.pane,
        Pane::Controls,
        "Tab off the last pane wraps, past the Status pane, which is never focused"
    );
    ch(&mut app, 'e');
    assert_eq!(app.screen, Screen::Tone);
    ch(&mut app, 't');
    assert_eq!(app.screen, Screen::Telemetry);
    ch(&mut app, 'd');
    assert_eq!(
        app.screen,
        Screen::Devices,
        "the list is where a device is added as well as switched to"
    );
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, Screen::Device);
}

/// Shift-Tab walks the panes the other way round from `Tab` and comes back to where it
/// started; a device with no tone control has only the Controls pane, which it leaves focused.
#[test]
fn shift_tab_cycles_the_panes_backwards() {
    let mut app = app();
    for pane in [Pane::Tone, Pane::Controls] {
        app.key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
        assert_eq!(app.pane, pane, "Shift-Tab did not step back to {pane:?}");
    }

    let mut no_eq = App::new(
        Registry::builtin()
            .expect("the built-in profiles load")
            .get("lyr5")
            .expect("lyr5 is built in")
            .clone(),
        "lyr5".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    no_eq.key(KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT));
    assert_eq!(no_eq.pane, Pane::Controls, "no tone control");
    let tab = keys::all()
        .iter()
        .find(|hint| hint.code == KeyCode::Tab && hint.screens.contains(&Screen::Device))
        .expect("the table names Tab on the device screen");
    assert!(
        !keys::available(tab, &no_eq),
        "Tab is offered with no second pane to go to"
    );
    assert!(
        keys::available(tab, &app),
        "Tab is not offered beside a Tone pane"
    );
}

/// A screen with only the one pane has no cycle: `Tab` and Shift-Tab leave the keys
/// exactly where they were, and raise no light for a focus that never moved.
#[test]
fn tab_does_nothing_on_a_one_pane_screen() {
    for screen in [Screen::Telemetry, Screen::Devices] {
        let mut app = app();
        app.show(screen);
        app.pane = app.panes()[0];
        for code in [KeyCode::Tab, KeyCode::BackTab] {
            app.frame_flags.pane_changed = false;
            press(&mut app, code);
            assert_eq!(
                app.pane,
                app.panes()[0],
                "{screen:?} {code:?} moved the pane"
            );
            assert!(
                !app.frame_flags.pane_changed,
                "{screen:?} {code:?} raised the light"
            );
        }
    }
}

/// The cursor is wherever the keys are acting, which on the device screen is the focused
/// pane's own row: the Controls pane's selection, the Tone pane's band, and nothing at all
/// in the Status pane, whose lines no key moves through. Moving between bands in the Tone
/// pane leaves a row behind like any other move.
#[test]
fn the_cursor_follows_the_focused_pane_on_the_device_screen() {
    let mut app = app();
    let eq = app
        .profile
        .eq
        .clone()
        .expect("the profile has tone control");
    let band = |n: usize| eq.band_id(n, EqParam::Gain);
    assert_eq!(
        app.cursor_id(),
        app.selected_control().map(|control| control.id),
        "the Controls pane's own selection"
    );

    press(&mut app, KeyCode::Tab);
    assert_eq!(app.pane, Pane::Tone);
    assert_eq!(app.cursor_id(), band(0), "the Tone pane's focused band");
    app.frame_flags.left_row = None;
    press(&mut app, KeyCode::Down);
    assert_eq!(app.cursor_id(), band(1), "and its next band");
    assert_eq!(app.frame_flags.left_row, band(0), "the band it left trails");

    press(&mut app, KeyCode::Tab);
    assert_eq!(app.pane, Pane::Controls);
    assert_eq!(
        app.cursor_id(),
        app.selected_control().map(|control| control.id),
        "back on the Controls pane's own selection"
    );
    assert_eq!(
        app.frame_flags.left_row,
        band(1),
        "the band the focus left trails"
    );

    ch(&mut app, 'e');
    assert_eq!(app.screen, Screen::Tone);
    assert_eq!(
        app.cursor_id(),
        band(1),
        "the tone screen's cursor is its focused band, whatever pane was focused"
    );
}

#[test]
fn enter_on_the_device_list_switches_device() {
    let mut app = app();
    app.devices.rows.push(row("bifrost"));
    ch(&mut app, 'd');
    assert_eq!(app.screen, Screen::Devices);
    press(&mut app, KeyCode::Down);
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::SwitchDevice("bifrost".into())]
    );
    press(&mut app, KeyCode::Up);
    assert_eq!(
        press(&mut app, KeyCode::Enter),
        vec![Intent::SwitchDevice("gungnir".into())]
    );
}

/// `u` opens the editor on the focused device and `Enter` asks for what it holds to be
/// written over the block it came from; `x` asks first and `Enter` answers.
#[test]
fn n_u_and_x_on_the_devices_screen_produce_their_intents() {
    let mut app = listing();
    assert_eq!(ch(&mut app, 'n'), vec![Intent::NewDevice]);

    ch(&mut app, 'u');
    let editor = app.devices.editor.as_ref().expect("u opens the editor");
    assert_eq!(editor.original, "gungnir");
    // The name row opens with the cursor at the end of what is there.
    for _ in 0..7 {
        press(&mut app, KeyCode::Backspace);
    }
    type_into_editor(&mut app, "dac");
    let intents = press(&mut app, KeyCode::Enter);
    assert!(
        matches!(&intents[0], Intent::UpdateDevice(old, device)
                if old == "gungnir" && device.name == "dac" && device.udid == "0102030405060708"),
        "got {intents:?}"
    );
    assert!(
        app.devices.editor.is_none(),
        "the editor stays open after a save"
    );

    ch(&mut app, 'x');
    assert_eq!(app.devices.confirm.as_deref(), Some("gungnir"));
    let intents = press(&mut app, KeyCode::Enter);
    assert!(
        matches!(&intents[0], Intent::RemoveDevice(name) if name == "gungnir"),
        "got {intents:?}"
    );
    assert!(app.devices.confirm.is_none());
}

/// Have `app` hear one unwritable reading per id, in the order the screen will list them.
fn hear_unwritable(app: &mut App, ids: &[&str]) {
    for id in ids {
        hear(app, raw_vo(id, "u8", 5));
    }
}

/// The reading the telemetry screen has focused.
fn focused_reading(app: &App) -> String {
    app.telemetry_focus()
        .expect("a focused reading")
        .to_string()
}

/// Leaving the device screen for another and coming back finds the row it was on, through
/// telemetry and through the tone screen.
#[test]
fn the_device_screen_keeps_its_row_across_a_visit() {
    for key in ['t', 'e'] {
        let mut app = app();
        for _ in 0..3 {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(app.selected, 3);
        ch(&mut app, key);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen, Screen::Device, "{key}");
        assert_eq!(app.selected, 3, "{key}");
    }
}

/// A reading that arrives for the first time above the focused one leaves the highlight on
/// the reading it was on, and the arrows still nudge that reading.
#[test]
fn a_new_lower_reading_leaves_the_telemetry_highlight_on_its_reading() {
    let mut app = app();
    hear_unwritable(&mut app, &["90.01", "90.02", "90.3", "90.04"]);
    ch(&mut app, 't');
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(focused_reading(&app), "90.3");
    hear_unwritable(&mut app, &["89.01"]);
    assert_eq!((app.selected, focused_reading(&app).as_str()), (3, "90.3"));
    let intents = press(&mut app, KeyCode::Right);
    assert!(
        matches!(&intents[..], [Intent::Raw { id, value, .. }] if id == "90.3" && value == "6"),
        "{intents:?}"
    );
    // A reading below the focused one moves nothing.
    hear_unwritable(&mut app, &["91.01"]);
    assert_eq!(focused_reading(&app), "90.3");
}

/// Coming back to telemetry finds the same reading, wherever it sits by then.
#[test]
fn telemetry_comes_back_to_the_same_reading() {
    let mut app = app();
    hear_unwritable(&mut app, &["90.01", "90.02", "90.3"]);
    ch(&mut app, 't');
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Esc);
    hear_unwritable(&mut app, &["89.01"]);
    ch(&mut app, 't');
    assert_eq!((app.selected, focused_reading(&app).as_str()), (3, "90.3"));
}

/// A refresh that leaves out the focused reading puts the highlight on the first row.
#[test]
fn a_refresh_without_the_focused_reading_focuses_the_first_row() {
    let mut app = app();
    hear_unwritable(&mut app, &["90.01", "90.02", "90.3"]);
    ch(&mut app, 't');
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(focused_reading(&app), "90.3");
    app.replace_all(
        vec![raw_vo("90.01", "u8", 5), raw_vo("90.02", "u8", 5)],
        Instant::now(),
    );
    assert_eq!(app.selected, 0);
}

/// A remembered row past the end of a list that has shrunk since lands on the last row.
#[test]
fn a_remembered_row_past_a_shorter_list_lands_on_its_last_row() {
    let mut app = app();
    for name in ["a", "b", "c"] {
        app.devices.rows.push(row(name));
    }
    ch(&mut app, 'd');
    for _ in 0..3 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(app.selected, 3);
    press(&mut app, KeyCode::Esc);
    app.devices.rows.truncate(2);
    app.show(Screen::Devices);
    assert_eq!(app.selected, 1);
}

/// A device switch starts every screen at its first row.
#[test]
fn a_device_switch_clears_what_each_screen_remembered() {
    let mut app = app();
    app.devices.rows.push(row("bifrost"));
    hear_unwritable(&mut app, &["90.01", "90.02"]);
    ch(&mut app, 't');
    press(&mut app, KeyCode::Down);
    ch(&mut app, 's');
    press(&mut app, KeyCode::Down);
    ch(&mut app, 'd');
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Esc);
    for _ in 0..2 {
        press(&mut app, KeyCode::Down);
    }
    app.settle_on(Screen::Device);
    assert_eq!(app.selected, 0);
    for screen in [Screen::Telemetry, Screen::Devices, Screen::Tone] {
        app.show(screen);
        assert_eq!(app.selected, 0, "{screen:?}");
    }
}

/// Closing the Settings box and opening it again with `s` finds the row it was on, and the
/// cursor of the screen under it is untouched.
#[test]
fn settings_comes_back_to_its_row() {
    let mut app = app();
    let under = app.selected;
    ch(&mut app, 's');
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Esc);
    ch(&mut app, 's');
    assert_eq!(
        app.overlays.settings,
        Some(crate::tui::app::SettingsBox {
            category: 0,
            row: 2
        })
    );
    assert_eq!(app.selected, under);
}

/// `c` shows the Controls screen from every other screen the way `e`, `t` and `d` show theirs.
#[test]
fn c_goes_to_the_controls_screen_from_every_screen() {
    for screen in [Screen::Tone, Screen::Telemetry, Screen::Devices] {
        let mut app = app();
        app.show(screen);
        press(&mut app, KeyCode::Char('c'));
        assert_eq!(app.screen, Screen::Device, "from {screen:?}");
    }
}

/// On the device list the editor opens on `u`, and `c` leaves for the Controls screen instead.
#[test]
fn the_device_editor_opens_on_u_and_c_no_longer_does() {
    let mut app = crate::tui::test_support::app_showing(Screen::Devices);
    press(&mut app, KeyCode::Char('u'));
    assert!(app.devices.editor.is_some(), "u did not open the editor");
    let mut app = crate::tui::test_support::app_showing(Screen::Devices);
    press(&mut app, KeyCode::Char('c'));
    assert!(app.devices.editor.is_none(), "c still opens the editor");
    assert_eq!(app.screen, Screen::Device);
}
