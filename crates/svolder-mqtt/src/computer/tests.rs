// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_host::eq::PointView;

use super::*;

fn view(on: bool, preset: Option<&str>, gain: f64) -> EqView {
    EqView {
        on,
        points: vec![PointView {
            slot: 0,
            kind: "PK".into(),
            f: 1000.0,
            g: gain,
            q: 1.0,
        }],
        preset: preset.map(str::to_string),
        max: 16,
        ..Default::default()
    }
}

#[test]
fn read_only_makes_the_switch_a_binary_sensor() {
    let entries = entries("svolder", "homeassistant", false);
    assert_eq!(
        entries[0].topic,
        "homeassistant/binary_sensor/svolder_computer/computer_eq/config"
    );
    assert!(entries[0].payload.get("command_topic").is_none());
    let with = super::entries("svolder", "homeassistant", true);
    assert_eq!(
        with[0].topic,
        "homeassistant/switch/svolder_computer/computer_eq/config"
    );
    assert_eq!(
        with[0].payload["command_topic"],
        "svolder/computer/computer_eq/set"
    );
}

#[test]
fn an_empty_discovery_prefix_publishes_nothing() {
    assert!(entries("svolder", "", true).is_empty());
}

#[test]
fn no_volume_mute_or_point_entity_exists() {
    for commands in [true, false] {
        let keys: Vec<String> = entries("svolder", "homeassistant", commands)
            .iter()
            .map(|e| e.topic.clone())
            .collect();
        assert_eq!(keys.len(), 2, "{keys:?}");
        for word in ["volume", "mute", "point", "gain", "frequency"] {
            assert!(keys.iter().all(|k| !k.contains(word)), "{word}: {keys:?}");
        }
    }
}

#[test]
fn the_sensor_names_the_preset_custom_or_nothing() {
    assert_eq!(
        state(Some(&view(true, Some("Warm"), -3.0))),
        ("ON", "Warm".to_string())
    );
    assert_eq!(
        state(Some(&view(true, Some("custom"), -3.0))),
        ("ON", "custom".to_string())
    );
    assert_eq!(
        state(Some(&view(true, None, -3.0))),
        ("ON", "custom".to_string())
    );
    assert_eq!(
        state(Some(&view(false, Some("Warm"), -3.0))),
        ("OFF", String::new())
    );
    assert_eq!(state(None), ("OFF", String::new()));
}

#[test]
fn nothing_is_kept_only_for_a_flat_curve_no_preset_made() {
    assert!(nothing_kept(&view(false, None, 0.0)));
    assert!(!nothing_kept(&view(false, None, 2.0)));
    assert!(!nothing_kept(&view(false, Some("Flat"), 0.0)));
}

#[test]
fn a_command_is_on_or_off_in_any_case() {
    assert_eq!(parse("ON"), Some(true));
    assert_eq!(parse(" off\n"), Some(false));
    assert_eq!(parse("On"), Some(true));
    assert_eq!(parse("toggle"), None);
    assert_eq!(parse(""), None);
}
