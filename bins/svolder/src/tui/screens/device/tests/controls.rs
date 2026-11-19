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
    app.set_selected(1);
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
    hear(
        &mut app,
        vec![converted("10", "volume", 63, "63 (-26.1 dB)", -26.1, "dB")],
    );
    app.settle();
    let toggle = app
        .rows()
        .iter()
        .position(|row| row.control.kind == svolder_profiles::ControlKind::Toggle)
        .expect("a toggle row");
    app.set_selected(toggle);
    let label = app.rows()[toggle].control.label.clone();
    let buffer = frame_with_theme(&app, 80, 24, &theme);
    let cells = row_cells(&buffer, &label);
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
        (&app, "Volume", 39),
        (&app, "Balance", 39),
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
        {
            let (selection, label) = (4, "Volume");
            app.set_selected(selection);
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

/// The Volume row carries the switch between the device's volume and the computer's on a line of
/// its own over the bar, where the pane has room; a click on either name shows that side, and the
/// row is the device's own where the computer's controls are not there.
#[test]
fn the_volume_row_carries_the_switch() {
    let mut app = mockup_app();
    let wide = |app: &App| text(&frame(app, 130, 30));
    assert!(
        wide(&app).iter().all(|row| !row.contains("Computer")),
        "no switch yet"
    );

    app.host.computer.eq = Some(crate::tui::test_support::computer_eq(
        false,
        &[("PK", 1000.0, 0.0, 1.0)],
    ));
    let name = app.session.product.clone();
    let switch = format!("Target ‹ {name} | Computer ›");
    let rows = wide(&app);
    let at = rows
        .iter()
        .position(|row| row.contains(&switch))
        .unwrap_or_else(|| panic!("{switch}: {rows:#?}"));
    assert!(
        rows[at + 1].contains("█109/127"),
        "the bar is under it: {:?}",
        rows[at + 1]
    );
    assert!(
        text(&screen(&app))
            .iter()
            .any(|row| row.contains("| Computer ›")),
        "the device's name gives way to it at 80 columns"
    );

    let mut terminal = Terminal::new(TestBackend::new(130, 30)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, &app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    let rows = text(terminal.backend().buffer());
    let y = rows
        .iter()
        .position(|row| row.contains("| Computer"))
        .expect("a row");
    let byte = rows[y].find("Computer").expect("the switch");
    let x = u16::try_from(rows[y][..byte].chars().count()).unwrap();
    assert_eq!(
        hits.hit_at(x, u16::try_from(y).unwrap()),
        Some(&Hit::Side(crate::tui::app::Side::Computer))
    );

    app.flip_side();
    assert!(
        wide(&app).iter().any(|row| row.contains(&switch)),
        "the switch stays on the other side"
    );
}

/// [`mockup_app`] with the computer's controls there, the keys on the Volume row.
fn switch_app() -> App {
    let mut app = mockup_app();
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq(
        false,
        &[("PK", 1000.0, 0.0, 1.0)],
    ));
    let volume = app.volume().expect("a volume").id;
    app.set_selected(
        app.rows()
            .iter()
            .position(|row| row.control.id == volume)
            .expect("the volume row"),
    );
    app
}

fn press(app: &mut App, code: KeyCode) -> Vec<crate::tui::app::Intent> {
    app.key(crossterm::event::KeyEvent::from(code))
}

/// `↑` from the Volume row reaches the switch row, `←→` there pick the side and write nothing,
/// and `↓` goes back to the volume, whose `←→` change only the volume.
#[test]
fn the_switch_is_a_row_above_volume_whose_arrows_pick_the_side() {
    let mut app = switch_app();
    assert!(!app.switch_focused());
    // on the volume, the arrows step it and never flip the side
    assert!(!press(&mut app, KeyCode::Left).is_empty());
    assert!(!app.on_computer());
    assert!(press(&mut app, KeyCode::Up).is_empty());
    assert!(app.switch_focused());
    assert!(press(&mut app, KeyCode::Right).is_empty());
    assert!(app.on_computer());
    assert!(press(&mut app, KeyCode::Right).is_empty());
    assert!(app.on_computer(), "→ is the computer, it does not toggle");
    assert!(press(&mut app, KeyCode::Left).is_empty());
    assert!(!app.on_computer());
    assert!(app.switch_focused());
    press(&mut app, KeyCode::Down);
    assert!(!app.switch_focused());
    assert!(
        !press(&mut app, KeyCode::Right).is_empty(),
        "the volume steps"
    );
    assert!(!app.on_computer());
    // `C` flips from wherever the keys are
    assert!(press(&mut app, KeyCode::Char('C')).is_empty());
    assert!(app.on_computer());
}

/// `↓` from the row above Volume lands on the switch first, a further `↓` on Volume; `↑` from
/// Volume reaches the switch and a further `↑` the row above, so no row is skipped either way.
#[test]
fn the_switch_is_reached_going_down_as_well_as_up() {
    let mut app = switch_app();
    let volume = app.selected();
    assert!(volume > 0, "a row above Volume");
    app.set_selected(volume - 1);
    assert!(!app.switch_focused());
    press(&mut app, KeyCode::Down);
    assert!(app.switch_focused(), "↓ from the row above lands on Target");
    assert_eq!(app.selected(), volume);
    press(&mut app, KeyCode::Down);
    assert!(!app.switch_focused());
    assert_eq!(app.selected(), volume);
    press(&mut app, KeyCode::Up);
    assert!(app.switch_focused());
    press(&mut app, KeyCode::Up);
    assert!(!app.switch_focused());
    assert_eq!(app.selected(), volume - 1);
}

/// The switch has a cursor of its own: moving onto it from Volume and off it again each name the
/// row the cursor left, so the trail plays on it as on any row.
#[test]
fn the_switch_has_its_own_cursor_and_earns_the_trail() {
    let mut app = switch_app();
    let volume = app.cursor_id().expect("Volume has a cursor");
    press(&mut app, KeyCode::Up);
    let switch = app.cursor_id().expect("the switch has a cursor");
    assert_ne!(switch, volume);
    assert_eq!(app.frame_flags.left_row, Some(volume), "left Volume");
    app.frame_flags.left_row = None;
    press(&mut app, KeyCode::Down);
    assert_eq!(app.frame_flags.left_row, Some(switch), "left the switch");
    assert_eq!(app.cursor_id(), Some(volume));
    // the frame draws the switch's own line as a row the trail can play over
    press(&mut app, KeyCode::Up);
    let mut terminal = Terminal::new(TestBackend::new(130, 30)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut areas = None;
    terminal
        .draw(|f| {
            areas = Some(render::draw(f, &app, &Theme::new(false), &mut hits));
        })
        .expect("the frame draws");
    let areas = areas.expect("areas");
    let at = areas
        .rows
        .iter()
        .find(|(id, _)| *id == switch)
        .expect("the switch is a drawn row")
        .1;
    let volume_at = areas
        .rows
        .iter()
        .find(|(id, _)| *id == volume)
        .expect("Volume is a drawn row")
        .1;
    assert_eq!(at.height, 1);
    assert!(at.y < volume_at.y, "{at:?} over {volume_at:?}");
}

/// Which side the Target line shows stands out: unselected, the side shown is bold and the other
/// dim; selected, the side shown is the highlight box and the other plain dim with no box, in
/// colour and in monochrome alike.
#[test]
fn the_selected_target_line_boxes_the_side_shown() {
    use crate::tui::test_support::target_sides;
    use ratatui::style::Modifier;
    for mono in [false, true] {
        let theme = Theme::new(mono);
        let mut app = switch_app();
        let name = app.session.product.clone();
        let shot = |app: &App| target_sides(&frame_with_theme(app, 130, 30, &theme), &name);

        let (line, dac, computer) = shot(&app);
        eprintln!("mono={mono} unselected, device : {line}");
        assert!(dac.0.contains(Modifier::BOLD) && !dac.0.contains(Modifier::REVERSED));
        assert!(!computer.0.contains(Modifier::BOLD));
        assert_ne!(dac, computer);

        press(&mut app, KeyCode::Up);
        let (line, dac, computer) = shot(&app);
        eprintln!("mono={mono} selected,   device : {line}");
        assert!(
            dac.0.contains(Modifier::BOLD | Modifier::REVERSED),
            "{dac:?}"
        );
        assert!(
            !computer.0.contains(Modifier::REVERSED),
            "no box: {computer:?}"
        );
        assert!(!computer.0.contains(Modifier::BOLD));
        assert_ne!(dac, computer);

        press(&mut app, KeyCode::Right);
        let (line, dac, computer) = shot(&app);
        eprintln!("mono={mono} selected,   computer: {line}");
        assert!(
            computer.0.contains(Modifier::BOLD | Modifier::REVERSED),
            "{computer:?}"
        );
        assert!(!dac.0.contains(Modifier::REVERSED), "no box: {dac:?}");

        press(&mut app, KeyCode::Down);
        let (line, _, computer) = shot(&app);
        eprintln!("mono={mono} unselected, computer: {line}");
        assert!(computer.0.contains(Modifier::BOLD) && !computer.0.contains(Modifier::REVERSED));
    }
}

/// The focused switch row is the one drawn with the cursor, the Volume row is not, and the label
/// is `Target` over the bar on both sides.
#[test]
fn the_focused_switch_row_wears_the_cursor() {
    let mut app = switch_app();
    let on_cursor = |app: &App| {
        text(&frame(app, 130, 30))
            .iter()
            .any(|row| row.contains("▶ Target ‹ "))
    };
    assert!(!on_cursor(&app));
    press(&mut app, KeyCode::Up);
    assert!(on_cursor(&app));
    assert!(
        text(&frame(&app, 130, 30))
            .iter()
            .all(|row| !row.contains("▶ Volume")),
        "the Volume row is not focused while the switch is"
    );
    press(&mut app, KeyCode::Right);
    assert!(on_cursor(&app), "the keys stay on the switch");
    press(&mut app, KeyCode::Down);
    assert!(!on_cursor(&app));
}

/// Where the computer's controls cannot be used the reason stands in the switch's place, and no
/// key focuses it.
#[test]
fn the_unavailable_reason_is_not_focusable() {
    let mut app = switch_app();
    app.host.computer.eq.as_mut().expect("an eq").unavailable =
        Some("computer controls unavailable: no pw-cli on this computer".into());
    let rows = text(&frame(&app, 130, 30));
    assert!(
        rows.iter()
            .any(|row| row.contains("Target") && row.contains("unavailable")),
        "{rows:?}"
    );
    let before = app.selected();
    press(&mut app, KeyCode::Up);
    assert!(!app.switch_focused());
    assert_ne!(app.selected(), before, "↑ moves to the row above instead");
}

/// At 80 columns the device's name is cut and the switch stays on its own line above the bar.
#[test]
fn the_switch_stays_off_the_volume_line_in_a_narrow_pane() {
    let mut app = switch_app();
    app.session.product = "A very long device name indeed".into();
    let rows = text(&screen(&app));
    let at = rows
        .iter()
        .position(|row| row.contains("Target ‹ ") && row.contains("| Computer ›"))
        .unwrap_or_else(|| panic!("{rows:#?}"));
    assert!(rows[at].contains('…'), "the name is cut: {:?}", rows[at]);
    assert!(!rows[at + 1].contains("Computer"));
    assert!(
        rows[at + 1].contains('█') || rows[at + 1].contains("109"),
        "{:?}",
        rows[at + 1]
    );
}

/// While the switch is focused only its own keys act: `+`, `-` and `m` do nothing, and a key of
/// the volume row still works once the focus is back on it.
#[test]
fn the_volume_keys_do_nothing_on_the_target_row() {
    let mut app = switch_app();
    press(&mut app, KeyCode::Up);
    assert!(app.switch_focused());
    for key in ['+', '=', '-', 'm'] {
        assert!(press(&mut app, KeyCode::Char(key)).is_empty(), "{key}");
    }
    assert!(app.switch_focused());
    press(&mut app, KeyCode::Down);
    assert!(!press(&mut app, KeyCode::Char('+')).is_empty());
}

/// A focus on the switch ends with anything that moves the focus: Tab, a click on a row, a
/// device change, and the controls going unusable — which a return of them does not undo.
#[test]
fn the_switch_focus_does_not_outlive_its_row() {
    let mut app = switch_app();
    press(&mut app, KeyCode::Up);
    assert!(app.switch_focused());
    press(&mut app, KeyCode::Tab);
    assert!(!app.switch_focused(), "Tab left the pane");

    let mut app = switch_app();
    press(&mut app, KeyCode::Up);
    app.focus(crate::tui::app::Pane::Controls, 0);
    assert!(!app.switch_focused());

    let mut app = switch_app();
    press(&mut app, KeyCode::Up);
    app.settle_on(crate::tui::app::Screen::Device);
    assert!(!app.switch_focused());

    let mut app = switch_app();
    press(&mut app, KeyCode::Up);
    app.host.computer.eq.as_mut().expect("an eq").unavailable = Some("no pw-cli".into());
    // the key finds the focus gone and steps the volume
    assert!(!press(&mut app, KeyCode::Left).is_empty());
    assert!(!app.on_computer());
    app.host.computer.eq.as_mut().expect("an eq").unavailable = None;
    assert!(!app.switch_focused(), "the focus does not come back");
}

/// A click on a name shows that side and focuses the switch, so `←→` go on from there; a click on
/// the line outside the names focuses it too.
#[test]
fn a_click_on_the_switch_focuses_it() {
    let mut app = switch_app();
    let mut terminal = Terminal::new(TestBackend::new(130, 30)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, &app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    let rows = text(terminal.backend().buffer());
    let y = u16::try_from(
        rows.iter()
            .position(|row| row.contains("Target ‹"))
            .expect("the switch"),
    )
    .unwrap();
    let col = |needle: &str| {
        let byte = rows[usize::from(y)].find(needle).expect(needle);
        u16::try_from(rows[usize::from(y)][..byte].chars().count()).unwrap()
    };
    let click = |x| crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: x,
        row: y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    // the label
    let action = crate::tui::mouse::resolve(&hits, click(col("Target")), &app);
    assert_eq!(action, Some(crate::tui::mouse::MouseAction::Target(None)));
    assert!(app.click_target(None).is_empty());
    assert!(app.switch_focused());
    // a name
    let action = crate::tui::mouse::resolve(&hits, click(col("Computer")), &app);
    assert_eq!(
        action,
        Some(crate::tui::mouse::MouseAction::Target(Some(
            crate::tui::app::Side::Computer
        )))
    );
    app.focus(crate::tui::app::Pane::Controls, 0);
    assert!(!app.switch_focused());
    assert!(
        app.click_target(Some(crate::tui::app::Side::Computer))
            .is_empty()
    );
    assert!(app.on_computer() && app.switch_focused());
    assert!(press(&mut app, KeyCode::Left).is_empty());
    assert!(!app.on_computer());
}

/// At the smallest size the switch is drawn on the Controls pane and the keys know it; a frame
/// that could not draw it leaves no row to be on.
#[test]
fn the_switch_fits_the_controls_pane_at_80_columns() {
    let app = switch_app();
    assert!(
        text(&screen(&app))
            .iter()
            .any(|row| row.contains("Target ‹ "))
    );
    assert!(app.drawn.controls_switch.get());
    assert!(crate::tui::screens::switch::fits(25));
    assert!(!crate::tui::screens::switch::fits(24));
    let mut app = switch_app();
    app.drawn.controls_switch.set(false);
    assert!(!app.switch_available());
    assert!(press(&mut app, KeyCode::Up).is_empty());
    assert!(!app.switch_focused());
}

/// A select row (Input) is a choice among values: with the keys on it only its label and its
/// value take the selected style, as the Target switch has it, and no band runs across the pane.
#[test]
fn a_focused_select_row_marks_its_label_and_value_not_the_row() {
    let theme = Theme::new(false);
    let selected = theme.style(Role::Selected);
    let mut app = mockup_app();
    app.set_selected(0);
    let buffer = frame(&app, 80, 24);
    let cells = controls_row_cells(&buffer, "Input");
    let is_selected = |cell: &ratatui::buffer::Cell| cell.modifier.contains(selected.add_modifier);
    let label = cells
        .iter()
        .position(|c| c.symbol() == "I")
        .expect("the label");
    assert!(is_selected(&cells[label]), "the label is not selected");
    let value = cells
        .iter()
        .position(|c| c.symbol() == "‹")
        .expect("the arrows")
        + 2;
    assert!(is_selected(&cells[value]), "the value is not selected");
    let last = last_character(&cells);
    assert!(
        !is_selected(&cells[last + 1]),
        "the select row is still a band"
    );
}
