// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use svolder_ipc::{ColorDepth, TuiConfig};

use crossterm::event::{KeyCode, KeyEvent};

use crate::backend::DeviceRow;
use crate::tui::app::{App, Screen};
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::render;
use crate::tui::test_support::configured;
use crate::tui::test_support::{assert_rows, frame, frame_with_theme, profile, text, theme_for};
use crate::tui::theme::{Role, Theme};

/// One device the daemon reported.
fn row(name: &str, product: Option<&str>, connected: Option<bool>) -> DeviceRow {
    DeviceRow {
        name: name.into(),
        product: product.map(str::to_string),
        connected,
        retry_in_secs: None,
        ..DeviceRow::default()
    }
}

/// An app on the device list, watching `gungnir` of the two devices `rows` names, with the
/// configuration file saying about each of them what `svolder add` would have written.
fn app(rows: Vec<DeviceRow>) -> App {
    let mut app = App::new(profile(), "gungnir".into(), &TuiConfig::default(), rows);
    app.screen = Screen::Devices;
    app.clock = "14:02".into();
    app.devices.configured = vec![
        configured("bifrost", "1112131415161718"),
        configured("gungnir", "0102030405060708"),
    ];
    app
}

/// The same app with the editor open on the focused device, as `u` opens it.
fn editing(rows: Vec<DeviceRow>) -> App {
    let mut app = app(rows);
    app.key(KeyEvent::from(KeyCode::Char('u')));
    assert!(app.devices.editor.is_some(), "the editor did not open");
    app
}

/// The row of a frame carrying `text`.
fn row_with(buffer: &Buffer, wanted: &str) -> String {
    text(buffer)
        .into_iter()
        .find(|row| row.contains(wanted))
        .unwrap_or_else(|| panic!("no row carries {wanted:?}"))
}

/// The row of `lines` carrying `wanted`.
fn row_with_in<'a>(lines: &'a [String], wanted: &str) -> &'a str {
    lines
        .iter()
        .find(|row| row.starts_with('│') && row.contains(wanted))
        .unwrap_or_else(|| panic!("no list row carries {wanted:?}"))
}

/// The two devices a daemon holding both links reports.
fn daemon_rows() -> Vec<DeviceRow> {
    vec![
        row("bifrost", Some("Bifrost 3"), Some(true)),
        row("gungnir", Some("Gungnir 2"), Some(false)),
    ]
}

#[test]
fn the_device_list_names_every_device_its_product_and_its_state() {
    let mut expected = vec![
        " svolder  Gungnir 2  ● connected                                  mouse:on 14:02",
        "─ Controls ── EQ ── Telemetry ──┤ Devices ├───────────────── ? help ─ esc back ─",
        "┌ Devices ────────────────────────────────── new ─ update ─ x remove ─ refresh ┐",
        "│ ▶ bifrost   Bifrost 3   connected                                            │",
        "│   gungnir   Gungnir 2   disconnected (current)                               │",
    ];
    expected.append(&mut vec![
        "│                                                                              │";
        18
    ]);
    expected
        .push("└──────────────────────────────────────────────────────────────────────────────┘");
    assert_rows(&frame(&app(daemon_rows()), 80, 24), &expected);
}

/// The wall clock the list's signal cases hold, in unix milliseconds.
const WALL: u64 = 1_800_000_000_000;

/// A row of a device with `signal` heard `secs` ago as an advert.
fn heard(name: &str, product: &str, dbm: i16, secs: u64) -> DeviceRow {
    DeviceRow {
        signal: Some(svolder_ipc::Signal {
            dbm,
            source: svolder_ipc::SignalSource::Advert,
            heard_at_ms: WALL - secs * 1000,
        }),
        ..row(name, Some(product), Some(false))
    }
}

/// A `signal` column in the short form (`−62 dBm 3 min`, `—` for a device with none) shows once
/// any row has a signal, between the product and the state, and goes first when the list is too
/// narrow for it; a list no row of which has a signal has no such column.
#[test]
fn devices_list_shows_signal_and_drops_it_first() {
    crate::tui::widgets::graph::hold_wall_ms(WALL);
    let rows = vec![
        heard("bifrost", "Bifrost 3", -62, 180),
        row("gungnir", Some("Gungnir 2"), Some(true)),
    ];
    let lines = text(&frame(&app(rows.clone()), 80, 24));
    assert_eq!(
        row_with_in(&lines, "Bifrost 3"),
        "│ ▶ bifrost   Bifrost 3   \u{2212}62 dBm 3 min   disconnected                         │"
    );
    assert!(
        row_with_in(&lines, "Gungnir 2")
            .contains(&format!("Gungnir 2   \u{2014}{}connected", " ".repeat(15))),
        "{}",
        row_with_in(&lines, "Gungnir 2")
    );
    // A name and a state that leave no room for the column on the narrow tier push it out,
    // and the wide tier has room for it again.
    let waiting = |name: &str| DeviceRow {
        retry_in_secs: Some(15),
        ..heard(name, "Bifrost 3", -62, 180)
    };
    let rows = vec![waiting("bifrost-living-room")];
    let narrow = text(&frame(&app(rows.clone()), 80, 24));
    assert!(narrow.iter().all(|row| !row.contains("dBm")), "{narrow:#?}");
    assert!(
        row_with_in(&narrow, "Bifrost 3").contains("disconnected, next attempt in 15 s"),
        "{narrow:#?}"
    );
    let wide = text(&frame(&app(rows), 120, 24));
    assert!(
        row_with_in(&wide, "Bifrost 3").contains("\u{2212}62 dBm 3 min"),
        "{wide:#?}"
    );
    let none = text(&frame(&app(daemon_rows()), 80, 24));
    assert!(none.iter().all(|row| !row.contains("dBm")));
}

/// A link the device refused the passphrase on, and readings gone stale, are said in the state
/// column, in the words the Status pane and the web page use too.
#[test]
fn refused_and_stale_are_said() {
    let rows = vec![
        DeviceRow {
            refused: true,
            ..row("bifrost", Some("Bifrost 3"), Some(true))
        },
        DeviceRow {
            stale: true,
            ..row("gungnir", Some("Gungnir 2"), Some(false))
        },
    ];
    let lines = text(&frame(&app(rows), 80, 24));
    assert!(
        row_with_in(&lines, "Bifrost 3").contains("connected \u{b7} passphrase refused"),
        "{}",
        row_with_in(&lines, "Bifrost 3")
    );
    assert!(
        row_with_in(&lines, "Gungnir 2").contains("disconnected \u{b7} readings stale"),
        "{}",
        row_with_in(&lines, "Gungnir 2")
    );
}

/// The list's columns are laid out in display cells: a device named in wide glyphs takes two
/// cells for each, so its product starts in the same screen column as the others'. (`text`
/// gives one string per screen cell, so a character count is a column.)
#[test]
fn the_columns_of_the_list_align_under_wide_device_names() {
    let rows = vec![
        row("世界", Some("Bifrost 3"), Some(true)),
        row("gungnir", Some("Gungnir 2"), Some(false)),
    ];
    let mut app = app(rows);
    app.devices.configured[0].name = "世界".into();
    let lines = text(&frame(&app, 80, 24));
    let column = |wanted: &str| {
        let line = row_with_in(&lines, wanted);
        let before = line.split(wanted).next().expect("the row carries the text");
        before.chars().count()
    };
    assert_eq!(column("Bifrost 3"), column("Gungnir 2"), "{lines:#?}");
}

/// The editor is drawn over the list it is about: one row for each of the four things a
/// user changes, the cursor on the row the keys are on, and the keys that end it.
#[test]
fn the_editor_draws_the_device_over_the_list_it_is_about() {
    assert_rows(
        &frame(&editing(daemon_rows()), 80, 24),
        &[
            " svolder  Gungnir 2  ● connected                                  mouse:on 14:02",
            "─ Controls ── EQ ── Telemetry ──┤ Devices ├───────────────── ? help ─ esc back ─",
            "┌ Devices ────────────────────────────────── new ─ update ─ x remove ─ refresh ┐",
            "│ ▶ bifrost   Bifrost 3   connected                                            │",
            "│   gungnir   Gungnir 2   disconnected (current)                               │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "│           ┌ Edit bifrost ────────────────────────────────────────┐           │",
            "│           │ ▶ Name        bifrost                                │           │",
            "│           │   Passphrase  ‹ auto ›                               │           │",
            "│           │   Profile     ‹ automatic ›                          │           │",
            "│           │   Default     [ ] no                                 │           │",
            "│           │                                                      │           │",
            "│           │ Enter saves · Esc cancels                            │           │",
            "│           └──────────────────────────────────────────────────────┘           │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "└──────────────────────────────────────────────────────────────────────────────┘",
        ],
    );
}

/// The Passphrase row is drawn between its arrows in each of its three states, the code with a
/// `_` for each digit still to come, and with the focus on it the terminal's own cursor is not
/// placed in it: a code is typed in order, so there is no caret to show.
#[test]
fn the_editor_draws_the_passphrase_row_in_each_of_its_states() {
    let mut app = app(daemon_rows());
    app.key(KeyEvent::from(KeyCode::Char('u')));
    app.key(KeyEvent::from(KeyCode::Down));
    let theme = Theme::new(false);
    let mut shown = Vec::new();
    for key in [
        KeyCode::Right,
        KeyCode::Char('4'),
        KeyCode::Char('2'),
        KeyCode::Left,
        KeyCode::Left,
    ] {
        app.key(KeyEvent::from(key));
        let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal");
        let mut hits = HitMap::default();
        terminal
            .draw(|f| {
                render::draw(f, &app, &theme, &mut hits);
            })
            .expect("the frame draws");
        shown.push(text(terminal.backend().buffer())[11].clone());
    }
    // From auto: the code, with a digit typed and another, then back to auto, then to none.
    assert_eq!(
        shown,
        [
            "│           │ ▶ Passphrase  ‹ ____ ›                               │           │",
            "│           │ ▶ Passphrase  ‹ 4___ ›                               │           │",
            "│           │ ▶ Passphrase  ‹ 42__ ›                               │           │",
            "│           │ ▶ Passphrase  ‹ auto ›                               │           │",
            "│           │ ▶ Passphrase  ‹ none ›                               │           │",
        ]
    );
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, &app, &theme, &mut hits);
        })
        .expect("the frame draws");
    let cursor = terminal.get_cursor_position().expect("the cursor");
    assert_ne!(
        cursor.y, 11,
        "the terminal's cursor was placed in a row that has no caret"
    );
}

/// A device name longer than the editor's row scrolls through it with the cursor, which
/// stays inside the box; with the focus on another row the name shows from its start.
#[test]
fn a_long_device_name_scrolls_inside_the_editor() {
    let long = "the Gungnir in the living room on the left of the big shelf";
    let mut app = app(daemon_rows());
    app.devices.configured[0].name = long.into();
    app.devices.rows[0].name = long.into();
    app.key(KeyEvent::from(KeyCode::Char('u')));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal");
    let mut hits = HitMap::default();
    let theme = Theme::new(false);
    terminal
        .draw(|f| {
            render::draw(f, &app, &theme, &mut hits);
        })
        .expect("the frame draws");
    let rows = text(terminal.backend().buffer());
    assert_eq!(
        rows[10],
        "│           │ ▶ Name        …ng room on the left of the big shelf  │           │"
    );
    let cursor = terminal.get_cursor_position().expect("the cursor");
    assert_eq!((cursor.x, cursor.y), (65, 10), "the cursor left the field");

    app.key(KeyEvent::from(KeyCode::Down));
    let rows = text(&frame(&app, 80, 24));
    assert_eq!(
        rows[10],
        "│           │   Name        the Gungnir in the living room on the… │           │"
    );
}

/// `x` asks before anything is written, over the list, naming the device it is about.
#[test]
fn the_removal_asks_about_the_device_it_would_take_away() {
    let mut app = app(daemon_rows());
    app.key(KeyEvent::from(KeyCode::Char('x')));
    assert_rows(
        &frame(&app, 80, 24),
        &[
            " svolder  Gungnir 2  ● connected                                  mouse:on 14:02",
            "─ Controls ── EQ ── Telemetry ──┤ Devices ├───────────────── ? help ─ esc back ─",
            "┌ Devices ────────────────────────────────── new ─ update ─ x remove ─ refresh ┐",
            "│ ▶ bifrost   Bifrost 3   connected                                            │",
            "│   gungnir   Gungnir 2   disconnected (current)                               │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "│           ┌ Remove ──────────────────────────────────────────────┐           │",
            "│           │ Remove bifrost from the configuration?               │           │",
            "│           │                                                      │           │",
            "│           │ Enter removes · Esc cancels                          │           │",
            "│           └──────────────────────────────────────────────────────┘           │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "│                                                                              │",
            "└──────────────────────────────────────────────────────────────────────────────┘",
        ],
    );
}

/// The editor and the question a removal asks are the one thing in focus, so each is drawn
/// in the theme's own colours — the border and the title as a focused pane's, the keys that
/// end it in the accent with what they do in the text colour — and nothing inside the box
/// carries the grey everything around it is dimmed to.
#[test]
fn the_editor_and_the_removal_are_drawn_in_the_themes_colours() {
    use crate::tui::test_support::{assert_drawn_in_focus, modal_themes};

    for (depth, theme) in modal_themes() {
        let mut removing = app(daemon_rows());
        removing.key(KeyEvent::from(KeyCode::Char('x')));
        for (what, app) in [("editor", editing(daemon_rows())), ("removal", removing)] {
            let label = format!("{what} at {depth}");
            let buffer = frame_with_theme(&app, 80, 24, &theme);
            let mut hits = HitMap::default();
            let mut terminal =
                Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
            let mut areas = render::FrameAreas::default();
            terminal
                .draw(|f| areas = render::draw(f, &app, &theme, &mut hits))
                .expect("the frame draws");
            let area = areas.modal.expect("the box is a modal");
            assert_drawn_in_focus(&buffer, area, &theme, &label);
            let keys = text(&buffer)
                .iter()
                .position(|row| row.contains("Esc cancels"))
                .expect("the box names its keys");
            let row = &text(&buffer)[keys];
            let enter = u16::try_from(row.find("Enter").map_or(0, |at| row[..at].chars().count()))
                .unwrap_or(0);
            let y = u16::try_from(keys).unwrap_or(0);
            assert_eq!(
                Some(buffer.cell((enter, y)).expect("the Enter label").fg),
                theme.style(Role::KeyHint).fg,
                "{label}: Enter is not in the accent"
            );
            assert_eq!(
                Some(buffer.cell((enter + 7, y)).expect("what Enter does").fg),
                theme.style(Role::Text).fg,
                "{label}: what Enter does is not in the text colour"
            );
        }
    }
}

#[test]
fn each_row_pushes_a_list_row_hit_and_the_title_leads_back_to_the_device_screen() {
    let a = app(daemon_rows());
    let mut hits = HitMap::default();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    terminal
        .draw(|f| {
            render::draw(f, &a, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");

    assert_eq!(hits.hit_at(2, 2), Some(&Hit::Tab(Screen::Device)));
    assert_eq!(hits.hit_at(4, 3), Some(&Hit::ListRow(0)), "the bifrost row");
    assert_eq!(hits.hit_at(4, 4), Some(&Hit::ListRow(1)), "the gungnir row");
}

#[test]
fn a_device_no_daemon_holds_draws_a_dash_for_what_is_not_known() {
    let rows = vec![
        row("bifrost", None, None),
        row("gungnir", Some("Gungnir 2"), None),
    ];
    let buffer = frame(&app(rows), 80, 24);
    assert_eq!(
        row_with(&buffer, "bifrost"),
        "│ ▶ bifrost   -           -                                                    │"
    );
    assert_eq!(
        row_with(&buffer, "gungnir"),
        "│   gungnir   Gungnir 2   - (current)                                          │"
    );
}

/// A device the daemon is waiting to try again for says when it will, in the words
/// `svolder daemon status` uses, a spinner turning in front of them, and in the colour of a
/// link that is down.
#[test]
fn a_device_the_daemon_is_waiting_on_says_when_it_tries_again() {
    let mut rows = daemon_rows();
    rows[1].retry_in_secs = Some(15);
    let mut app = app(rows);
    let buffer = frame(&app, 80, 24);
    assert_eq!(
        row_with(&buffer, "gungnir"),
        "│   gungnir   Gungnir 2   ⠋ disconnected, next attempt in 15 s (current)       │"
    );
    assert!(
        buffer
            .cell((50, 4))
            .expect("a cell of the wait")
            .modifier
            .contains(Modifier::DIM),
        "the wait is not drawn in Role::Disconnected"
    );

    // The daemon stopped saying when it will try again — a direct session's list carries
    // this too — so there is no wait left to turn a spinner in front of.
    app.devices.rows[1].retry_in_secs = None;
    let back = row_with(&frame(&app, 80, 24), "gungnir");
    assert!(
        !back.contains('⠋') && back.contains("disconnected"),
        "the spinner outlived the wait it was turning for: |{back}|"
    );
}

#[test]
fn the_state_column_carries_the_colour_of_the_link_it_reports() {
    let theme = Theme::new(false);
    let buffer = frame(&app(daemon_rows()), 80, 24);
    // Column 26 is where every row's state starts; row 3 is the connected device, which
    // is also the focused one, and row 4 the disconnected one.
    assert_eq!(
        buffer.cell((26, 3)).expect("the connected state cell").fg,
        Color::Green,
        "a live link is not drawn in Role::Connected"
    );
    assert_eq!(
        buffer.cell((4, 3)).expect("the focused row's name cell").fg,
        theme.accent(),
        "the focused row is not drawn in Role::Selected"
    );
    assert!(
        buffer
            .cell((26, 4))
            .expect("the disconnected state cell")
            .modifier
            .contains(Modifier::DIM),
        "a lost link is not drawn in Role::Disconnected"
    );
}

/// A focused row is extended by one blank cell in the selected style right after its last
/// character, so the highlight does not stop flush against it: on the Devices list, whose
/// focused row already names the current device with `(current)`, and on the device editor
/// drawn over it, whose focused Name row is typed into. The cell after that one stays
/// plain on both.
#[test]
fn a_focused_row_gets_one_highlighted_cell_past_its_last_character() {
    let theme = Theme::new(false);
    let selected = theme.style(Role::Selected);
    let check = |buffer: &Buffer, y: u16, borders_from_right: usize, what: &str| {
        let row: Vec<_> = (0..buffer.area.width)
            .map(|x| buffer.cell((x, y)).expect("a cell").clone())
            .collect();
        // The row's own right border — the outer one for the list, the editor box's own,
        // one border in from that, for the editor drawn over it — so the search for its
        // last character stops there rather than finding a border glyph itself.
        let borders: Vec<usize> = row
            .iter()
            .enumerate()
            .filter(|(_, cell)| cell.symbol() == "│")
            .map(|(at, _)| at)
            .collect();
        let border = borders
            .get(borders.len().wrapping_sub(1 + borders_from_right))
            .copied()
            .unwrap_or_else(|| panic!("{what}: no such border on the row: {borders:?}"));
        let cells = &row[..border];
        let last = cells
            .iter()
            .rposition(|cell| !cell.symbol().trim().is_empty())
            .unwrap_or_else(|| panic!("{what}: the row has a last character"));
        let after = &cells[last + 1];
        assert_eq!(after.symbol(), " ", "{what}: not a blank cell after it");
        assert_eq!(
            Some(after.fg),
            selected.fg,
            "{what}: the cell right after it is not selected"
        );
        assert!(
            after.modifier.contains(selected.add_modifier),
            "{what}: the cell right after it is not selected"
        );
        let next = &cells[last + 2];
        assert_ne!(
            Some(next.fg),
            selected.fg,
            "{what}: the highlight ran two cells past it"
        );
    };

    // Row 3, "bifrost", is the one the list opens on.
    check(
        &frame(&app(daemon_rows()), 80, 24),
        3,
        0,
        "the Devices list",
    );

    // Row 10, Name, is the editor row it opens on.
    check(
        &frame(&editing(daemon_rows()), 80, 24),
        10,
        1,
        "the device editor",
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
            let buffer = frame_with_theme(&app(daemon_rows()), 80, 24, &theme);

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

/// With no key row the list runs down to the terminal's last row: 22 rows at 80×24.
#[test]
fn the_list_runs_down_to_the_last_row_at_80_by_24() {
    let frames = super::frames_of(ratatui::layout::Rect::new(0, 0, 80, 24), &app(Vec::new()));
    assert_eq!(frames.len(), 1);
    let (_, list) = frames[0];
    assert_eq!((list.y, list.height), (2, 22));
}

/// An app on the device list with `count` daemon-held devices named `dev00`, `dev01`…, the
/// focus on `focused`.
fn crowded(count: usize, focused: usize) -> App {
    let rows = (0..count)
        .map(|at| row(&format!("dev{at:02}"), Some("Gungnir 2"), Some(true)))
        .collect();
    let mut a = app(rows);
    a.selected = focused;
    a
}

/// Draw `a` on an 80x24 frame, answering the text of every row and the hits it pushed.
fn drawn(a: &App) -> (Vec<String>, HitMap) {
    let mut hits = HitMap::default();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let done = terminal
        .draw(|f| {
            render::draw(f, a, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    (text(done.buffer), hits)
}

#[test]
fn a_long_device_list_scrolls_to_keep_the_focused_device_in_view() {
    let (rows, _) = drawn(&crowded(21, 20));
    assert!(rows.iter().any(|row| row.contains("dev20")), "{rows:#?}");
    assert!(!rows.iter().any(|row| row.contains("dev00")), "{rows:#?}");
    assert!(rows[2].contains("more ↑"), "|{}|", rows[2]);
    assert!(!rows[23].contains("more ↓"));

    let (rows, _) = drawn(&crowded(21, 0));
    assert!(rows.iter().any(|row| row.contains("dev00")));
    assert!(!rows.iter().any(|row| row.contains("dev20")));
    assert!(!rows[2].contains("more ↑"), "|{}|", rows[2]);
    assert!(rows[23].contains("more ↓"), "|{}|", rows[23]);

    let (rows, _) = drawn(&crowded(40, 25));
    assert!(rows[2].contains("more ↑") && rows[23].contains("more ↓"));
    assert!(rows.iter().any(|row| row.contains("dev25")));
}

#[test]
fn a_list_that_fits_marks_no_border() {
    let (rows, _) = drawn(&crowded(5, 4));
    assert!(!rows.iter().any(|row| row.contains("more ")));
}

#[test]
fn hits_of_a_scrolled_device_list_follow_the_rows_drawn() {
    let (rows, hits) = drawn(&crowded(21, 20));
    // Rows 3..=22 of the frame are the list's inside: 20 rows, dev01 to dev20.
    for y in 3..23u16 {
        let drawn_name: String = rows[usize::from(y)].chars().skip(4).take(5).collect();
        let Some(Hit::ListRow(index)) = hits.hit_at(4, y) else {
            panic!("no list row at {y}");
        };
        assert_eq!(drawn_name, format!("dev{index:02}"), "row {y}");
    }
    assert_eq!(hits.hit_at(4, 22), Some(&Hit::ListRow(20)));
}

#[test]
fn the_more_mark_on_the_border_is_not_a_click_target() {
    let (rows, hits) = drawn(&crowded(21, 20));
    let column = rows[2]
        .chars()
        .position(|c| c == '↑')
        .expect("the mark is drawn");
    let column = u16::try_from(column).expect("a column");
    assert_eq!(hits.hit_at(column, 2), None);
    assert_eq!(
        hits.hit_at(3, 2),
        Some(&Hit::Tab(Screen::Device)),
        "the title"
    );
    assert_eq!(
        hits.hit_at(40, 2),
        Some(&Hit::Tab(Screen::Device)),
        "the border beside the mark"
    );
}

#[test]
fn a_list_one_row_over_the_window_scrolls_by_one_when_focus_passes_it() {
    // The list's inside is 20 rows tall at 80x24.
    let (rows, _) = drawn(&crowded(21, 19));
    assert!(rows.iter().any(|row| row.contains("dev00")));
    assert!(!rows[2].contains("more ↑"), "|{}|", rows[2]);
    assert!(rows[23].contains("more ↓"), "|{}|", rows[23]);
    let (rows, _) = drawn(&crowded(21, 20));
    assert!(!rows.iter().any(|row| row.contains("dev00")));
    assert!(rows[2].contains("more ↑"), "|{}|", rows[2]);
}

/// The mockup docs/usage/tui.md shows of the device list is the frame this build draws.
#[test]
fn the_docs_show_the_device_list_this_build_draws() {
    crate::tui::test_support::assert_usage_md_shows(
        &text(&frame(&app(daemon_rows()), 80, 24)),
        "device list",
    );
}

/// The rows of a device list of forty fade by their distance from the focused one, a window
/// scrolled off the top counting from the focused index; nothing fades with the switch off,
/// below true colour or in monochrome.
#[test]
fn a_long_device_list_fades_with_the_distance_from_the_focus() {
    let rows: Vec<DeviceRow> = (0..40)
        .map(|n| row(&format!("dev{n:02}"), Some("Gungnir 2"), Some(false)))
        .collect();
    let theme = theme_for("amber", ColorDepth::TrueColor);
    for selected in [0, 33] {
        let mut app = app(rows.clone());
        app.selected = selected;
        let plain = frame_with_theme(&app, 80, 24, &theme);
        app.list_fade = true;
        let faded = frame_with_theme(&app, 80, 24, &theme);
        let top = crate::tui::widgets::scroll::scroll_start(40, 20, selected);
        let lines: Vec<(u16, usize)> = (0..20)
            .map(|at| (3 + at as u16, (top + at).abs_diff(selected)))
            .collect();
        crate::tui::test_support::assert_list_faded(&plain, &faded, &lines, 1..79, &theme);
    }

    let mut app = app(rows);
    for theme in [
        theme_for("amber", ColorDepth::Ansi256),
        theme_for("amber", ColorDepth::Ansi16),
        Theme::from_config(&TuiConfig::default(), true, ColorDepth::TrueColor),
    ] {
        app.list_fade = false;
        let off = frame_with_theme(&app, 80, 24, &theme);
        app.list_fade = true;
        assert_eq!(off, frame_with_theme(&app, 80, 24, &theme));
    }
    app.list_fade = false;
    let truecolor = theme_for("amber", ColorDepth::TrueColor);
    let off = frame_with_theme(&app, 80, 24, &truecolor);
    app.list_fade = true;
    assert_ne!(off, frame_with_theme(&app, 80, 24, &truecolor));
}

/// On the focused row, where reverse video is the selection, the `(current)` badge drops the
/// reverse (and keeps its bold): it reads as a hole in the band, in monochrome and at 16
/// colours alike, where reverse on reverse would be no mark at all. On another row it is the
/// inverse video it is everywhere else.
#[test]
fn the_current_badge_stays_distinct_on_the_focused_row() {
    for monochrome in [true, false] {
        let theme = Theme::from_config(&TuiConfig::default(), monochrome, ColorDepth::Ansi16);
        for (focused, badge_reversed) in [(1, false), (0, true)] {
            let mut app = app(daemon_rows());
            app.selected = focused;
            let buffer = frame_with_theme(&app, 80, 24, &theme);
            let y = text(&buffer)
                .iter()
                .position(|row| row.contains("(current)"))
                .expect("the current device's row") as u16;
            let cells: Vec<_> = (0..80)
                .map(|x| buffer.cell((x, y)).unwrap().clone())
                .collect();
            let at = cells
                .iter()
                .position(|cell| cell.symbol() == "(")
                .expect("the badge");
            let reversed = |x: usize| cells[x].modifier.contains(Modifier::REVERSED);
            for (x, cell) in cells.iter().enumerate().skip(at).take("(current)".len()) {
                assert_eq!(
                    reversed(x),
                    badge_reversed,
                    "mono={monochrome} focused={focused}: badge cell {x}"
                );
                assert!(cell.modifier.contains(Modifier::BOLD), "the badge is bold");
            }
            if focused == 1 {
                assert!(reversed(at - 2), "the row's band runs up to the badge");
            }
        }
    }
}

/// With nothing configured the list says so and names the key that adds a device, rather than
/// drawing an empty frame.
#[test]
fn an_empty_device_list_says_nothing_is_configured() {
    let mut app = app(vec![]);
    app.session.device_name = String::new();
    let rows = text(&frame(&app, 80, 24));
    assert!(
        rows[3].starts_with("│   No devices configured - n adds one"),
        "{:?}",
        rows[3]
    );
    assert!(rows[4].trim_matches(|c| c == '│' || c == ' ').is_empty());
}

/// A device that is open with no entry in the configuration (the fake transport's) is listed
/// under the message as a row that says it is not configured.
#[test]
fn an_open_device_with_no_entry_is_listed_as_not_configured() {
    let rows = text(&frame(&app(vec![]), 80, 24));
    assert!(
        rows[3].starts_with("│   No devices configured - n adds one"),
        "{:?}",
        rows[3]
    );
    assert!(
        rows[4].starts_with("│   gungnir   not configured (current)"),
        "{:?}",
        rows[4]
    );
}

/// An open device the configuration does not name is listed after the configured ones, whether
/// or not any are configured.
#[test]
fn an_open_device_missing_from_a_non_empty_list_follows_the_rows() {
    let mut app = app(vec![row("bifrost", Some("Bifrost 3"), Some(true))]);
    app.session.device_name = "scratch".into();
    let rows = text(&frame(&app, 80, 24));
    assert!(rows[3].starts_with("│ ▶ bifrost"), "{:?}", rows[3]);
    assert!(
        rows[4].starts_with("│   scratch   not configured (current)"),
        "{:?}",
        rows[4]
    );
}

/// The panel the failure is told in keeps a cell of padding between its frame and its words, as
/// every other frame does, and is exactly as tall as the words it wraps to: the height counted
/// and the drawing wrap at the same width.
#[test]
fn the_not_opened_panel_pads_its_words_and_is_as_tall_as_they_are() {
    use crate::tui::app::Holding;

    let mut a = app(daemon_rows());
    let what = "no daemon answers on /home/user/run/svolder.sock; nothing was started for this run";
    a.session.holding = Some(Holding::Nothing {
        why: Some((what.into(), "Run `svolder daemon install`".into())),
    });
    let (rows, _) = drawn(&a);
    let top = rows
        .iter()
        .position(|row| row.contains(" Not opened "))
        .expect("the panel is drawn");
    for row in &rows[top + 1..top + 4] {
        assert!(
            row.starts_with("│ ") && !row[4..].starts_with(' '),
            "{row:?}"
        );
    }
    assert!(
        rows[top + 1].contains("no daemon answers"),
        "{:?}",
        rows[top + 1]
    );
    assert!(
        rows[top + 4].starts_with('└') || rows[top + 4].starts_with('╰'),
        "no blank row before the bottom border: {:?}",
        rows[top + 4]
    );
}
