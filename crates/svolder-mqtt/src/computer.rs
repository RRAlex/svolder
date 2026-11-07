// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's EQ over MQTT: one ON/OFF switch and one read-only preset sensor, both under a
//! Home Assistant device of their own, `This computer`. Nothing else of the computer is
//! exposed: no volume, no mute, no points.
//!
//! The entities live under the bridge-level unit [`UNIT`], so their topics read
//! `<base>/computer/computer_eq/state` and `.../set`, and `<base>/computer/computer_eq_preset/state`.
//! The switch is a `switch` while commands are on and a `binary_sensor` when they are off, as
//! every toggle is. ON turns the EQ on with the points the daemon keeps; OFF removes it and keeps
//! them.

use serde_json::{Map, Value as Json, json};
use svolder_host::eq::EqView;

use crate::discovery::DiscoveryEntry;
use crate::topics;

/// The bridge-level unit the computer's topics sit under, in place of a device's unit id.
pub const UNIT: &str = "computer";

/// The switch's key.
pub const EQ_KEY: &str = "computer_eq";

/// The preset sensor's key.
pub const PRESET_KEY: &str = "computer_eq_preset";

/// The name the Home Assistant device carries.
const NAME: &str = "This computer";

/// What the switch publishes while the EQ is on.
const ON: &str = "ON";

/// What the switch publishes while the EQ is off.
const OFF: &str = "OFF";

/// What the preset sensor says for a curve no preset stands for.
pub const CUSTOM: &str = svolder_profiles::tone::preset::CUSTOM;

/// The computer's discovery entries: the switch (a `binary_sensor` unless `commands`) and the
/// preset sensor. An empty `discovery_prefix` turns discovery off and returns nothing.
pub fn entries(base_topic: &str, discovery_prefix: &str, commands: bool) -> Vec<DiscoveryEntry> {
    if discovery_prefix.is_empty() {
        return Vec::new();
    }
    let component = if commands { "switch" } else { "binary_sensor" };
    let mut switch = envelope(base_topic, "EQ", EQ_KEY);
    switch.insert(
        "state_topic".into(),
        topics::diagnostic(base_topic, UNIT, EQ_KEY).into(),
    );
    switch.insert("payload_on".into(), ON.into());
    switch.insert("payload_off".into(), OFF.into());
    if commands {
        switch.insert(
            "command_topic".into(),
            topics::command(base_topic, UNIT, EQ_KEY).into(),
        );
    }
    let mut preset = envelope(base_topic, "EQ preset", PRESET_KEY);
    preset.insert(
        "state_topic".into(),
        topics::diagnostic(base_topic, UNIT, PRESET_KEY).into(),
    );
    vec![
        DiscoveryEntry {
            topic: topics::discovery_config(discovery_prefix, component, UNIT, EQ_KEY),
            payload: Json::Object(switch),
        },
        DiscoveryEntry {
            topic: topics::discovery_config(discovery_prefix, "sensor", UNIT, PRESET_KEY),
            payload: Json::Object(preset),
        },
    ]
}

/// The fields every entity of the computer shares.
fn envelope(base_topic: &str, name: &str, key: &str) -> Map<String, Json> {
    let mut m = Map::new();
    m.insert("name".into(), name.into());
    m.insert("unique_id".into(), topics::unique_id(UNIT, key).into());
    m.insert(
        "availability".into(),
        Json::Array(vec![
            json!({"topic": topics::bridge_availability(base_topic)}),
            json!({"topic": topics::device_availability(base_topic, UNIT)}),
        ]),
    );
    m.insert("availability_mode".into(), "all".into());
    m.insert(
        "device".into(),
        json!({"identifiers": [topics::device_id(UNIT)], "name": NAME}),
    );
    m.insert(
        "origin".into(),
        json!({"name": "svolder", "sw_version": env!("CARGO_PKG_VERSION")}),
    );
    m
}

/// What the two state topics carry for `view`: the switch's `ON` or `OFF`, and the preset's
/// name, [`CUSTOM`] for points no preset stands for, and empty while the EQ is off or there is
/// no view. A view whose chain could not be verified reads as the saved state says.
pub fn state(view: Option<&EqView>) -> (&'static str, String) {
    match view {
        Some(eq) if eq.on => (ON, eq.preset.clone().unwrap_or_else(|| CUSTOM.to_string())),
        _ => (OFF, String::new()),
    }
}

/// Whether the daemon keeps nothing to turn on: no point has a gain and no preset was applied or
/// edited, so the points are the lone flat one a fresh state starts with.
pub fn nothing_kept(view: &EqView) -> bool {
    view.preset.is_none() && view.points.iter().all(|point| point.g == 0.0)
}

/// Parse a command payload: `ON` is `true`, `OFF` is `false`, in any case, blanks aside.
pub fn parse(body: &str) -> Option<bool> {
    let body = body.trim();
    if body.eq_ignore_ascii_case(ON) {
        Some(true)
    } else if body.eq_ignore_ascii_case(OFF) {
        Some(false)
    } else {
        None
    }
}

#[cfg(test)]
mod tests;
