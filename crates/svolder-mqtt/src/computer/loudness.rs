// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's loudness over MQTT: an ON/OFF switch and a strength number (0-100 %, a
//! read-only sensor when commands are off), on the computer's Home Assistant device beside the
//! crossfeed's entities.
//!
//! The number's bounds and step come from the view's ranges, never from a copy here. The
//! reference, the volume and the readout are no entities: the strength and the switch are the
//! whole surface. A command that is not a number is dropped; a number reaches the controller, which
//! snaps it to its step and holds it to its range (250 becomes 100) and refuses only a number
//! that is not finite.

use serde_json::{Value as Json, json};
use svolder_host::eq::LoudnessView;

use super::{Computer, OFF, ON, envelope};
use crate::discovery::DiscoveryEntry;
use crate::topics;

/// The loudness switch's key.
pub const SWITCH_KEY: &str = "computer_loudness";

/// The strength entity's key.
pub const STRENGTH_KEY: &str = "computer_loudness_strength";

/// The discovery topics of the two entities, for `commands`, whatever the view holds.
pub(super) fn topics_of(
    computer: &Computer,
    discovery_prefix: &str,
    commands: bool,
) -> Vec<String> {
    if discovery_prefix.is_empty() {
        return Vec::new();
    }
    vec![
        topics::discovery_config(
            discovery_prefix,
            if commands { "switch" } else { "binary_sensor" },
            computer.unit(),
            SWITCH_KEY,
        ),
        topics::discovery_config(
            discovery_prefix,
            if commands { "number" } else { "sensor" },
            computer.unit(),
            STRENGTH_KEY,
        ),
    ]
}

/// The two entries, for a computer whose controller serves loudness (`view` is its view).
pub(super) fn entries(
    computer: &Computer,
    base_topic: &str,
    discovery_prefix: &str,
    commands: bool,
    view: &LoudnessView,
) -> Vec<DiscoveryEntry> {
    let mut switch = envelope(computer, base_topic, "Loudness", SWITCH_KEY);
    switch.insert(
        "state_topic".into(),
        topics::diagnostic(base_topic, computer.unit(), SWITCH_KEY).into(),
    );
    switch.insert("payload_on".into(), ON.into());
    switch.insert("payload_off".into(), OFF.into());
    let mut strength = envelope(computer, base_topic, "Loudness strength", STRENGTH_KEY);
    strength.insert(
        "state_topic".into(),
        topics::diagnostic(base_topic, computer.unit(), STRENGTH_KEY).into(),
    );
    strength.insert("unit_of_measurement".into(), "%".into());
    if commands {
        switch.insert(
            "command_topic".into(),
            topics::command(base_topic, computer.unit(), SWITCH_KEY).into(),
        );
        strength.insert(
            "command_topic".into(),
            topics::command(base_topic, computer.unit(), STRENGTH_KEY).into(),
        );
        let range = view.ranges.strength_pct;
        strength.insert("min".into(), number(range.min));
        strength.insert("max".into(), number(range.max));
        strength.insert("step".into(), number(range.step));
        strength.insert("mode".into(), "slider".into());
    }
    let topics = topics_of(computer, discovery_prefix, commands);
    vec![
        DiscoveryEntry {
            topic: topics[0].clone(),
            payload: Json::Object(switch),
        },
        DiscoveryEntry {
            topic: topics[1].clone(),
            payload: Json::Object(strength),
        },
    ]
}

/// A whole number as an integer, else a float, as Home Assistant reads bounds.
fn number(value: f64) -> Json {
    if value.fract() == 0.0 && value.abs() < 1e9 {
        json!(value as i64)
    } else {
        json!(value)
    }
}

/// What the two state topics carry: the switch's `ON` or `OFF`, and the strength in percent.
/// The strength is told while loudness is off too, as the setting it will play when turned on.
pub fn state(view: &LoudnessView) -> (&'static str, String) {
    let on = if view.on { ON } else { OFF };
    (on, number(view.strength_pct).to_string())
}

/// Parse a command on the strength number: a finite number in percent, blanks aside.
pub fn parse_strength(body: &str) -> Option<f64> {
    body.trim().parse::<f64>().ok().filter(|v| v.is_finite())
}
