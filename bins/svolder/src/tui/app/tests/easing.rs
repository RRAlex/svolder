// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The easing of a changed value on its bar: when it starts, holds, lands and is dropped.

use super::*;

/// The volume function, which every easing case moves.
const VOLUME: FunctionId = FunctionId::one(10);

/// An app that has already confirmed a volume of `raw`, so the next reading of that
/// function is a change rather than a first value.
fn app_with_volume(raw: i64) -> App {
    let mut app = app();
    animated(&mut app);
    app.apply(status(volume(raw)), Instant::now());
    app
}

/// An app that has heard nothing at all, with the animations its settings ask for running.
fn app_without_readings() -> App {
    let mut app = app();
    animated(&mut app);
    app
}

#[test]
fn a_changed_value_glides_for_two_hundred_milliseconds() {
    let mut app = app_with_volume(63);
    let t0 = Instant::now();
    app.apply(status(volume(80)), t0);
    assert_eq!(app.eased_raw(VOLUME, t0), Some(63));
    // Halfway through the easing's own 200ms, the quadratic ease-out has already covered three
    // quarters of the distance (1 - 0.5²): 63 + round(17 × 0.75) = 76. A linear easing would
    // read 71 or 72 here instead, so this pins the curve's shape rather than just its bounds.
    assert_eq!(
        app.eased_raw(VOLUME, t0 + Duration::from_millis(100)),
        Some(76)
    );
    assert_eq!(
        app.eased_raw(VOLUME, t0 + Duration::from_millis(200)),
        Some(80)
    );
}

/// Only a value the device confirms as different from the one before it is drawn moving,
/// on an app whose easing is running: a first value has nowhere to come from, and an echo of
/// the value already shown has not moved.
#[test]
fn a_first_value_and_an_unchanged_echo_do_not_glide() {
    let mut app = app_without_readings();
    assert!(app.eased, "the easing is what this case is about");
    let t0 = Instant::now();
    app.apply(status(volume(63)), t0);
    assert_eq!(app.eased_raw(VOLUME, t0), Some(63));
    assert!(
        app.readings.easings.is_empty(),
        "a first value came from nowhere"
    );
    app.apply(status(volume(63)), t0 + Duration::from_millis(50));
    assert!(
        app.readings.easings.is_empty(),
        "an echo of the value shown moved"
    );
    app.apply(status(volume(80)), t0 + Duration::from_millis(100));
    assert!(
        !app.readings.easings.is_empty(),
        "a value that did change is drawn moving on this very app"
    );
}

/// `settle` lands every easing whatever the clock read when the value was heard.
#[test]
fn settle_lands_easings_started_long_after_the_clock() {
    let mut app = app_with_volume(63);
    let clock = app.now;
    let late = clock + Duration::from_secs(5);
    app.apply(status(volume(80)), late);
    assert!(app.is_easing(late), "the volume bar is gliding");
    app.now = clock;
    app.settle();
    assert!(!app.is_easing(app.now), "every easing landed");
}

/// An easing is what the loop redraws fast for, and it is dropped the moment it lands: one
/// that outlived its two hundred milliseconds would hold the fast tick for the rest of the
/// run.
#[test]
fn an_easing_holds_until_it_lands_and_is_dropped_at_the_next_tick() {
    let mut app = app_with_volume(63);
    let t0 = Instant::now();
    app.apply(status(volume(80)), t0);
    assert!(app.is_easing(t0), "the value has somewhere to go");
    app.tick(t0 + Duration::from_millis(100));
    assert!(app.is_easing(app.now), "and is still on its way");
    assert!(!app.readings.easings.is_empty());
    app.tick(t0 + Duration::from_millis(200));
    assert!(
        app.readings.easings.is_empty(),
        "a landed easing is dropped"
    );
    assert!(!app.is_easing(app.now));
    assert_eq!(
        app.eased_raw(VOLUME, app.now),
        Some(80),
        "and the bar draws the value the device confirmed"
    );
}

/// A swing across the whole of `i64` does not overflow computing the eased position partway
/// through the easing.
#[test]
fn a_full_range_swing_does_not_overflow_the_easing() {
    let mut app = app_with_volume(i64::MIN);
    let t0 = Instant::now();
    app.apply(status(volume(i64::MAX)), t0);
    assert!(
        app.is_easing(t0),
        "a value that changed this much still glides"
    );
    let _ = app.eased_raw(VOLUME, t0 + Duration::from_millis(50));
}

/// A refresh mid-easing does not leave the bar finishing an easing towards a value the refresh
/// has already overtaken: the fresh reading draws at once.
#[test]
fn a_refresh_mid_easing_drops_it_rather_than_finishing_it() {
    let mut app = app_with_volume(63);
    let t0 = Instant::now();
    app.apply(status(volume(80)), t0);
    assert!(app.is_easing(t0), "the easing toward 80 has started");
    app.replace_all(vec![volume(95)], t0 + Duration::from_millis(50));
    assert!(
        app.readings.easings.is_empty(),
        "the refresh drops an easing from before it"
    );
    assert_eq!(
        app.eased_raw(VOLUME, t0 + Duration::from_millis(50)),
        Some(95),
        "the fresh reading draws at once rather than chasing the old easing's target"
    );
}

/// A meter reports several times a second, so its bar is drawn at the level the device
/// last sent rather than chasing it: nothing glides, and the loop is not held at its fast
/// tick for as long as the device is streaming. A control the user sets still glides.
#[test]
fn a_meter_never_glides_and_a_volume_change_still_does() {
    let vidar = Registry::builtin()
        .expect("the built-in profiles load")
        .get("vidar2")
        .expect("vidar2 is built in")
        .clone();
    let meter = FunctionId::two(20, 1);
    let mut app = App::new(vidar, "vidar".into(), &TuiConfig::default(), Vec::new());
    animated(&mut app);
    let t0 = Instant::now();
    let level = |raw: i64| ValueObject {
        value_type: "u16".into(),
        ..vo("20.1", Some("meter_1"), json!(raw), &raw.to_string())
    };

    app.apply(status(level(100)), t0);
    app.apply(status(level(900)), t0);
    assert!(app.readings.easings.is_empty(), "a meter glides nowhere");
    assert!(!app.is_easing(t0), "and holds nothing at the fast tick");
    assert_eq!(
        app.eased_raw(meter, t0),
        None,
        "a meter's bar is drawn at the level the device sent"
    );

    let mut dac = app_with_volume(63);
    dac.apply(status(volume(80)), t0);
    assert_eq!(
        dac.eased_raw(VOLUME, t0),
        Some(63),
        "a control the user sets still glides"
    );
    assert!(dac.is_easing(t0));
}

/// With the `eased` switch off no value glides at all: the bar draws what the device
/// confirmed, and the loop has nothing to redraw fast for.
#[test]
fn nothing_glides_with_the_eased_switch_off() {
    let tui = TuiConfig {
        effects: EffectsConfig {
            eased: false,
            ..EffectsConfig::default()
        },
        ..TuiConfig::default()
    };
    let mut app = App::new(profile(), "gungnir".into(), &tui, vec![row("gungnir")]);
    // With the layer running, so the one switch this case turns off is what stops the
    // easing rather than a terminal that could not have shown it anyway.
    animated(&mut app);
    assert!(app.effects && !app.eased, "only the eased switch is off");
    let t0 = Instant::now();
    app.apply(status(volume(63)), t0);
    app.apply(status(volume(80)), t0);
    assert!(app.readings.easings.is_empty(), "the switch is off");
    assert_eq!(app.eased_raw(VOLUME, t0), Some(80));
}
