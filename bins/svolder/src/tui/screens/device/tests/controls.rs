// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Controls pane: its rows, their columns, groups, selection and mouse hits.

use super::*;

#[test]
fn every_control_row_starts_its_value_in_the_same_column() {
    let buffer = screen(&mockup_app());
    // The Input, Phase invert, NOS filter, Mute, Volume and Balance rows, one line each.
    for (y, first) in [(3, "‹"), (4, "["), (5, "["), (6, "["), (8, "█"), (10, "L")] {
        let cell = buffer
            .cell((VALUE_COLUMN, y))
            .expect("a cell inside the Controls pane");
        assert_eq!(
            cell.symbol(),
            first,
            "row {y} starts its value somewhere else"
        );
        assert_eq!(
            buffer
                .cell((VALUE_COLUMN - 1, y))
                .expect("the label column")
                .symbol(),
            " ",
            "row {y} has no space between its label and its value"
        );
    }
}

#[test]
fn the_finer_hits_land_on_the_widgets_they_describe() {
    let app = mockup_app();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let theme = Theme::new(false);
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, &app, &theme, &mut hits);
        })
        .expect("the frame draws");
    let buffer = terminal.backend().buffer().clone();

    // The Input row's select arrows, at the same column every row's value starts in and
    // wherever `›` lands, its own name and raw value included.
    assert_eq!(
        hits.hit_at(VALUE_COLUMN, 3),
        Some(&Hit::SelectPrev(0)),
        "‹ does not flip to Hit::SelectPrev"
    );
    assert_eq!(
        hits.hit_at(first_cell(&buffer, 3, "›"), 3),
        Some(&Hit::SelectNext(0)),
        "› does not flip to Hit::SelectNext"
    );
    // The Phase invert row's toggle box, three rows a click can land on.
    for x in VALUE_COLUMN..VALUE_COLUMN + 3 {
        assert_eq!(
            hits.hit_at(x, 4),
            Some(&Hit::ToggleOf(1)),
            "column {x} of [ ] is not Hit::ToggleOf"
        );
    }
    // The Volume row's bar, at the same column: a level with no centre.
    assert_eq!(
        hits.hit_at(VALUE_COLUMN, 8),
        Some(&Hit::LevelBar(4)),
        "the volume bar is not Hit::LevelBar"
    );
    // The Balance row's track, past the "L " that opens it.
    assert_eq!(
        hits.hit_at(first_cell(&buffer, 10, BALANCE_MARK), 10),
        Some(&Hit::LevelBar(5)),
        "the balance track is not Hit::LevelBar"
    );
    // Every level row is one line tall: the line under the balance is no row at all.
    assert_eq!(hits.hit_at(4, 10), Some(&Hit::Row(Pane::Controls, 5)));
    assert_eq!(
        hits.hit_at(4, 11),
        None,
        "a level row still takes two lines"
    );
    // The Tone pane's Mode row: its own arrows still step the select, and the rest of the
    // row toggles it.
    let mode_prev = first_cell_from(&buffer, 3, CONTROLS_WIDTH, "‹");
    assert_eq!(hits.hit_at(mode_prev, 3), Some(&Hit::ModePrev));
    let mode_next = first_cell_from(&buffer, 3, CONTROLS_WIDTH, "›");
    assert_eq!(hits.hit_at(mode_next, 3), Some(&Hit::ModeNext));
    let mode_label = first_cell_from(&buffer, 3, CONTROLS_WIDTH, "M");
    assert_eq!(hits.hit_at(mode_label, 3), Some(&Hit::Mode));
    // The Tone pane's loudness bar sets the strength; the rest of its row toggles the mode.
    assert_eq!(
        hits.hit_at(first_cell(&buffer, 4, "░"), 4),
        Some(&Hit::LoudnessBar),
        "the loudness bar in the Tone pane is not Hit::LoudnessBar"
    );
    let loudness_label = first_cell_from(&buffer, 4, CONTROLS_WIDTH, "L");
    assert_eq!(hits.hit_at(loudness_label, 4), Some(&Hit::Loudness));
    // The Controls and Tone titles focus their pane; the Status title, on a pane that is
    // never focused, asks for nothing.
    assert_eq!(hits.hit_at(2, 2), Some(&Hit::PaneHeader(Pane::Controls)));
    assert_eq!(
        hits.hit_at(first_cell(&buffer, 2, "T"), 2),
        Some(&Hit::PaneHeader(Pane::Tone))
    );
    assert_eq!(hits.hit_at(2, 20), None);
}

/// The Controls pane sets its groups a blank line apart, the balance standing apart from the
/// volume, wherever it has the lines for every row and the blanks; a line short of that it packs
/// them, so no row goes out of sight.
#[test]
fn the_controls_pane_spaces_its_groups_where_it_has_room() {
    let app = mockup_app();
    let theme = Theme::new(false);
    let blanks = |height: usize| -> Vec<bool> {
        let tier = crate::tui::layout_tier::Tier::default();
        super::super::controls::controls_pane(&app, &theme, 32, tier, height)
            .lines
            .iter()
            .map(|line| line.to_string().trim().is_empty())
            .collect()
    };
    // Input, Phase invert, NOS filter, Mute | Volume | Balance.
    let spaced = [false, false, false, false, true, false, true, false];
    assert_eq!(
        blanks(8),
        spaced,
        "eight lines hold the rows and their blanks"
    );
    assert_eq!(blanks(16), spaced);
    assert_eq!(blanks(7), [false; 6], "seven lines pack the six rows");
}

/// The column of the first cell at or past column `from` on row `y` whose symbol is
/// `symbol`: the Tone pane's own, past the Controls pane's, where a select of its own —
/// the Input row's — draws the same glyph first.
fn first_cell_from(buffer: &Buffer, y: u16, from: u16, symbol: &str) -> u16 {
    (from..buffer.area.width)
        .find(|&x| {
            buffer
                .cell((x, y))
                .is_some_and(|cell| cell.symbol() == symbol)
        })
        .unwrap_or_else(|| panic!("no {symbol:?} cell on row {y} from column {from}"))
}

/// The Mode row's `‹ ›` arrows are drawn in the accent whether or not the row is focused —
/// unlike the Controls pane's own selects, whose arrows take whatever role the row around
/// them does, which is the plain text role off the focused row this checks against.
#[test]
fn the_mode_rows_arrows_are_in_the_accent_focused_or_not() {
    let theme = Theme::new(false);
    let accent = theme.accent();
    let mut app = mockup_app();
    // Off the Input row and off the Tone pane's own focus, so neither row's own selected
    // style — the accent among it too — is what colours these arrows.
    app.selected = 1;
    app.pane = Pane::Status;
    let buffer = frame_with_theme(&app, 80, 24, &theme);
    let y = u16::try_from(
        text(&buffer)
            .iter()
            .position(|row| row.contains("Mode"))
            .expect("the Mode row"),
    )
    .unwrap_or(0);

    let input_arrow = first_cell(&buffer, y, "‹");
    assert_ne!(
        buffer.cell((input_arrow, y)).expect("a cell").fg,
        accent,
        "the Input row's own arrow is in the accent too"
    );
    for symbol in ["‹", "›"] {
        let x = first_cell_from(&buffer, y, CONTROLS_WIDTH, symbol);
        assert_eq!(
            buffer.cell((x, y)).expect("a cell").fg,
            accent,
            "the Mode row's {symbol} is not in the accent"
        );
    }
}

#[test]
fn the_selected_rows_cursor_carries_the_accent_colour() {
    let config = TuiConfig {
        accent: Some("magenta".into()),
        ..TuiConfig::default()
    };
    let theme = Theme::from_config(&config, false, ColorDepth::Ansi16);
    let buffer = frame_with_theme(&mockup_app(), 80, 24, &theme);
    let (x, y) = cursor_cell(&buffer);
    let cell = buffer.cell((x, y)).expect("the cursor cell");
    assert_eq!(
        cell.fg,
        Color::Magenta,
        "the ▶ cursor carries the accent colour"
    );
}

/// In monochrome the selected row's reverse video is the only mark of the selection, and it
/// runs to the Controls pane's inner right edge, not just past the row's own text.
#[test]
fn the_selected_row_is_reversed_to_the_panes_inner_right_edge() {
    let theme = Theme::new(true);
    let mut app = mockup_app();
    app.selected = 0; // Input, a short row
    let buffer = frame_with_theme(&app, 80, 24, &theme);
    let cells = row_cells(&buffer, "Input");
    let border = cells
        .iter()
        .position(|cell| cell.symbol() == "│")
        .expect("the pane's left border");
    let right = cells[border + 1..]
        .iter()
        .position(|cell| cell.symbol() == "│")
        .map(|at| at + border + 1)
        .expect("the pane's right border");
    for (x, cell) in cells.iter().enumerate().take(right).skip(border + 1) {
        assert!(
            cell.modifier.contains(Modifier::REVERSED),
            "column {x} of the selected row is not reversed"
        );
    }
    assert!(
        !cells[right].modifier.contains(Modifier::REVERSED),
        "the border"
    );
}

/// The Controls pane lists its rows in three groups — the selects and toggles, the levels,
/// the meters — whatever order the profile's addresses put them in: the Lyr 5's `Mode`
/// select, at an address past its volume, sits with the other selects and toggles, and the
/// tall tier sets exactly one blank row between the two groups this amp has.
#[test]
fn the_controls_rows_come_in_three_groups_whatever_their_addresses() {
    let mut app = app_on(profile_named("lyr5"));
    hear(
        &mut app,
        vec![
            vo("2", Some("input"), json!(1), "1"),
            vo("3", Some("gain"), json!(0), "0"),
            vo("4.1", Some("headphone_mute"), json!(false), "off"),
            vo("4.2", Some("preout"), json!(false), "off"),
            converted("6", "volume", 100, "100", -10.0, "dB"),
            vo("7", Some("mode"), json!(0), "0"),
        ],
    );
    let keys: Vec<&str> = app
        .rows()
        .iter()
        .map(|row| row.control.key.as_str())
        .collect();
    let (mode, volume) = (
        keys.iter()
            .position(|key| *key == "mode")
            .expect("a mode row"),
        keys.iter()
            .position(|key| *key == "volume")
            .expect("a volume row"),
    );
    assert!(
        mode < volume,
        "the Mode select is not with the selects: {keys:?}"
    );
    let rows = text(&frame(&app, 80, 44));
    let pane: Vec<&str> = rows[4..12]
        .iter()
        .map(|row| row.trim_matches(['│', ' ']))
        .collect();
    let blanks: Vec<usize> = pane
        .iter()
        .enumerate()
        .filter(|(_, row)| row.is_empty())
        .map(|(at, _)| at)
        .collect();
    let at_mode = pane
        .iter()
        .position(|row| row.contains("Mode"))
        .expect("Mode");
    let at_volume = pane
        .iter()
        .position(|row| row.contains("Volume"))
        .expect("Volume");
    assert!(at_mode < at_volume, "{pane:#?}");
    assert_eq!(
        blanks
            .iter()
            .filter(|at| **at > 0 && **at < at_volume)
            .count(),
        1,
        "not one blank row between the groups: {pane:#?}"
    );
}

/// A Controls row ends at least one blank cell short of the pane's right border at every
/// size below the tiers: the volume and the balance beside a Tone pane, and an amp's
/// meters across the whole width.
#[test]
fn a_controls_row_never_touches_the_panes_right_border() {
    let mut app = mockup_app();
    hear(
        &mut app,
        vec![converted("10", "volume", 63, "63 (-26.1 dB)", -26.1, "dB")],
    );
    app.settle();
    let meters = app_with_meter_reading(9_000);
    for (app, label, border) in [
        (&app, "Volume", 37),
        (&app, "Balance", 37),
        (&meters, "Meter L", 79),
    ] {
        let chars = row_chars(&screen(app), label);
        assert_eq!(chars[border], '│', "{label}: {chars:?}");
        assert_eq!(
            chars[border - 1],
            ' ',
            "{label} touches the border: {}",
            chars.iter().collect::<String>()
        );
    }
}

/// A focused Controls row's selected style runs from its cursor across every cell the pane has
/// for rows, the blank [`CONTROLS_TRAIL`] cell included, at both the smallest terminal and the
/// wide and tall tier: the cell after the last character carries it on a short row (Input) and
/// on one that fills its budget (Volume), and the cell before the border does in the narrow
/// tier, where no padding sits between them.
#[test]
fn a_focused_controls_row_is_selected_across_the_pane() {
    let theme = Theme::new(false);
    let selected = theme.style(Role::Selected);
    for (width, height) in [(80, 24), (126, 62)] {
        let mut app = mockup_app();
        hear(
            &mut app,
            vec![converted("10", "volume", 63, "63 (-26.1 dB)", -26.1, "dB")],
        );
        app.settle();
        for (selection, label) in [(0, "Input"), (4, "Volume")] {
            app.selected = selection;
            let buffer = frame(&app, width, height);
            let cells = controls_row_cells(&buffer, label);
            let last = last_character(&cells);
            let after = &cells[last + 1];
            assert_eq!(after.symbol(), " ", "{width}x{height}: {label}");
            assert_eq!(
                Some(after.fg),
                selected.fg,
                "{width}x{height}: the cell after {label}'s value is not selected"
            );
            assert!(
                after.modifier.contains(selected.add_modifier),
                "{width}x{height}: the cell after {label}'s value is not selected"
            );
            if width == 80 {
                let edge = cells.last().expect("a cell before the border");
                assert!(
                    edge.modifier.contains(selected.add_modifier),
                    "{label}'s band stops short of the border"
                );
            }
        }
    }
}

/// The cells of the Controls row naming `label`, left to right, up to but not including the
/// pane's own right border — so a search for the row's last character stops there rather
/// than running on into the Tone pane sharing the same row.
fn controls_row_cells(buffer: &Buffer, label: &str) -> Vec<ratatui::buffer::Cell> {
    let row = row_cells(buffer, label);
    let border = row
        .iter()
        .skip(1)
        .position(|cell| cell.symbol() == "│")
        .map(|at| at + 1)
        .expect("the pane's own right border");
    row[..border].to_vec()
}

/// Where `cells`' last non-blank glyph is.
fn last_character(cells: &[ratatui::buffer::Cell]) -> usize {
    cells
        .iter()
        .rposition(|cell| !cell.symbol().trim().is_empty())
        .expect("the row has a last character")
}

/// [`row_style`] extends a focused row by one cell short of its own budget and never past
/// it: a row already at `max_width` gets nothing extra.
#[test]
fn row_style_extends_a_focused_row_up_to_its_budget_and_no_further() {
    let theme = Theme::new(false);
    let line = Line::raw("0123456789");
    let width = |line: &Line<'static>| line.spans.iter().map(Span::width).sum::<usize>();

    let short = row_style(line.clone(), true, false, &theme, 11);
    assert_eq!(width(&short), 11, "a row short of its budget did not grow");

    let full = row_style(line, true, false, &theme, 10);
    assert_eq!(width(&full), 10, "a row already at its budget grew");
}

/// On a row the link went down under, the raw digits written over the bar's empty cells
/// take the stale role the rest of the row is drawn in, at 256 and true colour, where that
/// role is a colour of its own.
#[test]
fn a_stale_rows_raw_digits_are_dimmed_with_it() {
    for depth in [ColorDepth::Ansi256, ColorDepth::TrueColor] {
        let theme = Theme::from_config(&TuiConfig::default(), false, depth);
        let mut app = mockup_app();
        hear(
            &mut app,
            vec![converted("10", "volume", 63, "63 (-26.1 dB)", -26.1, "dB")],
        );
        app.settle();
        app.apply(Update::Connection { connected: false }, app.now);
        let cells = row_cells(&frame_with_theme(&app, 80, 24, &theme), "Volume");
        let seven = cells
            .iter()
            .rev()
            .find(|cell| cell.symbol() == "7")
            .expect("the 7 of 127");
        assert_eq!(
            Some(seven.fg),
            theme.style(Role::Stale).fg,
            "{depth:?}: the digit over the empty cells is not stale"
        );
    }
}

/// While the backend holds a note that the configured passphrase does not match the device's
/// advertisement, the Status pane grows one line for it, its last, in the warning role and with
/// no digit; with no note the pane is as it was.
#[test]
fn the_passphrase_row_says_what_the_note_says() {
    use svolder_ipc::PassphraseNote;
    let theme = Theme::new(false);
    let plain = mockup_app();
    let mut noted = mockup_app();
    noted.session.passphrase_note = Some(PassphraseNote::Differs);
    let rows = text(&frame_with_theme(&noted, 80, 24, &theme));
    let bare = text(&frame_with_theme(&plain, 80, 24, &theme));
    assert!(
        bare.iter().all(|row| !row.contains("Passphrase")),
        "{bare:#?}"
    );
    assert_eq!(
        rows[22],
        "│ Passphrase: does not match the code the device advertises                    │"
    );
    assert!(
        rows[19].starts_with("┌ Status "),
        "the pane grew a line upward: {rows:#?}"
    );
    let warn = theme.style(Role::Warn);
    let buffer = frame_with_theme(&noted, 80, 24, &theme);
    let cell = buffer.cell((15, 22)).expect("the first word");
    assert!(
        warn.fg.is_none_or(|fg| cell.fg == fg) && cell.modifier.contains(warn.add_modifier),
        "the note is not in the warning role: {cell:?}"
    );
    for note in [
        PassphraseNote::NoneConfigured,
        PassphraseNote::NoCodeAdvertised,
    ] {
        noted.session.passphrase_note = Some(note);
        let row = text(&frame_with_theme(&noted, 80, 24, &theme))[22].clone();
        assert!(row.contains(note.short()), "{row}");
        assert!(!row.chars().any(|c| c.is_ascii_digit()), "{row}");
    }
}
