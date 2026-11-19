// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::KeyCode;
use svolder_ipc::HostEqOp;

use super::*;
use crate::tui::app::Xf;
use crate::tui::test_support::{app_showing, computer_eq_with_loudness, press};

/// An app on `screen`, computer side, with loudness `on` at `strength` and reference 100 %, the
/// volume read at 35 % and the depth the view works out for it.
fn app_on(screen: Screen, on: bool, strength: f64) -> App {
    let mut app = app_showing(Screen::Device);
    app.host.computer.eq = Some(computer_eq_with_loudness(
        on,
        (strength, 100.0),
        Some(35.0),
        13.7,
    ));
    app.flip_side();
    assert!(app.on_computer());
    if screen == Screen::Tone {
        app.show(Screen::Tone);
        app.tone.row = ToneRow::Mode;
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
    strength_pct: Option<f64>,
    reference_pct: Option<f64>,
    reference_now: Option<bool>,
) -> HostEqOp {
    HostEqOp::Loudness {
        on,
        strength_pct,
        reference_pct,
        reference_now,
        done: true,
    }
}

#[test]
fn the_section_is_absent_where_the_view_has_no_loudness() {
    let mut app = app_showing(Screen::Device);
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq_with_crossfeed(
        false, "bauer", None,
    ));
    app.flip_side();
    assert!(!app.ld_shown());
    app.focus_ld(Ld::Strength);
    assert_ne!(app.tone.row, ToneRow::Ld(Ld::Strength));
    assert!(app.ld_stops().is_empty());
    assert!(app.ld_key(KeyCode::Right).is_none());
    assert!(app.click_ld(Ld::Strength, 1).is_empty());
}

#[test]
fn the_tone_screen_walks_the_chain_in_order_and_tab_has_a_stop_per_section() {
    let mut app = app_on(Screen::Tone, true, 50.0);
    app.host.computer.eq.as_mut().expect("a view").crossfeed = None;
    let mut seen = Vec::new();
    for _ in 0..5 {
        press(&mut app, KeyCode::Down);
        seen.push(app.tone.row);
    }
    assert_eq!(
        seen,
        [
            ToneRow::Point(0),
            ToneRow::Point(1),
            ToneRow::Ld(Ld::Strength),
            ToneRow::Ld(Ld::Reference),
            ToneRow::Ld(Ld::Reference),
        ],
        "this view has no crossfeed rows: the walk stops at the last row"
    );
    app.tone.row = ToneRow::Mode;
    let mut tabs = Vec::new();
    for _ in 0..3 {
        press(&mut app, KeyCode::Tab);
        tabs.push(app.tone.row);
    }
    assert!(matches!(tabs[0], ToneRow::Point(_)));
    assert_eq!(tabs[1], ToneRow::Ld(Ld::Strength));
    assert_eq!(tabs[2], ToneRow::Mode);
}

#[test]
fn loudness_comes_before_crossfeed_in_the_walk_and_in_tab() {
    let mut app = app_on(Screen::Tone, true, 50.0);
    app.tone.row = ToneRow::Point(1);
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.tone.row, ToneRow::Ld(Ld::Reference));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Preset));
    app.tone.row = ToneRow::Ld(Ld::Strength);
    press(&mut app, KeyCode::Tab);
    assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Preset));
}

#[test]
fn the_device_pane_has_the_one_row_under_the_points() {
    let mut app = app_on(Screen::Device, true, 50.0);
    app.pane = crate::tui::app::Pane::Tone;
    app.host.computer.eq.as_mut().expect("a view").crossfeed = None;
    app.tone.row = ToneRow::Mode;
    let mut seen = Vec::new();
    for _ in 0..4 {
        press(&mut app, KeyCode::Down);
        seen.push(app.tone.row);
    }
    assert_eq!(
        seen,
        [
            ToneRow::Point(0),
            ToneRow::Point(1),
            ToneRow::Ld(Ld::Strength),
            ToneRow::Ld(Ld::Strength)
        ]
    );
    assert_eq!(app.screen, Screen::Device);
}

#[test]
fn the_arrows_step_the_strength_by_the_views_step_and_stop_at_the_ends() {
    let mut app = app_on(Screen::Tone, true, 50.0);
    app.focus_ld(Ld::Strength);
    assert_eq!(
        op(&press(&mut app, KeyCode::Right)),
        only(None, Some(55.0), None, None)
    );
    assert_eq!(
        op(&press(&mut app, KeyCode::Left)),
        only(None, Some(45.0), None, None)
    );
    for (strength, key) in [(100.0, KeyCode::Right), (0.0, KeyCode::Left)] {
        let mut app = app_on(Screen::Tone, true, strength);
        app.focus_ld(Ld::Strength);
        assert!(press(&mut app, key).is_empty(), "{strength}");
    }
}

#[test]
fn the_reference_row_steps_the_reference_and_r_takes_the_current_volume() {
    let mut app = app_on(Screen::Tone, true, 50.0);
    app.focus_ld(Ld::Reference);
    assert_eq!(
        op(&press(&mut app, KeyCode::Left)),
        only(None, None, Some(99.0), None)
    );
    assert_eq!(
        op(&press(&mut app, KeyCode::Char('r'))),
        only(None, None, None, Some(true))
    );
    // an unread volume says so and sends nothing
    let mut app = app_on(Screen::Tone, true, 50.0);
    let view = app.host.computer.eq.as_mut().expect("a view");
    view.loudness.as_mut().expect("loudness").volume_pct = None;
    app.focus_ld(Ld::Strength);
    assert!(press(&mut app, KeyCode::Char('r')).is_empty());
}

#[test]
fn space_and_enter_turn_it_on_and_off() {
    let mut app = app_on(Screen::Tone, false, 50.0);
    app.focus_ld(Ld::Strength);
    for key in [KeyCode::Char(' '), KeyCode::Enter] {
        assert_eq!(
            op(&press(&mut app, key)),
            only(Some(true), None, None, None)
        );
    }
    let mut app = app_on(Screen::Tone, true, 50.0);
    app.focus_ld(Ld::Strength);
    assert_eq!(
        op(&press(&mut app, KeyCode::Char(' '))),
        only(Some(false), None, None, None)
    );
}

#[test]
fn a_view_that_says_why_it_cannot_follow_still_takes_every_change() {
    let mut app = app_on(Screen::Tone, true, 50.0);
    let view = app.host.computer.eq.as_mut().expect("a view");
    view.loudness.as_mut().expect("loudness").unavailable = Some("no sink to follow".to_string());
    app.focus_ld(Ld::Strength);
    assert_eq!(press(&mut app, KeyCode::Right).len(), 1);
    assert_eq!(
        op(&press(&mut app, KeyCode::Char(' '))),
        only(Some(false), None, None, None)
    );
}

#[test]
fn the_section_is_unfolded_exactly_while_the_keys_are_in_it_and_esc_folds_it() {
    let mut app = app_on(Screen::Tone, true, 50.0);
    assert!(!app.ld_unfolded());
    app.focus_ld(Ld::Reference);
    assert!(app.ld_unfolded());
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.tone.row, ToneRow::Mode);
    assert!(!app.ld_unfolded());
    // the device pane never unfolds: its row has the strength alone
    let mut app = app_on(Screen::Device, true, 50.0);
    app.focus_ld(Ld::Strength);
    assert!(app.ld_row().is_some() && !app.ld_unfolded());
}

fn drag_op(done: bool, strength: f64) -> HostEqOp {
    only_done(Some(strength), done)
}

fn only_done(strength_pct: Option<f64>, done: bool) -> HostEqOp {
    HostEqOp::Loudness {
        on: None,
        strength_pct,
        reference_pct: None,
        reference_now: None,
        done,
    }
}

#[test]
fn a_press_on_the_bar_holds_it_a_drag_is_heard_and_the_release_saves_the_last_value() {
    use crate::tui::app::glide::{self, Bar};
    use ratatui::layout::Rect;
    let bar = Rect::new(10, 5, 10, 1);
    let mut app = app_on(Screen::Tone, true, 50.0);
    // the press focuses the row and is heard, not saved: cell 9 of 10 stands for 95 %
    let intents = glide::press(&mut app, Bar::Ld, bar, 19);
    assert_eq!(op(&intents), drag_op(false, 95.0));
    assert_eq!(app.tone.row, ToneRow::Ld(Ld::Strength));
    assert!(app.pointer.bar_drag.is_some(), "the bar is held");
    // a drag to the same cell asks nothing more; a drag to another is heard
    assert!(glide::to(&mut app, bar, 19).is_empty());
    assert_eq!(op(&glide::to(&mut app, bar, 14)), drag_op(false, 45.0));
    // the release writes once, saved, with the last value (here from where it let go)
    assert_eq!(
        op(&glide::release(&mut app, Some((bar, 12)))),
        drag_op(true, 25.0)
    );
    assert!(app.pointer.bar_drag.is_none());
}

#[test]
fn a_drag_past_either_end_gives_the_range_ends() {
    use crate::tui::app::glide::{self, Bar};
    use ratatui::layout::Rect;
    let bar = Rect::new(10, 5, 10, 1);
    let mut app = app_on(Screen::Tone, true, 50.0);
    glide::press(&mut app, Bar::Ld, bar, 15);
    assert_eq!(op(&glide::to(&mut app, bar, 2)), drag_op(false, 0.0));
    assert_eq!(op(&glide::to(&mut app, bar, 60)), drag_op(false, 100.0));
    assert_eq!(
        op(&glide::release(&mut app, Some((bar, 60)))),
        drag_op(true, 100.0)
    );
}

#[test]
fn a_key_while_the_bar_is_held_lets_go_of_it_saved() {
    use crate::tui::app::glide::{self, Bar};
    use ratatui::layout::Rect;
    let bar = Rect::new(10, 5, 10, 1);
    let mut app = app_on(Screen::Tone, true, 50.0);
    glide::press(&mut app, Bar::Ld, bar, 12);
    let intents = glide::stop(&mut app);
    assert_eq!(op(&intents), drag_op(true, 25.0));
}

#[test]
fn a_cell_of_a_bar_is_read_at_its_middle() {
    let bar = ratatui::layout::Rect::new(10, 0, 10, 1);
    assert!((bar_fraction(bar, 10) - 0.05).abs() < 1e-9);
    assert!((bar_fraction(bar, 19) - 0.95).abs() < 1e-9);
    assert_eq!(bar_fraction(bar, 3), 0.0);
    assert_eq!(bar_fraction(bar, 40), 1.0);
}

#[test]
fn the_readout_is_the_views_numbers_and_shortens_to_fit() {
    let app = app_on(Screen::Tone, true, 50.0);
    let view = app.loudness_view().expect("loudness");
    assert_eq!(
        readout(view, false, true, 80),
        "ref 100 % \u{b7} 35 %: bass +7.3 \u{b7} mids \u{2212}7.3 \u{b7} treble +2.1 dB"
    );
    assert_eq!(
        readout(view, false, false, 80),
        "ref 100 % \u{b7} 35 %: bass +7.3 \u{b7} mids \u{2212}7.3 dB"
    );
    assert_eq!(
        readout(view, false, false, 33),
        "35 %: bass +7.3 \u{b7} mids \u{2212}7.3 dB"
    );
    assert_eq!(
        readout(view, false, false, 25),
        "bass +7.3 \u{b7} mids \u{2212}7.3 dB"
    );
    assert!(readout(view, true, false, 120).ends_with(DAC_HINT));
    let off = app_on(Screen::Tone, false, 50.0);
    assert_eq!(
        readout(off.loudness_view().expect("loudness"), false, false, 80),
        "off: the sound plays as the EQ made it"
    );
}

#[test]
fn the_device_pane_row_comes_before_the_crossfeed_row() {
    let mut app = app_on(Screen::Device, true, 50.0);
    app.pane = crate::tui::app::Pane::Tone;
    app.tone.row = ToneRow::Point(1);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.tone.row, ToneRow::Ld(Ld::Strength));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Preset));
}

#[test]
fn keys_in_a_section_the_view_lost_go_back_to_the_mode_row() {
    let mut app = app_on(Screen::Tone, true, 50.0);
    app.focus_ld(Ld::Reference);
    app.host.computer.eq.as_mut().expect("a view").loudness = None;
    app.note_computer_view();
    assert_eq!(app.tone.row, ToneRow::Mode);
}

#[test]
fn a_chord_or_a_box_opening_mid_drag_saves_what_was_heard_once() {
    use crate::tui::app::glide::{self, Bar};
    use crossterm::event::{KeyEvent, KeyModifiers};
    use ratatui::layout::Rect;
    let bar = Rect::new(10, 5, 10, 1);
    let mut app = app_on(Screen::Tone, true, 50.0);
    glide::press(&mut app, Bar::Ld, bar, 12);
    let intents = app.key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL));
    assert_eq!(op(&intents), drag_op(true, 25.0));
    assert!(app.pointer.bar_drag.is_none());
    // a box opening over the screen does the same
    glide::press(&mut app, Bar::Ld, bar, 17);
    let intents = app.key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
    assert!(app.modal_open());
    assert!(
        intents
            .iter()
            .any(|i| *i == Intent::eq(drag_op(true, 75.0))),
        "{intents:?}"
    );
}

#[test]
fn a_press_on_the_value_it_stands_at_sends_nothing_at_all() {
    use crate::tui::app::glide::{self, Bar};
    use ratatui::layout::Rect;
    let bar = Rect::new(10, 5, 10, 1);
    let mut app = app_on(Screen::Tone, true, 55.0);
    // cell 5 of 10 is 55 %
    assert!(glide::press(&mut app, Bar::Ld, bar, 15).is_empty());
    assert!(glide::release(&mut app, Some((bar, 15))).is_empty());
    // a drag that moves away and the release there still send their pair
    glide::press(&mut app, Bar::Ld, bar, 15);
    assert_eq!(op(&glide::to(&mut app, bar, 12)), drag_op(false, 25.0));
    assert_eq!(
        op(&glide::release(&mut app, Some((bar, 12)))),
        drag_op(true, 25.0)
    );
}
