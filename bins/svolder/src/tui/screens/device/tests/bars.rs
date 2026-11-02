// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The bars in the Controls pane: volume, balance and loudness, their raw values, colours and
//! red zone.

use super::*;

/// The Tone pane's Loudness bar is drawn as wide as the Controls pane's volume bar, from the
/// smallest screen to the wide and tall tiers, and a press reaches every cell of it.
#[test]
fn the_loudness_bar_is_as_wide_as_the_volume_bar() {
    let app = mockup_app();
    let theme = Theme::new(false);
    for (width, height) in [(80, 24), (100, 30), (120, 24), (160, 40)] {
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
        let mut hits = HitMap::default();
        terminal
            .draw(|f| {
                render::draw(f, &app, &theme, &mut hits);
            })
            .expect("the frame draws");
        let buffer = terminal.backend().buffer().clone();
        let volume = hits.bar_area(Hit::LevelBar(4)).expect("the volume bar");
        let loudness = hits.bar_area(Hit::LoudnessBar).expect("the loudness bar");
        assert!(volume.width > 5, "{width}x{height}: volume {volume:?}");
        assert_eq!(loudness.width, volume.width, "{width}x{height}");
        // The mockup's loudness reads 0: every cell of the bar is track, then the value.
        for x in loudness.x..loudness.right() {
            assert_eq!(
                buffer[(x, loudness.y)].symbol(),
                "░",
                "{width}x{height} at {x}"
            );
            assert_eq!(hits.hit_at(x, loudness.y), Some(&Hit::LoudnessBar));
        }
        assert_eq!(buffer[(loudness.right(), loudness.y)].symbol(), " ");
        assert_eq!(buffer[(loudness.right() + 1, loudness.y)].symbol(), "0");
    }
}

#[test]
fn a_wide_converted_value_keeps_its_bar_and_stays_inside_the_pane() {
    let mut app = mockup_app();
    hear(
        &mut app,
        vec![converted("10", "volume", 63, "63 (-26.1 dB)", -26.1, "dB")],
    );
    // Volume changed, so its bar is gliding there: this case is about the settled row.
    app.settle();
    let rows = text(&screen(&app));
    assert_eq!(
        rows[8],
        "│   Volume       █63/127┃░  -26.1 dB ││ ▶ 1   40.0 Hz    +0.0 dB   4.3         │"
    );
    assert_eq!(
        rows[10],
        "│   Balance      L ────┃──── R  0    ││   3    632 Hz    +0.0 dB   4.3         │",
        "the balance track is as wide as the bar"
    );
}

/// The balance marker is one glyph on one cell of a track drawn in `─`: a heavy bar,
/// which fills the cell it is on rather than leaving the colour behind it showing round a
/// round dot, and nothing else on the track is drawn with it.
#[test]
fn the_balance_marker_is_one_heavy_bar_on_a_light_track() {
    let cells = row_chars(&screen(&mockup_app()), "Balance");
    let track: Vec<char> = cells
        .iter()
        .copied()
        .skip_while(|c| *c != '─' && c.to_string() != BALANCE_MARK)
        .take_while(|c| *c == '─' || c.to_string() == BALANCE_MARK)
        .collect();
    assert!(track.len() > 1, "no balance track on the row: {cells:?}");
    let marks: Vec<usize> = track
        .iter()
        .enumerate()
        .filter(|(_, c)| c.to_string() == BALANCE_MARK)
        .map(|(at, _)| at)
        .collect();
    assert_eq!(marks.len(), 1, "one marker on the track: {track:?}");
    // The mockup's balance reading is the centre of its range, so the marker sits in the
    // middle of the track it is drawn on.
    assert_eq!(marks[0], (track.len() - 1) / 2, "{track:?}");
}

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
            let buffer = frame_with_theme(&cutting_app(), 80, 24, &theme);

            // The accent rule under the header, across the whole width.
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

            // The marker on the selected row, and the row it marks in inverse video.
            let (x, y) = cursor_cell(&buffer);
            let marker = buffer.cell((x, y)).expect("the marker cell");
            assert_eq!(marker.symbol(), "▶", "{drawn}");
            assert_eq!(marker.fg, theme.accent(), "{drawn}");
            for cell in [marker, buffer.cell((x + 2, y)).expect("the row's label")] {
                assert!(
                    cell.modifier.contains(Modifier::REVERSED | Modifier::BOLD),
                    "{drawn}: the selected row is not inverse video and bold"
                );
            }

            // The empty segments of the volume bar, dotted and faint.
            let y = text(&buffer)
                .iter()
                .position(|row| row.contains("Volume"))
                .expect("the volume row") as u16;
            let x = first_cell(&buffer, y, "░");
            let empty = buffer.cell((x, y)).expect("an empty bar cell");
            assert_eq!(empty.symbol(), "░", "{drawn}");
            assert_eq!(
                Some(empty.fg),
                theme.style(Role::Faint).fg,
                "{drawn}: an empty bar segment is not faint"
            );
        }
    }
}

/// The raw value is written inside the volume bar, centred on its cells: reversed on the
/// shade of the cells the bar has filled, so it reads as a hole in it, and in the text
/// role on the cells it has not.
#[test]
fn the_raw_value_is_written_inside_the_volume_bar() {
    let theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    let mut app = mockup_app();
    hear(
        &mut app,
        vec![converted("10", "volume", 63, "63 (-26.1 dB)", -26.1, "dB")],
    );
    app.settle();
    let cells = row_cells(&frame_with_theme(&app, 80, 24, &theme), "Volume");
    let bar: Vec<&ratatui::buffer::Cell> = cells[17..26].iter().collect();
    let drawn: String = bar.iter().map(|cell| cell.symbol()).collect();
    assert_eq!(
        drawn, "█63/127┃░",
        "the raw value is not centred in the bar"
    );
    // 63 of 127 fills four of the nine cells: "63/" is over the fill, "127" is not.
    for cell in &bar[1..4] {
        assert!(
            cell.modifier.contains(Modifier::REVERSED),
            "{:?} over the fill is not a hole in the bar",
            cell.symbol()
        );
        assert_ne!(cell.fg, Color::Reset, "{:?} has no shade", cell.symbol());
    }
    for cell in &bar[4..7] {
        assert!(!cell.modifier.contains(Modifier::REVERSED));
        assert_eq!(Some(cell.fg), theme.style(Role::Text).fg);
    }
}

/// On the focused row a bar keeps its own colours — its filled cells their accent shades,
/// never reversed — its empty cells take the accent's dim shade rather than the faint grey,
/// and the rest of the row takes the selected style as every focused row does.
#[test]
fn a_focused_volume_row_keeps_its_bar_in_its_own_colours() {
    for depth in [ColorDepth::TrueColor, ColorDepth::Ansi16] {
        let theme = Theme::from_config(&TuiConfig::default(), false, depth);
        let mut app = cutting_app();
        app.selected = 4; // Volume
        let cells = row_cells(&frame_with_theme(&app, 80, 24, &theme), "Volume");
        let filled = &cells[17];
        assert_eq!(filled.symbol(), "█");
        assert!(
            !filled.modifier.contains(Modifier::REVERSED),
            "{depth:?}: the focused row reversed its bar"
        );
        assert_eq!(
            filled.fg,
            theme.accent_shade(0.0),
            "{depth:?}: the first filled cell lost its accent shade"
        );
        let empty = cells
            .iter()
            .find(|cell| cell.symbol() == "░")
            .expect("an empty bar cell");
        assert!(!empty.modifier.contains(Modifier::REVERSED));
        assert_ne!(
            Some(empty.fg),
            theme.style(Role::Faint).fg,
            "{depth:?}: the focused row's empty cells are still the faint grey"
        );
        assert!(
            empty.fg == theme.accent_shade(0.0) || empty.modifier.contains(Modifier::DIM),
            "{depth:?}: the empty cells are not the accent's dim shade"
        );
        for (x, what) in [(2, "cursor"), (4, "label")] {
            assert!(
                cells[x]
                    .modifier
                    .contains(Modifier::REVERSED | Modifier::BOLD),
                "{depth:?}: the row's {what} is not drawn selected"
            );
        }
        let value = cells
            .iter()
            .find(|cell| cell.symbol() == "d")
            .expect("the value's dB");
        assert!(value.modifier.contains(Modifier::REVERSED), "{depth:?}");
    }
}

/// The balance track and the loudness bar keep their own colours on the focused row the
/// way the volume bar does, on the device screen's Tone pane as on the Controls pane.
#[test]
fn a_focused_balance_or_loudness_row_keeps_its_track_in_its_own_colours() {
    let theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    let mut app = mockup_app();
    app.selected = 5; // Balance
    let cells = row_cells(&frame_with_theme(&app, 80, 24, &theme), "Balance");
    let mark = cells
        .iter()
        .find(|cell| cell.symbol() == BALANCE_MARK)
        .expect("the balance marker");
    assert!(!mark.modifier.contains(Modifier::REVERSED));
    assert_eq!(mark.fg, theme.accent());
    let track = cells
        .iter()
        .find(|cell| cell.symbol() == "─")
        .expect("the track");
    assert!(!track.modifier.contains(Modifier::REVERSED));
    assert_eq!(track.fg, theme.accent_shade(0.0));

    app.pane = Pane::Tone;
    app.tone.row = ToneRow::Loudness;
    hear(&mut app, vec![vo("12.52", None, json!(64), "64")]);
    app.settle();
    let cells = row_cells(&frame_with_theme(&app, 80, 24, &theme), "Loudness");
    let loudness: Vec<&ratatui::buffer::Cell> = cells
        .iter()
        .filter(|cell| matches!(cell.symbol(), "█" | "░"))
        .collect();
    assert_eq!(
        loudness.len(),
        10,
        "the loudness bar, as wide as the volume bar"
    );
    assert!(
        loudness
            .iter()
            .all(|cell| !cell.modifier.contains(Modifier::REVERSED)),
        "the focused loudness row reversed its bar"
    );
}

/// In monochrome the focused row has reverse video and nothing else to stand out with, so
/// its bar is drawn in plain glyphs — filled and empty told apart by their shape — and the
/// raw value over the fill is still a hole in it. The mockup's curve boosts by +1.4 dB, and
/// the tick on the threshold step's cell reads by its shape alone as well.
#[test]
fn a_focused_bar_still_reads_in_monochrome() {
    let theme = Theme::new(true);
    let mut app = mockup_app();
    app.selected = 4; // Volume
    let cells = row_cells(&frame_with_theme(&app, 80, 24, &theme), "Volume");
    let drawn: String = cells[17..27].iter().map(|cell| cell.symbol()).collect();
    assert_eq!(drawn, "█109/127┃░");
    assert!(!cells[17].modifier.contains(Modifier::REVERSED));
    assert!(
        cells[18].modifier.contains(Modifier::REVERSED),
        "the 1 of 109"
    );
    assert!(cells[4].modifier.contains(Modifier::REVERSED), "the label");
}

/// A bar too short for the raw value over its range is drawn without it rather than cut:
/// a level whose range runs to seven digits, on a pane whose bar is ten cells long.
#[test]
fn a_bar_too_short_for_its_raw_value_is_drawn_without_it() {
    let mut profile = profile_named("gungnir2-v2");
    for control in &mut profile.controls {
        if control.key == "volume" {
            control.range = Some((0, 1_000_000));
        }
    }
    let mut app = app_on(profile);
    hear(
        &mut app,
        vec![converted("10", "volume", 109, "109", -6.0, "dB")],
    );
    app.settle();
    let cells: String = row_chars(&screen(&app), "Volume").into_iter().collect();
    assert!(
        !cells.contains("109/1000000") && cells.contains("░░░░░░░░░░"),
        "|{cells}|"
    );
}

/// The mockup app with band 4 back at 0 dB, a curve that only cuts: nothing clips, so the
/// volume bar has no zone and no tick, and every one of its cells is the bar's own.
fn cutting_app() -> App {
    let mut app = mockup_app();
    hear(&mut app, vec![vo("12.4.3", None, json!(98), "98")]);
    app
}

/// Where the volume row is drawn on the 80×24 mockup screen, and its bar: eleven cells
/// from [`VALUE_COLUMN`] with the converted value `0.0 dB` beside it.
const VOLUME_ROW: u16 = 8;

/// How many cells that bar takes.
const VOLUME_BAR: u16 = 11;

/// The Controls pane's volume bar shows the clipping-risk zone while the curve boosts in
/// EQ mode: a +6 dB band clips past step 109, which on an eleven-cell bar is its last two
/// cells — red at full volume, dim red below it — with the tick on the cell before them,
/// at true colour and at 16 colours.
#[test]
fn the_volume_bar_shows_the_zone_while_the_curve_boosts_in_eq_mode() {
    for depth in [ColorDepth::TrueColor, ColorDepth::Ansi16] {
        let theme = Theme::from_config(&TuiConfig::default(), false, depth);
        let alert = theme.style(Role::Alert).fg.expect("the alert colour");
        let ground = theme.alert_ground();
        let full = frame_with_theme(&boosting(127, 2), 80, 24, &theme);
        let row: String = (VALUE_COLUMN..VALUE_COLUMN + VOLUME_BAR + 8)
            .map(|x| {
                full.cell((x, VOLUME_ROW))
                    .expect("a cell")
                    .symbol()
                    .to_string()
            })
            .collect();
        assert_eq!(row, "█127/127┃██  0.0 dB", "{depth:?}");
        // Step 109's own cell, the ninth, carries the tick: in the alert role, bold, at a
        // volume past it.
        let tick_x = VALUE_COLUMN + 8;
        for x in (VALUE_COLUMN..VALUE_COLUMN + VOLUME_BAR).filter(|x| *x != tick_x) {
            let cell = full.cell((x, VOLUME_ROW)).expect("a bar cell");
            let zone = x >= VALUE_COLUMN + 9;
            assert_eq!(cell.fg == alert, zone, "{depth:?}: column {x}, {cell:?}");
        }
        let tick = full.cell((tick_x, VOLUME_ROW)).expect("the tick's cell");
        assert_eq!(tick.symbol(), "┃", "{depth:?}");
        assert_eq!(tick.fg, alert, "{depth:?}");
        assert!(tick.modifier.contains(Modifier::BOLD), "{depth:?}");

        // At 63, well below step 109, the tick is in the warn role.
        let quiet = frame_with_theme(&boosting(63, 2), 80, 24, &theme);
        // The value beside this bar, `-26.1 dB`, is a cell wider, so the bar is nine cells and
        // step 109's cell is the seventh.
        let quiet_tick_x = VALUE_COLUMN + 6;
        let tick = quiet
            .cell((quiet_tick_x, VOLUME_ROW))
            .expect("the tick's cell");
        assert_eq!(tick.symbol(), "┃", "{depth:?}");
        assert_eq!(Some(tick.fg), theme.style(Role::Warn).fg, "{depth:?}");
        assert!(!tick.modifier.contains(Modifier::BOLD), "{depth:?}");
        for x in VALUE_COLUMN + 7..VALUE_COLUMN + 9 {
            let cell = quiet.cell((x, VOLUME_ROW)).expect("a bar cell");
            assert_eq!(cell.symbol(), "░", "{depth:?}: column {x}");
            assert_eq!(Some(cell.fg), ground.fg, "{depth:?}: column {x}");
            assert!(cell.modifier.contains(ground.add_modifier), "{depth:?}");
        }
    }
}

/// The mockup app in EQ mode with every band at 0 dB but `band` (its function address) at
/// the raw gain `gain`, and the volume at `volume`, converted through the attenuation table
/// as the device converts it — so the value beside the bar is as wide as it would be, and
/// the bar as long.
fn peaking(band: &str, gain: i64, volume: i64) -> App {
    let mut app = mockup_app();
    let table = svolder_profiles::db_tables::table("dac-attenuation-db").expect("the table");
    let db = table[usize::try_from(volume).expect("a step")];
    hear(
        &mut app,
        vec![
            vo("12.2.3", None, json!(98), "98"),
            vo("12.4.3", None, json!(98), "98"),
            vo(band, None, json!(gain), &gain.to_string()),
            converted("10", "volume", volume, &volume.to_string(), db, "dB"),
        ],
    );
    app.settle();
    app
}

/// A bar cell holds a dozen steps at 80 columns, so the steps just past the threshold share
/// the tick's cell with it and fill no red cell yet: the tick itself says they are past it,
/// in the alert role and bold, where at the threshold and below it is in the warn role. A
/// +5.4 dB boost (band 2 at raw 124) clips past step 110: 110 is safe and 111 to 115 are not,
/// though the bar fills the same cells at all six. A +0.4 dB boost (band 1 at raw 100) clips
/// past step 125: 125 is safe and 126 is not. At true colour, 16 colours and in monochrome.
#[test]
fn a_volume_past_the_threshold_turns_the_tick_red_before_any_cell_fills_red() {
    let cases = [
        ("12.2.3", 124, 110, vec![110], vec![111, 112, 113, 114, 115]),
        ("12.1.3", 100, 125, vec![125], vec![126]),
    ];
    for (depth, monochrome) in [
        (ColorDepth::TrueColor, false),
        (ColorDepth::Ansi16, false),
        (ColorDepth::Ansi16, true),
    ] {
        let theme = Theme::from_config(&TuiConfig::default(), monochrome, depth);
        let warn = theme.style(Role::Warn).fg;
        let alert = theme.style(Role::Alert).fg;
        for (band, gain, threshold, safe, past) in &cases {
            let volumes = safe.iter().map(|v| (*v, false));
            for (volume, over) in volumes.chain(past.iter().map(|v| (*v, true))) {
                let what = format!("{depth:?} mono={monochrome}, {band}={gain} at {volume}");
                let app = peaking(band, *gain, volume);
                assert_eq!(app.clip_step(), Some(*threshold), "{what}");
                let buffer = frame_with_theme(&app, 80, 24, &theme);
                let bar: Vec<ratatui::buffer::Cell> = (VALUE_COLUMN..VALUE_COLUMN + 11)
                    .map(|x| buffer.cell((x, VOLUME_ROW)).expect("a bar cell").clone())
                    .collect();
                let ticks: Vec<&ratatui::buffer::Cell> =
                    bar.iter().filter(|cell| cell.symbol() == "┃").collect();
                assert_eq!(ticks.len(), 1, "{what}: {bar:?}");
                let tick = ticks[0];
                // No filled cell of the bar is red before the tick turns it: the tick
                // alone is what tells these volumes from the safe one.
                let red_fill = bar
                    .iter()
                    .any(|cell| cell.symbol() == "█" && Some(cell.fg) == alert);
                assert!(!red_fill || over, "{what}: a red cell at a safe volume");
                match over {
                    true => {
                        if !monochrome {
                            assert_eq!(Some(tick.fg), alert, "{what}");
                        }
                        assert!(tick.modifier.contains(Modifier::BOLD), "{what}");
                    }
                    false => {
                        if !monochrome {
                            assert_eq!(Some(tick.fg), warn, "{what}");
                        }
                        assert!(!tick.modifier.contains(Modifier::BOLD), "{what}");
                    }
                }
            }
        }
    }
}

/// No zone and no tick in Flat mode, where no band applies, in Loudness mode, whose boost
/// nothing has measured, nor for a curve that only cuts.
#[test]
fn the_volume_bar_has_no_zone_in_flat_or_loudness_mode_or_for_a_cut() {
    let cut = || {
        let mut app = boosting(127, 2);
        hear(&mut app, vec![vo("12.2.3", None, json!(60), "60")]);
        app
    };
    for depth in [ColorDepth::TrueColor, ColorDepth::Ansi16] {
        let theme = Theme::from_config(&TuiConfig::default(), false, depth);
        let alert = theme.style(Role::Alert).fg;
        for (what, app) in [
            ("flat mode", boosting(127, 1)),
            ("loudness mode", boosting(127, 3)),
            ("a cut", cut()),
        ] {
            let buffer = frame_with_theme(&app, 80, 24, &theme);
            for x in VALUE_COLUMN..VALUE_COLUMN + VOLUME_BAR {
                let cell = buffer.cell((x, VOLUME_ROW)).expect("a bar cell");
                assert_ne!(Some(cell.fg), alert, "{depth:?}, {what}: column {x}");
                assert_ne!(cell.symbol(), "┃", "{depth:?}, {what}: a tick at {x}");
            }
        }
    }
}
