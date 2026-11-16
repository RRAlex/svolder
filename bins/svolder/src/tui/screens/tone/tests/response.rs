// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The Response block: the curve, its points and its cells.

use super::*;

#[test]
fn a_band_that_is_lifted_bends_the_curve_away_from_flat() {
    let flat_buffer = screen(&flat_app());
    let flat = text(&flat_buffer);
    let mut app = flat_app();
    // Raw 127 is the top of the gain range, +6.0 dB.
    hear(&mut app, vec![vo("12.2.3", 127)]);
    let buffer = screen(&app);
    let lifted = text(&buffer);
    assert_ne!(
        flat[3..15],
        lifted[3..15],
        "a band at +6 dB did not move the curve"
    );
    assert_eq!(
        lifted[19],
        "│   2    160 Hz    +6.0 dB    4.30      64 / 127 / 64                          │",
        "the band's own row does not carry its new gain"
    );
    // The curve did not just change somewhere — band 2's own point rose toward the ceiling,
    // the direction a lift bends it: its row (gain grows upward, so a smaller row) is above
    // where it sat flat. Points sort left to right by frequency, so index 1 is band 2's.
    let mut flat_points = points(&flat_buffer);
    flat_points.sort_unstable_by_key(|(x, _, _)| *x);
    let mut lifted_points = points(&buffer);
    lifted_points.sort_unstable_by_key(|(x, _, _)| *x);
    assert_eq!(
        flat_points.len(),
        5,
        "the flat curve should mark all five bands"
    );
    assert_eq!(
        lifted_points.len(),
        5,
        "the lifted curve should mark all five bands"
    );
    assert!(
        lifted_points[1].1 < flat_points[1].1,
        "band 2's own point did not move toward +6 dB: flat row {}, lifted row {}",
        flat_points[1].1,
        lifted_points[1].1
    );
}

/// A cell of the curve stands for the frequency and the gain the chart draws in it, at 80×24
/// and at 126×62: every column's frequency sits on that column of the log axis and every
/// row's gain draws back in that row; a band's step found from the cell its point is drawn
/// in — the step nearest what the cell stands for — is drawn in that same cell again, for
/// every step of every band. A column or a row past the curve stands for the end it is
/// past.
#[test]
fn a_cell_of_the_curve_stands_for_what_the_chart_draws_in_it() {
    let eq = profile().eq.expect("the Gungnir 2 has a tone control");
    for (width, height) in [(80, 24), (126, 62)] {
        let size = format!("{width}×{height}");
        let curve = tone::curve_area(Rect::new(0, 0, width, height), 5);
        for x in curve.x..curve.right() {
            let along = tone::along(curve, tone::frequency_at(curve, x).log10());
            assert!(
                (along - f64::from(x - curve.x)).abs() < 1e-9,
                "{size}: column {x} reads back as {along}"
            );
        }
        for y in curve.y..curve.bottom() {
            assert_eq!(
                tone::row_at(curve, tone::gain_at(curve, y)),
                Some(y),
                "{size}: row {y}"
            );
        }
        for band in 0..eq.bands.len() {
            for raw in 0..=EqSpec::STEP_MAX {
                let hz = eq.frequency(band, raw).expect("a band of the profile");
                let x = tone::column_at(curve, hz.log10()).expect("on the axis");
                let found = eq
                    .frequency_raw(band, tone::frequency_at(curve, x))
                    .expect("a band of the profile");
                let back = eq.frequency(band, found).expect("a band of the profile");
                assert_eq!(
                    tone::column_at(curve, back.log10()),
                    Some(x),
                    "{size}: band {} step {raw} came back as step {found}",
                    band + 1
                );
            }
        }
        assert!(
            (tone::frequency_at(curve, 0) - F_MIN).abs() < 1e-6,
            "{size}"
        );
        assert!(
            (tone::frequency_at(curve, curve.right() + 5) - F_MAX).abs() < 1e-6,
            "{size}"
        );
        assert_eq!(tone::gain_at(curve, 0), tone::GAIN_CEILING_DB, "{size}");
        assert_eq!(
            tone::gain_at(curve, curve.bottom() + 3),
            tone::GAIN_FLOOR_DB,
            "{size}"
        );
    }
}

/// The y axis names each gain on the row its gain is drawn in — the row [`row_at`] gives it,
/// the one a drag reads that gain from — at 80×24, 100×30 and 126×62, in the column left of
/// the axis, and nothing else is written in that column: the `0` sits on the row a flat curve
/// is drawn on.
#[test]
fn each_gain_label_sits_on_the_row_its_gain_is_drawn_in() {
    let app = flat_app();
    for (width, height) in [(80, 24), (100, 30), (126, 62)] {
        let size = format!("{width}×{height}");
        let buffer = frame_with_theme(&app, width, height, &Theme::new(false));
        let curve = tone::curve_area(Rect::new(0, 0, width, height), 5);
        let left = curve.x - tone::GAIN_LABEL_WIDTH - 1;
        let label = |y: u16| -> String {
            (left..left + tone::GAIN_LABEL_WIDTH)
                .map(|x| buffer.cell((x, y)).expect("a label cell").symbol())
                .collect()
        };
        let rows: Vec<u16> = tone::GAIN_LABELS
            .iter()
            .map(|&(db, text)| {
                let y = tone::row_at(curve, db).expect("a gain on the chart");
                assert_eq!(label(y), text, "{size}: {text:?}");
                y
            })
            .collect();
        for y in (curve.y..curve.bottom()).filter(|y| !rows.contains(y)) {
            assert!(label(y).trim().is_empty(), "{size}: row {y} |{}|", label(y));
        }
        let flat: Vec<u16> = braille_at(&buffer, curve)
            .into_iter()
            .map(|(_, y)| y)
            .collect();
        assert!(!flat.is_empty(), "{size}: no flat line");
        assert!(
            flat.iter().all(|&y| y == rows[2]),
            "{size}: the 0 label is on row {}, the flat line on {flat:?}",
            rows[2]
        );
    }
}

/// The cells of `curve` that draw a Braille dot.
fn braille_at(buffer: &Buffer, curve: Rect) -> Vec<(u16, u16)> {
    (curve.y..curve.bottom())
        .flat_map(|y| (curve.x..curve.right()).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            buffer
                .cell((x, y))
                .is_some_and(|cell| cell.symbol().chars().all(|c| ('⠁'..='⣿').contains(&c)))
        })
        .collect()
}

/// Every band's point is drawn in the very cell [`points`] names for it — the cell a press
/// grabs it by — at 80×24 and at 126×62, with the bands off 0 dB and off the frequency their
/// column stands for: the chart's `•` cells are exactly those, and the focused band's is the
/// one in the focused point's colour.
#[test]
fn every_point_is_drawn_in_the_cell_points_names_for_it() {
    let eq = profile().eq.expect("the Gungnir 2 has a tone control");
    let mut app = flat_app();
    let bands = [(74, 110), (52, 80), (94, 120), (38, 60), (100, 104)];
    let mut values = Vec::new();
    for (band, (hz, db)) in bands.into_iter().enumerate() {
        values.push(vo(&format!("12.{}.2", band + 1), hz));
        values.push(vo(&format!("12.{}.3", band + 1), db));
    }
    hear(&mut app, values);
    app.tone.band = 2;
    let theme = Theme::new(false);
    let focused = theme.style(Role::CurveSelected).fg;
    for (width, height) in [(80, 24), (126, 62)] {
        let size = format!("{width}×{height}");
        let buffer = frame_with_theme(&app, width, height, &theme);
        let curve = tone::curve_area(Rect::new(0, 0, width, height), 5);
        let points = tone::points(&app, curve);
        assert_eq!(points.len(), 5, "{size}");
        for &(band, (x, _)) in &points {
            let hz = u8::try_from(bands[band].0).expect("a step");
            assert_ne!(
                eq.frequency_raw(band, tone::frequency_at(curve, x)),
                Some(hz),
                "{size}: band {} sits on its column's own step",
                band + 1
            );
        }
        let mut drawn: Vec<(u16, u16)> = (curve.y..curve.bottom())
            .flat_map(|y| (curve.x..curve.right()).map(move |x| (x, y)))
            .filter(|&cell| buffer.cell(cell).is_some_and(|cell| cell.symbol() == "•"))
            .collect();
        let mut named: Vec<(u16, u16)> = points.iter().map(|&(_, cell)| cell).collect();
        drawn.sort_unstable();
        named.sort_unstable();
        assert_eq!(drawn, named, "{size}");
        let (_, cell) = points
            .iter()
            .find(|(band, _)| *band == 2)
            .expect("band 3's point");
        assert_eq!(
            Some(buffer.cell(*cell).expect("the point's cell").fg),
            focused,
            "{size}: the focused point is not where points names it"
        );
    }
}

#[test]
fn a_band_the_device_has_not_reported_in_full_is_left_off_the_curve() {
    let mut app = flat_app();
    app.readings
        .values
        .remove(&"12.2.1".parse().expect("a valid function id"));
    let rows = text(&screen(&app));
    assert_eq!(
        rows[19],
        "│   2    160 Hz    +0.0 dB       —      64 / 98 / —                            │",
        "the band's row does not show what the device has not said"
    );
    assert_eq!(
        rows[6].matches('•').count(),
        4,
        "a band with no Q is still drawn as a point on the curve"
    );
}

#[test]
fn the_focused_bands_point_stands_out_from_the_others() {
    let mut app = lifted_app();
    app.tone.band = 1;
    let theme = Theme::new(false);
    let selected = theme
        .style(Role::CurveSelected)
        .fg
        .expect("the focused point's colour");
    let buffer = frame_with_theme(&app, 80, 24, &theme);
    let curve = tone::curve_area(buffer.area, 5);
    let focused: Vec<u16> = (curve.x..curve.right())
        .filter(|x| {
            (curve.y..curve.bottom())
                .any(|y| buffer.cell((*x, y)).is_some_and(|c| c.fg == selected))
        })
        .collect();
    assert_eq!(
        focused.len(),
        1,
        "one band's point is the focused one, in Role::CurveSelected"
    );
    // Band 2 sits at 160 Hz, about three tenths of the way along a 20 Hz to 20 kHz log axis
    // (log10(160/20) / log10(20000/20), computed below rather than restated as a fraction).
    let along = f64::from(focused[0] - curve.x) / f64::from(curve.width - 1);
    assert!(
        (along - (160f64 / 20.0).log10() / 3.0).abs() < 0.02,
        "the focused point is not drawn at 160 Hz, but {along} along the axis"
    );
}

/// The cells of the curve's area that draw a Braille dot, with the style each is drawn in.
fn braille(buffer: &Buffer) -> Vec<(u16, u16, ratatui::buffer::Cell)> {
    let curve = tone::curve_area(buffer.area, 5);
    (curve.y..curve.bottom())
        .flat_map(|y| (curve.x..curve.right()).map(move |x| (x, y)))
        .filter_map(|(x, y)| {
            let cell = buffer.cell((x, y))?.clone();
            cell.symbol()
                .chars()
                .all(|c| ('⠁'..='⣿').contains(&c))
                .then_some((x, y, cell))
        })
        .collect()
}

/// The cells of the curve's area that mark a band's centre.
fn points(buffer: &Buffer) -> Vec<(u16, u16, ratatui::buffer::Cell)> {
    let curve = tone::curve_area(buffer.area, 5);
    (curve.y..curve.bottom())
        .flat_map(|y| (curve.x..curve.right()).map(move |x| (x, y)))
        .filter_map(|(x, y)| {
            let cell = buffer.cell((x, y))?.clone();
            (cell.symbol() == "•").then_some((x, y, cell))
        })
        .collect()
}

/// In EQ mode with a band off 0 dB the graph draws the summed curve in its own role, which
/// the spectrum then colours, each band's point in the point roles and the focused one in
/// its own; the band table is drawn as ever.
#[test]
fn in_eq_mode_a_shaped_curve_is_drawn_as_a_curve() {
    let theme = Theme::new(false);
    let app = lifted_app();
    let buffer = frame_with_theme(&app, 80, 24, &theme);
    assert!(
        tone::curve_of(buffer.area, &app).is_some(),
        "no curve to colour"
    );
    let curve = theme.style(Role::CurveSum);
    let dots = braille(&buffer);
    assert!(dots.len() > 60, "{} dots", dots.len());
    assert!(dots.iter().all(|(_, _, cell)| Some(cell.fg) == curve.fg));
    let marks = points(&buffer);
    assert_eq!(marks.len(), 5);
    let focused = theme.style(Role::CurveSelected).fg;
    let point = theme.style(Role::CurvePoint).fg;
    assert_eq!(
        marks
            .iter()
            .filter(|(_, _, cell)| Some(cell.fg) == focused)
            .count(),
        1
    );
    assert_eq!(
        marks
            .iter()
            .filter(|(_, _, cell)| Some(cell.fg) == point)
            .count(),
        4
    );
    let band = buffer.cell((9, 19)).expect("band 2's frequency");
    assert_eq!(band.symbol(), "1");
    assert!(
        !band.modifier.contains(Modifier::DIM),
        "the band table is dimmed"
    );
}

/// In EQ mode with every band at 0 dB the graph draws the flat curve muted: the line and
/// every band's point, the focused one's too, in the dim role at the widget layer — what the
/// spectrum then colours desaturated at true colour ([`curve_of`] answers a muted curve
/// rather than none, [`curve_muted`] says so); a 16-colour theme has no spectrum to colour it
/// with, so it keeps the dim role, which this buffer-only render (no spectrum applied over
/// it) shows at every depth. The table and its text are as ever.
#[test]
fn in_eq_mode_a_flat_curve_is_a_muted_flat_line() {
    for theme in [
        Theme::new(false),
        Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor),
    ] {
        let app = flat_app();
        let buffer = frame_with_theme(&app, 80, 24, &theme);
        assert_eq!(
            tone::curve_of(buffer.area, &app),
            Some(tone::curve_area(buffer.area, 5))
        );
        assert!(tone::curve_muted(&app), "not reported muted");
        let dim = theme.style(Role::Dim);
        for (x, y, cell) in braille(&buffer).into_iter().chain(points(&buffer)) {
            assert_eq!(
                (Some(cell.fg), cell.modifier.contains(dim.add_modifier)),
                (dim.fg.or(Some(cell.fg)), true),
                "({x}, {y}) {cell:?}"
            );
        }
        assert_eq!(points(&buffer).len(), 5);
        assert!(braille(&buffer).iter().all(|(_, y, _)| *y == 6), "not flat");
        let band = buffer.cell((9, 19)).expect("band 2's frequency");
        assert!(
            !band.modifier.contains(Modifier::DIM),
            "the band table is dimmed"
        );
    }
}

/// In Flat mode, the device's bypass, the graph draws the curve the bands hold, muted, with
/// their points on it — [`curve_of`] answers a muted curve rather than none, what the
/// spectrum then colours desaturated at true colour and keeps the dim role at 16 colours;
/// the band table is dimmed and says under its rows that the bands apply in EQ mode, except
/// the row the cursor is on, which keeps its selected style.
#[test]
fn in_flat_mode_the_graph_is_the_stored_curve_muted_and_the_bands_are_dimmed() {
    let app = in_mode(lifted_app(), 1, 127);
    let buffer = screen(&app);
    assert_rows(
        &buffer,
        &[
            " svolder  Gungnir 2  Preset custom    Mode ‹ Flat ›   Loudness ░░░░░ 0  mouse:on",
            "─ Controls ──┤ EQ ├── Telemetry ── Devices ───────────────── ? help ─ esc back ─",
            "┌ Response ────────────────────────────────────────── flat ─ presets ─ refresh ┐",
            "│ +6 │                     ⡰•                                                  │",
            "│    │                    ⢠⠃⠈⡆                                                 │",
            "│    │                  ⢀⡠⠊  ⠘⢄⡀                                               │",
            "│  0 │⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠉⠉⠁      ⠈⠉⠉⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒│",
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
            "│ ▶ 1   40.0 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "│   2    160 Hz    +6.0 dB    4.30      64 / 127 / 64                          │",
            "│   3    632 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "│   4   2515 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "│   5  10000 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "└ bands apply in EQ mode ──────────────────────────────────────────────────────┘",
        ],
    );
    let theme = Theme::new(false);
    assert!(
        braille(&buffer).iter().any(|(_, y, _)| *y < 6),
        "the stored curve did not rise above 0 dB for band 2's boost"
    );
    assert_eq!(
        tone::curve_of(buffer.area, &app),
        Some(tone::curve_area(buffer.area, 5))
    );
    assert!(tone::curve_muted(&app), "not reported muted");
    for (x, y, cell) in braille(&buffer).into_iter().chain(points(&buffer)) {
        assert!(cell.modifier.contains(Modifier::DIM), "({x}, {y}) {cell:?}");
    }
    // Row 18 is band 1, which the cursor is on: it keeps its selected style rather than
    // dimming; every other band row still dims.
    for y in 19..23 {
        let cell = buffer.cell((9, y)).expect("a band's frequency");
        assert!(
            cell.modifier.contains(Modifier::DIM),
            "row {y} is not dimmed"
        );
    }
    let focused = buffer.cell((9, 18)).expect("band 1's frequency");
    assert!(
        !focused.modifier.contains(Modifier::DIM),
        "the focused band row is dimmed: {focused:?}"
    );
    assert!(
        focused
            .modifier
            .contains(Modifier::BOLD | Modifier::REVERSED),
        "the focused band row lost its selected style: {focused:?}"
    );
    let hint = buffer.cell((2, 23)).expect("the hint's first letter");
    assert_eq!(hint.symbol(), "b");
    assert!(hint.modifier.contains(theme.style(Role::Dim).add_modifier));
}

/// In Loudness mode the graph draws no curve and no points, since the device's contour is
/// not known until it is measured: a line on the 0 dB row says what the DAC's own volume
/// leaves the loudness — nothing at the top step, a boost that grows below it with the
/// strength it is set to — and the band table is dimmed as in Flat mode, except the row the
/// cursor is on, which keeps its selected style.
#[test]
fn in_loudness_mode_a_line_of_text_takes_the_curves_place() {
    let mut app = in_mode(lifted_app(), 3, 127);
    hear(&mut app, vec![vo("12.52", 40)]);
    let buffer = screen(&app);
    assert!(braille(&buffer).is_empty(), "a curve in loudness mode");
    assert!(points(&buffer).is_empty(), "a point in loudness mode");
    assert_eq!(tone::curve_of(buffer.area, &app), None);
    let rows = text(&buffer);
    assert_eq!(
        rows[6],
        "│  0 │                        no effect at full volume                         │"
    );
    assert_eq!(
        rows[23],
        "└ bands apply in EQ mode ──────────────────────────────────────────────────────┘"
    );
    // Row 18 is band 1, which the cursor is on: it keeps its selected style rather than
    // dimming; every other band row still dims.
    for y in 19..23 {
        let cell = buffer.cell((9, y)).expect("a band's frequency");
        assert!(
            cell.modifier.contains(Modifier::DIM),
            "row {y} is not dimmed"
        );
    }
    let focused = buffer.cell((9, 18)).expect("band 1's frequency");
    assert!(
        !focused.modifier.contains(Modifier::DIM),
        "the focused band row is dimmed: {focused:?}"
    );

    hear(&mut app, vec![vo("10", 126)]);
    assert_eq!(
        text(&screen(&app))[6],
        "│  0 │ loudness active — the boost grows as the volume goes down · strength 40 │"
    );
    // A volume not read yet says what the mode does rather than guess the volume.
    app.readings.values.remove(&FunctionId::one(10));
    assert_eq!(
        tone::loudness_text(&app, app.profile.eq.as_ref().expect("a tone control")),
        "loudness active — the boost grows as the volume goes down · strength 40"
    );
}
