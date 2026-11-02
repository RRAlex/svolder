// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Meter readings: peaks and their decay, smoothing, channel levels and the level's source.

use super::*;

/// The Vidar 2's left meter.
const METER_L: FunctionId = FunctionId::two(20, 1);

/// An app on the Vidar 2, whose two meters are the streaming readings a peak is kept of.
fn amp() -> App {
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get("vidar2")
        .expect("vidar2 is built in")
        .clone();
    let mut app = App::new(profile, "vidar".into(), &TuiConfig::default(), Vec::new());
    animated(&mut app);
    app
}

/// The Vidar 2 with its meters scaled `0..=3000`, so a reading `r` stands `r / 3000` of the way
/// up the bar.
fn scaled_amp() -> App {
    let mut app = amp();
    app.profile = with_meter_range(app.profile.clone(), (0, 3_000));
    app
}

/// Whether the marker `at` stands where the fraction `expected` is, within rounding.
fn marker_is(at: Option<f64>, expected: f64) -> bool {
    at.is_some_and(|at| (at - expected).abs() < 1e-9)
}

/// A meter's marker stays at the loudest reading for its hold, falls from there onto the
/// reading behind it, and is gone once it meets it.
#[test]
fn a_peak_holds_and_then_decays_over_a_second() {
    let mut app = scaled_amp();
    let t0 = Instant::now();
    feed(&mut app, meter_reading("20.1", 3_000), t0);
    feed(
        &mut app,
        meter_reading("20.1", 1_000),
        t0 + Duration::from_millis(100),
    );

    assert!(
        marker_is(app.peak(METER_L, t0 + Duration::from_millis(100)), 1.0),
        "the marker left the peak before its hold was up"
    );
    let later = app
        .peak(METER_L, t0 + Duration::from_millis(600))
        .expect("the marker is still falling");
    assert!(
        later < 1.0 && later > 1.0 / 3.0,
        "the marker is neither held nor landed at {later}"
    );
    assert_eq!(
        app.peak(METER_L, t0 + Duration::from_millis(1_200)),
        None,
        "the marker outlasted its fall"
    );
}

/// A marker already falling never rises again without a new peak: a reading that climbs
/// back (but stays under the peak) leaves it falling on the way it was going, or hides it
/// where it has met the reading, and the marker never comes back.
#[test]
fn a_falling_marker_never_rises_without_a_new_peak() {
    let mut app = scaled_amp();
    let ms = Duration::from_millis;
    let t0 = Instant::now();
    feed(&mut app, meter_reading("20.1", 3_000), t0);
    feed(&mut app, meter_reading("20.1", 1_000), t0 + ms(100));
    let before = app
        .peak(METER_L, t0 + ms(800))
        .expect("the marker is falling");
    // The reading climbs back to 1800, under the peak: no new peak.
    feed(&mut app, meter_reading("20.1", 1_800), t0 + ms(800));
    let mut last = Some(before);
    for step in 0..=14u64 {
        let marker = app.peak(METER_L, t0 + ms(800 + step * 50));
        assert!(
            marker <= last,
            "the marker rose from {last:?} to {marker:?} at +{}ms with no new peak",
            800 + step * 50
        );
        last = marker;
    }
    assert_eq!(last, None, "the marker outlasted its fall");
}

/// A reading louder than the marker takes it with it and starts the hold again; a quieter
/// one leaves it where it was.
#[test]
fn a_louder_reading_takes_the_marker_with_it() {
    let mut app = scaled_amp();
    let t0 = Instant::now();
    feed(&mut app, meter_reading("20.1", 1_000), t0);
    feed(
        &mut app,
        meter_reading("20.1", 3_000),
        t0 + Duration::from_secs(1),
    );
    assert_eq!(
        app.peak(METER_L, t0 + Duration::from_secs(1)),
        None,
        "a marker at the reading itself marks nothing the bar does not show"
    );

    // Quieter again, a tenth of a second later: the marker stays where the loud reading
    // put it, and is still held there three hundred milliseconds after that one — long
    // past the hold the first reading of all would have had.
    feed(
        &mut app,
        meter_reading("20.1", 2_000),
        t0 + Duration::from_millis(1_100),
    );
    assert!(
        marker_is(app.peak(METER_L, t0 + Duration::from_millis(1_300)), 1.0),
        "the louder reading did not take the marker with it and start its hold again"
    );
}

/// Only the meters keep a peak: a level a user sets is not a streaming reading, and the
/// switch takes the markers away altogether.
#[test]
fn nothing_but_a_meter_keeps_a_peak_and_the_switch_takes_it_away() {
    let mut app = app();
    let t0 = Instant::now();
    feed(&mut app, vo("10", Some("volume"), json!(109), "109"), t0);
    feed(&mut app, vo("10", Some("volume"), json!(60), "60"), t0);
    assert_eq!(
        app.peak(FunctionId::one(10), t0),
        None,
        "a level control kept a peak"
    );

    let mut off = scaled_amp();
    off.peak_hold = false;
    feed(&mut off, meter_reading("20.1", 3_000), t0);
    feed(&mut off, meter_reading("20.1", 1_000), t0);
    assert_eq!(
        off.peak(METER_L, t0),
        None,
        "a marker was kept with peak hold switched off"
    );
    assert!(!off.peaking(t0), "and nothing is falling to redraw for");
}

/// A falling marker is something moving, which the loop redraws fast for until it lands —
/// and only where there is a bar to draw it on: a meter whose profile names no scale
/// moves nothing on the screen however loud it reads.
#[test]
fn a_falling_marker_is_something_moving() {
    let mut unscaled = amp();
    let t0 = Instant::now();
    assert!(!unscaled.peaking(t0), "nothing has reported yet");
    feed(&mut unscaled, meter_reading("20.1", 3_000), t0);
    feed(&mut unscaled, meter_reading("20.1", 1_000), t0);
    assert_eq!(
        unscaled.peak(METER_L, t0),
        None,
        "a meter with no scale has no bar to place a marker on"
    );
    assert!(
        !unscaled.peaking(t0 + Duration::from_millis(600)),
        "a meter with no bar asked the loop to redraw for a marker it cannot show"
    );

    let mut app = scaled_amp();
    feed(&mut app, meter_reading("20.1", 3_000), t0);
    feed(&mut app, meter_reading("20.1", 1_000), t0);
    assert!(app.peaking(t0 + Duration::from_millis(600)));
    assert!(
        !app.peaking(t0 + Duration::from_millis(1_200)),
        "the marker landed and the loop is still redrawing for it"
    );
}

/// A marker falls evenly along the bar, which on a logarithmic scale is not evenly in the
/// reading: half way through the fall it stands half way between the two readings' fractions.
#[test]
fn the_peak_falls_evenly_along_a_log_bar() {
    let mut app = amp();
    app.profile = with_log_scale(app.profile.clone(), AEGIR_SCALE);
    let placed = app
        .placed(app.profile.control_by_id(METER_L).unwrap())
        .unwrap();
    let t0 = Instant::now();
    feed(&mut app, meter_reading("20.1", 350), t0);
    feed(
        &mut app,
        meter_reading("20.1", 161),
        t0 + Duration::from_millis(1),
    );
    let behind = placed.fraction(161.0);
    assert!(behind > 0.1 && behind < 0.2, "{behind}");
    // 400 ms of hold and then 400 ms of the 800 ms fall.
    let halfway = app.peak(METER_L, t0 + Duration::from_millis(800));
    assert!(marker_is(halfway, (1.0 + behind) / 2.0), "{halfway:?}");
}

/// Meters that are switched off hold no peak and ask for no redraw.
#[test]
fn a_dead_meter_holds_no_peak() {
    let mut app = amp();
    app.profile = with_log_scale(app.profile.clone(), AEGIR_SCALE);
    let t0 = Instant::now();
    feed(&mut app, meter_reading("20.1", 350), t0);
    feed(&mut app, meter_reading("20.1", 161), t0);
    assert!(
        app.peak(METER_L, t0).is_some(),
        "the marker is held while live"
    );
    feed(
        &mut app,
        ValueObject {
            value_type: "bool".into(),
            ..vo("21.2", Some("power_meters"), json!(false), "off")
        },
        t0,
    );
    assert_eq!(app.peak(METER_L, t0), None);
    assert!(!app.peaking(t0));
}

/// A reading that arrives while the meters are dead gives no peak: once they come back, a
/// quieter reading leaves no marker standing at the loud one.
#[test]
fn a_reading_heard_while_the_meters_are_off_leaves_no_marker_behind() {
    let mut app = amp();
    app.profile = with_log_scale(app.profile.clone(), AEGIR_SCALE);
    let t0 = Instant::now();
    let switch = |on: bool| ValueObject {
        value_type: "bool".into(),
        ..vo("21.2", Some("power_meters"), json!(on), "")
    };
    feed(&mut app, switch(false), t0);
    feed(&mut app, meter_reading("20.1", 350), t0);
    feed(&mut app, switch(true), t0);
    feed(&mut app, meter_reading("20.1", 161), t0);
    assert_eq!(
        app.peak(METER_L, t0),
        None,
        "the loud reading heard while the meters were off left a marker"
    );
}

/// The level the borders breathe with follows the meters a step at a time rather than
/// jumping with every reading, and there is none at all until a meter with a scale of its
/// own reports.
#[test]
fn the_meter_level_is_smoothed_across_the_readings() {
    let mut app = amp();
    app.profile = without_scale(app.profile.clone());
    let t0 = Instant::now();
    feed(&mut app, meter_reading("20.1", 3_000), t0);
    assert_eq!(
        app.meters.meter_level, None,
        "a meter with no scale of its own gave a level"
    );

    app.profile = with_meter_range(app.profile.clone(), (0, 3_000));
    feed(&mut app, meter_reading("20.1", 3_000), t0);
    assert_eq!(
        app.meters.meter_level,
        Some(1.0),
        "the first reading is the level itself"
    );

    feed(&mut app, meter_reading("20.1", 0), t0 + METER_PERIOD);
    let level = app.meters.meter_level.expect("the meters have reported");
    assert!(
        level > 0.0 && level < 1.0,
        "a reading at the bottom of the scale took the level straight down to {level}"
    );
    for n in 2..=21 {
        feed(&mut app, meter_reading("20.1", 0), t0 + METER_PERIOD * n);
    }
    assert!(
        app.meters.meter_level.is_some_and(|level| level < 0.01),
        "the level never settled on the readings: {:?}",
        app.meters.meter_level
    );
}

/// The breathing level settles in the same wall time whatever rate the device reports at: the
/// smoothing is a time constant, not a step per reading. A level falling from full scale to
/// silence stands in the same place after one second and after two at 2, 10 and 20 readings
/// a second, and at the pace the meters' ten a second always had.
#[test]
fn the_meter_level_settles_in_the_same_wall_time_at_any_report_rate() {
    let settled = |per_second: u64| {
        let mut app = amp();
        app.profile = with_meter_range(app.profile.clone(), (0, 3_000));
        let t0 = Instant::now();
        feed(&mut app, meter_reading("20.1", 3_000), t0);
        let period = Duration::from_micros(1_000_000 / per_second);
        let mut at_one_second = None;
        let mut at_two_seconds = None;
        for n in 1..=(2 * per_second) {
            feed(
                &mut app,
                meter_reading("20.1", 0),
                t0 + period * u32::try_from(n).unwrap(),
            );
            if n == per_second {
                at_one_second = app.meters.meter_level;
            }
            if n == 2 * per_second {
                at_two_seconds = app.meters.meter_level;
            }
        }
        (
            at_one_second.expect("a level"),
            at_two_seconds.expect("a level"),
        )
    };
    // Today's feel: LEVEL_ALPHA of the way per tenth of a second.
    let tenth = 1.0 - LEVEL_ALPHA;
    let (one, two) = (tenth.powf(10.0), tenth.powf(20.0));
    for per_second in [2, 10, 20] {
        let (at_one, at_two) = settled(per_second);
        assert!(
            (at_one - one).abs() < 1e-6 && (at_two - two).abs() < 1e-6,
            "{per_second}/s: {at_one} and {at_two} after 1 s and 2 s, not {one} and {two}"
        );
    }
}

/// `channel_readings` lists only the meters whose profile gives them a scale, linear or log: the
/// one without a scale reported here is left out entirely, not carried as a `None` entry, and a
/// log scale reads as its own fraction.
#[test]
fn channel_readings_keeps_only_the_meters_with_a_scale() {
    let mut profile = profile_with_a_scaled_and_an_unscaled_meter();
    profile.controls.push(Control {
        log_scale: Some(LogScale {
            zero: Zero::Fixed(160),
            full: 350,
            class_a: 0.25,
        }),
        ..meter_control(FunctionId::two(20, 3), "C", None)
    });
    let mut app = App::new(
        profile,
        "hand-built".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    hear(&mut app, meter_reading("20.1", 1_023));
    hear(&mut app, meter_reading("20.2", 1_023));
    hear(&mut app, meter_reading("20.3", 173));
    let readings = app.channel_readings();
    assert_eq!(
        readings.len(),
        2,
        "the meter without a scale should have been left out: {readings:?}"
    );
    assert_eq!(readings[0].0, "L");
    assert!(
        readings[0].1.is_some(),
        "the meter with a scale has no reading: {readings:?}"
    );
    assert_eq!(readings[1].0, "C");
    let log = readings[1].1.expect("the log-scaled meter has a reading");
    assert!(
        (log - 0.50).abs() < 0.005,
        "raw 173 on the Aegir 2's scale reads half: {log}"
    );
}

/// A single meter with a scale gives one entry, labelled with the channel its profile names, and
/// nothing until it has reported.
#[test]
fn channel_levels_holds_one_entry_for_a_single_meter_profile() {
    let mut app = App::new(
        profile_named("fake-mono"),
        "mono".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    assert_eq!(
        app.meters.channel_levels.clone(),
        None,
        "a meter that has not reported gave a level"
    );
    feed(&mut app, meter_reading("20.1", 1_023), Instant::now());
    assert_eq!(
        app.meters.channel_levels.clone(),
        Some(vec![("M".to_string(), 1.0)]),
        "the fake-mono profile's own channel"
    );
}

/// Two meters with a scale give the facing pair the title draws, `L` then `R` in the
/// profile's own order, each smoothed the way the one level the borders breathe with is.
#[test]
fn channel_levels_keeps_the_left_right_pair_in_profile_order() {
    let mut app = App::new(
        profile_named("fake-amp"),
        "amp".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let t0 = Instant::now();
    feed(&mut app, meter_reading("20.1", 1_023), t0);
    feed(&mut app, meter_reading("20.2", 8), t0);
    let levels = app
        .meters
        .channel_levels
        .clone()
        .expect("both meters have reported");
    let labels: Vec<&str> = levels.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(labels, vec!["L", "R"]);
    assert!(
        (levels[0].1 - 1.0).abs() < 1e-9,
        "the left meter's own reading came out as {}",
        levels[0].1
    );
    assert!(
        levels[1].1.abs() < 1e-9,
        "the right meter's own reading came out as {}",
        levels[1].1
    );

    for n in 1..=20 {
        feed(&mut app, meter_reading("20.1", 8), t0 + METER_PERIOD * n);
    }
    let settled = app
        .meters
        .channel_levels
        .clone()
        .expect("the meters have reported");
    assert!(
        settled[0].1 < 0.01,
        "the left bar never settled on the readings: {settled:?}"
    );
}

/// Five meters with a scale give five entries, `1` through `5`, in the profile's own order — the
/// generalisation from a left/right pair to one level per meter with a scale.
#[test]
fn channel_levels_holds_five_entries_in_profile_order() {
    let mut app = App::new(
        profile_named("fake-five"),
        "five".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let t0 = Instant::now();
    let mut tick = 0;
    for n in 1..=5 {
        for _ in 0..20 {
            tick += 1;
            feed(
                &mut app,
                meter_reading(&format!("20.{n}"), 1_023),
                t0 + METER_PERIOD * tick,
            );
        }
    }
    let levels = app
        .meters
        .channel_levels
        .clone()
        .expect("every meter has reported");
    let labels: Vec<&str> = levels.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(labels, vec!["1", "2", "3", "4", "5"]);
    for (label, level) in &levels {
        assert!(*level > 0.99, "{label} settled at {level}");
    }
}

/// A meter with no scale of its own, as a user's profile may leave one, gives the title and the
/// glow no level however it streams.
#[test]
fn unscaled_meters_stay_out_of_channel_levels_and_the_meter_level() {
    for name in ["tyr", "kraken"] {
        let profile = without_scale(profile_named(name));
        let mut app = App::new(profile, name.into(), &TuiConfig::default(), Vec::new());
        let t0 = Instant::now();
        for control in &app.profile.controls.clone() {
            if control.kind == ControlKind::Meter {
                feed(&mut app, meter_reading(&control.id.to_string(), 900), t0);
            }
        }
        assert_eq!(
            app.meters.channel_levels.clone(),
            None,
            "{name}: an unscaled meter gave the title a level"
        );
        assert_eq!(
            app.meters.meter_level, None,
            "{name}: an unscaled meter gave the glow a level"
        );
    }
}

/// A reading of the meter switch, `on` or off.
fn meters_switch(on: bool) -> ValueObject {
    ValueObject {
        value_type: "bool".into(),
        ..vo("21.2", Some("power_meters"), json!(on), "")
    }
}

/// A log-scaled amp streaming live readings takes its level from the meters; the instant the
/// switch reads off, the meters stop counting as streaming and the level is handed back, not
/// [`STREAMING_FOR`] later (spec 6.5).
#[test]
fn switching_the_meters_off_hands_the_level_back_at_once() {
    let mut app = amp();
    app.profile = with_log_scale(app.profile.clone(), AEGIR_SCALE);
    app.host.linked = true;
    app.host.capturing = true;
    let t0 = Instant::now();
    feed(&mut app, meter_reading("20.1", 300), t0);
    assert!(app.meters_streaming());
    assert_eq!(app.level_from(), LevelFrom::Meters);

    let later = t0 + METER_PERIOD;
    feed(&mut app, meters_switch(false), later);
    assert!(!app.meters_streaming(), "a dead meter still counts");
    assert_eq!(
        app.level_from(),
        LevelFrom::Host,
        "the level was not handed back to the host on the redraw after the switch read off"
    );
    // A live-looking reading while the switch reads off does not start the meters again.
    feed(&mut app, meter_reading("20.1", 320), later);
    assert!(!app.meters_streaming());
    assert_eq!(
        app.channel_readings(),
        vec![("L".into(), None), ("R".into(), None)]
    );

    // The reading that came while the switch read off is not the meters streaming: once the
    // switch reads on, the meters count again only from their next reading.
    let much_later = t0 + STREAMING_FOR * 2;
    feed(&mut app, meter_reading("20.1", 325), much_later);
    feed(&mut app, meters_switch(true), much_later);
    assert!(!app.meters_streaming());
    feed(&mut app, meter_reading("20.1", 330), much_later);
    assert_eq!(app.level_from(), LevelFrom::Meters);
}

/// Each channel's level is read against that channel's own floor: the same raw reading on both
/// channels of a Wotan, whose floors differ, reads a level each channel works out from its own
/// floor and its own full scale (spec 6.4).
#[test]
fn channel_levels_follow_each_channels_own_floor() {
    let profile = profile_named("wotan");
    let mut app = App::new(profile, "wotan".into(), &TuiConfig::default(), Vec::new());
    let (floor_l, floor_r, raw) = (40, 100, 140);
    // The floors: L on 20.4, R on 20.3.
    for (id, floor) in [("20.4", floor_l), ("20.3", floor_r)] {
        feed(&mut app, meter_reading(id, floor), Instant::now());
    }
    feed(&mut app, meter_reading("20.1", raw), Instant::now());
    feed(&mut app, meter_reading("20.2", raw), Instant::now());

    let on = |floor: i64, full: i64| {
        Placed::Log {
            zero: zero_from_floor(Some(floor)),
            full,
            class_a: 0.10,
        }
        .fraction(raw as f64)
    };
    let readings = app.channel_readings();
    let labels: Vec<&str> = readings.iter().map(|(label, _)| label.as_str()).collect();
    assert_eq!(labels, vec!["L", "R"]);
    for ((label, level), expected) in readings.iter().zip([on(floor_l, 160), on(floor_r, 420)]) {
        let level = level.unwrap_or_else(|| panic!("{label} has a reading"));
        assert!(
            (level - expected).abs() < 1e-9,
            "{label} reads {level}, not {expected}"
        );
    }
    // Swapping the floors moves both levels, so a channel reading the other's floor is caught.
    assert!((on(floor_r, 160) - on(floor_l, 160)).abs() > 1e-3);
    assert!((on(floor_r, 420) - on(floor_l, 420)).abs() > 1e-3);
}

/// The level comes from one place at a time: the device's scaled meters while they
/// stream, else the host's capture while host audio shows and the capture runs, else
/// nowhere — a meter silent for ten seconds no longer counts, and a meter with no scale of
/// its own never does.
#[test]
fn the_level_comes_from_the_meters_while_they_stream_else_from_the_host() {
    let mut app = App::new(
        profile_named("fake-amp"),
        "amp".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    app.host.linked = true;
    app.host.capturing = true;
    let t0 = Instant::now();
    assert_eq!(app.level_from(), LevelFrom::Host, "no meter has reported");

    feed(&mut app, meter_reading("20.1", 900), t0);
    assert_eq!(app.level_from(), LevelFrom::Meters, "the meters stream");
    app.tick(t0 + STREAMING_FOR - Duration::from_millis(100));
    assert_eq!(
        app.level_from(),
        LevelFrom::Meters,
        "a gap between readings"
    );
    app.tick(t0 + STREAMING_FOR);
    assert_eq!(
        app.level_from(),
        LevelFrom::Host,
        "meters that stopped still drive the level"
    );

    app.host.capturing = false;
    assert_eq!(
        app.level_from(),
        LevelFrom::Nowhere,
        "no capture runs, yet the host is the source"
    );
    app.host.capturing = true;
    app.tui.host_audio = false;
    assert_eq!(
        app.level_from(),
        LevelFrom::Nowhere,
        "the host is the source with host audio off"
    );
    app.tui.host_audio = true;
    app.host.linked = false;
    assert_eq!(app.level_from(), LevelFrom::Nowhere, "an unlinked host");

    let mut kraken = App::new(
        without_scale(profile_named("kraken")),
        "kraken".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    kraken.host.linked = true;
    kraken.host.capturing = true;
    let now = Instant::now();
    for control in &kraken.profile.controls.clone() {
        if control.kind == ControlKind::Meter {
            feed(
                &mut kraken,
                meter_reading(&control.id.to_string(), 900),
                now,
            );
        }
    }
    assert_eq!(
        kraken.level_from(),
        LevelFrom::Host,
        "meters with no scale of their own took the level from the host"
    );
}

/// A meter with a scale that reports every three seconds keeps the level for as long as it
/// does: the host never takes it between two readings, and so no capture is started and
/// stopped with each of them. Ten seconds of silence hand it over.
#[test]
fn a_meter_reporting_every_three_seconds_never_hands_the_level_to_the_host() {
    let mut app = App::new(
        profile_named("fake-amp"),
        "amp".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    app.host.linked = true;
    app.host.capturing = true;
    let t0 = Instant::now();
    for n in 0..20u32 {
        let at = t0 + Duration::from_secs(3) * n;
        feed(&mut app, meter_reading("20.1", 900), at);
        for gap in [1, 2_000, 2_999] {
            app.tick(at + Duration::from_millis(gap));
            assert_eq!(
                app.level_from(),
                LevelFrom::Meters,
                "handed over {gap} ms after reading {n}"
            );
        }
    }
    let last = t0 + Duration::from_secs(3) * 19;
    app.tick(last + Duration::from_millis(9_999));
    assert_eq!(app.level_from(), LevelFrom::Meters);
    app.tick(last + Duration::from_secs(10));
    assert_eq!(
        app.level_from(),
        LevelFrom::Host,
        "ten seconds of silence kept the meters"
    );
}

/// The values a refresh, a reconnect or a device switch reads the whole device for are
/// not the meters streaming: a snapshot that carries fresh meter values leaves the level
/// where it was, and only a reading the device sends on its feed picks the meters.
#[test]
fn a_snapshot_alone_never_makes_the_meters_the_source() {
    let mut app = App::new(
        profile_named("fake-amp"),
        "amp".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    app.host.linked = true;
    app.host.capturing = true;
    let t0 = Instant::now();
    app.replace_all(
        vec![meter_reading("20.1", 900), meter_reading("20.2", 800)],
        t0,
    );
    assert_eq!(
        app.level_from(),
        LevelFrom::Host,
        "a snapshot picked the meters"
    );
    feed(
        &mut app,
        meter_reading("20.1", 900),
        t0 + Duration::from_secs(1),
    );
    assert_eq!(
        app.level_from(),
        LevelFrom::Meters,
        "a live reading did not"
    );
    app.replace_all(
        vec![meter_reading("20.1", 900)],
        t0 + Duration::from_secs(10),
    );
    assert_eq!(
        app.level_from(),
        LevelFrom::Meters,
        "a snapshot nine seconds on dropped the meters early"
    );
    app.replace_all(
        vec![meter_reading("20.1", 900)],
        t0 + Duration::from_secs(11),
    );
    assert_eq!(
        app.level_from(),
        LevelFrom::Host,
        "a snapshot kept the meters streaming past their silence"
    );
}

/// The glow's level and the title's channels are read from the one source the level comes
/// from, never a mix: the meters' own while they stream, whatever the capture has read, and
/// the capture's left and right once they have stopped; nothing from nowhere.
#[test]
fn the_level_and_the_channels_are_read_from_the_one_source() {
    let mut app = App::new(
        profile_named("fake-amp"),
        "amp".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    app.host.linked = true;
    app.host.capturing = true;
    app.host.level = Some(0.6);
    app.host.channels = Some([0.4, 0.6]);
    let t0 = Instant::now();
    feed(&mut app, meter_reading("20.1", 1_023), t0);
    feed(&mut app, meter_reading("20.2", 8), t0);
    assert_eq!(app.level(), Some(1.0), "the meters stream");
    assert_eq!(app.levels(), Some(vec![1.0, 0.0]), "the meters stream");

    app.tick(t0 + STREAMING_FOR);
    assert_eq!(app.level(), Some(0.6), "the meters have stopped");
    assert_eq!(
        app.levels(),
        Some(vec![0.4, 0.6]),
        "the meters have stopped"
    );

    app.host.capturing = false;
    assert_eq!(app.level(), None, "a level from nowhere");
    assert_eq!(app.levels(), None, "channels from nowhere");
}

/// The capture's levels follow its blocks at the meters' own pace: the first block is its
/// levels themselves, and a block a meter's period after the one before moves them the
/// part of the way a meter reading moves the meters' level — never an easing of their own
/// between blocks.
#[test]
fn the_captures_levels_are_smoothed_across_its_blocks_as_the_meters_are() {
    let mut app = app();
    let t0 = Instant::now();
    let block = |left: f32, right: f32| Block {
        bands: [0.0; svolder_host::fft::BANDS],
        left,
        right,
    };
    app.heard_block(&block(0.2, 0.8), t0);
    assert_eq!(app.host.level, Some(f64::from(0.8f32)));
    assert_eq!(
        app.host.channels,
        Some([f64::from(0.2f32), f64::from(0.8f32)])
    );
    app.heard_block(&block(1.0, f32::NAN), t0 + METER_PERIOD);
    let [left, right] = app.host.channels.expect("the channels");
    let stepped = |held: f64, fresh: f64| held + LEVEL_ALPHA * (fresh - held);
    assert!(
        (left - stepped(f64::from(0.2f32), 1.0)).abs() < 1e-9,
        "{left}"
    );
    assert!(
        (right - stepped(f64::from(0.8f32), 0.0)).abs() < 1e-9,
        "a sample that is not a number read as {right}"
    );
    let level = app.host.level.expect("the level");
    assert!(
        (level - stepped(f64::from(0.8f32), 1.0)).abs() < 1e-9,
        "the level is not the louder channel smoothed: {level}"
    );
    app.tick(t0 + Duration::from_secs(5));
    assert_eq!(
        app.host.level,
        Some(level),
        "the level glided between blocks"
    );
}

/// The capture's levels are smoothed against real time rather than per block, so the
/// breathing borders breathe at the same pace on a DAC as on an amp: after a quarter of a
/// second the level stands in the same place whether that quarter came in twelve blocks or
/// in one, and that place is where the meters' level would be after two and a half
/// readings.
#[test]
fn the_captures_level_after_a_quarter_second_is_the_same_in_twelve_blocks_or_one() {
    let t0 = Instant::now();
    let quarter = Duration::from_millis(250);
    let block = |left: f32, right: f32| Block {
        bands: [0.0; svolder_host::fft::BANDS],
        left,
        right,
    };
    let mut many = app();
    many.heard_block(&block(0.0, 0.0), t0);
    for n in 1..=12u32 {
        many.heard_block(&block(1.0, 0.5), t0 + quarter * n / 12);
    }
    let mut one = app();
    one.heard_block(&block(0.0, 0.0), t0);
    one.heard_block(&block(1.0, 0.5), t0 + quarter);

    let (many_level, one_level) = (
        many.host.level.expect("a level"),
        one.host.level.expect("a level"),
    );
    assert!(
        (many_level - one_level).abs() < 1e-9,
        "twelve blocks came to {many_level}, one to {one_level}"
    );
    let [many_left, many_right] = many.host.channels.expect("the channels");
    let [one_left, one_right] = one.host.channels.expect("the channels");
    assert!((many_left - one_left).abs() < 1e-9 && (many_right - one_right).abs() < 1e-9);
    let meters = 1.0 - (1.0 - LEVEL_ALPHA).powf(2.5);
    assert!(
        (one_level - meters).abs() < 1e-9,
        "{one_level} is not the meters' pace, {meters}"
    );
}
