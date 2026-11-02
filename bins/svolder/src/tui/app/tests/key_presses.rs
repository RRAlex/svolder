// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the keys do on each screen: rows, toggles, nudges, the quit question, the help overlay
//! and the three key rules.

use super::*;

#[test]
fn down_moves_the_selection_and_stops_at_the_last_row() {
    let mut app = app();
    let last = app.rows().len() - 1;
    for _ in 0..10 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(app.selected, last);
    press(&mut app, KeyCode::Up);
    assert_eq!(app.selected, last - 1);
    for _ in 0..10 {
        press(&mut app, KeyCode::Up);
    }
    assert_eq!(app.selected, 0, "and stops at the first");
}

#[test]
fn right_on_a_select_sends_the_next_value_and_left_the_previous() {
    let mut app = app();
    hear(&mut app, vo("2", Some("input"), json!(1), "1"));
    assert_eq!(app.selected, 0, "the Input row starts focused");
    assert_eq!(press(&mut app, KeyCode::Right), vec![set("input", "2")]);
    assert!(
        press(&mut app, KeyCode::Left).is_empty(),
        "raw 1 is the lowest value the profile names"
    );
    hear(&mut app, vo("2", Some("input"), json!(5), "5"));
    assert_eq!(press(&mut app, KeyCode::Left), vec![set("input", "4")]);
    assert!(
        press(&mut app, KeyCode::Right).is_empty(),
        "raw 5 is the highest"
    );
}

#[test]
fn left_and_right_on_a_level_or_toggle_row_of_the_controls_pane() {
    let mut app = app();
    hear(&mut app, vo("10", Some("volume"), json!(100), "100"));
    app.selected = row_of(&app, "volume");
    assert_eq!(press(&mut app, KeyCode::Right), vec![set("volume", "up")]);
    assert_eq!(press(&mut app, KeyCode::Left), vec![set("volume", "down")]);

    hear(&mut app, vo("5", Some("mute"), json!(false), "off"));
    app.selected = row_of(&app, "mute");
    assert_eq!(press(&mut app, KeyCode::Right), vec![set("mute", "on")]);
    hear(&mut app, vo("5", Some("mute"), json!(true), "on"));
    assert_eq!(press(&mut app, KeyCode::Left), vec![set("mute", "off")]);
}

#[test]
fn space_on_a_toggle_flips_it() {
    let mut app = app();
    hear(&mut app, vo("5", Some("mute"), json!(false), "off"));
    app.selected = row_of(&app, "mute");
    assert_eq!(ch(&mut app, ' '), vec![set("mute", "on")]);
    hear(&mut app, vo("5", Some("mute"), json!(true), "on"));
    assert_eq!(ch(&mut app, ' '), vec![set("mute", "off")]);
    app.selected = row_of(&app, "volume");
    assert!(ch(&mut app, ' ').is_empty(), "a level has nothing to flip");
}

#[test]
fn space_flips_nothing_while_another_pane_is_focused() {
    let mut app = app();
    hear(&mut app, vo("5", Some("mute"), json!(false), "off"));
    app.selected = row_of(&app, "mute");
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.pane, Pane::Tone);
    assert!(
        ch(&mut app, ' ').is_empty(),
        "the Tone pane is not showing the row the selection points at"
    );
    assert!(
        press(&mut app, KeyCode::Left).is_empty(),
        "nor are its arrows"
    );
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.pane, Pane::Controls, "Tab off the last pane wraps");
    assert_eq!(ch(&mut app, ' '), vec![set("mute", "on")]);
}

#[test]
fn plus_and_minus_nudge_volume_wherever_the_selection_is() {
    let mut app = app();
    app.selected = row_of(&app, "input");
    assert_eq!(ch(&mut app, '+'), vec![set("volume", "up")]);
    assert_eq!(ch(&mut app, '-'), vec![set("volume", "down")]);
    app.pane = Pane::Status;
    assert_eq!(
        ch(&mut app, '+'),
        vec![set("volume", "up")],
        "a pane with no rows still takes the volume keys"
    );
}

#[test]
fn m_toggles_mute_from_any_row() {
    let mut app = app();
    app.selected = row_of(&app, "volume");
    assert_eq!(ch(&mut app, 'm'), vec![set("mute", "on")]);
    hear(&mut app, vo("5", Some("mute"), json!(true), "on"));
    assert_eq!(ch(&mut app, 'm'), vec![set("mute", "off")]);
}

/// `i` flips the phase and `o` the NOS, the way `m` flips the mute, from any row.
#[test]
fn i_flips_phase_and_o_flips_nos_from_any_row() {
    let mut app = app();
    app.selected = row_of(&app, "volume");
    assert_eq!(ch(&mut app, 'i'), vec![set("phase", "on")]);
    assert_eq!(ch(&mut app, 'o'), vec![set("nos", "on")]);
    hear(&mut app, vo("3", Some("phase"), json!(true), "on"));
    assert_eq!(ch(&mut app, 'i'), vec![set("phase", "off")]);
}

/// The two keys reach their control from every screen, the Telemetry and Tone screens among
/// them, as `m` does.
#[test]
fn i_and_o_work_on_every_screen() {
    for screen in [
        Screen::Device,
        Screen::Tone,
        Screen::Telemetry,
        Screen::Devices,
    ] {
        let mut app = listing();
        app.show(screen);
        assert!(
            matches!(&ch(&mut app, 'i')[0], Intent::Set { key, .. } if key == "phase"),
            "i is not phase on {screen:?}"
        );
        assert!(
            matches!(&ch(&mut app, 'o')[0], Intent::Set { key, .. } if key == "nos"),
            "o is not NOS on {screen:?}"
        );
    }
}

/// A device without the control ignores the key and the table says it is not worth pressing.
#[test]
fn i_and_o_do_nothing_on_a_device_without_the_control() {
    let mut app = App::new(
        Registry::builtin()
            .expect("the built-in profiles load")
            .get("lyr5")
            .expect("lyr5 is built in")
            .clone(),
        "lyr".into(),
        &TuiConfig::default(),
        vec![row("lyr")],
    );
    assert!(ch(&mut app, 'i').is_empty());
    assert!(ch(&mut app, 'o').is_empty());
    for (code, when) in [('i', keys::When::HasPhase), ('o', keys::When::HasNos)] {
        let hint = keys::all()
            .iter()
            .find(|hint| hint.code == KeyCode::Char(code))
            .expect("the table names the key");
        assert_eq!(hint.when, when);
        assert!(
            !keys::available(hint, &app),
            "{code} is available on a Lyr 5"
        );
    }
    let app = self::app();
    assert!(
        keys::all()
            .iter()
            .filter(|hint| matches!(hint.when, keys::When::HasPhase | keys::When::HasNos))
            .all(|hint| keys::available(hint, &app))
    );
}

/// A device whose profile offers no identify (a Mesh Card) has `b` dimmed in the table, with the
/// words that say why in the help overlay; on the Devices list it is the highlighted row's profile
/// that counts, whatever device is on show.
#[test]
fn b_is_dimmed_where_the_profile_offers_no_identify() {
    let identify = || {
        keys::all()
            .iter()
            .find(|hint| hint.code == KeyCode::Char('b'))
            .expect("the table names the key")
    };
    assert_eq!(identify().when, keys::When::CanIdentify);
    let registry = Registry::builtin().expect("the built-in profiles load");
    let card = registry.get("meshcard").expect("built in").clone();
    let app = App::new(
        card,
        "card".into(),
        &TuiConfig::default(),
        vec![row("card")],
    );
    assert!(!keys::available(identify(), &app));
    assert!(keys::available(identify(), &self::app()));

    let mut listing = self::app();
    listing.devices.rows = vec![
        DeviceRow {
            profile: Some("meshcard".into()),
            ..row("card")
        },
        DeviceRow {
            profile: Some("gungnir2-v2".into()),
            ..row("gungnir")
        },
    ];
    ch(&mut listing, 'd');
    listing.selected = 0;
    assert!(!keys::available(identify(), &listing));
    listing.selected = 1;
    assert!(keys::available(identify(), &listing));
}

/// `=` is `+` without the Shift most layouts put `+` behind: it raises the volume from
/// any row of the device screen, as `+` does.
#[test]
fn equals_raises_the_volume_like_plus() {
    let mut app = app();
    app.selected = row_of(&app, "input");
    assert_eq!(ch(&mut app, '='), vec![set("volume", "up")]);
    app.pane = Pane::Status;
    assert_eq!(ch(&mut app, '='), ch(&mut app, '+'));
}

/// On the tone-control screen each Shift+arrow is the symbol key it stands for — the
/// brackets for ↑↓, the comma and the full stop for ←→ — so a keyboard that reaches
/// those only through AltGr still steps a band's Q and frequency. Everywhere else a
/// Shift+arrow is the arrow, and an uppercase letter is the letter either way.
#[test]
fn shift_arrows_on_the_tone_screen_are_the_frequency_and_q_keys() {
    let mut device = app();
    let mut app = app();
    ch(&mut app, 'e');
    let now = Instant::now();
    feed(&mut app, raw_vo("12.1.2", "u8", 64), now);
    feed(&mut app, raw_vo("12.1.3", "u8", 98), now);
    feed(&mut app, raw_vo("12.1.1", "u8", 64), now);
    for (code, alias) in [
        (KeyCode::Up, '['),
        (KeyCode::Down, ']'),
        (KeyCode::Left, ','),
        (KeyCode::Right, '.'),
    ] {
        let shifted = app.key(KeyEvent::new(code, KeyModifiers::SHIFT));
        answered(&mut app);
        assert_eq!(
            shifted,
            ch(&mut app, alias),
            "Shift+{code:?} is not {alias}"
        );
        answered(&mut app);
        assert!(!shifted.is_empty(), "Shift+{code:?} asked for nothing");
    }
    let semitone = 2f64.powf(1.0 / 12.0);
    assert_eq_set(
        &app.key(KeyEvent::new(KeyCode::Up, KeyModifiers::SHIFT)),
        1,
        "q",
        4.3 / 1.1,
    );
    answered(&mut app);
    assert_eq_set(
        &app.key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT)),
        1,
        "f",
        40.0 * semitone,
    );
    assert_eq!(
        app.tone.row,
        ToneRow::Band,
        "a Shift+arrow moved the cursor"
    );
    assert_eq!(
        shift(&mut app, 'M'),
        vec![Intent::Tui(TuiChange::Mouse(false))],
        "an uppercase letter is still the letter on the tone screen"
    );

    device.selected = 0;
    device.key(KeyEvent::new(KeyCode::Down, KeyModifiers::SHIFT));
    assert_eq!(device.selected, 1, "Shift+↓ is ↓ on the device screen");
    device.selected = row_of(&device, "volume");
    assert_eq!(
        device.key(KeyEvent::new(KeyCode::Right, KeyModifiers::SHIFT)),
        vec![set("volume", "up")],
        "Shift+→ is → on the device screen"
    );
}

#[test]
fn q_r_mouse_advice_and_help_keys_work_from_every_screen() {
    let mut app = app();
    ch(&mut app, 't');
    assert!(ch(&mut app, 'q').is_empty(), "q quit without asking");
    assert!(app.overlays.quitting, "q did not ask");
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert_eq!(
        app.screen,
        Screen::Telemetry,
        "Esc answered more than the question"
    );
    assert_eq!(ch(&mut app, 'r'), vec![Intent::Refresh]);
    assert_eq!(
        shift(&mut app, 'M'),
        vec![Intent::Tui(TuiChange::Mouse(false))],
        "mouse capture starts on, so M asks for it off"
    );
    assert!(!app.notices.show_advice);
    ch(&mut app, 'a');
    assert!(
        !app.notices.show_advice,
        "no fault stands: a posts a toast instead"
    );
    assert!(app.toast().is_some());
    assert!(!app.overlays.help_open);
    ch(&mut app, '?');
    assert!(app.overlays.help_open);
}

/// Every chord with control or alt is a terminal habit that reaches nothing, but Ctrl-C:
/// raw mode keeps it from being a signal, so it is the key that quits, at once.
#[test]
fn a_control_chord_changes_nothing_but_ctrl_c_quits() {
    let mut app = app();
    for c in ['l', 'm', 'q'] {
        let event = KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL);
        assert!(app.key(event).is_empty(), "ctrl-{c} asked for a write");
    }
    let alt_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::ALT);
    assert!(app.key(alt_c).is_empty(), "alt-c asked for a write");
    assert_eq!(app.screen, Screen::Device);
    assert!(!app.overlays.quitting, "a chord opened the quit question");
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    assert_eq!(app.key(ctrl_c), vec![Intent::Quit]);
}

/// Ctrl-C quits past every box that takes the keys: the question `q` asks, the help
/// overlay, the menu, the Settings box, the device editor and the removal it asks about, the
/// preset list and its name prompt, and a band's edit line.
#[test]
fn ctrl_c_quits_past_every_modal_box() {
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    let mut asking = app();
    ch(&mut asking, 'q');
    let mut helping = app();
    ch(&mut helping, '?');
    let mut menu = app();
    press(&mut menu, KeyCode::Esc);
    let mut settings = app();
    ch(&mut settings, 's');
    let mut editing = crate::tui::test_support::app_showing(Screen::Devices);
    ch(&mut editing, 'u');
    let mut removing = crate::tui::test_support::app_showing(Screen::Devices);
    ch(&mut removing, 'x');
    let mut choosing = crate::tui::test_support::app_showing(Screen::Tone);
    ch(&mut choosing, 'p');
    let mut typing = crate::tui::test_support::app_showing(Screen::Tone);
    press(&mut typing, KeyCode::Enter);
    let mut naming = crate::tui::test_support::app_showing(Screen::Tone);
    ch(&mut naming, 'p');
    ch(&mut naming, 'n');
    assert!(
        matches!(
            naming.presets.preset_list.as_ref().map(|list| &list.step),
            Some(presets::Step::Naming(_))
        ),
        "n opened no name prompt"
    );
    for (what, mut app) in [
        ("the quit question", asking),
        ("the help overlay", helping),
        ("the menu", menu),
        ("the Settings box", settings),
        ("the device editor", editing),
        ("the removal", removing),
        ("the preset list", choosing),
        ("the preset list's name prompt", naming),
        ("a band's edit line", typing),
    ] {
        assert!(app.modal_open(), "{what} did not open");
        assert_eq!(app.key(ctrl_c), vec![Intent::Quit], "{what}");
    }
}

/// `q` asks before it quits: `Enter`, `y` and a second `q` answer yes, `Esc` and `n`
/// answer no, and every other key is swallowed while the question stands.
#[test]
fn q_asks_and_the_question_takes_every_key() {
    for yes in [
        KeyCode::Enter,
        KeyCode::Char('y'),
        KeyCode::Char('Y'),
        KeyCode::Char('q'),
    ] {
        let mut app = app();
        assert!(ch(&mut app, 'q').is_empty(), "q quit without asking");
        assert!(app.overlays.quitting && app.modal_open());
        assert_eq!(press(&mut app, yes), vec![Intent::Quit], "{yes:?}");
    }
    for no in [KeyCode::Esc, KeyCode::Char('n'), KeyCode::Char('N')] {
        let mut app = app();
        ch(&mut app, 'q');
        assert!(press(&mut app, no).is_empty(), "{no:?}");
        assert!(!app.overlays.quitting, "{no:?} left the question open");
    }
    let mut app = app();
    hear(&mut app, vo("5", Some("mute"), json!(false), "off"));
    ch(&mut app, 'q');
    let before = app.snapshot_for_test();
    for code in [
        KeyCode::Char('m'),
        KeyCode::Char('t'),
        KeyCode::Char('?'),
        KeyCode::Down,
        KeyCode::Right,
        KeyCode::Tab,
    ] {
        assert!(
            press(&mut app, code).is_empty(),
            "{code:?} reached the screen"
        );
    }
    assert_eq!(
        app.snapshot_for_test(),
        before,
        "a key moved something under it"
    );
    assert!(app.overlays.quitting);
}

/// `0` centres the balance from any row and any pane of the device screen, as the value
/// the profile names its centre, and the row it writes is the one the shimmer answers.
#[test]
fn zero_centres_the_balance_from_anywhere_on_the_device_screen() {
    let mut app = app();
    hear(&mut app, vo("11", Some("balance"), json!(70), "70"));
    let balance = FunctionId::one(11);
    for (pane, row) in [
        (Pane::Controls, 0),
        (Pane::Controls, row_of(&app, "volume")),
        (Pane::Tone, 0),
        (Pane::Status, 0),
    ] {
        app.pane = pane;
        app.selected = row;
        app.frame_flags.touched_row = None;
        assert_eq!(
            ch(&mut app, '0'),
            vec![set("balance", "64")],
            "{pane:?} row {row}"
        );
        assert_eq!(
            app.frame_flags.touched_row,
            Some(balance),
            "{pane:?}: no shimmer for the balance"
        );
    }
    // The tone screen's own `0` is a flat curve, which says nothing about the balance.
    ch(&mut app, 'e');
    let flattened = ch(&mut app, '0');
    assert!(
        !flattened.is_empty()
            && flattened
                .iter()
                .all(|intent| matches!(intent, Intent::EqSet { param, .. }
                    if ["g", "f", "q"].contains(&param.as_str()))),
        "0 on the tone screen is not the flatten key: {flattened:?}"
    );
}

/// A device with no balance and no tone control asks for nothing on either key, and the
/// key line lists both dimmed rather than as keys that seem to work.
#[test]
fn zero_and_f_ask_for_nothing_on_a_device_without_a_balance_or_a_tone_control() {
    let mut app = app_on("lyr5");
    assert!(app.balance().is_none() && app.profile.eq.is_none());
    assert!(ch(&mut app, '0').is_empty());
    assert!(ch(&mut app, 'f').is_empty());
    let scene = keys::Scene::of(&app);
    for label in ["0", "f"] {
        let hint = keys::hints_for(Screen::Device)
            .into_iter()
            .find(|hint| hint.label == label)
            .unwrap_or_else(|| panic!("the device screen lists {label}"));
        assert!(!keys::available_in(hint, scene), "{label} is not dimmed");
    }
}

/// With the question switched off, `q` quits at once, from the screen and from the help
/// overlay alike.
#[test]
fn q_quits_at_once_with_the_question_switched_off() {
    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig {
            confirm_quit: false,
            ..TuiConfig::default()
        },
        vec![row("gungnir")],
    );
    assert_eq!(ch(&mut app, 'q'), vec![Intent::Quit]);
    ch(&mut app, '?');
    assert_eq!(ch(&mut app, 'q'), vec![Intent::Quit]);
    assert!(!app.overlays.quitting);
}

/// The settings row turns the question the other way, from what the settings say now.
#[test]
fn the_settings_row_turns_the_quit_question() {
    let mut app = app();
    ch(&mut app, 's');
    app.overlays.settings = Some(crate::tui::app::SettingsBox {
        category: 2,
        row: 1,
    });
    assert_eq!(
        settings::current(app.overlays.settings.expect("the box is open")),
        Setting::Key(&CONFIRM_QUIT)
    );
    assert_eq!(
        press(&mut app, KeyCode::Char(' ')),
        vec![Intent::Tui(TuiChange::Set(
            &CONFIRM_QUIT,
            TuiValue::Bool(false)
        ))]
    );
}

/// Look's seventh row is the theme background: on by default, and Space and both arrows turn it.
#[test]
fn the_theme_background_row_is_look_s_seventh_and_space_and_the_arrows_turn_it() {
    let mut app = app();
    assert!(app.tui.theme_background, "the default has it on");
    assert_eq!(settings::rows(0).len(), 10);
    ch(&mut app, 's');
    app.overlays.settings = Some(crate::tui::app::SettingsBox {
        category: 0,
        row: 6,
    });
    assert_eq!(
        settings::current(app.overlays.settings.expect("the box is open")),
        Setting::Key(&THEME_BACKGROUND)
    );
    for key in [KeyCode::Char(' '), KeyCode::Left, KeyCode::Right] {
        assert_eq!(
            press(&mut app, key),
            vec![Intent::Tui(TuiChange::Set(
                &THEME_BACKGROUND,
                TuiValue::Bool(false)
            ))],
            "{key:?}"
        );
    }
}

#[test]
fn telemetry_arrows_step_the_entries_no_control_sets_by_key() {
    let mut app = app();
    hear(&mut app, vo("6", Some("sample_rate"), json!(2), "2"));
    hear(&mut app, raw_vo("7.1", "u16", 65_535));
    hear(&mut app, vo("10", Some("volume"), json!(109), "109"));
    ch(&mut app, 't');
    assert_eq!(app.selected, 0, "the lowest id starts focused");
    assert_eq!(
        press(&mut app, KeyCode::Right),
        vec![Intent::Raw {
            id: "6".into(),
            value_type: "u8".into(),
            value: "3".into(),
        }],
        "a readout takes a raw nudge"
    );
    press(&mut app, KeyCode::Down);
    assert!(
        press(&mut app, KeyCode::Right).is_empty(),
        "65535 is the top of a u16"
    );
    assert_eq!(
        press(&mut app, KeyCode::Left),
        vec![Intent::Raw {
            id: "7.1".into(),
            value_type: "u16".into(),
            value: "65534".into(),
        }]
    );
    press(&mut app, KeyCode::Down);
    assert!(
        press(&mut app, KeyCode::Left).is_empty(),
        "a control with a key of its own is not nudged raw"
    );
    press(&mut app, KeyCode::Down);
    assert_eq!(app.selected, 2, "and the list stops at its last entry");
}

/// The uppercase letters turn a mode of the TUI's own, which the loop writes to the
/// config; no lowercase letter does.
#[test]
fn uppercase_letters_toggle_tui_modes_and_lowercase_never_do() {
    let mut app = app();
    assert!(matches!(
        shift(&mut app, 'E')[0],
        Intent::Tui(TuiChange::Effects(_))
    ));
    assert!(matches!(
        shift(&mut app, 'H')[0],
        Intent::Tui(TuiChange::HueDrift(_))
    ));
    assert!(
        shift(&mut app, 'I').is_empty(),
        "capital I is bound to nothing"
    );
    assert!(matches!(
        shift(&mut app, 'M')[0],
        Intent::Tui(TuiChange::Mouse(_))
    ));
    assert!(matches!(
        shift(&mut app, 'F')[0],
        Intent::Tui(TuiChange::Effect(EffectSwitch::Frames, false))
    ));
    assert!(matches!(
        shift(&mut app, 'V')[0],
        Intent::Tui(TuiChange::Set(key, TuiValue::Bool(_))) if *key == TITLE_METER
    ));
    assert!(matches!(
        shift(&mut app, 'R')[0],
        Intent::Tui(TuiChange::SpectrumDrift(_))
    ));
    // Each asks for the other way from what the Settings box shows for the same switch.
    app.tui.effects.enabled = false;
    assert_eq!(
        shift(&mut app, 'E'),
        vec![Intent::Tui(TuiChange::Effects(true))]
    );
}

/// `m` is the device's mute wherever it is pressed, and the keys the tone screen had for
/// the loudness are gone from every screen.
#[test]
fn m_mutes_on_every_screen_and_nothing_lowercase_toggles_a_mode() {
    for screen in [
        Screen::Device,
        Screen::Tone,
        Screen::Telemetry,
        Screen::Devices,
    ] {
        let mut app = listing();
        app.show(screen);
        assert!(
            matches!(&ch(&mut app, 'm')[0], Intent::Set { key, .. } if key == "mute"),
            "m is not mute on {screen:?}"
        );
        assert!(
            ch(&mut app, 'l').is_empty(),
            "l does something on {screen:?}"
        );
    }
}

/// `b` asks the device on show to identify itself from every device screen, and on the Devices
/// list the device of the row the cursor is on, which need not be the one on show.
#[test]
fn b_identifies_on_controls_and_on_the_devices_row() {
    let mut app = app();
    for screen in ['c', 't', 'e'] {
        ch(&mut app, screen);
        assert_eq!(ch(&mut app, 'b'), vec![Intent::Identify(None)], "{screen}");
    }
    app.devices.rows = vec![row("bifrost"), row("gungnir")];
    ch(&mut app, 'd');
    app.selected = 0;
    assert_eq!(
        ch(&mut app, 'b'),
        vec![Intent::Identify(Some("bifrost".into()))]
    );
    press(&mut app, KeyCode::Down);
    assert_eq!(
        ch(&mut app, 'b'),
        vec![Intent::Identify(Some("gungnir".into()))]
    );
    app.devices.rows.clear();
    assert!(ch(&mut app, 'b').is_empty(), "no device under the cursor");
}

/// `b` is a lowercase key that acts on the device: it is in the key table for every device
/// screen and not for the boxes, which take their own keys.
#[test]
fn b_is_listed_on_every_device_screen_and_nowhere_else() {
    for screen in [
        Screen::Device,
        Screen::Tone,
        Screen::Telemetry,
        Screen::Devices,
    ] {
        assert!(
            keys::hints_for(screen)
                .iter()
                .any(|hint| hint.code == KeyCode::Char('b') && hint.does == "identify"),
            "{screen:?}"
        );
    }
    assert!(
        !keys::hints_for(Screen::Settings)
            .iter()
            .any(|hint| hint.code == KeyCode::Char('b'))
    );
}

/// Assert the three key rules of `hint`: an uppercase key toggles a mode of the TUI's own, and no
/// lowercase key does.
fn assert_three_rules(hint: &keys::KeyHint) {
    match hint.code {
        // The token row's own keys are bound to that row, not to a mode of the TUI.
        _ if hint.when == keys::When::TokenRow => {}
        KeyCode::Char(c) if c.is_ascii_uppercase() => assert!(
            hint.does.starts_with("toggle "),
            "{} does not toggle a mode",
            hint.label
        ),
        KeyCode::Char(c) if c.is_ascii_lowercase() => assert!(
            !hint.does.starts_with("toggle "),
            "{} is a lowercase key that toggles a mode",
            hint.label
        ),
        _ => {}
    }
}

/// Every key the table names follows the three rules the help overlay prints.
#[test]
fn every_screens_keys_follow_the_three_rules() {
    for hint in keys::all() {
        assert_three_rules(hint);
    }
}

/// The same on a Loki Max, whose list is the one that opens under `p`: every key the table lights
/// there follows the rules, and so do the keys the list names on itself, `u` writing to the device
/// as a lowercase key does and `Enter`, `y` and `Esc` toggling nothing.
#[test]
fn every_screens_keys_follow_the_three_rules_on_a_loki_max() {
    let loki = crate::tui::test_support::app_showing_on("lokimax", Screen::Device);
    let lit: Vec<_> = keys::all()
        .iter()
        .filter(|hint| keys::available(hint, &loki))
        .collect();
    assert!(lit.iter().any(|hint| hint.label == "p"), "p is lit there");
    for hint in lit {
        assert_three_rules(hint);
    }
    // While the list is open, a lowercase key acts on the device and an uppercase one does
    // nothing: no key of the box toggles a mode of the TUI.
    let mut app = loki;
    ch(&mut app, 'p');
    let before = app.snapshot_for_test();
    for key in ['F', 'M', 'V', 'E'] {
        assert!(ch(&mut app, key).is_empty(), "{key} sent a write");
        assert!(
            app.snapshot_for_test() == before,
            "{key} changed what the open list leaves alone"
        );
    }
}

#[test]
fn question_mark_opens_the_help_overlay_and_escape_or_question_mark_closes_it() {
    let mut app = app();
    ch(&mut app, '?');
    assert!(
        app.overlays.help_open && app.frame_flags.screen_changed,
        "the overlay sweeps in"
    );
    press(&mut app, KeyCode::Esc);
    assert!(!app.overlays.help_open && app.screen == Screen::Device);
    ch(&mut app, '?');
    assert!(app.overlays.help_open);
    ch(&mut app, '?');
    assert!(!app.overlays.help_open);
}

#[test]
fn the_open_help_overlay_swallows_every_key_that_is_not_its_own() {
    let mut app = app();
    ch(&mut app, '?');
    let before = app.snapshot_for_test();
    for code in [
        KeyCode::Down,
        KeyCode::Up,
        KeyCode::PageDown,
        KeyCode::PageUp,
        KeyCode::Home,
        KeyCode::End,
        KeyCode::Right,
        KeyCode::Tab,
        KeyCode::Enter,
        KeyCode::Char('t'),
        KeyCode::Char('m'),
        KeyCode::Char('a'),
        KeyCode::Char('r'),
    ] {
        assert!(
            press(&mut app, code).is_empty(),
            "{code:?} asked for a write"
        );
    }
    assert_eq!(
        app.snapshot_for_test(),
        before,
        "a key changed something under the overlay"
    );
    assert!(
        ch(&mut app, 'q').is_empty(),
        "q over the overlay did not ask"
    );
    assert!(
        app.overlays.quitting && !app.overlays.help_open,
        "the question did not take the overlay's place"
    );
    assert_eq!(ch(&mut app, 'q'), vec![Intent::Quit], "qq did not quit");
}

#[test]
fn the_help_overlay_opens_at_its_first_key_again() {
    let mut app = app();
    ch(&mut app, '?');
    app.overlays.help_scroll = 7;
    press(&mut app, KeyCode::Esc);
    ch(&mut app, '?');
    assert_eq!(app.overlays.help_scroll, 0);
}

#[test]
fn a_meter_reading_on_telemetry_is_not_nudged_raw() {
    let mut app = app();
    let id: svolder_proto::FunctionId = "20.1".parse().expect("a function id");
    app.profile
        .controls
        .push(meter_control(id, "l", Some((0, 3_000))));
    hear(&mut app, raw_vo("20.1", "u16", 1_000));
    ch(&mut app, 't');
    assert!(press(&mut app, KeyCode::Right).is_empty());
    assert!(press(&mut app, KeyCode::Left).is_empty());
    assert!(app.readings.pending.is_empty(), "no pending mark");
}

/// The first arrow in help after the terminal grew moves the view, not an offset the old size
/// allowed.
#[test]
fn help_scroll_is_held_inside_the_view_after_a_resize() {
    let mut app = app();
    app.resize(80, 24);
    ch(&mut app, '?');
    let (_, most) = crate::tui::screens::help::scroll_of(&app);
    assert!(most > 0, "help scrolls at 80x24");
    press(&mut app, KeyCode::End);
    assert_eq!(app.overlays.help_scroll, most);
    app.resize(200, 60);
    let (_, most) = crate::tui::screens::help::scroll_of(&app);
    assert_eq!(most, 0, "the whole of help fits at 200x60");
    assert_eq!(app.overlays.help_scroll, 0, "the offset follows the resize");
}

/// Help's content shrinking without a resize (mouse capture turning on drops its reasons) leaves the first `↑` moving the
/// view.
#[test]
fn help_scroll_is_held_inside_the_view_when_the_content_shrinks() {
    let mut app = app();
    app.mouse = false;
    app.resize(80, 24);
    ch(&mut app, '?');
    press(&mut app, KeyCode::End);
    let (_, before) = crate::tui::screens::help::scroll_of(&app);
    app.mouse = true;
    let (_, after) = crate::tui::screens::help::scroll_of(&app);
    assert!(after < before, "help shrank: {before} -> {after}");
    press(&mut app, KeyCode::Up);
    assert_eq!(app.overlays.help_scroll, after.saturating_sub(1));
}

#[test]
fn p_on_a_loki_max_opens_its_preset_list_with_three_slots_and_five_builtins() {
    let mut app = App::new(
        profile_named("lokimax"),
        "lokimax".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("lokimax")],
    );
    assert!(press(&mut app, KeyCode::Char('p')).is_empty());
    assert!(app.presets.preset_list.is_none(), "it has no tone control");
    let list = app.presets.device_list.as_ref().expect("the list opens");
    assert_eq!(list.step, device_presets::Step::Choosing);
    assert!(app.modal_open());
    let labels: Vec<String> = device_presets::rows(&app.profile)
        .iter()
        .map(|row| row.label(&app.profile))
        .collect();
    assert_eq!(
        labels,
        [
            "Preset 1",
            "Preset 2",
            "Preset 3",
            "Built in",
            "Flat",
            "Bass Boost",
            "Treble Boost",
            "Smiley",
            "Loudness"
        ]
    );
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert!(!app.modal_open());
}
