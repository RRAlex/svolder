// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_host::eq::crossfeed::{Model, ModelView};
use svolder_host::eq::{CrossfeedView, PointView};

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
    let entries = entries("svolder", "homeassistant", false, None);
    assert_eq!(
        entries[0].topic,
        "homeassistant/binary_sensor/svolder_computer/computer_eq/config"
    );
    assert!(entries[0].payload.get("command_topic").is_none());
    let with = super::entries("svolder", "homeassistant", true, None);
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
    assert!(entries("svolder", "", true, None).is_empty());
}

#[test]
fn no_volume_mute_or_point_entity_exists() {
    for commands in [true, false] {
        let keys: Vec<String> = entries("svolder", "homeassistant", commands, None)
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

fn xf_view(on: bool, model: &str, preset: Option<&str>) -> CrossfeedView {
    CrossfeedView {
        on,
        model: model.to_string(),
        preset: preset.map(str::to_string),
        models: Model::ALL.into_iter().map(ModelView::of).collect(),
        ..Default::default()
    }
}

#[test]
fn the_crossfeed_entities_are_a_switch_and_a_select_with_commands() {
    let view = xf_view(false, "bauer", Some("bs2b default"));
    let all = entries("svolder", "homeassistant", true, Some(&view));
    assert_eq!(all.len(), 4);
    assert_eq!(
        all[2].topic,
        "homeassistant/switch/svolder_computer/computer_crossfeed/config"
    );
    assert_eq!(
        all[2].payload["command_topic"],
        "svolder/computer/computer_crossfeed/set"
    );
    assert_eq!(
        all[2].payload["state_topic"],
        "svolder/computer/computer_crossfeed/state"
    );
    assert_eq!(
        all[3].topic,
        "homeassistant/select/svolder_computer/computer_crossfeed_preset/config"
    );
    assert_eq!(
        all[3].payload["command_topic"],
        "svolder/computer/computer_crossfeed_preset/set"
    );
    assert_eq!(
        all[3].payload["options"],
        serde_json::json!(crossfeed::options(&view.models))
    );
    assert_eq!(all[3].payload["device"], all[0].payload["device"]);
}

#[test]
fn without_commands_the_crossfeed_is_a_binary_sensor_and_a_sensor() {
    let view = xf_view(false, "bauer", None);
    let all = entries("svolder", "homeassistant", false, Some(&view));
    assert_eq!(
        all[2].topic,
        "homeassistant/binary_sensor/svolder_computer/computer_crossfeed/config"
    );
    assert_eq!(
        all[3].topic,
        "homeassistant/sensor/svolder_computer/computer_crossfeed_preset/config"
    );
    for entry in &all[2..] {
        assert!(entry.payload.get("command_topic").is_none());
        assert!(entry.payload.get("options").is_none());
    }
}

#[test]
fn a_controller_without_crossfeed_announces_none_and_topics_of_lists_every_entity() {
    assert_eq!(entries("svolder", "homeassistant", true, None).len(), 2);
    for commands in [true, false] {
        let view = xf_view(true, "meier", None);
        let announced: Vec<String> = entries("svolder", "homeassistant", commands, Some(&view))
            .into_iter()
            .map(|e| e.topic)
            .collect();
        assert_eq!(topics_of("homeassistant", commands), announced);
    }
    assert!(topics_of("", true).is_empty());
}

#[test]
fn the_options_are_each_preset_of_each_model_then_custom() {
    let view = xf_view(false, "bauer", None);
    let options = crossfeed::options(&view.models);
    assert_eq!(
        options[..3],
        [
            "Bauer · Jan Meier",
            "Bauer · Chu Moy",
            "Bauer · bs2b default"
        ]
    );
    assert_eq!(
        options[3..6],
        ["Linkwitz", "Meier · Default", "Meier · Rockbox"]
    );
    for single in ["Mid/Side bass", "Linkwitz"] {
        assert!(options.iter().any(|o| o == single), "{single}: {options:?}");
    }
    assert_eq!(options.last().map(String::as_str), Some("Custom"));
    let presets: usize = view.models.iter().map(|m| m.presets.len()).sum();
    assert_eq!(options.len(), presets + 1);
}

#[test]
fn the_crossfeed_states_follow_the_settings_on_or_off() {
    assert_eq!(
        crossfeed::state(&xf_view(true, "bauer", Some("Chu Moy"))),
        ("ON", "Bauer · Chu Moy".to_string())
    );
    assert_eq!(
        crossfeed::state(&xf_view(false, "linkwitz", Some("Linkwitz"))),
        ("OFF", "Linkwitz".to_string())
    );
    assert_eq!(
        crossfeed::state(&xf_view(true, "meier", None)),
        ("ON", "Custom".to_string())
    );
}

#[test]
fn a_preset_command_names_a_model_and_a_preset_or_is_custom() {
    let view = xf_view(false, "bauer", None);
    assert_eq!(
        crossfeed::parse_preset(&view, " bauer · chu moy "),
        Some(crossfeed::Choice::Preset {
            model: "bauer".into(),
            preset: "Chu Moy".into()
        })
    );
    let Some(crossfeed::Choice::Preset { model, preset }) =
        crossfeed::parse_preset(&view, "Mid/Side bass")
    else {
        panic!("a model with one preset is its own option");
    };
    assert_eq!(model, "mid-side");
    assert_eq!(
        preset,
        Model::MidSide.spec().presets[0].name,
        "the model's one preset"
    );
    assert_eq!(
        crossfeed::parse_preset(&view, "custom"),
        Some(crossfeed::Choice::Custom)
    );
    assert_eq!(crossfeed::parse_preset(&view, "Bauer"), None);
    assert_eq!(crossfeed::parse_preset(&view, ""), None);
}
