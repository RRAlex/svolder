// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Cells to pixels: geometry, colours and modifiers.

use ratatui::buffer::Cell;
use ratatui::style::{Color, Modifier};

use crate::tui::screenshots::raster::geometry::{CellSize, drawn};
use crate::tui::screenshots::raster::*;
use crate::tui::screenshots::tests::palette;

const CELL: CellSize = CellSize { w: 16, h: 36 };

fn cover(c: char) -> Vec<f32> {
    drawn(c, CELL)
        .expect("drawn from geometry")
        .expect("a known shape")
}

fn at(cov: &[f32], x: u32, y: u32) -> f32 {
    cov[(y * CELL.w + x) as usize]
}

#[test]
fn a_horizontal_line_meets_a_corner_with_no_gap_at_the_cell_edge() {
    let line = cover('─');
    let corner = cover('┐');
    let rows: Vec<u32> = (0..CELL.h)
        .filter(|&y| at(&line, CELL.w - 1, y) > 0.99)
        .collect();
    assert!(!rows.is_empty());
    for y in rows {
        assert!(at(&corner, 0, y) > 0.99, "row {y} has a gap at the join");
    }
}

#[test]
fn a_vertical_line_meets_the_corner_above_it_with_no_gap() {
    let line = cover('│');
    let corner = cover('┐');
    let columns: Vec<u32> = (0..CELL.w).filter(|&x| at(&line, x, 0) > 0.99).collect();
    assert!(!columns.is_empty());
    for x in columns {
        assert!(
            at(&corner, x, CELL.h - 1) > 0.99,
            "column {x} has a gap at the join"
        );
    }
}

#[test]
fn a_heavy_line_is_thicker_than_a_light_one() {
    let light: f32 = cover('│').iter().sum();
    let heavy: f32 = cover('┃').iter().sum();
    assert!(heavy > light * 1.5);
}

#[test]
fn a_lower_eighths_block_fills_exactly_its_share() {
    for n in 1..=7u32 {
        let c = char::from_u32(0x2580 + n).expect("a block element");
        let cov = cover(c);
        let full = (0..CELL.h).filter(|&y| at(&cov, 0, y) > 0.99).count() as u32;
        assert_eq!(full, (CELL.h * n + 4) / 8, "{c}");
    }
}

#[test]
fn every_block_element_is_drawn_from_geometry() {
    for code in 0x2580..=0x259f {
        let c = char::from_u32(code).expect("a block element");
        assert!(matches!(drawn(c, CELL), Some(Ok(_))), "U+{code:04X}");
    }
}

#[test]
fn halves_and_quadrants_split_the_cell_at_the_same_lines() {
    let full = cover('█');
    let [ul, ur, ll, lr] = ['▘', '▝', '▖', '▗'].map(cover);
    for i in 0..full.len() {
        assert_eq!(ul[i] + ur[i] + ll[i] + lr[i], full[i], "pixel {i}");
    }
    let (left, right) = (cover('▌'), cover('▐'));
    for i in 0..full.len() {
        assert_eq!(left[i] + right[i], 1.0);
    }
}

#[test]
fn the_arc_corners_are_drawn_as_the_square_ones() {
    for (arc, square) in [('╭', '┌'), ('╮', '┐'), ('╰', '└'), ('╯', '┘')] {
        assert_eq!(cover(arc), cover(square), "{arc}");
    }
}

#[test]
fn a_tee_or_cross_covers_each_of_its_arms_to_the_cell_edge_and_no_other() {
    // (glyph, up, right, down, left): which cell edges the glyph reaches.
    for (c, arms) in [
        ('├', [true, true, true, false]),
        ('┤', [true, false, true, true]),
        ('┬', [false, true, true, true]),
        ('┴', [true, true, false, true]),
        ('┼', [true; 4]),
        ('┣', [true, true, true, false]),
        ('┫', [true, false, true, true]),
        ('┳', [false, true, true, true]),
        ('┻', [true, true, false, true]),
        ('╋', [true; 4]),
    ] {
        let cov = cover(c);
        let (mx, my) = (CELL.w / 2, CELL.h / 2);
        let reach = [
            at(&cov, mx, 0),
            at(&cov, CELL.w - 1, my),
            at(&cov, mx, CELL.h - 1),
            at(&cov, 0, my),
        ];
        for (side, (got, want)) in reach.iter().zip(arms).enumerate() {
            assert_eq!(*got > 0.99, want, "{c}: edge {side}");
        }
    }
}

#[test]
fn the_heavy_tees_and_cross_are_thicker_than_the_light_ones() {
    for (light, heavy) in [('├', '┣'), ('┤', '┫'), ('┬', '┳'), ('┴', '┻'), ('┼', '╋')]
    {
        let (l, h): (f32, f32) = (cover(light).iter().sum(), cover(heavy).iter().sum());
        assert!(h > l * 1.3, "{heavy}");
    }
}

#[test]
fn a_mixed_weight_tee_is_an_error_naming_it() {
    let why = drawn('┝', CELL)
        .expect("a box character")
        .expect_err("not drawn");
    assert!(why.contains("U+251D"), "{why}");
}

#[test]
fn every_quadrant_glyph_is_the_union_of_its_quadrants() {
    let [ul, ur, ll, lr] = ['▘', '▝', '▖', '▗'].map(cover);
    let sum = |parts: &[&Vec<f32>]| -> Vec<f32> {
        (0..ul.len())
            .map(|i| parts.iter().map(|p| p[i]).sum())
            .collect()
    };
    for (c, parts) in [
        ('▙', sum(&[&ul, &ll, &lr])),
        ('▚', sum(&[&ul, &lr])),
        ('▛', sum(&[&ul, &ur, &ll])),
        ('▜', sum(&[&ul, &ur, &lr])),
        ('▞', sum(&[&ur, &ll])),
        ('▟', sum(&[&ur, &ll, &lr])),
    ] {
        assert_eq!(cover(c), parts, "{c}");
    }
}

#[test]
fn the_shades_are_uniform_quarter_half_and_three_quarter_cover() {
    for (c, share) in [('░', 0.25), ('▒', 0.5), ('▓', 0.75)] {
        assert!(cover(c).iter().all(|&v| (v - share).abs() < 1e-6), "{c}");
    }
}

#[test]
fn a_braille_character_lights_exactly_its_dots() {
    // Dot n of the 2×4 grid, as (column, row): 1 2 3 7 down the left, 4 5 6 8 down the right.
    let dots = [
        (0, 0),
        (0, 1),
        (0, 2),
        (1, 0),
        (1, 1),
        (1, 2),
        (0, 3),
        (1, 3),
    ];
    for bits in [0x01u32, 0x80, 0x5a, 0xff] {
        let cov = cover(char::from_u32(0x2800 + bits).expect("braille"));
        for (bit, (col, row)) in dots.iter().enumerate() {
            let x = (2 * col + 1) * CELL.w / 4;
            let y = (2 * row + 1) * CELL.h / 8;
            let lit = at(&cov, x, y) > 0.99;
            assert_eq!(
                lit,
                bits & (1 << bit) != 0,
                "bits {bits:#x}, dot {}",
                bit + 1
            );
        }
    }
}

#[test]
fn a_box_character_the_raster_does_not_draw_is_an_error_naming_it() {
    let why = drawn('╳', CELL)
        .expect("a box character")
        .expect_err("not drawn");
    assert!(why.contains("U+2573"), "{why}");
}

#[test]
fn letters_and_symbols_are_left_to_the_fonts() {
    for c in ['A', '‹', '▶', '·'] {
        assert!(drawn(c, CELL).is_none(), "{c}");
    }
}

#[test]
fn the_palette_takes_the_amber_ground_and_text_from_the_theme() {
    let theme = crate::tui::test_support::theme_for("amber", svolder_ipc::ColorDepth::TrueColor);
    let p = Palette::of(&theme);
    assert_eq!(p.ground, (0x07, 0x06, 0x05));
    assert_eq!(p.text, (0xf2, 0xf0, 0xea));
}

#[test]
fn rgb_and_reset_resolve_as_the_spec_says_and_indexed_is_an_error() {
    let p = palette();
    assert_eq!(resolve(Color::Rgb(1, 2, 3), Side::Fg, &p), Ok((1, 2, 3)));
    assert_eq!(resolve(Color::Reset, Side::Bg, &p), Ok((7, 6, 5)));
    assert_eq!(resolve(Color::Reset, Side::Fg, &p), Ok((0xf2, 0xf0, 0xea)));
    assert!(resolve(Color::Indexed(42), Side::Fg, &p).is_err());
}

#[test]
fn named_colours_resolve_through_the_crates_xterm_table() {
    assert_eq!(
        resolve(Color::White, Side::Fg, &palette()),
        Ok((255, 255, 255))
    );
}

#[test]
fn dim_blends_half_way_to_the_background_and_reversed_swaps_after_it() {
    let mut cell = Cell::default();
    cell.set_fg(Color::Rgb(200, 100, 0))
        .set_bg(Color::Rgb(0, 0, 0));
    cell.modifier = Modifier::DIM | Modifier::REVERSED | Modifier::BOLD;
    let ink = ink(&cell, &palette()).expect("drawable");
    assert_eq!(ink.fg, (0, 0, 0));
    assert_eq!(ink.bg, (100, 50, 0));
    assert!(ink.bold);
}

#[test]
fn a_modifier_the_tui_does_not_use_is_an_error() {
    let mut cell = Cell::default();
    cell.modifier = Modifier::UNDERLINED;
    assert!(ink(&cell, &palette()).is_err());
}

#[test]
fn a_linear_light_blend_returns_its_ends_exactly() {
    for c in [0u8, 1, 7, 127, 128, 254, 255] {
        let a = (c, 255 - c, c / 2);
        assert_eq!(blend(a, (9, 9, 9), 1.0), a);
        assert_eq!(blend((9, 9, 9), a, 0.0), a);
    }
    let mid = blend((255, 255, 255), (0, 0, 0), 0.5);
    assert!(
        mid.0 > 180 && mid.0 < 195,
        "linear light, not sRGB: {mid:?}"
    );
}
