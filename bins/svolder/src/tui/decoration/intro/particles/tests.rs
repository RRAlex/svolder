// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;

use crate::tui::palette::rgb_of;
use crate::tui::test_support::true_colour_background;
use crate::tui::theme::DIM_GREY;

use super::super::sparks::{Colours, FACE, colour, mix};
use super::*;

const SEED: u64 = 0x5EED;
const BRAILLE: std::ops::RangeInclusive<char> = '\u{2800}'..='\u{28ff}';

fn frame_at(build: &Build, t: f32) -> Buffer {
    let theme = true_colour_background(true);
    let mut buf = Buffer::empty(build.area());
    build.frame(t, &mut buf, &theme, &Colours::of(&theme));
    buf
}

fn still(area: Rect) -> Buffer {
    let mut buf = Buffer::empty(area);
    super::super::draw_still(&mut buf, area, &true_colour_background(true));
    buf
}

fn glyph(buf: &Buffer, x: u16, y: u16) -> char {
    buf.cell((x, y))
        .unwrap()
        .symbol()
        .chars()
        .next()
        .unwrap_or(' ')
}

#[test]
fn there_is_no_build_below_the_large_mark() {
    assert!(Build::new(Rect::new(0, 0, 69, 12), SEED).is_none());
    assert!(Build::new(Rect::new(0, 0, 70, 11), SEED).is_none());
    assert!(Build::new(Rect::new(0, 0, 70, 12), SEED).is_some());
}

#[test]
fn every_face_cell_gets_one_spark_thrown_from_the_edge_onto_its_centre() {
    let area = Rect::new(0, 0, 80, 24);
    let build = Build::new(area, SEED).unwrap();
    assert_eq!(build.face.len(), 154);
    for spark in &build.face {
        assert!(
            (1.0 - 0.04..=2.2 + 0.04).contains(&spark.land),
            "{}",
            spark.land
        );
        assert!(spark.throw >= 0.25 && spark.throw < spark.land);
        let (x, y) = spark.start;
        let on_edge = x <= 0.11 || y <= 0.11 || x >= 79.89 || y >= 23.89;
        assert!(on_edge, "{x},{y}");
        let (lx, ly) = spark.path.at(1.0);
        let (cx, cy) = super::super::sparks::centre(build.mark, &spark.cell);
        assert!((lx - cx).abs() < 1e-3 && (ly - cy).abs() < 1e-3);
    }
}

#[test]
fn the_landings_follow_the_front() {
    let build = Build::new(Rect::new(0, 0, 120, 40), SEED).unwrap();
    let mut sparks: Vec<&Spark> = build.face.iter().collect();
    sparks.sort_by(|a, b| a.cell.front.total_cmp(&b.cell.front));
    for pair in sparks.windows(2) {
        assert!(pair[0].land <= pair[1].land + 0.08 + 1e-4);
    }
}

#[test]
fn it_starts_on_bare_ground_and_the_first_sparks_wait_on_the_edge() {
    let area = Rect::new(0, 0, 80, 24);
    let build = Build::new(area, SEED).unwrap();
    let first = frame_at(&build, 0.0);
    assert!(first.content().iter().all(|cell| cell.symbol() == " "));
    let early = frame_at(&build, 0.2);
    let mut lit = 0;
    for y in 0..24 {
        for x in 0..80 {
            let ch = glyph(&early, x, y);
            if ch == ' ' {
                continue;
            }
            lit += 1;
            assert!(BRAILLE.contains(&ch), "{ch} at {x},{y}");
            assert!(
                x == 0 || y == 0 || x == 79 || y == 23,
                "{x},{y} is off the edge"
            );
        }
    }
    assert!(lit > 0);
}

#[test]
fn at_one_and_a_half_seconds_the_s_is_built_and_the_r_not_begun() {
    let build = Build::new(Rect::new(0, 0, 80, 24), SEED).unwrap();
    let buf = frame_at(&build, 1.5);
    for spark in &build.face {
        let (x, y) = (build.mark.x + spark.cell.col, build.mark.y + spark.cell.row);
        match spark.cell.letter {
            0 => assert_eq!(glyph(&buf, x, y), FACE),
            6 => assert_ne!(glyph(&buf, x, y), FACE),
            _ => {}
        }
    }
}

/// The seconds at which the last face spark of `letter` lands.
fn built(build: &Build, letter: usize) -> f32 {
    build
        .face
        .iter()
        .filter(|spark| spark.cell.letter == letter)
        .fold(0.0, |latest, spark| latest.max(spark.land))
}

#[test]
fn no_shadow_shows_before_its_letter_is_built() {
    let build = Build::new(Rect::new(0, 0, 80, 24), SEED).unwrap();
    // A shadow starts only after its letter's face is whole, a little later for the lighter one.
    for shadow in &build.shadows {
        let delay = [0.05, 0.15][usize::from(shadow.depth - 1)];
        assert!(
            shadow.start >= built(&build, shadow.cell.letter) + delay - 1e-4,
            "{shadow:?}"
        );
    }
    for step in 0..56 {
        let t = step as f32 * 0.05;
        let buf = frame_at(&build, t);
        for y in 0..24 {
            for x in 0..80 {
                let ch = glyph(&buf, x, y);
                if !matches!(ch, '▒' | '░') {
                    continue;
                }
                // Each shadow cell on screen is a started shadow of that glyph, standing at its
                // place or up to its depth up-left of it.
                let owned = build.shadows.iter().any(|shadow| {
                    let (px, py) = build.at(&shadow.cell);
                    shadow.cell.glyph == ch
                        && shadow.start <= t
                        && (0..=shadow.depth)
                            .any(|off| (px.saturating_sub(off), py.saturating_sub(off)) == (x, y))
                });
                assert!(owned, "a {ch} at {x},{y} before its letter at {t}s");
            }
        }
    }
}

#[test]
fn a_landing_flashes_towards_hot_and_cools_to_its_row_colour() {
    let theme = true_colour_background(true);
    let colours = Colours::of(&theme);
    let build = Build::new(Rect::new(0, 0, 80, 24), SEED).unwrap();
    let spark = build
        .face
        .iter()
        .min_by(|a, b| a.land.total_cmp(&b.land))
        .unwrap();
    let (x, y) = build.at(&spark.cell);
    let row = colours.rows[usize::from(spark.cell.row)];
    let at = |t| frame_at(&build, t).cell((x, y)).unwrap().fg;
    assert_eq!(at(spark.land), colour(mix(row, colours.hot, 0.9)));
    assert_ne!(at(spark.land), colour(row));
    assert_eq!(at(spark.land + 0.15), colour(row));
}

#[test]
fn the_glint_lifts_the_mark_towards_hot_while_it_crosses() {
    let theme = true_colour_background(true);
    let colours = Colours::of(&theme);
    let area = Rect::new(0, 0, 80, 24);
    let build = Build::new(area, SEED).unwrap();
    let still = still(area);
    let mid = frame_at(&build, 2.6 + 0.35 / 2.0);
    let away = |buf: &Buffer, x, y| {
        let (r, g, b) = rgb_of(buf.cell((x, y)).unwrap().fg);
        let (hr, hg, hb) = colours.hot;
        r.abs_diff(hr) as u32 + g.abs_diff(hg) as u32 + b.abs_diff(hb) as u32
    };
    let mut lifted = 0;
    for y in build.mark.top()..build.mark.bottom() {
        for x in build.mark.left()..build.mark.right() {
            if still.cell((x, y)).unwrap().symbol() == " " {
                continue;
            }
            assert!(away(&mid, x, y) <= away(&still, x, y), "{x},{y}");
            lifted += usize::from(away(&mid, x, y) < away(&still, x, y));
        }
    }
    assert!(lifted > 0, "no mark cell is lifted mid-glint");
}

#[test]
fn a_shadow_is_never_drawn_over_a_landed_face_cell() {
    let build = Build::new(Rect::new(0, 0, 80, 24), SEED).unwrap();
    for step in 0..64 {
        let t = step as f32 * 0.05;
        let buf = frame_at(&build, t);
        for spark in build.face.iter().filter(|spark| t >= spark.land) {
            let (x, y) = build.at(&spark.cell);
            assert_eq!(glyph(&buf, x, y), FACE, "{x},{y} at {t}s");
        }
    }
}

#[test]
fn the_notice_comes_in_from_two_point_six_seconds_and_is_whole_by_three_point_two() {
    let area = Rect::new(0, 0, 80, 24);
    let build = Build::new(area, SEED).unwrap();
    let letters = |buf: &Buffer| {
        (22..24)
            .flat_map(|y| (0..80).map(move |x| (x, y)))
            .filter(|&(x, y)| glyph(buf, x, y).is_alphanumeric())
            .count()
    };
    assert_eq!(letters(&frame_at(&build, 2.5)), 0);
    assert!(letters(&frame_at(&build, 2.9)) > 0);
    let whole = still(area);
    for x in 0..80 {
        for y in 22..24 {
            assert_eq!(frame_at(&build, 3.2).cell((x, y)), whole.cell((x, y)));
        }
    }
}

#[test]
fn it_settles_on_exactly_the_still_frame_and_hands_over_without_a_jump() {
    for (w, h) in [(80, 24), (120, 40)] {
        let area = Rect::new(0, 0, w, h);
        let build = Build::new(area, SEED).unwrap();
        assert_eq!(frame_at(&build, 3.6), still(area));
        assert_eq!(frame_at(&build, 3.2), still(area));
        // Just before the still takes over, every glyph of the mark already stands as the
        // still draws it. The blanks between the letters carry no colour anyone sees.
        let before = frame_at(&build, 3.19);
        let whole = still(area);
        for y in build.mark.top()..build.mark.bottom() {
            for x in build.mark.left()..build.mark.right() {
                let want = whole.cell((x, y)).unwrap();
                if want.symbol() != " " {
                    assert_eq!(before.cell((x, y)), Some(want), "{x},{y}");
                }
            }
        }
    }
}

#[test]
fn two_seeds_scatter_differently_and_finish_alike() {
    let area = Rect::new(0, 0, 80, 24);
    let a = Build::new(area, 1).unwrap();
    let b = Build::new(area, 2).unwrap();
    assert_ne!(frame_at(&a, 1.5), frame_at(&b, 1.5));
    assert_eq!(frame_at(&a, 3.6), frame_at(&b, 3.6));
}

/// Odd sizes: the tightest frame, one cell more, a terminal far larger than the mark. Every frame
/// paints without a panic, with no theme background as with one.
#[test]
fn every_frame_paints_at_odd_sizes_with_or_without_a_ground() {
    for background in [true, false] {
        let theme = true_colour_background(background);
        let colours = Colours::of(&theme);
        for (w, h) in [(70, 12), (71, 13), (80, 24), (250, 70)] {
            let build = Build::new(Rect::new(0, 0, w, h), SEED).unwrap();
            for step in 0..=72 {
                let mut buf = Buffer::empty(build.area());
                build.frame(step as f32 * 0.05, &mut buf, &theme, &colours);
            }
        }
    }
}

#[test]
fn without_a_ground_cells_keep_reset_and_waiting_sparks_mix_from_the_dim_grey() {
    let theme = true_colour_background(false);
    let colours = Colours::of(&theme);
    let build = Build::new(Rect::new(0, 0, 80, 24), SEED).unwrap();
    let t = 0.24;
    let mut buf = Buffer::empty(build.area());
    build.frame(t, &mut buf, &theme, &colours);
    assert!(buf.content().iter().all(|cell| cell.bg == Color::Reset));
    let ground = rgb_of(DIM_GREY);
    let waiting = |spark: &Spark| {
        let swing = (std::f32::consts::TAU * (WAIT_HZ * t + spark.phase)).sin();
        (
            (WAIT_GLOW + WAIT_SWING * swing) * (t / APPEAR).min(1.0),
            spark.start,
        )
    };
    let mut checked = 0;
    for spark in &build.face {
        let (glow, (sx, sy)) = waiting(spark);
        // The brightest waiting spark in a cell is the one it shows.
        let cell = (sx.floor() as u16, sy.floor() as u16);
        let brightest = build
            .face
            .iter()
            .map(waiting)
            .filter(|&(_, (x, y))| (x.floor() as u16, y.floor() as u16) == cell)
            .fold(glow, |best, (g, _)| best.max(g));
        if glow < brightest {
            continue;
        }
        let want = mix(ground, mix(colours.accent, colours.hot, WAIT_HEAT), glow);
        assert_eq!(buf.cell(cell).unwrap().fg, colour(want), "{cell:?}");
        checked += 1;
    }
    assert!(checked > 100, "{checked}");
}
