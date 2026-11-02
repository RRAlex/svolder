// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the app does with readings and keys, by topic. The helpers here are the ones several
//! topics use; the tests are in the topic files listed below.

use super::meters::{LEVEL_ALPHA, STREAMING_FOR};
use super::*;
use crate::backend::Update;
use crate::tui::audio_level::LevelFrom;
use crate::tui::boxes::{device_presets, presets};
use crate::tui::host::{self, HostRow};
use crate::tui::keys;
use crate::tui::screens::settings::{self, Setting};
use crate::tui::test_support::{
    AEGIR_SCALE, animated, feed, meter_reading, press, profile, profile_named, row_of, vo,
    with_log_scale, with_meter_range, with_overcurrent, without_scale,
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde_json::json;
use std::time::Duration;
use svolder_host::{Block, Button};
use svolder_ipc::EffectsConfig;
use svolder_ipc::TuiValue;
use svolder_ipc::ValueObject;
use svolder_ipc::config::{CONFIRM_QUIT, THEME_BACKGROUND, TITLE_METER};
use svolder_profiles::Registry;
use svolder_profiles::meter::{LogScale, Zero, zero_from_floor};
use svolder_profiles::tone::preset::SWITCHED_TO_EQ;
use svolder_profiles::{Control, ControlKind, EqMode, EqParam, EqSpec, Family, Placed};
use svolder_proto::ValueType;

// readings, rows, connection state, the volume and graph series
mod readings;
// the easing of a changed value
mod easing;
// keys on each screen, the help overlay and the key rules
mod key_presses;
// panes, screens, rows, devices and what they remember
mod navigation;
// the tone control and its clicks
mod tone;
// meter peaks, smoothing, channel levels and the level source
mod meters;
// the Host group's rows
mod host_rows;
// flags for the animation layer and the mascot
mod effects;

/// One configured device.
fn row(name: &str) -> DeviceRow {
    DeviceRow {
        name: name.into(),
        ..DeviceRow::default()
    }
}

/// An app on the built-in profile with the default `[tui]` settings and one device.
fn app() -> App {
    App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        vec![row("gungnir")],
    )
}

/// One reading of a function no control names, of a wire type of its own.
fn raw_vo(id: &str, value_type: &str, raw: i64) -> ValueObject {
    ValueObject {
        heard_at_ms: None,
        id: id.into(),
        key: None,
        label: "?".into(),
        value_type: value_type.into(),
        raw: json!(raw),
        display: raw.to_string(),
        value: None,
        unit: None,
    }
}

/// Report one reading, for a case that does not care when it arrived.
fn hear(app: &mut App, value: ValueObject) {
    feed(app, value, Instant::now());
}

/// Type one character.
fn ch(app: &mut App, c: char) -> Vec<Intent> {
    press(app, KeyCode::Char(c))
}

/// Type one shifted character, as a terminal reports it.
fn shift(app: &mut App, c: char) -> Vec<Intent> {
    app.key(KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT))
}

/// One `Set` of `key` to `value`, the form every key press sends.
fn set(key: &str, value: &str) -> Intent {
    Intent::Set {
        key: key.into(),
        value: value.into(),
        step: 1,
    }
}

/// The device answering every band write asked for so far with a reading that leaves the
/// band where it was: what a case that steps from the reading each time needs between
/// presses, since a band key otherwise steps from the value it asked for last.
fn answered(app: &mut App) {
    let pending: Vec<FunctionId> = app.readings.pending.keys().copied().collect();
    for id in pending {
        let raw = app.raw_of(id).expect("a band parameter the case has read");
        for _ in 0..16 {
            if !app.readings.pending.contains_key(&id) {
                break;
            }
            hear(app, raw_vo(&id.to_string(), "u8", raw));
        }
        assert!(
            !app.readings.pending.contains_key(&id),
            "{id} is still pending after its writes were answered"
        );
    }
}

/// Assert one `EqSet` on `band` and `param`, its value within a hundredth.
fn assert_eq_set(intents: &[Intent], band: usize, param: &str, expected: f64) {
    match intents {
        [
            Intent::EqSet {
                band: b,
                param: p,
                value,
            },
        ] => {
            assert_eq!((*b, p.as_str()), (band, param));
            assert!(
                (value - expected).abs() < 0.01,
                "expected {expected}, got {value}"
            );
        }
        other => panic!("expected one EqSet on band {band} {param}, got {other:?}"),
    }
}

/// One volume reading of `raw`, as a backend hands it over.
fn volume(raw: i64) -> ValueObject {
    vo("10", Some("volume"), json!(raw), &raw.to_string())
}

/// One reading as the update feed carries it.
fn status(value: ValueObject) -> Update {
    Update::Status(value)
}

/// An app on the built-in `name` profile, with the default settings and one device.
fn app_on(name: &str) -> App {
    let profile = Registry::builtin()
        .expect("the built-in profiles load")
        .get(name)
        .unwrap_or_else(|| panic!("{name} is built in"))
        .clone();
    App::new(profile, name.into(), &TuiConfig::default(), vec![row(name)])
}

/// One `Meter` control, with a linear range or without one, on its own channel.
fn meter_control(id: FunctionId, channel: &str, range: Option<(i64, i64)>) -> Control {
    Control {
        id,
        key: format!("meter_{channel}"),
        label: format!("Meter {channel}"),
        kind: ControlKind::Meter,
        value_type: ValueType::U16,
        values: Vec::new(),
        range,
        center: None,
        unit: None,
        convert: None,
        channel: Some(channel.to_string()),
        floor: None,
        tick: None,
        log_scale: None,
        advice: None,
        only_while: None,
        auto_off: false,
        output_gain: false,
        mutes_output: false,
        powers_output: false,
        asks_on_change: false,
        reads: Vec::new(),
    }
}

/// A profile no registry names: one meter with a scale and one without, so `channel_readings`
/// can be proven to keep the one with a scale and drop the one without, whatever else is
/// true of either.
fn profile_with_a_scaled_and_an_unscaled_meter() -> Profile {
    Profile {
        name: "hand-built".into(),
        display: "Hand-built".into(),
        product_id: None,
        min_firmware: 0,
        controls: vec![
            meter_control(FunctionId::two(20, 1), "L", Some((0, 1023))),
            meter_control(FunctionId::two(20, 2), "R", None),
        ],
        eq: None,
        family: Family::default(),
        firmware_subsystems: Vec::new(),
        identify: true,
        actions: Vec::new(),
        builtin_settings: None,
    }
}

/// An app on the device list, with the two devices the configuration file names.
fn listing() -> App {
    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        vec![row("gungnir"), row("bifrost")],
    );
    app.devices.configured = vec![
        crate::tui::test_support::configured("gungnir", "0102030405060708"),
        crate::tui::test_support::configured("bifrost", "1112131415161718"),
    ];
    app.show(Screen::Devices);
    app
}

/// The value a series holds for its one sample, read back as its only column.
fn held(app: &App, id: &str, at: Instant) -> Option<f64> {
    let id: FunctionId = id.parse().expect("a function id");
    let series = app.readings.histories.reading(id)?;
    crate::tui::widgets::graph::columns(
        series,
        at + Duration::from_secs(1),
        Duration::from_secs(1),
        1,
    )[0]
}
