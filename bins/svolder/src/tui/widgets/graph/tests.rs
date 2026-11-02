// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::style::Color;
use svolder_ipc::ColorDepth;
use svolder_profiles::Placed;

use crate::tui::test_support::theme_for;

use super::*;

/// The text of the one row of a graph.
fn text(shape: Shape, upper: &[Option<f64>]) -> String {
    let theme = theme_for("default", ColorDepth::Ansi16);
    let rows = draw(shape, upper, &[], &theme);
    rows[0].iter().map(|s| s.content.as_ref()).collect()
}

fn braille(bits: u32) -> char {
    char::from_u32(0x2800 + bits).unwrap()
}

/// A linear scale from `from` (empty) to `to` (full), with no class-A tick.
fn linear(from: i64, to: i64) -> Placed {
    Placed::Linear {
        from,
        to,
        tick: None,
    }
}

fn some(values: &[f64]) -> Vec<Option<f64>> {
    values.iter().map(|&v| Some(v)).collect()
}

#[test]
fn every_dot_of_both_columns_packs_to_its_bit() {
    // One lit dot in the left column, bottom up, then the right.
    for (dots, left, right) in [
        (1, 0x40, 0x80),
        (2, 0x44, 0xA0),
        (3, 0x46, 0xB0),
        (4, 0x47, 0xB8),
    ] {
        let level = (dots - 1) as f64 / 3.0;
        assert_eq!(
            text(Shape::Fill, &some(&[level])),
            braille(left).to_string()
        );
        assert_eq!(
            text(Shape::Fill, &[None, Some(level)]),
            braille(right).to_string()
        );
        assert_eq!(
            text(Shape::Fill, &some(&[level, level])),
            braille(left | right).to_string()
        );
    }
}

#[test]
fn the_fill_levels_run_from_one_dot_to_four_and_no_data_is_blank() {
    for (level, bits) in [
        (0.0, 0xC0),
        (1.0 / 3.0, 0xE4),
        (2.0 / 3.0, 0xF6),
        (1.0, 0xFF),
    ] {
        assert_eq!(
            text(Shape::Fill, &some(&[level, level])),
            braille(bits).to_string()
        );
    }
    assert_eq!(text(Shape::Fill, &[None, None]), " ");
    // Out of range levels are held to the ends.
    assert_eq!(
        text(Shape::Fill, &some(&[7.0, -3.0])),
        braille(0x47 | 0x80).to_string()
    );
}

#[test]
fn a_flat_window_and_a_control_at_its_floor_draw_the_bottom_dots() {
    let flat = normalise(None, &some(&[5.0, 5.0]));
    assert_eq!(text(Shape::Fill, &flat), braille(0xC0).to_string());
    let floor = normalise(Some(linear(0, 200)), &some(&[0.0, 0.0]));
    assert_eq!(text(Shape::Fill, &floor), braille(0xC0).to_string());
}

#[test]
fn an_odd_column_count_leaves_the_right_half_blank() {
    assert_eq!(
        text(Shape::Fill, &some(&[1.0, 1.0, 1.0])),
        format!("{}{}", braille(0xFF), braille(0x47))
    );
}

#[test]
fn a_dot_line_is_top_for_on_and_bottom_for_off() {
    assert_eq!(
        text(Shape::Dots, &some(&[1.0, 1.0])),
        braille(0x09).to_string()
    );
    assert_eq!(
        text(Shape::Dots, &some(&[0.0, 0.0])),
        braille(0xC0).to_string()
    );
    assert_eq!(
        text(Shape::Dots, &some(&[1.0, 0.0])),
        braille(0x81).to_string()
    );
    assert_eq!(
        text(Shape::Dots, &some(&[0.0, 1.0])),
        braille(0x48).to_string()
    );
}

#[test]
fn history_before_the_first_sample_is_blank() {
    assert_eq!(
        text(Shape::Dots, &[None, None, Some(1.0), None]),
        format!(" {}", braille(0x01))
    );
}

#[test]
fn mirrored_fills_the_upper_row_up_and_the_lower_row_down() {
    let theme = theme_for("default", ColorDepth::Ansi16);
    let rows = draw(
        Shape::Mirrored,
        &some(&[1.0 / 3.0, 1.0]),
        &some(&[0.0, 2.0 / 3.0]),
        &theme,
    );
    assert_eq!(rows.len(), 2);
    let row = |i: usize| {
        rows[i]
            .iter()
            .map(|s| s.content.as_ref())
            .collect::<String>()
    };
    // Upper: left 2 dots from the bottom (0x40|0x04), right 4 (0xB8).
    assert_eq!(row(0), braille(0x44 | 0xB8).to_string());
    // Lower: left 1 dot from the top (0x01), right 3 from the top (0x08|0x10|0x20).
    assert_eq!(row(1), braille(0x01 | 0x38).to_string());
}

#[test]
fn roles_colour_fills_ok_dots_and_down_dots() {
    let theme = theme_for("default", ColorDepth::Ansi16);
    let fg = |spans: &[Span<'static>]| spans[0].style.fg;
    let fill = draw(Shape::Fill, &some(&[1.0, 1.0]), &[], &theme);
    assert_eq!(fg(&fill[0]), theme.style(Role::Bar).fg);
    let up = draw(Shape::Link, &some(&[1.0, 1.0]), &[], &theme);
    assert_eq!(fg(&up[0]), Some(Color::Green));
    let down = draw(Shape::Link, &some(&[0.0, 1.0]), &[], &theme);
    assert_eq!(fg(&down[0]), theme.style(Role::Warn).fg);
}

#[test]
fn monochrome_draws_the_dots_alone() {
    let theme = Theme::new(true);
    for shape in [Shape::Fill, Shape::Dots, Shape::Fault, Shape::Link] {
        let rows = draw(shape, &some(&[1.0, 0.0]), &[], &theme);
        for span in &rows[0] {
            assert_eq!(span.style.fg, None);
        }
        assert!(!rows[0].is_empty());
    }
}

#[test]
fn the_link_strip_and_a_fault_draw_their_alarm_in_warn_and_a_plain_bool_never() {
    let theme = theme_for("default", ColorDepth::Ansi16);
    let fg = |shape, values: &[f64]| draw(shape, &some(values), &[], &theme)[0][0].style.fg;
    let ok = theme.style(Role::MeterOk).fg;
    let warn = theme.style(Role::Warn).fg;
    assert_ne!(ok, warn);
    for values in [[1.0, 1.0], [0.0, 0.0], [0.0, 1.0]] {
        assert_eq!(fg(Shape::Dots, &values), ok);
    }
    assert_eq!(fg(Shape::Link, &[1.0, 1.0]), ok);
    assert_eq!(fg(Shape::Link, &[0.0, 1.0]), warn);
    assert_eq!(fg(Shape::Fault, &[0.0, 0.0]), ok);
    assert_eq!(fg(Shape::Fault, &[0.0, 1.0]), warn);
}
