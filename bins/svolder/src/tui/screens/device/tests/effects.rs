// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device screen under effects: breathing borders, the arrival trace and glowing frames.

use super::*;

/// The pane contours take their colour from how loud the meters read, and keep the accent
/// rule's own where nothing is breathing: with the switch off, and at a colour depth with
/// no room to shade.
#[test]
fn the_pane_borders_breathe_with_the_meters_and_stand_still_without_them() {
    let true_colour = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    let corner = |app: &App, theme: &Theme| {
        frame_with_theme(app, 80, 24, theme)
            .cell((0, 2))
            .expect("the Controls pane's top-left corner")
            .clone()
    };

    let quiet = corner(&app_with_meter_reading(1_000), &true_colour);
    let loud = corner(&app_with_meter_reading(9_000), &true_colour);
    assert_eq!(quiet.symbol(), "┌", "the corner moved");
    assert_ne!(
        quiet.fg, loud.fg,
        "the contour is the same colour at a tenth of full scale as at nine tenths"
    );

    let rule = true_colour.style(Role::Rule).fg.expect("the rule's colour");
    let mut still = app_with_meter_reading(9_000);
    still.breathing = false;
    assert_eq!(
        corner(&still, &true_colour).fg,
        rule,
        "a contour with the breathing switched off left the accent rule's colour"
    );
    let mut silent = app_on(profile_named("vidar2"));
    animated(&mut silent);
    assert!(
        silent.breathing,
        "the switch is on, so the readings are what is missing"
    );
    assert_eq!(
        corner(&silent, &true_colour).fg,
        rule,
        "a device with no meter readings breathed anyway"
    );

    // Below true colour a shade is one flat colour, so the contour stands still there
    // however loud the device is.
    let ansi16 = Theme::from_config(&TuiConfig::default(), false, ColorDepth::Ansi16);
    assert_eq!(
        corner(&app_with_meter_reading(1_000), &ansi16).fg,
        corner(&app_with_meter_reading(9_000), &ansi16).fg,
        "the contour breathes at sixteen colours"
    );
}

/// On a DAC, which streams no meters, the contours breathe with the capture of the host's
/// sink while it runs, and keep the accent rule's own colour once it has stopped.
#[test]
fn the_pane_borders_breathe_with_the_hosts_capture_on_a_dac() {
    let true_colour = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    let rule = true_colour.style(Role::Rule).fg.expect("the rule's colour");
    let corner = |level: f64, capturing: bool| {
        let mut app = hearing(Some(track("So What")));
        animated(&mut app);
        app.host.capturing = capturing;
        app.host.level = Some(level);
        frame_with_theme(&app, 80, 24, &true_colour)
            .cell((0, 2))
            .expect("the Controls pane's top-left corner")
            .fg
    };
    assert_ne!(
        corner(0.1, true),
        corner(0.9, true),
        "the contour is the same colour at a tenth of the capture's scale as at nine tenths"
    );
    assert_ne!(
        corner(0.1, true),
        rule,
        "a quiet capture left the contour at rest"
    );
    assert_eq!(
        corner(0.9, false),
        rule,
        "the contour breathed with no capture running"
    );
}

/// The words are part of the frame the arrival trace draws: with the trace over, the border
/// row reads as it does without effects.
#[test]
fn the_words_are_drawn_once_the_arrival_trace_ends() {
    let app = mockup_app();
    let area = Rect::new(0, 0, 80, 24);
    let mut effects = Effects::new(true, &app.tui.effects);
    let frames: Vec<Rect> = frames_of(area, &app)
        .into_iter()
        .map(|(_, frame)| frame)
        .collect();
    assert!(effects.on_arrival(&frames), "the panes announced nothing");
    let mut terminal =
        Terminal::new(ratatui::backend::TestBackend::new(80, 24)).expect("a test terminal opens");
    let theme = Theme::new(false);
    let mut hits = HitMap::default();
    for tick in [Duration::from_millis(100), Duration::from_millis(900)] {
        terminal
            .draw(|f| {
                render::draw(f, &app, &theme, &mut hits);
                effects.render(f, tick);
            })
            .expect("the frame draws");
    }
    assert!(!effects.active(), "the trace is still running");
    assert_eq!(
        text(terminal.backend().buffer())[2],
        "┌ Controls ───────── mute ─ 0 centre ┐┌ Tone control ────────── flat ─ presets ┐"
    );
}

/// On a palette with no emphasis colour every screen's frames are drawn in the colour the
/// device screen's focused pane is, glowing with the level as it does, the Tone screen's
/// Response, Bands and AutoEq panes, the Telemetry list and the Devices list among them.
#[test]
fn every_screens_frames_glow_in_the_device_screens_colour() {
    let theme = theme_for("amber", ColorDepth::TrueColor);
    let mut app = hearing(Some(track("So What")));
    animated(&mut app);
    app.host.capturing = true;
    app.host.level = Some(0.9);
    app.screen = Screen::Device;
    let rule = theme.style(Role::Rule).fg.expect("the rule's colour");
    let reference = frame_with_theme(&app, 120, 40, &theme)
        .content()
        .iter()
        .find(|cell| cell.symbol() == "┌")
        .expect("the Controls pane's top-left corner")
        .clone();
    assert_ne!(
        reference.fg, rule,
        "the contour is not breathing, so this proves nothing"
    );
    for screen in [Screen::Tone, Screen::Telemetry, Screen::Devices] {
        app.screen = screen;
        let buffer = frame_with_theme(&app, 120, 40, &theme);
        let corners: Vec<_> = buffer
            .content()
            .iter()
            .filter(|cell| cell.symbol() == "┌")
            .collect();
        assert!(!corners.is_empty(), "{screen:?} drew no frame");
        for corner in corners {
            assert_eq!(
                corner.fg, reference.fg,
                "{screen:?}: a frame is not in the glow"
            );
        }
    }
}

/// A beat in the daemon's frames lifts the contours of a DAC's frames for a moment: at the
/// beat they are shaded as for a louder level, a pulse later as at rest, and a level the capture
/// has not heard lifts nothing.
#[test]
fn a_beat_lifts_the_borders_for_a_moment() {
    let true_colour = Theme::from_config(&TuiConfig::default(), false, ColorDepth::TrueColor);
    let corner = |at: Duration, beat: bool| {
        let mut app = hearing(Some(track("So What")));
        animated(&mut app);
        app.host.capturing = true;
        app.host.level = Some(0.3);
        let landed = app.now;
        if beat {
            app.host.beat_landed(landed);
        }
        app.now = landed + at;
        frame_with_theme(&app, 80, 24, &true_colour)
            .cell((0, 2))
            .expect("the Controls pane's top-left corner")
            .fg
    };
    let rest = corner(Duration::ZERO, false);
    assert_ne!(
        corner(Duration::ZERO, true),
        rest,
        "a beat left the borders as they were"
    );
    assert_eq!(
        corner(host::BEAT_PULSE, true),
        rest,
        "the borders stayed lifted after the pulse"
    );
}
