// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Instant;

use ratatui::style::Style;
use ratatui::text::Span;

use super::*;
use crate::tui::test_support::sixteen_colour;

/// A bar at the 8-cell minimum with the Aegir 2's quarter of class-A block: two block cells,
/// the tick on the second, a live part of six cells empty at a reading just above the zero,
/// and a peak at full scale marked on the last cell, never on the block.
#[test]
fn a_narrow_bar_keeps_its_block_its_tick_and_a_live_cell() {
    let control = crate::tui::test_support::with_log_scale(
        crate::tui::test_support::profile_named("vidar2"),
        crate::tui::test_support::AEGIR_SCALE,
    )
    .control_by_key("meter_1")
    .expect("a meter")
    .clone();
    let reading = Reading {
        value: svolder_ipc::ValueObject {
            heard_at_ms: None,
            id: "20.1".into(),
            key: Some("meter_1".into()),
            label: "Meter L".into(),
            value_type: "u16".into(),
            raw: serde_json::json!(161),
            display: "161".into(),
            value: None,
            unit: None,
        },
        received: Instant::now(),
        rate: Default::default(),
    };
    let lines = meter_lines(
        &control,
        Some(&reading),
        &sixteen_colour(),
        Columns {
            field: 8,
            bar: 8,
            room: 24,
        },
        MeterState {
            placed: Some(svolder_profiles::Placed::Log {
                zero: 160,
                full: 350,
                class_a: 0.25,
            }),
            live: true,
            eased: None,
            peak: Some(1.0),
        },
        1,
    );
    let bar: String = lines[0]
        .spans
        .iter()
        .flat_map(|span| span.content.chars())
        .filter(|c| matches!(c, '▓' | '┃' | '█' | '░' | '▎'))
        .collect();
    assert_eq!(bar, "▓┃░░░░░▎");
}

/// The class-A block takes at least one cell of a bar and never all of it, whatever fraction
/// the profile names, so the tick and a live cell both stay on the bar.
#[test]
fn the_class_a_block_takes_at_least_a_cell_and_leaves_a_live_one() {
    let control = crate::tui::test_support::profile_named("vidar2")
        .control_by_key("meter_1")
        .expect("a meter")
        .clone();
    let bar_of = |class_a: f64| -> String {
        let lines = meter_lines(
            &control,
            None,
            &sixteen_colour(),
            Columns {
                field: 8,
                bar: 8,
                room: 24,
            },
            MeterState {
                placed: Some(svolder_profiles::Placed::Log {
                    zero: 160,
                    full: 350,
                    class_a,
                }),
                live: true,
                eased: None,
                peak: None,
            },
            1,
        );
        lines[0]
            .spans
            .iter()
            .flat_map(|span| span.content.chars())
            .filter(|c| matches!(c, '▓' | '┃' | '█' | '░' | '▎'))
            .collect()
    };
    assert_eq!(bar_of(0.01), "┃░░░░░░░");
    assert_eq!(bar_of(0.99), "▓▓▓▓▓▓┃░");
}

/// One meter row of the `vidar2` meter on the Aegir 2's scale, at the given raw reading, bar
/// width, liveness and peak, and its drawn spans.
fn aegir_meter_spans(
    theme: &Theme,
    raw: i64,
    bar: usize,
    live: bool,
    peak: Option<f64>,
) -> Vec<Span<'static>> {
    let control = crate::tui::test_support::with_log_scale(
        crate::tui::test_support::profile_named("vidar2"),
        crate::tui::test_support::AEGIR_SCALE,
    )
    .control_by_key("meter_1")
    .expect("a meter")
    .clone();
    let reading = Reading {
        value: svolder_ipc::ValueObject {
            heard_at_ms: None,
            id: "20.1".into(),
            key: Some("meter_1".into()),
            label: "Meter L".into(),
            value_type: "u16".into(),
            raw: serde_json::json!(raw),
            display: raw.to_string(),
            value: None,
            unit: None,
        },
        received: Instant::now(),
        rate: Default::default(),
    };
    let lines = meter_lines(
        &control,
        Some(&reading),
        theme,
        Columns {
            field: 8,
            bar,
            room: 40,
        },
        MeterState {
            placed: Some(svolder_profiles::Placed::Log {
                zero: 160,
                full: 350,
                class_a: 0.25,
            }),
            live,
            eased: None,
            peak,
        },
        1,
    );
    lines[0].spans.clone()
}

/// At 16 colours the fill's role goes by the reading's own fraction `p` of the live part, not
/// by the cells it fills: ok below 0.70, hot from 0.70, clip from 0.90, on a bar so narrow that
/// its cells are coarser than those steps.
#[test]
fn at_16_colours_a_log_meters_role_goes_by_the_reading_not_its_cells() {
    let theme = Theme::new(false);
    let placed = svolder_profiles::Placed::Log {
        zero: 160,
        full: 350,
        class_a: 0.25,
    };
    // The lowest raw reading at or above `p`, and the one before it.
    let first_at = |p: f64| -> i64 {
        (160..=350)
            .find(|raw| placed.fraction(*raw as f64) >= p)
            .expect("a reading reaches p")
    };
    let fill_style = |raw: i64| -> Style {
        aegir_meter_spans(&theme, raw, 12, true, None)
            .into_iter()
            .find(|span| span.content == "█")
            .map(|span| span.style)
            .unwrap_or_else(|| panic!("{raw}: no filled cell"))
    };
    let hot = first_at(0.70);
    let clip = first_at(0.90);
    assert_eq!(
        fill_style(hot - 1),
        theme.style(Role::MeterOk),
        "just below 0.70"
    );
    assert_eq!(fill_style(hot), theme.style(Role::MeterHot), "at 0.70");
    assert_eq!(
        fill_style(clip - 1),
        theme.style(Role::MeterHot),
        "just below 0.90"
    );
    assert_eq!(fill_style(clip), theme.style(Role::MeterClip), "at 0.90");
}

/// A dead meter draws no peak marker even when handed one, its class-A block and tick are
/// drawn faint, and a live meter's tick carries no style.
#[test]
fn a_dead_meter_draws_no_peak_and_a_faint_tick() {
    let theme = Theme::new(false);
    let faint = theme.style(Role::Faint);
    let dead = aegir_meter_spans(&theme, 200, 20, false, Some(1.0));
    assert!(
        dead.iter().all(|span| span.content != PEAK_MARK),
        "a dead meter drew its peak: {dead:?}"
    );
    for glyph in ["▓", TICK] {
        let cell = dead
            .iter()
            .find(|span| span.content == glyph)
            .unwrap_or_else(|| panic!("no {glyph} on a dead meter"));
        assert_eq!(cell.style, faint, "a dead meter's {glyph}");
    }
    let live = aegir_meter_spans(&theme, 200, 20, true, None);
    let tick = live
        .iter()
        .find(|span| span.content == TICK)
        .expect("a tick");
    assert_eq!(
        tick.style,
        Style::default(),
        "a live meter's tick is unstyled"
    );
}
