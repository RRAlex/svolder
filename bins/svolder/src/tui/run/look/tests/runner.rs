// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The screen change's frame trace and the runner, the light that laps a frame, with the switches
//! that hold either back.

use super::*;

/// The three bytes of a colour a true-colour frame drew, for a case that measures how far
/// apart two of them are.
fn rgb(colour: Color) -> (u8, u8, u8) {
    let Color::Rgb(..) = colour else {
        panic!("{colour:?} is not a colour a true-colour frame drew");
    };
    palette::rgb_of(colour)
}

/// A screen change traces every frame the new screen drew — each from its own top-left
/// corner, with the plain body sweep left out of it — and a redraw of the same screen
/// announces nothing. A screen with no frame to trace, the keys over the screen among
/// them, sweeps its whole body in.
#[test]
fn a_screen_change_traces_the_new_screens_frames_and_a_redraw_traces_nothing() {
    let mut app = gungnir();
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let areas = row_areas(&app);
    assert_eq!(areas.frames.len(), 3, "the device screen's three panes");

    let mut effects = capable_over_no_colour(&EffectsConfig::default());
    app.frame_flags.screen_changed = true;
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(effects.active(), "the new screen's frames did not arrive");
    let early = animated(&app, &mut effects, Duration::from_millis(20));
    for (id, pane) in &areas.frames {
        let (left, top, bottom) = (pane.x, pane.y, pane.bottom() - 1);
        assert_ne!(
            fg(&early, left, top),
            SWEPT_FROM,
            "{id:?} is not tracing from its own top-left corner"
        );
        assert_eq!(
            fg(&early, left, bottom),
            SWEPT_FROM,
            "{id:?} drew the far end of its contour before its turn came"
        );
    }
    // The body sweep comes in from the left, so a pane on the right is what it would still be
    // holding back: there the corner the trace has drawn shows as the widgets drew it.
    let plain = animated(
        &app,
        &mut Effects::new(false, &EffectsConfig::default()),
        Duration::from_millis(20),
    );
    let far = areas
        .frames
        .iter()
        .map(|(_, pane)| *pane)
        .max_by_key(|pane| pane.x)
        .expect("a pane");
    assert!(
        far.x >= SIZE.0 / 3,
        "no pane past the sweep's reach: {far:?}"
    );
    assert_eq!(
        fg(&early, far.x, far.y),
        fg(&plain, far.x, far.y),
        "the body sweep played over the panes as well as the trace"
    );
    run_out(&mut effects);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(!effects.active(), "a redraw announced the panes again");

    // A screen change to a screen with no frame sweeps the body in: one that drew no body
    // either has nothing to sweep, and one that drew a body carries it in from the left.
    let mut effects = capable_over_no_colour(&EffectsConfig::default());
    let frameless = FrameAreas {
        frames: Vec::new(),
        focused: None,
        ..areas.clone()
    };
    let bodyless = FrameAreas {
        body: Rect::ZERO,
        ..frameless.clone()
    };
    app.frame_flags.screen_changed = true;
    arm_effects(&mut app, &mut effects, &bodyless, &theme, t0);
    assert!(
        !effects.active(),
        "a screen with neither a frame nor a body armed something"
    );
    app.frame_flags.screen_changed = true;
    arm_effects(&mut app, &mut effects, &frameless, &theme, t0);
    assert!(effects.active(), "the plain sweep did not play");
    let swept = animated(&app, &mut effects, Duration::from_millis(20));
    assert_eq!(
        fg(&swept, areas.body.right() - 1, areas.body.y),
        SWEPT_FROM,
        "the far side of the body is not waiting for the sweep to reach it"
    );

    // The keys are read over whatever screen is showing, and the overlay reports no frame
    // of its own: what it earns is the sweep over the box.
    app.overlays.help_open = true;
    let overlay = row_areas(&app);
    assert!(overlay.frames.is_empty(), "the overlay reported a frame");
    let mut effects = capable_over_no_colour(&EffectsConfig::default());
    app.frame_flags.screen_changed = true;
    arm_effects(&mut app, &mut effects, &overlay, &theme, t0);
    assert!(effects.active(), "the overlay did not sweep in");
}

/// `F` is the runner's switch and nothing else's: with it off a focus change runs no light
/// round the pane the keys moved to, and a screen change still traces the new screen's
/// frames round.
#[test]
fn the_frames_switch_off_stops_the_runner_and_leaves_the_trace() {
    let mut app = gungnir();
    let theme = Theme::new(false);
    let t0 = Instant::now();
    // The trail and the shimmer a cursor move earns are held back too, so that what a
    // focus change could still run is the light alone.
    let mut effects = capable_over_no_colour(&EffectsConfig {
        frames: false,
        glow: false,
        shimmer: false,
        ..EffectsConfig::default()
    });
    let first = row_areas(&app);
    arm_effects(&mut app, &mut effects, &first, &theme, t0);
    assert!(effects.active(), "the first frame's panes were not traced");
    run_out(&mut effects);

    app.key(KeyEvent::from(KeyCode::Tab));
    assert!(app.frame_flags.pane_changed, "Tab did not move the keys");
    let areas = row_areas(&app);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(
        !effects.active(),
        "a light ran with the runner switched off"
    );

    app.key(KeyEvent::from(KeyCode::Char('t')));
    let telemetry = row_areas(&app);
    arm_effects(&mut app, &mut effects, &telemetry, &theme, t0);
    assert!(effects.active(), "the screen change traced nothing");
    let early = animated(&app, &mut effects, Duration::from_millis(20));
    let (_, frame) = telemetry.frames[0];
    assert_eq!(
        fg(&early, frame.x, frame.bottom() - 1),
        SWEPT_FROM,
        "the telemetry frame is not being traced round"
    );
}

/// The keys are read over a screen rather than in place of it: the frames underneath the
/// overlay never went anywhere, so the overlay lifting announces none of them.
#[test]
fn an_overlay_lifting_announces_nothing_underneath() {
    let mut app = gungnir();
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let mut effects = Effects::new(true, &EffectsConfig::default());

    let screen = row_areas(&app);
    arm_effects(&mut app, &mut effects, &screen, &theme, t0);
    assert!(
        effects.active(),
        "the screen's first frame announced nothing"
    );
    run_out(&mut effects);

    app.key(KeyEvent::from(KeyCode::Char('?')));
    assert!(app.overlays.help_open, "the overlay did not open");
    let overlay = row_areas(&app);
    assert!(overlay.frames.is_empty(), "the overlay reported a frame");
    arm_effects(&mut app, &mut effects, &overlay, &theme, t0);
    assert!(effects.active(), "the overlay did not sweep in");
    run_out(&mut effects);

    // A second frame with the overlay still up, so that what the panes are compared
    // against is not the empty list the overlay reports.
    arm_effects(&mut app, &mut effects, &overlay, &theme, t0);
    assert!(!effects.active(), "the overlay announced itself twice");

    app.key(KeyEvent::from(KeyCode::Esc));
    assert!(!app.overlays.help_open, "the overlay did not close");
    let back = row_areas(&app);
    assert_eq!(back.frames.len(), 3, "the panes are in sight again");
    arm_effects(&mut app, &mut effects, &back, &theme, t0);
    assert!(
        !effects.active(),
        "the panes under the overlay announced themselves again"
    );
}

/// A pane the screen has just drawn for the first time announces itself without waiting
/// for a screen change, and one that was already there is left alone.
#[test]
fn a_pane_that_appears_announces_itself_and_one_that_stays_does_not() {
    let mut app = gungnir();
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let framed = |frames: Vec<(FrameId, Rect)>| FrameAreas {
        frames,
        ..FrameAreas::default()
    };
    let left = (FrameId::Pane(Pane::Controls), Rect::new(0, 2, 40, 10));
    let right = (FrameId::Pane(Pane::Tone), Rect::new(40, 2, 40, 10));

    let mut effects = Effects::new(true, &EffectsConfig::default());
    arm_effects(&mut app, &mut effects, &framed(vec![left]), &theme, t0);
    assert!(
        effects.active(),
        "the first frame of a screen did not arrive"
    );
    run_out(&mut effects);

    arm_effects(&mut app, &mut effects, &framed(vec![left]), &theme, t0);
    assert!(!effects.active(), "a redraw is not an appearance");

    arm_effects(
        &mut app,
        &mut effects,
        &framed(vec![left, right]),
        &theme,
        t0,
    );
    assert!(effects.active(), "the pane that appeared said nothing");
}

/// How many cells of `frame`'s own contour the light is on in `buffer`: the screens are
/// drawn in no colour at all for these cases, so a cell carrying `lifted` — the colour the
/// loop hands a lap — is one the runner lit and nothing else.
fn on_the_contour(buffer: &Buffer, frame: Rect, lifted: Color) -> usize {
    effects::perimeter(frame)
        .into_iter()
        .filter(|(x, y)| fg(buffer, *x, *y) == lifted)
        .count()
}

/// The keys moving to another pane run a light once round that pane's own contour and
/// round no other: what `Tab` did is answered where the keys have landed.
#[test]
fn tab_runs_the_light_round_the_pane_the_keys_moved_to() {
    let mut app = gungnir();
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let mut effects = capable_over_no_colour(&EffectsConfig::default());
    // The screen's own frames arrive first; the light answers what happens after that.
    let first = row_areas(&app);
    arm_effects(&mut app, &mut effects, &first, &theme, t0);
    run_out(&mut effects);

    app.key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.pane, Pane::Tone, "Tab moved the keys to the Tone pane");
    let areas = row_areas(&app);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(
        !app.frame_flags.pane_changed,
        "the focus change was not taken"
    );

    let lit = animated(&app, &mut effects, Duration::from_millis(100));
    for (id, frame) in &areas.frames {
        let cells = on_the_contour(&lit, *frame, theme.glow_lift_at(0.0));
        match *id == FrameId::Pane(Pane::Tone) {
            true => assert!(cells > 0, "the pane the keys moved to is unlit"),
            false => assert_eq!(cells, 0, "a light ran round {id:?} as well"),
        }
    }
    run_out(&mut effects);

    // Tab off the last pane wraps back to the first, which earns its own lap in turn.
    app.key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(app.pane, Pane::Controls, "Tab off the last pane wraps");
    let areas = row_areas(&app);
    assert_eq!(areas.focused, Some(FrameId::Pane(Pane::Controls)));
}

/// Arriving on a screen runs the light once round the frame the keys are in, after the trace
/// has drawn the frames: nothing is lit while the trace runs, the light follows it, and with
/// no transition it starts at once.
#[test]
fn arriving_on_a_screen_runs_the_light_once_the_trace_is_done() {
    let theme = Theme::new(false);
    let lifted = theme.glow_lift_at(0.0);
    let t0 = Instant::now();
    let arrive = |transition: Transition| {
        let mut app = gungnir();
        let mut effects = capable_over_no_colour(&EffectsConfig::default());
        effects.set_transition(transition);
        let areas = row_areas(&app);
        app.frame_flags.screen_changed = true;
        arm_effects(&mut app, &mut effects, &areas, &theme, t0);
        (app, effects, areas)
    };
    let focused = |areas: &FrameAreas| {
        areas
            .frames
            .iter()
            .find(|(id, _)| Some(*id) == areas.focused)
            .map(|(_, frame)| *frame)
            .expect("the keys are in a frame")
    };

    let (app, mut effects, areas) = arrive(Transition::Trace);
    let frame = focused(&areas);
    let during = animated(&app, &mut effects, Duration::from_millis(100));
    assert_eq!(
        on_the_contour(&during, frame, lifted),
        0,
        "the light ran while the trace was still drawing the frames"
    );
    animated(&app, &mut effects, Duration::from_millis(200));
    let after = animated(&app, &mut effects, Duration::from_millis(150));
    assert!(
        on_the_contour(&after, frame, lifted) > 0,
        "the light did not follow the trace"
    );
    for (id, other) in &areas.frames {
        if *other != frame {
            assert_eq!(
                on_the_contour(&after, *other, lifted),
                0,
                "a light ran round {id:?} as well"
            );
        }
    }
    run_out(&mut effects);
    assert!(!effects.active(), "the light ran more than once");

    let (app, mut effects, areas) = arrive(Transition::None);
    let early = animated(&app, &mut effects, Duration::from_millis(100));
    assert!(
        on_the_contour(&early, focused(&areas), lifted) > 0,
        "with no transition the light did not start at once"
    );

    let (mut app, mut effects, areas) = arrive(Transition::Trace);
    effects.configure(&EffectsConfig {
        frames: false,
        ..EffectsConfig::default()
    });
    app.frame_flags.screen_changed = true;
    run_out(&mut effects);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    animated(&app, &mut effects, Duration::from_millis(300));
    let none = animated(&app, &mut effects, Duration::from_millis(150));
    assert_eq!(
        on_the_contour(&none, focused(&areas), lifted),
        0,
        "the frames switch is off and the light ran"
    );
}

/// `Tab` onto the tone screen's header is flagged for the light exactly as any other pane
/// change is, but the header is one line with no border, so nothing is there for a lap to
/// run round: the static selected style over the half is what says the keys are on it.
#[test]
fn tab_onto_the_tone_screens_header_runs_no_light_anywhere() {
    let mut app = gungnir();
    app.show(crate::tui::app::Screen::Tone);
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let mut effects = capable_over_no_colour(&EffectsConfig::default());
    let first = row_areas(&app);
    arm_effects(&mut app, &mut effects, &first, &theme, t0);
    run_out(&mut effects);

    app.key(KeyEvent::from(KeyCode::Tab));
    assert_eq!(
        app.tone.row,
        crate::tui::app::ToneRow::Mode,
        "Tab reached the header"
    );
    assert!(app.frame_flags.pane_changed, "Tab did not raise the light");
    let areas = row_areas(&app);
    assert_eq!(areas.focused, None, "the header is in no frame");
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    let unlit = animated(&app, &mut effects, Duration::from_millis(100));
    for (id, frame) in &areas.frames {
        assert_eq!(
            on_the_contour(&unlit, *frame, theme.glow_lift_at(0.0)),
            0,
            "the header ran a light round {id:?}"
        );
    }
}

/// The light parts from the contour it runs over on the screen a user actually sees: the
/// contour is drawn in the glow and the band in the glow lifted toward white, so a cell of
/// the band stands well clear of a cell of the contour beside it. The other lap cases draw
/// in no colour at all, where a band of the contour's own colour would read as plainly as
/// one of its own.
#[test]
fn the_light_stands_out_against_the_contour_it_runs_over() {
    let mut app = gungnir();
    let mut look = colourful_look();
    let t0 = Instant::now();
    colour(&mut look, &app);
    assert!(
        look.effects.enabled(),
        "a true-colour terminal shows the animation layer"
    );
    let first = row_areas(&app);
    arm_effects(&mut app, &mut look.effects, &first, &look.theme, t0);
    run_out(&mut look.effects);

    app.key(KeyEvent::from(KeyCode::Tab));
    let areas = row_areas(&app);
    arm_effects(&mut app, &mut look.effects, &areas, &look.theme, t0);
    let lit = animated_with(
        &app,
        &mut look.effects,
        &look.theme,
        Duration::from_millis(100),
    );

    let (_, frame) = areas
        .frames
        .iter()
        .find(|(id, _)| Some(*id) == areas.focused)
        .expect("the frame the keys are in");
    let cells = effects::perimeter(*frame);
    let band = cells
        .iter()
        .copied()
        .find(|(x, y)| fg(&lit, *x, *y) == look.theme.glow_lift_at(0.0))
        .expect("a cell of the contour the light is on");
    let contour = cells
        .iter()
        .copied()
        .find(|(x, y)| fg(&lit, *x, *y) == look.theme.glow())
        .expect("a cell of the contour the light has not reached");
    let apart = palette::distance(
        rgb(fg(&lit, band.0, band.1)),
        rgb(fg(&lit, contour.0, contour.1)),
    );
    assert!(
        apart > 100.0,
        "the light is drawn in the contour's own colour: {apart} apart at {band:?} and {contour:?}"
    );
}

/// A reminder instant never runs the light round the focused frame, on the Status pane —
/// which carries no row for the shimmer to sweep instead — or anywhere else: the schedule
/// belongs to the shimmer alone, and a focus change is the only thing that still earns the
/// runner a lap.
#[test]
fn no_lap_runs_on_a_reminder_instant() {
    let mut app = gungnir();
    app.pane = Pane::Status;
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let mut effects = Effects::new(true, &EffectsConfig::default());
    assert_eq!(app.cursor_id(), None, "the Status pane has no row");
    let areas = row_areas(&app);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    run_out(&mut effects);

    effects.on_key(t0);
    for due in REMINDERS {
        arm_effects(&mut app, &mut effects, &areas, &theme, t0 + due);
        assert!(
            !effects.active(),
            "a reminder at {due:?} ran a lap with no focus change"
        );
    }
}

/// The frame a focus change points at still earns its lap on the very instant a reminder
/// falls due, since the two are unrelated: the reminder answers nothing there, and the
/// focus change answers regardless.
#[test]
fn a_lap_runs_on_a_focus_change_even_on_a_reminder_instant() {
    let mut app = gungnir();
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let mut effects = capable_over_no_colour(&EffectsConfig::default());
    let first = row_areas(&app);
    arm_effects(&mut app, &mut effects, &first, &theme, t0);
    run_out(&mut effects);

    app.key(KeyEvent::from(KeyCode::Tab));
    let areas = row_areas(&app);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0 + REMINDERS[0]);
    assert!(effects.active(), "the focus change earned no lap");

    let lit = animated(&app, &mut effects, Duration::from_millis(100));
    let (_, frame) = areas
        .frames
        .iter()
        .find(|(id, _)| Some(*id) == areas.focused)
        .expect("the frame the keys moved to");
    assert!(
        on_the_contour(&lit, *frame, theme.glow_lift_at(0.0)) > 0,
        "the light did not run round the pane the keys moved to"
    );
}

/// The light's colour follows how loud the device plays whether or not the whole screen's
/// decoration is travelling with it: with the hue-drift mode off the screen stays on its
/// resting accent, and a lap armed at the top of the meters' scale is still drawn in the
/// lift of the spectrum's hot end.
#[test]
fn the_light_takes_its_colour_from_the_level_with_the_drift_off() {
    let mut app = gungnir();
    let mut look = colourful_look();
    let t0 = Instant::now();
    assert!(
        !look.tui.effects.hue_drift,
        "the hue-drift mode is off by default"
    );
    colour(&mut look, &app);
    assert!(
        look.effects.enabled(),
        "a true-colour terminal shows the animation layer"
    );
    assert_eq!(
        look.effects.glow_position(),
        0.0,
        "the screen's own decoration is on the accent it rests at"
    );

    // The two colours the band could be drawn in: the lift of the spectrum's hot end, and
    // the lift of the accent the screen is resting on.
    let hot = rgb(look.theme.glow_lift_at(1.0));
    let rest = rgb(look.theme.glow_lift_at(0.0));
    assert_ne!(
        hot, rest,
        "the spectrum's hot end is the accent it rests on"
    );

    let first = row_areas(&app);
    arm_effects(&mut app, &mut look.effects, &first, &look.theme, t0);
    run_out(&mut look.effects);

    look.effects.set_level(Some(1.0));
    app.key(KeyEvent::from(KeyCode::Tab));
    let areas = row_areas(&app);
    arm_effects(&mut app, &mut look.effects, &areas, &look.theme, t0);
    let lit = animated_with(
        &app,
        &mut look.effects,
        &look.theme,
        Duration::from_millis(100),
    );
    let (_, frame) = areas
        .frames
        .iter()
        .find(|(id, _)| Some(*id) == areas.focused)
        .expect("the frame the keys moved to");
    assert!(
        on_the_contour(&lit, *frame, look.theme.glow_lift_at(1.0)) > 0,
        "the light is not drawn in the lift of the level the meters read"
    );
    assert_eq!(
        on_the_contour(&lit, *frame, look.theme.glow_lift_at(0.0)),
        0,
        "the light is drawn at the accent the screen rests on"
    );
    // And measured rather than matched: a cell of the band is nearer the spectrum's hot
    // end than the colour the contour and the whole screen rest at.
    let contour = look.theme.glow();
    let band = effects::perimeter(*frame)
        .into_iter()
        .map(|(x, y)| fg(&lit, x, y))
        .find(|drawn| *drawn != contour)
        .map(rgb)
        .expect("a cell of the contour the light is on");
    assert!(
        palette::distance(band, hot) < palette::distance(band, rest),
        "a band drawn in {band:?} is nearer the resting {rest:?} than the hot {hot:?}"
    );
}

/// With no level streaming and the hue drift on, the runner's band follows the frame's own
/// drifted colour rather than the accent it rests at: on `amber` with the ripple carried
/// toward the cool end, a lap over the focused pane is a brighter form of that same cool
/// colour the contour is already drawn in, not of the gold the screen would rest at with
/// the drift off.
#[tokio::test(start_paused = true)]
async fn the_runner_follows_the_frames_own_colour_with_no_level_streaming() {
    let mut app = gungnir();
    let tui = TuiConfig {
        effects: EffectsConfig {
            hue_drift: true,
            ..EffectsConfig::default()
        },
        ..TuiConfig::default()
    };
    let mut look = Look::new(tui, false, ColorDepth::TrueColor, scratch_config());
    let t0 = Instant::now();
    colour(&mut look, &app);
    assert!(
        look.effects.enabled(),
        "a true-colour terminal shows the animation layer"
    );

    let first = row_areas(&app);
    arm_effects(&mut app, &mut look.effects, &first, &look.theme, t0);
    run_out(&mut look.effects);

    // Let the ripple carry the contour away from the accent it rests at before the lap is
    // armed, so the two are measurably apart.
    animated_with(&app, &mut look.effects, &look.theme, Duration::from_secs(5));
    colour(&mut look, &app);
    assert_ne!(
        look.effects.glow_position(),
        0.0,
        "the ripple left the contour on the accent it rests at"
    );
    // The two lifts the band could be drawn in: the frame's own drifted position, and the
    // accent the screen would rest at with no drift at all.
    let drifted = rgb(look
        .theme
        .glow_lift_at(f64::from(look.effects.glow_position())));
    let rest = rgb(look.theme.glow_lift_at(0.0));
    assert_ne!(drifted, rest, "the drift left the contour at rest");

    app.key(KeyEvent::from(KeyCode::Tab));
    let areas = row_areas(&app);
    arm_effects(&mut app, &mut look.effects, &areas, &look.theme, t0);
    let lit = animated_with(
        &app,
        &mut look.effects,
        &look.theme,
        Duration::from_millis(100),
    );
    let (_, frame) = areas
        .frames
        .iter()
        .find(|(id, _)| Some(*id) == areas.focused)
        .expect("the frame the keys moved to");
    let contour = look.theme.glow();
    let band = effects::perimeter(*frame)
        .into_iter()
        .map(|(x, y)| fg(&lit, x, y))
        .find(|drawn| *drawn != contour)
        .map(rgb)
        .expect("a cell of the contour the light is on");
    assert!(
        palette::distance(band, drifted) < palette::distance(band, rest),
        "a band drawn in {band:?} is nearer the resting lift {rest:?} than the frame's own \
             drifted lift {drifted:?}"
    );
}

/// Nothing runs round a contour while the user is not looking at the screen it is on: the
/// keys over it, the device editor, the question a removal asks and the setup wizard each
/// hold the light back — and the focus change is taken all the same, so one made under an
/// overlay does not run a lap the moment it lifts.
#[test]
fn no_light_runs_round_a_frame_under_an_overlay_or_on_the_wizard() {
    let mut app = App::new(
        gungnir().profile.clone(),
        "gungnir".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("gungnir")],
    );
    app.devices.configured = vec![DeviceConfig {
        name: "gungnir".into(),
        udid: "0102030405060708".into(),
        profile: None,
        passphrase: "auto".into(),
        inputs: std::collections::BTreeMap::new(),
    }];
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let areas = FrameAreas {
        frames: vec![(FrameId::Pane(Pane::Controls), Rect::new(0, 2, 40, 10))],
        focused: Some(FrameId::Pane(Pane::Controls)),
        ..FrameAreas::default()
    };
    let mut effects = Effects::new(true, &EffectsConfig::default());
    // The frame is already on the terminal, so what a case below arms is the lap alone.
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    run_out(&mut effects);

    // What is over the screen holds the light back, and leaves the screen in sight again
    // for the case after it.
    let holds_back = |what: &str, app: &mut App, effects: &mut Effects| {
        assert!(
            !reminding(app, &FrameAreas::default()),
            "{what} is up and the screen is answered"
        );
        app.frame_flags.pane_changed = true;
        effects.on_key(t0);
        arm_effects(app, effects, &areas, &theme, t0 + REMINDERS[0]);
        assert!(!effects.active(), "a light ran under {what}");
        assert!(
            !app.frame_flags.pane_changed,
            "the focus change outlasted {what}"
        );
        app.overlays.help_open = false;
        app.devices.editor = None;
        app.devices.confirm = None;
        app.overlays.quitting = false;
        app.screen = Screen::Device;
    };

    app.overlays.help_open = true;
    holds_back("the keys", &mut app, &mut effects);
    crate::tui::boxes::editor::open(&mut app);
    holds_back("the device editor", &mut app, &mut effects);
    app.devices.confirm = Some("gungnir".into());
    holds_back("a removal", &mut app, &mut effects);
    app.overlays.quitting = true;
    holds_back("the quit question", &mut app, &mut effects);
    app.screen = Screen::Setup;
    holds_back("the wizard", &mut app, &mut effects);

    app.frame_flags.pane_changed = true;
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(
        effects.active(),
        "the same focus change ran no light with the screen in sight"
    );
}

/// Each switch holds back its own animation and nothing else's: with the shimmer's off a
/// reminder sweeps no row, and with the frames switch off a focus change lights no
/// contour — and a reminder never lights one, whatever either switch says.
#[test]
fn each_switch_holds_back_its_own_animation_and_a_reminder_never_lights_the_contour() {
    let mut app = gungnir();
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let cursor = app.cursor_id().expect("a row is focused from the start");
    let frame = Rect::new(0, 2, 40, 10);
    let row = Rect::new(1, 3, 38, 1);
    let areas = FrameAreas {
        rows: vec![(cursor, row)],
        frames: vec![(FrameId::Pane(Pane::Controls), frame)],
        focused: Some(FrameId::Pane(Pane::Controls)),
        ..FrameAreas::default()
    };
    let swept =
        |buffer: &Buffer| (row.x..row.right()).any(|x| fg(buffer, x, row.y) == Color::White);
    // The Controls frame's own arrival is the first thing any fresh layer answers to, so
    // every case below spends it first: what it goes on to check is the reminder and the
    // focus change, not the pane arriving.
    let primed = |config: &EffectsConfig| -> Effects {
        let mut effects = capable_over_no_colour(config);
        arm_effects(&mut gungnir(), &mut effects, &areas, &theme, t0);
        run_out(&mut effects);
        effects
    };

    // The shimmer's switch off: no row sweeps on a reminder, and the reminder still lights
    // no contour, since it never does.
    let mut effects = primed(&EffectsConfig {
        shimmer: false,
        ..EffectsConfig::default()
    });
    effects.on_key(t0);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0 + REMINDERS[0]);
    assert!(
        !effects.active(),
        "the shimmer's switch is off and something ran"
    );

    // The frames switch off: a focus change lights nothing.
    let mut effects = primed(&EffectsConfig {
        frames: false,
        ..EffectsConfig::default()
    });
    app.frame_flags.pane_changed = true;
    arm_effects(&mut app, &mut effects, &areas, &theme, t0);
    assert!(
        !effects.active(),
        "the frames switch is off and a light ran"
    );

    // Both switches on: the reminder still sweeps the row and lights no contour.
    let mut effects = primed(&EffectsConfig::default());
    effects.on_key(t0);
    arm_effects(&mut app, &mut effects, &areas, &theme, t0 + REMINDERS[0]);
    assert!(effects.active(), "the reminder swept no row");
    let sweeping = animated(&app, &mut effects, Duration::from_millis(100));
    assert!(swept(&sweeping), "the row under the cursor was not swept");
    assert_eq!(
        on_the_contour(&sweeping, frame, theme.glow_lift_at(0.0)),
        0,
        "a reminder ran a light round the contour"
    );
}
