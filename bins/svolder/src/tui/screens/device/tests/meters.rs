// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The meter rows on an amp, linear and logarithmic, their floors, ticks and peaks.

use super::*;

#[test]
fn an_amp_lists_its_meters_and_has_no_tone_pane() {
    let mut app = app_on(profile_named("vidar2"));
    hear(
        &mut app,
        vec![
            vo("10", Some("power"), json!(true), "on"),
            vo("21.2", Some("power_meters"), json!(true), "on"),
            ValueObject {
                value_type: "u16".into(),
                ..vo("20.1", Some("meter_1"), json!(1180), "1180")
            },
            ValueObject {
                value_type: "u16".into(),
                ..vo("20.2", Some("meter_2"), json!(0), "0")
            },
        ],
    );
    let rows = text(&screen(&app));
    // The Vidar 2's meters read on its own scale (zero 50, full scale 1000): a reading past
    // full scale fills the bar and reads 100%, one under the zero leaves the live part empty.
    assert_eq!(
        &rows[3..8],
        [
            pane_row(" ▶ Power        [x] on"),
            pane_row("   Power meters [x] on"),
            pane_row(""),
            pane_row("   Meter L      ▓┃█████████████  100%"),
            pane_row("   Meter R      ▓┃░░░░░░░░░░░░░    0%"),
        ],
    );
    assert!(
        rows.iter().all(|row| !row.contains("Tone control")),
        "an amp has no tone control"
    );
}

/// Five scaled meters — `fake-five`'s own, the most any built-in profile streams — list
/// one row each and leave the Status pane's own line standing at 80×24, the screen this
/// whole mockup is drawn for.
#[test]
fn five_meters_fit_the_controls_pane_at_80x24() {
    let mut app = app_on(profile_named("fake-five"));
    hear(
        &mut app,
        (1..=5)
            .map(|n| {
                let id = format!("20.{n}");
                let key = format!("meter_{n}");
                ValueObject {
                    value_type: "u16".into(),
                    ..vo(&id, Some(&key), json!(500), "500")
                }
            })
            .collect(),
    );
    let rows = text(&screen(&app));
    for n in 1..=5 {
        assert!(
            rows.iter()
                .any(|row| row.contains(&format!("Meter {n}")) && row.contains("97%")),
            "Meter {n} missing at 80x24: {rows:?}"
        );
    }
    assert!(
        rows.iter().any(|row| row.contains("Faults:")),
        "the Status pane's own line is gone at 80x24: {rows:?}"
    );
}

/// A meter that has just been loud keeps a marker where it reached, out past the fill its
/// quieter reading draws, and drops it with the peak-hold switch off.
#[test]
fn a_meters_recent_peak_is_marked_out_past_the_bar() {
    let mut app = amp_with_scale();
    let now = Instant::now();
    app.apply(Update::Status(meter(8_000)), now);
    app.apply(Update::Status(meter(1_000)), now);

    let buffer = screen(&app);
    let row = text(&buffer)
        .iter()
        .position(|row| row.contains("Meter L"))
        .expect("a Meter L row") as u16;
    let mark = first_cell(&buffer, row, "▎");
    let last_filled = (0..buffer.area.width)
        .rfind(|&x| {
            buffer
                .cell((x, row))
                .is_some_and(|cell| cell.symbol() == "█")
        })
        .expect("the bar has a filled cell");
    assert!(
        mark > last_filled,
        "the marker sits inside the bar's own fill: {mark} is not past {last_filled}"
    );
    assert_eq!(
        buffer.cell((mark, row)).expect("the marker cell").fg,
        Color::Yellow,
        "the marker is not drawn in the warn role"
    );

    app.peak_hold = false;
    assert!(
        !text(&screen(&app))[usize::from(row)].contains("▎"),
        "a marker was drawn with the peak-hold switch off"
    );
}

#[test]
fn a_meter_takes_one_role_at_sixteen_colours_and_blends_above_it() {
    let ansi16 = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16);

    // 8,000 / 10,000 = 80%, past the class-A tick and short of full scale: hot (yellow),
    // the one role the whole bar takes at a depth with no room to shade along it.
    let hot = app_with_meter_reading(8_000);
    let hot_fgs = bar_cell_fgs(&frame_with_theme(&hot, 80, 24, &ansi16), "Meter L");
    assert!(
        hot_fgs.iter().all(|&fg| fg == Color::Yellow),
        "a bar 80% full is uniformly yellow at 16 colours: {hot_fgs:?}"
    );

    // Full scale: clipped (red), uniformly, not shaded from green to red along its length.
    let clipped = app_with_meter_reading(10_000);
    let clip_fgs = bar_cell_fgs(&frame_with_theme(&clipped, 80, 24, &ansi16), "Meter L");
    assert!(
        clip_fgs.iter().all(|&fg| fg == Color::Red),
        "a full-scale bar is uniformly red at 16 colours: {clip_fgs:?}"
    );

    // The same full-scale reading blends along the bar once there is depth to draw it in.
    let true_colour = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    let true_fgs = bar_cell_fgs(&frame_with_theme(&clipped, 80, 24, &true_colour), "Meter L");
    assert!(
        true_fgs.len() >= 2,
        "the bar has more than one filled cell to shade across"
    );
    assert_ne!(
        true_fgs.first(),
        true_fgs.last(),
        "true colour shades along the bar rather than drawing one colour"
    );
}

/// The foreground colour of every `█` cell on the row naming `label`, left to right.
fn bar_cell_fgs(buffer: &Buffer, label: &str) -> Vec<Color> {
    let y = text(buffer)
        .iter()
        .position(|row| row.contains(label))
        .unwrap_or_else(|| panic!("a row naming {label:?}")) as u16;
    (0..buffer.area.width)
        .filter_map(|x| {
            let cell = buffer.cell((x, y)).expect("a cell inside the buffer");
            (cell.symbol() == "█").then_some(cell.fg)
        })
        .collect()
}

/// An app on `vidar2` with `meter_1` given a scale, a floor and a class-A tick, reporting
/// `raw` on it.
fn app_with_meter_floor_and_tick(raw: i64) -> App {
    let mut profile = with_range_on(profile_named("vidar2"), "meter_1", (0, 3000));
    for control in &mut profile.controls {
        if control.key == "meter_1" {
            control.floor = Some(900);
            control.tick = Some(1400);
        }
    }
    let mut app = app_on(profile);
    hear(
        &mut app,
        vec![ValueObject {
            value_type: "u16".into(),
            ..vo(
                "20.1",
                Some("meter_1"),
                json!(raw),
                raw.to_string().as_str(),
            )
        }],
    );
    app
}

#[test]
fn a_meter_with_a_floor_fills_from_the_floor_and_marks_the_class_a_tick() {
    let cells = row_chars(&screen(&app_with_meter_floor_and_tick(1400)), "Meter L");
    let bar_start = cells
        .iter()
        .position(|c| matches!(c, '█' | '░' | '┃'))
        .expect("a bar");
    let bar_len = cells
        .iter()
        .filter(|c| matches!(c, '█' | '░' | '┃'))
        .count();
    // The bar is 15 cells wide on this mockup; a layout change that moved it is its own
    // finding, not one to fold into the fill count below.
    assert_eq!(bar_len, 15, "{cells:?}");
    let filled = cells.iter().filter(|&&c| c == '█').count();
    // 1400 is 500 of the 2100 (3000 floor-subtracted to 900) left to fill: 500 * 15 / 2100
    // truncates to 3. Pinned to that literal count, not re-derived by the bar's own
    // truncating-divide formula, so a wrong rounding or clamp in `fill` (widgets/rows.rs) reads
    // differently here rather than reproducing itself in the expectation too.
    assert_eq!(filled, 3, "{cells:?}");
    let tick_at = cells
        .iter()
        .position(|&c| c == '┃')
        .expect("the tick glyph");
    // The tick sits on the same fraction, mapped over one less step than the fill itself
    // (a marker on a track, the way the balance marker sits on its track): 500 * 14 / 2100
    // truncates to 3.
    assert_eq!(tick_at - bar_start, 3, "{cells:?}");
}

#[test]
fn a_meter_reading_at_or_below_its_floor_draws_no_filled_cells() {
    for raw in [500, 900] {
        let cells = row_chars(&screen(&app_with_meter_floor_and_tick(raw)), "Meter L");
        assert_eq!(
            cells.iter().filter(|&&c| c == '█').count(),
            0,
            "raw {raw}: {cells:?}"
        );
        assert!(
            cells.contains(&'┃'),
            "the tick still marks its column regardless of the reading: {cells:?}"
        );
    }
}

#[test]
fn a_meter_reading_at_or_above_max_fills_every_cell_but_the_ticks_own() {
    let cells = row_chars(&screen(&app_with_meter_floor_and_tick(5_000)), "Meter L");
    let bar_len = cells
        .iter()
        .filter(|c| matches!(c, '█' | '░' | '┃'))
        .count();
    let filled = cells.iter().filter(|&&c| c == '█').count();
    assert_eq!(
        filled,
        bar_len - 1,
        "every cell but the tick's own is filled at or above max: {cells:?}"
    );
}

#[test]
fn a_meter_with_a_tick_turns_hot_at_the_tick_rather_than_at_the_usual_fraction() {
    let ansi16 = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16);

    // 1400 is only 24% of the floor-subtracted range, well under the 70% that would turn
    // a meter without a tick hot, but at the tick itself the bar is already hot.
    let at_tick = app_with_meter_floor_and_tick(1400);
    let at_tick_fgs = bar_cell_fgs(&frame_with_theme(&at_tick, 80, 24, &ansi16), "Meter L");
    assert!(
        at_tick_fgs.iter().all(|&fg| fg == Color::Yellow),
        "at the tick the bar is hot: {at_tick_fgs:?}"
    );

    // Just below the tick, the bar is still in its normal range.
    let below_tick = app_with_meter_floor_and_tick(1399);
    let below_tick_fgs = bar_cell_fgs(&frame_with_theme(&below_tick, 80, 24, &ansi16), "Meter L");
    assert!(
        below_tick_fgs.iter().all(|&fg| fg == Color::Green),
        "just below the tick the bar is still ok: {below_tick_fgs:?}"
    );
}

/// In the tall tier a meter's bar is drawn on two rows, its label and reading on the first
/// alone, and the meters sit a row below the toggles above them.
#[test]
fn a_tall_terminal_draws_a_meter_two_rows_tall() {
    let rows = text(&frame(&app_with_meter_reading(5_000), 126, 62));
    let at = rows
        .iter()
        .position(|row| row.contains("Meter L"))
        .expect("a Meter L row");
    assert!(
        rows[at - 1].trim_matches(['│', ' ']).is_empty(),
        "no row above the meters"
    );
    let first: String = rows[at].chars().skip(17).take(25).collect();
    let second: String = rows[at + 1].chars().skip(17).take(25).collect();
    assert_eq!(first, second, "the bar is not drawn on both rows");
    assert!(first.contains('█'));
    assert!(
        !rows[at + 1].contains("50%") && rows[at].contains("50%"),
        "the percentage is not on the first row alone"
    );
    let short = text(&frame(&app_with_meter_reading(5_000), 126, 30));
    let at = short
        .iter()
        .position(|row| row.contains("Meter L"))
        .expect("a Meter L row");
    assert!(
        !short[at + 1].contains('█'),
        "a short terminal still draws the meter on two rows"
    );
}

/// The text of the row naming `label`, without the pane border and the blanks after it.
fn row_text(buffer: &Buffer, label: &str) -> String {
    let row: String = row_chars(buffer, label).into_iter().collect();
    row.trim_end().trim_end_matches('│').trim_end().to_string()
}

/// An app on `vidar2` with the Aegir 2's scale on its meters, the switch, power and readings
/// given as `(id, key, json)` triples.
fn aegir_scaled(heard: &[(&str, &str, serde_json::Value)]) -> App {
    let mut app = app_on(with_log_scale(profile_named("vidar2"), AEGIR_SCALE));
    hear(
        &mut app,
        heard
            .iter()
            .map(|(id, key, value)| ValueObject {
                value_type: match value {
                    serde_json::Value::Bool(_) => "bool".into(),
                    _ => "u16".into(),
                },
                ..vo(id, Some(key), value.clone(), &value.to_string())
            })
            .collect(),
    );
    app
}

/// The bar's glyphs on the "Meter L" row, in order: `▓`, `┃`, `█`, `░` and `▎`.
fn meter_bar(app: &App) -> String {
    row_chars(&screen(app), "Meter L")
        .into_iter()
        .filter(|c| matches!(c, '▓' | '┃' | '█' | '░' | '▎'))
        .collect()
}

#[test]
fn a_log_meter_draws_its_class_a_block_then_the_reading_on_the_live_part() {
    // 173 is half way up the Aegir 2's live part.
    let app = aegir_scaled(&[("20.1", "meter_1", json!(173))]);
    let bar = meter_bar(&app);
    let w = bar.chars().count();
    let k = ((0.25 * w as f64).round() as usize).clamp(1, w - 1);
    let live = w - k;
    let expected: String = std::iter::repeat_n('▓', k - 1)
        .chain(std::iter::once('┃'))
        .chain(std::iter::repeat_n('█', live / 2))
        .chain(std::iter::repeat_n('░', live - live / 2))
        .collect();
    assert_eq!(bar, expected);
    assert!(
        row_text(&screen(&app), "Meter L")
            .trim_end()
            .ends_with(" 50%")
    );
}

#[test]
fn a_log_meter_at_its_zero_shows_only_the_block_and_at_full_scale_fills_the_live_part() {
    let quiet = aegir_scaled(&[("20.1", "meter_1", json!(160))]);
    assert!(!meter_bar(&quiet).contains('█'), "{}", meter_bar(&quiet));
    assert!(
        row_text(&screen(&quiet), "Meter L")
            .trim_end()
            .ends_with("  0%")
    );
    let loud = aegir_scaled(&[("20.1", "meter_1", json!(400))]);
    assert!(!meter_bar(&loud).contains('░'), "{}", meter_bar(&loud));
    assert!(
        row_text(&screen(&loud), "Meter L")
            .trim_end()
            .ends_with("100%")
    );
}

#[test]
fn a_dead_meter_draws_dim_and_empty_and_reads_off() {
    for (key, id, value) in [
        ("power_meters", "21.2", json!(false)),
        ("power", "10", json!(false)),
        ("warming_up", "4", json!(true)),
    ] {
        let app = aegir_scaled(&[
            ("20.1", "meter_1", json!(300)),
            ("10", "power", json!(key != "power")),
            (id, key, value),
        ]);
        let bar = meter_bar(&app);
        assert!(!bar.contains('█') && !bar.contains('▎'), "{key}: {bar}");
        assert!(
            bar.contains('▓') || bar.starts_with('┃'),
            "{key}: the block stays: {bar}"
        );
        assert!(
            row_text(&screen(&app), "Meter L")
                .trim_end()
                .ends_with(" off"),
            "{key}"
        );
    }
}

#[test]
fn a_meter_whose_switch_has_not_reported_reads_as_live() {
    let app = aegir_scaled(&[("20.1", "meter_1", json!(173))]);
    assert!(app.meters_live());
    assert!(meter_bar(&app).contains('█'));
}

#[test]
fn a_scaled_meter_with_no_reading_yet_draws_its_empty_bar_and_a_dash() {
    let app = aegir_scaled(&[]);
    let bar = meter_bar(&app);
    assert!(bar.contains('┃') && !bar.contains('█'), "{bar}");
    assert!(row_text(&screen(&app), "Meter L").trim_end().ends_with("—"));
}

#[test]
fn the_floor_arriving_moves_the_same_reading_down_the_bar() {
    // Tyr: no floor yet, so the zero is 0 and 31 reads half way; a floor of 20 puts the zero at
    // 30, where 31 is one step above it: log10(2) / log10(971), 10 %.
    let mut app = app_on(tyr_following_its_floor());
    hear(
        &mut app,
        vec![ValueObject {
            key: Some("meter_1".into()),
            ..meter_reading("20.1", 31)
        }],
    );
    let before = row_text(&screen(&app), "Meter");
    assert!(before.trim_end().ends_with(" 50%"), "{before}");
    hear(
        &mut app,
        vec![ValueObject {
            key: Some("meter_floor".into()),
            ..meter_reading("20.2", 20)
        }],
    );
    let after = row_text(&screen(&app), "Meter");
    assert!(after.trim_end().ends_with(" 10%"), "{after}");
}

#[test]
fn a_floor_past_full_scale_draws_an_empty_bar_at_zero_percent() {
    let mut app = app_on(tyr_following_its_floor());
    hear(
        &mut app,
        vec![
            ValueObject {
                key: Some("meter_floor".into()),
                ..meter_reading("20.2", 700)
            },
            ValueObject {
                key: Some("meter_1".into()),
                ..meter_reading("20.1", 65_000)
            },
        ],
    );
    assert!(
        row_text(&screen(&app), "Meter")
            .trim_end()
            .ends_with("  0%")
    );
    assert!(
        !row_chars(&screen(&app), "Meter").contains(&'█'),
        "a floor past full scale filled the bar"
    );
}

#[test]
fn the_floor_readout_leaves_the_status_pane() {
    let mut app = app_on(tyr_following_its_floor());
    hear(
        &mut app,
        vec![ValueObject {
            key: Some("meter_floor".into()),
            ..meter_reading("20.2", 20)
        }],
    );
    assert!(
        !screen(&app)
            .content()
            .iter()
            .map(|c| c.symbol())
            .collect::<String>()
            .contains("Meter floor")
    );
}

/// At 16 colours a log meter's fill takes one role by how far up the live part it reaches (ok
/// under 70%, hot from 70%, clip from 90%), and its class-A block is faint at every depth.
#[test]
fn a_log_meters_fill_takes_one_role_and_its_block_stays_faint() {
    let ansi16 = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16);
    let true_colour = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    // The fraction 0.5 at 173, about 0.8 at 270 and 1.0 at 350 on the 160..350 scale.
    for (raw, colour) in [(173, Color::Green), (270, Color::Yellow), (350, Color::Red)] {
        let app = aegir_scaled(&[("20.1", "meter_1", json!(raw))]);
        let fgs = bar_cell_fgs(&frame_with_theme(&app, 80, 24, &ansi16), "Meter L");
        assert!(
            !fgs.is_empty() && fgs.iter().all(|&fg| fg == colour),
            "{raw}: {fgs:?}"
        );
        for theme in [&ansi16, &true_colour] {
            let buffer = frame_with_theme(&app, 80, 24, theme);
            let y = text(&buffer)
                .iter()
                .position(|row| row.contains("Meter L"))
                .expect("a Meter L row") as u16;
            let block: Vec<Color> = (0..buffer.area.width)
                .filter_map(|x| buffer.cell((x, y)))
                .filter(|cell| cell.symbol() == "▓")
                .map(|cell| cell.fg)
                .collect();
            let faint = theme.style(Role::Faint).fg;
            assert!(
                !block.is_empty() && block.iter().all(|&fg| Some(fg) == faint),
                "{block:?}"
            );
        }
    }
}

/// A log meter that has just been loud marks its peak on the live cell its fraction reached,
/// past the fill of the quieter reading behind it.
#[test]
fn a_log_meters_recent_peak_is_marked_on_its_live_part() {
    let mut app = app_on(with_log_scale(profile_named("vidar2"), AEGIR_SCALE));
    animated(&mut app);
    let now = Instant::now();
    app.apply(Update::Status(meter(300)), now);
    app.apply(Update::Status(meter(170)), now);
    // The bar is 15 cells: a block of 4 (a quarter, rounded), 11 live cells, the reading 170
    // filling 5 of them, and the peak 300 (0.94 of the way up) landing on live cell 9 of the 10
    // a marker may take, cell 13 of the bar.
    let bar = meter_bar(&app);
    assert_eq!(bar, "▓▓▓┃█████░░░░▎░");
}

/// In the tall tier a log meter's block and tick are on both rows of its bar.
#[test]
fn a_tall_terminal_draws_a_log_meter_with_its_block_on_both_rows() {
    let app = aegir_scaled(&[("20.1", "meter_1", json!(173))]);
    let rows = text(&frame(&app, 126, 62));
    let at = rows
        .iter()
        .position(|row| row.contains("Meter L"))
        .expect("a Meter L row");
    for row in [&rows[at], &rows[at + 1]] {
        assert!(
            row.contains('▓') && row.contains('┃') && row.contains('█'),
            "{row}"
        );
    }
}
