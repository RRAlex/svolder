// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone screen's layout: its tiers, blocks, frames and titles.

use super::*;

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
        let frames = tone::frames_of(area, &app);
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
        let curve = tone::curve_of(area, &app).expect("a curve");
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
    let blocks: Vec<(u16, u16)> = tone::frames_of(Rect::new(0, 0, 80, 24), &flat_app())
        .iter()
        .map(|(_, block)| (block.y, block.height))
        .collect();
    assert_eq!(blocks, [(2, 14), (16, 8)]);
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

/// The layout the mouse reads follows the terminal it is asked about: right after a resize into
/// or out of the wide tier, before any frame is drawn, the curve is as narrow as the next frame
/// will draw it.
#[test]
fn the_curve_follows_a_resize_before_the_next_frame() {
    let app = fitted_app();
    // One frame at the wide size records that a target is shown.
    frame_with_theme(&app, 126, 30, &Theme::new(false));
    let wide = tone::curve_of(Rect::new(0, 0, 126, 30), &app).unwrap();
    let narrow = tone::curve_of(Rect::new(0, 0, 100, 30), &app).unwrap();
    assert_eq!(narrow, tone::curve_area(Rect::new(0, 0, 100, 30), 5));
    assert_eq!(
        wide.width + tone::fit::SIDE_WIDTH + 1,
        tone::curve_area(Rect::new(0, 0, 126, 30), 5).width
    );
}

/// At 80 by 24 the Response, the Points table and the Bands table stay whole under the two-row
/// header, on both sides.
#[test]
fn the_tone_screen_at_80_by_24_keeps_its_blocks_whole() {
    let mut app = computer_app();
    app.screen = Screen::Tone;
    let rows = rows_of(&app, 80, 24);
    assert!(holds(&rows, "┌ Response "), "{rows:#?}");
    assert!(holds(&rows, "│   # "), "{rows:#?}");
    assert!(holds(&rows, "10000 Hz"), "{rows:#?}");
    assert!(rows.last().is_some_and(|r| r.starts_with('└')), "{rows:#?}");
    assert!(
        rows.iter().filter(|r| r.contains("Hz")).count() >= 5,
        "{rows:#?}"
    );
    assert!(app.drawn.tone_switch.get());
    app.key(KeyEvent::from(KeyCode::Char('C')));
    let rows = rows_of(&app, 80, 24);
    assert!(holds(&rows, "┌ Points · computer"), "{rows:#?}");
    assert!(holds(&rows, "│   #  Type"), "{rows:#?}");
    assert!(
        rows.iter()
            .filter(|r| r.contains(" PK ") || r.contains(" LSC ") || r.contains(" HSC "))
            .count()
            >= 3,
        "{rows:#?}"
    );
}
