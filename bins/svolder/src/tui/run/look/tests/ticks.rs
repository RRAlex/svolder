// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The loop's tick period: a quarter second at rest, a frame while something moves, and what holds
//! it there.

use super::*;

/// The loop ticks four times a second, which is what ages the notice and the readings, and
/// thirty times a second only while the animation layer has something to move: an effect
/// running, or the tone curve's spectrum drifting.
#[test]
fn the_tick_period_is_a_quarter_second_and_falls_to_a_frame_only_for_an_effect() {
    let mut app = App::new(
        Registry::generic(0),
        "fake".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let slow = Duration::from_millis(250);
    assert_eq!(app.tick_period(), slow);

    let off = Effects::new(false, &EffectsConfig::default());
    assert_eq!(tick_period(&app, &off), slow);
    app.screen = Screen::Tone;
    assert_eq!(
        tick_period(&app, &off),
        slow,
        "nothing drifts with effects off"
    );

    let mut on = Effects::new(true, &EffectsConfig::default());
    assert_eq!(
        tick_period(&app, &on),
        slow,
        "a tone screen that drew no curve has nothing scrolling"
    );
    app.screen = Screen::Device;
    assert_eq!(
        tick_period(&app, &on),
        slow,
        "an animation layer with nothing running ticks as slowly as one that is off"
    );
    on.on_screen_change(Rect::new(0, 0, 40, 6));
    assert_eq!(
        tick_period(&app, &on),
        FAST_TICK,
        "a running effect is redrawn a frame at a time"
    );
}

/// The loop ticks a frame at a time while the volume climbs to a target, with the animation layer
/// off as well as on, and is back on its slow tick once the climb has written its target.
#[test]
fn the_tick_period_is_a_frame_while_the_volume_climbs() {
    use crate::tui::app::glide::{self, Bar};
    use crate::tui::test_support::{hear_one, profile};
    use ratatui::layout::Rect;

    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("gungnir")],
    );
    hear_one(&mut app, "10", 64);
    let slow = Duration::from_millis(250);
    let off = Effects::new(false, &EffectsConfig::default());
    assert_eq!(tick_period(&app, &off), slow);

    let row = app
        .rows()
        .iter()
        .position(|row| row.control.key == "volume")
        .expect("a volume row");
    let bar = Rect::new(0, 0, 127, 1);
    let t0 = app.now;
    glide::press(&mut app, Bar::Row(row), bar, 126);
    assert_eq!(tick_period(&app, &off), FAST_TICK, "the climb is running");

    let asked: Vec<_> = app
        .readings
        .pending
        .keys()
        .map(ToString::to_string)
        .collect();
    for id in asked {
        hear_one(&mut app, &id, 95);
    }
    for tenth in 1..=40 {
        app.tick(t0 + Duration::from_millis(100 * tenth));
        for intent in glide::confirmed(&mut app) {
            if let Intent::Set { value, .. } = intent {
                hear_one(&mut app, "10", value.parse().expect("a step"));
            }
        }
    }
    assert!(app.pointer.climb.is_none());
    assert_eq!(tick_period(&app, &off), slow, "the climb is over");
}

/// The loop ticks at a hundred milliseconds or faster while a spinner is turning — the
/// header's while the link is down, the Devices screen's while a listed device carries a
/// retry wait — with the animation layer off as well as on, and falls back to its own slow
/// tick once neither is.
#[test]
fn the_tick_period_is_a_hundred_milliseconds_while_a_spinner_turns() {
    let mut app = App::new(
        Registry::generic(0),
        "fake".into(),
        &TuiConfig::default(),
        vec![DeviceRow::for_test("fake")],
    );
    let (slow, spinning) = (Duration::from_millis(250), Duration::from_millis(100));
    let off = Effects::new(false, &EffectsConfig::default());
    assert_eq!(tick_period(&app, &off), slow, "nothing is waiting yet");

    app.session.connected = false;
    assert_eq!(
        tick_period(&app, &off),
        spinning,
        "the header's own spinner did not speed the tick up with effects off"
    );
    app.session.connected = true;
    assert_eq!(
        tick_period(&app, &off),
        slow,
        "the tick did not fall back once the link came back"
    );

    app.screen = Screen::Devices;
    assert_eq!(
        tick_period(&app, &off),
        slow,
        "the list carries no retry wait yet"
    );
    app.devices.rows[0].retry_in_secs = Some(15);
    assert_eq!(
        tick_period(&app, &off),
        spinning,
        "the Devices screen's own spinner did not speed the tick up"
    );
}

/// The shimmer costs one burst and nothing between, and the light running round a frame
/// costs one lap: once each is over the loop is back at its quarter-second tick, which is
/// where it waits out the gap to the next reminder.
#[test]
fn the_period_returns_to_a_quarter_second_once_the_shimmers_burst_is_over() {
    let app = App::new(
        Registry::generic(0),
        "fake".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let mut on = Effects::new(true, &EffectsConfig::default());
    on.on_shimmer(Rect::new(0, 0, 20, 1));
    assert_eq!(
        tick_period(&app, &on),
        FAST_TICK,
        "a shimmer is redrawn a frame at a time"
    );

    let mut terminal =
        Terminal::new(TestBackend::new(SIZE.0, SIZE.1)).expect("a test terminal opens");
    terminal
        .draw(|frame| on.render(frame, Duration::from_millis(u64::from(shimmer_ms(20)) + 50)))
        .expect("the frame draws");

    assert_eq!(
        tick_period(&app, &on),
        Duration::from_millis(250),
        "the loop idles between one sweep and the next"
    );

    on.on_lap(Rect::new(0, 0, 20, 5), Color::Yellow);
    assert_eq!(
        tick_period(&app, &on),
        FAST_TICK,
        "a light running round a frame is redrawn a frame at a time"
    );
    terminal
        .draw(|frame| on.render(frame, Duration::from_millis(1_300)))
        .expect("the frame draws");
    assert_eq!(
        tick_period(&app, &on),
        Duration::from_millis(250),
        "the loop idles once the light has been round"
    );
}

/// A value on its way to what the device confirmed is something animating, so the loop
/// redraws fast until it lands and falls back to its slow tick there.
#[test]
fn a_gliding_value_is_redrawn_a_frame_at_a_time_until_it_lands() {
    let mut app = App::new(
        Registry::generic(0),
        "fake".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let on = Effects::new(true, &EffectsConfig::default());
    // What the loop itself does once the layer has said what this terminal can show, which
    // is what sets a changed value gliding.
    app.set_look(&TuiConfig::default(), &on);
    let t0 = Instant::now();
    let reading = |raw: i64| ValueObject {
        heard_at_ms: None,
        id: "10".into(),
        key: Some("volume".into()),
        label: "Volume".into(),
        value_type: "u8".into(),
        raw: json!(raw),
        display: raw.to_string(),
        value: None,
        unit: None,
    };
    app.apply(Update::Status(reading(63)), t0);
    assert_eq!(
        tick_period(&app, &on),
        Duration::from_millis(250),
        "a first value does not glide"
    );

    app.apply(Update::Status(reading(80)), t0);
    assert_eq!(tick_period(&app, &on), FAST_TICK, "the value is gliding");
    app.tick(t0 + Duration::from_millis(200));
    assert_eq!(
        tick_period(&app, &on),
        Duration::from_millis(250),
        "the value landed"
    );
}

/// The reminder is noticed on the loop's own tick, so that tick must come round well
/// before the first reminder falls due: the schedule asks the loop for no extra speed.
#[test]
fn the_loops_slow_tick_comes_round_before_the_first_reminder() {
    let app = gungnir();
    let quiet = Effects::new(true, &EffectsConfig::default());
    let period = tick_period(&app, &quiet);
    assert!(
        period < REMINDERS[0],
        "the loop waits {period:?} and the first reminder is due at {:?}",
        REMINDERS[0]
    );
}

/// A meter's marker falling back onto its reading is something moving, so the loop
/// redraws fast until it lands — and the hue-drift mode is not: the glow takes one step per
/// tick, at whatever pace the loop is already running.
#[test]
fn a_falling_peak_marker_redraws_fast_and_the_hue_drift_does_not() {
    let profile = with_range_on(
        Registry::builtin()
            .expect("the built-in profiles load")
            .get("vidar2")
            .expect("vidar2 is built in")
            .clone(),
        "meter_1",
        (0, 10_000),
    );
    let mut app = App::new(profile, "vidar".into(), &TuiConfig::default(), Vec::new());
    let slow = Duration::from_millis(250);
    let on = Effects::new(true, &EffectsConfig::default());
    // What the loop itself does once the layer has said what this terminal can show, which
    // is what turns the marker on.
    app.set_look(&TuiConfig::default(), &on);
    let t0 = Instant::now();
    let meter = |raw: i64| ValueObject {
        heard_at_ms: None,
        id: "20.1".into(),
        key: Some("meter_1".into()),
        label: "Meter L".into(),
        value_type: "u16".into(),
        raw: json!(raw),
        display: raw.to_string(),
        value: None,
        unit: None,
    };

    app.apply(Update::Status(meter(3_000)), t0);
    app.apply(Update::Status(meter(1_000)), t0);
    assert_eq!(tick_period(&app, &on), FAST_TICK, "the marker is falling");
    app.tick(t0 + Duration::from_millis(1_300));
    assert_eq!(tick_period(&app, &on), slow, "the marker landed");

    let drifting = Effects::new(
        true,
        &EffectsConfig {
            hue_drift: true,
            ..EffectsConfig::default()
        },
    );
    assert_eq!(
        tick_period(&app, &drifting),
        slow,
        "the glow asked the loop to redraw thirty times a second"
    );
}

/// On a DAC the host's capture is the level's source, and a block draws no frame of its
/// own: while the breathing borders or the hue drift follow that level the loop redraws at
/// the meters' own pace, as an amp's readings redraw the same decoration, and back at its
/// slow tick with neither of them on, with no capture running, or on an amp whose meters
/// stream — whose readings draw their own frames.
#[test]
fn the_hosts_level_is_redrawn_at_the_meters_pace_while_the_decoration_follows_it() {
    let slow = Duration::from_millis(250);
    let on = Effects::new(true, &EffectsConfig::default());
    let mut app = gungnir();
    app.set_look(&TuiConfig::default(), &on);
    app.host.linked = true;
    app.host.capturing = true;
    assert!(app.breathing, "the borders do not breathe to begin with");
    assert_eq!(
        tick_period(&app, &on),
        METER_PERIOD,
        "breathing with the host"
    );

    app.host.capturing = false;
    assert_eq!(tick_period(&app, &on), slow, "no capture runs");
    app.host.capturing = true;

    let drift_only = TuiConfig {
        effects: EffectsConfig {
            breathing: false,
            hue_drift: true,
            ..EffectsConfig::default()
        },
        ..TuiConfig::default()
    };
    let drifting = Effects::new(true, &drift_only.effects);
    app.set_look(&drift_only, &drifting);
    assert_eq!(
        tick_period(&app, &drifting),
        METER_PERIOD,
        "drifting with the host"
    );

    let neither = TuiConfig {
        effects: EffectsConfig {
            breathing: false,
            hue_drift: false,
            ..EffectsConfig::default()
        },
        ..TuiConfig::default()
    };
    let still = Effects::new(true, &neither.effects);
    app.set_look(&neither, &still);
    assert_eq!(
        tick_period(&app, &still),
        slow,
        "the title meter alone asked for the meters' pace"
    );

    let mut amp = App::new(
        Registry::builtin()
            .expect("the built-in profiles load")
            .get("fake-amp")
            .expect("fake-amp is built in")
            .clone(),
        "amp".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    amp.set_look(&TuiConfig::default(), &on);
    amp.host.linked = true;
    amp.host.capturing = true;
    amp.apply(
        Update::Status(ValueObject {
            heard_at_ms: None,
            id: "20.1".into(),
            key: None,
            label: "Meter L".into(),
            value_type: "u16".into(),
            raw: json!(8),
            display: "8".into(),
            value: None,
            unit: None,
        }),
        Instant::now(),
    );
    assert_eq!(
        tick_period(&amp, &on),
        slow,
        "an amp's streaming meters asked for the meters' pace as well"
    );
}

/// A tooltip waiting for its delay keeps the loop's tick at a tenth of a second or less, so the
/// box shows on time; once shown the loop ticks at its usual pace.
#[test]
fn a_pending_tooltip_keeps_the_tick_within_a_tenth_of_a_second() {
    use crate::tui::boxes::tooltip::{DELAY, PENDING_TICK, Tooltip};

    let mut app = App::new(
        Registry::generic(0),
        "fake".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let off = Effects::new(false, &EffectsConfig::default());
    assert_eq!(tick_period(&app, &off), app.tick_period());

    let now = Instant::now();
    app.pointer.tooltip = Some(Tooltip {
        hit: crate::tui::mouse::Hit::HelpMark,
        at: (0, 0),
        since: now,
        shown: false,
    });
    assert!(tick_period(&app, &off) <= PENDING_TICK);
    app.tick(now + DELAY);
    assert_eq!(tick_period(&app, &off), app.tick_period());
}
