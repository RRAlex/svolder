// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::{Color, Modifier};
use serde_json::json;
use svolder_ipc::{ColorDepth, TuiConfig, ValueObject};
use svolder_profiles::Registry;
use svolder_profiles::{Control, ControlKind};
use svolder_proto::ValueType;

use crate::tui::app::{App, Screen};
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::render;
use crate::tui::test_support::{
    AEGIR_SCALE, assert_rows, feed, frame, frame_with_theme, profile, text, theme_for,
    with_log_scale,
};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets;

/// One reading as a backend hands it over.
fn vo(id: &str, raw: serde_json::Value, value_type: &str) -> ValueObject {
    ValueObject {
        heard_at_ms: None,
        id: id.into(),
        key: None,
        label: "?".into(),
        value_type: value_type.into(),
        raw,
        display: String::new(),
        value: None,
        unit: None,
    }
}

/// An app on the built-in profile with the default `[tui]` settings but for the graphs, which
/// are off: the goldens that predate them pin the screen as it draws without them.
fn app() -> App {
    let tui = TuiConfig {
        graphs: false,
        ..TuiConfig::default()
    };
    App::new(profile(), "gungnir".into(), &tui, Vec::new())
}

/// An app on the telemetry screen with the mockup's mix of readings: a control's own
/// readout, an unlabelled entry, a tone-control band, a firmware query, a fault, and one
/// entry updated fast enough to draw in the fast-rate role.
fn mockup_app() -> App {
    let mut app = app();
    let base = Instant::now();
    feed(&mut app, vo("6", json!(2), "u8"), base);
    for n in 1..=3 {
        feed(
            &mut app,
            vo("7.3", json!(0), "u16"),
            base + Duration::from_secs(3 * n),
        );
    }
    feed(&mut app, vo("12.1.2", json!(64), "u8"), base);
    feed(&mut app, vo("210.2.1", json!(3), "u8"), base);
    feed(&mut app, vo("240.1", json!(false), "bool"), base);
    for n in 0..10 {
        feed(
            &mut app,
            vo("9", json!(1180), "u16"),
            base + Duration::from_millis(11_000 + 100 * n),
        );
    }
    app.now = base + Duration::from_secs(12);
    app.screen = Screen::Telemetry;
    app.clock = "14:02".into();
    app
}

#[test]
fn the_telemetry_screen_lists_every_reading_labelled_aged_and_rated() {
    let mut blank_rows = vec![
        "│                                                                              │";
        13
    ];
    let mut expected = vec![
        " svolder  Gungnir 2 · fw 3  ● connected                           mouse:on 14:02",
        "─ Controls ── EQ ──┤ Telemetry ├── Devices ───────────────── ? help ─ esc back ─",
        "┌ Telemetry ──────────────────────────────────────────── refresh ─ 6 functions ┐",
        "│   ID       Type  Value       Label                               Age  Rate   │",
        "│ ▶ 6        u8    2           Sample rate (48 kHz)                12s     -   │",
        "│   7.3      u16   0           ?                                    3s 0.3/s   │",
        "│   9        u16   1180        ?                                  0.1s  10/s   │",
        "│   12.1.2   u8    64          EQ band 1 freq                      12s     -   │",
        "│   210.2.1  u8    3           Firmware                            12s     -   │",
        "│   240.1    bool  false       ?                                   12s     -   │",
    ];
    expected.append(&mut blank_rows);
    expected
        .push("└──────────────────────────────────────────────────────────────────────────────┘");
    assert_rows(&frame(&mockup_app(), 80, 24), &expected);
}

/// The Rate field is held off the pane's right border by as much as the cursor column
/// holds the first field off its left, on the column titles and on every row under them:
/// the row is framed the same on both sides rather than running into the border.
#[test]
fn the_rate_field_stands_off_the_right_border_by_the_cursor_columns_width() {
    let rows = text(&frame(&mockup_app(), 80, 24));
    // The column titles and the six readings under them.
    for row in rows.iter().skip(3).take(7) {
        let cells: Vec<char> = row.chars().collect();
        let border = cells.len() - 1;
        let margin = border - super::MARGIN_WIDTH;
        assert!(
            cells[margin..border].iter().all(|c| *c == ' '),
            "the margin after the Rate field is not blank: {row}"
        );
        assert!(
            !cells[margin - 1].is_whitespace(),
            "the Rate field does not reach the margin: {row}"
        );
        assert!(
            cells[1..=super::CURSOR_WIDTH]
                .iter()
                .all(|c| c.is_whitespace() || *c == '▶'),
            "the cursor column is not the first thing on the row: {row}"
        );
    }
}

/// A boolean readout shows its value the way a toggle's does: the wire type, `true` or
/// `false`, and the control's label.
#[test]
fn a_boolean_readout_shows_its_value_and_its_label() {
    let mut app = app();
    feed(&mut app, vo("8", json!(true), "bool"), Instant::now());
    app.screen = Screen::Telemetry;
    let rows = text(&frame(&app, 80, 24));
    assert!(
        rows.iter().any(|row| {
            row.contains("bool") && row.contains("true") && row.contains("S/PDIF locked")
        }),
        "the boolean readout's row is missing: {rows:?}"
    );
}

/// A Data reading's raw value is a string such as `aa bb`; the Value column shows it bare,
/// without the JSON quotes.
#[test]
fn a_string_raw_value_shows_bare_in_the_value_column() {
    let mut app = app();
    feed(&mut app, vo("99", json!("aa bb"), "Data"), Instant::now());
    app.screen = Screen::Telemetry;
    let rows = text(&frame(&app, 80, 24));
    let row = rows
        .iter()
        .find(|r| r.contains(" 99 "))
        .expect("the Data row is drawn");
    assert!(row.contains("aa bb"), "{row}");
    assert!(!row.contains("\"aa bb\""), "the string is quoted: {row}");
}

/// With no key row the list runs down to the terminal's last row: 22 rows at 80×24.
#[test]
fn the_list_runs_down_to_the_last_row_at_80_by_24() {
    let frames = super::frames_of(ratatui::layout::Rect::new(0, 0, 80, 24));
    assert_eq!(frames.len(), 1);
    let (_, list) = frames[0];
    assert_eq!((list.y, list.height), (2, 22));
}

/// A toast says its words in the corner: a raw nudge the backend refused is said where it was
/// asked for, not on a screen the user has left.
#[test]
fn a_toast_says_its_words_in_the_corner() {
    let mut app = mockup_app();
    app.notify("210.1 is never written to");
    let rows = text(&frame(&app, 80, 24));
    assert!(
        rows[22].contains("210.1 is never written to"),
        "{}",
        rows[22]
    );
}

#[test]
fn each_row_pushes_a_list_row_hit_and_the_title_leads_back_to_the_device_screen() {
    let app = mockup_app();
    let mut hits = HitMap::default();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    terminal
        .draw(|f| {
            render::draw(f, &app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");

    assert_eq!(hits.hit_at(2, 2), Some(&Hit::Tab(Screen::Device)));
    // The six data rows, in the order the mockup lists them, one line under the column
    // titles.
    for (row, index) in (4..10).zip(0..6) {
        assert_eq!(hits.hit_at(4, row), Some(&Hit::ListRow(index)), "row {row}");
    }
}

#[test]
fn a_taller_reading_list_than_its_terminal_scrolls_to_keep_the_selection_visible() {
    let mut app = app();
    let now = Instant::now();
    for n in 1..=30u8 {
        feed(&mut app, vo(&format!("100.{n}"), json!(n), "u8"), now);
    }
    app.now = now;
    app.screen = Screen::Telemetry;
    app.selected = 29;
    let rows = text(&frame(&app, 80, 24));
    let last = rows
        .iter()
        .rposition(|r| r.contains("100.30"))
        .expect("the last reading is drawn");
    assert!(
        rows[last].contains('▶'),
        "the selected row does not carry the cursor: {}",
        rows[last]
    );
    assert!(
        rows.iter().any(|r| r.contains("100.12")),
        "the window did not scroll to keep the selection in view"
    );
    assert!(
        !rows.iter().any(|r| r.contains("100.11")),
        "the window scrolled further than the selection needed"
    );
}

/// A reading list longer than the pane says which way the rest are, dimmed on the pane's
/// right border beside its first and last rows: from the top of the list `▼` by its last
/// row and no `▲`, from its end `▲` by its first row and no `▼`, and neither for a list
/// the pane holds whole.
#[test]
fn a_list_longer_than_the_pane_marks_which_way_the_hidden_readings_are() {
    let mut app = app();
    let now = Instant::now();
    for n in 1..=30u8 {
        feed(&mut app, vo(&format!("100.{n}"), json!(n), "u8"), now);
    }
    app.now = now;
    app.screen = Screen::Telemetry;
    let dim = Theme::new(false).style(Role::Dim).add_modifier;
    // The rows run from the line under the column titles to the line above the bottom
    // border, and the pane's right border is the last column.
    let marks = |app: &App| {
        let buffer = frame(app, 80, 24);
        let rows = text(&buffer);
        let titles = rows
            .iter()
            .position(|row| row.contains("ID") && row.contains("Type"))
            .expect("the column titles");
        let bottom = rows
            .iter()
            .rposition(|row| row.starts_with('└'))
            .expect("the pane's bottom border");
        let at = |y: usize| {
            let cell = buffer
                .cell((79, u16::try_from(y).expect("a line")))
                .expect("a border cell");
            (cell.symbol().to_string(), cell.modifier.contains(dim))
        };
        (at(titles + 1), at(bottom - 1))
    };

    app.selected = 0;
    let (first, last) = marks(&app);
    assert_eq!(
        first.0, "│",
        "the top of the list is marked as having rows above it"
    );
    assert_eq!(
        last,
        (widgets::scroll::MORE_BELOW.to_string(), true),
        "the rows below are not marked"
    );

    app.selected = 29;
    let (first, last) = marks(&app);
    assert_eq!(
        first,
        (widgets::scroll::MORE_ABOVE.to_string(), true),
        "the rows above are not marked"
    );
    assert_eq!(
        last.0, "│",
        "the end of the list is marked as having rows below it"
    );

    let whole = text(&frame(&mockup_app(), 80, 24));
    assert!(
        !whole
            .iter()
            .any(|row| row.contains(widgets::scroll::MORE_ABOVE)
                || row.contains(widgets::scroll::MORE_BELOW)),
        "a list the pane holds whole is marked"
    );
}

#[test]
fn an_unlabelled_row_is_yellow_and_a_fast_rate_cell_is_green() {
    let buffer = frame(&mockup_app(), 80, 24);
    // Column 4 is where every row's id starts, column 70 where its rate cell starts; row 5 is
    // the unlabelled, slow "7.3" entry, row 6 the unlabelled entry updated fast enough to
    // earn the green rate cell on top of the row's own yellow.
    assert_eq!(
        buffer.cell((4, 5)).expect("the 7.3 row's id cell").fg,
        Color::Yellow,
        "an unlabelled row is not drawn in Role::Unknown"
    );
    assert_eq!(
        buffer.cell((4, 6)).expect("the fast row's id cell").fg,
        Color::Yellow,
        "an unlabelled row is not drawn in Role::Unknown"
    );
    assert_eq!(
        buffer.cell((70, 6)).expect("the fast row's rate cell").fg,
        Color::Green,
        "a fast update rate is not drawn in Role::RateFast even on an unlabelled row"
    );
}

/// A selected row draws bold (`Role::Selected`'s own modifier), and a fast rate cell keeps
/// its own green even on the selected row, since a span's style overrides the line's.
#[test]
fn the_selected_row_is_bold_and_its_fast_rate_cell_stays_green() {
    let mut app = mockup_app();
    // Ids sort 6, 7.3, 9, 12.1.2, 210.2.1, 240.1: index 2 is "9", the fast one.
    app.selected = 2;
    let buffer = frame(&app, 80, 24);
    let id_cell = buffer.cell((4, 6)).expect("the fast row's id cell");
    assert!(
        id_cell.modifier.contains(Modifier::BOLD),
        "the selected row is not drawn bold"
    );
    assert_eq!(
        buffer.cell((70, 6)).expect("the fast row's rate cell").fg,
        Color::Green,
        "a fast update rate is not drawn in Role::RateFast on the selected row"
    );
}

/// An empty reading list draws no data row and does not panic: `scroll_start` and the
/// selection math both go through `saturating_sub` rather than an unchecked subtraction.
#[test]
fn an_empty_reading_list_draws_nothing_and_does_not_panic() {
    let mut app = app();
    app.screen = Screen::Telemetry;
    let rows = text(&frame(&app, 80, 24));
    assert!(
        rows.iter().all(|r| !r.contains('9')),
        "nothing was fed, so no row should show a reading: {rows:?}"
    );
}

/// A pane too short to have any room left for data (its own interior height, after the rule
/// row, saturates to zero) still draws without panicking.
#[test]
fn a_pane_with_no_room_for_a_data_row_does_not_panic() {
    let app = mockup_app();
    let _ = frame(&app, 80, 3);
}

/// A raw value wider than the Value column is cut the same way a long label is: the
/// columns after it stay under the titles the header put them under.
#[test]
fn a_raw_value_longer_than_its_column_is_cut_with_an_ellipsis() {
    let mut app = app();
    let base = Instant::now();
    feed(
        &mut app,
        vo("99", json!(123_456_789_012_345_i64), "u64"),
        base,
    );
    app.now = base;
    app.screen = Screen::Telemetry;

    let rows = text(&frame(&app, 80, 24));
    let row = rows
        .iter()
        .find(|r| r.contains(" 99 "))
        .expect("the wide-value row is drawn");
    assert!(
        row.contains("12345678901…"),
        "the raw value is not cut to the column with an ellipsis: {row}"
    );
    assert!(
        row.ends_with(" 0.0s     -   │"),
        "the raw value shifted the Label, Age or Rate columns: {row}"
    );
}

#[test]
fn a_label_longer_than_its_column_is_cut_with_an_ellipsis_and_the_fixed_columns_stay_put() {
    let long_label = "This label is deliberately much too long to fit in the Label column";
    let mut profile = profile();
    profile.controls.push(Control {
        id: "99".parse().expect("a valid function id"),
        key: "long_label_test".into(),
        label: long_label.into(),
        kind: ControlKind::Readout,
        value_type: ValueType::U8,
        values: vec![],
        range: None,
        center: None,
        unit: None,
        convert: None,
        channel: None,
        floor: None,
        tick: None,
        log_scale: None,
        advice: None,
        only_while: None,
        auto_off: false,
        output_gain: false,
        mutes_output: false,
        powers_output: false,
        asks_on_change: false,
        reads: Vec::new(),
    });
    let tui = TuiConfig {
        graphs: false,
        ..TuiConfig::default()
    };
    let mut app = App::new(profile, "gungnir".into(), &tui, Vec::new());
    let base = Instant::now();
    feed(&mut app, vo("99", json!(0), "u8"), base);
    app.now = base;
    app.screen = Screen::Telemetry;

    // The same column width the screen itself computes for an 80-column terminal (78
    // columns inside the border), so the expected cut tracks the real implementation
    // rather than a number copied by hand.
    let width = super::label_width(78);
    let mut cut: String = long_label.chars().take(width - 1).collect();
    cut.push('…');

    let rows = text(&frame(&app, 80, 24));
    let row = rows
        .iter()
        .find(|r| r.contains("99"))
        .expect("the long-label row is drawn");
    assert!(
        row.contains(&cut),
        "the long label is not cut to the column with an ellipsis: {row}"
    );
    assert!(
        !row.contains(long_label),
        "the label overflowed its column instead of being cut: {row}"
    );
    assert_eq!(
        row.chars().count(),
        80,
        "the row is not the full terminal width"
    );
    assert!(
        row.ends_with(" 0.0s     -   │"),
        "the label cut into the Age or Rate columns: {row}"
    );
}

/// Every palette preset at every colour depth draws this screen's own conventions: the
/// accent rule under the header in the accent the theme resolved, the list's bottom border
/// on the terminal's last row, and a whole frame drawn without a panic.
#[test]
fn every_preset_draws_the_conventions_at_every_depth() {
    for palette in ["amber", "ice", "red", "neon"] {
        for depth in [
            ColorDepth::TrueColor,
            ColorDepth::Ansi256,
            ColorDepth::Ansi16,
        ] {
            let theme = theme_for(palette, depth);
            let drawn = format!("{palette} at {depth:?}");
            let buffer = frame_with_theme(&app(), 80, 24, &theme);

            // The strip's dashes are the rule, and its tabs and marks sit between them.
            assert_eq!(
                buffer.cell((0, 1)).expect("a cell").symbol(),
                "─",
                "{drawn}"
            );
            for x in 0..buffer.area.width {
                let cell = buffer.cell((x, 1)).expect("a cell on the rule row");
                if cell.symbol() == "─" {
                    assert_eq!(cell.fg, theme.accent(), "{drawn}: column {x} of the rule");
                }
            }

            assert_eq!(
                buffer
                    .cell((0, buffer.area.height - 1))
                    .expect("the first cell of the last row")
                    .symbol(),
                "└",
                "{drawn}: the list does not end on the last row"
            );
        }
    }
}

/// A zero-width column asks `cell` to fit text into no room at all: it gives back nothing,
/// never the one-column "…", which would be a column wider than asked for.
#[test]
fn a_zero_width_cell_is_empty() {
    assert_eq!(super::cell("anything", 0), "");
    assert_eq!(super::cell("", 0), "");
}

/// The mockup's six readings with a minute of history, the way a device that has been on for a
/// while reports them: a meter-like stream that swells and falls, a sample rate that stepped
/// once, a band's frequency that was moved, constants, and a fault that tripped and cleared.
fn usage_app() -> App {
    let mut app = app();
    app.clock = "14:02".into();
    app.tui.graphs = true;
    app.tui.graph_span = svolder_ipc::GraphSpan::M1;
    let base = Instant::now();
    for n in 0..60u64 {
        let at = base + Duration::from_secs(n);
        let swell = 400.0 + 350.0 * (n as f64 / 9.0).sin().abs() + 40.0 * ((n * 7 % 11) as f64);
        feed(
            &mut app,
            vo("6", json!(if n < 24 { 1 } else { 2 }), "u8"),
            at,
        );
        feed(
            &mut app,
            vo("7.3", json!(if n < 40 { 0 } else { 3 }), "u16"),
            at,
        );
        feed(&mut app, vo("9", json!(swell as u64), "u16"), at);
        feed(&mut app, vo("12.1.2", json!(64), "u8"), at);
        feed(&mut app, vo("210.2.1", json!(3), "u8"), at);
        feed(
            &mut app,
            vo("240.1", json!((30..36).contains(&n)), "bool"),
            at,
        );
    }
    app.now = base + Duration::from_secs(60);
    app.screen = Screen::Telemetry;
    app
}

/// The mockup docs/usage/tui.md shows of the telemetry screen is the frame this build draws.
#[test]
fn the_docs_show_the_telemetry_screen_this_build_draws() {
    crate::tui::test_support::assert_usage_md_shows(
        &text(&frame(&usage_app(), 80, 24)),
        "telemetry screen",
    );
}

/// An app on the telemetry screen with forty readings, the cursor on the `selected`th.
fn long_app(selected: usize) -> App {
    let mut app = app();
    let base = Instant::now();
    for n in 0..40 {
        feed(&mut app, vo(&format!("{}", 100 + n), json!(n), "u8"), base);
    }
    app.now = base;
    app.screen = Screen::Telemetry;
    app.selected = selected;
    app
}

/// The lines of the list at 80×24 with the index each shows, for a window of `shown` rows
/// whose first is `top`.
fn list_lines(top: usize, shown: usize, selected: usize) -> Vec<(u16, usize)> {
    (0..shown)
        .map(|at| (4 + at as u16, (top + at).abs_diff(selected)))
        .collect()
}

/// The rows fade by their distance from the selected one while the fade runs at true colour, a
/// window scrolled off the top counting from the selected index; the header and the borders keep
/// their colours.
#[test]
fn the_readings_fade_with_their_distance_from_the_selection() {
    let theme = theme_for("amber", ColorDepth::TrueColor);
    for selected in [0, 30] {
        let mut app = long_app(selected);
        let plain = frame_with_theme(&app, 80, 24, &theme);
        app.list_fade = true;
        let faded = frame_with_theme(&app, 80, 24, &theme);
        let top = widgets::scroll::scroll_start(40, 19, selected);
        assert_eq!(top > 0, selected == 30);
        crate::tui::test_support::assert_list_faded(
            &plain,
            &faded,
            &list_lines(top, 19, selected),
            1..79,
            &theme,
        );
    }
}

/// Nothing fades with the switch off, below true colour, or in monochrome.
#[test]
fn the_readings_stay_as_drawn_without_the_fade() {
    let mut app = long_app(0);
    for theme in [
        theme_for("amber", ColorDepth::TrueColor),
        theme_for("amber", ColorDepth::Ansi256),
        theme_for("amber", ColorDepth::Ansi16),
        Theme::from_config(&TuiConfig::default(), true, ColorDepth::TrueColor),
    ] {
        app.list_fade = false;
        let off = frame_with_theme(&app, 80, 24, &theme);
        app.list_fade = true;
        let on = frame_with_theme(&app, 80, 24, &theme);
        if theme.depth() == ColorDepth::TrueColor && !theme.monochrome() {
            assert_ne!(off, on);
        } else {
            assert_eq!(off, on);
        }
        app.list_fade = false;
        assert_eq!(off, frame_with_theme(&app, 80, 24, &theme));
    }
}

/// An app on the telemetry screen with graphs on and a minute of history: a meter-like stream
/// that ramps with one spike, a toggling bool, a stepping select, and a string.
fn graph_app() -> App {
    let mut app = app();
    app.clock = "14:02".into();
    app.tui.graphs = true;
    app.tui.graph_span = svolder_ipc::GraphSpan::M1;
    let base = Instant::now();
    for n in 0..60u64 {
        let at = base + Duration::from_secs(n);
        let ramp = if n == 20 { 1_000 } else { 100 + 10 * n };
        feed(&mut app, vo("9", json!(ramp), "u16"), at);
        feed(&mut app, vo("240.1", json!(n % 20 < 10), "bool"), at);
        feed(&mut app, vo("6", json!(n / 20), "u8"), at);
    }
    feed(&mut app, vo("101", json!("hello"), "str"), base);
    app.now = base + Duration::from_secs(60);
    app.screen = Screen::Telemetry;
    app.selected = 0;
    app
}

/// With graphs on, every numeric reading draws its history before its age: a stepped select, a
/// ramp with its autoscale, a bool as a dot line; a string has no graph. The selected row's
/// graph is the one in the first row.
#[test]
fn the_telemetry_screen_draws_a_graph_column_at_80_by_24() {
    let expected = [
        " svolder  Gungnir 2  ● connected                                  mouse:on 14:02",
        "─ Controls ── EQ ──┤ Telemetry ├── Devices ───────────────── ? help ─ esc back ─",
        "┌ Telemetry ──────────────────────────────────────────── refresh ─ 4 functions ┐",
        "│   ID       Type  Value       Label           History             Age  Rate   │",
        "│ ▶ 6        u8    2           Sample rate (4… ⣀⣀⣀⣀⣀⣀⣶⣶⣶⣶⣶⣶⣿⣿⣿⣿⣿⣿   1s   1/s   │",
        "│   9        u16   690         ?               ⣀⣀⣀⣤⣤⣤⣤⣤⣤⣶⣶⣶⣶⣶⣶⣿⣿⣿   1s   1/s   │",
        "│   101      str   hello       ?                                    1m     -   │",
        "│   240.1    bool  false       ?               ⠉⠉⠉⣀⣀⣀⠉⠉⠉⣀⣀⣀⠉⠉⠉⣀⣀⣀   1s   1/s   │",
        "│                                                                              │",
        "│                                                                              │",
        "│                                                                              │",
        "│                                                                              │",
        "│                                                                              │",
        "│                                                                              │",
        "│                                                                              │",
        "│                                                                              │",
        "│                                                                              │",
        "│                                                                              │",
        "│                                                                              │",
        "│                                                                              │",
        "│                                                                              │",
        "│                                                                              │",
        "│                                                                              │",
        "└──────────────────────────────────────────────────────────────────────────────┘",
    ];
    assert_rows(&frame(&graph_app(), 80, 24), &expected);
}

/// The same readings at 120 columns: the graph column takes its widest and the label the rest.
#[test]
fn the_telemetry_screen_draws_a_wider_graph_column_at_120_by_24() {
    let expected = [
        " █▀▀▀ █  █ █▀▀█ █ ▄▄▄█ ████ █▀▀                                                                                         ",
        " ▄▄▄█  ██  █▄▄█ █ █▄▄█ █▄▄▄ █    Gungnir 2  ● connected                                                   mouse:on 14:02",
        "─ Controls ── EQ ──┤ Telemetry ├── Devices ───────────────────────────────────────────────────────── ? help ─ esc back ─",
        "┌ Telemetry ──────────────────────────────────────────────────────────────────────────────────── refresh ─ 4 functions ┐",
        "│    ID       Type  Value       Label                           History                                   Age  Rate    │",
        "│  ▶ 6        u8    2           Sample rate (48 kHz)            ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶⣾⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿⣿   1s   1/s    │",
        "│    9        u16   690         ?                               ⣀⣀⣀⣀⣀⣀⣀⣀⣀⣀⣤⣤⣤⣿⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣤⣶⣶⣶⣶⣶⣶⣶⣶⣶⣶   1s   1/s    │",
        "│    101      str   hello       ?                                                                          1m     -    │",
        "│    240.1    bool  false       ?                               ⠉⠉⠉⠉⠉⠉⢁⣀⣀⣀⣀⣀⣀⠉⠉⠉⠉⠉⠉⠉⣀⣀⣀⣀⣀⣀⡈⠉⠉⠉⠉⠉⠉⣀⣀⣀⣀⣀⣀⣀   1s   1/s    │",
        "│                                                                                                                      │",
        "│                                                                                                                      │",
        "│                                                                                                                      │",
        "│                                                                                                                      │",
        "│                                                                                                                      │",
        "│                                                                                                                      │",
        "│                                                                                                                      │",
        "│                                                                                                                      │",
        "│                                                                                                                      │",
        "│                                                                                                                      │",
        "│                                                                                                                      │",
        "│                                                                                                                      │",
        "│                                                                                                                      │",
        "│                                                                                                                      │",
        "└──────────────────────────────────────────────────────────────────────────────────────────────────────────────────────┘",
    ];
    assert_rows(&frame(&graph_app(), 120, 24), &expected);
}

/// The selected row's reversed band runs unbroken across the graph, and the graph's dots on
/// the rows around it keep a colour of their own.
#[test]
fn the_selected_rows_highlight_runs_across_the_graph() {
    let buffer = frame_with_theme(
        &graph_app(),
        80,
        24,
        &theme_for("amber", ColorDepth::TrueColor),
    );
    for x in 2..78 {
        let cell = buffer.cell((x, 4)).expect("a cell");
        assert!(
            cell.modifier.contains(Modifier::REVERSED),
            "column {x} of the selected row is not highlighted: {:?}",
            cell.symbol()
        );
    }
    let dot = buffer.cell((51, 5)).expect("a cell");
    assert_eq!(dot.symbol(), "⣤");
    assert!(dot.fg != Color::Reset, "an unselected graph has a colour");
    assert!(!dot.modifier.contains(Modifier::REVERSED));
}

/// With graphs off no row has a graph or a History title: the label has the room it had.
#[test]
fn with_graphs_off_no_row_draws_a_graph() {
    let mut app = graph_app();
    app.tui.graphs = false;
    for row in text(&frame(&app, 120, 24)) {
        assert!(!row.contains("History"), "{row}");
        assert!(
            row.chars().all(|c| !('\u{2800}'..='\u{28ff}').contains(&c)),
            "{row}"
        );
    }
}

/// A pane too narrow to leave the label 16 cells and the graph 16 draws no graph; and the
/// graph takes at most 40 cells wherever there is room.
#[test]
fn the_graph_column_needs_sixteen_cells_after_a_sixteen_cell_label() {
    // The fixed columns take 44 of the pane's width.
    assert_eq!(super::columns_of(75, true), (31, 0));
    assert_eq!(super::columns_of(76, true), (16, 16));
    assert_eq!(super::columns_of(78, true), (16, 18));
    assert_eq!(super::columns_of(200, true), (116, 40));
    assert_eq!(super::columns_of(78, false), (34, 0));
}

/// A meter's peak survives the graph's coarsest span: the column that holds it is the top of
/// its scale, though more samples follow it within the column.
#[test]
fn a_meter_peak_shows_in_the_graph_at_the_longest_span() {
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get("vidar2")
        .expect("vidar2 is built in")
        .clone();
    let mut app = App::new(profile, "vidar".into(), &TuiConfig::default(), Vec::new());
    app.tui.graph_span = svolder_ipc::GraphSpan::M60;
    let base = Instant::now();
    for n in 0..3_600u64 {
        // The Vidar 2's full scale is 1000: the peak reads at the top of its scale.
        let raw = if n == 1_000 { 1_000 } else { 100 };
        feed(
            &mut app,
            vo("20.1", json!(raw), "u16"),
            base + Duration::from_secs(n),
        );
    }
    app.now = base + Duration::from_secs(3_600);
    app.screen = Screen::Telemetry;
    let rows = text(&frame(&app, 80, 24));
    let row = rows
        .iter()
        .find(|row| row.contains("u16"))
        .expect("the meter's row");
    // The quiet readings sit part way up the fixed scale (⣶), and the peak is one half-cell
    // column filled to the top beside them: ⣷ on the left of its cell, ⣾ on the right.
    assert_eq!(row.matches(['⣷', '⣾']).count(), 1, "{row}");
}

/// A log-scaled meter's History column is on its fixed scale: readings a hair above the zero
/// for the whole span draw in the bottom of the cell, where the window's own smallest and
/// largest value would stretch them to the full height (spec 6.4).
#[test]
fn a_log_scaled_meters_history_sits_at_the_bottom_of_its_scale() {
    let upper_dots = |scaled: bool| {
        let profile = Registry::builtin()
            .expect("the built-in profiles load")
            .get("vidar2")
            .expect("vidar2 is built in")
            .clone();
        let profile = if scaled {
            with_log_scale(profile, AEGIR_SCALE)
        } else {
            profile
        };
        let mut app = App::new(profile, "vidar".into(), &TuiConfig::default(), Vec::new());
        app.tui.graphs = true;
        app.tui.graph_span = svolder_ipc::GraphSpan::M1;
        let base = Instant::now();
        for n in 0..60u64 {
            feed(
                &mut app,
                vo("20.1", json!(161 + n / 30), "u16"),
                base + Duration::from_secs(n),
            );
        }
        app.now = base + Duration::from_secs(60);
        app.screen = Screen::Telemetry;
        let rows = text(&frame(&app, 80, 24));
        let row = rows
            .iter()
            .find(|row| row.contains("u16"))
            .expect("the meter's row")
            .clone();
        let dots: Vec<u32> = row
            .chars()
            .filter(|c| ('\u{2800}'..='\u{28ff}').contains(c))
            .map(|c| (u32::from(c) - 0x2800) & 0x1b)
            .collect();
        assert!(!dots.is_empty(), "{row}");
        dots
    };
    assert!(upper_dots(true).iter().all(|dots| *dots == 0));
    // The unscaled meter stretches the same readings to the graph's full height.
    assert!(upper_dots(false).iter().any(|dots| *dots != 0));
}

/// The graph row of reading `id` on a vidar2 (an amp: 230.1 is a fault, 240.1 not a control the
/// profile names) fed `values` a second apart, and the colours of its braille cells.
fn bool_dot_colours(id: &str, values: &[bool]) -> Vec<Color> {
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get("vidar2")
        .expect("vidar2 is built in")
        .clone();
    let mut app = App::new(profile, "vidar".into(), &TuiConfig::default(), Vec::new());
    app.tui.graph_span = svolder_ipc::GraphSpan::M1;
    let base = Instant::now();
    // The selected row draws in the selection's colours, so a lower id takes the cursor.
    feed(&mut app, vo("6", json!(0), "u8"), base);
    for (n, &on) in values.iter().enumerate() {
        feed(
            &mut app,
            vo(id, json!(on), "bool"),
            base + Duration::from_secs(n as u64),
        );
    }
    app.now = base + Duration::from_secs(values.len() as u64);
    app.screen = Screen::Telemetry;
    let theme = theme_for("amber", ColorDepth::TrueColor);
    let buffer = frame_with_theme(&app, 80, 24, &theme);
    let rows = text(&buffer);
    let y = rows
        .iter()
        .position(|row| row.contains("bool"))
        .expect("the bool's row");
    assert!(rows[y - 1].contains("u8"), "the selected row is above it");
    (0..80)
        .filter_map(|x| buffer.cell((x, y as u16)))
        .filter(|cell| {
            cell.symbol()
                .chars()
                .any(|c| ('\u{2801}'..='\u{28ff}').contains(&c))
        })
        .map(|cell| cell.fg)
        .collect()
}

/// An ordinary bool draws on and off in the OK colour, the dot's position telling them apart; a
/// fault draws its active dots in the warning colour and its inactive ones in the OK colour.
#[test]
fn a_bool_draws_ok_and_a_fault_draws_warn_only_while_active() {
    let theme = theme_for("amber", ColorDepth::TrueColor);
    let ok = theme.style(Role::MeterOk).fg.expect("a colour");
    let warn = theme.style(Role::Warn).fg.expect("a colour");
    assert_ne!(ok, warn);
    let flips: Vec<bool> = (0..60).map(|n| n % 20 < 10).collect();
    let toggle = bool_dot_colours("240.1", &flips);
    assert!(!toggle.is_empty());
    assert!(toggle.iter().all(|&fg| fg == ok), "{toggle:?}");
    let clear = bool_dot_colours("230.1", &[false; 60]);
    assert!(!clear.is_empty());
    assert!(clear.iter().all(|&fg| fg == ok), "{clear:?}");
    let tripped = bool_dot_colours("230.1", &flips);
    assert!(
        tripped.contains(&warn) && tripped.contains(&ok),
        "{tripped:?}"
    );
}

/// A fault that trips and clears inside one column still shows at the longest span.
#[test]
fn a_short_fault_shows_at_the_longest_span() {
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get("vidar2")
        .expect("vidar2 is built in")
        .clone();
    let mut app = App::new(profile, "vidar".into(), &TuiConfig::default(), Vec::new());
    app.tui.graph_span = svolder_ipc::GraphSpan::M60;
    let base = Instant::now();
    for n in 0..3_600u64 {
        let on = n == 1_000 || n == 1_001;
        feed(
            &mut app,
            vo("230.1", json!(on), "bool"),
            base + Duration::from_secs(n),
        );
    }
    app.now = base + Duration::from_secs(3_600);
    let series = app
        .readings
        .histories
        .reading("230.1".parse().expect("an id"))
        .expect("a series");
    let columns =
        crate::tui::widgets::graph::columns(series, app.now, Duration::from_secs(3_600), 80);
    assert_eq!(
        columns.iter().flatten().filter(|&&v| v >= 0.5).count(),
        1,
        "{columns:?}"
    );
}

/// A column's text is cut and padded in display cells, so a wide glyph in a label or a value
/// leaves the next column where it is.
#[test]
fn a_wide_glyph_keeps_the_next_column_in_place() {
    for text in ["ab", "世界", "世界世界世界", "a世"] {
        assert_eq!(widgets::text::cells(&super::cell(text, 7)), 7, "{text}");
        let fits = widgets::text::cells(text) <= 7;
        assert!(
            !fits || widgets::text::cells(&super::pad_right(text, 7)) == 7,
            "{text}"
        );
    }
    assert_eq!(super::cell("世界世界世界", 7), "世界世…");
    assert_eq!(super::pad_right("世界", 7), "   世界");
}
