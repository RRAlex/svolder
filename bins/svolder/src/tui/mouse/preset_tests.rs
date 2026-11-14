// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The preset list answers the mouse: a click on a row picks it, the wheel moves the selection,
//! a click on a `more` word goes to that end, and a click outside closes it.

use crossterm::event::{KeyCode, KeyEvent, MouseEventKind};
use svolder_profiles::EqMode;

use super::*;
use crate::tui::test_support::presets::{own, resting};
use crate::tui::test_support::{click, drawn, mouse_at};

/// The preset list open over the tone-control screen of a Gungnir 2, with `own_presets` presets of the user's own after the built-ins.
fn listing(own_presets: usize) -> App {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    app.presets.saved = (0..own_presets)
        .map(|at| own(&format!("Mine {at}"), 5, 1.0))
        .collect();
    app.key(KeyEvent::from(KeyCode::Char('p')));
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

#[test]
fn a_click_on_a_row_picks_it() {
    let app = listing(1);
    let (_, hits) = drawn(&app, 80, 24);
    for row in [0, 3, 4] {
        let (x, y) = cell_of(&hits, Hit::PresetRow(row));
        assert_eq!(
            resolve(&hits, click(x, y), &app),
            Some(MouseAction::PresetPick(row)),
            "row {row}"
        );
    }
}

/// The text of the cells `hit` was drawn over in `buffer`.
fn text_of(buffer: &ratatui::buffer::Buffer, hits: &HitMap, hit: Hit) -> String {
    let area = hits
        .entries()
        .find(|(_, drawn)| *drawn == hit)
        .map(|(area, _)| area)
        .unwrap_or_else(|| panic!("{hit:?} was not drawn"));
    (area.left()..area.right())
        .filter_map(|x| buffer.cell((x, area.y)).map(|cell| cell.symbol()))
        .collect()
}

#[test]
fn a_click_on_a_row_of_a_scrolled_list_picks_that_row() {
    let mut app = listing(20);
    for _ in 0..30 {
        app.key(KeyEvent::from(KeyCode::Down));
    }
    let (buffer, hits) = drawn(&app, 80, 24);
    // 4 built-ins and 20 of the user's own: the last row is index 23, "Mine 19".
    assert!(text_of(&buffer, &hits, Hit::PresetRow(23)).contains("Mine 19"));
    assert!(text_of(&buffer, &hits, Hit::PresetRow(22)).contains("Mine 18"));
    for row in [22, 23] {
        let (x, y) = cell_of(&hits, Hit::PresetRow(row));
        assert_eq!(
            resolve(&hits, click(x, y), &app),
            Some(MouseAction::PresetPick(row)),
            "row {row}"
        );
    }
}

#[test]
fn the_wheel_moves_the_selection_like_the_arrows() {
    let app = listing(1);
    let (_, hits) = drawn(&app, 80, 24);
    let (x, y) = cell_of(&hits, Hit::PresetRow(2));
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
    let app = listing(1);
    let (_, hits) = drawn(&app, 80, 24);
    assert_eq!(
        resolve(&hits, click(0, 23), &app),
        Some(MouseAction::Key(KeyCode::Esc))
    );
}

#[test]
fn a_click_in_the_box_off_the_rows_asks_for_nothing() {
    let app = listing(1);
    let (_, hits) = drawn(&app, 80, 24);
    let (x, y) = cell_of(&hits, Hit::PresetRow(4));
    // The blank line under the rows and the key lines are inside the box.
    assert_eq!(resolve(&hits, click(x, y + 2), &app), None);
}

#[test]
fn a_click_on_the_more_words_goes_to_that_end() {
    // More presets than the box has rows: the border says which way the rest are.
    let mut app = listing(20);
    let (buffer, hits) = drawn(&app, 80, 24);
    assert_eq!(text_of(&buffer, &hits, Hit::PresetEnd(false)), " more ↓ ");
    let (x, y) = cell_of(&hits, Hit::PresetEnd(false));
    assert_eq!(
        resolve(&hits, click(x, y), &app),
        Some(MouseAction::PresetGo(usize::MAX))
    );
    for _ in 0..30 {
        app.key(KeyEvent::from(KeyCode::Down));
    }
    let (buffer, hits) = drawn(&app, 80, 24);
    assert_eq!(text_of(&buffer, &hits, Hit::PresetEnd(true)), " more ↑ ");
    let (x, y) = cell_of(&hits, Hit::PresetEnd(true));
    assert_eq!(
        resolve(&hits, click(x, y), &app),
        Some(MouseAction::PresetGo(0))
    );
}

#[test]
fn another_step_of_the_box_is_not_clicked() {
    let mut app = listing(1);
    app.key(KeyEvent::from(KeyCode::Char('n')));
    let (_, hits) = drawn(&app, 80, 24);
    assert_eq!(
        resolve(&hits, mouse_at(MouseEventKind::ScrollDown, 40, 10), &app),
        None
    );
}
