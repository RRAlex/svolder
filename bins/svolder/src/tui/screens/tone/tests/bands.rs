// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Bands block: the table, the edit line and the notes.

use super::*;

/// While the curve boosts in EQ mode the band table's bottom border says how far it peaks
/// and how far down that keeps the volume, in the dim role — whole where it fits, without
/// its last words where it does not — and nothing once the curve is flat or only cuts.
#[test]
fn the_bands_note_how_far_a_boosting_curve_peaks() {
    let theme = Theme::new(false);
    let app = lifted_app();
    let buffer = screen(&app);
    assert_eq!(
        text(&buffer)[23],
        "└ peaks at +6.0 dB — keep the volume at or below -6.0 dB to avoid clipping ────┘"
    );
    let note = buffer.cell((2, 23)).expect("the note's first letter");
    assert!(note.modifier.contains(theme.style(Role::Dim).add_modifier));

    let wide = text(&frame_with_theme(&app, 126, 30, &theme));
    assert!(
        wide.iter().any(|row| row.contains(
            "peaks at +6.0 dB — keep the volume at or below -6.0 dB to avoid clipping on \
                 loud masters "
        )),
        "{wide:#?}"
    );

    assert_eq!(
        text(&screen(&flat_app()))[23],
        "└──────────────────────────────────────────────────────────────────────────────┘"
    );
    let mut cut = flat_app();
    hear(&mut cut, vec![vo("12.2.3", 60)]);
    assert_eq!(
        text(&screen(&cut))[23],
        "└──────────────────────────────────────────────────────────────────────────────┘"
    );
}

/// The column `wanted` starts at on row `y`, counted in cells.
fn cell_column(buffer: &Buffer, y: u16, wanted: &str) -> u16 {
    let row: Vec<String> = (0..buffer.area.width)
        .map(|x| buffer.cell((x, y)).expect("a cell").symbol().to_string())
        .collect();
    let wanted: Vec<String> = wanted.chars().map(String::from).collect();
    let at = row
        .windows(wanted.len())
        .position(|window| window == wanted.as_slice())
        .unwrap_or_else(|| panic!("no {wanted:?} on row {y}: {}", row.concat()));
    u16::try_from(at).expect("a column inside the frame")
}

/// An 80×24 terminal with band 2's edit line open and its gain typed as `1.5`: the line
/// takes the place of the band's row, its fields where the row's values were read from and
/// its own keys after them, the terminal's cursor after what has been typed. The line is
/// the modal the frame reports.
#[test]
fn the_edit_line_takes_the_place_of_its_bands_row_at_80_by_24() {
    let mut app = flat_app();
    keyed(
        &mut app,
        &[KeyCode::Char('2'), KeyCode::Enter, KeyCode::Tab],
    );
    type_text(&mut app, "1.5");
    let (buffer, areas, cursor) = framed(&app, (80, 24), &Theme::new(false));
    assert_rows(
        &buffer,
        &[
            " svolder  Gungnir 2  Preset Flat        Mode ‹ EQ ›   Loudness ░░░░░ 0  mouse:on",
            "─ Controls ──┤ EQ ├── Telemetry ── Devices ───────────────── ? help ─ esc back ─",
            "┌ Response ────────────────────────────────────────── flat ─ presets ─ refresh ┐",
            "│ +6 │                                                                         │",
            "│    │                                                                         │",
            "│    │                     ⡠•⡀                                                 │",
            "│  0 │⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠊⠉  ⠈⠉⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒│",
            "│    │                                                                         │",
            "│    │                                                                         │",
            "│ -6 │                                                                         │",
            "│    │                                                                         │",
            "│    │                                                                         │",
            "│-12 │                                                                         │",
            "│    └─────────────────────────────────────────────────────────────────────────│",
            "│    20        50     100    200       500    1k     2k        5k           20k│",
            "└──────────────────────────────────────────────────────────────────────────────┘",
            "┌ Bands ──────────────────────────────────────────────────────────── 0 flatten ┐",
            "│   #   Freq        Gain       Q        raw (f/g/q)                            │",
            "│   1   40.0 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "│ ▶ 2 f 160 Hz     g 1.5 dB     q 4.30      Tab ←→ ↑↓ · ↵ writes · Esc cancels │",
            "│   3    632 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "│   4   2515 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "│   5  10000 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "└ peaks at +1.4 dB — keep the volume at or below -1.4 dB to avoid clipping ────┘",
        ],
    );
    assert_eq!(
        areas.modal,
        Some(Rect {
            x: 1,
            y: 19,
            width: 78,
            height: 1
        })
    );
    assert_eq!(
        cursor,
        (cell_column(&buffer, 19, "1.5") + 3, 19),
        "the cursor is not after the 1.5 typed"
    );
    // A frequency typed long scrolls inside its field, the cursor at the field's end, and
    // the fields after it keep their places.
    keyed(&mut app, &[KeyCode::BackTab]);
    type_text(&mut app, "123456789");
    let (buffer, _, cursor) = framed(&app, (80, 24), &Theme::new(false));
    assert_eq!(
        text(&buffer)[19],
        "│ ▶ 2 f …56789 Hz  g 1.5 dB     q 4.30      Tab ←→ ↑↓ · ↵ writes · Esc cancels │"
    );
    assert_eq!(cursor, (cell_column(&buffer, 19, "…56789") + 6, 19));
}

/// Where the row has room for them, the line says what every one of its keys does.
#[test]
fn a_wide_edit_line_says_what_every_key_does() {
    let mut app = flat_app();
    keyed(&mut app, &[KeyCode::Char('2'), KeyCode::Enter]);
    let (buffer, areas, _) = framed(&app, (126, 30), &Theme::new(false));
    let row = areas.modal.expect("the line is a modal");
    let drawn = &text(&buffer)[usize::from(row.y)];
    assert!(
        drawn.contains("f 160 Hz     g +0.0 dB    q 4.30")
            && drawn.contains("Tab/⇧Tab ←→ moves · ↑↓ steps · Enter writes · Esc cancels "),
        "{drawn}"
    );
}

/// The edit line is drawn as a modal box is, at true colour and at 16 colours: the screen
/// round it dimmed and nothing in it, the fields in the primary text and its keys in the
/// accent. The field about to be typed over is drawn selected, and the one being typed into
/// in the cursor's accent. Nothing of the screen beneath is handed to the animation layer.
#[test]
fn the_edit_line_stands_at_full_strength_over_a_dimmed_screen() {
    for (depth, theme) in crate::tui::test_support::modal_themes() {
        let mut app = lifted_app();
        keyed(&mut app, &[KeyCode::Char('2'), KeyCode::Enter]);
        let (buffer, areas, _) = framed(&app, (80, 24), &theme);
        let row = areas.modal.expect("the line is a modal");
        assert!(
            crate::tui::test_support::greyed(&buffer, row, &theme).is_empty(),
            "{depth}: a grey cell on the line"
        );
        let whole = buffer.area;
        let dimmed = crate::tui::test_support::greyed(&buffer, whole, &theme).len();
        let glyphs = (whole.top()..whole.bottom())
            .flat_map(|y| (whole.left()..whole.right()).map(move |x| (x, y)))
            .filter(|&(x, y)| !row.contains(ratatui::layout::Position::new(x, y)))
            .filter(|&(x, y)| {
                !buffer
                    .cell((x, y))
                    .expect("a cell")
                    .symbol()
                    .trim()
                    .is_empty()
            })
            .count();
        assert_eq!(
            dimmed, glyphs,
            "{depth}: a glyph round the line is not dimmed"
        );
        let style_at = |wanted: &str| {
            let x = cell_column(&buffer, row.y, wanted);
            let cell = buffer.cell((x, row.y)).expect("a cell");
            (cell.fg, cell.modifier)
        };
        let role = |role: Role| {
            let style = theme.style(role);
            (
                style.fg.unwrap_or(ratatui::style::Color::Reset),
                style.add_modifier,
            )
        };
        assert_eq!(
            style_at("160"),
            role(Role::Selected),
            "{depth}: fresh field"
        );
        assert_eq!(style_at("+6.0").0, role(Role::Text).0, "{depth}: gain");
        assert_eq!(style_at("Esc").0, role(Role::KeyHint).0, "{depth}: keys");
        assert!(areas.rows.is_empty() && areas.curve.is_none(), "{depth}");
        assert!(areas.frames.is_empty(), "{depth}");

        type_text(&mut app, "2");
        let (buffer, _, _) = framed(&app, (80, 24), &theme);
        let x = cell_column(&buffer, row.y, "f 2 Hz") + 2;
        let cell = buffer.cell((x, row.y)).expect("a cell");
        assert_eq!(
            cell.fg,
            role(Role::Cursor).0,
            "{depth}: the field typed into"
        );
        assert!(!cell.modifier.contains(Modifier::REVERSED), "{depth}");
    }
}

/// The band table reads a gain to the hundredth the device's step lands on, and a whole
/// number of dB to the tenth, the unit in one column down the table whatever the width: what
/// `1.5` typed into the edit line reads once the device has taken it.
#[test]
fn the_band_table_reads_a_gain_to_the_step_the_device_took() {
    let mut app = flat_app();
    hear(&mut app, vec![vo("12.2.3", 105), vo("12.3.3", 47)]);
    let rows = text(&screen(&app));
    assert_eq!(
        rows[19],
        "│   2    160 Hz   +1.45 dB    4.30      64 / 105 / 64                          │"
    );
    assert_eq!(
        rows[20],
        "│   3    632 Hz  -10.41 dB    4.30      64 / 47 / 64                           │"
    );
    let unit = |row: &str| row[..row.find(" dB").expect("a gain")].chars().count();
    assert_eq!(unit(&rows[18]), unit(&rows[19]));
    assert_eq!(unit(&rows[18]), unit(&rows[20]));
}

/// A refused `Enter` says why on the line itself, in the keys' place and in the alert role,
/// at full strength over the dimmed screen at true colour and at 16 colours, and the keys come
/// back with the next key. A band whose window starts below 100 Hz says it to the tenth.
#[test]
fn a_refusal_stands_on_the_line_in_the_alert_role_until_the_next_key() {
    for (depth, theme) in crate::tui::test_support::modal_themes() {
        let mut app = flat_app();
        keyed(&mut app, &[KeyCode::Char('2'), KeyCode::Enter]);
        type_text(&mut app, "5");
        keyed(&mut app, &[KeyCode::Enter]);
        let (buffer, areas, _) = framed(&app, (80, 24), &theme);
        let row = areas.modal.expect("the line is a modal");
        assert_eq!(
            text(&buffer)[usize::from(row.y)],
            "│ ▶ 2 f 5 Hz       g +0.0 dB    q 4.30                   f must be 82.0–312 Hz │",
            "{depth}"
        );
        assert_eq!(
            app.toast().map(|toast| (toast.kind, toast.text.as_str())),
            Some((ToastKind::Failure, "band 2: f must be 82.0–312 Hz")),
            "{depth}: the toast"
        );
        assert!(
            text(&buffer)[22].contains("band 2: f must be 82.0–312 Hz"),
            "{depth}: the toast is not in the corner"
        );
        let x = cell_column(&buffer, row.y, "f must");
        let alert = theme.style(Role::Alert).fg;
        for x in x..x + 21 {
            let cell = buffer.cell((x, row.y)).expect("a cell");
            assert_eq!(
                Some(cell.fg),
                alert,
                "{depth}: column {x} is not in the alert role"
            );
        }
        assert!(
            crate::tui::test_support::greyed(&buffer, row, &theme).is_empty(),
            "{depth}: the refusal is dimmed"
        );

        type_text(&mut app, "8");
        let (buffer, _, _) = framed(&app, (80, 24), &theme);
        assert_eq!(
            text(&buffer)[usize::from(row.y)],
            "│ ▶ 2 f 8 Hz       g +0.0 dB    q 4.30      Tab ←→ ↑↓ · ↵ writes · Esc cancels │",
            "{depth}: the keys did not come back"
        );
    }
    let mut app = flat_app();
    keyed(&mut app, &[KeyCode::Char('1'), KeyCode::Enter]);
    type_text(&mut app, "90");
    keyed(&mut app, &[KeyCode::Enter]);
    let (buffer, areas, _) = framed(&app, (80, 24), &Theme::new(false));
    let row = areas.modal.expect("the line is a modal");
    assert!(
        text(&buffer)[usize::from(row.y)].ends_with("f must be 20.0–78.0 Hz │"),
        "{}",
        text(&buffer)[usize::from(row.y)]
    );
}

/// Below 100 Hz the band table reads a frequency to the tenth of a hertz, so two neighbouring
/// steps never read the same, in the column's own width.
#[test]
fn a_frequency_below_100_hz_reads_to_the_tenth() {
    let mut app = flat_app();
    hear(&mut app, vec![vo("12.1.2", 65)]);
    assert_eq!(
        text(&screen(&app))[18],
        "│ ▶ 1   40.4 Hz    +0.0 dB    4.30      65 / 98 / 64                           │"
    );
}

/// A number that lands on the band's own step says so where the keys were, in the notice
/// role rather than the alert one: nothing was wrong with it, and nothing was written.
#[test]
fn a_number_on_its_bands_step_says_so_in_the_notice_role() {
    for (depth, theme) in crate::tui::test_support::modal_themes() {
        let mut app = flat_app();
        keyed(
            &mut app,
            &[KeyCode::Char('2'), KeyCode::Enter, KeyCode::Tab],
        );
        type_text(&mut app, "0.1");
        keyed(&mut app, &[KeyCode::Enter]);
        let (buffer, areas, _) = framed(&app, (80, 24), &theme);
        let row = areas.modal.expect("the line is still up");
        assert_eq!(
            text(&buffer)[usize::from(row.y)],
            "│ ▶ 2 f 160 Hz     g 0.1 dB     q 4.30                      already at +0.0 dB │",
            "{depth}"
        );
        let x = cell_column(&buffer, row.y, "already");
        let cell = buffer.cell((x, row.y)).expect("a cell");
        assert_eq!(Some(cell.fg), theme.style(Role::Notice).fg, "{depth}");
    }
}

/// Every refusal the line can say fits whole where the keys are on an 80-column row.
#[test]
fn every_refusal_fits_an_80_column_line() {
    for (keys, words) in [
        (
            &[KeyCode::Char('5'), KeyCode::Enter][..],
            "f must be 82.0–312 Hz",
        ),
        (
            &[KeyCode::Char('x')][..],
            "only digits, one . or , and a sign",
        ),
        (
            &[KeyCode::Char('.'), KeyCode::Char('.')][..],
            "only digits, one . or , and a sign",
        ),
    ] {
        let mut app = flat_app();
        keyed(&mut app, &[KeyCode::Char('2'), KeyCode::Enter]);
        keyed(&mut app, keys);
        let (buffer, areas, _) = framed(&app, (80, 24), &Theme::new(false));
        let row = areas.modal.expect("the line is up");
        assert!(
            text(&buffer)[usize::from(row.y)].ends_with(&format!("{words} │")),
            "{}",
            text(&buffer)[usize::from(row.y)]
        );
    }
    let mut app = flat_app();
    keyed(&mut app, &[KeyCode::Char('2'), KeyCode::Enter]);
    type_text(&mut app, &"9".repeat(13));
    let (buffer, areas, _) = framed(&app, (80, 24), &Theme::new(false));
    let row = areas.modal.expect("the line is up");
    assert!(
        text(&buffer)[usize::from(row.y)].ends_with("a number takes at most 12 characters │"),
        "{}",
        text(&buffer)[usize::from(row.y)]
    );
}
