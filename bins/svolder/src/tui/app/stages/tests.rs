// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::KeyCode;
use svolder_host::eq::BypassView;
use svolder_ipc::HostEqOp;

use super::*;
use crate::tui::test_support::{app_showing, computer_eq_stages, press};

/// An app on the tone screen's computer side with every stage served, the keys on the header.
fn stages_app(space: Space) -> App {
    let mut app = app_showing(Screen::Device);
    app.host.computer.eq = Some(computer_eq_stages(space));
    app.flip_side();
    app.show(Screen::Tone);
    app.tone.row = ToneRow::Mode;
    app
}

fn op(intents: &[Intent]) -> HostEqOp {
    match intents {
        [Intent::HostEq(op)] => (**op).clone(),
        other => panic!("one computer change wanted, got {other:?}"),
    }
}

#[test]
fn tab_walks_the_header_and_every_stage_and_round() {
    let mut app = stages_app(Space::Speakers);
    let mut seen = Vec::new();
    for _ in 0..5 {
        press(&mut app, KeyCode::Tab);
        seen.push(app.tone.row);
    }
    assert_eq!(
        seen,
        [
            ToneRow::Point(0),
            ToneRow::Ld(Ld::On),
            ToneRow::Crossfeed(Xf::Space),
            ToneRow::Bypass,
            ToneRow::Mode,
        ]
    );
    press(&mut app, KeyCode::BackTab);
    assert_eq!(app.tone.row, ToneRow::Bypass, "Shift-Tab goes back");
}

#[test]
fn the_strip_has_a_stop_per_stage_that_the_arrows_walk_without_wrapping() {
    let mut app = stages_app(Space::Speakers);
    app.focus_strip(Stage::Eq);
    assert!(press(&mut app, KeyCode::Left).is_empty());
    assert_eq!(
        app.tone.row,
        ToneRow::Strip(Stage::Eq),
        "the first stop stays"
    );
    let mut seen = Vec::new();
    for _ in 0..4 {
        press(&mut app, KeyCode::Right);
        seen.push(app.tone.row);
    }
    assert_eq!(
        seen,
        [
            ToneRow::Strip(Stage::Loudness),
            ToneRow::Strip(Stage::Space),
            ToneRow::Strip(Stage::Bypass),
            ToneRow::Strip(Stage::Bypass),
        ]
    );
}

#[test]
fn down_and_enter_go_into_the_panel_up_and_esc_come_back() {
    for key in [KeyCode::Down, KeyCode::Enter] {
        let mut app = stages_app(Space::Speakers);
        app.focus_strip(Stage::Space);
        assert!(press(&mut app, key).is_empty());
        assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Space), "{key:?}");
        assert!(press(&mut app, KeyCode::Esc).is_empty());
        assert_eq!(
            app.tone.row,
            ToneRow::Strip(Stage::Space),
            "Esc: panel to strip"
        );
        assert_eq!(app.screen, Screen::Tone);
    }
    let mut app = stages_app(Space::Speakers);
    app.focus_strip(Stage::Loudness);
    press(&mut app, KeyCode::Up);
    assert_eq!(
        app.tone.row,
        ToneRow::Mode,
        "Up from the strip is the header"
    );
    press(&mut app, KeyCode::Down);
    assert_eq!(
        app.tone.row,
        ToneRow::Strip(Stage::Loudness),
        "and Down from it the strip of the stage the panel shows"
    );
}

#[test]
fn esc_goes_panel_strip_leave_and_opens_no_menu() {
    let mut app = stages_app(Space::Speakers);
    app.enter_stage(Stage::Eq);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.tone.row, ToneRow::Strip(Stage::Eq));
    press(&mut app, KeyCode::Esc);
    assert_eq!(
        app.screen,
        Screen::Device,
        "from the strip the screen is left"
    );
    assert!(app.overlays.menu.is_none(), "no menu opens on the way");
}

#[test]
fn space_on_a_stop_turns_that_stage_on_or_off() {
    let mut app = stages_app(Space::Speakers);
    app.focus_strip(Stage::Eq);
    assert_eq!(
        op(&press(&mut app, KeyCode::Char(' '))),
        HostEqOp::Mode { on: false }
    );
    app.focus_strip(Stage::Loudness);
    assert!(matches!(
        op(&press(&mut app, KeyCode::Char(' '))),
        HostEqOp::Loudness {
            on: Some(false),
            ..
        }
    ));
    app.focus_strip(Stage::Space);
    assert!(matches!(
        op(&press(&mut app, KeyCode::Char(' '))),
        HostEqOp::Speakers {
            on: Some(false),
            ..
        }
    ));
    app.focus_strip(Stage::Bypass);
    assert_eq!(
        op(&press(&mut app, KeyCode::Char(' '))),
        HostEqOp::Bypass { on: true }
    );
}

#[test]
fn space_in_a_panel_turns_its_stage_on_or_off_from_any_row() {
    let mut app = stages_app(Space::Speakers);
    app.focus_ld(Ld::Reference);
    assert!(matches!(
        op(&press(&mut app, KeyCode::Char(' '))),
        HostEqOp::Loudness {
            on: Some(false),
            ..
        }
    ));
    app.focus_point(2);
    assert_eq!(
        op(&press(&mut app, KeyCode::Char(' '))),
        HostEqOp::Mode { on: false }
    );
}

#[test]
fn f_is_the_bypass_from_every_row_of_the_tone_and_the_device_screens() {
    let mut app = stages_app(Space::Speakers);
    for row in [
        ToneRow::Mode,
        ToneRow::Strip(Stage::Loudness),
        ToneRow::Point(1),
        ToneRow::Ld(Ld::Reference),
        ToneRow::Crossfeed(Xf::Set),
        ToneRow::Bypass,
    ] {
        app.tone.row = row;
        assert_eq!(
            op(&press(&mut app, KeyCode::Char('f'))),
            HostEqOp::Bypass { on: true },
            "{row:?}"
        );
    }
    // from the bypass held, `f` brings the stages back
    app.host.computer.eq.as_mut().expect("a view").bypass = Some(BypassView {
        on: true,
        ..BypassView::default()
    });
    assert_eq!(
        op(&press(&mut app, KeyCode::Char('f'))),
        HostEqOp::Bypass { on: false }
    );
    // the device screen's Tone pane too; the DAC side's f stays flat
    app.show(Screen::Device);
    assert_eq!(
        op(&press(&mut app, KeyCode::Char('f'))),
        HostEqOp::Bypass { on: false }
    );
}

#[test]
fn f_without_a_bypass_in_the_view_says_so_and_sends_nothing() {
    let mut app = stages_app(Space::Speakers);
    app.host.computer.eq.as_mut().expect("a view").bypass = None;
    assert!(press(&mut app, KeyCode::Char('f')).is_empty());
    assert!(
        app.toast()
            .is_some_and(|toast| toast.text.contains("no bypass"))
    );
}

#[test]
fn the_dacs_f_stays_flat() {
    let mut app = app_showing(Screen::Tone);
    assert!(!app.on_computer());
    let sent = press(&mut app, KeyCode::Char('f'));
    assert!(
        sent.iter()
            .all(|intent| !matches!(intent, Intent::HostEq(_))),
        "{sent:?}"
    );
}

#[test]
fn l_and_x_jump_to_their_panels_and_again_go_back_to_the_strip() {
    let mut app = stages_app(Space::Speakers);
    press(&mut app, KeyCode::Char('L'));
    assert_eq!(app.tone.row, ToneRow::Ld(Ld::On));
    press(&mut app, KeyCode::Char('L'));
    assert_eq!(app.tone.row, ToneRow::Strip(Stage::Loudness));
    press(&mut app, KeyCode::Char('X'));
    assert_eq!(app.tone.row, ToneRow::Crossfeed(Xf::Space));
    // from the device screen they open the tone screen on the panel
    app.show(Screen::Device);
    press(&mut app, KeyCode::Char('L'));
    assert_eq!(
        (app.screen, app.tone.row),
        (Screen::Tone, ToneRow::Ld(Ld::On))
    );
}

#[test]
fn the_bypass_panel_takes_the_arrows_enter_and_space() {
    let mut app = stages_app(Space::Speakers);
    app.enter_stage(Stage::Bypass);
    assert_eq!(
        op(&press(&mut app, KeyCode::Right)),
        HostEqOp::Bypass { on: true }
    );
    assert!(press(&mut app, KeyCode::Left).is_empty(), "already off");
    assert_eq!(
        op(&press(&mut app, KeyCode::Enter)),
        HostEqOp::Bypass { on: true }
    );
    assert_eq!(
        op(&press(&mut app, KeyCode::Char(' '))),
        HostEqOp::Bypass { on: true }
    );
}

#[test]
fn the_points_keys_act_only_in_the_eq_panel() {
    let mut app = stages_app(Space::Speakers);
    app.enter_stage(Stage::Eq);
    assert!(matches!(
        op(&press(&mut app, KeyCode::Char('a'))),
        HostEqOp::Add { .. }
    ));
    for stage in [Stage::Loudness, Stage::Space, Stage::Bypass] {
        app.enter_stage(stage);
        for key in ['a', 'x', 'y', '0', '3', '[', ']'] {
            assert!(
                press(&mut app, KeyCode::Char(key)).is_empty(),
                "{stage:?} {key}: no point is touched"
            );
        }
    }
    app.focus_strip(Stage::Eq);
    assert!(press(&mut app, KeyCode::Char('a')).is_empty());
    assert_eq!(app.computer_points().len(), 9, "nothing was added");
}

#[test]
fn the_eq_panel_steps_the_gain_with_the_arrows_and_types_with_enter() {
    let mut app = stages_app(Space::Speakers);
    app.enter_stage(Stage::Eq);
    let sent = press(&mut app, KeyCode::Right);
    assert!(matches!(
        op(&sent),
        HostEqOp::Set { g: Some(g), .. } if (g - 8.0).abs() < 1e-9
    ));
    assert!(press(&mut app, KeyCode::Enter).is_empty());
    assert!(
        app.tone.point_line.is_some(),
        "Enter opens the point's line"
    );
}

#[test]
fn up_on_the_first_point_goes_to_the_strip_and_down_walks_the_points() {
    let mut app = stages_app(Space::Speakers);
    app.enter_stage(Stage::Eq);
    press(&mut app, KeyCode::Down);
    assert_eq!(app.tone.row, ToneRow::Point(1));
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Up);
    assert_eq!(app.tone.row, ToneRow::Strip(Stage::Eq));
}

#[test]
fn a_stage_the_view_loses_gives_the_keys_back_to_the_header() {
    let mut app = stages_app(Space::Speakers);
    app.focus_strip(Stage::Bypass);
    app.host.computer.eq.as_mut().expect("a view").bypass = None;
    app.note_computer_view();
    assert_eq!(app.tone.row, ToneRow::Mode);
    assert_eq!(
        app.open_stage(),
        Stage::Eq,
        "the panel falls back to the EQ's"
    );
}

#[test]
fn the_keys_help_lists_this_screens_keys_first() {
    let mut app = stages_app(Space::Speakers);
    app.enter_stage(Stage::Loudness);
    let here = crate::tui::keys::this_screen(&app);
    assert!(here.iter().any(|(label, _, _)| *label == "Tab/⇧Tab"));
    assert!(here.iter().any(|(label, _, _)| *label == "r"));
    app.focus_strip(Stage::Loudness);
    let strip = crate::tui::keys::this_screen(&app);
    assert!(strip.iter().any(|(label, _, _)| label.contains("Enter")));
    assert!(crate::tui::keys::this_screen(&app_showing(Screen::Tone)).is_empty());
}

/// Every key `?` lists under "This screen" does something where it is listed, on every row a
/// stage has: the list cannot drift from the keys.
#[test]
fn every_key_this_screen_lists_does_something_on_its_row() {
    let rows = |app: &App| {
        let mut rows = vec![ToneRow::Strip(Stage::Eq), ToneRow::Strip(Stage::Space)];
        for stage in Stage::ALL_FOR_TEST {
            rows.extend(app.panel_rows(stage));
        }
        rows
    };
    let base = stages_app(Space::Crossfeed);
    for row in rows(&base) {
        let mut app = stages_app(Space::Crossfeed);
        app.tone.row = row;
        for (label, does, code) in crate::tui::keys::this_screen(&app) {
            // Enter has nothing to apply or type on the Model and Preset rows, which the list's
            // words do not name; the fixture's model has one preset, so nothing steps there either
            if matches!(row, ToneRow::Crossfeed(Xf::Model | Xf::Preset)) && code == KeyCode::Enter {
                continue;
            }
            // an arrow that leaves a value where it is (at its end) is tried the other way too
            let tries: &[KeyCode] = match code {
                KeyCode::Right if row == ToneRow::Crossfeed(Xf::Preset) => &[],
                KeyCode::Right => &[KeyCode::Right, KeyCode::Left],
                _ => &[code],
            };
            let acted = tries.iter().any(|code| {
                let mut app = stages_app(Space::Crossfeed);
                app.tone.row = row;
                if *code == KeyCode::Enter && row == ToneRow::Crossfeed(Xf::Space) {
                    // Enter plays the highlighted choice: there has to be one
                    app.key(crossterm::event::KeyEvent::from(KeyCode::Right));
                }
                let before = app.snapshot_for_test();
                let intents = app.key(crossterm::event::KeyEvent::from(*code));
                app.snapshot_for_test() != before
                    || !intents.is_empty()
                    || app.modal_open()
                    || app.toast().is_some()
                    || app.tone.point_line.is_some()
                    || app.tone.plays.is_some()
            });
            assert!(
                acted || row == ToneRow::Crossfeed(Xf::Preset),
                "{row:?}: {label} ({does}) does nothing"
            );
        }
    }
}

/// `,` and `.` never change the crossfeed model, on the Model row of the panel or of the device
/// pane; `←→` do.
#[test]
fn the_dot_keys_do_nothing_on_the_model_row() {
    let mut app = stages_app(Space::Crossfeed);
    app.focus_crossfeed(Xf::Model);
    for key in [',', '.'] {
        assert!(press(&mut app, KeyCode::Char(key)).is_empty(), "{key}");
    }
    assert!(matches!(
        op(&press(&mut app, KeyCode::Right)),
        HostEqOp::Crossfeed { model: Some(_), .. }
    ));
}

/// On the device pane the Plays row follows the panel: the arrows highlight and `Enter` plays;
/// a click on its `‹ ›` highlights too.
#[test]
fn on_the_device_pane_plays_highlights_with_the_arrows_and_enter_plays() {
    let mut app = stages_app(Space::Off);
    app.show(Screen::Device);
    app.pane = crate::tui::app::Pane::Tone;
    app.focus_crossfeed(Xf::Space);
    assert!(press(&mut app, KeyCode::Right).is_empty());
    assert_eq!(app.plays_highlight(), Space::Crossfeed);
    assert!(app.click_crossfeed(Xf::Space, 1).is_empty());
    assert_eq!(app.plays_highlight(), Space::Speakers);
    assert!(matches!(
        op(&press(&mut app, KeyCode::Enter)),
        HostEqOp::Speakers { on: Some(true), .. }
    ));
}

/// The device pane's Preset row says `(the only one)` for a model with one preset, as the panel
/// does.
#[test]
fn the_device_pane_preset_row_says_the_only_one() {
    let mut app = stages_app(Space::Crossfeed);
    app.show(Screen::Device);
    let rows = crate::tui::test_support::text(&crate::tui::test_support::frame(&app, 120, 40));
    assert!(
        rows.iter()
            .any(|row| row.contains("Preset") && row.contains("(the only one)")),
        "{rows:#?}"
    );
}

/// A highlight on the device pane's Plays row is not left reading as a choice once the keys move
/// to another row: the row shows what plays.
#[test]
fn the_device_pane_plays_highlight_does_not_outlive_the_row() {
    let mut app = stages_app(Space::Off);
    app.show(Screen::Device);
    app.pane = crate::tui::app::Pane::Tone;
    app.focus_crossfeed(Xf::Space);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.plays_highlight(), Space::Crossfeed);
    press(&mut app, KeyCode::Up);
    let rows = crate::tui::test_support::text(&crate::tui::test_support::frame(&app, 120, 40));
    let row = rows
        .iter()
        .find(|row| row.contains("Space"))
        .expect("the Space row");
    assert!(row.contains("Off") && !row.contains("Enter plays"), "{row}");
}

/// A hand-edited preset (`your own`) is not "the only one" of its model.
#[test]
fn a_hand_edited_preset_is_not_the_only_one() {
    let mut app = stages_app(Space::Crossfeed);
    app.host
        .computer
        .eq
        .as_mut()
        .unwrap()
        .crossfeed
        .as_mut()
        .unwrap()
        .preset = None;
    app.show(Screen::Device);
    let rows = crate::tui::test_support::text(&crate::tui::test_support::frame(&app, 120, 40));
    assert!(
        rows.iter()
            .any(|r| r.contains("Preset") && r.contains("your own")),
        "{rows:#?}"
    );
    assert!(
        !rows.iter().any(|r| r.contains("your own (the only one)")),
        "{rows:#?}"
    );
}
