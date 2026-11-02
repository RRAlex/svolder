// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use svolder_ipc::{ColorDepth, TuiConfig};

use crate::tui::palette::rgb_of;
use crate::tui::theme::{DIM_GREY, Theme};

use super::super::rng::Rng;
use super::*;

fn truecolor(background: bool) -> Theme {
    let config = TuiConfig {
        theme_background: background,
        ..TuiConfig::default()
    };
    Theme::from_config(&config, false, ColorDepth::TrueColor)
}

#[test]
fn the_large_mark_is_258_cells_in_seven_letters_numbered_left_to_right() {
    let cells = mark_cells();
    let count = |glyph| cells.iter().filter(|c| c.glyph == glyph).count();
    assert_eq!(cells.len(), 258);
    assert_eq!((count(FACE), count('▒'), count('░')), (154, 69, 35));
    let letters = cells.iter().map(|c| c.letter).max().unwrap() + 1;
    assert_eq!(letters, 7);
    let left = |k| {
        cells
            .iter()
            .filter(|c| c.letter == k)
            .map(|c| c.col)
            .min()
            .unwrap()
    };
    for k in 1..letters {
        assert!(
            left(k - 1) < left(k),
            "letter {k} starts left of letter {}",
            k - 1
        );
    }
}

/// The front reaches the `s` from 1.00 to 1.35 s of the build and the `r` from 1.88 to
/// 2.20 s, before jitter: the landing times the art gives.
#[test]
fn the_front_rises_from_the_bottom_left_at_thirty_degrees() {
    let cells = mark_cells();
    let face: Vec<&MarkCell> = cells.iter().filter(|c| c.glyph == FACE).collect();
    let (low, high) = span(face.iter().map(|c| c.front));
    let land = |c: &MarkCell| 1.0 + norm(c.front, low, high) * 1.2;
    let window = |k| {
        let times: Vec<f32> = face
            .iter()
            .filter(|c| c.letter == k)
            .map(|c| land(c))
            .collect();
        span(times.into_iter())
    };
    let (s_first, s_last) = window(0);
    let (r_first, r_last) = window(6);
    assert!(
        (s_first - 1.0).abs() < 0.01 && (s_last - 1.35).abs() < 0.01,
        "{s_first} {s_last}"
    );
    assert!(
        (r_first - 1.88).abs() < 0.01 && (r_last - 2.2).abs() < 0.01,
        "{r_first} {r_last}"
    );
}

#[test]
fn a_curve_starts_where_it_is_thrown_from_and_lands_on_its_cell() {
    let mut rng = Rng::new(3);
    for _ in 0..100 {
        let from = (rng.unit() * 120.0, rng.unit() * 40.0);
        let to = (rng.unit() * 120.0, rng.unit() * 40.0);
        let path = Path::curve(from, to, &mut rng);
        let (start, end) = (path.at(0.0), path.at(1.0));
        assert!((start.0 - from.0).abs() < 1e-3 && (start.1 - from.1).abs() < 1e-3);
        assert!((end.0 - to.0).abs() < 1e-3 && (end.1 - to.1).abs() < 1e-3);
        assert_eq!(
            path.at(1.5),
            path.at(1.0),
            "past its landing a spark stays landed"
        );
    }
}

#[test]
fn a_dot_lights_the_braille_dot_under_it_in_the_brightest_dots_colour() {
    let theme = truecolor(true);
    let colours = Colours::of(&theme);
    let area = Rect::new(0, 0, 20, 10);
    let mut dots = Dots::new(area);
    dots.put((10.0, 5.0), 0.3, 0.0);
    dots.put((10.6, 5.9), 1.0, 1.0);
    dots.put((25.0, 5.0), 1.0, 1.0);
    dots.put((3.0, 3.0), 0.0, 1.0);
    let mut buf = Buffer::empty(area);
    dots.paint(&mut buf, &colours);
    let cell = buf.cell((10, 5)).unwrap();
    assert_eq!(cell.symbol(), "⢁", "the top-left and bottom-right dots");
    assert_eq!(cell.fg, colour(mix(colours.ground, colours.hot, 1.0)));
    assert_eq!(
        buf.cell((3, 3)).unwrap().symbol(),
        " ",
        "no brightness lights nothing"
    );
}

#[test]
fn the_colours_are_the_wordmarks_gradient_over_the_ground_or_the_dim_grey() {
    let theme = truecolor(true);
    let colours = Colours::of(&theme);
    assert_eq!(colours.rows[0], theme.accent_rgb());
    assert_eq!(colours.rows[7], rgb_of(theme.accent_shade(0.5)));
    assert_eq!(colours.ground, (7, 6, 5));
    assert_eq!(Colours::of(&truecolor(false)).ground, rgb_of(DIM_GREY));
    assert_ne!(colours.hot, colours.accent);
}

#[test]
fn a_curve_bends_sideways_off_its_midpoint_by_at_most_a_third_of_its_length() {
    let mut rng = Rng::new(11);
    for _ in 0..50 {
        let up = Path::curve((10.0, 0.0), (10.0, 10.0), &mut rng);
        assert!(
            (up.bend.1 - 5.0).abs() < 1e-3,
            "on the perpendicular through the midpoint"
        );
        assert!(
            (up.bend.0 - 10.0).abs() <= 0.35 * 20.0 + 1e-3,
            "{:?}",
            up.bend
        );
        let along = Path::curve((0.0, 4.0), (20.0, 4.0), &mut rng);
        assert!((along.bend.0 - 10.0).abs() < 1e-3);
        assert!(
            (along.bend.1 - 4.0).abs() <= 0.35 * 20.0 / 2.0 + 1e-3,
            "{:?}",
            along.bend
        );
    }
}
