// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::test_support::{device, profile};
use svolder_profiles::Registry;

#[test]
fn an_empty_discovery_prefix_publishes_nothing() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("gungnir2-v2 exists");
    let entries = configs(
        "svolder",
        "",
        profile,
        &device("0102030405060708"),
        false,
        &[],
    );
    assert!(entries.is_empty());
}

#[test]
fn every_topic_sits_under_the_discovery_prefix_and_names_its_component() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("gungnir2-v2 exists");
    let entries = configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        true,
        &[],
    );
    let volume = entries
        .iter()
        .find(|e| e.topic.ends_with("/volume/config"))
        .expect("a volume entry");
    assert_eq!(
        volume.topic,
        "homeassistant/number/svolder_0102030405060708/volume/config"
    );
    let mute = entries
        .iter()
        .find(|e| e.topic.ends_with("/mute/config"))
        .expect("a mute entry");
    assert_eq!(
        mute.topic,
        "homeassistant/switch/svolder_0102030405060708/mute/config"
    );
    let input = entries
        .iter()
        .find(|e| e.topic.ends_with("/input/config"))
        .expect("an input entry");
    assert_eq!(
        input.topic,
        "homeassistant/select/svolder_0102030405060708/input/config"
    );
}

#[test]
fn a_level_becomes_a_number_with_commands_and_a_sensor_without() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("gungnir2-v2 exists");
    let with_commands = configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        true,
        &[],
    );
    let volume = with_commands
        .iter()
        .find(|e| e.topic.contains("/volume/"))
        .expect("a volume entry");
    assert!(volume.topic.starts_with("homeassistant/number/"));
    assert_eq!(
        volume.payload["command_topic"],
        "svolder/0102030405060708/volume/set"
    );
    assert_eq!(volume.payload["mode"], "slider");
    assert_eq!(volume.payload["unit_of_measurement"], "dB");
    assert_eq!(volume.payload["min"], -96.33);
    assert_eq!(volume.payload["max"], 0.0);
    assert_eq!(volume.payload["step"], 0.334);

    let read_only = configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        false,
        &[],
    );
    let volume = read_only
        .iter()
        .find(|e| e.topic.contains("/volume/"))
        .expect("a volume entry");
    assert!(volume.topic.starts_with("homeassistant/sensor/"));
    assert!(volume.payload.get("command_topic").is_none());
}

#[test]
fn a_level_with_no_conversion_uses_its_raw_range() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("gungnir2-v2 exists");
    let entries = configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        true,
        &[],
    );
    let balance = entries
        .iter()
        .find(|e| e.topic.contains("/balance/"))
        .expect("a balance entry");
    assert_eq!(balance.payload["min"], -63.0);
    assert_eq!(balance.payload["max"], 63.0);
    assert_eq!(balance.payload["step"], 1.0);
    assert!(balance.payload.get("unit_of_measurement").is_none());
}

#[test]
fn a_toggle_carries_on_off_payloads_both_ways() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("gungnir2-v2 exists");
    let entries = configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        false,
        &[],
    );
    let mute = entries
        .iter()
        .find(|e| e.topic.contains("/mute/"))
        .expect("a mute entry");
    assert!(mute.topic.starts_with("homeassistant/binary_sensor/"));
    assert_eq!(mute.payload["payload_on"], "ON");
    assert_eq!(mute.payload["payload_off"], "OFF");
    assert!(mute.payload.get("command_topic").is_none());
}

#[test]
fn a_select_lists_its_options_only_with_commands() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("gungnir2-v2 exists");
    let with_commands = configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        true,
        &[],
    );
    let input = with_commands
        .iter()
        .find(|e| e.topic.contains("/input/"))
        .expect("an input entry");
    assert_eq!(
        input.payload["options"],
        json!(["USB 1", "USB 2", "Optical", "Coaxial", "AES"])
    );
    let read_only = configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        false,
        &[],
    );
    let input = read_only
        .iter()
        .find(|e| e.topic.contains("/input/"))
        .expect("an input entry");
    assert!(input.payload.get("options").is_none());
}

#[test]
fn the_tone_control_mode_is_exposed_as_a_select_with_its_three_names() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("gungnir2-v2 exists");
    let entries = configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        true,
        &[],
    );
    let mode = entries
        .iter()
        .find(|e| e.topic.contains("/eq_mode/"))
        .expect("an eq_mode entry");
    assert!(mode.topic.starts_with("homeassistant/select/"));
    assert_eq!(mode.payload["options"], json!(["flat", "eq", "loudness"]));
    assert_eq!(
        mode.payload["command_topic"],
        "svolder/0102030405060708/eq_mode/set"
    );
}

#[test]
fn a_profile_with_no_eq_exposes_no_mode() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("fake-amp").expect("fake-amp exists");
    let entries = configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        true,
        &[],
    );
    assert!(!entries.iter().any(|e| e.topic.contains("/eq_mode/")));
}

#[test]
fn a_meter_with_a_scale_is_always_a_measurement_sensor_on_the_meters_topic() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("fake-amp").expect("fake-amp exists");
    for commands in [false, true] {
        let entries = configs(
            "svolder",
            "homeassistant",
            profile,
            &device("0102030405060708"),
            commands,
            &[],
        );
        let meter = entries
            .iter()
            .find(|e| e.topic.contains("/meter_1/"))
            .expect("a meter_1 entry");
        assert!(meter.topic.starts_with("homeassistant/sensor/"));
        assert_eq!(meter.payload["state_class"], "measurement");
        assert_eq!(
            meter.payload["state_topic"],
            "svolder/0102030405060708/meters"
        );
        assert!(meter.payload.get("command_topic").is_none());
    }
}

#[test]
fn a_numeric_readout_is_a_sensor() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("gungnir2-v2 exists");
    let entries = configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        true,
        &[],
    );
    let sample_rate = entries
        .iter()
        .find(|e| e.topic.contains("/sample_rate/"))
        .expect("a sample_rate entry");
    assert!(sample_rate.topic.starts_with("homeassistant/sensor/"));
    assert!(sample_rate.payload.get("command_topic").is_none());
}

#[test]
fn a_boolean_readout_is_a_binary_sensor_with_on_off_payloads() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("gungnir2-v2 exists");
    let entries = configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        true,
        &[],
    );
    let locked = entries
        .iter()
        .find(|e| e.topic.contains("/spdif_locked/"))
        .expect("a spdif_locked entry");
    assert!(locked.topic.starts_with("homeassistant/binary_sensor/"));
    assert_eq!(locked.payload["payload_on"], "ON");
    assert_eq!(locked.payload["payload_off"], "OFF");
    assert!(locked.payload.get("device_class").is_none());
    assert!(locked.payload.get("command_topic").is_none());
}

#[test]
fn a_fault_is_a_problem_binary_sensor() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("aegir2").expect("aegir2 exists");
    let entries = configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        true,
        &[],
    );
    let fault = entries
        .iter()
        .find(|e| e.topic.contains("/fault_overcurrent/"))
        .expect("a fault entry");
    assert!(fault.topic.starts_with("homeassistant/binary_sensor/"));
    assert_eq!(fault.payload["device_class"], "problem");
}

#[test]
fn nothing_in_the_firmware_family_is_ever_exposed() {
    let mut control = eq_mode_control(&sample_eq());
    control.id = svolder_proto::FunctionId::two(210, 5);
    let mut profile = profile("gungnir2-v2");
    profile.controls.push(control);
    let exposed = exposed_controls(&profile);
    assert!(
        !exposed
            .iter()
            .any(|c| c.id.class() == FunctionClass::Restricted)
    );
}

#[test]
fn the_passphrase_function_is_never_exposed_either() {
    let mut control = eq_mode_control(&sample_eq());
    control.id = svolder_proto::function::PASSPHRASE;
    control.key = "not_a_real_control".to_string();
    let mut profile = profile("gungnir2-v2");
    assert_eq!(control.id.class(), FunctionClass::Sensitive);
    profile.controls.push(control);
    let exposed = exposed_controls(&profile);
    assert!(!exposed.iter().any(|c| c.key == "not_a_real_control"));
    assert!(
        !exposed
            .iter()
            .any(|c| c.id.class() == FunctionClass::Sensitive)
    );
}

fn sample_eq() -> EqSpec {
    profile("gungnir2-v2")
        .eq
        .expect("gungnir2-v2 has a tone control")
}

#[test]
fn the_link_entity_depends_only_on_the_bridge_availability() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("gungnir2-v2 exists");
    let entries = configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        false,
        &[],
    );
    let link = entries
        .iter()
        .find(|e| e.topic.contains("/link/"))
        .expect("a link entry");
    assert!(link.topic.starts_with("homeassistant/binary_sensor/"));
    assert_eq!(
        link.payload["state_topic"],
        "svolder/0102030405060708/availability"
    );
    assert_eq!(link.payload["payload_on"], "online");
    assert_eq!(link.payload["payload_off"], "offline");
    assert_eq!(link.payload["device_class"], "connectivity");
    assert_eq!(
        link.payload["availability"],
        json!([{"topic": "svolder/bridge/availability"}])
    );
}

#[test]
fn every_entity_carries_the_device_and_origin_blocks() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let profile = registry.get("gungnir2-v2").expect("gungnir2-v2 exists");
    let entries = configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        true,
        &[],
    );
    let volume = entries
        .iter()
        .find(|e| e.topic.contains("/volume/"))
        .expect("a volume entry");
    assert_eq!(
        volume.payload["device"],
        json!({
            "identifiers": ["svolder_0102030405060708"],
            "name": "Living room DAC",
            "manufacturer": "Schiit Audio",
            "model": "Gungnir 2",
            "sw_version": "3",
        })
    );
    assert_eq!(volume.payload["origin"]["name"], "svolder");
    assert_eq!(
        volume.payload["availability"],
        json!([
            {"topic": "svolder/bridge/availability"},
            {"topic": "svolder/0102030405060708/availability"},
        ])
    );
    assert_eq!(volume.payload["availability_mode"], "all");
}

#[test]
fn the_clearing_payload_is_empty() {
    assert_eq!(CLEARING_PAYLOAD, "");
}

/// The discovery entry whose topic ends `/<key>/config`.
fn entry_of<'a>(entries: &'a [DiscoveryEntry], key: &str) -> &'a DiscoveryEntry {
    entries
        .iter()
        .find(|e| e.topic.ends_with(&format!("/{key}/config")))
        .unwrap_or_else(|| panic!("no {key} entry"))
}

fn entries_for(commands: bool) -> Vec<DiscoveryEntry> {
    configs(
        "svolder",
        "homeassistant",
        &profile("gungnir2-v2"),
        &device("0102030405060708"),
        commands,
        &[],
    )
}

#[test]
fn discovery_has_a_diagnostic_signal_sensor() {
    for commands in [false, true] {
        let entries = entries_for(commands);
        let signal = entry_of(&entries, "signal");
        assert_eq!(
            signal.topic,
            "homeassistant/sensor/svolder_0102030405060708/signal/config"
        );
        let p = &signal.payload;
        assert_eq!(p["name"], "Signal");
        assert_eq!(p["unique_id"], "svolder_0102030405060708_signal");
        assert_eq!(p["device_class"], "signal_strength");
        assert_eq!(p["unit_of_measurement"], "dBm");
        assert_eq!(p["entity_category"], "diagnostic");
        assert_eq!(p["state_class"], "measurement");
        assert_eq!(p["state_topic"], "svolder/0102030405060708/signal/state");
        assert_eq!(
            p["json_attributes_topic"],
            "svolder/0102030405060708/signal/attributes"
        );
        assert!(p.get("command_topic").is_none(), "{p}");
        // The device availability every entity carries: unavailable while the link is down.
        assert_eq!(
            p["availability"],
            json!([
                {"topic": "svolder/bridge/availability"},
                {"topic": "svolder/0102030405060708/availability"},
            ])
        );
    }
}

#[test]
fn discovery_has_firmware_and_dsp_diagnostic_sensors() {
    for commands in [false, true] {
        let entries = entries_for(commands);
        for (key, name) in [("firmware", "Firmware"), ("dsp_firmware", "DSP firmware")] {
            let sensor = entry_of(&entries, key);
            assert_eq!(
                sensor.topic,
                format!("homeassistant/sensor/svolder_0102030405060708/{key}/config")
            );
            let p = &sensor.payload;
            assert_eq!(p["name"], name);
            assert_eq!(p["unique_id"], format!("svolder_0102030405060708_{key}"));
            assert_eq!(p["entity_category"], "diagnostic");
            assert_eq!(
                p["state_topic"],
                format!("svolder/0102030405060708/{key}/state")
            );
            assert!(p.get("device_class").is_none(), "{p}");
            assert!(p.get("unit_of_measurement").is_none(), "{p}");
        }
    }
    // The device block still carries the advertised build, not either reading.
    let entries = entries_for(false);
    assert_eq!(
        entry_of(&entries, "firmware").payload["device"]["sw_version"],
        "3"
    );
}

#[test]
fn discovery_has_an_identify_button_only_with_commands() {
    let without = entries_for(false);
    assert!(
        !without.iter().any(|e| e.topic.contains("/identify/")),
        "read-only offers no button"
    );
    assert!(
        !without.iter().any(|e| e.topic.contains("/button/")),
        "read-only offers no button"
    );

    let with = entries_for(true);
    let button = entry_of(&with, "identify");
    assert_eq!(
        button.topic,
        "homeassistant/button/svolder_0102030405060708/identify/config"
    );
    let p = &button.payload;
    assert_eq!(p["name"], "Identify");
    assert_eq!(p["unique_id"], "svolder_0102030405060708_identify");
    assert_eq!(p["entity_category"], "config");
    assert_eq!(p["command_topic"], "svolder/0102030405060708/identify/set");
    assert_eq!(p["payload_press"], "PRESS");
    assert!(p.get("state_topic").is_none(), "a button has no state: {p}");
    assert_eq!(
        p["availability"],
        json!([
            {"topic": "svolder/bridge/availability"},
            {"topic": "svolder/0102030405060708/availability"},
        ])
    );
    assert_eq!(p["availability_mode"], "all");
}

/// Home Assistant is offered no Identify button for a device whose profile offers no identify (a
/// Mesh Card), with commands on or off.
#[test]
fn discovery_has_no_identify_button_for_a_profile_that_offers_none() {
    let card = profile("meshcard");
    assert!(!card.identify);
    for commands in [false, true] {
        let entries = configs(
            "svolder",
            "homeassistant",
            &card,
            &device("0102030405060708"),
            commands,
            &[],
        );
        assert!(
            !entries.iter().any(|e| e.topic.contains("/identify/")),
            "commands {commands}"
        );
        assert!(!entries.is_empty());
    }
}

/// A firmware sensor is offered only for a build the session asks for: `firmware` where the
/// profile's `firmware_subsystems` holds 1, `dsp_firmware` where it holds 2.
#[test]
fn the_firmware_sensors_follow_the_subsystems_a_profile_asks() {
    for (name, firmware, dsp) in [
        ("gungnir2-v2", true, true),
        ("mimir", true, false),
        ("meshcard", true, false),
        ("freya", false, false),
    ] {
        let entries = configs(
            "svolder",
            "homeassistant",
            &profile(name),
            &device("0102030405060708"),
            false,
            &[],
        );
        let has = |key: &str| {
            entries
                .iter()
                .any(|e| e.topic.ends_with(&format!("/{key}/config")))
        };
        assert_eq!(
            (has("firmware"), has("dsp_firmware")),
            (firmware, dsp),
            "{name}"
        );
        assert!(has("signal"), "{name}");
    }
}

/// The keys the bridge's own entities use are exactly the ones a profile may not give a control.
#[test]
fn the_bridges_own_keys_are_the_ones_profiles_may_not_take() {
    let mut own = vec![
        LINK_KEY,
        SIGNAL_KEY,
        FIRMWARE_KEY,
        DSP_FIRMWARE_KEY,
        IDENTIFY_KEY,
        EQ_MODE_KEY,
        crate::eq::PRESET_KEY,
        crate::eq::LOUDNESS_KEY,
    ];
    let mut reserved = svolder_profiles::RESERVED_KEYS.to_vec();
    own.sort_unstable();
    reserved.sort_unstable();
    assert_eq!(own, reserved);
    // And no built-in profile's control takes one (the registry refuses it on load).
    for control in profile("gungnir2-v2").controls {
        assert!(!reserved.contains(&control.key.as_str()), "{}", control.key);
    }
}

/// Discovery off still offers nothing, the new entities included.
#[test]
fn an_empty_prefix_offers_no_diagnostic_entity_either() {
    let entries = configs(
        "svolder",
        "",
        &profile("gungnir2-v2"),
        &device("0102030405060708"),
        true,
        &[],
    );
    assert!(entries.is_empty());
}
