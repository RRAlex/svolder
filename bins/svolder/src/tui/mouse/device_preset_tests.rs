// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device's own preset list answers the mouse as the tone control's preset list does: a click
//! on a row picks it, the wheel moves the selection, a click on a `more` word goes to that end,
//! and a click outside closes it.

use crossterm::event::{KeyCode, MouseEventKind};

use super::*;
use crate::backend::DeviceRow;
use crate::tui::boxes::device_presets::{self, Row};
use crate::tui::test_support::{click, drawn, mouse_at, profile_named};
use svolder_ipc::TuiConfig;

/// A Loki Max with its device preset list open.
fn listing() -> App {
    let mut app = App::new(
        profile_named("lokimax"),
        "Loki Max".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("Loki Max")],
    );
    device_presets::open(&mut app);
    app
}

/// The first cell of the row hit `hit` over the frame `hits`.
fn cell_of(hits: &HitMap, hit: Hit) -> (u16, u16) {
    let area = hits
        .entries()
        .find(|(_, drawn)| *drawn == hit)
        .map(|(area, _)| area)
        .unwrap_or_else(|| panic!("{hit:?} was not drawn"));
    (area.x, area.y)
}

/// The index of the first row of the list that is `wanted`.
fn index_of(app: &App, wanted: Row) -> usize {
    device_presets::rows(&app.profile)
        .iter()
        .position(|row| *row == wanted)
        .expect("the row exists")
}

#[test]
fn a_click_on_a_row_picks_it() {
    let app = listing();
    let (_, hits) = drawn(&app, 80, 24);
    let built_in = index_of(&app, Row::Setting(0));
    for row in [0, built_in] {
        let (x, y) = cell_of(&hits, Hit::PresetRow(row));
        assert_eq!(
            resolve(&hits, click(x, y), &app),
            Some(MouseAction::PresetPick(row)),
            "row {row}"
        );
    }
}

#[test]
fn the_header_is_not_a_row_to_click() {
    let app = listing();
    let (_, hits) = drawn(&app, 80, 24);
    let header = index_of(&app, Row::Header);
    assert!(
        hits.entries().all(|(_, hit)| hit != Hit::PresetRow(header)),
        "the header was drawn as a row"
    );
}

#[test]
fn the_wheel_moves_the_selection_like_the_arrows() {
    let app = listing();
    let (_, hits) = drawn(&app, 80, 24);
    let (x, y) = cell_of(&hits, Hit::PresetRow(1));
    assert_eq!(
        resolve(&hits, mouse_at(MouseEventKind::ScrollDown, x, y), &app),
        Some(MouseAction::Key(KeyCode::Down))
    );
    assert_eq!(
        resolve(&hits, mouse_at(MouseEventKind::ScrollUp, x, y), &app),
        Some(MouseAction::Key(KeyCode::Up))
    );
}

#[test]
fn a_click_outside_the_box_closes_it() {
    let app = listing();
    let (_, hits) = drawn(&app, 80, 24);
    assert_eq!(
        resolve(&hits, click(0, 23), &app),
        Some(MouseAction::Key(KeyCode::Esc))
    );
}

#[test]
fn a_click_in_the_box_off_the_rows_asks_for_nothing() {
    let app = listing();
    let (_, hits) = drawn(&app, 80, 24);
    let last = device_presets::rows(&app.profile).len() - 1;
    let (x, y) = cell_of(&hits, Hit::PresetRow(last));
    // The blank line under the rows is inside the box.
    assert_eq!(resolve(&hits, click(x, y + 1), &app), None);
}

#[test]
fn a_click_on_the_more_words_goes_to_that_end() {
    // More built-ins than the box has rows: its border says which way the rest are.
    let mut app = listing();
    if let Some(table) = app.profile.builtin_settings.as_mut() {
        let first = table.settings[0].clone();
        table.settings.extend(std::iter::repeat_n(first, 20));
    }
    let (_, hits) = drawn(&app, 80, 24);
    let (x, y) = cell_of(&hits, Hit::PresetEnd(false));
    assert_eq!(
        resolve(&hits, click(x, y), &app),
        Some(MouseAction::PresetGo(usize::MAX))
    );
    for _ in 0..30 {
        app.key(crossterm::event::KeyEvent::from(KeyCode::Down));
    }
    let (_, hits) = drawn(&app, 80, 24);
    let (x, y) = cell_of(&hits, Hit::PresetEnd(true));
    assert_eq!(
        resolve(&hits, click(x, y), &app),
        Some(MouseAction::PresetGo(0))
    );
}

#[test]
fn the_save_question_is_not_clicked() {
    let mut app = listing();
    app.key(crossterm::event::KeyEvent::from(KeyCode::Char('u')));
    let (_, hits) = drawn(&app, 80, 24);
    assert!(
        hits.entries()
            .all(|(_, hit)| !matches!(hit, Hit::PresetRow(_)))
    );
}

/// A listing of a Loki Max with many built-ins, scrolled to its last row.
fn scrolled() -> App {
    let mut app = listing();
    if let Some(table) = app.profile.builtin_settings.as_mut() {
        let first = table.settings[0].clone();
        table.settings.extend(std::iter::repeat_n(first, 20));
    }
    for _ in 0..40 {
        app.key(crossterm::event::KeyEvent::from(KeyCode::Down));
    }
    app
}

#[test]
fn a_click_on_a_row_of_a_scrolled_list_picks_the_drawn_row() {
    let app = scrolled();
    let last = device_presets::rows(&app.profile).len() - 1;
    let (_, hits) = drawn(&app, 80, 24);
    let (x, y) = cell_of(&hits, Hit::PresetRow(last));
    assert_eq!(
        resolve(&hits, click(x, y), &app),
        Some(MouseAction::PresetPick(last))
    );
    // The first slot is scrolled out: no row of it is drawn.
    assert!(hits.entries().all(|(_, hit)| hit != Hit::PresetRow(0)));
}

#[test]
fn focus_moves_to_a_row_clamps_past_the_end_and_skips_the_header() {
    let mut app = listing();
    let rows = device_presets::rows(&app.profile);
    let header = index_of(&app, Row::Header);
    let row = |app: &App| app.presets.device_list.as_ref().map(|list| list.row);
    device_presets::focus(&mut app, 2);
    assert_eq!(row(&app), Some(2));
    device_presets::focus(&mut app, header);
    assert_eq!(row(&app), Some(2), "the header took focus");
    device_presets::focus(&mut app, usize::MAX);
    assert_eq!(row(&app), Some(rows.len() - 1));
    device_presets::focus(&mut app, 0);
    assert_eq!(row(&app), Some(0));
}
