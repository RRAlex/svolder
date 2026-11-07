// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Instant;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use svolder_ipc::{ColorDepth, TuiConfig};
use svolder_profiles::EqSpec;
use svolder_profiles::tone::response::{F_MAX, F_MIN};
use svolder_proto::FunctionId;

use crossterm::event::{KeyCode, KeyEvent};

use crate::tui::app::{App, Intent, Screen, ToneRow};
use crate::tui::boxes::toast::ToastKind;
use crate::tui::mouse::{Hit, HitMap};
use crate::tui::render;
use crate::tui::test_support::{
    assert_rows, assert_usage_md_shows, frame_with_theme, hear, plain_vo as vo, profile, text,
    theme_for,
};
use crate::tui::theme::{Role, Theme};
use crate::tui::widgets;

/// An app on the tone-control screen, in EQ mode with the loudness curve off and every
/// band flat: raw 64 is each band's own centre frequency, 98 is 0 dB and 64 is Q 4.30.
fn flat_app() -> App {
    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let mut values = vec![vo("12.50", 2), vo("12.52", 0)];
    for band in 1..=5 {
        values.push(vo(&format!("12.{band}.2"), 64));
        values.push(vo(&format!("12.{band}.3"), 98));
        values.push(vo(&format!("12.{band}.1"), 64));
    }
    hear(&mut app, values);
    app.screen = Screen::Tone;
    app
}

/// [`flat_app`] with band 2 lifted to +6 dB, a curve the bands shape.
fn lifted_app() -> App {
    let mut app = flat_app();
    hear(&mut app, vec![vo("12.2.3", 127)]);
    app
}

/// Where on the header row the text `wanted` starts.
fn column_of(buffer: &Buffer, wanted: &str) -> u16 {
    let row: String = (0..buffer.area.width)
        .map(|x| buffer.cell((x, 0)).expect("a header cell").symbol())
        .collect();
    u16::try_from(
        row.find(wanted)
            .unwrap_or_else(|| panic!("no {wanted:?} in |{row}|")),
    )
    .expect("a column inside the frame")
}

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

/// The tone screen follows the tiers: at 126×62 its two blocks are a row apart, each keeps
/// a blank row at its top, the chart keeps two columns clear inside its border and the
/// band table two cells before its cursor; at 200×60 both blocks take the whole width, and
/// at 300×60 they are held to the 256 columns in the middle. The curve's cells and the frames it reports are where the draw
/// put them.
#[test]
fn the_tone_screen_follows_the_tiers() {
    let app = lifted_app();
    for (width, height, left) in [(126, 62, 0), (200, 60, 0), (300, 60, 22)] {
        let buffer = frame_with_theme(&app, width, height, &Theme::new(false));
        let rows = text(&buffer);
        let area = Rect::new(0, 0, width, height);
        let frames = super::frames_of(area, &app);
        let [(_, response), (_, bands)] =
            <[(crate::tui::render::FrameId, Rect); 2]>::try_from(frames).expect("two blocks");
        assert_eq!((response.x, response.width), (left, width.min(256)));
        assert_eq!(
            bands.y,
            response.bottom() + 1,
            "{width}×{height}: no row between"
        );
        for pane in [response, bands] {
            let corner = buffer.cell((pane.x, pane.y)).expect("a corner").symbol();
            assert_eq!(corner, "┌", "{width}×{height}: {pane:?}");
            let blank: String = rows[usize::from(pane.y + 1)]
                .chars()
                .skip(usize::from(pane.x + 1))
                .take(usize::from(pane.width - 2))
                .collect();
            assert!(blank.trim().is_empty(), "{width}×{height}: |{blank}|");
        }
        let band = &rows[usize::from(bands.y + 3)];
        let from = usize::from(bands.x);
        assert_eq!(
            band.chars().skip(from).take(5).collect::<String>(),
            "│  ▶ ",
            "{width}×{height}: |{band}|"
        );
        let gain = &rows[usize::from(response.y + 2)];
        assert_eq!(
            gain.chars().skip(from).take(7).collect::<String>(),
            "│   +6 ",
            "{width}×{height}: the chart's labels are not two cells in"
        );
        let curve = super::curve_of(area, &app).expect("a curve");
        assert!(
            (curve.x..curve.right()).any(|x| {
                (curve.y..curve.bottom()).any(|y| {
                    buffer
                        .cell((x, y))
                        .is_some_and(|cell| cell.symbol().chars().all(|c| ('⠁'..='⣿').contains(&c)))
                })
            }),
            "{width}×{height}: no curve in the cells reported for it"
        );
        assert!(
            curve.right() <= response.right() - 3,
            "{width}×{height}: {curve:?}"
        );
    }
}

/// One frame of `app` in the 80×24 terminal the mockup is drawn for.
fn screen(app: &App) -> Buffer {
    frame_with_theme(app, 80, 24, &Theme::new(false))
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

#[test]
fn the_tone_screen_draws_the_response_and_the_bands() {
    assert_rows(
        &screen(&flat_app()),
        &[
            " svolder  Gungnir 2  Preset Flat        Mode ‹ EQ ›   Loudness ░░░░░ 0  mouse:on",
            "─ Controls ──┤ EQ ├── Telemetry ── Devices ───────────────── ? help ─ esc back ─",
            "┌ Response ────────────────────────────────────────── flat ─ presets ─ refresh ┐",
            "│ +6 │                                                                         │",
            "│    │                                                                         │",
            "│    │                                                                         │",
            "│  0 │⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒⠒•⠒⠒⠒⠒⠒⠒⠒│",
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
            "│   2    160 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "│   3    632 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "│   4   2515 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "│   5  10000 Hz    +0.0 dB    4.30      64 / 98 / 64                           │",
            "└──────────────────────────────────────────────────────────────────────────────┘",
        ],
    );
}

/// With no key row the Response block takes the row: at 80×24 it is 14 rows tall and the Bands
/// block under it ends on the terminal's last row.
#[test]
fn the_blocks_run_down_to_the_last_row_at_80_by_24() {
    let blocks: Vec<(u16, u16)> = super::frames_of(Rect::new(0, 0, 80, 24), &flat_app())
        .iter()
        .map(|(_, block)| (block.y, block.height))
        .collect();
    assert_eq!(blocks, [(2, 14), (16, 8)]);
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

#[test]
fn the_loudness_and_response_title_hits_land_where_they_are_drawn() {
    let app = flat_app();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let theme = Theme::new(false);
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, &app, &theme, &mut hits);
        })
        .expect("the frame draws");
    let buffer = terminal.backend().buffer().clone();

    let loudness_x = (0..buffer.area.width)
        .find(|&x| buffer.cell((x, 0)).is_some_and(|cell| cell.symbol() == "░"))
        .expect("the loudness bar on the header row");
    // The bar's five cells set the strength under a press; they are the bar and no more.
    for x in loudness_x..loudness_x + 5 {
        assert_eq!(hits.hit_at(x, 0), Some(&Hit::LoudnessBar), "column {x}");
    }
    assert_eq!(hits.hit_at(loudness_x + 5, 0), Some(&Hit::Loudness));
    // The word beside the bar toggles the mode — the rest of the half is one target.
    let loudness_word = column_of(&buffer, "Loudness");
    assert_eq!(hits.hit_at(loudness_word, 0), Some(&Hit::Loudness));

    // The Mode select's own arrows still step it; the rest of its half toggles it.
    let mode = column_of(&buffer, "Mode");
    assert_eq!(hits.hit_at(mode, 0), Some(&Hit::Mode));
    let mode_prev = (mode..buffer.area.width)
        .find(|&x| buffer.cell((x, 0)).is_some_and(|cell| cell.symbol() == "‹"))
        .expect("the Mode select's ‹");
    assert_eq!(hits.hit_at(mode_prev, 0), Some(&Hit::ModePrev));
    let mode_next = (mode..buffer.area.width)
        .find(|&x| buffer.cell((x, 0)).is_some_and(|cell| cell.symbol() == "›"))
        .expect("the Mode select's ›");
    assert_eq!(hits.hit_at(mode_next, 0), Some(&Hit::ModeNext));

    // The Response block's own title leads back to the device screen.
    assert_eq!(hits.hit_at(2, 2), Some(&Hit::Tab(Screen::Device)));
}

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
        let curve = super::curve_area(Rect::new(0, 0, width, height), 5);
        for x in curve.x..curve.right() {
            let along = super::along(curve, super::frequency_at(curve, x).log10());
            assert!(
                (along - f64::from(x - curve.x)).abs() < 1e-9,
                "{size}: column {x} reads back as {along}"
            );
        }
        for y in curve.y..curve.bottom() {
            assert_eq!(
                super::row_at(curve, super::gain_at(curve, y)),
                Some(y),
                "{size}: row {y}"
            );
        }
        for band in 0..eq.bands.len() {
            for raw in 0..=EqSpec::STEP_MAX {
                let hz = eq.frequency(band, raw).expect("a band of the profile");
                let x = super::column_at(curve, hz.log10()).expect("on the axis");
                let found = eq
                    .frequency_raw(band, super::frequency_at(curve, x))
                    .expect("a band of the profile");
                let back = eq.frequency(band, found).expect("a band of the profile");
                assert_eq!(
                    super::column_at(curve, back.log10()),
                    Some(x),
                    "{size}: band {} step {raw} came back as step {found}",
                    band + 1
                );
            }
        }
        assert!(
            (super::frequency_at(curve, 0) - F_MIN).abs() < 1e-6,
            "{size}"
        );
        assert!(
            (super::frequency_at(curve, curve.right() + 5) - F_MAX).abs() < 1e-6,
            "{size}"
        );
        assert_eq!(super::gain_at(curve, 0), super::GAIN_CEILING_DB, "{size}");
        assert_eq!(
            super::gain_at(curve, curve.bottom() + 3),
            super::GAIN_FLOOR_DB,
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
        let curve = super::curve_area(Rect::new(0, 0, width, height), 5);
        let left = curve.x - super::GAIN_LABEL_WIDTH - 1;
        let label = |y: u16| -> String {
            (left..left + super::GAIN_LABEL_WIDTH)
                .map(|x| buffer.cell((x, y)).expect("a label cell").symbol())
                .collect()
        };
        let rows: Vec<u16> = super::GAIN_LABELS
            .iter()
            .map(|&(db, text)| {
                let y = super::row_at(curve, db).expect("a gain on the chart");
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
        let curve = super::curve_area(Rect::new(0, 0, width, height), 5);
        let points = super::points(&app, curve);
        assert_eq!(points.len(), 5, "{size}");
        for &(band, (x, _)) in &points {
            let hz = u8::try_from(bands[band].0).expect("a step");
            assert_ne!(
                eq.frequency_raw(band, super::frequency_at(curve, x)),
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
    let curve = super::curve_area(buffer.area, 5);
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

/// Every palette preset at every colour depth draws this screen's own conventions: the
/// accent rule under the header in the accent the theme resolved, the bands' bottom border
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
            let buffer = frame_with_theme(&flat_app(), 80, 24, &theme);

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
                "{drawn}: the bands do not end on the last row"
            );
        }
    }
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

/// `app` with the tone control in `mode` (its raw value) and the volume at `volume`.
fn in_mode(mut app: App, mode: i64, volume: i64) -> App {
    hear(&mut app, vec![vo("12.50", mode), vo("10", volume)]);
    app
}

/// The cells of the curve's area that draw a Braille dot, with the style each is drawn in.
fn braille(buffer: &Buffer) -> Vec<(u16, u16, ratatui::buffer::Cell)> {
    let curve = super::curve_area(buffer.area, 5);
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
    let curve = super::curve_area(buffer.area, 5);
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
        super::curve_of(buffer.area, &app).is_some(),
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
            super::curve_of(buffer.area, &app),
            Some(super::curve_area(buffer.area, 5))
        );
        assert!(super::curve_muted(&app), "not reported muted");
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
        super::curve_of(buffer.area, &app),
        Some(super::curve_area(buffer.area, 5))
    );
    assert!(super::curve_muted(&app), "not reported muted");
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
    assert_eq!(super::curve_of(buffer.area, &app), None);
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
        super::loudness_text(&app, app.profile.eq.as_ref().expect("a tone control")),
        "loudness active — the boost grows as the volume goes down · strength 40"
    );
}

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

/// One frame of `app` in a terminal `width` by `height` drawn with `theme`, where it put its
/// pieces, and where it left the terminal's own cursor.
fn framed(
    app: &App,
    (width, height): (u16, u16),
    theme: &Theme,
) -> (Buffer, render::FrameAreas, (u16, u16)) {
    let mut terminal =
        Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut areas = render::FrameAreas::default();
    terminal
        .draw(|f| areas = render::draw(f, app, theme, &mut hits))
        .expect("the frame draws");
    let cursor = terminal.get_cursor_position().expect("the cursor");
    (
        terminal.backend().buffer().clone(),
        areas,
        (cursor.x, cursor.y),
    )
}

/// Press each key of `keys` in turn: a character for each of `text`'s, bar the named keys.
fn keyed(app: &mut App, keys: &[KeyCode]) {
    for code in keys {
        app.key(KeyEvent::from(*code));
    }
}

/// Type `text` a character at a time.
fn type_text(app: &mut App, text: &str) {
    for letter in text.chars() {
        app.key(KeyEvent::from(KeyCode::Char(letter)));
    }
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

/// The mockups docs/usage/tui.md shows of the tone-control screen are the frames this build draws:
/// the flat curve, the stored curve in Flat mode, and band 2's gain being typed.
#[test]
fn the_docs_show_the_tone_screens_this_build_draws() {
    use crate::tui::test_support::assert_usage_md_shows;

    assert_usage_md_shows(&text(&screen(&flat_app())), "tone-control screen");
    assert_usage_md_shows(
        &text(&screen(&in_mode(lifted_app(), 1, 127))),
        "tone-control screen in Flat mode",
    );
    let mut app = flat_app();
    keyed(
        &mut app,
        &[KeyCode::Char('2'), KeyCode::Enter, KeyCode::Tab],
    );
    type_text(&mut app, "1.5");
    let (buffer, _, _) = framed(&app, (80, 24), &Theme::new(false));
    assert_usage_md_shows(&text(&buffer), "tone-control screen's edit line");
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

/// [`lifted_app`] with the imported preset `Phones` in the file and loaded with changes: the
/// bands are not what the preset fits and match no preset, so the header reads `Phones*`.
fn imported_app() -> App {
    let mut app = lifted_app();
    app.presets.saved = vec![crate::tui::test_support::presets::imported("Phones")];
    app.presets.loaded_preset = Some("Phones".into());
    app
}

/// [`imported_app`] with the bands on what the preset fits, so `Phones` is on the device.
fn fitted_app() -> App {
    let mut app = imported_app();
    let eq = app.profile.eq.clone().expect("a tone control");
    let imported = app.presets.saved[0]
        .source
        .clone()
        .expect("an imported preset");
    let fit = svolder_profiles::tone::preset::fit_for(&eq, &imported);
    let mut values = Vec::new();
    for (band, values_of) in fit.bands.iter().enumerate() {
        for (param, value) in [
            (svolder_profiles::EqParam::Frequency, values_of.f),
            (svolder_profiles::EqParam::Gain, values_of.g),
            (svolder_profiles::EqParam::Q, values_of.q),
        ] {
            let id = eq.band_id(band, param).expect("a band of the profile");
            let raw = eq.step(band, param, value).expect("a step");
            values.push(vo(&id.to_string(), i64::from(raw)));
        }
    }
    hear(&mut app, values);
    app
}

/// The cells of `buffer` inside `curve` that hold one of `symbols`.
fn cells_of(buffer: &Buffer, curve: Rect, symbols: &str) -> Vec<(u16, u16)> {
    let mut found = Vec::new();
    for y in curve.y..curve.bottom() {
        for x in curve.x..curve.right() {
            let cell = buffer.cell((x, y)).expect("a cell of the curve");
            if cell
                .symbol()
                .chars()
                .next()
                .is_some_and(|c| symbols.contains(c))
            {
                found.push((x, y));
            }
        }
    }
    found
}

/// The quadrant glyphs the target is dotted in.
const QUADRANTS: &str = "▖▗▘▙▚▛▜▝▞▟";

/// The header's text row of `app` drawn `width` by `height`: the wordmark takes the rows above it
/// where there is room for one.
fn header_row(app: &App, width: u16, height: u16) -> String {
    text(&frame_with_theme(app, width, height, &Theme::new(false)))
        .into_iter()
        .find(|row| row.contains("mouse:"))
        .expect("the header row")
}

/// The target and the gap are drawn only for an imported preset that is on the device or loaded
/// with changes: not for a band preset, not for no preset at all, and not while the imported one
/// is only being loaded.
#[test]
fn the_target_is_drawn_for_an_imported_preset_only() {
    let theme = theme_for("amber", ColorDepth::TrueColor);
    let curve = super::curve_area(Rect::new(0, 0, 80, 24), 5);
    let drawn = |app: &App| cells_of(&frame_with_theme(app, 80, 24, &theme), curve, QUADRANTS);

    assert!(
        drawn(&flat_app()).is_empty(),
        "no imported preset, no target"
    );
    let mut warm = flat_app();
    warm.presets.loaded_preset = Some("Warm".into());
    assert!(drawn(&warm).is_empty(), "a built-in has no target");
    let changed = imported_app();
    assert!(
        !drawn(&changed).is_empty(),
        "loaded with changes draws the target"
    );
    assert!(
        !drawn(&fitted_app()).is_empty(),
        "on the device draws the target"
    );
    let mut other = imported_app();
    other.presets.loaded_preset = Some("Other".into());
    assert!(
        drawn(&other).is_empty(),
        "a preset that is not in the file has none"
    );
}

/// The target is dotted in the dim target role and never in a Braille cell, which the spectrum
/// recolours, nor in the dot a band's centre is marked with.
#[test]
fn the_target_is_dim_and_not_in_braille_cells() {
    let theme = theme_for("amber", ColorDepth::TrueColor);
    let app = imported_app();
    let buffer = frame_with_theme(&app, 80, 24, &theme);
    let curve = super::curve_area(Rect::new(0, 0, 80, 24), 5);
    let want = theme.style(Role::CurveTarget).fg.expect("a colour");
    let dots = cells_of(&buffer, curve, QUADRANTS);
    assert!(!dots.is_empty());
    for (x, y) in dots {
        assert_eq!(buffer.cell((x, y)).unwrap().fg, want, "({x}, {y})");
    }
}

/// The gap is filled in the warning colour where the fit misses the target by more than 2 dB,
/// and in no column where it does not: flat bands against the preset's +5 dB bass shelf miss
/// it, and the preset's own fit does not.
#[test]
fn the_gap_is_filled_in_the_warning_colour_where_the_miss_exceeds_two_db() {
    let theme = theme_for("amber", ColorDepth::TrueColor);
    let curve = super::curve_area(Rect::new(0, 0, 80, 24), 5);
    let warn = theme.style(Role::CurveGap).fg.expect("a colour");
    let filled = |app: &App| -> Vec<(u16, u16)> {
        let buffer = frame_with_theme(app, 80, 24, &theme);
        cells_of(&buffer, curve, "░")
            .into_iter()
            .filter(|&(x, y)| buffer.cell((x, y)).unwrap().fg == warn)
            .collect()
    };
    let miss = filled(&imported_app());
    assert!(
        !miss.is_empty(),
        "the lifted band misses a +5 dB shelf and a notch"
    );
    assert!(
        filled(&fitted_app()).is_empty(),
        "the preset's own fit is within 2 dB"
    );
}

/// The gap is drawn under both curves: in a column where the fill reaches the row of the fit's
/// own line, the cell on that row holds the line's Braille (or a band's handle), not the fill.
#[test]
fn the_curve_is_drawn_over_the_gap() {
    let theme = theme_for("amber", ColorDepth::TrueColor);
    let app = imported_app();
    let buffer = frame_with_theme(&app, 80, 24, &theme);
    let curve = super::curve_area(Rect::new(0, 0, 80, 24), 5);
    let zero = super::zero_row(curve);
    let symbol = |x: u16, y: u16| buffer.cell((x, y)).unwrap().symbol().to_string();
    let is_braille = |text: &str| {
        text.chars()
            .next()
            .is_some_and(|c| ('\u{2801}'..='\u{28ff}').contains(&c))
    };
    // The bass bands sit at 0 dB under the preset's +5 dB shelf, so the fill runs down to the
    // 0 dB row, where the fit's line is drawn, in the columns the shelf covers.
    let bass = super::column_at(curve, 50f64.log10()).unwrap();
    let covered: Vec<u16> = (curve.x..bass)
        .filter(|&x| symbol(x, zero - 1) == "░")
        .collect();
    assert!(!covered.is_empty(), "the fill reaches the fit's row");
    for x in covered {
        let there = symbol(x, zero);
        assert!(
            is_braille(&there) || there == "•",
            "column {x}: {there:?} is not the fit's line or a handle"
        );
    }
}

/// The header says how well the bands fit — `fit 0.9 dB` — after the preset's name, for an
/// imported preset only, and a band preset's header is as it was.
#[test]
fn the_header_names_the_fit_of_an_imported_preset() {
    let app = fitted_app();
    let rms = {
        let eq = app.profile.eq.as_ref().unwrap();
        let imported = app.presets.saved[0].source.as_ref().unwrap();
        svolder_profiles::tone::preset::fit_for(eq, imported).rms_db
    };
    let at = |app: &App, width| header_row(app, width, 30);
    let header = at(&app, 126);
    let want = format!("Preset Phones  fit {rms:.1} dB");
    assert!(header.contains(&want), "|{header}|");
    assert!(!at(&app, 80).contains("fit "), "no room at 80 columns");
    let changed = at(&imported_app(), 126);
    assert!(
        changed.contains(&format!("Preset Phones*  fit {rms:.1} dB")),
        "|{changed}|"
    );
    let mut warm = flat_app();
    warm.presets.loaded_preset = Some("Warm".into());
    let header = at(&warm, 126);
    assert!(!header.contains("fit "), "|{header}|");
}

/// The fit leaves the header where there is no room for it whole: the name is cut first.
#[test]
fn the_fit_gives_way_in_a_crowded_header() {
    let mut app = fitted_app();
    app.session.product = "Forkbeard Gungnir 2 Limited Anniversary Edition".into();
    let header = header_row(&app, 80, 24);
    assert!(!header.contains("fit "), "|{header}|");
    assert!(header.trim_end().ends_with("mouse:on"), "|{header}|");
}

/// The side pane is only in the wide tier: the source and the filters, the preamp and the fit
/// line. The tiers below have the chart alone, as wide as ever.
#[test]
fn the_side_pane_is_in_the_wide_tier_only() {
    let app = fitted_app();
    let wide = frame_with_theme(&app, 126, 40, &Theme::new(false));
    let rows = text(&wide).join("\n");
    for wanted in [
        "Correction",
        "Imported · 2 filters",
        "LSC   105",
        "+5.0",
        "Preamp -3.0 dB",
        "fit ",
    ] {
        assert!(rows.contains(wanted), "no {wanted:?} in\n{rows}");
    }
    for (width, height) in [(80, 24), (119, 39), (100, 45)] {
        let rows = text(&frame_with_theme(&app, width, height, &Theme::new(false))).join("\n");
        assert!(!rows.contains("Correction"), "{width}×{height}\n{rows}");
        assert!(!rows.contains("Imported ·"), "{width}×{height}");
    }
    // A band preset has no side pane at any size.
    let rows = text(&frame_with_theme(&flat_app(), 126, 40, &Theme::new(false))).join("\n");
    assert!(!rows.contains("Correction"));
}

/// The side pane takes columns from the chart, and everything read back from the layout — the
/// curve's cells, the frames — follows the chart.
#[test]
fn the_curve_is_narrower_beside_the_side_pane() {
    let area = Rect::new(0, 0, 126, 40);
    let plain = flat_app();
    let with = fitted_app();
    let paint = |app: &App| {
        let mut terminal = Terminal::new(TestBackend::new(126, 40)).expect("a terminal");
        let mut hits = HitMap::default();
        let mut areas = render::FrameAreas::default();
        terminal
            .draw(|f| areas = render::draw(f, app, &Theme::new(false), &mut hits))
            .expect("the frame draws");
        areas
    };
    let (a, b) = (paint(&plain), paint(&with));
    let (a, b) = (a.curve.expect("a curve"), b.curve.expect("a curve"));
    assert_eq!(a, super::curve_area(area, 5));
    assert_eq!(b.width + super::fit::SIDE_WIDTH + 1, a.width);
    assert_eq!(b.x, a.x);
}

/// `Host EQ NAME` is in the wide tier's header while the computer EQ is on, and in no other
/// tier, nor once it is off.
#[test]
fn the_wide_header_names_the_computer_eq_while_it_is_on() {
    let mut app = flat_app();
    app.host.computer.eq = Some(host_eq_view(true));
    let wide = header_row(&app, 170, 40);
    assert!(wide.contains("Host EQ Phones"), "|{wide}|");
    let narrow = header_row(&app, 80, 24);
    assert!(!narrow.contains("Host EQ"), "|{narrow}|");
    app.host.computer.eq = Some(host_eq_view(false));
    let off = header_row(&app, 170, 40);
    assert!(!off.contains("Host EQ"), "|{off}|");
}

/// What the computer EQ's view holds for `Phones`.
fn host_eq_view(on: bool) -> Box<svolder_host::eq::EqView> {
    let mut eq = crate::tui::test_support::computer_eq(on, &[("PK", 1000.0, 2.0, 1.0)]);
    eq.preset = Some("Phones".into());
    eq
}

/// The imported preset's view is worked out once and kept: the same one comes back until the
/// presets are read again.
#[test]
fn the_target_view_is_kept_between_frames_and_dropped_with_the_presets() {
    let app = imported_app();
    let first = app.target_view("Phones").expect("an imported preset");
    let again = app.target_view("Phones").expect("kept");
    assert!(std::sync::Arc::ptr_eq(&first, &again));
    assert!(app.target_view("Warm").is_none(), "not an imported preset");
    assert!(
        app.target_view("Phones").is_some(),
        "the miss did not drop it"
    );
    app.tone.target_cache.clear();
    let after = app.target_view("Phones").expect("worked out again");
    assert!(!std::sync::Arc::ptr_eq(&first, &after));
}

/// The target is read at any frequency off the points it is kept at: the preset's +5 dB shelf
/// is flat above its corner and its own response where the shelf bends, and nothing outside the
/// audio band.
#[test]
fn the_target_reads_back_the_correction_at_any_frequency() {
    let app = imported_app();
    let view = app.target_view("Phones").expect("an imported preset");
    let source = app.presets.saved[0].source.as_ref().unwrap();
    for hz in [20.0, 37.0, 105.0, 800.0, 3000.0, 9000.0, 20000.0] {
        let want = svolder_profiles::tone::response::correction_db(&source.correction, &[hz])[0];
        let got = view.db_at(hz).expect("inside the band");
        assert!((got - want).abs() < 0.05, "{hz} Hz: {got} against {want}");
    }
    assert_eq!(view.db_at(10.0), None);
    assert_eq!(view.db_at(30000.0), None);
}

/// docs/usage/tui.md shows the tone screen with an imported preset loaded with changes at 80×24,
/// and the side pane the wide tier adds, byte for byte what this build draws.
#[test]
fn usage_md_shows_the_target_and_the_side_pane_this_build_draws() {
    let app = imported_app();
    let narrow = text(&frame_with_theme(&app, 80, 24, &Theme::new(false)));
    assert_usage_md_shows(&narrow, "tone-control screen with an imported preset");
    let wide = text(&frame_with_theme(&app, 126, 30, &Theme::new(false)));
    let pane: Vec<String> = wide[3..12]
        .iter()
        .map(|row| {
            row.chars()
                .skip(126 - usize::from(super::fit::SIDE_WIDTH))
                .collect()
        })
        .collect();
    assert_usage_md_shows(&pane, "tone-control screen's side pane");
}

/// Every palette tier draws the target and the gap in a way that reads: true colour and 256
/// colours in the dim and warning colours, 16 colours in the dim attribute and the terminal's
/// yellow, and monochrome with no colour at all, the gap dim so it is not a solid bar.
#[test]
fn the_target_and_the_gap_read_at_every_depth_and_in_monochrome() {
    let app = imported_app();
    let curve = super::curve_area(Rect::new(0, 0, 80, 24), 5);
    let mono = Theme::from_config(
        &TuiConfig {
            monochrome: true,
            ..TuiConfig::default()
        },
        false,
        ColorDepth::TrueColor,
    );
    let tiers = [
        ("true colour", theme_for("amber", ColorDepth::TrueColor)),
        ("256 colours", theme_for("amber", ColorDepth::Ansi256)),
        ("16 colours", theme_for("amber", ColorDepth::Ansi16)),
        ("monochrome", mono),
    ];
    for (what, theme) in tiers {
        let buffer = frame_with_theme(&app, 80, 24, &theme);
        let target = cells_of(&buffer, curve, QUADRANTS);
        let gap = cells_of(&buffer, curve, "░");
        assert!(!target.is_empty(), "{what}: no target");
        assert!(!gap.is_empty(), "{what}: no gap");
        for (x, y) in target {
            let cell = buffer.cell((x, y)).unwrap();
            assert_eq!(
                cell.fg,
                theme.style(Role::CurveTarget).fg.unwrap_or(cell.fg),
                "{what}"
            );
            assert_eq!(
                cell.modifier.contains(Modifier::DIM),
                theme
                    .style(Role::CurveTarget)
                    .add_modifier
                    .contains(Modifier::DIM),
                "{what}"
            );
        }
    }
    let sixteen = theme_for("amber", ColorDepth::Ansi16);
    assert_eq!(
        sixteen.style(Role::CurveGap).fg,
        sixteen.style(Role::Warn).fg,
        "the gap is the warning colour"
    );
    assert_ne!(sixteen.style(Role::CurveGap).fg, None);
    assert!(
        sixteen
            .style(Role::CurveTarget)
            .add_modifier
            .contains(Modifier::DIM)
    );
}

/// A preset rewritten under the same name — by another front end, or `svolder eq import` — shows
/// its new target: the view is keyed by the correction as well as the name.
#[test]
fn the_same_name_with_new_filters_shows_the_new_target() {
    use svolder_profiles::tone::apo::{FilterKind, SourceFilter};
    let mut app = imported_app();
    let before = app.target_view("Phones").expect("an imported preset");
    let source = app.presets.saved[0].source.as_mut().expect("imported");
    source.correction.filters = vec![SourceFilter {
        kind: FilterKind::Peaking,
        f: 1000.0,
        g: 4.0,
        q: 1.0,
    }];
    let after = app.target_view("Phones").expect("still imported");
    assert!(!std::sync::Arc::ptr_eq(&before, &after), "a stale view");
    assert!(after.db_at(1000.0).unwrap() > 3.5);
    assert!(before.db_at(1000.0).unwrap() < 1.0);
    assert_eq!(after.filters.len(), 1);
}

/// The layout the mouse reads follows the terminal it is asked about: right after a resize into
/// or out of the wide tier, before any frame is drawn, the curve is as narrow as the next frame
/// will draw it.
#[test]
fn the_curve_follows_a_resize_before_the_next_frame() {
    let app = fitted_app();
    // One frame at the wide size records that a target is shown.
    frame_with_theme(&app, 126, 30, &Theme::new(false));
    let wide = super::curve_of(Rect::new(0, 0, 126, 30), &app).unwrap();
    let narrow = super::curve_of(Rect::new(0, 0, 100, 30), &app).unwrap();
    assert_eq!(narrow, super::curve_area(Rect::new(0, 0, 100, 30), 5));
    assert_eq!(
        wide.width + super::fit::SIDE_WIDTH + 1,
        super::curve_area(Rect::new(0, 0, 126, 30), 5).width
    );
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
        if let Some(text) = super::preset_text(Some(&standing), room) {
            assert!(widgets::text::cells(&text) <= room, "room {room}: |{text}|");
        }
    }
    // `  ` and `*` leave seven cells for a name of twelve: three glyphs and the `…`.
    assert_eq!(
        super::preset_text(Some(&standing), 10).as_deref(),
        Some("  世界世…*")
    );
}

/// [`flat_app`] with the computer's controls there to flip to: three points, on.
fn computer_app() -> App {
    let mut app = flat_app();
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq(
        true,
        &[
            ("PK", 1000.0, 3.0, 1.4),
            ("LSC", 100.0, -4.0, 0.7),
            ("HSC", 8000.0, 5.0, 0.7),
        ],
    ));
    app
}

/// One frame of `app`, as rows of text.
fn rows_of(app: &App, width: u16, height: u16) -> Vec<String> {
    text(&frame_with_theme(app, width, height, &Theme::new(false)))
}

/// Whether any row of `rows` holds `needle`.
fn holds(rows: &[String], needle: &str) -> bool {
    rows.iter().any(|row| row.contains(needle))
}

/// The top line names the shown device's own name and the computer, whichever side is shown, and
/// reads as the DAC's own line where the computer's controls are not there.
#[test]
fn the_eq_tab_reads_eq_for_the_device_or_computer() {
    let mut app = computer_app();
    let name = app.session.product.clone();
    let line = format!("EQ for ‹ {name} | Computer ›");
    assert!(holds(&rows_of(&app, 120, 30), &line), "{line}");
    app.key(KeyEvent::from(KeyCode::Char('C')));
    assert!(app.on_computer());
    assert!(holds(&rows_of(&app, 120, 30), &line));
    let bare = flat_app();
    assert!(!holds(&rows_of(&bare, 120, 30), "EQ for"));
}

/// `C` flips the side and writes nothing, and a click on either name of the switch shows that
/// side.
#[test]
fn c_and_a_click_flip_the_side() {
    let mut app = computer_app();
    assert!(app.key(KeyEvent::from(KeyCode::Char('C'))).is_empty());
    assert!(app.on_computer());
    assert!(app.key(KeyEvent::from(KeyCode::Char('C'))).is_empty());
    assert!(!app.on_computer());
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, &app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    let (y, row) = text(terminal.backend().buffer())
        .into_iter()
        .enumerate()
        .find(|(_, row)| row.contains("EQ for"))
        .expect("the switch is drawn");
    let y = u16::try_from(y).unwrap();
    let byte = row.find("Computer").expect("the switch is drawn");
    let at = u16::try_from(row[..byte].chars().count()).unwrap();
    assert_eq!(
        hits.hit_at(at, y),
        Some(&Hit::Side(crate::tui::app::Side::Computer))
    );
    let click = crossterm::event::MouseEvent {
        kind: crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left),
        column: at,
        row: y,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    assert!(matches!(
        crate::tui::mouse::resolve(&hits, click, &app),
        Some(crate::tui::mouse::MouseAction::Key(KeyCode::Char('C')))
    ));
}

/// On the computer the mode is written `‹ Off | EQ ›` and there is no Loudness.
#[test]
fn the_computer_mode_reads_off_or_eq_with_no_loudness() {
    let mut app = computer_app();
    assert!(holds(&rows_of(&app, 120, 30), "Loudness"));
    app.key(KeyEvent::from(KeyCode::Char('C')));
    let rows = rows_of(&app, 120, 30);
    assert!(holds(&rows, "Mode ‹ Off | EQ ›"), "{rows:?}");
    assert!(!holds(&rows, "Loudness"));
}

/// The Points pane has the columns, a row per point in its type, and its title carries the count.
#[test]
fn the_points_pane_has_type_freq_gain_q() {
    let mut app = computer_app();
    app.key(KeyEvent::from(KeyCode::Char('C')));
    let rows = rows_of(&app, 120, 30);
    for word in ["Points · computer", "3 of 16", "#  Type  Freq", "Gain", "Q"] {
        assert!(holds(&rows, word), "{word}: {rows:?}");
    }
    for kind in ["PK", "LSC", "HSC"] {
        assert!(holds(&rows, &format!("  {kind} ")), "{kind}");
    }
    assert!(holds(&rows, "preamp \u{2212}6.0 dB"));
}

/// Sixteen points do not grow the table: it scrolls, keeping the focused row in view, with a mark
/// for the rows out of sight.
#[test]
fn sixteen_points_scroll_with_the_selection_in_view() {
    let mut app = computer_app();
    let sixteen: Vec<(&str, f64, f64, f64)> = (0..16)
        .map(|n| ("PK", 100.0 * f64::from(n + 1), 1.0, 1.0))
        .collect();
    app.host.computer.eq = Some(crate::tui::test_support::computer_eq(true, &sixteen));
    app.key(KeyEvent::from(KeyCode::Char('C')));
    let top = rows_of(&app, 120, 30);
    assert!(holds(&top, "↓ more") && !holds(&top, "↑ more"), "{top:?}");
    assert!(holds(&top, "16 of 16"));
    app.focus_point(15);
    let bottom = rows_of(&app, 120, 30);
    assert!(holds(&bottom, "↑ more") && !holds(&bottom, "↓ more"));
    assert!(
        bottom
            .iter()
            .any(|row| row.contains("▶") && row.contains("1600 Hz")),
        "the last point is in view and marked: {bottom:?}"
    );
    assert_eq!(
        rows_of(&app, 120, 30).len(),
        top.len(),
        "the frame keeps its size"
    );
}

/// The Correction pane says nothing is fitted on the computer, in the wide tier.
#[test]
fn the_correction_pane_says_nothing_is_fitted_on_the_computer() {
    let mut app = computer_app();
    app.key(KeyEvent::from(KeyCode::Char('C')));
    let rows = rows_of(&app, 140, 40);
    assert!(holds(&rows, "Correction"), "{rows:?}");
    let joined = rows.join(" ");
    let joined: Vec<&str> = joined.split_whitespace().collect();
    let said = "on the computer nothing is fitted: every filter plays as written";
    for word in said.split_whitespace() {
        assert!(joined.contains(&word), "{word}");
    }
}

/// A chain whose running could not be told is shown as the state file says and marked not
/// verified; a verified one carries no mark.
#[test]
fn an_unknown_chain_is_marked_not_verified() {
    let mut app = computer_app();
    app.key(KeyEvent::from(KeyCode::Char('C')));
    assert!(!holds(&rows_of(&app, 120, 30), "not verified"));
    app.host
        .computer
        .eq
        .as_mut()
        .expect("an eq")
        .running_unknown = true;
    let rows = rows_of(&app, 120, 30);
    assert!(holds(&rows, "Mode ‹ Off | EQ › not verified"), "{rows:?}");
}

/// A click on a point's type cell cycles it, one on a value focuses the point; the cells are hits.
#[test]
fn a_point_s_cells_are_clickable() {
    let mut app = computer_app();
    app.key(KeyEvent::from(KeyCode::Char('C')));
    let mut terminal = Terminal::new(TestBackend::new(120, 30)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    terminal
        .draw(|f| {
            render::draw(f, &app, &Theme::new(false), &mut hits);
        })
        .expect("the frame draws");
    let rows = text(terminal.backend().buffer());
    let y = rows
        .iter()
        .position(|row| row.contains(" LSC "))
        .expect("the LSC row");
    let byte = rows[y].find("LSC").expect("its type");
    let x = u16::try_from(rows[y][..byte].chars().count()).unwrap();
    assert_eq!(
        hits.hit_at(x, u16::try_from(y).unwrap()),
        Some(&Hit::PointField(1, 0))
    );
    let intents = app.click_point_field(1, 0);
    assert!(
        matches!(&intents[..], [Intent::HostEq(op)] if matches!(&**op, svolder_ipc::HostEqOp::Set { index: 2, kind: Some(k), .. } if k == "HSC"))
    );
}
