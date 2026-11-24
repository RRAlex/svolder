// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's bypass and its levels over MQTT, on the computer's Home Assistant device: a
//! `Bypass` switch (ON = bypassed; a `binary_sensor` when commands are off) while the view
//! carries a bypass, and three read-only dB sensors (`Chain level`, `EQ level`, `Space level`)
//! while it carries levels.
//!
//! The levels are the daemon's own numbers (`EqView.levels`), shown to one decimal; nothing here
//! works a level out. A command is a plain `ON` or `OFF`: it goes through the host's `Bypass` op,
//! never through a device writer.

use serde_json::Value as Json;
use svolder_host::eq::{BypassView, ChainLevels, EqView};

use super::{Computer, OFF, ON, envelope};
use crate::discovery::DiscoveryEntry;
use crate::topics;

/// The bypass switch's key.
pub const BYPASS_KEY: &str = "computer_bypass";

/// The whole chain's level sensor key.
pub const CHAIN_LEVEL_KEY: &str = "computer_level_chain";

/// The EQ's level sensor key.
pub const EQ_LEVEL_KEY: &str = "computer_level_eq";

/// The space stage's level sensor key.
pub const SPACE_LEVEL_KEY: &str = "computer_level_space";

/// The level sensors' keys with the names they show, in the order of [`levels_state`].
pub const LEVELS: [(&str, &str); 3] = [
    (CHAIN_LEVEL_KEY, "Chain level"),
    (EQ_LEVEL_KEY, "EQ level"),
    (SPACE_LEVEL_KEY, "Space level"),
];

/// The topic of the Chain level sensor's attributes,
/// `<base>/computer_<host>/computer_level_chain/attributes`.
pub fn attributes_topic(computer: &Computer, base_topic: &str) -> String {
    format!(
        "{base_topic}/{}/{CHAIN_LEVEL_KEY}/attributes",
        computer.unit()
    )
}

/// The discovery topics of the four entities, for `commands`, whatever the view holds.
pub(super) fn topics_of(
    computer: &Computer,
    discovery_prefix: &str,
    commands: bool,
) -> Vec<String> {
    if discovery_prefix.is_empty() {
        return Vec::new();
    }
    let mut all = vec![topics::discovery_config(
        discovery_prefix,
        if commands { "switch" } else { "binary_sensor" },
        computer.unit(),
        BYPASS_KEY,
    )];
    all.extend(LEVELS.iter().map(|(key, _)| {
        topics::discovery_config(discovery_prefix, "sensor", computer.unit(), key)
    }));
    all
}

/// The entries for what `eq` carries: the bypass switch while it has a bypass, the three level
/// sensors while it has levels. An empty `discovery_prefix` returns nothing.
pub(super) fn entries(
    computer: &Computer,
    base_topic: &str,
    discovery_prefix: &str,
    commands: bool,
    eq: &EqView,
) -> Vec<DiscoveryEntry> {
    if discovery_prefix.is_empty() {
        return Vec::new();
    }
    let all = topics_of(computer, discovery_prefix, commands);
    let mut out = Vec::new();
    if eq.bypass.is_some() {
        let mut switch = envelope(computer, base_topic, "Bypass", BYPASS_KEY);
        switch.insert(
            "state_topic".into(),
            topics::diagnostic(base_topic, computer.unit(), BYPASS_KEY).into(),
        );
        switch.insert("payload_on".into(), ON.into());
        switch.insert("payload_off".into(), OFF.into());
        if commands {
            switch.insert(
                "command_topic".into(),
                topics::command(base_topic, computer.unit(), BYPASS_KEY).into(),
            );
        }
        out.push(DiscoveryEntry {
            topic: all[0].clone(),
            payload: Json::Object(switch),
        });
    }
    if eq.levels.is_some() {
        for ((key, name), topic) in LEVELS.iter().zip(&all[1..]) {
            let mut sensor = envelope(computer, base_topic, name, key);
            sensor.insert(
                "state_topic".into(),
                topics::diagnostic(base_topic, computer.unit(), key).into(),
            );
            sensor.insert("unit_of_measurement".into(), "dB".into());
            sensor.insert("state_class".into(), "measurement".into());
            sensor.insert("suggested_display_precision".into(), 1.into());
            // The chain's words travel beside its level, so a card says what the daemon says.
            if *key == CHAIN_LEVEL_KEY {
                sensor.insert(
                    "json_attributes_topic".into(),
                    attributes_topic(computer, base_topic).into(),
                );
            }
            out.push(DiscoveryEntry {
                topic: topic.clone(),
                payload: Json::Object(sensor),
            });
        }
    }
    out
}

/// What the switch's state topic carries: `ON` while the bypass is on.
pub fn state(view: &BypassView) -> &'static str {
    if view.on { ON } else { OFF }
}

/// A level in dB to one decimal, with a plain minus and never `-0.0`.
pub fn level_text(db: f64) -> String {
    let rounded = (db * 10.0).round() / 10.0;
    if rounded == 0.0 || !rounded.is_finite() {
        return "0.0".to_string();
    }
    format!("{rounded:.1}")
}

/// What the three level topics carry, in the order of [`LEVELS`]: the whole chain, the EQ's
/// share, the space stage.
pub fn levels_state(levels: &ChainLevels) -> [String; 3] {
    [
        level_text(levels.total_db),
        level_text(levels.eq_db),
        level_text(levels.space_db),
    ]
}

/// What the Chain level sensor's attributes carry: the daemon's `words` for the levels (empty
/// with no chain) and whether the chain is a `passthrough` and `untouched`.
pub fn levels_attributes(levels: &ChainLevels) -> String {
    serde_json::json!({
        "words": levels.words,
        "passthrough": levels.passthrough,
        "untouched": levels.untouched,
    })
    .to_string()
}
