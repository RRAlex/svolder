// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::layout::Rect;

use crate::tui::app::{App, Screen, ToneRow, Xf};
use crate::tui::test_support::{app_showing, computer_eq_with_crossfeed, frame, text};

/// An app on `screen`, computer side, crossfeed on `model`'s default preset.
fn app_on(screen: Screen, on: bool, model: &str) -> App {
    let mut app = app_showing(Screen::Device);
    app.host.computer.eq = Some(computer_eq_with_crossfeed(on, model, None));
    app.flip_side();
    if screen == Screen::Tone {
        app.show(Screen::Tone);
        app.tone.row = ToneRow::Mode;
    }
    app
}

fn rows(app: &App, width: u16, height: u16) -> Vec<String> {
    text(&frame(app, width, height))
}

fn row_with<'a>(rows: &'a [String], needle: &str) -> Option<&'a String> {
    rows.iter().find(|row| row.contains(needle))
}

fn graph_height(app: &App, width: u16, height: u16) -> u16 {
    super::super::curve_of(Rect::new(0, 0, width, height), app)
        .expect("the graph draws a curve")
        .height
}

#[test]
fn folded_the_section_is_one_line_between_the_graph_and_the_points() {
    for (width, height) in [(80, 24), (120, 36)] {
        let off = rows(&app_on(Screen::Tone, false, "bauer"), width, height);
        let line = row_with(&off, "Crossfeed").expect("the line");
        assert!(line.contains("off") && line.contains("(X)"), "{line}");
        let on = rows(&app_on(Screen::Tone, true, "bauer"), width, height);
        let line = row_with(&on, "Crossfeed").expect("the line");
        assert!(
            line.contains(
                "on \u{b7} Bauer \u{b7} bs2b default \u{b7} 37 % \u{b7} 700 Hz \u{b7} 0.2"
            ),
            "{line}"
        );
        // between the Response block's bottom border and the Points block's top
        let at = |needle: &str| on.iter().position(|r| r.contains(needle)).unwrap();
        assert!(at("Response") < at("Crossfeed") && at("Crossfeed") < at("Points"));
        assert!(!on.iter().any(|r| r.contains("Level")), "folded");
    }
}

#[test]
fn unfolding_opens_beside_the_points_in_the_wide_tier_and_keeps_the_graph_height() {
    let mut app = app_on(Screen::Tone, true, "bauer");
    let folded = graph_height(&app, 120, 36);
    app.focus_crossfeed(Xf::Preset);
    assert_eq!(graph_height(&app, 120, 36), folded);
    let rows = rows(&app, 120, 36);
    let line = row_with(&rows, "Level").expect("the Level row");
    assert!(line.contains("37 %"), "{line}");
    assert!(row_with(&rows, "Crossover").is_some_and(|r| r.contains("700 Hz")));
    assert!(row_with(&rows, "Delay").is_some());
    // beside the Points: the block's title shares a row with the Points title
    assert!(
        rows.iter()
            .any(|r| r.contains("Points") && r.contains("Crossfeed")),
        "{rows:#?}"
    );
}

#[test]
fn unfolding_at_80_by_24_takes_the_graphs_rows_only_while_focused() {
    let mut app = app_on(Screen::Tone, true, "bauer");
    let folded = graph_height(&app, 80, 24);
    app.focus_crossfeed(Xf::Preset);
    let open = graph_height(&app, 80, 24);
    assert_eq!(
        open + 3,
        folded,
        "Level, Crossover and Delay take three rows"
    );
    let rows = rows(&app, 80, 24);
    for word in ["Level", "Crossover", "Delay"] {
        assert!(row_with(&rows, word).is_some(), "{word}");
    }
    assert!(
        row_with(&rows, "Points").is_some(),
        "the table keeps its place"
    );
    app.tone.row = ToneRow::Mode;
    assert_eq!(
        graph_height(&app, 80, 24),
        folded,
        "folds when the keys leave"
    );
}

#[test]
fn a_model_with_no_delay_says_none_and_a_custom_preset_says_where_it_came_from() {
    let mut app = app_on(Screen::Tone, true, "mid-side");
    app.focus_crossfeed(Xf::Preset);
    let shown = rows(&app, 80, 24);
    assert!(row_with(&shown, "Delay").is_some_and(|r| r.contains("none")));
    let view = app
        .host
        .computer
        .eq
        .as_mut()
        .unwrap()
        .crossfeed
        .as_mut()
        .unwrap();
    view.preset = None;
    view.edited_from = Some("Mid/Side bass".into());
    let shown = rows(&app, 120, 36);
    assert!(row_with(&shown, "Custom").is_some_and(|r| r.contains("edited from")));
}

#[test]
fn the_edit_line_takes_the_place_of_the_rows_value() {
    let mut app = app_on(Screen::Tone, true, "bauer");
    app.focus_crossfeed(Xf::Crossover);
    app.open_xf_line(Xf::Crossover);
    let shown = rows(&app, 80, 24);
    let line = row_with(&shown, "Crossover").expect("the row");
    assert!(
        line.contains("[700] Hz") && line.contains("Esc cancels"),
        "{line}"
    );
}

#[test]
fn the_curves_are_drawn_beside_the_points_where_they_fit() {
    let mut app = app_on(Screen::Tone, true, "bauer");
    app.focus_crossfeed(Xf::Preset);
    let shown = rows(&app, 120, 36);
    assert!(row_with(&shown, "same ear").is_some_and(|r| r.contains("other ear")));
}

#[test]
fn a_daemon_without_crossfeed_draws_the_screen_as_before() {
    let mut app = app_showing(Screen::Device);
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq(
        true,
        &[("PK", 1000.0, 0.0, 1.0)],
    ));
    app.flip_side();
    app.show(Screen::Tone);
    assert!(!rows(&app, 80, 24).iter().any(|r| r.contains("Crossfeed")));
}

#[test]
fn the_device_pane_keeps_the_crossfeed_row_at_its_bottom_under_the_points() {
    for (width, height) in [(80, 24), (120, 36)] {
        let app = app_on(Screen::Device, true, "bauer");
        let shown = rows(&app, width, height);
        let at = |needle: &str| shown.iter().position(|r| r.contains(needle));
        let row = at("Crossfeed").expect("the row");
        let points = at("#  Type").expect("the points table");
        assert!(row > points, "{width}x{height}: under the table");
        assert!(
            shown[row].contains("on \u{203a} Bauer \u{b7} bs2b"),
            "{}",
            shown[row]
        );
        let summary = &shown[row + 1];
        assert!(
            summary.contains("37 % \u{b7} \u{2212}") && summary.contains("700 Hz"),
            "{width}x{height}: {summary}"
        );
        // the EQ keeps its place at the top
        assert!(at("Mode").unwrap() < points);
    }
}
