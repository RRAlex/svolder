// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! svolder's MQTT bridge to Home Assistant: topic names, discovery configs, the state and meter
//! payloads and command parsing, all pure functions, and in [`client`] the one piece that does
//! I/O, the connection to the broker. `svolderd` feeds them its devices.
//!
//! Nothing here ever names a control the firmware-update family owns: see
//! [`discovery::exposed_controls`] for what a profile exposes, and [`eq`] for the tone
//! control's preset, loudness strength and bands.
//!
//! - [`client`]: the connection to the broker.
//! - [`command`]: turning one MQTT command payload into the message the set path sends.
//! - [`computer`]: the computer's EQ switch and preset sensor.
//! - [`device_presets`]: the preset select of a device with slots and built-in settings.
//! - [`discovery`]: Home Assistant discovery: one retained config per exposed control.
//! - [`eq`]: the tone control over MQTT beyond its mode.
//! - [`payload`]: the JSON published on the state and meters topics.
//! - [`topics`]: the topic names the bridge publishes and subscribes to.

#![warn(missing_docs)]

pub mod client;
pub mod command;
pub mod computer;
pub mod device_presets;
pub mod discovery;
pub mod eq;
pub mod payload;
#[cfg(test)]
pub(crate) mod test_support;
pub mod topics;

pub use client::{Client, ClientEvent, Link, Settings};
pub use command::{ParsedCommand, parse as parse_command};
pub use discovery::{
    CLEARING_PAYLOAD, Device, DiscoveryEntry, EQ_MODE_KEY, configs as discovery_configs,
    exposed_controls,
};

/// An `f64` as a JSON number, or `null` for one JSON cannot carry (`NaN`, `±infinity`) — the
/// digital volume table's silence point among them.
pub(crate) fn json_number(v: f64) -> serde_json::Value {
    serde_json::Number::from_f64(v)
        .map(serde_json::Value::Number)
        .unwrap_or(serde_json::Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_number_carries_a_finite_value_and_nulls_a_non_finite_one() {
        assert_eq!(json_number(-6.021), serde_json::json!(-6.021));
        assert_eq!(json_number(f64::NEG_INFINITY), serde_json::Value::Null);
        assert_eq!(json_number(f64::NAN), serde_json::Value::Null);
    }
}
