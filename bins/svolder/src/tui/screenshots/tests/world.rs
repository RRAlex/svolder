// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The world the scenes are drawn in.

use svolder_profiles::convert::raw_to_display;
use svolder_profiles::roles;

use crate::tui::app::App;
use crate::tui::audio_level::LevelFrom;
use crate::tui::boxes::presets;
use crate::tui::screenshots::world::*;
use crate::tui::widgets::graph;

fn key_raw(app: &App, key: &str) -> Option<i64> {
    app.raw_of(app.profile.control_by_key(key).expect(key).id)
}

/// Whether the toggle or readout `key` reads on: a bool is no integer to `raw_of`.
fn reads_on(app: &App, key: &str) -> bool {
    let id = app.profile.control_by_key(key).expect(key).id;
    app.readings
        .values
        .get(&id)
        .is_some_and(crate::tui::app::Reading::is_active)
}

#[test]
fn the_gungnir_reads_as_the_scenes_have_it() {
    runtime().block_on(async {
        let world = World::new(stand_in());
        let app = gungnir(&world, world.base, STILL_VOLUME_DB).await;
        assert_eq!(key_raw(&app, "input"), Some(3));
        assert!(reads_on(&app, "spdif_locked"));
        let volume = roles::volume(&app.profile).expect("a volume");
        let raw = app.raw_of(volume.id).expect("a volume reading");
        let shown = raw_to_display(volume, raw).expect("the volume converts");
        assert!(
            (shown - STILL_VOLUME_DB).abs() < 0.6,
            "the nearest step to -24 dB: {shown}"
        );
        assert!(app.session.connected && app.session.feed_open);
        assert_eq!(app.session.unit.as_deref(), Some(UNIT));
        assert_eq!(app.presets.loaded_preset.as_deref(), Some(PRESET));
        assert_eq!(
            presets::standing(&app).map(|s| format!("{}{}{}", s.lead, s.name, s.mark)),
            Some(format!("Preset {PRESET}"))
        );
        assert!(app.session.passphrase_note.is_none());
    });
}

#[test]
fn the_aegir_is_on_and_its_two_meters_stream_the_signals_two_channels() {
    runtime().block_on(async {
        let world = World::new(stand_in());
        let mut app = aegir2(&world, world.base).await;
        assert!(reads_on(&app, "power"));
        assert!(reads_on(&app, "power_meters"));
        assert!(!reads_on(&app, "warming_up"));
        assert!(!app.meters_streaming(), "no meter reading before the feed");
        feed_meter(&mut app, STILL_SIGNAL_AT, world.base);
        for (key, channel) in [("meter_1", 0), ("meter_2", 1)] {
            let control = app.profile.control_by_key(key).expect(key);
            let placed = app.placed(control).expect("the meter has a scale");
            let expected = placed.raw_at(level(STILL_SIGNAL_AT, channel));
            assert_eq!(key_raw(&app, key), Some(expected), "{key}");
            let (zero, full) = (160, 350);
            assert!((zero..=full).contains(&expected), "{key}: {expected}");
        }
        assert!(app.meters_streaming());
    });
}

#[test]
fn the_host_plays_so_what_at_54_percent_with_its_level_from_the_capture() {
    runtime().block_on(async {
        let world = World::new(stand_in());
        let mut app = gungnir(&world, world.base, STILL_VOLUME_DB).await;
        app.heard_block(&block(STILL_SIGNAL_AT), world.base);
        assert!(app.host_shown());
        assert_eq!(app.level_from(), LevelFrom::Host);
        assert_eq!(app.host.sink.as_ref().map(|s| s.volume), Some(0.54));
        let playing = app.host.playing.as_ref().expect("a player");
        assert_eq!(
            (
                playing.title.as_str(),
                playing.artist.as_str(),
                playing.identity.as_str()
            ),
            ("So What", "Miles Davis", "mpv")
        );
    });
}

#[test]
fn history_fills_the_five_minute_graph() {
    runtime().block_on(async {
        let world = World::new(stand_in());
        let mut app = gungnir(&world, world.base, STILL_VOLUME_DB).await;
        feed_host_history(&mut app, STILL_SIGNAL_AT, world.base);
        let [left, right] = app.readings.histories.host();
        for series in [left, right] {
            let columns = graph::columns(series, world.base, HISTORY, 80);
            assert!(
                columns.first().is_some_and(Option::is_some),
                "the oldest column has a sample"
            );
            assert!(
                columns.last().is_some_and(Option::is_some),
                "the newest column has a sample"
            );
            assert!(
                columns.iter().filter(|c| c.is_some()).count() > 70,
                "the graph is filled"
            );
        }
    });
}

#[test]
fn the_amps_meter_history_fills_the_five_minute_graph() {
    runtime().block_on(async {
        let world = World::new(stand_in());
        let mut app = aegir2(&world, world.base).await;
        feed_meter_history(&mut app, STILL_SIGNAL_AT, world.base);
        for key in ["meter_1", "meter_2"] {
            let id = app.profile.control_by_key(key).expect(key).id;
            let series = app.readings.histories.reading(id).expect("a series");
            let columns = graph::columns(series, world.base, HISTORY, 80);
            assert!(
                columns.first().is_some_and(Option::is_some),
                "{key}: the oldest column has a sample"
            );
            assert!(
                columns.iter().filter(|c| c.is_some()).count() > 70,
                "{key}: the graph is filled"
            );
        }
    });
}

#[test]
fn the_preset_is_made_through_the_import_path() {
    let preset = preset(stand_in());
    let source = preset.source.as_ref().expect("imported");
    assert_eq!(source.origin, Some(measurement().origin()));
    assert!(
        source
            .origin
            .as_deref()
            .is_some_and(|o| o.starts_with("AutoEq · oratory1990 · over-ear"))
    );
    let eq = crate::tui::test_support::eq();
    assert_eq!(
        svolder_profiles::tone::preset::fit_for(&eq, source)
            .bands
            .len(),
        5
    );
}

#[test]
fn the_signal_is_loud_at_every_instant_a_scene_reads() {
    for t in crate::tui::screenshots::scenes::signal_instants() {
        assert!(level(t, 0) > 0.1 && level(t, 1) > 0.1, "{t:?}");
    }
}

#[test]
fn the_wall_clock_is_held() {
    let _world = World::new(stand_in());
    assert_eq!(graph::wall_ms(), WALL_MS);
}

#[test]
fn the_volume_moves_from_the_animations_start_to_the_stills() {
    runtime().block_on(async {
        let world = World::new(stand_in());
        let mut app = gungnir(&world, world.base, START_VOLUME_DB).await;
        let volume = roles::volume(&app.profile).expect("a volume").clone();
        let shown = |app: &App| {
            raw_to_display(&volume, app.raw_of(volume.id).expect("read")).expect("converts")
        };
        // A nearest-step quantiser lands within half the table's gap to its neighbours, which
        // are looked up only where the table has them (it ends at both sides).
        let bound = |app: &App| {
            let raw = app.raw_of(volume.id).expect("read");
            let at = |r: i64| raw_to_display(&volume, r);
            let here = at(raw).expect("converts");
            let gap = [raw - 1, raw + 1]
                .into_iter()
                .filter_map(at)
                .map(|neighbour| (neighbour - here).abs())
                .fold(0.0, f64::max);
            gap / 2.0 + 1e-9
        };
        // The scene constants themselves rise by the 6.0 dB the animation shows
        // (`-30.1 dB` to `-24.1 dB`), which reading back a value just set cannot check.
        assert!(
            (STILL_VOLUME_DB - START_VOLUME_DB - 6.0).abs() < 1e-9,
            "{START_VOLUME_DB} → {STILL_VOLUME_DB}"
        );
        let start = shown(&app);
        assert!((start - START_VOLUME_DB).abs() <= bound(&app));
        set_volume(&mut app, STILL_VOLUME_DB, world.base);
        let still = shown(&app);
        assert!((still - STILL_VOLUME_DB).abs() <= bound(&app));
        // The steps either side of the rise are each within a gap of the constants.
        assert!(
            (still - start - 6.0).abs() <= 2.0 * bound(&app),
            "{start} → {still}"
        );
    });
}

#[test]
fn the_gain_rise_moves_one_band_three_db() {
    runtime().block_on(async {
        let world = World::new(stand_in());
        let mut app = gungnir(&world, world.base, STILL_VOLUME_DB).await;
        let eq = app.profile.eq.clone().expect("a tone control");
        let before: Vec<Option<i64>> = (0..eq.bands.len())
            .map(|b| {
                app.raw_of(
                    eq.band_id(b, svolder_profiles::EqParam::Gain)
                        .expect("a band"),
                )
            })
            .collect();
        let band = rise(&mut app, 3.0, world.base);
        let id = eq
            .band_id(band, svolder_profiles::EqParam::Gain)
            .expect("a band");
        let was_raw = u8::try_from(before[band].expect("read")).expect("a step");
        let was = eq.gain_db(was_raw);
        let now = eq.gain_db(u8::try_from(app.raw_of(id).expect("read")).expect("a step"));
        let step = (eq.gain_db(was_raw + 1) - eq.gain_db(was_raw)).abs();
        assert!((now - was - 3.0).abs() <= step, "{was} → {now}");
        let mark = presets::standing(&app).expect("a standing").mark;
        assert_eq!(mark, "*");
    });
}

#[test]
fn the_snapshot_drops_meters_and_readings_with_no_function_id() {
    use svolder_ipc::ValueObject;
    use svolder_profiles::Registry;
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get("aegir2")
        .expect("built in")
        .clone();
    let reading = |id: &str| ValueObject {
        heard_at_ms: None,
        id: id.into(),
        key: None,
        label: "?".into(),
        value_type: "u8".into(),
        raw: 0.into(),
        display: "0".into(),
        value: None,
        unit: None,
    };
    let meter = profile.control_by_key("meter_1").expect("a meter").id;
    let power = profile.control_by_key("power_meters").expect("a toggle").id;
    assert!(!kept_in_snapshot(&profile, &reading(&meter.to_string())));
    assert!(kept_in_snapshot(&profile, &reading(&power.to_string())));
    assert!(!kept_in_snapshot(&profile, &reading("not an id")));
}
