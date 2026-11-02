// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::{KeyCode, KeyEvent};
use svolder_ipc::TuiConfig;

use crate::backend::DeviceRow;
use crate::tui::app::Intent;
use crate::tui::decoration::mascot::codes::KONAMI_KEYS;
use crate::tui::test_support::{app_showing, frame, profile_named, text};

use super::*;

/// An app on `name`, with no tone control and one device: what [`available`] sees for the
/// keys a profile or a device list gates.
fn app_with_profile(name: &str) -> App {
    App::new(
        profile_named(name),
        name.into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test(name)],
    )
}

#[test]
fn every_screen_lists_its_keys_in_key_row_order_and_the_device_screen_names_the_screen_keys() {
    let labels: Vec<&str> = hints_for(Screen::Device).iter().map(|h| h.label).collect();
    for wanted in [
        "↑↓",
        "←→",
        "␣/Enter",
        "+/=/-",
        "m",
        "Tab/⇧Tab",
        "r",
        "e",
        "t",
        "d",
        "s",
        "M",
        "a",
        "?",
        "q",
    ] {
        assert!(
            labels.contains(&wanted),
            "the device screen lacks {wanted}: {labels:?}"
        );
    }
    let nav = labels.iter().position(|l| *l == "↑↓").unwrap();
    let screens = labels.iter().position(|l| *l == "e").unwrap();
    let global = labels.iter().position(|l| *l == "q").unwrap();
    assert!(nav < screens && screens < global);
}

/// The band keys' label reads one to as many bands as the profile on the screen has, on
/// the help overlay: a digit past that picks nothing.
#[test]
fn the_band_keys_read_one_to_the_profiles_band_count() {
    for (name, wanted) in [("mimir", "1-3"), ("gungnir2-v2", "1-5")] {
        let mut app = app_with_profile(name);
        app.show(Screen::Tone);
        app.overlays.help_open = true;
        let rows = text(&frame(&app, 126, 62));
        assert!(
            rows.iter().any(|row| row.contains(&format!("{wanted} "))),
            "{name}: {rows:#?}"
        );
        assert!(
            !rows.iter().any(|row| row.contains("1-9 ")),
            "{name}: {rows:#?}"
        );
        let band = hints_for(Screen::Tone)
            .into_iter()
            .find(|h| h.does == "band")
            .expect("the tone screen has band keys");
        assert_eq!(band.label_in(Scene::of(&app)), wanted, "{name}");
    }
}

#[test]
fn conditional_keys_are_dimmed_not_hidden() {
    let app = app_with_profile("lyr5"); // no EQ, one device
    let e = hints_for(Screen::Device)
        .into_iter()
        .find(|h| h.label == "e")
        .unwrap();
    assert!(!available(e, &app));
    let mut app = app;
    app.overlays.help_open = true;
    let rows = text(&frame(&app, 126, 62));
    assert!(
        rows.iter()
            .any(|row| row.contains("tone") && row.contains("needs an EQ")),
        "the help overlay hides the dimmed key: {rows:#?}"
    );
}

/// `␣/Enter` is lit where a toggle (or the host's rows) can answer it and `+/=/-` where the device
/// has a volume; a Loki Max has neither, so both are dimmed with the words that say why.
#[test]
fn the_toggle_and_volume_hints_are_dimmed_on_a_device_without_those_controls() {
    let hint = |label: &str| {
        hints_for(Screen::Device)
            .into_iter()
            .find(|h| h.label == label)
            .expect("the device screen lists it")
    };
    let loki = app_with_profile("lokimax");
    assert!(!available(hint("␣/Enter"), &loki));
    assert!(!available(hint("+/=/-"), &loki));
    let gungnir = app_with_profile("gungnir2-v2");
    assert!(available(hint("␣/Enter"), &gungnir));
    assert!(available(hint("+/=/-"), &gungnir));

    let mut loki = loki;
    loki.overlays.help_open = true;
    let rows = text(&frame(&loki, 126, 62));
    assert!(
        rows.iter()
            .any(|row| row.contains("toggle/press") && row.contains("needs a toggle")),
        "{rows:#?}"
    );
    assert!(
        rows.iter()
            .any(|row| row.contains("volume") && row.contains("needs a volume")),
        "{rows:#?}"
    );
}

/// `p` is lit where there is a list to open — a tone control's, or the device's own — and dimmed
/// where there is neither; the help overlay promises `a i h` only inside the tone control's list.
#[test]
fn the_p_hint_is_lit_on_a_loki_max_and_dimmed_on_a_device_with_neither() {
    let hint = hints_for(Screen::Device)
        .into_iter()
        .find(|h| h.label == "p")
        .expect("the device screen lists p");
    let loki = app_with_profile("lokimax");
    assert!(available(hint, &loki), "a Loki Max has presets of its own");
    assert!(
        available(hint, &app_with_profile("gungnir2-v2")),
        "a tone control has presets"
    );
    let lyr = app_with_profile("lyr5");
    assert!(!available(hint, &lyr), "a Lyr 5 has neither");

    assert_eq!(hint.long_in(Scene::of(&loki)), "presets");
    assert_eq!(
        hint.long_in(Scene::of(&app_with_profile("gungnir2-v2"))),
        "presets (a i h inside)"
    );

    let mut lyr = lyr;
    lyr.overlays.help_open = true;
    let rows = text(&frame(&lyr, 126, 62));
    assert!(
        rows.iter()
            .any(|row| row.contains("presets") && row.contains("needs presets")),
        "the help overlay says why p is dimmed: {rows:#?}"
    );
}

#[test]
fn every_hint_on_a_screen_does_something_when_pressed() {
    for screen in [
        Screen::Device,
        Screen::Tone,
        Screen::Telemetry,
        Screen::Devices,
        Screen::Settings,
    ] {
        for hint in hints_for(screen) {
            let mut app = pressing(screen);
            if hint.when == When::TokenRow {
                // The row's own keys act on the token row, with a token to act on.
                app.overlays.settings = Some(crate::tui::app::SettingsBox {
                    category: crate::tui::screens::settings::CONNECT,
                    row: 4,
                });
                app.overlays.conn.token = crate::tui::boxes::connections::TokenView::Present {
                    masked: "••••••3fQ".into(),
                    source: svolder_ipc::token::TokenSource::Generated("/x".into()),
                };
            }
            let before = app.snapshot_for_test();
            let intents = app.key(KeyEvent::from(hint.code));
            assert!(
                app.snapshot_for_test() != before
                    || !intents.is_empty()
                    || app.overlays.conn.modal.is_some(),
                "{:?} {}",
                screen,
                hint.label
            );
        }
    }
}

/// On a Loki Max, every key the table lights on a screen does something when pressed; the keys
/// the profile dims by design (a mute, a phase switch, the tone control's) are not pressed, as
/// they are not on a device that has none.
#[test]
fn every_lit_hint_on_a_loki_max_does_something_when_pressed() {
    // The tone-control screen is left out: a Loki Max has no tone control for its keys, `p`
    // included, to act on.
    for screen in [
        Screen::Device,
        Screen::Telemetry,
        Screen::Devices,
        Screen::Settings,
    ] {
        for hint in hints_for(screen) {
            let mut app = crate::tui::test_support::app_showing_on("lokimax", screen);
            app.pane = app.panes()[0];
            if screen == Screen::Device {
                // The Input select reads its first value, so the arrows have a row to step.
                crate::tui::test_support::hear_one(&mut app, "2", 1);
                app.selected = 0;
            }
            if screen == Screen::Settings {
                app.overlays.settings = Some(crate::tui::app::SettingsBox {
                    category: 2,
                    row: 0,
                });
            }
            if hint.when == When::TokenRow || !available(hint, &app) {
                continue;
            }
            let before = app.snapshot_for_test();
            let intents = app.key(KeyEvent::from(hint.code));
            assert!(
                app.snapshot_for_test() != before
                    || !intents.is_empty()
                    || app.overlays.conn.modal.is_some(),
                "{:?} {}",
                screen,
                hint.label
            );
        }
    }
}

/// An app showing `screen` with its own body focused, on the first row of it: every hint
/// of the screen does something from there — the Settings box's `←→` flip its first row,
/// a switch, as they step a row of choices.
fn pressing(screen: Screen) -> App {
    let mut app = app_showing(screen);
    app.pane = app.panes()[0];
    if screen == Screen::Settings {
        // Mouse capture, the first row of Behaviour, is a switch.
        app.overlays.settings = Some(crate::tui::app::SettingsBox {
            category: 2,
            row: 0,
        });
    }
    app
}

/// The wizard's `Esc back` is worth pressing only where the wizard can go back.
#[test]
fn the_wizards_esc_needs_a_step_to_go_back_to() {
    let esc = hints_for(Screen::Setup)
        .into_iter()
        .find(|hint| hint.code == KeyCode::Esc)
        .expect("the wizard lists Esc");
    let mut scene = Scene::default();
    assert!(!available_in(esc, scene), "the device is written");
    scene.can_go_back = true;
    assert!(available_in(esc, scene));
}

/// The help overlay spells out the one-word descriptions the key row abbreviates, and the key
/// row keeps the short form.
#[test]
fn the_overlay_writes_abbreviated_words_in_full_and_the_key_row_does_not() {
    let scene = Scene::default();
    for (short, long) in [
        ("telem", "telemetry"),
        ("devs", "devices"),
        ("vol", "volume"),
        ("freq", "frequency"),
    ] {
        let hint = all()
            .iter()
            .find(|hint| hint.does == short)
            .unwrap_or_else(|| panic!("no key says {short}"));
        assert_eq!(hint.does_in(scene), short);
        assert_eq!(hint.long_in(scene), long);
    }
    let mute = all().iter().find(|hint| hint.does == "mute").unwrap();
    assert_eq!(mute.long_in(scene), "mute");
}

/// An app showing the device screen whose mascot may be seen.
fn mascot_app() -> App {
    let mut app = app_showing(Screen::Device);
    app.mascot.capable = true;
    app
}

fn press_all(app: &mut App, keys: &[KeyCode]) -> Vec<Intent> {
    let mut last = Vec::new();
    for key in keys {
        last = crate::tui::test_support::press(app, *key);
    }
    last
}

fn chars(text: &str) -> Vec<KeyCode> {
    text.chars().map(KeyCode::Char).collect()
}

#[test]
fn the_first_code_typed_in_help_calls_the_squirrel_and_closes_help() {
    let mut app = mascot_app();
    press_all(&mut app, &[KeyCode::Char('?')]);
    assert!(app.overlays.help_open);
    press_all(&mut app, &KONAMI_KEYS);
    assert!(!app.overlays.help_open);
    assert!(app.mascot.called());
}

#[test]
fn the_second_code_typed_in_the_menu_turns_god_mode_on_and_does_not_quit() {
    let mut app = mascot_app();
    press_all(&mut app, &[KeyCode::Esc]);
    assert!(app.overlays.menu.is_some());
    let intents = press_all(&mut app, &chars("iddqd"));
    assert!(app.overlays.menu.is_none());
    assert!(app.mascot.god());
    assert!(!intents.contains(&Intent::Quit), "{intents:?}");
    assert!(!app.overlays.quitting);
}

#[test]
fn a_q_after_i_d_in_the_menu_quits() {
    let mut app = mascot_app();
    press_all(&mut app, &[KeyCode::Esc]);
    let intents = press_all(&mut app, &chars("idq"));
    assert!(app.overlays.menu.is_none());
    assert!(!app.mascot.god());
    assert!(app.overlays.quitting);
    assert!(!intents.contains(&Intent::Quit), "{intents:?}");
}

#[test]
fn esc_mid_code_abandons_it_even_across_a_reopen() {
    let mut app = mascot_app();
    press_all(&mut app, &[KeyCode::Char('?')]);
    press_all(&mut app, &chars("idd"));
    press_all(&mut app, &[KeyCode::Esc]);
    assert!(!app.overlays.help_open);
    press_all(&mut app, &[KeyCode::Char('?')]);
    press_all(&mut app, &chars("qd"));
    assert!(!app.mascot.god(), "the code did not resume");
}

#[test]
fn closing_help_with_a_question_mark_also_clears_the_code() {
    let mut app = mascot_app();
    press_all(&mut app, &[KeyCode::Char('?')]);
    press_all(&mut app, &chars("idd"));
    press_all(&mut app, &[KeyCode::Char('?')]);
    press_all(&mut app, &[KeyCode::Char('?')]);
    press_all(&mut app, &chars("qd"));
    assert!(!app.mascot.god());
}

#[test]
fn with_the_mascot_off_q_after_idd_quits() {
    let mut app = mascot_app();
    app.tui.effects.mascot = false;
    press_all(&mut app, &[KeyCode::Char('?')]);
    press_all(&mut app, &chars("idd"));
    let intents = press_all(&mut app, &[KeyCode::Char('q')]);
    assert!(app.overlays.quitting);
    assert!(!intents.contains(&Intent::Quit), "{intents:?}");
    assert!(!app.mascot.god());
}

#[test]
fn a_key_more_than_two_seconds_after_the_last_does_not_continue_the_code() {
    let mut app = mascot_app();
    press_all(&mut app, &[KeyCode::Char('?')]);
    press_all(&mut app, &chars("idd"));
    app.now += std::time::Duration::from_millis(2_100);
    press_all(&mut app, &chars("qd"));
    assert!(!app.mascot.god());
    // Inside the gap the same keys finish the code.
    let mut app = mascot_app();
    press_all(&mut app, &[KeyCode::Char('?')]);
    press_all(&mut app, &chars("idd"));
    app.now += std::time::Duration::from_millis(1_900);
    press_all(&mut app, &chars("qd"));
    assert!(app.mascot.god());
}
