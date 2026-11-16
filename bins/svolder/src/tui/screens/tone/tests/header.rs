// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone screen's header: the mode and loudness halves, the preset name and the fit.

use super::*;

/// The mode and the loudness are rows the arrows reach here as well as on the device
/// screen, so the header says which of them they are on.
#[test]
fn the_header_marks_the_row_the_arrows_are_on() {
    let theme = Theme::new(false);
    let mut app = flat_app();
    let resting = screen(&app);
    let (mode, loudness) = (column_of(&resting, "Mode"), column_of(&resting, "Loudness"));
    assert_ne!(
        resting.cell((mode, 0)).expect("the mode cell").fg,
        theme.accent(),
        "the mode stands out before the cursor is on it"
    );

    app.key(KeyEvent::from(KeyCode::Up));
    let on_loudness = screen(&app);
    assert_eq!(
        on_loudness
            .cell((loudness, 0))
            .expect("the loudness cell")
            .fg,
        theme.accent(),
        "the loudness row does not stand out while the arrows are on it"
    );

    app.key(KeyEvent::from(KeyCode::Up));
    let on_mode = screen(&app);
    assert_eq!(
        on_mode.cell((mode, 0)).expect("the mode cell").fg,
        theme.accent(),
        "the mode row does not stand out while the arrows are on it"
    );
    assert_ne!(
        on_mode.cell((loudness, 0)).expect("the loudness cell").fg,
        theme.accent(),
        "two rows stand out at once"
    );
}

/// The Mode select's `‹ ›` arrows are in the accent even at rest, before the cursor is on
/// the row at all — unlike the row's own selected style, which only the cursor earns.
#[test]
fn the_mode_selects_arrows_are_in_the_accent_at_rest() {
    let theme = Theme::new(false);
    let app = flat_app();
    let buffer = screen(&app);
    let mode = column_of(&buffer, "Mode");
    for symbol in ["‹", "›"] {
        let x = (mode..buffer.area.width)
            .find(|&x| {
                buffer
                    .cell((x, 0))
                    .is_some_and(|cell| cell.symbol() == symbol)
            })
            .unwrap_or_else(|| panic!("no {symbol:?} past the Mode label"));
        assert_eq!(
            buffer.cell((x, 0)).expect("a header cell").fg,
            theme.accent(),
            "the Mode select's {symbol} is not in the accent at rest"
        );
    }
}

/// The loudness bar in the header keeps its own colours while the arrows are on the
/// loudness row: its cells are never reversed, though the word beside it is.
#[test]
fn the_headers_loudness_bar_keeps_its_colours_while_it_stands_out() {
    let mut app = flat_app();
    hear(&mut app, vec![vo("12.52", 64)]);
    app.key(KeyEvent::from(KeyCode::Up));
    let buffer = screen(&app);
    let loudness = column_of(&buffer, "Loudness");
    assert!(
        buffer
            .cell((loudness, 0))
            .expect("the loudness word")
            .modifier
            .contains(Modifier::REVERSED),
        "the loudness row does not stand out"
    );
    let bar: Vec<u16> = (loudness..buffer.area.width)
        .filter(|x| {
            matches!(
                buffer.cell((*x, 0)).expect("a header cell").symbol(),
                "█" | "░"
            )
        })
        .collect();
    assert_eq!(bar.len(), 5, "the loudness bar");
    for x in bar {
        assert!(
            !buffer
                .cell((x, 0))
                .expect("a bar cell")
                .modifier
                .contains(Modifier::REVERSED),
            "the header reversed the loudness bar at column {x}"
        );
    }
}

/// The mode and the loudness the header draws are rows of this screen as the bands are:
/// what a change to either glows and shimmers over, `f`'s own change to the mode among them.
#[test]
fn the_header_s_mode_and_loudness_are_rows_of_the_frame() {
    let app = flat_app();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut areas = render::FrameAreas::default();
    terminal
        .draw(|f| areas = render::draw(f, &app, &Theme::new(false), &mut hits))
        .expect("the frame draws");
    let buffer = terminal.backend().buffer().clone();
    let eq = app
        .profile
        .eq
        .as_ref()
        .expect("the tone app has a tone control");
    let text_of = |id| {
        let (_, rect) = areas
            .rows
            .iter()
            .find(|(row, _)| *row == id)
            .unwrap_or_else(|| panic!("{id} is no row of the frame"));
        (rect.left()..rect.right())
            .filter_map(|x| {
                buffer
                    .cell((x, rect.y))
                    .map(|cell| cell.symbol().to_string())
            })
            .collect::<String>()
    };
    assert_eq!(text_of(eq.mode_id()), "Mode ‹ EQ ›");
    assert!(
        text_of(eq.loudness_id()).starts_with("Loudness "),
        "{}",
        text_of(eq.loudness_id())
    );
}

/// The header names the preset on the device in the room the rest of the line leaves it: a
/// name too long for an 80-column header is cut short with `…`, the mark of a changed curve
/// kept after it, and a wider header draws it whole. A load still being confirmed says so.
#[test]
fn the_header_names_the_preset_cutting_a_long_name_to_fit() {
    let mut app = flat_app();
    app.presets.saved = vec![crate::tui::test_support::presets::own(
        "Warm (mellow)",
        5,
        1.0,
    )];
    app.presets.loaded_preset = Some("Warm (mellow)".into());
    // Band 2 at +6 dB is a curve no preset describes, moved away from the one loaded.
    hear(&mut app, vec![vo("12.2.3", 127)]);
    assert_eq!(
        text(&screen(&app))[0],
        " svolder  Gungnir 2  Preset Warm (me…*  Mode ‹ EQ ›   Loudness ░░░░░ 0  mouse:on"
    );
    let wide = text(&frame_with_theme(&app, 126, 30, &Theme::new(false)));
    let line = wide
        .iter()
        .find(|row| row.contains("Mode"))
        .expect("the header's text row");
    assert!(
        line.contains("Gungnir 2  Preset Warm (mellow)*  "),
        "the wide header cut the name: |{line}|"
    );

    // The mode beside it takes its own room: Flat's name is wider than EQ's, and Loudness's
    // wider still.
    for (mode, cut) in [(1, "  Preset Warm (…*  "), (3, "  Preset Wa…*  ")] {
        hear(&mut app, vec![vo("12.50", mode)]);
        let header = text(&screen(&app))[0].clone();
        assert!(header.contains(cut), "mode {mode}: |{header}|");
        assert!(header.ends_with("mouse:on"), "mode {mode}: |{header}|");
    }
    hear(&mut app, vec![vo("12.50", 2)]);

    app.readings.loading = Some(crate::tui::boxes::presets::Loading {
        name: "Bass".into(),
        last: "12.50".parse().expect("the mode's function"),
        since: Instant::now(),
    });
    assert!(
        text(&screen(&app))[0].starts_with(" svolder  Gungnir 2  loading Bass  "),
        "|{}|",
        text(&screen(&app))[0]
    );
}

/// The column right after the last time `symbols` are drawn in that order on row `y` —
/// the rightmost match, for a value that can repeat earlier on the same row — for a check
/// that needs the exact cell rather than a byte offset into the row's own text (which
/// multi-byte glyphs such as `‹` and `›` would throw off).
fn column_after(buffer: &Buffer, y: u16, symbols: &[&str]) -> u16 {
    let width = usize::from(buffer.area.width);
    let last = width.saturating_sub(symbols.len());
    'start: for start in (0..=last).rev() {
        for (offset, want) in symbols.iter().enumerate() {
            let x = u16::try_from(start + offset).unwrap_or(u16::MAX);
            let Some(cell) = buffer.cell((x, y)) else {
                continue 'start;
            };
            if cell.symbol() != *want {
                continue 'start;
            }
        }
        return u16::try_from(start + symbols.len()).unwrap_or(u16::MAX);
    }
    panic!("{symbols:?} not found on row {y}");
}

/// The focused band row, and the header's focused half (`standing_out`), are extended by
/// one blank cell in the selected style right after their last character; the cell after
/// that one stays plain.
#[test]
fn a_focused_band_row_or_header_half_gets_one_highlighted_cell_past_its_last_character() {
    let theme = Theme::new(false);
    let selected = theme.style(Role::Selected);
    let assert_selected_then_plain = |buffer: &Buffer, after: u16, y: u16, what: &str| {
        let cell = buffer.cell((after, y)).expect("a cell after the value");
        assert_eq!(cell.symbol(), " ", "{what}: not a blank cell after it");
        assert_eq!(
            Some(cell.fg),
            selected.fg,
            "{what}: the cell right after it is not selected"
        );
        assert!(
            cell.modifier.contains(selected.add_modifier),
            "{what}: the cell right after it is not selected"
        );
        let next = buffer
            .cell((after + 1, y))
            .expect("a cell two past the value");
        assert_ne!(
            Some(next.fg),
            selected.fg,
            "{what}: the highlight ran two cells past it"
        );
    };

    // Band 1, the row the Bands table opens on: its raw Q, the last of the three raw
    // values the row ends on.
    let buffer = screen(&flat_app());
    let after = column_after(&buffer, 18, &["6", "4"]);
    assert_selected_then_plain(&buffer, after, 18, "the focused band row");

    // The header's mode half, once the cursor is on it.
    let mut app = flat_app();
    app.tone.row = ToneRow::Mode;
    let buffer = screen(&app);
    let after = column_after(&buffer, 0, &["E", "Q", " ", "›"]);
    assert_selected_then_plain(&buffer, after, 0, "the header's focused mode half");

    // The header's loudness half, once the cursor is there instead: a distinct raw value
    // (7, not otherwise on the header) marks where it ends.
    app.tone.row = ToneRow::Loudness;
    hear(&mut app, vec![vo("12.52", 7)]);
    let buffer = screen(&app);
    let after = column_after(&buffer, 0, &[" ", "7"]);
    assert_selected_then_plain(&buffer, after, 0, "the header's focused loudness half");
}

/// The longest product name the header test tries: far past what leaves the line any spare
/// cell, so the name is cut short for every length from there on.
const HEADER_LONGEST_NAME_LEN: usize = 60;

/// The mouse indicator's text, read off row 0 up to the terminal's own right edge.
fn mouse_reads_in_full(app: &App) -> bool {
    let buffer = screen(app);
    let row: String = (0..buffer.area.width)
        .map(|x| buffer.cell((x, 0)).unwrap().symbol())
        .collect();
    row.trim_end().ends_with("mouse:on")
}

/// A product name of any length, cut short with `…` once it leaves the header no spare cell, must
/// not let a focused half's trailing pad push `mouse:on` off the terminal's right edge: the mode
/// and loudness halves' pad has to be measured against the header's own real remaining budget.
#[test]
fn a_focused_headers_pad_never_pushes_the_mouse_indicator_off() {
    let mut app = flat_app();
    for len in 9..=HEADER_LONGEST_NAME_LEN {
        app.session.product = "X".repeat(len);
        for row in [ToneRow::Band, ToneRow::Mode, ToneRow::Loudness] {
            app.tone.row = row;
            assert!(
                mouse_reads_in_full(&app),
                "{row:?} with a {len}-letter product: mouse:on does not read in full"
            );
        }
    }
}

/// A device name too long for the tone screen's header line is cut short with `…` so the mode,
/// the loudness and the mouse indicator read whole, at 80, 100 (where the small wordmark takes
/// 33 columns) and 120.
#[test]
fn a_long_device_name_is_elided_in_the_tone_header() {
    for width in [80, 100, 120] {
        let mut app = flat_app();
        app.session.product = "Forkbeard Gungnir 2 Limited Anniversary Edition Prototype".into();
        let buffer = frame_with_theme(&app, width, 30, &Theme::new(false));
        let rows = text(&buffer);
        let row = rows
            .iter()
            .find(|row| row.contains("Mode"))
            .expect("the header row");
        let row = row.trim_end();
        assert!(row.ends_with("mouse:on"), "{width}: |{row}|");
        assert!(row.contains("…"), "{width}: the name is elided: |{row}|");
        assert!(row.contains("Loudness"), "{width}: |{row}|");
    }
}

/// A standing preset named in wide glyphs is cut by cells: the text the header gets is never
/// wider than its room, whatever the room, and ends in `…` when the name is cut.
#[test]
fn a_wide_preset_name_is_cut_to_the_cells_the_header_has() {
    let standing = crate::tui::boxes::presets::Standing {
        lead: "",
        name: "世界世界世界".into(),
        mark: "*",
        preset: true,
    };
    for room in 0..30 {
        if let Some(text) = tone::preset_text(Some(&standing), room) {
            assert!(widgets::text::cells(&text) <= room, "room {room}: |{text}|");
        }
    }
    // `  ` and `*` leave seven cells for a name of twelve: three glyphs and the `…`.
    assert_eq!(
        tone::preset_text(Some(&standing), 10).as_deref(),
        Some("  世界世…*")
    );
}
