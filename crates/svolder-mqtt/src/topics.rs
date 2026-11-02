// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The topic names the bridge publishes and subscribes to, and the ids Home Assistant
//! discovery groups entities by. One function per name, so a change to the layout touches
//! this file alone.

/// `<base>/bridge/availability`: `online`/`offline` for the bridge itself, retained; `offline`
/// is the last will.
pub fn bridge_availability(base_topic: &str) -> String {
    format!("{base_topic}/bridge/availability")
}

/// `<base>/<unit_id>/availability`: `online` while the daemon holds the device's link, else
/// `offline`, retained.
pub fn device_availability(base_topic: &str, unit_id: &str) -> String {
    format!("{base_topic}/{unit_id}/availability")
}

/// `<base>/<unit_id>/state`: the retained JSON of every exposed control but the meters.
pub fn state(base_topic: &str, unit_id: &str) -> String {
    format!("{base_topic}/{unit_id}/state")
}

/// `<base>/<unit_id>/meters`: the JSON of the meters, one with a scale as a whole percentage
/// of it (0 while the meters are dead) and any other in its own units, not retained.
pub fn meters(base_topic: &str, unit_id: &str) -> String {
    format!("{base_topic}/{unit_id}/meters")
}

/// `<base>/<unit_id>/<key>/state`: the retained value of one diagnostic entity (the signal
/// strength, a firmware build), a plain text with no JSON around it.
pub fn diagnostic(base_topic: &str, unit_id: &str, key: &str) -> String {
    format!("{base_topic}/{unit_id}/{key}/state")
}

/// `<base>/<unit_id>/signal/attributes`: the retained JSON of the signal sensor's attributes,
/// where it was heard and when.
pub fn signal_attributes(base_topic: &str, unit_id: &str) -> String {
    format!("{base_topic}/{unit_id}/signal/attributes")
}

/// `<base>/<unit_id>/<key>/set`: where a command for one control arrives, subscribed only
/// with `commands = true`.
pub fn command(base_topic: &str, unit_id: &str, key: &str) -> String {
    format!("{base_topic}/{unit_id}/{key}/set")
}

/// `svolder_<unit_id>`: the id a device and every one of its entities share in Home
/// Assistant's `device` block.
pub fn device_id(unit_id: &str) -> String {
    format!("svolder_{unit_id}")
}

/// `svolder_<unit_id>_<key>`: one entity's `unique_id`.
pub fn unique_id(unit_id: &str, key: &str) -> String {
    format!("svolder_{unit_id}_{key}")
}

/// `<prefix>/<component>/svolder_<unit_id>/<key>/config`: one entity's discovery topic.
pub fn discovery_config(
    discovery_prefix: &str,
    component: &str,
    unit_id: &str,
    key: &str,
) -> String {
    format!("{discovery_prefix}/{component}/svolder_{unit_id}/{key}/config")
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "svolder";
    const UNIT: &str = "0102030405060708";

    #[test]
    fn builds_the_availability_topics() {
        assert_eq!(bridge_availability(BASE), "svolder/bridge/availability");
        assert_eq!(
            device_availability(BASE, UNIT),
            "svolder/0102030405060708/availability"
        );
    }

    #[test]
    fn builds_the_state_and_meters_topics() {
        assert_eq!(state(BASE, UNIT), "svolder/0102030405060708/state");
        assert_eq!(meters(BASE, UNIT), "svolder/0102030405060708/meters");
    }

    #[test]
    fn builds_the_diagnostic_topics_under_their_key() {
        assert_eq!(
            diagnostic(BASE, UNIT, "dsp_firmware"),
            "svolder/0102030405060708/dsp_firmware/state"
        );
        assert_eq!(
            signal_attributes(BASE, UNIT),
            "svolder/0102030405060708/signal/attributes"
        );
    }

    #[test]
    fn builds_the_command_topic_under_its_key() {
        assert_eq!(
            command(BASE, UNIT, "volume"),
            "svolder/0102030405060708/volume/set"
        );
    }

    #[test]
    fn builds_the_device_and_unique_ids() {
        assert_eq!(device_id(UNIT), "svolder_0102030405060708");
        assert_eq!(unique_id(UNIT, "volume"), "svolder_0102030405060708_volume");
    }

    #[test]
    fn builds_the_discovery_topic_under_the_prefix_and_component() {
        assert_eq!(
            discovery_config("homeassistant", "number", UNIT, "volume"),
            "homeassistant/number/svolder_0102030405060708/volume/config"
        );
    }

    /// A base topic and a discovery prefix are taken as given, not defaulted here: a blank
    /// `base_topic` or `discovery_prefix` is a caller's choice this module does not second-guess.
    #[test]
    fn takes_the_base_topic_and_prefix_as_given() {
        assert_eq!(bridge_availability(""), "/bridge/availability");
        assert_eq!(
            discovery_config("", "sensor", UNIT, "volume"),
            "/sensor/svolder_0102030405060708/volume/config"
        );
    }
}
