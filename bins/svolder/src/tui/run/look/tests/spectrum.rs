// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone screen's scrolling spectrum and the Spectrum row, and the tick they hold.

use super::*;

/// The Gungnir 2 on the tone-control screen, every band flat but band 2 at raw gain `gain` (98
/// is 0 dB, 127 is +6 dB), in tone-control mode `mode` (2 is EQ, 3 is Loudness), every reading
/// heard at once, so nothing is gliding.
fn tone_screen(gain: i64, mode: i64) -> App {
    use crate::tui::test_support::{hear, plain_vo};
    let mut app = gungnir();
    let mut values = vec![plain_vo("12.50", mode), plain_vo("12.52", 0)];
    for band in 1..=5 {
        let raw_gain = if band == 2 { gain } else { 98 };
        values.push(plain_vo(&format!("12.{band}.2"), 64));
        values.push(plain_vo(&format!("12.{band}.3"), raw_gain));
        values.push(plain_vo(&format!("12.{band}.1"), 64));
    }
    hear(&mut app, values);
    app.show(Screen::Tone);
    app
}

/// An animation layer that runs nothing but the spectrum's scroll, switched as asked, so the
/// curve is the only reason it could hold the fast tick.
fn scroll_only(spectrum_drift: bool) -> Effects {
    let mut effects = Effects::new(
        true,
        &EffectsConfig {
            enabled: true,
            eased: false,
            glow: false,
            shimmer: false,
            breathing: false,
            frames: false,
            fault_presence: false,
            peak_hold: false,
            hue_drift: false,
            spectrum_drift,
            list_fade: false,
            mascot: false,
        },
    );
    effects.set_transition(Transition::None);
    effects
}

/// The period the loop takes once `effects` is armed from `areas`, a frame of `app`.
fn period_after(app: &mut App, effects: &mut Effects, areas: &FrameAreas) -> Duration {
    arm_effects(app, effects, areas, &Theme::new(false), Instant::now());
    assert!(
        !effects.active(),
        "an animation was armed, so the period proves nothing"
    );
    tick_period(app, effects)
}

/// The tone screen holds the fast tick for its spectrum's scroll only while the frame just
/// drawn has a curve the scroll moves: a shaped curve, with the scroll switched on. A terminal
/// too small for any screen, the scroll switched off, a flat (muted) curve and Loudness mode's
/// line of text each leave the loop on its slow tick.
#[test]
fn the_tone_screen_ticks_fast_only_while_a_scrolling_curve_is_drawn() {
    let slow = Duration::from_millis(250);

    let mut shaped = tone_screen(127, 2);
    let frame = row_areas(&shaped);
    assert!(
        frame.curve.is_some() && !frame.curve_muted,
        "the frame drew no shaped curve"
    );
    assert_eq!(
        period_after(&mut shaped, &mut scroll_only(true), &frame),
        FAST_TICK,
        "the scroll moves a shaped curve"
    );
    assert_eq!(
        period_after(&mut shaped, &mut scroll_only(true), &FrameAreas::default()),
        slow,
        "a terminal too small for any screen drew no curve"
    );
    assert_eq!(
        period_after(&mut shaped, &mut scroll_only(false), &frame),
        slow,
        "with the scroll off nothing on the curve moves"
    );

    let mut flat = tone_screen(98, 2);
    let frame = row_areas(&flat);
    assert!(frame.curve_muted, "a flat curve is drawn muted");
    assert_eq!(
        period_after(&mut flat, &mut scroll_only(true), &frame),
        slow,
        "the scroll never moves a muted curve"
    );

    let mut loudness = tone_screen(127, 3);
    let frame = row_areas(&loudness);
    assert_eq!(frame.curve, None, "Loudness mode drew a curve");
    assert_eq!(
        period_after(&mut loudness, &mut scroll_only(true), &frame),
        slow,
        "Loudness mode draws no curve"
    );
}

/// The help overlay covers the tone curve, so the spectrum has nothing on the screen to
/// drift over and the loop idles under it rather than redrawing thirty times a second.
#[test]
fn the_tone_screens_spectrum_does_not_hold_the_fast_tick_under_the_help_overlay() {
    let mut app = tone_screen(127, 2);
    let mut on = scroll_only(true);
    let frame = row_areas(&app);
    assert_eq!(
        period_after(&mut app, &mut on, &frame),
        FAST_TICK,
        "the spectrum drifts while the curve is on the screen"
    );

    app.overlays.help_open = true;
    assert_eq!(
        tick_period(&app, &on),
        Duration::from_millis(250),
        "the loop redrew fast for a curve the overlay is covering"
    );
}

/// The Spectrum row falling back is something moving, whether or not the animation layer
/// runs — it is what the host plays, not decoration — so the loop redraws fast while the
/// row is on the screen and a band is above silence, and is back at its slow tick once the
/// row has settled and whenever the row is out of sight.
#[test]
fn a_moving_spectrum_row_redraws_fast_while_it_is_on_the_screen() {
    let mut app = gungnir();
    app.host.linked = true;
    app.host.capturing = true;
    app.host.spectrum_on_screen = true;
    let t0 = Instant::now();
    let block = svolder_host::Block {
        bands: [0.8; svolder_host::fft::BANDS],
        left: 0.5,
        right: 0.5,
    };
    app.heard_block(&block, t0);
    let off = Effects::new(false, &EffectsConfig::default());
    let slow = Duration::from_millis(250);
    assert_eq!(tick_period(&app, &off), FAST_TICK, "the row is falling");
    app.host.spectrum_on_screen = false;
    assert_eq!(
        tick_period(&app, &off),
        slow,
        "a row out of sight redrawn fast"
    );
    app.host.spectrum_on_screen = true;
    app.tick(t0 + crate::tui::host::FALL);
    assert_eq!(tick_period(&app, &off), slow, "the row settled");
}
