// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's EQ over MQTT: one ON/OFF switch and one read-only preset sensor, and the
//! crossfeed's switch and preset select ([`crossfeed`]), the virtual speakers' switch and set
//! select ([`speakers`]), the loudness's switch and strength number ([`loudness`]) and the scenes'
//! select and text ([`scenes`]), all under a Home Assistant device of their own, named by the
//! computer's host name. Nothing else of the
//! computer is exposed: no volume, no mute, no points.
//!
//! The entities live under the computer's unit ([`Computer::unit`], `computer_<host>`), so their
//! topics read `<base>/computer_<host>/computer_eq/state` and `.../set`, and
//! `<base>/computer_<host>/computer_eq_preset/state`; two computers on one broker share none.
//! The switch is a `switch` while commands are on and a `binary_sensor` when they are off, as
//! every toggle is. ON turns the EQ on with the points the daemon keeps; OFF removes it and keeps
//! them.

use serde_json::{Map, Value as Json, json};
use svolder_host::eq::{CrossfeedView, EqView, SpeakersView};

use crate::discovery::DiscoveryEntry;
use crate::topics;

pub mod bypass;
pub mod crossfeed;
pub mod loudness;
pub mod path;
pub mod scenes;
pub mod speakers;

/// The unit the computer's topics sat under before they were per computer.
const LEGACY_UNIT: &str = "computer";

/// One computer as Home Assistant sees it: the host name shown as its device name, the id-safe
/// form every topic and id carries, and the unit those are built from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Computer {
    name: String,
    id: String,
    unit: String,
}

impl Computer {
    /// The computer of `host`, a host name as read (shown as is; ids use its
    /// [`topics::host_id`]).
    pub fn new(host: &str) -> Self {
        let id = topics::host_id(host);
        Self {
            name: host.to_string(),
            unit: topics::computer_unit(&id),
            id,
        }
    }

    /// The stand-in for the one computer of the releases before this one, whose topics sat under
    /// the unit `computer`; only for clearing what they announced.
    pub fn legacy() -> Self {
        Self {
            name: String::new(),
            id: String::new(),
            unit: LEGACY_UNIT.to_string(),
        }
    }

    /// The host name as read, the Home Assistant device's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The id-safe host name ([`topics::host_id`]).
    pub fn id(&self) -> &str {
        &self.id
    }

    /// `computer_<host_id>`, the unit the computer's topics and ids sit under.
    pub fn unit(&self) -> &str {
        &self.unit
    }
}

/// The switch's key.
pub const EQ_KEY: &str = "computer_eq";

/// The preset sensor's key.
pub const PRESET_KEY: &str = "computer_eq_preset";

/// The manufacturer the computer's device block names.
const MANUFACTURER: &str = "svolder";

/// The model the computer's device block names.
const MODEL: &str = "Computer";

/// What the switch publishes while the EQ is on.
const ON: &str = "ON";

/// What the switch publishes while the EQ is off.
const OFF: &str = "OFF";

/// What the preset sensor says for a curve no preset stands for.
pub const CUSTOM: &str = svolder_profiles::tone::preset::CUSTOM;

/// Every discovery topic the computer's entities can have for `commands`, whatever the view
/// holds, for clearing what an earlier run announced.
pub fn topics_of(computer: &Computer, discovery_prefix: &str, commands: bool) -> Vec<String> {
    let mut all = earlier_topics_of(computer, discovery_prefix, commands);
    all.extend(bypass::topics_of(computer, discovery_prefix, commands));
    all
}

/// The topics of the entities that existed before the bypass and the levels: what the legacy
/// unit announced.
fn earlier_topics_of(computer: &Computer, discovery_prefix: &str, commands: bool) -> Vec<String> {
    if discovery_prefix.is_empty() {
        return Vec::new();
    }
    let component = if commands { "switch" } else { "binary_sensor" };
    let mut all = vec![
        topics::discovery_config(discovery_prefix, component, computer.unit(), EQ_KEY),
        topics::discovery_config(discovery_prefix, "sensor", computer.unit(), PRESET_KEY),
    ];
    all.extend(crossfeed::topics_of(computer, discovery_prefix, commands));
    all.extend(speakers::topics_of(computer, discovery_prefix, commands));
    all.extend(loudness::topics_of(computer, discovery_prefix, commands));
    all.extend(path::topics_of(computer, discovery_prefix));
    all
}

/// The discovery topics the computer's entities had before they were per computer (unit
/// `computer`), for `commands`: cleared once when a bridge starts, so Home Assistant keeps no
/// `This computer` device beside the named one.
pub fn legacy_topics_of(discovery_prefix: &str, commands: bool) -> Vec<String> {
    earlier_topics_of(&Computer::legacy(), discovery_prefix, commands)
}

/// The retained availability, state and attributes topics the releases before per-computer ids
/// kept under the unit `computer`, for clearing them next to [`legacy_topics_of`].
pub fn legacy_state_topics(base_topic: &str) -> Vec<String> {
    let old = Computer::legacy();
    let unit = old.unit();
    let keys = [
        EQ_KEY,
        PRESET_KEY,
        crossfeed::SWITCH_KEY,
        crossfeed::PRESET_KEY,
        loudness::SWITCH_KEY,
        loudness::STRENGTH_KEY,
        speakers::SWITCH_KEY,
        speakers::SET_KEY,
        path::PATH_KEY,
        path::CONFIG_KEY,
    ];
    let mut all = vec![topics::device_availability(base_topic, unit)];
    all.extend(keys.map(|key| topics::diagnostic(base_topic, unit, key)));
    all.push(speakers::attributes_topic(&old, base_topic));
    all.push(path::path_attributes_topic(&old, base_topic));
    all.push(path::config_attributes_topic(&old, base_topic));
    all
}

/// The computer's discovery entries: the EQ switch (a `binary_sensor` unless `commands`) and
/// preset sensor, and, when the controller serves crossfeed (`crossfeed` is its view), the
/// crossfeed switch and preset select, and, when it serves the virtual speakers (`speakers` is
/// their view), their switch and set select ([`speakers`]). An empty `discovery_prefix` turns
/// discovery off and returns nothing.
pub fn entries(
    computer: &Computer,
    base_topic: &str,
    discovery_prefix: &str,
    commands: bool,
    crossfeed: Option<&CrossfeedView>,
    speakers: Option<&SpeakersView>,
) -> Vec<DiscoveryEntry> {
    if discovery_prefix.is_empty() {
        return Vec::new();
    }
    let component = if commands { "switch" } else { "binary_sensor" };
    let mut switch = envelope(computer, base_topic, "EQ", EQ_KEY);
    switch.insert(
        "state_topic".into(),
        topics::diagnostic(base_topic, computer.unit(), EQ_KEY).into(),
    );
    switch.insert("payload_on".into(), ON.into());
    switch.insert("payload_off".into(), OFF.into());
    if commands {
        switch.insert(
            "command_topic".into(),
            topics::command(base_topic, computer.unit(), EQ_KEY).into(),
        );
    }
    let mut preset = envelope(computer, base_topic, "EQ preset", PRESET_KEY);
    preset.insert(
        "state_topic".into(),
        topics::diagnostic(base_topic, computer.unit(), PRESET_KEY).into(),
    );
    let mut all = vec![
        DiscoveryEntry {
            topic: topics::discovery_config(discovery_prefix, component, computer.unit(), EQ_KEY),
            payload: Json::Object(switch),
        },
        DiscoveryEntry {
            topic: topics::discovery_config(
                discovery_prefix,
                "sensor",
                computer.unit(),
                PRESET_KEY,
            ),
            payload: Json::Object(preset),
        },
    ];
    if let Some(view) = crossfeed {
        all.extend(crossfeed::entries(
            computer,
            base_topic,
            discovery_prefix,
            commands,
            view,
        ));
    }
    if let Some(view) = speakers {
        all.extend(self::speakers::entries(
            computer,
            base_topic,
            discovery_prefix,
            commands,
            view,
        ));
    }
    all
}

/// Every entry the computer announces for `eq`: [`entries`] with the crossfeed's and the
/// speakers' views the controller serves, then, when it serves loudness, the loudness switch and
/// strength number ([`loudness`]), then the path sensor and the config problem sensor ([`path`]),
/// which are read-only whatever `commands` says, then, while the view carries them, the bypass
/// switch and the level sensors ([`bypass`]).
pub fn entries_of(
    computer: &Computer,
    base_topic: &str,
    discovery_prefix: &str,
    commands: bool,
    eq: &EqView,
) -> Vec<DiscoveryEntry> {
    let mut all = entries(
        computer,
        base_topic,
        discovery_prefix,
        commands,
        eq.crossfeed.as_ref(),
        eq.speakers.as_ref(),
    );
    if let Some(view) = eq
        .loudness
        .as_ref()
        .filter(|_| !discovery_prefix.is_empty())
    {
        all.extend(loudness::entries(
            computer,
            base_topic,
            discovery_prefix,
            commands,
            view,
        ));
    }
    all.extend(path::entries(computer, base_topic, discovery_prefix));
    all.extend(bypass::entries(
        computer,
        base_topic,
        discovery_prefix,
        commands,
        eq,
    ));
    all
}

/// The fields every entity of the computer shares.
fn envelope(computer: &Computer, base_topic: &str, name: &str, key: &str) -> Map<String, Json> {
    let mut m = Map::new();
    m.insert("name".into(), name.into());
    m.insert(
        "unique_id".into(),
        topics::unique_id(computer.unit(), key).into(),
    );
    m.insert(
        "availability".into(),
        Json::Array(vec![
            json!({"topic": topics::bridge_availability(base_topic, computer.id())}),
            json!({"topic": topics::device_availability(base_topic, computer.unit())}),
        ]),
    );
    m.insert("availability_mode".into(), "all".into());
    m.insert(
        "device".into(),
        json!({
            "identifiers": [topics::device_id(computer.unit())],
            "name": computer.name(),
            "manufacturer": MANUFACTURER,
            "model": MODEL,
        }),
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
