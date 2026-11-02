// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The glow carried onto the pane contours by the meters and the host's capture, the colours it is
//! pinned to and the pulse of a standing fault.

use super::*;

/// The animation layer `theme` really gets: capable exactly where production's rule says.
fn layer(theme: &Theme, config: &EffectsConfig) -> Effects {
    Effects::new(shows_effects(theme), config)
}

/// A 256-colour theme never shows the layer, so a helper that draws one with a capable layer
/// is a mistake the helper itself refuses.
#[test]
#[should_panic(expected = "never shows")]
fn the_animated_helper_refuses_a_theme_the_layer_would_not_run_on() {
    let app = gungnir();
    let mut effects = Effects::new(true, &EffectsConfig::default());
    let theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi256);
    animated_with(&app, &mut effects, &theme, Duration::ZERO);
}

/// Two rows whose values the device confirmed on the same frame both glow: the loop hands
/// every changed row to the layer at once.
#[test]
fn two_rows_the_device_changed_on_one_frame_both_glow() {
    let mut app = gungnir();
    let theme = colourful_look().theme;
    let areas = row_areas(&app);
    let mut off = Effects::new(false, &EffectsConfig::default());
    let plain = animated_with(&app, &mut off, &theme, Duration::from_millis(25));
    // A row already drawn in the accent (the cursor's) shows no glow, so it is left out.
    let mut picked: Vec<(FunctionId, Rect)> = Vec::new();
    for &(id, rect) in &areas.rows {
        let resting = fg(&plain, rect.x, rect.y);
        if resting != theme.accent() && !picked.iter().any(|(_, had)| *had == rect) {
            picked.push((id, rect));
        }
    }
    picked.truncate(2);
    assert_eq!(
        picked.len(),
        2,
        "the device screen reported fewer than two rows"
    );
    app.frame_flags.take();
    app.frame_flags.changed_rows = picked.iter().map(|(id, _)| *id).collect();
    let mut effects = layer(
        &theme,
        &EffectsConfig {
            shimmer: false,
            glow: false,
            frames: false,
            ..EffectsConfig::default()
        },
    );
    effects.set_transition(Transition::None);
    arm_effects(&mut app, &mut effects, &areas, &theme, Instant::now());
    animated_with(&app, &mut effects, &theme, Duration::ZERO);
    let glowing = animated_with(&app, &mut effects, &theme, Duration::from_millis(25));
    for (id, row) in picked {
        let lit = (row.x..row.right()).any(|x| {
            glowing.cell((x, row.y)).map(|cell| cell.fg)
                != plain.cell((x, row.y)).map(|cell| cell.fg)
        });
        assert!(lit, "the row of {id} did not glow");
    }
}

/// The glow reaches the screen through the loop: an amplifier playing near the top of its
/// meters' scale draws its pane contours nearer the hot end of the palette's spectrum than
/// the colour they rest at.
#[tokio::test(start_paused = true)]
async fn the_meters_carry_the_glow_onto_the_pane_contours() {
    let profile = with_range_on(
        Registry::builtin()
            .expect("the built-in profiles load")
            .get("vidar2")
            .expect("vidar2 is built in")
            .clone(),
        "meter_1",
        (0, 1_023),
    );
    let tui = TuiConfig {
        effects: EffectsConfig {
            hue_drift: true,
            ..EffectsConfig::default()
        },
        ..TuiConfig::default()
    };
    let mut look = Look::new(tui.clone(), false, ColorDepth::TrueColor, scratch_config());
    let mut app = App::new(profile, "vidar".into(), &tui, Vec::new());
    app.set_look(&look.tui, &look.effects);
    app.apply(
        Update::Status(ValueObject {
            heard_at_ms: None,
            id: "20.1".into(),
            key: Some("meter_1".into()),
            label: "Meter L".into(),
            value_type: "u16".into(),
            raw: json!(1_000),
            display: "1000".into(),
            value: None,
            unit: None,
        }),
        Instant::now(),
    );

    look.theme.set_family(Family::Amp);
    let rest = look.theme.glow_rgb();
    look.theme.set_glow(1.0);
    let hot = look.theme.glow_rgb();

    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    // Two turns of the loop: the first works the glow out from what the meters read, the
    // second draws the frame that carries it.
    for _ in 0..2 {
        colour(&mut look, &app);
        terminal
            .draw(|frame| {
                render::draw(frame, &app, &look.theme, &mut hits);
                look.effects.render(frame, Duration::from_millis(250));
            })
            .expect("the frame draws");
    }

    let corner = terminal
        .backend()
        .buffer()
        .cell((0, 2))
        .expect("the first pane's top-left corner")
        .clone();
    assert_eq!(corner.symbol(), "┌", "the corner moved");
    let drawn = palette::rgb_of(corner.fg);
    assert!(
        palette::distance(drawn, hot) < palette::distance(drawn, rest),
        "a contour drawn in {drawn:?} is nearer the colour it rests at {rest:?} \
             than the spectrum's hot end {hot:?}"
    );
}

/// On a DAC, which streams no meters, the capture of the host's sink carries the glow the
/// same way: playing loud, its level draws the pane contours nearer the hot end of the
/// palette's spectrum than the colour they rest at, and with the capture stopped they rest.
#[tokio::test(start_paused = true)]
async fn the_hosts_capture_carries_the_glow_onto_the_pane_contours() {
    let tui = TuiConfig {
        effects: EffectsConfig {
            hue_drift: true,
            ..EffectsConfig::default()
        },
        ..TuiConfig::default()
    };
    let corner = |capturing: bool| {
        let mut look = Look::new(tui.clone(), false, ColorDepth::TrueColor, scratch_config());
        let mut app = gungnir();
        app.set_look(&look.tui, &look.effects);
        app.host.linked = true;
        app.host.capturing = capturing;
        app.host.level = Some(1.0);
        app.host.channels = Some([1.0, 1.0]);
        look.theme.set_family(app.profile.family);
        let rest = look.theme.glow_rgb();
        look.theme.set_glow(1.0);
        let hot = look.theme.glow_rgb();
        let mut terminal =
            Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
        let mut hits = HitMap::default();
        // The first turn works the glow out from the level, the second draws it.
        for _ in 0..2 {
            colour(&mut look, &app);
            terminal
                .draw(|frame| {
                    render::draw(frame, &app, &look.theme, &mut hits);
                    look.effects.render(frame, Duration::from_millis(250));
                })
                .expect("the frame draws");
        }
        let cell = terminal
            .backend()
            .buffer()
            .cell((0, 2))
            .expect("the first pane's top-left corner")
            .clone();
        assert_eq!(cell.symbol(), "┌", "the corner moved");
        let drawn = palette::rgb_of(cell.fg);
        palette::distance(drawn, hot) < palette::distance(drawn, rest)
    };
    assert!(corner(true), "the capture's level left the contour at rest");
    assert!(!corner(false), "the contour glowed with no capture running");
}

/// Golden: the runner mid-lap on `amber`, true colour, with no level streaming — pinned as
/// the exact colours the code draws today, next to
/// [`the_light_stands_out_against_the_contour_it_runs_over`]'s relational check of the same
/// scene rather than in place of it. The band is `amber`'s resting accent (`#e0a030`)
/// lifted toward white, and a cell of the contour the band has not reached yet is that
/// same accent, unlifted.
#[test]
fn golden_the_runner_mid_lap_pins_the_band_and_contour_colours() {
    let mut app = gungnir();
    let mut look = colourful_look();
    let t0 = Instant::now();
    colour(&mut look, &app);
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
        .expect("the frame the keys moved to");
    let band = Color::Rgb(0xf3, 0xd9, 0xac);
    let contour = Color::Rgb(0xe0, 0xa0, 0x30);
    let cells = effects::perimeter(*frame);
    assert!(
        cells.iter().any(|(x, y)| fg(&lit, *x, *y) == band),
        "no cell of the lap's band is drawn in #f3d9ac, amber's resting accent lifted"
    );
    assert!(
        cells.iter().any(|(x, y)| fg(&lit, *x, *y) == contour),
        "no cell of the contour still rests in #e0a030, amber's plain resting accent"
    );
}

/// Render the device screen under `theme` and check that an unfocused pane's own frame and
/// title, and a key hint on a border, are all drawn in `expected` — the drift's own
/// colour, wherever it rests: the golden the two tests below pin at level 0 and at level 1.
fn assert_golden_frame_title_and_key_hint(theme: &Theme, expected: Color) {
    let app = gungnir();
    assert_eq!(app.pane, Pane::Controls, "Controls is the focused pane");
    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    let mut hits = HitMap::default();
    let mut areas = FrameAreas::default();
    terminal
        .draw(|frame| areas = render::draw(frame, &app, theme, &mut hits))
        .expect("the frame draws");
    let buffer = terminal.backend().buffer();

    let (_, tone) = areas
        .frames
        .iter()
        .find(|(id, _)| *id == FrameId::Pane(Pane::Tone))
        .expect("the Tone pane, unfocused from Controls");
    assert_eq!(
        fg(buffer, tone.x, tone.y),
        expected,
        "the unfocused pane's own top-left corner"
    );
    let is_letter = |x: u16, y: u16| {
        buffer
            .cell((x, y))
            .and_then(|cell| cell.symbol().chars().next())
            .is_some_and(|c| c.is_alphabetic())
    };
    let title_x = (tone.x + 1..tone.right() - 1)
        .find(|&x| is_letter(x, tone.y))
        .expect("the pane's own title text");
    assert_eq!(
        fg(buffer, title_x, tone.y),
        expected,
        "the unfocused pane's own title"
    );

    let (_, controls) = areas
        .frames
        .iter()
        .find(|(id, _)| *id == FrameId::Pane(Pane::Controls))
        .expect("the Controls pane");
    let key_x = (controls.x + 1..controls.right() - 1)
        .find(|&x| {
            buffer
                .cell((x, controls.y))
                .is_some_and(|cell| cell.symbol() == "0")
        })
        .expect("the `0` key of the Controls pane's border words");
    assert_eq!(
        fg(buffer, key_x, controls.y),
        expected,
        "the key hint itself"
    );
}

/// Golden: the glow at level 0 on `amber`, true colour, DAC family — pinned as the exact
/// colour (`#e0a030`, the plain resting accent) the frame, the title and a key hint draw
/// today, next to the relational checks elsewhere in this module.
#[test]
fn golden_the_glow_at_level_0_pins_the_frame_title_and_key_hint_colours() {
    let mut theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    theme.set_family(Family::Dac);
    theme.set_glow(0.0);
    assert_golden_frame_title_and_key_hint(&theme, Color::Rgb(0xe0, 0xa0, 0x30));
}

/// Golden: the glow at level 1 on `amber`, true colour, DAC family — pinned as the exact
/// colour (`#f6dca0`, the spectrum's hot end) the frame, the title and a key hint draw
/// today, next to the relational checks elsewhere in this module.
#[test]
fn golden_the_glow_at_level_1_pins_the_frame_title_and_key_hint_colours() {
    let mut theme = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    theme.set_family(Family::Dac);
    theme.set_glow(1.0);
    assert_golden_frame_title_and_key_hint(&theme, Color::Rgb(0xf6, 0xdc, 0xa0));
}

/// An effect that started before a modal box opened — the shimmer crossing the selected
/// row — still plays out over the area it was armed on, but the frame dims everything
/// outside the box again after it, so no dimmed cell is brightened.
#[test]
fn an_effect_started_before_a_modal_never_brightens_a_dimmed_cell() {
    let (width, height) = (126, 40);
    let grey = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor)
        .style(Role::Dim)
        .fg
        .expect("a grey at true colour");
    // One frame with the shimmer armed on the selected row, then the overlay opened over
    // it, drawn with `dim_again` saying whether the frame dims after the effects.
    let frame_of = |dim_again: bool| {
        let mut look = colourful_look();
        let mut app = crate::tui::test_support::app_showing(Screen::Device);
        app.set_look(&look.tui, &look.effects);
        let mut terminal =
            Terminal::new(TestBackend::new(width, height)).expect("a test terminal opens");
        let mut hits = HitMap::default();
        let mut areas = FrameAreas::default();
        terminal
            .draw(|frame| areas = render::draw(frame, &app, &look.theme, &mut hits))
            .expect("the frame draws");
        let row = row_rect(&areas, app.cursor_id().expect("a cursor row"))
            .expect("the selected row is drawn");
        assert!(look.effects.on_shimmer(row), "the shimmer did not start");
        app.overlays.help_open = true;
        terminal
            .draw(|frame| {
                areas = render::draw(frame, &app, &look.theme, &mut hits);
                match dim_again {
                    true => paint_over(
                        frame,
                        &areas,
                        &look.theme,
                        &mut look.effects,
                        Duration::from_millis(250),
                    ),
                    false => look.effects.render(frame, Duration::from_millis(250)),
                }
            })
            .expect("the frame draws");
        let boxed = areas.modal.expect("the overlay is a modal box");
        let buffer = terminal.backend().buffer().clone();
        (0..boxed.x)
            .map(|x| buffer.cell((x, row.y)).expect("a cell of the row").fg)
            .collect::<Vec<Color>>()
    };
    assert!(
        frame_of(false).iter().any(|fg| *fg != grey),
        "the shimmer brightened nothing to begin with, so this case proves nothing"
    );
    let dimmed = frame_of(true);
    assert!(
        dimmed.iter().all(|fg| *fg == grey),
        "a cell outside the overlay is brighter than the dim role: {dimmed:?}"
    );
}

/// A fault the device keeps reporting pulses the rows it is written on — the header and
/// the Status pane's fault line — and the pulse stops with the fault.
#[test]
fn a_standing_fault_pulses_the_header_and_the_fault_line() {
    let profile = crate::tui::test_support::with_overcurrent(
        Registry::builtin()
            .expect("the built-in profiles load")
            .get("gungnir2-v2")
            .expect("gungnir2-v2 is built in")
            .clone(),
    );
    let mut app = App::new(profile, "gungnir".into(), &TuiConfig::default(), Vec::new());
    let theme = Theme::new(false);
    let t0 = Instant::now();
    let fault = |active: bool| ValueObject {
        heard_at_ms: None,
        id: "230.1".into(),
        key: Some("fault_overcurrent".into()),
        label: "Over-current".into(),
        value_type: "bool".into(),
        raw: json!(active),
        display: if active { "active" } else { "clear" }.into(),
        value: None,
        unit: None,
    };
    // No transition, so the first frame's panes arriving is not what is running.
    let mut effects = Effects::new(true, &EffectsConfig::default());
    effects.set_transition(Transition::None);
    let arm = |app: &mut App, effects: &mut Effects| {
        let areas = row_areas(app);
        arm_effects(app, effects, &areas, &theme, t0);
        areas
    };

    let quiet = arm(&mut app, &mut effects);
    assert_eq!(quiet.faults, None, "a clear device has no fault line");
    assert!(!effects.active(), "nothing is wrong and something pulses");

    app.apply(Update::Status(fault(true)), t0);
    let areas = arm(&mut app, &mut effects);
    let line = areas.faults.expect("the Status pane's fault line");
    assert!(
        line.height == 1 && line.y > areas.header.bottom(),
        "the fault line is not one row inside the Status pane: {line:?}"
    );
    assert!(effects.active(), "a standing fault pulses nothing");

    app.apply(Update::Status(fault(false)), t0);
    arm(&mut app, &mut effects);
    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    // Long enough for the one-shot flash the fault going active earned to be over too:
    // what is left running after this could only be the pulse, which repeats for ever.
    terminal
        .draw(|frame| effects.render(frame, Duration::from_millis(700)))
        .expect("the frame draws");
    assert!(
        !effects.active(),
        "the pulse outlasted the fault that started it"
    );

    // A modal box dims the header and the fault line around it, and the pulse stops
    // there with the fault still standing: nothing plays over a dimmed cell.
    app.apply(Update::Status(fault(true)), t0);
    arm(&mut app, &mut effects);
    assert!(effects.active(), "the fault did not pulse again");
    app.overlays.help_open = true;
    let covered = arm(&mut app, &mut effects);
    assert!(covered.header.is_empty() && covered.faults.is_none());
    terminal
        .draw(|frame| effects.render(frame, Duration::from_millis(700)))
        .expect("the frame draws");
    assert!(
        !effects.active(),
        "the fault still pulses under the help overlay"
    );
}
