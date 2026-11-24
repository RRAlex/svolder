// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::KeyCode;
use svolder_ipc::HostEqOp;

use super::*;
use crate::tui::test_support::{app_showing, computer_eq_with_crossfeed, press};

/// An app on `screen`, computer side, with crossfeed on `model`'s `preset`.
fn app_on(screen: Screen, xf_on: bool, model: &str, preset: Option<&str>) -> App {
    let mut app = app_showing(Screen::Device);
    app.host.computer.eq = Some(computer_eq_with_crossfeed(xf_on, model, preset));
    app.flip_side();
    assert!(app.on_computer());
    if screen == Screen::Tone {
        app.show(Screen::Tone);
    }
    app
}

fn op(intents: &[Intent]) -> HostEqOp {
    match intents {
        [Intent::HostEq(op)] => (**op).clone(),
        other => panic!("one computer change wanted, got {other:?}"),
    }
}

fn only(
    on: Option<bool>,
    model: Option<&str>,
    preset: Option<&str>,
    amount_pct: Option<f64>,
    crossover_hz: Option<f64>,
    delay_ms: Option<f64>,
) -> HostEqOp {
    HostEqOp::Crossfeed {
        on,
        model: model.map(str::to_string),
        preset: preset.map(str::to_string),
        amount_pct,
        crossover_hz,
        delay_ms,
        done: true,
    }
}

#[test]
fn x_puts_the_keys_in_the_space_panel_on_the_tone_screen() {
    let mut app = app_on(Screen::Device, false, "bauer", None);
    assert!(press(&mut app, KeyCode::Char('X')).is_empty());
    assert_eq!(app.screen, Screen::Tone);
    assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Space));
    assert_eq!(app.open_stage(), crate::tui::app::Stage::Space);
}

#[test]
fn tab_goes_from_the_header_through_the_stages_and_esc_goes_up_a_level() {
    let mut app = app_on(Screen::Tone, false, "bauer", None);
    press(&mut app, KeyCode::Char('X'));
    // the chain's order: the header, the EQ, the Space panel
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.tone.row, ToneRow::Mode);
    press(&mut app, KeyCode::Tab);
    assert!(matches!(app.tone.row, ToneRow::Point(_)));
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Space));
    press(&mut app, KeyCode::BackTab);
    assert!(matches!(app.tone.row, ToneRow::Point(_)));
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, Screen::Tone, "Esc goes to the strip first");
    assert_eq!(app.tone.row, ToneRow::Strip(crate::tui::app::Stage::Space));
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.screen, Screen::Device, "a second Esc leaves");
}

#[test]
fn up_and_down_move_through_the_rows_of_the_panel_and_up_from_the_first_goes_to_the_strip() {
    let mut app = app_on(Screen::Tone, false, "bauer", None);
    press(&mut app, KeyCode::Char('X'));
    let mut seen = vec![app.tone.row];
    for _ in 0..6 {
        press(&mut app, KeyCode::Down);
        seen.push(app.tone.row);
    }
    assert_eq!(
        seen,
        [
            ToneRow::Crossfeed(Xf::Space),
            ToneRow::Crossfeed(Xf::Model),
            ToneRow::Crossfeed(Xf::Preset),
            ToneRow::Crossfeed(Xf::Level),
            ToneRow::Crossfeed(Xf::Crossover),
            ToneRow::Crossfeed(Xf::Delay),
            ToneRow::Crossfeed(Xf::Delay),
        ],
        "the last row stops"
    );
    for _ in 0..5 {
        press(&mut app, KeyCode::Up);
    }
    assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Space));
    press(&mut app, KeyCode::Up);
    assert_eq!(app.tone.row, ToneRow::Strip(crate::tui::app::Stage::Space));
}

#[test]
fn the_device_pane_has_the_plays_row_and_the_model_and_preset_rows_while_it_plays() {
    let mut app = app_on(Screen::Device, false, "bauer", None);
    app.pane = crate::tui::app::Pane::Tone;
    app.tone.row = ToneRow::Point(1);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Space));
    press(&mut app, KeyCode::Down);
    assert_eq!(
        app.tone.row,
        ToneRow::Crossfeed(Xf::Space),
        "off: the last row"
    );
    let mut app = app_on(Screen::Device, true, "bauer", None);
    app.pane = crate::tui::app::Pane::Tone;
    app.focus_crossfeed(Xf::Space);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Model));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Preset));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Preset), "the last row");
}

#[test]
fn on_the_device_pane_the_arrows_and_space_switch_it_and_the_model_and_preset_rows_step() {
    let mut app = app_on(Screen::Device, false, "bauer", None);
    app.pane = crate::tui::app::Pane::Tone;
    app.focus_crossfeed(Xf::Space);
    assert_eq!(
        op(&press(&mut app, KeyCode::Right)),
        only(Some(true), None, None, None, None, None)
    );
    assert!(press(&mut app, KeyCode::Left).is_empty(), "already off");
    assert_eq!(
        op(&press(&mut app, KeyCode::Char(' '))),
        only(Some(true), None, None, None, None, None)
    );
    let mut app = app_on(Screen::Device, true, "bauer", None);
    app.pane = crate::tui::app::Pane::Tone;
    app.focus_crossfeed(Xf::Preset);
    assert_eq!(
        op(&press(&mut app, KeyCode::Char(','))),
        only(None, None, Some("Chu Moy"), None, None, None)
    );
    app.focus_crossfeed(Xf::Model);
    assert_eq!(
        op(&press(&mut app, KeyCode::Right)),
        only(None, Some("linkwitz"), None, None, None, None)
    );
    // Enter opens the tone screen with the panel on the row
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert_eq!(app.screen, Screen::Tone);
    assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Model));
}

#[test]
fn the_model_and_the_preset_are_two_rows_that_step_round_the_ends() {
    let mut app = app_on(Screen::Tone, true, "bauer", Some("Chu Moy"));
    app.focus_crossfeed(Xf::Preset);
    assert_eq!(
        op(&press(&mut app, KeyCode::Right)),
        only(None, None, Some("bs2b default"), None, None, None)
    );
    assert_eq!(
        op(&press(&mut app, KeyCode::Left)),
        only(None, None, Some("Jan Meier"), None, None, None)
    );
    app.focus_crossfeed(Xf::Model);
    assert_eq!(
        op(&press(&mut app, KeyCode::Right)),
        only(None, Some("linkwitz"), None, None, None, None)
    );
    assert_eq!(
        op(&press(&mut app, KeyCode::Left)),
        only(None, Some("mid-side"), None, None, None, None),
        "the first model wraps to the last"
    );
    // the crossover model is chosen, not only its variants: Mid/Side bass is reachable
    let mut app = app_on(Screen::Tone, true, "meier", None);
    app.focus_crossfeed(Xf::Model);
    assert_eq!(
        op(&press(&mut app, KeyCode::Right)),
        only(None, Some("mid-side"), None, None, None, None)
    );
}

#[test]
fn the_dot_keys_no_longer_change_the_model() {
    let mut app = app_on(Screen::Tone, true, "bauer", Some("Chu Moy"));
    app.focus_crossfeed(Xf::Preset);
    let sent = press(&mut app, KeyCode::Char('.'));
    assert!(
        !sent.iter().any(|i| matches!(
            i,
            Intent::HostEq(op) if matches!(**op, HostEqOp::Crossfeed { model: Some(_), .. })
        )),
        "{sent:?}"
    );
}

#[test]
fn a_model_with_one_preset_sends_nothing_on_its_preset_row() {
    let mut app = app_on(Screen::Tone, true, "mid-side", None);
    assert_eq!(app.xf_model().unwrap().presets.len(), 1);
    app.focus_crossfeed(Xf::Preset);
    assert!(press(&mut app, KeyCode::Right).is_empty());
    assert!(press(&mut app, KeyCode::Left).is_empty());
}

#[test]
fn a_value_row_steps_by_the_views_step_held_to_the_views_range() {
    let mut app = app_on(Screen::Tone, true, "bauer", Some("bs2b default"));
    let model = app.xf_model().unwrap().clone();
    let view = app.crossfeed().unwrap().clone();
    app.focus_crossfeed(Xf::Level);
    let sent = op(&press(&mut app, KeyCode::Right));
    let HostEqOp::Crossfeed {
        amount_pct: Some(v),
        ..
    } = sent
    else {
        panic!("{sent:?}")
    };
    assert_eq!(
        v,
        model
            .amount_pct
            .snap(view.amount_pct + model.amount_pct.step)
    );
    // crossover steps by its own step
    app.focus_crossfeed(Xf::Crossover);
    let sent = op(&press(&mut app, KeyCode::Left));
    let HostEqOp::Crossfeed {
        crossover_hz: Some(v),
        ..
    } = sent
    else {
        panic!("{sent:?}")
    };
    assert_eq!(
        v,
        model
            .crossover_hz
            .snap(view.crossover_hz - model.crossover_hz.step)
    );
}

#[test]
fn a_value_at_the_end_of_its_range_sends_nothing() {
    let mut app = app_on(Screen::Tone, true, "bauer", None);
    let max = app.xf_model().unwrap().amount_pct.max;
    app.host
        .computer
        .eq
        .as_mut()
        .unwrap()
        .crossfeed
        .as_mut()
        .unwrap()
        .amount_pct = max;
    app.focus_crossfeed(Xf::Level);
    assert!(press(&mut app, KeyCode::Right).is_empty());
}

#[test]
fn the_delay_steps_up_from_its_floor_and_back_to_the_models_own() {
    let mut app = app_on(Screen::Tone, true, "bauer", None);
    let model = app.xf_model().unwrap().clone();
    let floor = app.crossfeed().unwrap().delay_floor_ms;
    app.focus_crossfeed(Xf::Delay);
    assert!(
        press(&mut app, KeyCode::Left).is_empty(),
        "below the floor is nothing"
    );
    let sent = op(&press(&mut app, KeyCode::Right));
    let HostEqOp::Crossfeed {
        delay_ms: Some(v), ..
    } = sent
    else {
        panic!("{sent:?}")
    };
    assert!(
        v > floor && (v - floor - model.delay_step_ms).abs() < 1e-6,
        "{v} {floor}"
    );
    // the view now says the total is a step above the floor: a step down asks for the floor
    app.host
        .computer
        .eq
        .as_mut()
        .unwrap()
        .crossfeed
        .as_mut()
        .unwrap()
        .delay_ms = v;
    let sent = op(&press(&mut app, KeyCode::Left));
    let HostEqOp::Crossfeed {
        delay_ms: Some(back),
        ..
    } = sent
    else {
        panic!("{sent:?}")
    };
    assert!(back <= floor + 1e-6);
}

#[test]
fn a_model_without_a_delay_has_a_row_that_does_nothing_and_says_why() {
    let mut app = app_on(Screen::Tone, true, "mid-side", None);
    assert!(!app.crossfeed().unwrap().delay_settable);
    app.focus_crossfeed(Xf::Delay);
    assert!(press(&mut app, KeyCode::Right).is_empty());
    assert!(app.notices.toast.is_some());
}

#[test]
fn an_unavailable_crossfeed_refuses_a_change_in_words() {
    let mut app = app_on(Screen::Tone, false, "bauer", None);
    app.host
        .computer
        .eq
        .as_mut()
        .unwrap()
        .crossfeed
        .as_mut()
        .unwrap()
        .unavailable = Some("this chain cannot hold it".into());
    app.focus_crossfeed(Xf::Space);
    assert!(press(&mut app, KeyCode::Char(' ')).is_empty());
    assert!(app.notices.toast.is_some());
}

#[test]
fn the_edit_line_types_a_number_and_enter_sends_it() {
    let mut app = app_on(Screen::Tone, true, "bauer", None);
    app.focus_crossfeed(Xf::Crossover);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(app.tone.xf_line.is_some());
    for c in ['8', '5', '0'] {
        press(&mut app, KeyCode::Char(c));
    }
    press(&mut app, KeyCode::Char('x'));
    assert!(app.tone.xf_line.as_ref().unwrap().refusal.is_some());
    assert_eq!(
        op(&press(&mut app, KeyCode::Enter)),
        only(None, None, None, None, Some(850.0), None)
    );
    assert!(app.tone.xf_line.is_none());
}

#[test]
fn escape_closes_the_edit_line_with_nothing_sent_and_keeps_the_section() {
    let mut app = app_on(Screen::Tone, true, "bauer", None);
    app.focus_crossfeed(Xf::Level);
    press(&mut app, KeyCode::Enter);
    assert!(press(&mut app, KeyCode::Esc).is_empty());
    assert!(app.tone.xf_line.is_none());
    assert!(app.xf_row().is_some());
}

#[test]
fn an_unchanged_line_sends_nothing() {
    let mut app = app_on(Screen::Tone, true, "bauer", None);
    app.focus_crossfeed(Xf::Level);
    press(&mut app, KeyCode::Enter);
    assert!(press(&mut app, KeyCode::Enter).is_empty());
}

#[test]
fn a_daemon_without_crossfeed_offers_no_row_and_no_key() {
    let mut app = app_showing(Screen::Device);
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq(
        true,
        &[("PK", 1000.0, 0.0, 1.0)],
    ));
    app.flip_side();
    assert!(!app.crossfeed_shown());
    press(&mut app, KeyCode::Char('X'));
    assert_eq!(app.screen, Screen::Device);
    app.show(Screen::Tone);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.tone.row, ToneRow::Mode, "two stops, as before");
    press(&mut app, KeyCode::Tab);
    assert!(matches!(app.tone.row, ToneRow::Point(_)));
}

#[test]
fn a_reset_note_is_said_once_in_a_toast() {
    let mut app = app_on(Screen::Tone, false, "bauer", None);
    let note = "crossfeed turned off: the output changed to Speakers";
    app.host
        .computer
        .eq
        .as_mut()
        .unwrap()
        .crossfeed
        .as_mut()
        .unwrap()
        .note = Some(note.into());
    app.note_computer_view();
    assert_eq!(
        app.notices.toast.as_ref().map(|t| t.text.as_str()),
        Some(note)
    );
    app.notices.toast = None;
    app.note_computer_view();
    assert!(app.notices.toast.is_none(), "said once");
    app.host
        .computer
        .eq
        .as_mut()
        .unwrap()
        .crossfeed
        .as_mut()
        .unwrap()
        .note = None;
    app.note_computer_view();
    app.host
        .computer
        .eq
        .as_mut()
        .unwrap()
        .crossfeed
        .as_mut()
        .unwrap()
        .note = Some(note.into());
    app.note_computer_view();
    assert!(app.notices.toast.is_some(), "a later reset says it again");
}

#[test]
fn crossfeed_focus_does_not_follow_the_user_to_other_screens() {
    for (key, screen) in [
        ('d', Screen::Devices),
        ('t', Screen::Telemetry),
        ('c', Screen::Device),
    ] {
        let mut app = app_on(Screen::Tone, false, "bauer", None);
        press(&mut app, KeyCode::Char('X'));
        assert!(app.xf_row().is_some());
        press(&mut app, KeyCode::Char(key));
        assert_eq!(app.screen, screen);
        assert!(
            app.xf_row().is_none(),
            "{screen:?}: no row of the section is focused"
        );
        // keys keep the meaning the screen gives them: no crossfeed change, no jump to the tone screen
        for code in [
            KeyCode::Right,
            KeyCode::Left,
            KeyCode::Char(' '),
            KeyCode::Enter,
        ] {
            let intents = press(&mut app, code);
            assert!(
                !intents.iter().any(|i| matches!(i, Intent::HostEq(_))),
                "{screen:?} {code:?}: {intents:?}"
            );
            assert_eq!(app.screen, screen, "{code:?} stays on {screen:?}");
        }
    }
}

#[test]
fn a_section_row_other_than_the_pane_row_is_not_focused_on_the_device_screen() {
    let mut app = app_on(Screen::Tone, false, "bauer", None);
    app.focus_crossfeed(Xf::Level);
    app.show(Screen::Device);
    assert!(app.xf_row().is_none());
}

#[test]
fn a_click_on_a_point_closes_an_open_edit_line_and_no_key_is_swallowed() {
    let mut app = app_on(Screen::Tone, true, "bauer", None);
    app.focus_crossfeed(Xf::Level);
    press(&mut app, KeyCode::Enter);
    assert!(app.tone.xf_line.is_some());
    app.focus_point(1);
    assert!(app.tone.xf_line.is_none(), "the line is closed, not hidden");
    assert!(!app.modal_open());
    press(&mut app, KeyCode::Tab);
    assert_eq!(
        app.tone.row,
        ToneRow::Crossfeed(Xf::Space),
        "Tab works: from the points it goes to the Space panel"
    );
    // also when something other than focus_point moved the keys
    app.focus_crossfeed(Xf::Level);
    press(&mut app, KeyCode::Enter);
    app.tone.row = ToneRow::Point(0);
    press(&mut app, KeyCode::Char('x'));
    assert!(app.tone.xf_line.is_none());
}
