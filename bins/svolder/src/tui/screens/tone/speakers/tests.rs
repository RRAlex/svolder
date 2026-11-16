// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::tui::app::{App, Screen, ToneRow, Xf};
use crate::tui::test_support::{app_showing, computer_eq_with_speakers, frame, text};

const SETS: [(&str, &str, &str); 2] = [
    ("sadie-d2", "no", "SADIE II, D2"),
    ("room-1", "with", "Control room 1"),
];

/// An app on the tone screen's computer side, the speakers `on`, the chosen set `sadie-d2` or
/// `room-1`.
fn app_on(screen: Screen, on: bool, chosen: &str) -> App {
    let mut app = app_showing(Screen::Device);
    app.host.computer.eq = Some(computer_eq_with_speakers(on, Some(chosen), &SETS));
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

#[test]
fn folded_the_line_names_the_choice_the_set_and_how_it_sounds() {
    for (width, height) in [(80, 24), (120, 36)] {
        let on = rows(&app_on(Screen::Tone, true, "sadie-d2"), width, height);
        let line = row_with(&on, "Space").expect("the line");
        assert!(
            line.contains(
                "Virtual speakers \u{b7} No room \u{b7} HRTF \u{b7} SADIE II, D2 \u{b7} \u{2212}7 dB"
            ),
            "{line}"
        );
        assert!(line.contains("(X)"), "{line}");
        assert!(!on.iter().any(|r| r.contains("Licence")), "folded");
        let off = rows(&app_on(Screen::Tone, false, "sadie-d2"), width, height);
        let line = row_with(&off, "Space").expect("the line");
        assert!(line.contains("off") && line.contains("(X)"), "{line}");
    }
}

#[test]
fn unfolded_at_80_by_24_the_block_has_the_facts_and_the_two_strips() {
    let mut app = app_on(Screen::Tone, true, "sadie-d2");
    app.focus_crossfeed(Xf::Space);
    let rows = rows(&app, 80, 24);
    for needle in [
        "Space",
        "\u{2039} Virtual speakers \u{203a}",
        "Off \u{b7} Crossfeed \u{b7} Virtual speakers",
        "No room \u{b7} HRTF   SADIE II, D2",
        "S sets",
        "Apache-2.0 \u{b7} sample credit words",
        "left 30.0\u{b0} \u{b7} right 330.0\u{b0} \u{b7} 1.5 m \u{b7} 5.3 ms",
        "about 7 dB quieter than Off",
        "Same ear",
        "Other ear",
    ] {
        assert!(
            rows.iter().any(|r| r.contains(needle)),
            "{needle:?} in\n{}",
            rows.join("\n")
        );
    }
    // the strips are level blocks
    let strips = row_with(&rows, "Same ear").expect("the strips");
    assert!(
        strips.contains('\u{2588}') || strips.contains('\u{2587}'),
        "{strips}"
    );
}

#[test]
fn unfolded_from_120_columns_the_facts_have_the_curves_beside_them() {
    let mut app = app_on(Screen::Tone, true, "room-1");
    app.focus_crossfeed(Xf::Space);
    let rows = rows(&app, 120, 36);
    for needle in [
        "With room \u{b7} BRIR   Control room 1",
        "Length",
        "5.3 ms",
        "Source",
        "sofacoustics.example/data/",
        "same ear",
        "other ear",
    ] {
        assert!(
            rows.iter().any(|r| r.contains(needle)),
            "{needle:?} in\n{}",
            rows.join("\n")
        );
    }
}

#[test]
fn the_set_row_is_the_cursor_when_the_keys_are_on_it() {
    let mut app = app_on(Screen::Tone, true, "sadie-d2");
    app.focus_crossfeed(Xf::Set);
    let rows = rows(&app, 80, 24);
    let line = row_with(&rows, "Set  ").expect("the row");
    assert!(line.contains('\u{25b6}'), "the cursor: {line}");
    let space = row_with(&rows, "Off \u{b7} Crossfeed").expect("the space row");
    assert!(!space.contains('\u{25b6}'), "{space}");
}

#[test]
fn off_or_crossfeed_unfolds_the_crossfeeds_rows_under_the_space_row() {
    let mut app = app_on(Screen::Tone, false, "sadie-d2");
    app.focus_crossfeed(Xf::Space);
    let narrow = rows(&app, 80, 24);
    let at = |needle: &str| {
        narrow
            .iter()
            .position(|r| r.contains(needle))
            .unwrap_or_else(|| panic!("{needle:?} in\n{}", narrow.join("\n")))
    };
    assert!(at("Space") < at("Crossfeed off") && at("Crossfeed off") < at("Level"));
    assert!(at("Level") < at("Crossover") && at("Crossover") < at("Delay"));
    // beside the Points in the wide tier, the Preset row first in the block
    let wide = rows(&app, 120, 36);
    assert!(row_with(&wide, "Level").is_some_and(|r| r.contains("37 %")));
    assert!(
        wide.iter()
            .any(|r| r.contains("Points") && r.contains("Crossfeed"))
    );
}

#[test]
fn the_graph_keeps_a_data_row_at_the_small_sizes_with_the_block_unfolded() {
    for (width, height) in [(80, 24), (100, 24), (80, 30)] {
        let mut app = app_on(Screen::Tone, true, "sadie-d2");
        app.focus_crossfeed(Xf::Space);
        let area = ratatui::layout::Rect::new(0, 0, width, height);
        let curve = super::super::curve_of(area, &app).expect("a curve");
        assert!(curve.height >= 1, "{width}x{height}: {curve:?}");
        let rows = rows(&app, width, height);
        for needle in [
            "Response", "Licence", "Angles", "Level", "Same ear", "Points",
        ] {
            assert!(
                rows.iter().any(|r| r.contains(needle)),
                "{width}x{height}: {needle:?} in\n{}",
                rows.join("\n")
            );
        }
        // the Response box is closed: its bottom border is drawn above the Space block
        let at = |needle: &str| rows.iter().position(|r| r.contains(needle)).expect("row");
        assert!(
            rows[at("Space \u{2500}") - 1].contains('\u{2518}'),
            "{}",
            rows.join("\n")
        );
    }
}

#[test]
fn the_block_takes_the_graphs_rows_only_while_focused() {
    let mut app = app_on(Screen::Tone, true, "sadie-d2");
    let area = ratatui::layout::Rect::new(0, 0, 80, 40);
    let folded = super::super::curve_of(area, &app).expect("a curve").height;
    app.focus_crossfeed(Xf::Space);
    let open = super::super::curve_of(area, &app).expect("a curve").height;
    assert!(open < folded, "{open} < {folded}");
    app.leave_crossfeed();
    assert_eq!(
        super::super::curve_of(area, &app).expect("a curve").height,
        folded
    );
}

#[test]
fn the_crossfeed_row_has_the_whole_width_under_the_space_row_at_120_columns() {
    let mut app = app_on(Screen::Tone, false, "sadie-d2");
    app.focus_crossfeed(Xf::Space);
    let rows = rows(&app, 120, 36);
    let line = row_with(&rows, "Crossfeed off").expect("the Preset row");
    assert!(line.contains("\u{2039} bs2b default \u{203a}"), "{line}");
    let at = |needle: &str| rows.iter().position(|r| r.contains(needle)).expect(needle);
    assert_eq!(at("Crossfeed off"), at("Off \u{b7} Crossfeed") + 1);
}

#[test]
fn the_strips_read_the_curves_level_by_level() {
    use super::{BLOCKS, strip};
    assert_eq!(
        strip(&[3.0, -30.0], 2),
        format!("{}{}", BLOCKS[7], BLOCKS[0])
    );
    assert_eq!(strip(&[], 4), "");
    assert_eq!(strip(&[0.0], 0), "");
    // one column reads the first point
    assert_eq!(strip(&[3.0, -30.0], 1), BLOCKS[7].to_string());
    // out of range is held to the ends
    assert_eq!(
        strip(&[40.0, -90.0], 2),
        format!("{}{}", BLOCKS[7], BLOCKS[0])
    );
}

#[test]
fn the_key_title_says_what_comma_and_dot_step_on_the_row_the_keys_are_on() {
    let mut app = app_on(Screen::Tone, true, "sadie-d2");
    app.focus_crossfeed(Xf::Space);
    let on_space = rows(&app, 80, 24);
    assert!(
        row_with(&on_space, ", . choice").is_some(),
        "{}",
        on_space.join("\n")
    );
    assert!(row_with(&on_space, ", . set").is_none());
    app.focus_crossfeed(Xf::Set);
    let on_set = rows(&app, 80, 24);
    assert!(
        row_with(&on_set, ", . set").is_some(),
        "{}",
        on_set.join("\n")
    );
}

#[test]
fn a_set_with_no_angles_shows_no_empty_angles_row() {
    for (width, height) in [(80, 24), (120, 36)] {
        let mut app = app_on(Screen::Tone, true, "sadie-d2");
        if let Some(view) = app
            .host
            .computer
            .eq
            .as_mut()
            .and_then(|eq| eq.speakers.as_mut())
        {
            view.angles = String::new();
        }
        app.focus_crossfeed(Xf::Space);
        let rows = rows(&app, width, height);
        assert!(
            row_with(&rows, "Angles").is_none(),
            "{width}: {}",
            rows.join("\n")
        );
        assert!(row_with(&rows, "5.3 ms").is_some(), "the length stays");
    }
}
