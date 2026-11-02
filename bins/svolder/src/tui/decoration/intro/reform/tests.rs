// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::Duration;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Color;
use svolder_ipc::TuiConfig;

use crate::tui::app::App;
use crate::tui::mouse::HitMap;
use crate::tui::palette::rgb_of;
use crate::tui::render::{self, FrameAreas, Logo};
use crate::tui::test_support::{profile, true_colour_background};
use crate::tui::theme::{DIM_GREY, Rgb, Theme};

use super::*;

const SEED: u64 = 0x5EED;
const STEP: Duration = Duration::from_millis(16);

/// The Controls screen as the loop's first frame draws it, and where it put its pieces.
fn controls(w: u16, h: u16, theme: &Theme) -> (Buffer, FrameAreas) {
    let app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    let mut hits = HitMap::default();
    let mut areas = FrameAreas::default();
    terminal
        .draw(|frame| areas = render::draw(frame, &app, theme, &mut hits))
        .unwrap();
    (terminal.backend().buffer().clone(), areas)
}

/// `reform` painted over a fresh copy of `screen` once `at` has passed since its start.
fn painted_at(reform: &mut Reform, screen: &Buffer, logo: Option<Logo>, at: Duration) -> Buffer {
    let mut buf = screen.clone();
    let behind = Duration::from_millis(u64::from(reform.elapsed_ms()));
    reform.advance(at.saturating_sub(behind));
    reform.paint(&mut buf, logo, &[], Color::Rgb(21, 19, 15));
    buf
}

#[test]
fn the_logo_splits_into_seven_letters_at_either_size() {
    let theme = true_colour_background(true);
    for (w, h, cells) in [(80, 24, 7), (120, 40, 42)] {
        let (screen, areas) = controls(w, h, &theme);
        let mut reform = Reform::for_test(screen.area, SEED, &theme);
        reform.paint(&mut screen.clone(), areas.logo, &[], Color::Reset);
        let plan = reform.plan.as_ref().expect("planned on its first frame");
        assert_eq!(plan.targets.len(), cells);
        let letters = plan.targets.iter().map(|t| t.letter).max().unwrap() + 1;
        assert_eq!(letters, 7, "{w}x{h}");
        for flyer in &plan.flyers {
            assert_eq!(plan.targets[flyer.target].letter, flyer.letter);
        }
        assert_eq!(plan.flyers.len(), 258);
    }
}

#[test]
fn the_first_frame_hides_the_logo_and_shows_the_whole_large_mark() {
    let theme = true_colour_background(true);
    let (screen, areas) = controls(120, 40, &theme);
    let mut reform = Reform::for_test(screen.area, SEED, &theme);
    let first = painted_at(&mut reform, &screen, areas.logo, Duration::ZERO);
    let plan = reform.plan.as_ref().unwrap();
    for target in &plan.targets {
        assert_eq!(first.cell(target.at).unwrap().symbol(), " ");
    }
    let mut still = Buffer::empty(screen.area);
    super::super::draw_still(&mut still, screen.area, &theme);
    let mark = super::super::large_mark(screen.area).unwrap();
    for y in mark.top()..mark.bottom() {
        for x in mark.left()..mark.right() {
            let want = still.cell((x, y)).unwrap();
            if want.symbol() != " " {
                assert_eq!(
                    first.cell((x, y)).unwrap().symbol(),
                    want.symbol(),
                    "{x},{y}"
                );
                assert_eq!(first.cell((x, y)).unwrap().fg, want.fg, "{x},{y}");
            }
        }
    }
}

#[test]
fn it_plays_out_inside_one_point_one_seconds_and_leaves_the_screen_as_drawn() {
    let theme = true_colour_background(true);
    for (w, h) in [(80, 24), (120, 40)] {
        let (screen, areas) = controls(w, h, &theme);
        let mut reform = Reform::for_test(screen.area, SEED, &theme);
        let mut at = Duration::ZERO;
        while reform.playing() {
            assert!(
                at < Duration::from_millis(1100),
                "{w}x{h} still playing at {at:?}"
            );
            painted_at(&mut reform, &screen, areas.logo, at);
            at += STEP;
        }
        assert_eq!(painted_at(&mut reform, &screen, areas.logo, at), screen);
    }
}

#[test]
fn a_frame_of_another_size_or_with_no_logo_ends_it_untouched() {
    let theme = true_colour_background(true);
    let (screen, areas) = controls(120, 40, &theme);
    let mut resized = Reform::for_test(Rect::new(0, 0, 100, 30), SEED, &theme);
    assert_eq!(
        painted_at(&mut resized, &screen, areas.logo, Duration::ZERO),
        screen
    );
    assert!(!resized.playing());
    let mut bare = Reform::for_test(screen.area, SEED, &theme);
    assert_eq!(painted_at(&mut bare, &screen, None, Duration::ZERO), screen);
    assert!(!bare.playing());
}

#[test]
fn held_panes_are_bare_pane_grey_until_their_turn() {
    let theme = true_colour_background(true);
    let (screen, areas) = controls(120, 40, &theme);
    let pane = Color::Rgb(21, 19, 15);
    let frames: Vec<Rect> = areas.frames.iter().map(|(_, rect)| *rect).collect();
    let mut reform = Reform::for_test(screen.area, SEED, &theme);
    let mut buf = screen.clone();
    reform.advance(Duration::from_millis(200));
    reform.paint(&mut buf, areas.logo, &frames, pane);
    // Every pane cell no spark or letter stands on is pane grey.
    let mut checked = 0;
    for rect in &frames {
        for y in rect.top()..rect.bottom() {
            for x in rect.left()..rect.right() {
                let (now, drawn) = (buf.cell((x, y)).unwrap(), screen.cell((x, y)).unwrap());
                if now.symbol() == drawn.symbol() {
                    assert_eq!(now.fg, pane, "{x},{y}");
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 0);
    assert!(reform.holding_panes());
    reform.advance(Duration::from_millis(200));
    assert!(!reform.holding_panes());
}

#[test]
fn with_no_ground_the_hidden_logo_is_blank_and_cells_keep_reset() {
    let theme = true_colour_background(false);
    for (w, h) in [(70, 12), (80, 24), (120, 40)] {
        let (screen, areas) = controls(w, h, &theme);
        let mut reform = Reform::for_test(screen.area, SEED, &theme);
        let first = painted_at(&mut reform, &screen, areas.logo, Duration::ZERO);
        if let Some(plan) = reform.plan.as_ref() {
            for target in &plan.targets {
                assert_eq!(first.cell(target.at).unwrap().symbol(), " ");
            }
        }
        let mut at = Duration::ZERO;
        while reform.playing() {
            let frame = painted_at(&mut reform, &screen, areas.logo, at);
            assert!(
                frame.content().iter().all(|cell| cell.bg == Color::Reset),
                "{w}x{h} at {at:?}"
            );
            at += STEP;
        }
    }
}

#[test]
fn with_no_ground_a_flying_spark_mixes_from_the_dim_grey() {
    let theme = true_colour_background(false);
    let colours = Colours::of(&theme);
    let ground = rgb_of(DIM_GREY);
    let (screen, areas) = controls(120, 40, &theme);
    let mut reform = Reform::for_test(screen.area, SEED, &theme);
    let t = 0.2;
    let frame = painted_at(&mut reform, &screen, areas.logo, Duration::from_millis(200));
    let plan = reform.plan.as_ref().unwrap();
    // Every dot of every spark in flight, by the cell it lights: its brightness and heat.
    let mut dots: std::collections::HashMap<(u16, u16), Vec<(f32, f32)>> = Default::default();
    for flyer in &plan.flyers {
        let tau = t - flyer.depart;
        if !(0.0..flyer.flight).contains(&tau) {
            continue;
        }
        let heat = 1.0 - COOL * tau / flyer.flight;
        let mut seen = vec![(tau, 1.0)];
        seen.extend(
            [0.55, 0.32, 0.16]
                .into_iter()
                .enumerate()
                .map(|(k, b)| (tau - (k + 1) as f32 * 0.022, b)),
        );
        for (back, brightness) in seen.into_iter().filter(|(back, _)| *back >= 0.0) {
            let (x, y) = flyer.path.at(back / flyer.flight);
            dots.entry((x.floor() as u16, y.floor() as u16))
                .or_default()
                .push((brightness, heat));
        }
    }
    let mut checked = 0;
    for (cell, lit) in &dots {
        // A cell one dot lights, dimmer than a head, shows the mix from the ground.
        let [(brightness, heat)] = lit[..] else {
            continue;
        };
        let shown = frame.cell(*cell).unwrap();
        let braille = shown
            .symbol()
            .chars()
            .next()
            .is_some_and(|c| ('\u{2800}'..='\u{28ff}').contains(&c));
        if brightness >= 1.0 || !braille {
            continue;
        }
        let glow = mix(colours.accent, colours.hot, heat);
        assert_eq!(shown.fg, colour(mix(ground, glow, brightness)), "{cell:?}");
        checked += 1;
    }
    assert!(checked > 0, "no trail cell to check");
}

#[test]
fn the_exit_keeps_its_timings() {
    let theme = true_colour_background(true);
    let (screen, areas) = controls(120, 40, &theme);
    let mut reform = Reform::for_test(screen.area, SEED, &theme);
    reform.paint(&mut screen.clone(), areas.logo, &[], Color::Reset);
    let plan = reform.plan.as_ref().unwrap();
    for flyer in &plan.flyers {
        assert!(
            (0.0..=0.3 + 1e-4).contains(&flyer.depart),
            "{}",
            flyer.depart
        );
        assert!(
            (0.55 - 0.08 - 1e-4..=0.55 + 0.08 + 1e-4).contains(&flyer.flight),
            "{}",
            flyer.flight
        );
    }
    let colours = Colours::of(&theme);
    let near = |a: Color, b: Rgb| {
        let (r, g, bl) = rgb_of(a);
        r.abs_diff(b.0) <= 1 && g.abs_diff(b.1) <= 1 && bl.abs_diff(b.2) <= 1
    };
    let (mut held, mut cooled) = (0, 0);
    let last_of_all = plan.end - 0.15;
    for target in plan.targets.iter().filter(|t| t.last < last_of_all - 0.01) {
        let own = rgb_of(screen.cell(target.at).unwrap().fg);
        let braille = |buf: &Buffer| {
            buf.cell(target.at)
                .unwrap()
                .symbol()
                .chars()
                .any(|c| ('\u{2800}'..='\u{28ff}').contains(&c))
        };
        // Held at ninety per cent towards hot as its last spark lands...
        let at = |reform: &mut Reform, t: f32| {
            let mut buf = screen.clone();
            reform.advance(Duration::from_secs_f32((t - reform.elapsed).max(0.0)));
            reform.paint(&mut buf, areas.logo, &[], Color::Reset);
            buf
        };
        let mut fresh = Reform::for_test(screen.area, SEED, &theme);
        at(&mut fresh, 0.0);
        if target.last - target.first > 0.05 {
            let buf = at(&mut fresh, (target.first + target.last) / 2.0);
            if !braille(&buf) {
                assert!(
                    near(buf.cell(target.at).unwrap().fg, mix(own, colours.hot, 0.9)),
                    "held between its sparks {:?}",
                    target.at
                );
            }
        }
        let buf = at(&mut fresh, target.last + 0.001);
        if !braille(&buf) {
            assert!(
                near(
                    buf.cell(target.at).unwrap().fg,
                    mix(own, colours.hot, 0.9 * (1.0 - 0.001 / 0.15))
                ),
                "held {:?}",
                target.at
            );
            held += 1;
        }
        // ...and its own colour again a flash later.
        let buf = at(&mut fresh, target.last + 0.15);
        if !braille(&buf) {
            assert!(
                near(buf.cell(target.at).unwrap().fg, own),
                "cooled {:?}",
                target.at
            );
            cooled += 1;
        }
    }
    assert!(held > 3 && cooled > 3, "{held} held, {cooled} cooled");
}

#[test]
fn one_seed_replays_the_same_exit() {
    let theme = true_colour_background(true);
    let (screen, areas) = controls(120, 40, &theme);
    let at = Duration::from_millis(400);
    let mut a = Reform::for_test(screen.area, 1, &theme);
    let mut b = Reform::for_test(screen.area, 1, &theme);
    let mut c = Reform::for_test(screen.area, 2, &theme);
    let (fa, fb) = (
        painted_at(&mut a, &screen, areas.logo, at),
        painted_at(&mut b, &screen, areas.logo, at),
    );
    assert_eq!(fa, fb);
    assert_ne!(fa, painted_at(&mut c, &screen, areas.logo, at));
}

#[test]
fn a_frame_with_no_logo_after_planning_ends_it_untouched() {
    let theme = true_colour_background(true);
    let (screen, areas) = controls(120, 40, &theme);
    let mut reform = Reform::for_test(screen.area, SEED, &theme);
    let planned = painted_at(&mut reform, &screen, areas.logo, Duration::from_millis(100));
    assert!(planned != screen && reform.playing());
    assert_eq!(
        painted_at(&mut reform, &screen, None, Duration::from_millis(200)),
        screen
    );
    assert!(!reform.playing());
}
