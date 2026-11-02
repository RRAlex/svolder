// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::time::{Duration, Instant};

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use svolder_ipc::{ColorDepth, EffectsConfig, TuiConfig};
use svolder_profiles::Registry;

use crate::backend::Update;
use crate::tui::app::{App, Pane, Screen};
use crate::tui::mouse::HitMap;
use crate::tui::render::{self, FrameAreas};
use crate::tui::test_support::{fg, plain_vo as vo, true_colour};
use crate::tui::theme::{Role, Theme};

use super::*;

/// The terminal the effect cases work in.
const SIZE: (u16, u16) = (40, 6);
/// The terminal a whole screen is drawn in, the smallest any screen is drawn in.
const SCREEN: (u16, u16) = (80, 24);
/// One of the Braille characters the response curve is drawn with.
const DOT: &str = "⠒";
/// A character the curve is never drawn with: a band's own point.
const POINT: &str = "•";

/// The smallest step the reminder tests walk over one of the schedule's instants with.
const TINY: Duration = Duration::from_millis(1);
/// The colour the loop hands a lap, which every case here stands the theme's glow in for
/// with one colour of its own: a frame drawn in no colour at all carries
/// [`Color::Reset`], so whatever the runner painted is exactly this.
const RUNNER_GLOW: Color = Color::Rgb(0xff, 0x55, 0x22);

/// Paint every cell of `area` with `symbol`, so a case has cells to work over.
fn paint(buf: &mut Buffer, area: Rect, symbol: &str) {
    for y in area.y..area.bottom() {
        for x in area.x..area.right() {
            if let Some(cell) = buf.cell_mut((x, y)) {
                cell.set_symbol(symbol);
            }
        }
    }
}

/// What every cell of `buffer` says, which is what no effect may change.
fn symbols(buffer: &Buffer) -> Vec<&str> {
    let area = buffer.area;
    (area.y..area.bottom())
        .flat_map(move |y| {
            (area.x..area.right()).map(move |x| {
                buffer
                    .cell((x, y))
                    .expect("a cell inside the buffer")
                    .symbol()
            })
        })
        .collect()
}

/// One reading as a backend hands it over.
/// An app on the tone-control screen in EQ mode with band 3 cut a little and every other
/// band at 0 dB, which draws a curve to colour.
fn tone_app() -> App {
    tone_app_at(2, 90)
}

/// An app on the tone-control screen in `mode` (the raw mode value), every band at its
/// centre frequency and Q and at 0 dB but band 3, at the raw gain `band_3`.
fn tone_app_at(mode: i64, band_3: i64) -> App {
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get("gungnir2-v2")
        .expect("gungnir2-v2 is built in")
        .clone();
    let mut app = App::new(profile, "gungnir".into(), &TuiConfig::default(), Vec::new());
    let now = Instant::now();
    let mut values = vec![vo("12.50", mode), vo("12.52", 0)];
    for band in 1..=5 {
        values.push(vo(&format!("12.{band}.2"), 64));
        let gain = match band {
            3 => band_3,
            _ => 98,
        };
        values.push(vo(&format!("12.{band}.3"), gain));
        values.push(vo(&format!("12.{band}.1"), 64));
    }
    for value in values {
        app.apply(Update::Status(value), now);
    }
    app.show(Screen::Tone);
    app
}

/// The switches with the hue-drift mode on, which is what the glow travels the spectrum
/// in; everything else is at its default.
fn drifting() -> EffectsConfig {
    EffectsConfig {
        hue_drift: true,
        ..EffectsConfig::default()
    }
}

/// The switches with the spectrum-drift mode on, which is what scrolls the tone curve's
/// colours round the hue circle; everything else is at its default.
fn scrolling() -> EffectsConfig {
    EffectsConfig {
        spectrum_drift: true,
        ..EffectsConfig::default()
    }
}

impl Effects {
    /// Draw one frame of a bordered pane filling `area`, advance the layer by `last_tick`
    /// and answer with the cells that came out, for a case that reads what an effect did
    /// to a frame of its own.
    fn frame_for_test(&mut self, area: Rect, last_tick: Duration) -> Buffer {
        self.frames_for_test(&[area], last_tick)
    }

    /// The same for a screen of several bordered panes, for a case about what one
    /// animation does to more than one of them at once.
    ///
    /// The panes are drawn in no colour at all, so every cell the effect has not touched
    /// carries [`Color::Reset`] and whatever an effect painted stands out against it.
    fn frames_for_test(&mut self, panes: &[Rect], last_tick: Duration) -> Buffer {
        let width = panes.iter().map(|pane| pane.right()).max().unwrap_or(1);
        let height = panes.iter().map(|pane| pane.bottom()).max().unwrap_or(1);
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
        terminal
            .draw(|frame| {
                for pane in panes {
                    frame.render_widget(Block::bordered(), *pane);
                }
                self.render(frame, last_tick);
            })
            .expect("the frame draws");
        terminal.backend().buffer().clone()
    }

    /// Draw one frame of nothing but the layer itself and advance it by `last_tick`, for a
    /// case that reads a drift rather than a screen: the drifts are worked out by
    /// [`Effects::render`], so a case that only wants them still has to render.
    fn render_for_test(&mut self, last_tick: Duration) {
        let mut terminal =
            Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
        terminal
            .draw(|frame| self.render(frame, last_tick))
            .expect("the frame draws");
    }
}

/// One frame of `app` drawn through `effects` at true colour.
fn frame(app: &App, effects: &mut Effects, last_tick: Duration) -> (Buffer, FrameAreas) {
    frame_with(app, effects, last_tick, &true_colour())
}

/// One frame of `app` drawn through `effects` on `theme`, exactly as the event loop draws
/// it, through the loop's own `paint_over`.
fn frame_with(
    app: &App,
    effects: &mut Effects,
    last_tick: Duration,
    theme: &Theme,
) -> (Buffer, FrameAreas) {
    let mut terminal =
        Terminal::new(TestBackend::new(SCREEN.0, SCREEN.1)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut areas = FrameAreas::default();
    terminal
        .draw(|frame| {
            areas = render::draw(frame, app, theme, &mut hits);
            crate::tui::run::paint_over(frame, &areas, theme, effects, last_tick);
        })
        .expect("the frame draws");
    (terminal.backend().buffer().clone(), areas)
}

/// The helper draws what the loop draws: a box over the screen dims everything around it,
/// whatever the animation layer painted there.
#[test]
fn a_frame_under_a_box_is_dimmed_around_it() {
    let mut app = tone_app();
    app.overlays.help_open = true;
    let mut effects = Effects::new(true, &EffectsConfig::default());
    effects.on_screen_change(Rect::new(0, 0, SCREEN.0, SCREEN.1));
    let theme = true_colour();
    let (buffer, areas) = frame_with(&app, &mut effects, Duration::from_millis(100), &theme);
    assert!(effects.active(), "the sweep is still running");
    let modal = areas.modal.expect("the help box is a modal");
    let dim = theme.style(Role::Dim).fg.unwrap();
    let outside: Vec<Color> = (0..SCREEN.0)
        .flat_map(|x| (0..SCREEN.1).map(move |y| (x, y)))
        .filter(|&(x, y)| !modal.contains((x, y).into()))
        .filter(|&(x, y)| !buffer.cell((x, y)).unwrap().symbol().trim().is_empty())
        .map(|(x, y)| buffer.cell((x, y)).unwrap().fg)
        .collect();
    assert!(!outside.is_empty(), "text outside the box");
    assert!(
        outside.iter().all(|&fg| fg == dim),
        "a cell outside the box kept its colour"
    );
}

/// The colours the curve's own Braille cells carry across `curve`, column by column, for
/// the columns the curve passes through.
fn curve_colours(buffer: &Buffer, curve: Rect) -> Vec<Color> {
    (curve.x..curve.right())
        .filter_map(|x| {
            (curve.y..curve.bottom()).find_map(|y| {
                let cell = buffer.cell((x, y)).expect("a cell inside the buffer");
                is_braille(cell.symbol()).then_some(cell.fg)
            })
        })
        .collect()
}

/// With effects off nothing is armed, nothing is drawn and no animation recolours a cell:
/// the frame comes out of the animation layer exactly as the widgets left it.
#[test]
fn effects_that_are_off_run_nothing_at_all() {
    let mut effects = Effects::new(false, &EffectsConfig::default());
    let area = Rect::new(0, 0, SIZE.0, SIZE.1);
    effects.on_screen_change(area);
    effects.on_fault(area);
    effects.on_reconnect(area);
    assert!(!effects.active(), "an effect was armed with effects off");

    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    let (mut before, mut after) = (Buffer::empty(area), Buffer::empty(area));
    terminal
        .draw(|frame| {
            paint(frame.buffer_mut(), area, DOT);
            before = frame.buffer_mut().clone();
            effects.render(frame, Duration::from_millis(100));
            after = frame.buffer_mut().clone();
        })
        .expect("the frame draws");

    assert_eq!(
        before, after,
        "an effect touched the frame with effects off"
    );
    assert!(!effects.active(), "nothing is running to redraw fast for");
}

/// A screen change sweeps once, and the sweep ends on its own: the loop asks
/// [`Effects::active`] whether to keep redrawing fast, so an effect that never finished
/// would hold the fast tick for the rest of the run.
#[test]
fn a_screen_change_sweeps_and_the_sweep_ends() {
    let mut effects = Effects::new(true, &EffectsConfig::default());
    let area = Rect::new(0, 0, SIZE.0, SIZE.1);
    effects.on_screen_change(area);
    assert!(effects.active(), "a screen change did not start the sweep");

    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    terminal
        .draw(|frame| {
            paint(frame.buffer_mut(), area, DOT);
            effects.render(frame, Duration::from_millis(1_000));
        })
        .expect("the frame draws");

    assert!(
        !effects.active(),
        "a second is longer than the sweep, which is still running"
    );
}

/// The spectrum colours the curve's own Braille cells by frequency and leaves everything
/// else — the axes, the labels, the bands' points — with the role the theme gave it. No
/// cell says anything different afterwards.
#[test]
fn the_spectrum_colours_the_curves_braille_cells_and_nothing_else() {
    let curve = Rect::new(0, 0, 40, 1);
    let mut buffer = Buffer::empty(curve);
    paint(&mut buffer, curve, DOT);
    buffer
        .cell_mut((5, 0))
        .expect("a cell inside the buffer")
        .set_symbol(POINT)
        .set_fg(Color::Yellow);
    let before = buffer.clone();

    spectrum(&mut buffer, curve, &true_colour(), 0.0, false);

    let (first, last) = (fg(&buffer, 0, 0), fg(&buffer, curve.right() - 1, 0));
    assert!(
        matches!(first, Color::Rgb(..)) && matches!(last, Color::Rgb(..)),
        "the curve is not coloured as a spectrum: {first:?} to {last:?}"
    );
    assert_ne!(first, last, "the spectrum is one colour across the band");
    assert_eq!(
        fg(&buffer, 5, 0),
        Color::Yellow,
        "a cell that is not the curve was recoloured"
    );
    assert_eq!(
        symbols(&before),
        symbols(&buffer),
        "the spectrum changed what a cell says"
    );
}

/// The spectrum runs the way light does: red at the curve's lowest frequency, violet at
/// its highest, with no drift carrying it round.
#[test]
fn the_spectrum_runs_from_red_at_the_lowest_frequency_to_violet_at_the_highest() {
    let curve = Rect::new(0, 0, 40, 1);
    let mut buffer = Buffer::empty(curve);
    paint(&mut buffer, curve, DOT);

    spectrum(&mut buffer, curve, &true_colour(), 0.0, false);

    let Color::Rgb(r, g, b) = fg(&buffer, curve.x, 0) else {
        panic!("the low end of the curve is not coloured");
    };
    assert!(
        r > 200 && g < 100 && b < 100,
        "the lowest frequency is not red: {r},{g},{b}"
    );
    let Color::Rgb(r, g, b) = fg(&buffer, curve.right() - 1, 0) else {
        panic!("the high end of the curve is not coloured");
    };
    assert!(
        b > 200 && g < 100 && (100..b).contains(&r),
        "the highest frequency is not violet: {r},{g},{b}"
    );
}

/// The colouring is what the curve says rather than an animation over it, so a frame drawn
/// with the animation layer off still runs red to violet across the curve — and stands
/// still, since only the drift is the layer's.
#[test]
fn the_curve_keeps_its_spectrum_with_the_animation_layer_off() {
    let app = tone_app();
    let mut off = Effects::new(false, &EffectsConfig::default());
    let (buffer, areas) = frame(&app, &mut off, Duration::from_secs(5));
    let curve = areas.curve.expect("the tone screen draws a curve");
    let colours = curve_colours(&buffer, curve);
    assert!(colours.len() > 2, "the curve was not drawn: {colours:?}");

    let Some(Color::Rgb(r, g, b)) = colours.first().copied() else {
        panic!("the low end of the curve is not coloured with effects off: {colours:?}");
    };
    assert!(
        r > 200 && g < 100 && b < 100,
        "the lowest frequency is not red with effects off: {r},{g},{b}"
    );
    let Some(Color::Rgb(r, g, b)) = colours.last().copied() else {
        panic!("the high end of the curve is not coloured with effects off: {colours:?}");
    };
    assert!(
        b > 150 && g < 100 && r < b,
        "the highest frequency is not violet with effects off: {r},{g},{b}"
    );

    let (later, _) = frame(&app, &mut off, Duration::from_secs(5));
    assert_eq!(
        curve_colours(&later, curve),
        colours,
        "the spectrum drifted with the layer off"
    );
}

/// A curve the bands shape (EQ mode, a band off 0 dB) is handed to the spectrum unmuted; a
/// flat curve in EQ mode and Flat mode, whatever the bands hold, are handed over muted
/// instead of left ungiven, so they are coloured desaturated rather than left in the dim
/// role; Loudness mode's line of text draws no curve at all, so it gets neither.
#[test]
fn the_tone_screen_reports_a_flat_curve_and_flat_mode_muted() {
    let theme = true_colour();
    for (what, app) in [
        ("a flat curve", tone_app_at(2, 98)),
        ("flat mode", tone_app_at(1, 90)),
    ] {
        let mut effects = Effects::new(true, &scrolling());
        for tick in [Duration::ZERO, Duration::from_secs(5)] {
            let (_, areas) = frame_with(&app, &mut effects, tick, &theme);
            assert!(areas.curve.is_some(), "{what}: no curve reported");
            assert!(areas.curve_muted, "{what}: not reported muted");
        }
    }
    let shaped = tone_app();
    let mut effects = Effects::new(true, &scrolling());
    let (_, areas) = frame_with(&shaped, &mut effects, Duration::ZERO, &theme);
    assert!(areas.curve.is_some(), "a shaped curve reported none");
    assert!(!areas.curve_muted, "a shaped curve reported muted");

    let loudness = tone_app_at(3, 90);
    let (_, areas) = frame_with(&loudness, &mut effects, Duration::ZERO, &theme);
    assert_eq!(areas.curve, None, "loudness mode reported a curve");
}

/// A muted spectrum colours the point cells too, since the curve and its points are one
/// faded thing there rather than a curve with points marked on it — unlike a shaped
/// curve's, which leaves them in the roles the theme gave them
/// ([`the_spectrum_colours_the_curves_braille_cells_and_nothing_else`] above).
#[test]
fn a_muted_spectrum_colours_the_points_too() {
    let curve = Rect::new(0, 0, 40, 1);
    let mut buffer = Buffer::empty(curve);
    paint(&mut buffer, curve, DOT);
    buffer
        .cell_mut((5, 0))
        .expect("a cell inside the buffer")
        .set_symbol(POINT)
        .set_fg(Color::Yellow);

    spectrum(&mut buffer, curve, &true_colour(), 0.0, true);

    assert_ne!(
        fg(&buffer, 5, 0),
        Color::Yellow,
        "a muted spectrum left a band's point uncoloured"
    );
}

/// The muted spectrum runs red to violet too, its hue kept but its saturation cut — less
/// saturated than the shaped spectrum's colours at the same columns — and never the flat
/// dim grey; and, unlike the shaped spectrum, it never drifts: `hue_offset` is ignored while
/// muted, so the rainbow scroll carries nothing there.
#[test]
fn the_muted_spectrum_keeps_the_hue_order_desaturated_and_never_drifts() {
    let theme = true_colour();
    let dim = theme.style(Role::Dim).fg.expect("the dim role's colour");
    let curve = Rect::new(0, 0, 40, 1);
    let drawn = |offset, muted| {
        let mut buffer = Buffer::empty(curve);
        paint(&mut buffer, curve, DOT);
        spectrum(&mut buffer, curve, &theme, offset, muted);
        buffer
    };
    let shaped = drawn(0.0, false);
    let dulled = drawn(0.0, true);
    let spread = |c: Color| match c {
        Color::Rgb(r, g, b) => r.max(g).max(b) - r.min(g).min(b),
        _ => 0,
    };
    for x in curve.x..curve.right() {
        let (p, m) = (fg(&shaped, x, 0), fg(&dulled, x, 0));
        assert!(
            spread(m) < spread(p),
            "column {x}: muted {m:?} is not less saturated than the shaped {p:?}"
        );
        assert_ne!(
            m, dim,
            "column {x}: the muted spectrum is the flat dim grey"
        );
    }
    let Color::Rgb(r, g, b) = fg(&dulled, curve.x, 0) else {
        panic!("the low end is not coloured");
    };
    assert!(
        r > g && r > b,
        "the low end is not still reddish: {r},{g},{b}"
    );
    let Color::Rgb(r, g, b) = fg(&dulled, curve.right() - 1, 0) else {
        panic!("the high end is not coloured");
    };
    assert!(
        b > g && b >= r,
        "the high end is not still blue-violet: {r},{g},{b}"
    );

    assert_eq!(
        drawn(90.0, true),
        dulled,
        "the rainbow scroll carried the muted curve round the circle"
    );
}

/// On `red` the tone screen's curve runs the palette's warm ramp rather than the hue
/// circle — dark red at 20 Hz to the off-white at 20 kHz, no column of it blue or cyan —
/// shaped or muted, and the muted one stays where it rests however far the scroll has gone.
#[test]
fn on_red_the_curve_runs_dark_red_to_off_white_and_never_blue() {
    let red = Theme::from_config(
        &TuiConfig {
            palette: "red".into(),
            ..TuiConfig::default()
        },
        false,
        ColorDepth::TrueColor,
    );
    for (what, app) in [("shaped", tone_app()), ("muted", tone_app_at(2, 98))] {
        let mut off = Effects::new(false, &EffectsConfig::default());
        let (buffer, areas) = frame_with(&app, &mut off, Duration::ZERO, &red);
        let curve = areas.curve.expect("the tone screen draws a curve");
        let colours = curve_colours(&buffer, curve);
        assert!(
            colours.len() > 2,
            "{what}: the curve was not drawn: {colours:?}"
        );
        for colour in &colours {
            assert!(
                !crate::tui::theme::bluish(crate::tui::palette::rgb_of(*colour)),
                "{what}: a column of the curve is {colour:?}"
            );
        }
        if what == "shaped" {
            assert_eq!(
                colours.first(),
                Some(&Color::Rgb(0xa0, 0x24, 0x14)),
                "20 Hz"
            );
            assert_eq!(
                colours.last(),
                Some(&Color::Rgb(0xf2, 0xf0, 0xea)),
                "20 kHz"
            );
        }
    }
    let curve = Rect::new(0, 0, 40, 1);
    let drawn = |offset, muted| {
        let mut buffer = Buffer::empty(curve);
        paint(&mut buffer, curve, DOT);
        spectrum(&mut buffer, curve, &red, offset, muted);
        buffer
    };
    assert_ne!(
        drawn(90.0, false),
        drawn(0.0, false),
        "the scroll moved nothing"
    );
    assert_eq!(
        drawn(90.0, true),
        drawn(0.0, true),
        "the rainbow scroll carried the muted curve along"
    );
}

/// A terminal with sixteen colours has no spectrum to draw in, so the curve keeps the role
/// the tone screen gave it there.
#[test]
fn a_sixteen_colour_terminal_keeps_the_curve_in_its_own_role() {
    let app = tone_app();
    let theme = Theme::new(false);
    let mut off = Effects::new(false, &EffectsConfig::default());
    let (buffer, areas) = frame_with(&app, &mut off, Duration::ZERO, &theme);
    let curve = areas.curve.expect("the tone screen draws a curve");
    let wanted = theme.style(Role::CurveSum).fg.expect("the curve's colour");
    let colours = curve_colours(&buffer, curve);
    assert!(colours.len() > 2, "the curve was not drawn: {colours:?}");
    assert!(
        colours.iter().all(|colour| *colour == wanted),
        "the curve left its own role at sixteen colours: {colours:?}"
    );
}

/// The hue drifts as the frames go by, so the same curve is a different spectrum a moment
/// later — and stays inside the circle rather than growing without end.
#[test]
fn the_spectrum_drifts_round_the_hue_circle() {
    let mut effects = Effects::new(true, &scrolling());
    let app = tone_app();
    assert_eq!(
        effects.spectrum_hue(),
        0.0,
        "the drift starts where it starts"
    );

    let (early, _) = frame(&app, &mut effects, Duration::from_secs(5));
    let drifted = effects.spectrum_hue();
    assert!(
        (drifted - 90.0).abs() < 1.0,
        "a quarter of the twenty-second period is a quarter turn, not {drifted}°"
    );
    let (later, _) = frame(&app, &mut effects, Duration::ZERO);
    assert_ne!(early, later, "the spectrum did not drift with the hue");

    let (_, _) = frame(&app, &mut effects, Duration::from_secs(60));
    assert!(
        (0.0..HUE_CIRCLE).contains(&effects.spectrum_hue()),
        "the drift left the hue circle: {}°",
        effects.spectrum_hue()
    );
}

/// With the spectrum-drift switch off, the curve rests at hue 0 — red at the lowest
/// frequency, violet at the highest — however long the layer runs, even though the layer
/// itself is on.
#[test]
fn with_spectrum_drift_off_the_curve_rests_at_hue_zero() {
    let mut effects = Effects::new(true, &EffectsConfig::default());
    let app = tone_app();
    frame(&app, &mut effects, Duration::from_secs(60));
    assert_eq!(
        effects.spectrum_hue(),
        0.0,
        "the spectrum drifted with the switch off"
    );
}

/// A fault the device keeps reporting keeps the rows that say so pulsing: the pulse is
/// still running three seconds on, it is not restarted by the frames that go by, and it
/// is gone one frame after the device reports the fault clear.
#[test]
fn a_standing_fault_pulses_until_it_clears() {
    let mut effects = Effects::new(true, &EffectsConfig::default());
    let header = Rect::new(0, 0, SIZE.0, 2);
    let line = Rect::new(0, 4, SIZE.0, 1);
    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");

    effects.on_faults(&[header, line], Color::Red);
    assert!(effects.active(), "a standing fault pulsed nothing");
    let mut lit = Vec::new();
    for _ in 0..30 {
        // Every frame says the fault still stands, exactly as the loop does.
        effects.on_faults(&[header, line], Color::Red);
        terminal
            .draw(|frame| {
                let area = frame.area();
                paint(frame.buffer_mut(), area, DOT);
                effects.render(frame, Duration::from_millis(100));
                lit.push(fg(frame.buffer_mut(), 0, 0));
            })
            .expect("the frame draws");
    }
    assert!(
        effects.active(),
        "the pulse gave up before the fault did, three seconds in"
    );
    assert!(
        lit.iter().any(|colour| *colour != lit[0]),
        "the pulse held one colour for three seconds: {:?}",
        lit[0]
    );

    effects.on_faults(&[], Color::Red);
    terminal
        .draw(|frame| effects.render(frame, Duration::from_millis(100)))
        .expect("the frame draws");
    assert!(
        !effects.active(),
        "the pulse outlasted the fault that started it"
    );
}

/// The pulse plays over the rows the fault is written on and changes nothing they say.
#[test]
fn the_fault_pulse_recolours_its_rows_and_says_nothing_new() {
    let mut effects = Effects::new(true, &EffectsConfig::default());
    let header = Rect::new(0, 0, SIZE.0, 1);
    effects.on_faults(&[header], Color::Red);

    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    let (mut before, mut after) = (Buffer::empty(header), Buffer::empty(header));
    terminal
        .draw(|frame| {
            let area = frame.area();
            paint(frame.buffer_mut(), area, DOT);
            frame
                .buffer_mut()
                .set_style(area, Style::default().fg(Color::Rgb(0xf2, 0xf0, 0xea)));
            before = frame.buffer_mut().clone();
            effects.render(frame, Duration::from_millis(750));
            after = frame.buffer_mut().clone();
        })
        .expect("the frame draws");

    assert_ne!(
        fg(&after, 0, 0),
        fg(&before, 0, 0),
        "the pulse left the row it plays over exactly as it was"
    );
    assert_eq!(
        fg(&after, 0, 2),
        fg(&before, 0, 2),
        "the pulse reached a row the fault is not written on"
    );
    assert_eq!(
        symbols(&before),
        symbols(&after),
        "the pulse changed what a cell says"
    );
}

/// The fault pulse has a switch of its own, and the layer's own switch takes it with
/// everything else.
#[test]
fn the_fault_pulse_is_held_back_by_its_own_switch() {
    let header = Rect::new(0, 0, SIZE.0, 1);
    let without = EffectsConfig {
        fault_presence: false,
        ..EffectsConfig::default()
    };
    let mut effects = Effects::new(true, &without);
    effects.on_faults(&[header], Color::Red);
    assert!(!effects.active(), "a switched-off pulse was armed");

    let mut off = Effects::new(false, &EffectsConfig::default());
    off.on_faults(&[header], Color::Red);
    assert!(!off.active(), "a pulse was armed with effects off");
}

/// Without the mode that asks for the glow to travel, the decoration rests on the accent
/// however loud the device is: a user who has not switched the mode on sees the palette's
/// own colour whatever the music does.
#[test]
fn with_hue_drift_off_the_glow_rests_whatever_the_meters_say() {
    let mut fx = Effects::new(true, &EffectsConfig::default());
    fx.set_level(Some(1.0));
    fx.render_for_test(Duration::from_millis(500));
    assert_eq!(fx.glow_position(), 0.0);
}

/// With the mode on and nothing playing, the glow breathes slowly through the cool half of
/// the spectrum: never past the accent it rests on, never further out than the ripple
/// reaches, and never still.
#[test]
fn with_hue_drift_on_and_no_level_the_glow_ripples_in_the_cool_half() {
    // A quarter of the ripple's own twenty-second loop carries the wave halfway out,
    // which is half of the reach into the cool half: the reach and the period, pinned.
    let mut quarter = Effects::new(true, &drifting());
    quarter.set_level(None);
    quarter.render_for_test(Duration::from_secs(5));
    assert!(
        (quarter.glow_position() + 0.3).abs() < 0.001,
        "five seconds of the ripple is not half its reach: {}",
        quarter.glow_position()
    );

    let mut fx = Effects::new(true, &drifting());
    fx.set_level(None);
    let mut seen = Vec::new();
    for _ in 0..80 {
        fx.render_for_test(Duration::from_millis(250));
        seen.push(fx.glow_position());
    }
    assert!(seen.iter().all(|g| (-0.6..=0.0).contains(g)), "{seen:?}");
    assert!(seen.iter().any(|g| *g < -0.3), "it moves: {seen:?}");
}

/// A device whose meters are reading carries the glow to where they read, the ripple
/// riding on top of it — and lets it fall back as the music does.
#[test]
fn a_level_pushes_the_glow_toward_the_hot_end() {
    let mut fx = Effects::new(true, &drifting());
    fx.set_level(Some(0.8));
    fx.render_for_test(Duration::from_millis(250));
    assert!(
        (0.7..=0.9).contains(&fx.glow_position()),
        "{}",
        fx.glow_position()
    );
    // The ripple rides on the level rather than standing in for it: a quarter of a second
    // in the wave is at the bottom of its swing, half of the swing below the reading.
    assert!(
        (fx.glow_position() - (0.8 - RIPPLE_SWING / 2.0)).abs() < 0.01,
        "the swing is not {RIPPLE_SWING} from end to end: {}",
        fx.glow_position()
    );
    fx.set_level(Some(0.0));
    for _ in 0..8 {
        fx.render_for_test(Duration::from_millis(250));
    }
    assert!(
        fx.glow_position() <= 0.15,
        "it falls back: {}",
        fx.glow_position()
    );
}

/// The eased level answers the same real time whatever the tick rate happened to be: one
/// tick of a quarter second and eight ticks that add up to the same quarter second close
/// the same share of the gap, so a burst of readings does not move the decoration faster
/// just because some other animation is forcing the loop to redraw every frame.
#[test]
fn the_level_ease_answers_the_same_real_time_whatever_the_tick_rate() {
    let prime = |fx: &mut Effects| {
        fx.set_level(Some(1.0));
        fx.render_for_test(Duration::ZERO);
        fx.set_level(Some(0.0));
    };

    let mut one_tick = Effects::new(true, &EffectsConfig::default());
    prime(&mut one_tick);
    one_tick.render_for_test(Duration::from_millis(250));

    let mut eight_ticks = Effects::new(true, &EffectsConfig::default());
    prime(&mut eight_ticks);
    for _ in 0..8 {
        eight_ticks.render_for_test(Duration::from_micros(31_250));
    }

    let (slow, fast) = (
        one_tick.smoothed.expect("a level was primed"),
        eight_ticks.smoothed.expect("a level was primed"),
    );
    assert!(
        (slow - fast).abs() < 0.01,
        "one slow tick and eight fast ticks over the same quarter second landed apart: \
             {slow} vs {fast}"
    );
}

/// The glow rests at the accent the moment the mode or the layer is off, however far along
/// the spectrum it had travelled, and a level that is not a number is no level at all.
#[test]
fn a_glow_the_layer_no_longer_shows_rests_on_the_accent() {
    let mut effects = Effects::new(true, &drifting());
    effects.set_level(Some(0.9));
    effects.render_for_test(Duration::from_millis(250));
    assert!(effects.glow_position() > 0.5, "the glow did not travel");

    effects.configure(&EffectsConfig::default());
    effects.render_for_test(Duration::from_millis(250));
    assert_eq!(effects.glow_position(), 0.0, "the glow stayed adrift");

    let mut incapable = Effects::new(false, &drifting());
    incapable.set_level(Some(0.9));
    incapable.render_for_test(Duration::from_millis(250));
    assert_eq!(
        incapable.glow_position(),
        0.0,
        "a terminal that shows no effects travelled its glow"
    );

    let mut nonsense = Effects::new(true, &drifting());
    nonsense.set_level(Some(f64::NAN));
    nonsense.render_for_test(Duration::from_millis(250));
    let glow = nonsense.glow_position();
    assert!(
        glow.is_finite() && (-0.6..=0.0).contains(&glow),
        "a level that is not a number left the glow at {glow}"
    );
}

/// A layer or a mode switched off lets go of the level it was easing, so the first frame
/// after it comes back eases from what the meters read then rather than from what they
/// read before: the glow lands on the reading itself rather than travelling down to it
/// from wherever it was left.
#[test]
fn a_layer_switched_off_forgets_the_level_it_was_easing() {
    let quiet_again = |fx: &mut Effects| {
        fx.set_level(Some(0.0));
        fx.render_for_test(Duration::from_millis(250));
        assert!(
            fx.glow_position() < 0.05,
            "the glow is still coming down from the level it held: {}",
            fx.glow_position()
        );
    };

    let mut switched = Effects::new(true, &drifting());
    switched.set_level(Some(1.0));
    switched.render_for_test(Duration::from_millis(250));
    assert!(switched.glow_position() > 0.9, "the glow travelled");
    switched.configure(&EffectsConfig::default());
    switched.configure(&drifting());
    quiet_again(&mut switched);

    let mut dimmed = Effects::new(true, &drifting());
    dimmed.set_level(Some(1.0));
    dimmed.render_for_test(Duration::from_millis(250));
    dimmed.set_capable(false);
    dimmed.set_capable(true);
    quiet_again(&mut dimmed);
}

/// The spectrum's own drift is not the glow's: with the spectrum-drift switch on and the
/// hue-drift mode off, the spectrum still turns and the glow still does not.
#[test]
fn the_spectrum_drifts_whether_or_not_the_glow_does() {
    let app = tone_app();
    let mut plain = Effects::new(true, &scrolling());
    frame(&app, &mut plain, Duration::from_secs(5));
    assert!(plain.spectrum_hue() > 0.0, "the spectrum drifts regardless");
    assert_eq!(
        plain.glow_position(),
        0.0,
        "the glow travelled without the mode being asked for"
    );
}

/// `what`, armed by `arm`, is still running fifty milliseconds short of `burst` and over
/// fifty milliseconds past it.
fn runs_for_its_burst(what: &str, burst: u32, arm: impl FnOnce(&mut Effects)) {
    const MARGIN: Duration = Duration::from_millis(50);
    let burst = Duration::from_millis(u64::from(burst));
    let mut effects = Effects::new(true, &EffectsConfig::default());
    arm(&mut effects);
    assert!(effects.active(), "{what} armed nothing");

    let area = Rect::new(0, 0, SIZE.0, SIZE.1);
    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    terminal
        .draw(|frame| {
            paint(frame.buffer_mut(), area, DOT);
            effects.render(frame, burst - MARGIN);
        })
        .expect("the frame draws");
    assert!(effects.active(), "{what} ended before its burst was up");
    terminal
        .draw(|frame| effects.render(frame, MARGIN * 2))
        .expect("the frame draws");
    assert!(!effects.active(), "{what} outlasted its burst");
}

/// Each of the three animations a row earns runs for its own burst and ends there — the
/// value's glow and the cursor's trail three hundred milliseconds, the shimmer's crossing
/// ([`shimmer_ms`]) — which is what lets the loop fall back to its slow tick between them.
#[test]
fn a_value_change_a_cursor_move_and_a_shimmer_each_run_for_their_burst() {
    let row = Rect::new(0, 0, SIZE.0, 1);
    runs_for_its_burst("a value change", VALUE_MS, |fx| {
        fx.on_value_change(row, Color::Yellow)
    });
    runs_for_its_burst("a cursor move", CURSOR_MS, |fx| fx.on_cursor_move(row));
    runs_for_its_burst("a shimmer", shimmer_ms(row.width), |fx| {
        fx.on_shimmer(row);
    });
}

/// The shimmer moves a bright band across the row it is given and leaves the rest of it
/// alone — and says nothing the row did not say already.
#[test]
fn the_shimmer_lights_part_of_the_row_and_says_nothing_new() {
    let row = Rect::new(0, 0, SIZE.0, 1);
    let mut effects = Effects::new(true, &EffectsConfig::default());
    effects.on_shimmer(row);

    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    let (mut before, mut after) = (Buffer::empty(row), Buffer::empty(row));
    terminal
        .draw(|frame| {
            paint(frame.buffer_mut(), row, DOT);
            before = frame.buffer_mut().clone();
            effects.render(frame, Duration::from_millis(150));
            after = frame.buffer_mut().clone();
        })
        .expect("the frame draws");

    let lit: Vec<u16> = (row.x..row.right())
        .filter(|x| {
            let cell = after.cell((*x, row.y)).expect("a cell inside the buffer");
            cell.fg == Color::White && cell.modifier.contains(Modifier::BOLD)
        })
        .collect();
    assert_eq!(lit.len(), usize::from(SHIMMER_BAND), "{lit:?}");
    assert!(
        lit.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "the band is not one run of cells: {lit:?}"
    );
    assert_eq!(
        symbols(&before),
        symbols(&after),
        "the shimmer changed what a cell says"
    );
}

/// The shimmer's band takes [`shimmer_ms`] to cross a row, and crosses it at one steady pace:
/// each quarter of that time carries it a quarter of the way, the first quarter as far as the
/// last, and the row is left alone once the time is up.
#[test]
fn the_shimmer_crosses_its_row_at_a_steady_pace() {
    const STEPS: u32 = 4;
    let row = Rect::new(0, 0, SIZE.0, 1);
    let crossing = Duration::from_millis(u64::from(shimmer_ms(row.width)));
    let mut effects = Effects::new(true, &EffectsConfig::default());
    effects.on_shimmer(row);

    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    let travel = f32::from(row.width + SHIMMER_BAND);
    for step in 1..STEPS {
        let mut after = Buffer::empty(row);
        terminal
            .draw(|frame| {
                paint(frame.buffer_mut(), row, DOT);
                effects.render(frame, crossing / STEPS);
                after = frame.buffer_mut().clone();
            })
            .expect("the frame draws");
        let lit: Vec<u16> = (row.x..row.right())
            .filter(|x| {
                let cell = after.cell((*x, row.y)).expect("a cell inside the buffer");
                cell.fg == Color::White && cell.modifier.contains(Modifier::BOLD)
            })
            .collect();
        let front = travel * step as f32 / STEPS as f32;
        let expected: Vec<u16> = (row.x..row.right())
            .filter(|x| {
                let along = f32::from(*x - row.x);
                along >= front - f32::from(SHIMMER_BAND) && along < front
            })
            .collect();
        assert_eq!(
            lit, expected,
            "{step} quarter(s) of the way through, the band is not a quarter of the row \
                 further on"
        );
        assert!(effects.active(), "the shimmer ended at step {step}");
    }
    terminal
        .draw(|frame| effects.render(frame, crossing / STEPS + Duration::from_millis(5)))
        .expect("the frame draws");
    assert!(!effects.active(), "the shimmer outlasted its crossing");
}

/// The band travels at one pace in cells per second whatever the width of the row it crosses:
/// a row twice as wide takes twice as long, and the head is the same number of cells along
/// after the same time.
#[test]
fn the_shimmer_sweeps_every_row_at_the_same_cells_per_second() {
    let head = |width: u16, after: Duration| -> f32 {
        let row = Rect::new(0, 0, width, 1);
        let mut effects = Effects::new(true, &EffectsConfig::default());
        effects.on_shimmer(row);
        let mut terminal =
            Terminal::new(TestBackend::new(width, 1)).expect("a test terminal opens");
        let mut lit = Vec::new();
        terminal
            .draw(|frame| {
                paint(frame.buffer_mut(), row, DOT);
                effects.render(frame, after);
                lit = (0..width)
                    .filter(|x| {
                        let cell = frame.buffer_mut().cell((*x, 0)).expect("a cell").clone();
                        cell.fg == Color::White && cell.modifier.contains(Modifier::BOLD)
                    })
                    .collect();
            })
            .expect("the frame draws");
        f32::from(*lit.last().expect("the band is on the row")) + 1.0
    };
    let after = Duration::from_millis(250);
    let (narrow, wide) = (head(40, after), head(110, after));
    assert!(
        (narrow - wide).abs() <= 1.0,
        "the band is {narrow} cells along a narrow row and {wide} along a wide one"
    );
    let pace = narrow / after.as_secs_f32();
    assert!(
        (pace - SHIMMER_CELLS_PER_S).abs() <= 6.0,
        "the pace is {pace} cells a second"
    );
    assert!(shimmer_ms(110) > shimmer_ms(40));
}

/// A row so narrow that it would flash by in a blink still takes long enough to show a frame
/// or two of the band.
#[test]
fn a_tiny_row_still_shimmers_for_a_moment() {
    assert!(shimmer_ms(1) >= SHIMMER_FLOOR_MS);
}

/// After the user's last key the shimmer comes back on the back-off schedule: once at
/// each of [`REMINDERS`], never twice at one of them, and never again once the last has
/// played — the screen is left alone until the next key.
#[tokio::test(start_paused = true)]
async fn the_shimmer_reminds_on_the_back_off_schedule_and_then_rests() {
    let now = || tokio::time::Instant::now().into_std();
    let mut effects = Effects::new(true, &EffectsConfig::default());
    assert!(
        !effects.remind(now()),
        "a reminder came due before any key was pressed"
    );

    effects.on_key(now());
    let mut waited = Duration::ZERO;
    for due in REMINDERS {
        tokio::time::advance(due - waited - TINY).await;
        assert!(!effects.remind(now()), "the reminder at {due:?} came early");
        tokio::time::advance(TINY).await;
        assert!(effects.remind(now()), "no reminder at {due:?}");
        assert!(
            !effects.remind(now()),
            "the reminder at {due:?} came round twice"
        );
        waited = due;
    }

    tokio::time::advance(Duration::from_secs(600)).await;
    assert!(
        !effects.remind(now()),
        "the shimmer did not rest after the last reminder"
    );
}

/// Every key starts the schedule again, so the first reminder is two seconds after the
/// user stopped rather than two seconds after they began — and a layer that is switched
/// off reminds of nothing at all.
#[tokio::test(start_paused = true)]
async fn a_key_starts_the_reminder_schedule_again_and_a_layer_thats_off_never_reminds() {
    let now = || tokio::time::Instant::now().into_std();
    let mut effects = Effects::new(true, &EffectsConfig::default());
    effects.on_key(now());
    tokio::time::advance(REMINDERS[0]).await;
    assert!(effects.remind(now()), "no first reminder");

    effects.on_key(now());
    assert!(
        !effects.remind(now()),
        "a key left the schedule where it was"
    );
    tokio::time::advance(REMINDERS[0] - TINY).await;
    assert!(!effects.remind(now()), "the schedule did not start again");
    tokio::time::advance(TINY).await;
    assert!(
        effects.remind(now()),
        "the first reminder did not come back"
    );

    let mut off = Effects::new(false, &EffectsConfig::default());
    off.on_key(now());
    tokio::time::advance(REMINDERS[REMINDERS.len() - 1]).await;
    assert!(
        !off.remind(now()),
        "a layer that shows nothing still reminded"
    );
}

/// The schedule belongs to the shimmer alone: a reminder is spent with the shimmer's own
/// switch on, whatever the frames switch says, and left alone with the shimmer's off.
#[tokio::test(start_paused = true)]
async fn a_reminder_is_spent_for_the_shimmer_alone() {
    let now = || tokio::time::Instant::now().into_std();
    let with_shimmer = |shimmer: bool| {
        Effects::new(
            true,
            &EffectsConfig {
                shimmer,
                ..EffectsConfig::default()
            },
        )
    };
    let mut sweeping = with_shimmer(true);
    let mut off = with_shimmer(false);
    for effects in [&mut sweeping, &mut off] {
        effects.on_key(now());
    }

    tokio::time::advance(REMINDERS[0]).await;
    assert!(sweeping.remind(now()), "the shimmer alone reminds of a row");
    assert!(
        !off.remind(now()),
        "a reminder was spent with the shimmer switched off"
    );
}

/// Each of the three switches turns its own animation off and leaves the others running,
/// and [`Effects::configure`] takes a changed set of switches while the TUI runs.
#[test]
fn each_switch_holds_back_its_own_animation() {
    let row = Rect::new(0, 0, SIZE.0, 1);
    let all_off = EffectsConfig {
        eased: false,
        glow: false,
        shimmer: false,
        ..EffectsConfig::default()
    };
    let mut effects = Effects::new(true, &all_off);
    effects.on_value_change(row, Color::Yellow);
    effects.on_cursor_move(row);
    effects.on_shimmer(row);
    assert!(!effects.active(), "a switched-off animation was armed");
    assert!(effects.enabled(), "the layer itself is still on");

    effects.configure(&EffectsConfig::default());
    effects.on_shimmer(row);
    assert!(effects.active(), "the shimmer was switched back on");
}

/// The text of a frame is identical with effects on and off: every animation changes how
/// the cells look and never what they say, which is what the screens' snapshots rest on.
#[test]
fn a_frame_says_the_same_with_effects_on_as_with_them_off() {
    let app = tone_app();
    let (plain, areas) = frame(
        &app,
        &mut Effects::new(false, &EffectsConfig::default()),
        Duration::from_millis(150),
    );

    let mut effects = Effects::new(true, &EffectsConfig::default());
    effects.on_screen_change(areas.body);
    // Half of the sweep's 300 ms: the front is somewhere across the screen.
    let (swept, _) = frame(&app, &mut effects, Duration::from_millis(150));

    assert_eq!(
        symbols(&plain),
        symbols(&swept),
        "the animation layer changed the text of the frame"
    );
    assert_ne!(
        plain, swept,
        "the animation layer changed nothing at all, so this proves nothing"
    );
}

/// The same over the Loki Max's preset list: the animation layer changes no cell's text.
#[test]
fn a_frame_over_the_loki_max_list_says_the_same_with_effects_on_as_with_them_off() {
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get("lokimax")
        .expect("lokimax is built in")
        .clone();
    let mut app = App::new(profile, "loki".into(), &TuiConfig::default(), Vec::new());
    crate::tui::test_support::press(&mut app, crossterm::event::KeyCode::Char('p'));
    assert!(app.presets.device_list.is_some());
    let (plain, areas) = frame(
        &app,
        &mut Effects::new(false, &EffectsConfig::default()),
        Duration::from_millis(150),
    );
    let mut effects = Effects::new(true, &EffectsConfig::default());
    effects.on_screen_change(areas.body);
    let (swept, _) = frame(&app, &mut effects, Duration::from_millis(150));
    assert_eq!(symbols(&plain), symbols(&swept));
}

/// The trace's way round a frame: from the top-left corner along the top, down the right
/// side, back along the bottom and up the left, every border cell once and no cell twice.
#[test]
fn the_perimeter_runs_clockwise_from_the_top_left_and_visits_every_border_cell_once() {
    let cells = perimeter(Rect::new(2, 1, 4, 3));
    assert_eq!(
        cells,
        vec![
            (2, 1),
            (3, 1),
            (4, 1),
            (5, 1),
            (5, 2),
            (5, 3),
            (4, 3),
            (3, 3),
            (2, 3),
            (2, 2)
        ]
    );
    assert_eq!(perimeter(Rect::new(0, 0, 1, 1)), vec![(0, 0)]);
    assert!(perimeter(Rect::new(0, 0, 0, 3)).is_empty());
}

/// A pane that has just appeared draws its own frame round from the top-left, then sweeps
/// its content in behind it — and both are over inside the burst.
#[test]
fn the_arrival_reveals_the_frame_before_the_content() {
    let mut fx = Effects::new(true, &EffectsConfig::default());
    let pane = Rect::new(0, 0, 20, 5);
    assert!(fx.on_arrival(&[pane]), "the pane did not announce itself");

    let early = fx.frame_for_test(pane, Duration::from_millis(100));
    // A third of the way round: the top edge is drawn and the bottom-left corner is not
    // yet, and neither cell says anything other than what the pane drew there.
    assert!(early.cell((5, 0)).is_some_and(|c| c.symbol() != " "));
    assert_eq!(
        fg(&early, 5, 0),
        Color::Reset,
        "the top edge is still hidden"
    );
    assert_eq!(early.cell((0, 4)).map(|c| c.symbol()), Some("└"));
    assert_eq!(
        fg(&early, 0, 4),
        SWEPT_FROM,
        "the corner is drawn before its turn comes"
    );
    assert_eq!(
        early
            .cell((5, 2))
            .map(|cell| (cell.fg, cell.bg))
            .expect("a cell inside the pane"),
        (SWEPT_FROM, Color::Reset),
        "the content waits in the grey, on the terminal's own ground, until the frame is closed"
    );
    assert!(fx.active());

    let done = fx.frame_for_test(pane, Duration::from_millis(700));
    assert!(!fx.active(), "arrival and sweep are over: {done:?}");
    assert_eq!(
        fg(&done, 0, 4),
        Color::Reset,
        "the frame is left hidden once the arrival is over"
    );
}

/// Two panes appearing together each trace their own contour and hold back their own
/// content: what holds the arrivals together hands neither of them the other's area.
#[test]
fn two_panes_appearing_together_each_trace_their_own_frame() {
    let mut fx = Effects::new(true, &EffectsConfig::default());
    let (left, right) = (Rect::new(0, 0, 20, 5), Rect::new(20, 0, 20, 5));
    assert!(fx.on_arrival(&[left, right]), "the panes announced nothing");

    let early = fx.frames_for_test(&[left, right], Duration::from_millis(100));
    for pane in [left, right] {
        assert_eq!(
            fg(&early, pane.x, pane.y),
            Color::Reset,
            "the corner the trace starts from is hidden in {pane:?}"
        );
        assert_eq!(
            fg(&early, pane.x, pane.bottom() - 1),
            SWEPT_FROM,
            "the far corner of {pane:?} is drawn before its turn comes"
        );
        let inside = early
            .cell((pane.x + 2, pane.y + 2))
            .map(|cell| cell.fg)
            .expect("a cell inside the pane");
        assert_eq!(
            inside, SWEPT_FROM,
            "the content of {pane:?} is in sight before its frame is closed"
        );
    }
    // Each pane's own far border column pins its own width rather than a container's: if
    // a layout bug pushed one area onto both panes — `right` as wide as `left` and `right`
    // together, say — `left`'s own top-right corner would be well inside that wider trace's
    // first third and already revealed at this alpha, where its own 20-cell-wide trace has
    // not reached it yet.
    assert_eq!(
        fg(&early, left.right() - 1, left.y),
        SWEPT_FROM,
        "the near pane's own far corner is drawn before its trace reaches it, so its \
             border is not the width a wider, merged area would give it"
    );

    fx.frames_for_test(&[left, right], Duration::from_millis(700));
    assert!(!fx.active(), "both arrivals are over");
}

/// Two bordered panes side by side with a squirrel's cell painted at `at` over the right one,
/// drawn through `fx` and advanced by `last_tick`, the cell named as one to hold back.
fn squirrel_frame(fx: &mut Effects, at: Position, last_tick: Duration) -> Buffer {
    let (left, right) = (Rect::new(0, 0, 20, 5), Rect::new(20, 0, 20, 5));
    let mut terminal = Terminal::new(TestBackend::new(40, 5)).expect("a test terminal opens");
    terminal
        .draw(|frame| {
            for pane in [left, right] {
                frame.render_widget(Block::bordered(), pane);
            }
            let buffer = frame.buffer_mut();
            let under = buffer[at].clone();
            buffer[at]
                .set_symbol("▀")
                .set_fg(RUNNER_GLOW)
                .set_bg(Color::Blue);
            fx.hold_back(&[(at, under)]);
            fx.render(frame, last_tick);
        })
        .expect("the frame draws");
    terminal.backend().buffer().clone()
}

/// The squirrel's cells show what is under them for as long as an arrival traces the pane they
/// stand in, then come in whole; a pane arriving beside them leaves them in sight throughout.
#[test]
fn the_squirrel_waits_out_only_the_arrival_of_its_own_pane() {
    let (left, right) = (Rect::new(0, 0, 20, 5), Rect::new(20, 0, 20, 5));
    let feet = Position::new(30, 4);
    let squirrel = |buffer: &Buffer| {
        let cell = &buffer[feet];
        (cell.symbol() == "▀", cell.fg, cell.bg)
    };
    let whole = (true, RUNNER_GLOW, Color::Blue);

    let mut own = Effects::new(true, &EffectsConfig::default());
    assert!(own.on_arrival(&[right]));
    let early = squirrel_frame(&mut own, feet, Duration::from_millis(100));
    assert_eq!(
        early[feet].symbol(),
        "─",
        "the feet came in with the contour"
    );
    let late = squirrel_frame(&mut own, feet, ARRIVAL_SPAN - Duration::from_millis(120));
    assert_eq!(
        late[feet].symbol(),
        "─",
        "the squirrel came in before its pane"
    );
    let done = squirrel_frame(&mut own, feet, Duration::from_millis(20));
    assert_eq!(
        squirrel(&done),
        whole,
        "the squirrel is not back once its pane is in"
    );

    let mut beside = Effects::new(true, &EffectsConfig::default());
    assert!(beside.on_arrival(&[left]));
    for _ in 0..3 {
        let shown = squirrel_frame(&mut beside, feet, Duration::from_millis(100));
        assert_eq!(
            squirrel(&shown),
            whole,
            "the squirrel left as a pane beside it came in"
        );
    }
}

/// Through a way in armed by `arm` and advanced `tick` at a time, the squirrel at the right
/// pane's feet shows on exactly the frames where nothing runs any more.
fn squirrel_waits_for(arm: impl FnOnce(&mut Effects), tick: Duration) {
    let feet = Position::new(30, 4);
    let mut fx = Effects::new(true, &EffectsConfig::default());
    arm(&mut fx);
    assert!(fx.active(), "nothing was armed");
    for frame in 1..=60 {
        let shown = squirrel_frame(&mut fx, feet, tick)[feet].symbol() == "▀";
        assert_eq!(
            shown,
            !fx.active(),
            "frame {frame} of {tick:?}: the squirrel's showing and the way in disagree"
        );
    }
    assert!(!fx.active(), "the way in never ended");
}

/// The squirrel comes in on the frame its pane's arrival ends, at a tick of a fraction of a
/// millisecond as much as a whole one: the wait counts time as the effects do.
#[test]
fn the_squirrel_comes_in_as_the_arrival_ends_at_any_tick() {
    let right = Rect::new(20, 0, 20, 5);
    for tick in [Duration::from_millis(20), Duration::from_micros(16_900)] {
        squirrel_waits_for(|fx| assert!(fx.on_arrival(&[right])), tick);
    }
}

/// A screen with no frame to trace sweeps its body in, and the squirrel waits that out too.
#[test]
fn the_squirrel_waits_out_a_body_sweep() {
    let body = Rect::new(0, 0, 40, 5);
    squirrel_waits_for(
        |fx| fx.on_screen_change(body),
        Duration::from_micros(16_900),
    );
}

/// A body's sweep and an arrival overlapping each keep the squirrel back for as long as they
/// run over it, whichever was armed first.
#[test]
fn the_squirrel_waits_out_every_way_in_over_it() {
    let (body, left, right) = (
        Rect::new(0, 0, 40, 5),
        Rect::new(0, 0, 20, 5),
        Rect::new(20, 0, 20, 5),
    );
    let feet = Position::new(30, 4);
    let shown = |fx: &mut Effects, ms: u64| {
        squirrel_frame(fx, feet, Duration::from_millis(ms))[feet].symbol() == "▀"
    };

    let mut sweep_first = Effects::new(true, &EffectsConfig::default());
    sweep_first.on_screen_change(body);
    assert!(!shown(&mut sweep_first, 100));
    assert!(sweep_first.on_arrival(&[left]));
    assert!(
        !shown(&mut sweep_first, 100),
        "a pane arriving beside it let the squirrel out of the sweep"
    );
    assert!(shown(&mut sweep_first, 100), "the sweep is over");

    let mut arrival_first = Effects::new(true, &EffectsConfig::default());
    assert!(arrival_first.on_arrival(&[right]));
    assert!(!shown(&mut arrival_first, 100));
    arrival_first.on_screen_change(body);
    assert!(
        !shown(&mut arrival_first, 300),
        "the sweep's end let the squirrel out of its pane's arrival"
    );
    assert!(shown(&mut arrival_first, 200), "the arrival is over");
}

/// The frames switch is the runner's alone: with it off no light runs round a frame, and a
/// pane that appears is still traced round. Nothing at all is armed for a screen that drew
/// no frame to trace.
#[test]
fn the_frames_switch_stops_the_runner_and_leaves_the_trace() {
    let pane = Rect::new(0, 0, 20, 5);
    let off = EffectsConfig {
        frames: false,
        ..EffectsConfig::default()
    };
    let mut runner = Effects::new(true, &off);
    runner.on_lap(pane, RUNNER_GLOW);
    assert!(!runner.active(), "the switch is off and a light ran");

    let mut trace = Effects::new(true, &off);
    assert!(
        trace.on_arrival(&[pane]),
        "the runner's switch held the trace back"
    );
    let early = trace.frame_for_test(pane, Duration::from_millis(100));
    assert_eq!(
        fg(&early, 0, 4),
        SWEPT_FROM,
        "the pane's far corner is drawn before the trace reaches it"
    );

    let mut on = Effects::new(true, &EffectsConfig::default());
    assert!(!on.on_arrival(&[]), "a screen with no frame announced one");
    assert!(
        !on.on_arrival(&[Rect::ZERO]),
        "a frame with no cell arrived"
    );
    assert!(!on.active());
}

/// The trace is one transition of four: with any of the others the word names, a pane that
/// appears is not drawn round and a screen with no frame is not swept in — and the trace,
/// named again, does both once more.
#[test]
fn only_the_trace_transition_draws_a_pane_round_or_sweeps_a_body_in() {
    let pane = Rect::new(0, 0, 20, 5);
    for word in [Transition::None, Transition::Noise, Transition::Melt] {
        let mut fx = Effects::new(true, &EffectsConfig::default());
        fx.set_transition(word);
        assert!(!fx.on_arrival(&[pane]), "{word:?} traced a pane");
        fx.on_screen_change(pane);
        assert!(!fx.active(), "{word:?} swept a body in");

        fx.set_transition(Transition::Trace);
        assert!(
            fx.on_arrival(&[pane]),
            "the trace, named again, traced nothing"
        );
        let mut swept = Effects::new(true, &EffectsConfig::default());
        swept.set_transition(word);
        swept.set_transition(Transition::Trace);
        swept.on_screen_change(pane);
        assert!(swept.active(), "the trace, named again, swept nothing in");
    }
}

/// A lap takes its pace from the music, as the lap armed and run out says rather than as
/// a sum repeated back: one is over inside half a second with the meters at the top of
/// their scale, still going there on a device whose meters say nothing, and over by the
/// time 1.3 s have gone by. Halfway up the scale is halfway between the two paces.
#[test]
fn a_lap_is_slow_at_silence_and_fast_at_full_level() {
    let frame = Rect::new(0, 0, 10, 4);
    let mut loud = Effects::new(true, &EffectsConfig::default());
    loud.set_level(Some(1.0));
    loud.on_lap(frame, RUNNER_GLOW);
    loud.frame_for_test(frame, Duration::from_millis(500));
    assert!(
        !loud.active(),
        "a lap at full level outlasted half a second"
    );

    let mut quiet = Effects::new(true, &EffectsConfig::default());
    quiet.set_level(None);
    quiet.on_lap(frame, RUNNER_GLOW);
    quiet.frame_for_test(frame, Duration::from_millis(500));
    assert!(quiet.active(), "a lap at silence was over in half a second");
    quiet.frame_for_test(frame, Duration::from_millis(800));
    assert!(!quiet.active(), "a lap at silence outlasted 1.3 s");

    assert_eq!(lap_ms(Some(0.5)), 800);
}

/// The band is a run of cells rather than a point: a fast lap carries the head several
/// cells between one frame and the next, and what is drawn reaches back to where the head
/// was, so every cell of a long contour is lit on some frame of the lap.
#[test]
fn a_fast_lap_skips_no_cell_of_a_long_contour() {
    let mut fx = Effects::new(true, &EffectsConfig::default());
    let frame = Rect::new(0, 0, 40, 14);
    let cells = perimeter(frame);
    fx.set_level(Some(1.0));
    fx.on_lap(frame, RUNNER_GLOW);

    let mut lit = vec![false; cells.len()];
    for _ in 0..8 {
        let buffer = fx.frame_for_test(frame, Duration::from_millis(100));
        for (at, (x, y)) in cells.iter().enumerate() {
            lit[at] |= fg(&buffer, *x, *y) == RUNNER_GLOW;
        }
        if !fx.active() {
            break;
        }
    }
    assert!(!fx.active(), "the lap did not finish inside 800 ms");

    let missed: Vec<usize> = lit
        .iter()
        .enumerate()
        .filter(|(_, lit)| !**lit)
        .map(|(at, _)| at)
        .collect();
    assert!(
        missed.is_empty(),
        "the light skipped {} of the contour's {} cells: {missed:?}",
        missed.len(),
        cells.len()
    );
}

/// The runner is a short band on the frame's own contour: at the slow pace, where the head
/// moves less than a band's length per frame, three cells side by side in the colour it
/// was handed, the head of them bold, moving on frame by frame — once round, and then the
/// frame is left exactly as it was drawn.
#[test]
fn the_runner_lights_three_consecutive_border_cells_and_moves_on() {
    let mut fx = Effects::new(true, &EffectsConfig::default());
    let frame = Rect::new(0, 0, 10, 4);
    fx.on_lap(frame, RUNNER_GLOW);
    let band = |buf: &Buffer| -> Vec<usize> {
        perimeter(frame)
            .into_iter()
            .enumerate()
            .filter(|(_, (x, y))| {
                buf.cell((*x, *y))
                    .is_some_and(|cell| cell.fg == RUNNER_GLOW)
            })
            .map(|(at, _)| at)
            .collect()
    };

    let a = fx.frame_for_test(frame, Duration::from_millis(100));
    let lit = band(&a);
    assert_eq!(
        lit.len(),
        RUNNER_LEN,
        "the band is not three cells: {lit:?}"
    );
    assert!(
        lit.windows(2).all(|pair| pair[1] == pair[0] + 1),
        "the band is not one run along the contour: {lit:?}"
    );
    let cells = perimeter(frame);
    for at in &lit {
        let (x, y) = cells[*at];
        assert_ne!(
            a.cell((x, y)).map(|cell| cell.symbol()),
            Some(" "),
            "the band wrote over the contour at {x},{y}"
        );
    }
    let (x, y) = cells[*lit.last().expect("the band's head")];
    assert!(
        a.cell((x, y))
            .is_some_and(|cell| cell.modifier.contains(Modifier::BOLD)),
        "the head of the band is not the brightest cell of it"
    );

    let b = fx.frame_for_test(frame, Duration::from_millis(100));
    assert_ne!(a, b, "the band has moved");
    fx.frame_for_test(frame, Duration::from_millis(1_100));
    assert!(!fx.active(), "one lap, then rest");
    let after = fx.frame_for_test(frame, Duration::from_millis(100));
    assert!(
        perimeter(frame)
            .into_iter()
            .all(|(x, y)| fg(&after, x, y) == Color::Reset),
        "the frame after the lap is still carrying it"
    );
}

/// The frames a screen draws are compared with the ones the frame before it drew: a pane
/// that was already there has not appeared, one the screen has just drawn has, and a
/// contour a resize has moved is the one it was.
#[test]
fn only_the_frames_the_last_frame_did_not_draw_have_appeared() {
    let mut effects = Effects::new(true, &EffectsConfig::default());
    let controls = |area| (FrameId::Pane(Pane::Controls), area);
    let tone = |area| (FrameId::Pane(Pane::Tone), area);
    let (left, right) = (Rect::new(0, 0, 10, 5), Rect::new(10, 0, 10, 5));

    assert_eq!(
        effects.appeared(&[controls(left)]),
        vec![left],
        "the first frame of a screen is an appearance"
    );
    assert!(
        effects.appeared(&[controls(left)]).is_empty(),
        "a redraw is not an appearance"
    );
    assert_eq!(
        effects.appeared(&[controls(left), tone(right)]),
        vec![right],
        "the pane that was already there appeared again"
    );
    assert!(
        effects
            .appeared(&[controls(Rect::new(0, 0, 20, 8)), tone(right)])
            .is_empty(),
        "a pane the terminal's new size moved announced itself"
    );
}

/// A theme at true colour that paints the palette's backgrounds.
fn painted_theme() -> Theme {
    Theme::from_config(
        &TuiConfig {
            theme_background: true,
            ..TuiConfig::default()
        },
        false,
        ColorDepth::TrueColor,
    )
}

/// What every cell of `buffer` has for a background.
fn backgrounds(buffer: &Buffer) -> Vec<Color> {
    let area = buffer.area;
    (area.y..area.bottom())
        .flat_map(|y| (area.x..area.right()).map(move |x| (x, y)))
        .map(|at| buffer.cell(at).expect("a cell inside the buffer").bg)
        .collect()
}

/// How a case arms one effect, given the screen's body and its frames.
type Arm = fn(&mut Effects, Rect, &[Rect]);

/// The sweep a screen change earns, the arrival trace, the glow and the shimmer, the runner
/// and a value's flash leave every cell's background as the theme painted it.
#[test]
fn the_effects_leave_the_backgrounds_as_the_theme_painted_them() {
    let theme = painted_theme();
    let ground = theme.surface(crate::tui::theme::Surface::Ground);
    assert_backgrounds_kept(&theme, ground.expect("a ground at true colour"));
}

/// With the theme background off, the same effects leave the terminal's own background
/// showing: the sweep and the trace bring text in from the dim grey and never paint the grey
/// behind it.
#[test]
fn the_effects_leave_the_terminals_own_background_alone() {
    let theme = Theme::from_config(
        &TuiConfig {
            theme_background: false,
            ..TuiConfig::default()
        },
        false,
        ColorDepth::TrueColor,
    );
    assert_backgrounds_kept(&theme, Color::Reset);
}

/// Arm each effect in turn over the tone screen drawn on `theme`, and check no frame of it
/// changes a cell's background; `ground` is a background the plain frame must carry, so the
/// case has backgrounds to keep.
fn assert_backgrounds_kept(theme: &Theme, ground: Color) {
    let app = tone_app();
    let (plain, areas) = frame_with(
        &app,
        &mut Effects::new(false, &EffectsConfig::default()),
        Duration::from_millis(150),
        theme,
    );
    assert!(
        backgrounds(&plain).contains(&ground),
        "the frame has no {ground:?} background, so this proves nothing"
    );
    let frames: Vec<Rect> = areas.frames.iter().map(|(_, rect)| *rect).collect();
    let arm: [(&str, Arm); 7] = [
        ("the shimmer", |e, _, _| {
            e.on_shimmer(Rect::new(1, 4, 30, 1));
        }),
        ("the fault flash", |e, _, _| {
            e.on_fault(Rect::new(0, 0, 80, 2))
        }),
        ("the screen sweep", |e, body, _| e.on_screen_change(body)),
        ("the arrival trace", |e, _, frames| {
            e.on_arrival(frames);
        }),
        ("the cursor's glow", |e, _, _| {
            e.on_cursor_move(Rect::new(1, 4, 30, 1));
        }),
        ("the value flash", |e, _, _| {
            e.on_value_change(Rect::new(1, 4, 30, 1), Color::Rgb(0xe0, 0xa0, 0x30));
        }),
        ("the runner", |e, _, frames| {
            e.on_lap(frames[0], Color::Rgb(0xff, 0x55, 0x22));
        }),
    ];
    for (what, arming) in arm {
        let mut effects = Effects::new(true, &EffectsConfig::default());
        effects.set_surfaces(theme);
        arming(&mut effects, areas.body, &frames);
        for tick in [30, 120, 150] {
            let (drawn, _) = frame_with(&app, &mut effects, Duration::from_millis(tick), theme);
            let moved = backgrounds(&plain)
                .into_iter()
                .zip(backgrounds(&drawn))
                .position(|(was, now)| was != now);
            assert_eq!(
                moved, None,
                "{what} changed the background of cell {moved:?} at tick {tick}"
            );
        }
    }
}

/// With the theme background on, the sweep and the trace start text from the surface the theme
/// painted (the ground for a screen's sweep, the pane for a pane's trace), not from black; with
/// no surface painted they start from the dim grey.
#[test]
fn the_sweep_and_the_trace_come_from_the_painted_surface() {
    use crate::tui::test_support::theme_for;
    use crate::tui::theme::Surface;
    use crate::tui::theme::Theme;
    use svolder_ipc::{ColorDepth, TuiConfig};

    let theme = theme_for("amber", ColorDepth::TrueColor);
    let ground = theme.surface(Surface::Ground).expect("a ground is painted");
    let pane_colour = theme.surface(Surface::Pane).expect("a pane is areas");
    assert_ne!(ground, Color::Black);
    assert_ne!(pane_colour, Color::Black);

    let mut sweep = Effects::new(true, &EffectsConfig::default());
    sweep.set_surfaces(&theme);
    let area = Rect::new(0, 0, 40, 6);
    sweep.on_screen_change(area);
    let first = sweep.frame_for_test(area, Duration::from_millis(10));
    assert_eq!(fg(&first, 35, 3), ground, "text ahead of the sweep");

    let mut trace = Effects::new(true, &EffectsConfig::default());
    trace.set_surfaces(&theme);
    let pane = Rect::new(0, 0, 20, 5);
    assert!(trace.on_arrival(&[pane]));
    let early = trace.frame_for_test(pane, Duration::from_millis(100));
    assert_eq!(fg(&early, 0, 4), pane_colour, "an untraced border glyph");
    assert_eq!(fg(&early, 5, 2), pane_colour, "content waiting its turn");

    // With no ground painted — 16 colours, monochrome, the theme background off — the terminal's
    // own background shows, which can be light: text comes in from the dim grey, which reads on
    // either, never from black.
    let no_ground = [
        theme_for("amber", ColorDepth::Ansi16),
        Theme::new(true),
        Theme::from_config(
            &TuiConfig {
                theme_background: false,
                ..TuiConfig::default()
            },
            false,
            ColorDepth::TrueColor,
        ),
    ];
    for theme in no_ground {
        let mut plain = Effects::new(true, &EffectsConfig::default());
        plain.set_surfaces(&theme);
        plain.on_screen_change(area);
        let first = plain.frame_for_test(area, Duration::from_millis(10));
        assert_eq!(fg(&first, 35, 3), SWEPT_FROM, "no surface areas");
        assert_ne!(fg(&first, 35, 3), Color::Black, "no surface areas");

        let mut trace = Effects::new(true, &EffectsConfig::default());
        trace.set_surfaces(&theme);
        assert!(trace.on_arrival(&[pane]));
        let early = trace.frame_for_test(pane, Duration::from_millis(100));
        assert_eq!(fg(&early, 5, 2), SWEPT_FROM, "content waiting its turn");
        assert_ne!(fg(&early, 5, 2), Color::Black, "content waiting its turn");
    }
}

fn reform_over(area: Rect) -> crate::tui::decoration::intro::Reform {
    let theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    crate::tui::decoration::intro::Reform::for_test(area, 0x5EED, &theme)
}

/// Under trace, a pane arriving while the exit holds the panes waits: nothing is armed
/// until the hold is over, then its trace starts.
#[test]
fn the_exit_holds_the_arrival_trace_for_a_third_of_a_second() {
    let mut fx = Effects::new(true, &EffectsConfig::default());
    fx.hand_over(reform_over(Rect::new(0, 0, 120, 40)));
    assert!(fx.reforming());
    let pane = Rect::new(0, 4, 40, 10);
    assert!(fx.on_arrival(&[pane]));
    assert!(!fx.active(), "the trace waits for the exit");
    let _ = fx.frame_for_test(pane, Duration::from_millis(400));
    assert!(fx.active(), "the trace runs once the hold is over");
}

/// A key ends the exit at once, and a pane it held starts its trace straight away rather than
/// staying hidden.
#[test]
fn a_key_ends_the_exit_and_lets_the_held_panes_in() {
    let mut fx = Effects::new(true, &EffectsConfig::default());
    fx.hand_over(reform_over(Rect::new(0, 0, 120, 40)));
    let pane = Rect::new(0, 4, 40, 10);
    fx.on_arrival(&[pane]);
    fx.on_key(Instant::now());
    assert!(!fx.reforming());
    assert!(fx.active(), "the held trace is armed");
}

/// Ending the exit directly, as the loop does on a click, lets the held panes in.
#[test]
fn ending_the_exit_lets_the_held_panes_in() {
    let mut fx = Effects::new(true, &EffectsConfig::default());
    fx.hand_over(reform_over(Rect::new(0, 0, 120, 40)));
    fx.on_arrival(&[Rect::new(0, 4, 40, 10)]);
    fx.end_reform();
    assert!(!fx.reforming());
    assert!(fx.active(), "the held trace is armed");
}

/// The squirrel stays back while the exit holds the pane it stands in, then through that
/// pane's arrival, and comes in once both are over.
#[test]
fn the_squirrel_waits_out_the_exits_hold_on_its_pane() {
    let right = Rect::new(20, 0, 20, 5);
    let feet = Position::new(30, 4);
    let mut fx = Effects::new(true, &EffectsConfig::default());
    fx.hand_over(reform_over(Rect::new(0, 0, 120, 40)));
    assert!(fx.on_arrival(&[right]));
    let held = squirrel_frame(&mut fx, feet, Duration::from_millis(100));
    assert_eq!(
        held[feet].symbol(),
        "─",
        "the squirrel showed in a held pane"
    );
    let arriving = squirrel_frame(&mut fx, feet, Duration::from_millis(300));
    assert!(fx.active(), "the held pane's arrival runs");
    assert_eq!(
        arriving[feet].symbol(),
        "─",
        "the squirrel came in before its pane"
    );
    let done = squirrel_frame(&mut fx, feet, ARRIVAL_SPAN);
    assert_eq!(
        done[feet].symbol(),
        "▀",
        "the squirrel is not back once its pane is in"
    );
}

/// Under any transition but trace the exit holds nothing, and with the layer off it is never
/// taken.
#[test]
fn the_exit_holds_nothing_under_another_transition_and_is_refused_with_the_layer_off() {
    let mut fx = Effects::new(true, &EffectsConfig::default());
    fx.set_transition(Transition::None);
    fx.hand_over(reform_over(Rect::new(0, 0, 120, 40)));
    assert!(!fx.on_arrival(&[Rect::new(0, 4, 40, 10)]));
    let mut off = Effects::new(false, &EffectsConfig::default());
    off.hand_over(reform_over(Rect::new(0, 0, 120, 40)));
    assert!(!off.reforming());
}

#[test]
fn the_exit_runs_at_the_pacers_period() {
    let mut fx = Effects::new(true, &EffectsConfig::default());
    assert_eq!(fx.reform_period(), None);
    fx.hand_over(reform_over(Rect::new(0, 0, 120, 40)));
    assert_eq!(fx.reform_period(), Some(Duration::from_millis(16)));
    for _ in 0..3 {
        fx.pace(Duration::from_millis(20));
    }
    assert_eq!(fx.reform_period(), Some(Duration::from_millis(33)));
}

/// While the exit plays it owns the light: the laps a focus change or an arrival earn arm
/// nothing, so none replaces the exit's own.
#[test]
fn the_exit_owns_the_light_while_it_plays() {
    let mut fx = Effects::new(true, &EffectsConfig::default());
    fx.hand_over(reform_over(Rect::new(0, 0, 120, 40)));
    let pane = Rect::new(0, 4, 40, 10);
    fx.on_arrival_lap(pane, Color::White);
    fx.on_lap(pane, Color::White);
    assert!(!fx.active());
    fx.end_reform();
    fx.on_lap(pane, Color::White);
    assert!(fx.active(), "the usual laps run again once it has ended");
}

/// The light waits for the exit and, under trace, for the panes' hold, arrival and sweep.
#[test]
fn the_exits_light_waits_for_the_exit_and_the_panes() {
    let theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    let app = App::new(
        crate::tui::test_support::profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    let mut hits = HitMap::default();
    let mut areas = FrameAreas::default();
    terminal
        .draw(|frame| areas = render::draw(frame, &app, &theme, &mut hits))
        .unwrap();
    let screen = terminal.backend().buffer().clone();
    for (transition, traced) in [(Transition::Trace, true), (Transition::None, false)] {
        let mut fx = Effects::new(true, &EffectsConfig::default());
        fx.set_transition(transition);
        fx.hand_over(reform_over(Rect::new(0, 0, 120, 40)));
        fx.paint_reform(&mut screen.clone(), areas.logo, &[]);
        assert!(fx.wants_reform_lap(), "the exit planned itself");
        let remaining = fx.reform.as_ref().unwrap().remaining_ms();
        let wait = fx.reform_lap_wait();
        assert!(wait >= remaining);
        match traced {
            true => assert!(wait >= 950, "{wait}"),
            false => assert_eq!(wait, remaining),
        }
    }
}

/// What `arm` leaves on the second of two frames of the case's size whose every cell is drawn
/// in `base`, 25 ms apart.
fn glowed(base: Color, arm: impl FnOnce(&mut Effects)) -> Buffer {
    let mut effects = Effects::new(true, &EffectsConfig::default());
    arm(&mut effects);
    let area = Rect::new(0, 0, SIZE.0, SIZE.1);
    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    let mut drawn = Buffer::empty(area);
    for step in [Duration::ZERO, Duration::from_millis(25)] {
        terminal
            .draw(|frame| {
                paint(frame.buffer_mut(), area, DOT);
                frame
                    .buffer_mut()
                    .set_style(area, Style::default().fg(base));
                effects.render(frame, step);
                drawn = frame.buffer_mut().clone();
            })
            .expect("the frame draws");
    }
    drawn
}

/// Rows whose values change on the same frame each glow, together: the second row's glow does
/// not take the first one's place, and a row between them that did not change stays as drawn.
#[test]
fn values_that_change_together_each_glow_their_own_row() {
    let base = Color::Rgb(0x20, 0x20, 0x20);
    let rows = [Rect::new(0, 1, SIZE.0, 1), Rect::new(0, 3, SIZE.0, 1)];
    let drawn = glowed(base, |effects| {
        effects.on_value_changes(&rows, Color::Yellow)
    });
    let fg = |y: u16| drawn.cell((0, y)).expect("a cell inside the buffer").fg;
    assert_ne!(
        fg(1),
        base,
        "the first row's glow was cut short by the second's"
    );
    assert_ne!(fg(3), base, "the second row did not glow");
    assert_eq!(fg(2), base, "a row that did not change glowed");
}

/// A row listed more than once — a tone band's row, reported under each of its three
/// functions — glows exactly as a row listed once does.
#[test]
fn a_row_listed_twice_glows_as_one_listed_once() {
    let base = Color::Rgb(0x20, 0x20, 0x20);
    let row = Rect::new(0, 1, SIZE.0, 1);
    assert_eq!(
        glowed(base, |effects| effects
            .on_value_changes(&[row, row], Color::Yellow)),
        glowed(base, |effects| effects.on_value_change(row, Color::Yellow)),
    );
}

/// A call reporting no frames clears the record, so the same frames reported after it all
/// appear again: a terminal too small for any screen, then grown back, announces them.
#[test]
fn frames_reported_again_after_an_empty_report_appear_again() {
    let mut effects = Effects::new(true, &EffectsConfig::default());
    let frames = [(FrameId::Screen, Rect::new(0, 0, 40, 6))];
    assert_eq!(effects.appeared(&frames).len(), 1, "first sight");
    assert!(effects.appeared(&[]).is_empty());
    assert_eq!(effects.appeared(&frames).len(), 1, "the record was kept");
}
