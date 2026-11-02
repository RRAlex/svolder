// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use svolder_profiles::EqMode;

use crate::tui::app::{App, Screen};
use crate::tui::mouse::HitMap;
use crate::tui::render::{self, FrameAreas};
use crate::tui::test_support::presets::{own, resting};
use crate::tui::test_support::{assert_drawn_in_focus, greyed, modal_themes};
use crate::tui::test_support::{assert_rows, text};
use crate::tui::theme::{Role, Theme};

use super::*;

/// One frame of `app` in a terminal `width` by `height` drawn with `theme`, and where it
/// put its pieces.
fn frame(app: &App, (width, height): (u16, u16), theme: &Theme) -> (Buffer, FrameAreas) {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut areas = FrameAreas::default();
    terminal
        .draw(|f| areas = render::draw(f, app, theme, &mut hits))
        .expect("the frame draws");
    (terminal.backend().buffer().clone(), areas)
}

/// The box's own rows, as text.
fn boxed_rows(buffer: &Buffer, area: Rect) -> Vec<String> {
    (area.top()..area.bottom())
        .map(|y| {
            (area.left()..area.right())
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect()
        })
        .collect()
}

/// Press one key.
fn press(app: &mut App, code: KeyCode) {
    app.key(KeyEvent::from(code));
}

/// A Gungnir 2 on its tone-control screen, resting flat in EQ mode, with one preset of the
/// user's own in the file, and the preset list open on it.
fn listing() -> App {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    app.presets.saved = vec![own("HD650 warm", 5, 1.0)];
    press(&mut app, KeyCode::Char('p'));
    app
}

/// The list over the tone-control screen on an 80×24 terminal: the built-ins first and
/// marked, the one that is on marked `●`, the keys named on the box, and the keys that
/// change a preset dimmed with the reason while a built-in is focused. usage/tui.md shows this
/// very frame.
#[test]
fn the_list_over_the_tone_screen_at_80_by_24() {
    let (buffer, _) = frame(&listing(), (80, 24), &Theme::new(false));
    assert_rows(
        &buffer,
        &[
            " svolder  Gungnir 2  Preset Flat        Mode ‹ EQ ›   Loudness ░░░░░ 0  mouse:on",
            "─ Controls ──┤ EQ ├── Telemetry ── Devices ───────────────── ? help ─ esc back ─",
            "┌ Response ────────────────────────────────────────── flat ─ presets ─ refresh ┐",
            "│ +6 │                                                                         │",
            "│    │                                                                         │",
            "│    │                                                                         │",
            "│  0 │⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒│",
            "│    │  ┌ Presets · 5 bands ──────────────────────────────────────────┐        │",
            "│    │  │ ▶ ● Flat        built-in                                    │        │",
            "│ -6 │  │     Warm        built-in                                    │        │",
            "│    │  │     Bass        built-in                                    │        │",
            "│    │  │     Presence    built-in                                    │        │",
            "│-12 │  │     HD650 warm                                              │        │",
            "│    └──│                                                             │────────│",
            "│    20 │ Enter loads · n saves new · Esc closes                      │     20k│",
            "└───────│ u updates · c renames · x deletes · built-ins are read-only │────────┘",
            "┌ Bands │ a AutoEq · i from file · h computer                         │flatten ┐",
            "│   #   └─────────────────────────────────────────────────────────────┘        │",
            "│ ▶ 1   40.0 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "│   2    160 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "│   3    632 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "│   4   2515 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "│   5  10000 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "└──────────────────────────────────────────────────────────────────────────────┘",
        ],
    );
}

/// On a preset of the user's own the keys that change it are named as every modal box names
/// its keys; on a three-band tone control the title says so, and so do the built-ins.
#[test]
fn the_keys_that_change_a_preset_are_live_on_the_users_own() {
    let mut app = listing();
    for _ in 0..4 {
        press(&mut app, KeyCode::Down);
    }
    let (buffer, areas) = frame(&app, (80, 24), &Theme::new(false));
    let area = areas.modal.expect("the list is a modal box");
    let rows = boxed_rows(&buffer, area);
    assert_eq!(
        rows[5],
        "│ ▶   HD650 warm                                              │"
    );
    assert_eq!(
        rows[8],
        "│ u updates · c renames · x deletes                           │"
    );

    let mut three = resting("mimir", EqMode::Eq);
    press(&mut three, KeyCode::Char('p'));
    let (buffer, areas) = frame(&three, (80, 24), &Theme::new(false));
    let rows = boxed_rows(&buffer, areas.modal.expect("the list is a modal box"));
    assert!(rows[0].starts_with("┌ Presets · 3 bands "), "|{}|", rows[0]);
    assert_eq!(
        rows.len(),
        4 + 6,
        "the four built-ins, the blank and the three lines of keys"
    );
}

/// The name prompt and the two questions are boxes of their own over the screen: what they
/// are about, the line being typed or the question, and the keys that answer them.
#[test]
fn the_prompt_and_the_questions_draw_what_they_ask() {
    let mut app = listing();
    press(&mut app, KeyCode::Char('n'));
    let (buffer, areas) = frame(&app, (80, 24), &Theme::new(false));
    assert_eq!(
        boxed_rows(&buffer, areas.modal.expect("the prompt is a modal box")),
        [
            "┌ New preset ──────────────────────────────────────────┐",
            "│ Name  custom 1                                       │",
            "│                                                      │",
            "│ Enter saves · Esc cancels                            │",
            "└──────────────────────────────────────────────────────┘",
        ]
    );
    press(&mut app, KeyCode::Esc);
    for _ in 0..4 {
        press(&mut app, KeyCode::Down);
    }
    press(&mut app, KeyCode::Char('c'));
    let (buffer, areas) = frame(&app, (80, 24), &Theme::new(false));
    let rows = boxed_rows(&buffer, areas.modal.expect("the prompt is a modal box"));
    assert!(
        rows[0].starts_with("┌ Rename HD650 warm ─"),
        "|{}|",
        rows[0]
    );
    assert_eq!(
        rows[3],
        "│ Enter renames · Esc cancels                          │"
    );

    press(&mut app, KeyCode::Esc);
    for (key, wanted) in [
        (
            'u',
            [
                "┌ Update ───────────────────────────────────────────────┐",
                "│ Update HD650 warm with the curve on the device? [y/N] │",
                "│                                                       │",
                "│ y updates · Enter/n/Esc cancels                       │",
                "└───────────────────────────────────────────────────────┘",
            ],
        ),
        (
            'x',
            [
                "┌ Delete ─────────────────────────────────────────┐",
                "│ Delete HD650 warm from the configuration? [y/N] │",
                "│                                                 │",
                "│ y deletes · Enter/n/Esc cancels                 │",
                "└─────────────────────────────────────────────────┘",
            ],
        ),
    ] {
        press(&mut app, KeyCode::Char(key));
        let (buffer, areas) = frame(&app, (80, 24), &Theme::new(false));
        assert_eq!(
            boxed_rows(&buffer, areas.modal.expect("the question is a modal box")),
            wanted,
            "{key}"
        );
        press(&mut app, KeyCode::Esc);
    }
}

/// The name prompt puts the terminal's own cursor where the next letter goes.
#[test]
fn the_name_prompt_puts_the_cursor_where_the_next_letter_goes() {
    let mut app = listing();
    press(&mut app, KeyCode::Char('n'));
    press(&mut app, KeyCode::Left);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal");
    let mut hits = HitMap::default();
    let mut areas = FrameAreas::default();
    let theme = Theme::new(false);
    terminal
        .draw(|f| areas = render::draw(f, &app, &theme, &mut hits))
        .expect("the frame draws");
    let area = areas.modal.expect("the prompt is a modal box");
    let cursor = terminal.get_cursor_position().expect("the cursor");
    // Border, the space the line opens with, "Name  ", then "custom " is seven letters in.
    assert_eq!(
        (cursor.x, cursor.y),
        (area.x + 1 + 1 + 6 + 7, area.y + 1),
        "the cursor is not on the 1 it was moved back onto"
    );
}

/// The box is drawn as the other modal boxes are, over the tone-control screen and over the
/// device screen alike: in the theme's own colours, with everything around it dimmed, and
/// the one grey inside it the built-ins' mark and, on a built-in, the keys that do nothing
/// there beside their reason.
#[test]
fn the_box_is_drawn_in_the_themes_colours_over_either_screen() {
    for (depth, theme) in modal_themes() {
        for screen in [Screen::Tone, Screen::Device] {
            let mut app = listing();
            app.show(screen);
            let label = format!("{screen:?} at {depth}");
            let (buffer, areas) = frame(&app, (80, 24), &theme);
            let area = areas.modal.expect("the list is a modal box");
            // Nothing the screen under it earned plays over the dimmed cells.
            assert!(
                areas.frames.is_empty()
                    && areas.header.is_empty()
                    && areas.curve.is_none()
                    && areas.rows.is_empty(),
                "{label}: the animation layer is handed the screen under the box"
            );
            assert_drawn_in_focus(&buffer, area, &theme, &label);
            let rows = text(&buffer);
            for (_, y, grey) in greyed(&buffer, area, &theme) {
                let row = &rows[usize::from(y)];
                assert!(
                    grey == "faint" && (row.contains(BUILT_IN) || row.contains(READ_ONLY)),
                    "{label}: a {grey} cell on |{row}|"
                );
            }
            let outside = buffer.cell((1, 23)).expect("the key line's badge");
            let dim = theme.style(Role::Dim);
            match dim.fg {
                Some(grey) => assert_eq!(outside.fg, grey, "{label}: the key line"),
                None => assert!(
                    outside.modifier.contains(ratatui::style::Modifier::DIM),
                    "{label}: the key line is not dimmed"
                ),
            }
        }
    }
}

/// The cells of the row of the box that carries `wanted`, with their columns.
fn row_cells(buffer: &Buffer, area: Rect, wanted: &str) -> (u16, Vec<ratatui::buffer::Cell>) {
    let rows = boxed_rows(buffer, area);
    let at = rows
        .iter()
        .position(|row| row.contains(wanted))
        .unwrap_or_else(|| panic!("no row of the box carries {wanted:?}: {rows:#?}"));
    let y = area.y + u16::try_from(at).unwrap_or(0);
    let cells = (area.left()..area.right())
        .map(|x| buffer.cell((x, y)).expect("a cell of the box").clone())
        .collect();
    (y, cells)
}

/// The first cell of `wanted` on a row whose cells are `cells`.
fn cell_of(cells: &[ratatui::buffer::Cell], wanted: &str) -> ratatui::buffer::Cell {
    let symbols: Vec<&str> = cells.iter().map(ratatui::buffer::Cell::symbol).collect();
    let first = wanted.chars().next().map(String::from).unwrap_or_default();
    let at = (0..symbols.len())
        .find(|at| symbols[*at] == first && symbols[*at..].concat().starts_with(wanted))
        .unwrap_or_else(|| panic!("{wanted:?} is not on the row"));
    cells[at].clone()
}

/// While a built-in is focused the keys that would change it are drawn in the faint grey,
/// and on a preset of the user's own in the accent the other keys take; the `built-in` mark
/// is faint on every row but the focused one.
#[test]
fn the_keys_a_built_in_refuses_are_faint_and_so_is_its_mark() {
    let theme = modal_themes()
        .into_iter()
        .find(|(depth, _)| *depth == "true colour")
        .map(|(_, theme)| theme)
        .expect("a true-colour theme");
    let faint = theme.style(Role::Faint).fg.expect("a faint grey");
    let accent = theme.style(Role::KeyHint).fg.expect("an accent");
    let mut app = listing();
    let (buffer, areas) = frame(&app, (80, 24), &theme);
    let area = areas.modal.expect("the list is a modal box");
    let (_, keys) = row_cells(&buffer, area, "u updates");
    for key in ["u updates", "c renames", "x deletes", READ_ONLY] {
        assert_eq!(cell_of(&keys, key).fg, faint, "{key} on a built-in");
    }
    let (_, warm) = row_cells(&buffer, area, "Warm");
    assert_eq!(
        cell_of(&warm, BUILT_IN).fg,
        faint,
        "an unfocused built-in's mark"
    );

    for _ in 0..4 {
        press(&mut app, KeyCode::Down);
    }
    let (buffer, areas) = frame(&app, (80, 24), &theme);
    let area = areas.modal.expect("the list is a modal box");
    let (_, keys) = row_cells(&buffer, area, "u updates");
    for key in ["u", "c", "x"] {
        let label = format!("{key} ");
        assert_eq!(cell_of(&keys, &label).fg, accent, "{key} on the user's own");
    }
    let (_, warm) = row_cells(&buffer, area, "Warm");
    assert_eq!(
        cell_of(&warm, BUILT_IN).fg,
        faint,
        "the mark with the focus elsewhere"
    );
}

/// A name too long for the box never breaks it: the list cuts it short with `…` so every
/// row keeps its marks and its border, the question keeps its `[y/N]`, and the prompt
/// scrolls the name through its field with the cursor inside the box.
#[test]
fn a_long_name_is_cut_and_scrolled_inside_the_boxes() {
    // Longer than any writer lets through, which the boxes still draw whole.
    let long = "Sennheiser HD650 with the tube amp, warm, for late nights at ni";
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    app.presets.saved = vec![own(long, 5, 1.0)];
    press(&mut app, KeyCode::Char('p'));
    let (buffer, areas) = frame(&app, (80, 24), &Theme::new(false));
    let area = areas.modal.expect("the list is a modal box");
    assert!(area.width <= 80);
    let rows = boxed_rows(&buffer, area);
    for row in &rows[1..5] {
        assert!(
            row.contains(BUILT_IN) && row.ends_with('│'),
            "a built-in's row lost its mark: |{row}|"
        );
    }
    assert!(
        rows[5].contains("Sennheiser") && rows[5].contains('…') && rows[5].ends_with('│'),
        "|{}|",
        rows[5]
    );

    for _ in 0..4 {
        press(&mut app, KeyCode::Down);
    }
    for key in ['u', 'x'] {
        press(&mut app, KeyCode::Char(key));
        let (buffer, areas) = frame(&app, (80, 24), &Theme::new(false));
        let area = areas.modal.expect("the question is a modal box");
        let question = &boxed_rows(&buffer, area)[1];
        assert!(
            question.contains("Sennheiser")
                && question.contains('…')
                && question.contains("[y/N] │"),
            "{key}: |{question}|"
        );
        press(&mut app, KeyCode::Esc);
    }

    press(&mut app, KeyCode::Char('c'));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal");
    let mut hits = HitMap::default();
    let mut areas = FrameAreas::default();
    let theme = Theme::new(false);
    terminal
        .draw(|f| areas = render::draw(f, &app, &theme, &mut hits))
        .expect("the frame draws");
    let area = areas.modal.expect("the prompt is a modal box");
    let rows = boxed_rows(terminal.backend().buffer(), area);
    assert!(
        rows[1].contains("Name  …") && rows[1].ends_with("ni │"),
        "the prompt does not show the end of the name it scrolled to: |{}|",
        rows[1]
    );
    assert!(
        rows[0].contains('…'),
        "the title did not cut the name: |{}|",
        rows[0]
    );
    let cursor = terminal.get_cursor_position().expect("the cursor");
    assert!(
        cursor.x < area.right() - 1 && cursor.y == area.y + 1,
        "the cursor is outside the field: {cursor:?} in {area:?}"
    );
}

/// A list longer than the box holds shows the rows round the focused one and says on its
/// border which way the rest are.
#[test]
fn a_list_longer_than_its_box_scrolls_with_the_focus() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    app.presets.saved = (1..=20)
        .map(|n| own(&format!("mine {n}"), 5, 0.0))
        .collect();
    press(&mut app, KeyCode::Char('p'));
    let (buffer, areas) = frame(&app, (80, 24), &Theme::new(false));
    let rows = boxed_rows(&buffer, areas.modal.expect("the list is a modal box"));
    assert!(rows[1].contains("▶ ● Flat"), "|{}|", rows[1]);
    assert!(rows.last().is_some_and(|row| row.contains("more ↓")));
    assert!(!rows[0].contains("more ↑"), "|{}|", rows[0]);

    for _ in 0..30 {
        press(&mut app, KeyCode::Down);
    }
    let (buffer, areas) = frame(&app, (80, 24), &Theme::new(false));
    let rows = boxed_rows(&buffer, areas.modal.expect("the list is a modal box"));
    let last = rows.len() - 6;
    assert!(rows[last].contains("▶   mine 20"), "|{}|", rows[last]);
    assert!(rows[0].contains("more ↑"), "|{}|", rows[0]);
    assert!(!rows.last().is_some_and(|row| row.contains("more ↓")));
}

/// The mockup docs/usage/tui.md shows of the preset list is the frame this build draws.
#[test]
fn the_docs_show_the_preset_list_this_build_draws() {
    let (buffer, _) = frame(&listing(), (80, 24), &Theme::new(false));
    crate::tui::test_support::assert_usage_md_shows(&text(&buffer), "preset list");
}

/// The preset list's rows fade by their distance from the focused one while the fade runs at
/// true colour; the box's border and its keys keep their colours, and nothing fades without
/// the switch.
#[test]
fn the_preset_rows_fade_with_the_distance_from_the_focus() {
    let theme = crate::tui::test_support::theme_for("amber", svolder_ipc::ColorDepth::TrueColor);
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    app.presets.saved = (1..=30)
        .map(|n| own(&format!("mine {n:02}"), 5, 0.0))
        .collect();
    press(&mut app, KeyCode::Char('p'));
    for _ in 0..20 {
        press(&mut app, KeyCode::Down);
    }
    let focused = app
        .presets
        .preset_list
        .as_ref()
        .expect("the list is open")
        .row;
    let (plain, areas) = frame(&app, (80, 24), &theme);
    app.list_fade = true;
    let (faded, _) = frame(&app, (80, 24), &theme);
    let modal = areas.modal.expect("the list is a modal box");
    let eq = app.profile.eq.as_ref().expect("a tone control");
    let names: Vec<String> = crate::tui::boxes::presets::entries(&app, eq)
        .into_iter()
        .map(|entry| entry.preset.name)
        .collect();
    let lines: Vec<(u16, usize)> = boxed_rows(&plain, modal)
        .iter()
        .enumerate()
        .filter_map(|(at, text)| {
            let index = names.iter().position(|name| text.contains(name.as_str()))?;
            Some((modal.y + at as u16, index.abs_diff(focused)))
        })
        .collect();
    assert!(lines.len() >= 15, "{lines:?}");
    crate::tui::test_support::assert_list_faded(
        &plain,
        &faded,
        &lines,
        modal.x + 1..modal.right() - 1,
        &theme,
    );

    let q = crate::tui::test_support::theme_for("amber", svolder_ipc::ColorDepth::Ansi256);
    app.list_fade = false;
    let (off, _) = frame(&app, (80, 24), &q);
    app.list_fade = true;
    assert_eq!(off, frame(&app, (80, 24), &q).0);
}

/// A Gungnir 2 whose file holds an imported preset and a preset of bands, with the list open.
fn with_imports(name: &str) -> App {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    app.presets.saved = vec![
        own("HD650 warm", 5, 1.0),
        crate::tui::test_support::presets::imported(name),
    ];
    press(&mut app, KeyCode::Char('p'));
    app
}

/// The row of `rows` holding `what`.
fn row_with<'a>(rows: &'a [String], what: &str) -> &'a str {
    rows.iter()
        .find(|row| row.contains(what))
        .unwrap_or_else(|| panic!("no row holds {what}: {rows:#?}"))
}

/// An imported preset's row says `imported` and its fit, and the one on the computer says
/// `computer` in the accent; at 80 columns and wide the same columns line up, and a box too
/// narrow for the fit keeps the marks and drops it.
#[test]
fn imported_and_computer_rows_carry_their_tags() {
    let mut app = with_imports("Phones");
    app.presets.computer = Some("phones".into());
    let eq = app.profile.eq.clone().expect("a tone control");
    let source = app.presets.saved[1].source.clone().expect("imported");
    let fit = format!(
        "fit {:.1} dB",
        svolder_profiles::tone::preset::fit_for(&eq, &source).rms_db
    );
    for size in [(80, 24), (200, 50), (300, 50)] {
        let (buffer, areas) = frame(&app, size, &Theme::new(false));
        let rows = boxed_rows(&buffer, areas.modal.expect("the list is a modal box"));
        let line = row_with(&rows, "Phones");
        assert!(
            line.contains(&format!("Phones      imported  {fit}  computer")),
            "{size:?}: |{line}|"
        );
        assert!(
            !row_with(&rows, "HD650 warm").contains("imported"),
            "{size:?}: a preset of bands is marked imported"
        );
        assert!(row_with(&rows, "Flat").contains("built-in"));
    }

    // The accent is the computer's own, not the faint grey of the other tags.
    let theme = crate::tui::test_support::theme_for("amber", svolder_ipc::ColorDepth::TrueColor);
    let (buffer, areas) = frame(&app, (80, 24), &theme);
    let modal = areas.modal.expect("the list is a modal box");
    let rows = boxed_rows(&buffer, modal);
    let at = rows
        .iter()
        .position(|row| row.contains("Phones"))
        .expect("a row");
    let colour_of = |what: &str| {
        let column = rows[at]
            .find(what)
            .map(|byte| rows[at][..byte].chars().count())
            .expect("the tag");
        let cell = buffer
            .cell((
                modal.x + u16::try_from(column).expect("a column"),
                modal.y + u16::try_from(at).expect("a row"),
            ))
            .expect("a cell");
        cell.fg
    };
    assert_ne!(
        colour_of("computer"),
        colour_of("imported"),
        "computer is not in its own colour"
    );
}

/// The box's key row and the help overlay name `a`, `i` and `h`.
#[test]
fn the_key_row_and_the_help_overlay_name_the_new_keys() {
    let (buffer, areas) = frame(&listing(), (80, 24), &Theme::new(false));
    let rows = boxed_rows(&buffer, areas.modal.expect("the list is a modal box"));
    assert!(
        row_with(&rows, "a AutoEq").contains("a AutoEq · i from file · h computer"),
        "{rows:#?}"
    );

    let mut app = listing();
    press(&mut app, KeyCode::Esc);
    app.overlays.help_open = true;
    let (buffer, _) = frame(&app, (126, 62), &Theme::new(false));
    let help = text(&buffer).join("\n");
    assert!(help.contains("presets (a i h inside)"), "{help}");
}

/// The search box at 80×24, with the index held, a query typed and the focus on the second
/// measurement of a headphone that two sources measured. usage/tui.md shows this very frame.
fn searching_frame() -> Vec<String> {
    let mut app = crate::tui::test_support::presets::searching(
        crate::tui::test_support::presets::two_headphones(),
    );
    crate::tui::test_support::type_text(&mut app, "hd 6");
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    let (buffer, _) = frame(&app, (80, 24), &Theme::new(false));
    text(&buffer)
}

#[test]
fn the_docs_show_the_search_box_this_build_draws() {
    crate::tui::test_support::assert_usage_md_shows(&searching_frame(), "search box");
}

/// The search box at 80×24 draws the query line, the rows grouped by headphone with the
/// focused one marked, the status line and the keys; before anything is typed it says what to
/// type, and while the index is on its way it says it is fetching.
#[test]
fn the_search_box_draws_its_rows_status_and_keys() {
    let rows = searching_frame();
    for (at, what) in [
        (7, "│ Search AutoEq  hd 6"),
        (9, "Sennheiser HD 600  oratory1990 · over-ear"),
        (11, "▶ Sennheiser HD 650  crinacle · over-ear · GRAS 43AG-7"),
        (17, "index from 2026-07-01 · 3 measurements"),
        (18, "↑↓ move · Enter imports · Ctrl-R refreshes · Esc back"),
    ] {
        assert!(rows[at].contains(what), "row {at}: |{}|", rows[at]);
    }

    let mut app = resting("gungnir2-v2", EqMode::Eq);
    press(&mut app, KeyCode::Char('p'));
    press(&mut app, KeyCode::Char('a'));
    let (buffer, areas) = frame(&app, (80, 24), &Theme::new(false));
    let rows = boxed_rows(&buffer, areas.modal.expect("the search is a modal box"));
    assert!(row_with(&rows, "fetching the index…").contains("fetching the index…"));
    assert!(row_with(&rows, "type a headphone name").contains("type a headphone name"));
    assert_eq!(rows.len(), 14, "{rows:#?}");
}

/// The file prompt over a temp tree: the path with the cursor at its end, and after a `Tab`
/// the candidates, six lines and then how many more, with the one stepped to focused.
#[test]
fn the_file_prompt_lists_its_candidates_six_and_then_how_many_more() {
    use crate::test_support::temp_dir;
    let dir = temp_dir("prompt-draw");
    for n in 1..=9 {
        std::fs::write(dir.0.join(format!("curve {n}.txt")), "x").expect("a file is written");
    }
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    press(&mut app, KeyCode::Char('p'));
    press(&mut app, KeyCode::Char('i'));
    if let Some(PresetList {
        step: Step::Prompting(prompt),
        ..
    }) = app.presets.preset_list.as_mut()
    {
        prompt.home = Some(dir.0.clone());
    }
    crate::tui::test_support::type_text(&mut app, "cu");
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    let (buffer, areas) = frame(&app, (80, 24), &Theme::new(false));
    let rows = boxed_rows(&buffer, areas.modal.expect("the prompt is a modal box"));
    assert_eq!(
        rows,
        [
            "┌ Import a file ───────────────────────────────────────┐",
            "│ File  ~/curve 1.txt                                  │",
            "│       curve 1.txt                                    │",
            "│       curve 2.txt                                    │",
            "│       curve 3.txt                                    │",
            "│       curve 4.txt                                    │",
            "│       curve 5.txt                                    │",
            "│       curve 6.txt                                    │",
            "│       … 3 more                                       │",
            "│                                                      │",
            "│ Tab completes · Enter imports · Esc back             │",
            "└──────────────────────────────────────────────────────┘",
        ]
    );
}

/// The list's columns are laid out in display cells: a preset named in wide glyphs takes two
/// cells for each, so the `imported` marks start in one screen column. (`text` gives one
/// string per screen cell, so a character count is a column.)
#[test]
fn the_columns_of_the_list_align_under_a_wide_preset_name() {
    let mut app = resting("gungnir2-v2", EqMode::Eq);
    app.presets.saved = vec![
        crate::tui::test_support::presets::imported("世界"),
        crate::tui::test_support::presets::imported("HD650 warm"),
    ];
    press(&mut app, KeyCode::Char('p'));
    let (buffer, areas) = frame(&app, (80, 24), &Theme::new(false));
    let rows = boxed_rows(&buffer, areas.modal.expect("the list is a modal box"));
    let column = |name: &str| {
        let row = row_with(&rows, name);
        row.chars().count()
            - row
                .split("imported")
                .nth(1)
                .expect("the mark")
                .chars()
                .count()
    };
    assert_eq!(column("世"), column("HD650"), "{rows:#?}");
}
