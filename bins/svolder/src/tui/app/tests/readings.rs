// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! How the app takes in readings: rows, the spinner, connection state, the volume and the graph
//! series.

use super::*;

/// The keys of the device screen's rows, in the order they are drawn.
fn row_keys(app: &App) -> Vec<String> {
    app.rows()
        .iter()
        .map(|r| r.control.key.clone())
        .collect::<Vec<_>>()
}

#[test]
fn a_new_app_takes_its_identity_from_the_profile_and_its_drawing_from_the_config() {
    let tui = TuiConfig {
        mouse: false,
        effects: EffectsConfig::off(),
        ..TuiConfig::default()
    };
    let app = App::new(profile(), "gungnir".into(), &tui, vec![row("gungnir")]);
    assert_eq!(app.session.product, "Gungnir 2");
    assert_eq!(app.session.device_name, "gungnir");
    assert_eq!(app.session.unit, None, "nothing has read the unit id yet");
    assert!(!app.mouse, "the config asked for no mouse capture");
    assert!(!app.effects, "the config asked for no animation");

    // Nothing animates before the layer has said what the terminal can show, however much
    // the configuration asks for.
    let wants = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        vec![row("gungnir")],
    );
    assert!(wants.tui.effects.enabled, "the defaults ask for animation");
    assert!(
        !wants.effects && !wants.eased && !wants.breathing && !wants.peak_hold,
        "a new app animates before the layer has said it can"
    );
    assert!(
        app.session.connected,
        "connected until an update says otherwise"
    );
    assert_eq!(app.screen, Screen::Device);
    assert_eq!(app.pane, Pane::Controls);
}

/// The spinner's glyph steps every hundred milliseconds from the moment the app began, and
/// is back where it started a whole cycle later.
#[test]
fn the_spinner_steps_every_hundred_milliseconds() {
    let mut app = app();
    let t0 = app.now;
    assert_eq!(
        app.spinner(),
        '⠋',
        "the spinner did not open on its first glyph"
    );

    app.tick(t0 + Duration::from_millis(100));
    assert_eq!(app.spinner(), '⠙', "the spinner did not step");

    app.tick(t0 + Duration::from_secs(1));
    assert_eq!(
        app.spinner(),
        '⠋',
        "a whole cycle did not bring the spinner back to its first glyph"
    );
}

/// A spinner turns on the header while the link is down and this session's feed could still
/// bring it back, and on the Devices screen while a listed device carries a retry wait —
/// and neither once the link is up, the feed has nothing left to try, or the list holds
/// nothing waiting to be tried again.
#[test]
fn spinning_is_true_while_the_link_is_down_or_a_device_is_retrying() {
    let mut app = app();
    assert!(app.session.feed_open, "a fresh session's feed starts open");
    assert!(!app.spinning(), "nothing is waiting yet");

    // A daemon session's dropped link: its own backoff is still trying, so the feed stays
    // open the whole time and the spinner keeps turning for it.
    app.apply(Update::Connection { connected: false }, app.now);
    assert!(app.spinning(), "the header's own wait did not turn it on");
    app.apply(Update::Connection { connected: true }, app.now);
    assert!(
        !app.spinning(),
        "the spinner outlived the link it was turning for"
    );

    app.screen = Screen::Devices;
    assert!(!app.spinning(), "the list carries no retry wait yet");
    app.devices.rows[0].retry_in_secs = Some(15);
    assert!(
        app.spinning(),
        "the list's own retry wait did not turn it on"
    );

    app.screen = Screen::Device;
    assert!(
        !app.spinning(),
        "a retrying row off the Devices screen still turned it"
    );
}

/// A direct session that has spent its one connect deadline has nothing left trying to
/// reconnect it: the feed says so by closing, and the header stops turning a spinner for a
/// wait that is not under way any more, whatever `connected` still reads.
#[test]
fn spinning_stops_once_the_feed_has_ended_for_good() {
    let mut app = app();
    app.apply(Update::Connection { connected: false }, app.now);
    assert!(app.spinning(), "a merely dropped link did not spin");

    app.session.feed_open = false;
    assert!(
        !app.spinning(),
        "a session with nothing left to read from still turned the spinner"
    );
}

#[test]
fn a_status_update_replaces_the_reading_and_stamps_its_time() {
    let mut app = app();
    let t0 = Instant::now();
    let later = t0 + Duration::from_millis(500);
    feed(&mut app, vo("10", Some("volume"), json!(100), "100"), t0);
    feed(&mut app, vo("10", Some("volume"), json!(109), "109"), later);
    let reading = app
        .readings
        .values
        .get(&FunctionId::one(10))
        .expect("the volume reading");
    assert_eq!(reading.value.display, "109");
    assert_eq!(reading.received, later);
    assert_eq!(
        app.readings.values.len(),
        1,
        "the second reading replaced the first"
    );
}

#[test]
fn an_unparsable_id_is_ignored() {
    let mut app = app();
    hear(&mut app, vo("nonsense", None, json!(1), "1"));
    assert!(
        app.readings.values.is_empty(),
        "nothing to key the reading by"
    );
}

#[test]
fn rows_follow_function_id_order_and_skip_faults_and_readouts() {
    let mut app = app();
    assert_eq!(
        row_keys(&app),
        ["input", "phase", "nos", "mute", "volume", "balance"]
    );
    hear(&mut app, vo("10", Some("volume"), json!(109), "109"));
    let rows = app.rows();
    assert_eq!(
        rows[row_of(&app, "volume")]
            .reading
            .map(|r| r.value.display.as_str()),
        Some("109"),
        "a row carries the last value its control confirmed"
    );
    assert!(
        rows[row_of(&app, "input")].reading.is_none(),
        "and nothing at all until one arrives"
    );
}

/// The step a failure named is what to do while it stands. A link that came back has
/// answered it, so the advice line stops offering it.
#[test]
fn a_link_that_comes_back_takes_the_step_it_needed_off_the_advice_line() {
    let mut app = app();
    let now = Instant::now();
    app.apply(Update::Connection { connected: false }, now);
    app.notify_failure(&crate::recovery::Failure::of(
        crate::recovery::Kind::Dropped,
        "device did not come back",
    ));
    assert!(app.notices.advice.is_some(), "the step was not kept");
    app.apply(Update::Connection { connected: true }, now);
    assert_eq!(
        app.notices.advice, None,
        "the advice line still offers a step a live link has answered"
    );
}

#[test]
fn connection_updates_flip_connected_and_mark_readings_stale_in_rows() {
    let mut app = app();
    hear(&mut app, vo("10", Some("volume"), json!(109), "109"));
    assert!(app.rows().iter().all(|r| !r.stale));
    app.apply(Update::Connection { connected: false }, Instant::now());
    assert!(!app.session.connected);
    assert!(
        app.rows().iter().all(|r| r.stale),
        "every value shown is now last-known"
    );
    app.apply(Update::Connection { connected: true }, Instant::now());
    assert!(app.session.connected);
    assert!(app.rows().iter().all(|r| !r.stale));
}

#[test]
fn rate_estimate_counts_updates_per_second() {
    let (mut steady, mut once) = (app(), app());
    let t0 = Instant::now();
    let volume = FunctionId::one(10);
    let reading = || vo("10", Some("volume"), json!(100), "100");
    for n in 0..10 {
        feed(&mut steady, reading(), t0 + Duration::from_millis(100 * n));
    }
    let last = t0 + Duration::from_millis(900);
    steady.tick(last);
    let rate = steady.rate_per_s(volume).expect("two readings make a rate");
    assert!((rate - 10.0).abs() < 0.5, "expected about 10/s, got {rate}");
    steady.tick(last + Duration::from_secs(5));
    let quiet = steady
        .rate_per_s(volume)
        .expect("the rate decays, it does not go");
    assert!(
        quiet > 0.0 && quiet < 1.0,
        "expected a fallen rate, got {quiet}"
    );

    feed(&mut once, vo("11", Some("balance"), json!(64), "64"), t0);
    once.tick(t0);
    assert!(
        once.rate_per_s(FunctionId::one(11)).is_none(),
        "one reading is not a rate"
    );
}

/// A meter and a level control keep the loudest value of a 250 ms bin; every other numeric
/// reading, an unprofiled id included, keeps the latest.
#[test]
fn readings_feed_their_series_with_the_aggregate_of_their_control() {
    let mut profile = profile_with_a_scaled_and_an_unscaled_meter();
    profile.controls.push(Control {
        kind: ControlKind::Level,
        ..meter_control(FunctionId::two(20, 3), "V", Some((0, 100)))
    });
    let mut app = App::new(
        profile,
        "hand-built".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    let t0 = Instant::now();
    for (id, first, second) in [("20.1", 900, 100), ("20.3", 90, 10), ("99.9", 900, 100)] {
        feed(&mut app, meter_reading(id, first), t0);
        feed(
            &mut app,
            meter_reading(id, second),
            t0 + Duration::from_millis(100),
        );
    }
    assert_eq!(
        held(&app, "20.1", t0),
        Some(900.0),
        "a meter keeps its peak"
    );
    assert_eq!(held(&app, "20.3", t0), Some(90.0), "a level keeps its peak");
    assert_eq!(
        held(&app, "99.9", t0),
        Some(100.0),
        "the rest keep the last"
    );
}

/// A bool reads as 0 or 1 and a string has no series at all.
#[test]
fn bools_are_recorded_as_zero_and_one_and_strings_not_at_all() {
    let mut app = app();
    let t0 = Instant::now();
    feed(&mut app, vo("240.1", None, json!(true), "true"), t0);
    feed(&mut app, vo("240.2", None, json!("hello"), "hello"), t0);
    assert_eq!(held(&app, "240.1", t0), Some(1.0));
    assert!(
        app.readings
            .histories
            .reading("240.2".parse().unwrap())
            .is_none()
    );
}

/// The host's two channels are fed from each block, the larger of a bin's samples kept.
#[test]
fn host_blocks_feed_the_left_and_right_series() {
    let mut app = app();
    let t0 = Instant::now();
    for (left, right) in [(0.2, 0.9), (0.7, 0.1)] {
        app.heard_block(
            &Block {
                bands: [0.0; svolder_host::fft::BANDS],
                left,
                right,
            },
            t0,
        );
    }
    let [left, right] = app.readings.histories.host();
    let at = |series: &crate::tui::widgets::graph::Series| {
        crate::tui::widgets::graph::columns(
            series,
            t0 + Duration::from_secs(1),
            Duration::from_secs(1),
            1,
        )[0]
    };
    assert_eq!(at(left), Some(f64::from(0.7f32)));
    assert_eq!(at(right), Some(f64::from(0.9f32)));
}

/// The app finds a device's volume control exactly where `roles::volume` does, on every
/// built-in profile.
#[test]
fn the_app_finds_the_volume_where_the_policy_does() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let mut with_volume = 0;
    for name in registry.names() {
        let profile = registry.get(name).expect("a built-in profile");
        let by_policy = svolder_profiles::roles::volume(profile).map(|c| c.key.as_str());
        assert_eq!(
            app_on(name).volume().map(|c| c.key.as_str()),
            by_policy,
            "{name}"
        );
        with_volume += usize::from(by_policy.is_some());
    }
    assert!(with_volume > 0, "no built-in profile has a volume");
}

/// The cursor's length counts the same control rows the screen lists, without building them.
#[test]
fn the_control_row_count_matches_the_rows_the_screen_lists() {
    let app = app();
    let rows = app.rows().len();
    assert!(rows > 0);
    assert_eq!(
        app.controls_len(),
        rows + usize::from(app.host_shown()) * app.host_rows().len()
    );
}
