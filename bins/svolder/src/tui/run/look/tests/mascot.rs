// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The exit that reforms the logo and the squirrel that comes in once its pane has drawn itself.

use super::*;

/// A called squirrel playing a climb asks for frames, and the tick falls back to the base
/// period once it rests; a hidden squirrel, or one the terminal cannot show, never changes it.
#[test]
fn the_tick_period_is_a_frame_while_a_called_squirrel_climbs() {
    use crate::tui::decoration::mascot::tricks::Trick;
    use crate::tui::decoration::mascot::{Code, View};

    let mut app = App::new(
        Registry::generic(0),
        "fake".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let slow = Duration::from_millis(250);
    let off = Effects::new(false, &EffectsConfig::default());
    let t0 = Instant::now();
    let mut view = View::bare();
    view.volume = Some((50, 0, 100));

    // Hidden: a volume step plays nothing.
    app.mascot.capable = true;
    app.mascot.observe(&view, t0);
    view.volume = Some((51, 0, 100));
    app.mascot.observe(&view, t0 + Duration::from_millis(100));
    assert_eq!(tick_period(&app, &off), slow);

    app.mascot.code(Code::Konami, t0 + Duration::from_secs(1));
    assert_eq!(tick_period(&app, &off), FAST_TICK, "the arrival plays");
    app.mascot.observe(&view, t0 + Duration::from_secs(20));
    assert_eq!(tick_period(&app, &off), slow, "at rest");

    view.volume = Some((52, 0, 100));
    let step = t0 + Duration::from_secs(30);
    app.mascot.observe(&view, step);
    assert_eq!(tick_period(&app, &off), FAST_TICK, "the climb plays");
    app.mascot.observe(
        &view,
        step + Trick::Climb.duration() + Duration::from_millis(1),
    );
    assert_eq!(tick_period(&app, &off), slow, "the climb is over");

    // The same step with the mascot switched off, or the terminal unable, asks for nothing.
    view.volume = Some((53, 0, 100));
    app.mascot.observe(&view, step + Duration::from_secs(10));
    assert_eq!(tick_period(&app, &off), FAST_TICK);
    app.mascot.capable = false;
    assert_eq!(tick_period(&app, &off), slow);
    app.mascot.capable = true;
    app.tui.effects.mascot = false;
    assert_eq!(tick_period(&app, &off), slow);
}

/// One loop frame of `app`, painted over as the loop paints it `tick` after the frame before,
/// with the animations it earns armed afterwards.
fn loop_frame(
    terminal: &mut Terminal<TestBackend>,
    app: &mut App,
    look: &mut Look,
    tick: Duration,
) -> Buffer {
    let mut hits = HitMap::default();
    let mut areas = FrameAreas::default();
    terminal
        .draw(|frame| {
            areas = render::draw(frame, app, &look.theme, &mut hits);
            paint_over(frame, &areas, &look.theme, &mut look.effects, tick);
        })
        .expect("the frame draws");
    arm_effects(app, &mut look.effects, &areas, &look.theme, Instant::now());
    terminal.backend().buffer().clone()
}

/// The exit over the Controls screen at either size: the header's logo is hidden on the first
/// frame, the loop ticks at the pacer's sixty frames, and after about a second the header rows
/// read what the screen draws.
#[test]
fn the_exit_reforms_the_logo_over_the_first_frames_and_leaves_the_screen() {
    use crate::tui::test_support::{frame_with_theme, profile};
    for (w, h) in [(80, 24), (120, 40)] {
        let mut look = colourful_look();
        let mut app = App::new(
            profile(),
            "gungnir".into(),
            &TuiConfig::default(),
            Vec::new(),
        );
        let area = Rect::new(0, 0, w, h);
        look.effects
            .hand_over(crate::tui::decoration::intro::Reform::for_test(
                area,
                0x5EED,
                &look.theme,
            ));
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        let first = loop_frame(&mut terminal, &mut app, &mut look, Duration::ZERO);
        let plain = frame_with_theme(&app, w, h, &look.theme);
        let row = |buf: &Buffer, y: u16| -> String {
            (0..w)
                .map(|x| buf.cell((x, y)).unwrap().symbol().to_string())
                .collect()
        };
        // The logo's top row is row 0 on both layouts: the word, or the small mark's top.
        assert_ne!(
            row(&first, 0),
            row(&plain, 0),
            "{w}x{h}: the logo is hidden at first"
        );
        assert_eq!(tick_period(&app, &look.effects), Duration::from_millis(16));
        let mut last = first;
        for _ in 0..80 {
            last = loop_frame(
                &mut terminal,
                &mut app,
                &mut look,
                Duration::from_millis(16),
            );
        }
        assert!(!look.effects.reforming(), "played out within 1.3 s");
        for y in 0..3.min(h) {
            assert_eq!(row(&last, y), row(&plain, y), "{w}x{h} header row {y}");
        }
    }
}

/// A terminal too small for any screen draws no header: the exit ends on the first frame,
/// leaves it as drawn, and holds no pane.
#[test]
fn a_too_small_terminal_gets_no_exit() {
    use crate::tui::test_support::profile;
    let mut look = colourful_look();
    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    look.effects
        .hand_over(crate::tui::decoration::intro::Reform::for_test(
            Rect::new(0, 0, 75, 20),
            1,
            &look.theme,
        ));
    let mut terminal = Terminal::new(TestBackend::new(75, 20)).unwrap();
    let first = loop_frame(&mut terminal, &mut app, &mut look, Duration::ZERO);
    assert!(!look.effects.reforming());
    let plain = crate::tui::test_support::frame_with_theme(&app, 75, 20, &look.theme);
    assert_eq!(first, plain, "the frame is left as the screen draws it");
}

/// The light that follows the exit is armed once, on the frame that plans it, and not again.
#[test]
fn the_light_waits_for_the_exit() {
    use crate::tui::test_support::profile;
    let mut look = colourful_look();
    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    look.effects
        .hand_over(crate::tui::decoration::intro::Reform::for_test(
            Rect::new(0, 0, 120, 40),
            3,
            &look.theme,
        ));
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    loop_frame(&mut terminal, &mut app, &mut look, Duration::ZERO);
    assert!(!look.effects.wants_reform_lap(), "armed on the first frame");
}

/// A key a tenth of a second in ends the exit at once: the next frame's header reads what the
/// screen draws, and the held panes start their trace rather than staying hidden.
#[test]
fn a_key_during_the_held_panes_ends_the_exit_and_lets_them_in() {
    use crate::tui::test_support::{frame_with_theme, profile};
    let mut look = colourful_look();
    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    look.effects
        .hand_over(crate::tui::decoration::intro::Reform::for_test(
            Rect::new(0, 0, 120, 40),
            3,
            &look.theme,
        ));
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    loop_frame(&mut terminal, &mut app, &mut look, Duration::ZERO);
    loop_frame(
        &mut terminal,
        &mut app,
        &mut look,
        Duration::from_millis(100),
    );
    look.effects.on_key(Instant::now());
    let next = loop_frame(
        &mut terminal,
        &mut app,
        &mut look,
        Duration::from_millis(16),
    );
    let plain = frame_with_theme(&app, 120, 40, &look.theme);
    for x in 0..30 {
        assert_eq!(
            next.cell((x, 0)).unwrap().symbol(),
            plain.cell((x, 0)).unwrap().symbol()
        );
    }
    assert!(look.effects.active(), "the held trace runs");
}

/// The seam the README screenshots draw through plays the intro's exit over the loop's first
/// frames as the loop does: the header's logo hidden on the first frame, the exit over within
/// 1.2 s, and the logo drawn as the screen draws it after.
#[test]
fn the_drawn_turn_plays_the_intros_exit_and_then_the_plain_screen() {
    let mut app = gungnir();
    let mut look = Look::new(
        TuiConfig::default(),
        false,
        ColorDepth::TrueColor,
        scratch_config(),
    );
    app.set_look(&look.tui, &look.effects);
    let area = Rect::new(0, 0, 120, 40);
    look.effects
        .hand_over(crate::tui::decoration::intro::Reform::for_test(
            area,
            7,
            &look.theme,
        ));
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("a test terminal opens");
    let frame = Duration::from_millis(33);
    let t0 = Instant::now();
    let (first, areas) = drawn_turn(&mut terminal, &mut app, &mut look, t0, Duration::ZERO);
    let Some(Logo::Mark(logo)) = areas.logo else {
        panic!("120 columns draw the small mark");
    };
    let blank = |buffer: &Buffer| {
        (logo.top()..logo.bottom()).all(|y| {
            (logo.left()..logo.right())
                .all(|x| buffer.cell((x, y)).is_some_and(|c| c.symbol() == " "))
        })
    };
    assert!(blank(&first), "the logo is hidden until its sparks land");
    let mut last = first;
    for n in 1..=36u32 {
        last = drawn_turn(&mut terminal, &mut app, &mut look, t0 + frame * n, frame).0;
    }
    assert!(!look.effects.reforming(), "the exit is over by 1.2 s");
    assert!(!blank(&last), "the logo is drawn once the exit is over");
}

/// The exit's first frame holds the panes in the theme's own pane grey, not in the grey the
/// layer starts with before it has seen a theme.
#[test]
fn the_exits_first_frame_holds_the_panes_in_the_themes_pane_grey() {
    use crate::tui::theme::Surface;
    let mut app = gungnir();
    let mut look = Look::new(
        TuiConfig::default(),
        false,
        ColorDepth::TrueColor,
        scratch_config(),
    );
    app.set_look(&look.tui, &look.effects);
    let area = Rect::new(0, 0, 120, 40);
    look.effects
        .hand_over(crate::tui::decoration::intro::Reform::for_test(
            area,
            7,
            &look.theme,
        ));
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("a test terminal opens");
    let pane = look.theme.surface(Surface::Pane).expect("a pane grey");
    assert_ne!(
        pane, SWEPT_FROM,
        "the two greys differ, or this proves nothing"
    );
    let (first, areas) = drawn_turn(
        &mut terminal,
        &mut app,
        &mut look,
        Instant::now(),
        Duration::ZERO,
    );
    let frame = areas.frames.first().expect("the screen draws a pane").1;
    // The exit's sparks fly over the pane; the cells it leaves alone are the pane grey.
    let (mut grey, mut stale) = (0, 0);
    for y in frame.top()..frame.bottom() {
        for x in frame.left()..frame.right() {
            match first.cell((x, y)).expect("in the frame").fg {
                fg if fg == pane => grey += 1,
                fg if fg == SWEPT_FROM => stale += 1,
                _ => {}
            }
        }
    }
    assert_eq!(stale, 0, "cells held in the layer's starting grey");
    assert!(grey > 100, "{grey} cells in the pane grey");
}

/// The loop's own dimming after the animation layer leaves the squirrel under a box in its greys.
#[test]
fn the_squirrel_under_a_box_keeps_its_greys_through_the_loops_dimming() {
    let mut look = colourful_look();
    let mut app = crate::tui::test_support::called_app();
    app.overlays.menu = Some(0);
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).expect("a test terminal opens");
    let buffer = loop_frame(&mut terminal, &mut app, &mut look, Duration::ZERO);
    let dim = look.theme.style(Role::Dim).fg.expect("a dim grey");
    let areas: Vec<(u16, u16)> = (91..91 + crate::tui::decoration::mascot::BOX_W as u16)
        .flat_map(|x| (27..33).map(move |y| (x, y)))
        .filter(|&(x, y)| matches!(buffer.cell((x, y)).expect("a cell").symbol(), "▀" | "▄"))
        .collect();
    assert!(areas.len() > 20, "{} cells", areas.len());
    assert!(
        areas
            .iter()
            .any(|&(x, y)| buffer.cell((x, y)).expect("a cell").fg != dim),
        "the loop's dimming flattened the squirrel"
    );
}

/// The loop's dimming leaves the menu's join row at full strength, over the squirrel as well.
#[test]
fn the_loops_dimming_leaves_the_menus_join_row_alone() {
    let mut look = colourful_look();
    let mut app = crate::tui::test_support::called_app();
    app.overlays.menu = Some(0);
    app.session.join_hint = Some("svolder tui --join /tmp/sd-abcdef/sv.sock".into());
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let buffer = loop_frame(&mut terminal, &mut app, &mut look, Duration::ZERO);
    let dim = look.theme.style(Role::Dim).fg.expect("a dim grey");
    let row: String = (0..80)
        .map(|x| buffer[(x, 17)].symbol().to_string())
        .collect();
    assert!(row.contains("Join with: svolder tui --join"), "{row:?}");
    let at = row[..row.find("Join with").expect("the label")]
        .chars()
        .count();
    for x in at as u16..at as u16 + 40 {
        assert_ne!(buffer[(x, 17)].fg, dim, "column {x} is dimmed");
    }
}

/// Every frame of the called squirrel's screen for a while after the loop armed a screen
/// change, a tick of [`SQUIRREL_TICK`] apart, drawn the way the loop draws and arms it, with
/// `hide` the squirrel switched off.
fn squirrel_timeline(tui: &TuiConfig, hide: bool) -> Vec<Buffer> {
    let mut app = crate::tui::test_support::called_app();
    app.mascot.capable = !hide;
    let mut look = Look::new(tui.clone(), false, ColorDepth::TrueColor, scratch_config());
    app.set_look(&look.tui, &look.effects);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).expect("a test terminal opens");
    let t0 = Instant::now();
    app.frame_flags.screen_changed = true;
    drawn_turn(&mut terminal, &mut app, &mut look, t0, Duration::ZERO);
    (0..40)
        .map(|_| drawn_turn(&mut terminal, &mut app, &mut look, t0, SQUIRREL_TICK).0)
        .collect()
}

/// The time between two frames of [`squirrel_timeline`].
const SQUIRREL_TICK: Duration = Duration::from_millis(20);

/// The cells the squirrel stands in: where the frame with it differs from the one without.
fn squirrel_at(with: &Buffer, without: &Buffer) -> Vec<(u16, u16)> {
    let area = with.area;
    (area.y..area.bottom())
        .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
        .filter(|at| with.cell(*at) != without.cell(*at))
        .collect()
}

/// A screen change, armed the way the loop arms it, brings the squirrel in once the pane it
/// stands in has drawn itself: until the arrival is over, its cells show exactly what the same
/// frame without it shows (its feet on the border as much as its body), and from then on all of
/// it, on the theme's ground and on the terminal's own.
#[test]
fn the_squirrel_comes_in_once_its_pane_has_drawn_itself() {
    for theme_background in [true, false] {
        let tui = TuiConfig {
            theme_background,
            ..TuiConfig::default()
        };
        let with = squirrel_timeline(&tui, false);
        let without = squirrel_timeline(&tui, true);
        let (done, bare) = (
            with.last().expect("frames"),
            without.last().expect("frames"),
        );
        let cells = squirrel_at(done, bare);
        assert!(
            cells.len() > 20,
            "the squirrel stands in {} cells",
            cells.len()
        );
        let mut elapsed = Duration::ZERO;
        for (shown, ground) in with.iter().zip(&without) {
            elapsed += SQUIRREL_TICK;
            let (expected, what) = match elapsed < ARRIVAL_SPAN {
                true => (ground, "the pane without it"),
                false => (done, "the squirrel"),
            };
            for at in &cells {
                assert_eq!(
                    shown.cell(*at),
                    expected.cell(*at),
                    "{at:?} at {elapsed:?} (theme background {theme_background}) is not {what}"
                );
            }
        }
    }
}
