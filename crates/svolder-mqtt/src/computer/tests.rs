// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_host::eq::crossfeed::{Model, ModelView};
use svolder_host::eq::{
    BypassView, ChainLevels, CrossfeedView, LoudnessView, PointView, SpeakersView,
};

use super::*;

fn desk() -> Computer {
    Computer::new("Reframe")
}

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
    let entries = entries(&desk(), "svolder", "homeassistant", false, None, None);
    assert_eq!(
        entries[0].topic,
        "homeassistant/binary_sensor/svolder_computer_reframe/computer_eq/config"
    );
    assert!(entries[0].payload.get("command_topic").is_none());
    let with = super::entries(&desk(), "svolder", "homeassistant", true, None, None);
    assert_eq!(
        with[0].topic,
        "homeassistant/switch/svolder_computer_reframe/computer_eq/config"
    );
    assert_eq!(
        with[0].payload["command_topic"],
        "svolder/computer_reframe/computer_eq/set"
    );
}

#[test]
fn an_empty_discovery_prefix_publishes_nothing() {
    assert!(entries(&desk(), "svolder", "", true, None, None).is_empty());
}

#[test]
fn no_volume_mute_or_point_entity_exists() {
    for commands in [true, false] {
        let keys: Vec<String> = entries(&desk(), "svolder", "homeassistant", commands, None, None)
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
    let all = entries(&desk(), "svolder", "homeassistant", true, Some(&view), None);
    assert_eq!(all.len(), 4);
    assert_eq!(
        all[2].topic,
        "homeassistant/switch/svolder_computer_reframe/computer_crossfeed/config"
    );
    assert_eq!(
        all[2].payload["command_topic"],
        "svolder/computer_reframe/computer_crossfeed/set"
    );
    assert_eq!(
        all[2].payload["state_topic"],
        "svolder/computer_reframe/computer_crossfeed/state"
    );
    assert_eq!(
        all[3].topic,
        "homeassistant/select/svolder_computer_reframe/computer_crossfeed_preset/config"
    );
    assert_eq!(
        all[3].payload["command_topic"],
        "svolder/computer_reframe/computer_crossfeed_preset/set"
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
    let all = entries(
        &desk(),
        "svolder",
        "homeassistant",
        false,
        Some(&view),
        None,
    );
    assert_eq!(
        all[2].topic,
        "homeassistant/binary_sensor/svolder_computer_reframe/computer_crossfeed/config"
    );
    assert_eq!(
        all[3].topic,
        "homeassistant/sensor/svolder_computer_reframe/computer_crossfeed_preset/config"
    );
    for entry in &all[2..] {
        assert!(entry.payload.get("command_topic").is_none());
        assert!(entry.payload.get("options").is_none());
    }
}

#[test]
fn a_controller_without_crossfeed_announces_none_and_topics_of_lists_every_entity() {
    assert_eq!(
        entries(&desk(), "svolder", "homeassistant", true, None, None).len(),
        2
    );
    assert_eq!(
        entries_of(
            &desk(),
            "svolder",
            "homeassistant",
            true,
            &EqView::default()
        )
        .len(),
        4
    );
    for commands in [true, false] {
        let eq = EqView {
            crossfeed: Some(xf_view(true, "meier", None)),
            speakers: Some(sp_view(false, None)),
            loudness: Some(ld_view(true, 35.0)),
            bypass: Some(bypass_view(false)),
            levels: Some(levels(0.0, 0.0, 0.0)),
            ..Default::default()
        };
        let announced: Vec<String> = entries_of(&desk(), "svolder", "homeassistant", commands, &eq)
            .into_iter()
            .map(|e| e.topic)
            .collect();
        assert_eq!(topics_of(&desk(), "homeassistant", commands), announced);
    }
    assert!(topics_of(&desk(), "", true).is_empty());
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

fn set_view(id: &str, label: &str, with_room: bool) -> svolder_host::eq::speakers::SetView {
    svolder_host::eq::speakers::SetView {
        id: id.into(),
        room: if with_room { "with" } else { "no" }.into(),
        room_name: if with_room { "With room" } else { "No room" }.into(),
        term: if with_room { "BRIR" } else { "HRTF" }.into(),
        label: label.into(),
        licence: "CC-BY-4.0".into(),
        credit: Some("Credit words".into()),
        source_url: "https://example.org/set".into(),
    }
}

fn sp_view(on: bool, chosen: Option<&str>) -> SpeakersView {
    SpeakersView {
        on,
        set: chosen.map(str::to_string),
        room_name: chosen.map(|_| "No room".into()),
        term: chosen.map(|_| "HRTF".into()),
        licence: "CC-BY-4.0".into(),
        credit: Some("Credit words".into()),
        source_url: "https://example.org/set".into(),
        angles: "left 30.0 deg, right 330.0 deg, 1.5 m".into(),
        loudness_db: -7.0,
        sets: vec![set_view("a", "Set A", false), set_view("b", "Set B", true)],
        ..Default::default()
    }
}

#[test]
fn the_speakers_entities_are_a_switch_and_a_select_with_commands() {
    let view = sp_view(false, Some("a"));
    let all = entries(&desk(), "svolder", "homeassistant", true, None, Some(&view));
    assert_eq!(all.len(), 4);
    assert_eq!(
        all[2].topic,
        "homeassistant/switch/svolder_computer_reframe/computer_speakers/config"
    );
    assert_eq!(all[2].payload["name"], "Virtual speakers");
    assert_eq!(
        all[2].payload["command_topic"],
        "svolder/computer_reframe/computer_speakers/set"
    );
    assert_eq!(
        all[3].topic,
        "homeassistant/select/svolder_computer_reframe/computer_speakers_set/config"
    );
    assert_eq!(
        all[3].payload["options"],
        serde_json::json!(["Set A · No room · HRTF", "Set B · With room · BRIR"])
    );
    assert_eq!(
        all[3].payload["json_attributes_topic"],
        "svolder/computer_reframe/computer_speakers_set/attributes"
    );
}

#[test]
fn without_commands_the_speakers_are_a_binary_sensor_and_a_sensor() {
    let view = sp_view(false, None);
    let all = entries(
        &desk(),
        "svolder",
        "homeassistant",
        false,
        None,
        Some(&view),
    );
    assert_eq!(
        all[2].topic,
        "homeassistant/binary_sensor/svolder_computer_reframe/computer_speakers/config"
    );
    assert_eq!(
        all[3].topic,
        "homeassistant/sensor/svolder_computer_reframe/computer_speakers_set/config"
    );
    assert!(all[3].payload.get("command_topic").is_none());
    assert!(all[3].payload.get("options").is_none());
}

#[test]
fn a_controller_without_speakers_announces_none_and_topics_of_lists_every_entity() {
    assert_eq!(
        entries(&desk(), "svolder", "homeassistant", true, None, None).len(),
        2
    );
    let view = sp_view(false, None);
    for commands in [true, false] {
        let announced: Vec<String> = entries(
            &desk(),
            "svolder",
            "homeassistant",
            commands,
            None,
            Some(&view),
        )
        .into_iter()
        .map(|e| e.topic)
        .collect();
        let all = topics_of(&desk(), "homeassistant", commands);
        assert!(announced.iter().all(|t| all.contains(t)), "{announced:?}");
        assert!(all.iter().any(|t| t.contains("computer_speakers_set")));
    }
}

#[test]
fn two_sets_with_one_label_are_told_apart_by_their_id() {
    let sets = [set_view("a", "Same", false), set_view("b", "Same", false)];
    assert_eq!(
        speakers::options(&sets),
        ["Same · No room · HRTF (a)", "Same · No room · HRTF (b)"]
    );
}

#[test]
fn the_speakers_state_names_the_chosen_set_and_carries_its_licence() {
    let state = speakers::state(&sp_view(true, Some("b")));
    assert_eq!(state.on, "ON");
    assert_eq!(state.set, "Set B · With room · BRIR");
    let attrs: serde_json::Value = serde_json::from_str(&state.attributes).unwrap();
    assert_eq!(attrs["licence"], "CC-BY-4.0");
    assert_eq!(attrs["credit"], "Credit words");
    assert_eq!(attrs["source_url"], "https://example.org/set");
    // Kept while off.
    assert_eq!(speakers::state(&sp_view(false, Some("a"))).on, "OFF");
    assert_eq!(
        speakers::state(&sp_view(false, Some("a"))).set,
        "Set A · No room · HRTF"
    );
    // No choice, or a choice that is not fetched: no set, no attributes.
    for chosen in [None, Some("gone")] {
        let state = speakers::state(&sp_view(false, chosen));
        assert!(state.set.is_empty() && state.attributes.is_empty());
    }
}

#[test]
fn a_set_command_names_a_fetched_set_in_any_case() {
    let view = sp_view(false, None);
    assert_eq!(
        speakers::parse_set(&view, " set b · with room · brir "),
        Some("b".to_string())
    );
    assert_eq!(speakers::parse_set(&view, "Set B"), None);
    assert_eq!(speakers::parse_set(&view, ""), None);
}

#[test]
fn no_chosen_set_resets_the_select_to_unknown_and_the_attributes_to_nothing() {
    for chosen in [None, Some("gone")] {
        let state = speakers::state(&sp_view(false, chosen));
        assert_eq!(state.set_payload(), "None");
        assert_eq!(state.attributes_payload(), "{}");
    }
    let state = speakers::state(&sp_view(false, Some("a")));
    assert_eq!(state.set_payload(), "Set A · No room · HRTF");
    assert!(state.attributes_payload().contains("CC-BY-4.0"));
}

#[test]
fn an_option_that_differs_only_in_case_is_matched_exactly_first() {
    let mut view = sp_view(false, None);
    view.sets = vec![set_view("a", "Room", false), set_view("b", "room", false)];
    assert_eq!(
        speakers::parse_set(&view, "room · No room · HRTF"),
        Some("b".to_string())
    );
    assert_eq!(
        speakers::parse_set(&view, "Room · No room · HRTF"),
        Some("a".to_string())
    );
    assert_eq!(
        speakers::parse_set(&view, "ROOM · no room · hrtf"),
        Some("a".to_string())
    );
}

fn ld_view(on: bool, strength_pct: f64) -> LoudnessView {
    LoudnessView {
        on,
        strength_pct,
        ..Default::default()
    }
}

fn eq_with_loudness(view: Option<LoudnessView>) -> EqView {
    EqView {
        loudness: view,
        ..Default::default()
    }
}

/// The loudness entities (the last two of `all`) each have their own stable `unique_id` and sit on
/// the same device as the EQ switch (the first).
fn assert_loudness_identity(all: &[DiscoveryEntry]) {
    assert_eq!(
        all[2].payload["unique_id"],
        "svolder_computer_reframe_computer_loudness"
    );
    assert_eq!(
        all[3].payload["unique_id"],
        "svolder_computer_reframe_computer_loudness_strength"
    );
    let ids: Vec<_> = all.iter().map(|e| &e.payload["unique_id"]).collect();
    for (i, id) in ids.iter().enumerate() {
        assert!(!ids[..i].contains(id), "unique_id {id} is used twice");
    }
    for e in &all[2..] {
        assert_eq!(e.payload["device"], all[0].payload["device"]);
    }
}

#[test]
fn the_loudness_entities_are_a_switch_and_a_number_with_the_views_ranges() {
    let mut view = ld_view(true, 35.0);
    view.ranges.strength_pct = svolder_host::eq::crossfeed::Range {
        min: 0.0,
        max: 100.0,
        step: 5.0,
    };
    let eq = eq_with_loudness(Some(view));
    let all = entries_of(&desk(), "svolder", "homeassistant", true, &eq);
    assert_eq!(all.len(), 6);
    assert_eq!(
        all[2].topic,
        "homeassistant/switch/svolder_computer_reframe/computer_loudness/config"
    );
    assert_eq!(all[2].payload["name"], "Loudness");
    assert_eq!(
        all[2].payload["command_topic"],
        "svolder/computer_reframe/computer_loudness/set"
    );
    assert_eq!(
        all[2].payload["state_topic"],
        "svolder/computer_reframe/computer_loudness/state"
    );
    assert_eq!(
        all[3].topic,
        "homeassistant/number/svolder_computer_reframe/computer_loudness_strength/config"
    );
    assert_eq!(all[3].payload["name"], "Loudness strength");
    assert_eq!(
        all[3].payload["command_topic"],
        "svolder/computer_reframe/computer_loudness_strength/set"
    );
    assert_eq!(all[3].payload["min"], 0);
    assert_eq!(all[3].payload["max"], 100);
    assert_eq!(all[3].payload["step"], 5);
    assert_eq!(all[3].payload["unit_of_measurement"], "%");
    assert_loudness_identity(&all);
    // The ranges are the view's: a view with other ones moves the bounds.
    let mut other = ld_view(true, 35.0);
    other.ranges.strength_pct = svolder_host::eq::crossfeed::Range {
        min: 10.0,
        max: 80.0,
        step: 2.5,
    };
    let all = entries_of(
        &desk(),
        "svolder",
        "homeassistant",
        true,
        &eq_with_loudness(Some(other)),
    );
    assert_eq!(all[3].payload["min"], 10);
    assert_eq!(all[3].payload["max"], 80);
    assert_eq!(all[3].payload["step"], 2.5);
}

#[test]
fn without_commands_the_loudness_is_a_binary_sensor_and_a_sensor() {
    let eq = eq_with_loudness(Some(ld_view(false, 50.0)));
    let all = entries_of(&desk(), "svolder", "homeassistant", false, &eq);
    assert_eq!(
        all[2].topic,
        "homeassistant/binary_sensor/svolder_computer_reframe/computer_loudness/config"
    );
    assert_eq!(
        all[3].topic,
        "homeassistant/sensor/svolder_computer_reframe/computer_loudness_strength/config"
    );
    for e in &all[2..4] {
        assert!(e.payload.get("command_topic").is_none());
        assert!(e.payload.get("min").is_none());
    }
    assert_loudness_identity(&all);
}

#[test]
fn a_view_without_loudness_announces_none_and_an_empty_prefix_nothing() {
    assert_eq!(
        entries_of(
            &desk(),
            "svolder",
            "homeassistant",
            true,
            &eq_with_loudness(None)
        )
        .len(),
        4
    );
    assert!(
        entries_of(
            &desk(),
            "svolder",
            "",
            true,
            &eq_with_loudness(Some(ld_view(true, 35.0)))
        )
        .is_empty()
    );
    // The sweep still knows the topics, whatever the view holds.
    let topics = topics_of(&desk(), "homeassistant", true);
    assert!(
        topics.contains(
            &"homeassistant/number/svolder_computer_reframe/computer_loudness_strength/config"
                .to_string()
        )
    );
}

#[test]
fn the_loudness_states_follow_the_view_and_the_strength_parses() {
    assert_eq!(
        loudness::state(&ld_view(true, 35.0)),
        ("ON", "35".to_string())
    );
    assert_eq!(
        loudness::state(&ld_view(false, 52.5)),
        ("OFF", "52.5".to_string())
    );
    assert_eq!(loudness::parse_strength(" 40 "), Some(40.0));
    assert_eq!(loudness::parse_strength("37.5"), Some(37.5));
    assert_eq!(loudness::parse_strength("loud"), None);
    assert_eq!(loudness::parse_strength("NaN"), None);
    assert_eq!(loudness::parse_strength("inf"), None);
    assert_eq!(loudness::parse_strength(""), None);
}

fn path_entries(commands: bool) -> Vec<DiscoveryEntry> {
    let all = entries_of(
        &desk(),
        "svolder",
        "homeassistant",
        commands,
        &EqView::default(),
    );
    all[all.len() - 2..].to_vec()
}

#[test]
fn the_path_sensor_and_the_config_problem_sensor_are_read_only_discovery_entries() {
    for commands in [true, false] {
        let both = path_entries(commands);
        assert_eq!(
            both[0].topic,
            "homeassistant/sensor/svolder_computer_reframe/computer_path/config"
        );
        assert_eq!(
            both[1].topic,
            "homeassistant/binary_sensor/svolder_computer_reframe/computer_config/config"
        );
        for entry in &both {
            assert!(entry.payload.get("command_topic").is_none());
            assert_eq!(entry.payload["device"]["name"], "Reframe");
            assert_eq!(entry.payload["availability_mode"], "all");
        }
        assert_eq!(both[0].payload["name"], "Path");
        assert_eq!(
            both[0].payload["unique_id"],
            "svolder_computer_reframe_computer_path"
        );
        assert_eq!(
            both[0].payload["state_topic"],
            "svolder/computer_reframe/computer_path/state"
        );
        assert_eq!(
            both[0].payload["json_attributes_topic"],
            "svolder/computer_reframe/computer_path/attributes"
        );
        assert_eq!(both[0].payload["device_class"], "enum");
        assert_eq!(
            both[0].payload["options"],
            serde_json::json!([
                "untouched",
                "changed",
                "not_this_dac",
                "other_input",
                "unknown",
                "not_read"
            ])
        );
        assert_eq!(both[1].payload["name"], "PipeWire config");
        assert_eq!(both[1].payload["device_class"], "problem");
        assert_eq!(both[1].payload["payload_on"], "ON");
        assert_eq!(both[1].payload["payload_off"], "OFF");
        assert_eq!(
            both[1].payload["state_topic"],
            "svolder/computer_reframe/computer_config/state"
        );
        assert_eq!(
            both[1].payload["json_attributes_topic"],
            "svolder/computer_reframe/computer_config/attributes"
        );
    }
}

#[test]
fn the_path_entities_are_cleared_with_the_computer_and_an_empty_prefix_has_none() {
    for commands in [true, false] {
        let topics = topics_of(&desk(), "homeassistant", commands);
        for entry in path_entries(commands) {
            assert!(topics.contains(&entry.topic), "{}", entry.topic);
        }
    }
    assert!(path::topics_of(&desk(), "").is_empty());
    assert!(path::entries(&desk(), "svolder", "").is_empty());
}

#[test]
fn the_path_state_carries_its_words_sentence_and_dac() {
    let said = path::path_state(
        path::CHANGED,
        &["dither".to_string()],
        Some("dither; the DAC lowers it"),
        Some("Bifrost 3"),
    );
    assert_eq!(said.state, "changed");
    let attributes: serde_json::Value = serde_json::from_str(&said.attributes).unwrap();
    assert_eq!(
        attributes,
        serde_json::json!({
            "words": ["dither"],
            "sentence": "dither; the DAC lowers it",
            "dac": "Bifrost 3",
        })
    );
    let other = path::path_state(path::NOT_THIS_DAC, &["→ DELL".to_string()], None, None);
    let attributes: serde_json::Value = serde_json::from_str(&other.attributes).unwrap();
    assert!(attributes["sentence"].is_null() && attributes["dac"].is_null());
}

#[test]
fn the_config_state_is_on_for_a_failing_line_off_for_a_clean_one_and_none_unread() {
    let lines = [
        path::ConfigLine {
            key: "rates",
            outcome: "pass",
            words: "all allowed",
        },
        path::ConfigLine {
            key: "filter",
            outcome: "fail",
            words: "a filter runs",
        },
    ];
    let on = path::config_state(
        Some(true),
        "changes",
        "summary",
        "Bifrost 3",
        "limit",
        &lines,
    );
    assert_eq!(on.state, Some("ON"));
    let attributes: serde_json::Value = serde_json::from_str(&on.attributes).unwrap();
    assert_eq!(attributes["outcome"], "changes");
    assert_eq!(attributes["summary"], "summary");
    assert_eq!(attributes["limit"], "limit");
    assert_eq!(attributes["sink"], "Bifrost 3");
    assert_eq!(attributes["lines"][1]["key"], "filter");
    assert_eq!(attributes["lines"][1]["outcome"], "fail");
    assert_eq!(attributes["lines"][0]["words"], "all allowed");
    assert_eq!(
        path::config_state(Some(false), "untouched", "ok", "x", "l", &lines).state,
        Some("OFF")
    );
    assert_eq!(
        path::config_state(None, "unread", "u", "x", "l", &lines).state,
        None
    );
    let none = path::config_unjudged();
    assert_eq!((none.state, none.attributes.as_str()), (None, "{}"));
}

#[test]
fn the_config_payload_is_none_for_an_unjudged_state_never_empty() {
    let on = path::config_state(Some(true), "k", "x", "y", "l", &[]);
    let off = path::config_state(Some(false), "k", "x", "y", "l", &[]);
    assert_eq!((on.payload(), off.payload()), ("ON", "OFF"));
    let unread = path::config_state(None, "k", "x", "y", "l", &[]);
    assert_eq!(unread.payload(), "None");
    assert_eq!(path::config_unjudged().payload(), "None");
}

/// The topics the releases before per-computer ids announced, written out from the old code's
/// `topics_of` (unit `computer`), not built from the current functions.
fn old_topics(commands: bool) -> Vec<String> {
    let (switch, select, number) = match commands {
        true => ("switch", "select", "number"),
        false => ("binary_sensor", "sensor", "sensor"),
    };
    let p = "homeassistant";
    vec![
        format!("{p}/{switch}/svolder_computer/computer_eq/config"),
        format!("{p}/sensor/svolder_computer/computer_eq_preset/config"),
        format!("{p}/{switch}/svolder_computer/computer_crossfeed/config"),
        format!("{p}/{select}/svolder_computer/computer_crossfeed_preset/config"),
        format!("{p}/{switch}/svolder_computer/computer_speakers/config"),
        format!("{p}/{select}/svolder_computer/computer_speakers_set/config"),
        format!("{p}/{switch}/svolder_computer/computer_loudness/config"),
        format!("{p}/{number}/svolder_computer/computer_loudness_strength/config"),
        format!("{p}/sensor/svolder_computer/computer_path/config"),
        format!("{p}/binary_sensor/svolder_computer/computer_config/config"),
    ]
}

#[test]
fn the_legacy_topics_are_exactly_what_the_old_computer_announced() {
    for commands in [true, false] {
        assert_eq!(
            legacy_topics_of("homeassistant", commands),
            old_topics(commands)
        );
    }
    assert!(legacy_topics_of("", true).is_empty());
}

#[test]
fn the_device_block_names_the_host_and_every_id_carries_it() {
    let all = entries_of(
        &desk(),
        "svolder",
        "homeassistant",
        true,
        &EqView::default(),
    );
    assert!(!all.is_empty());
    for entry in &all {
        assert_eq!(
            entry.payload["device"],
            json!({
                "identifiers": ["svolder_computer_reframe"],
                "name": "Reframe",
                "manufacturer": "svolder",
                "model": "Computer",
            })
        );
        assert!(entry.topic.contains("/svolder_computer_reframe/"));
        let unique = entry.payload["unique_id"].as_str().unwrap();
        assert!(unique.starts_with("svolder_computer_reframe_"), "{unique}");
        assert_eq!(
            entry.payload["availability"][0]["topic"],
            "svolder/bridge/reframe/availability"
        );
        assert_eq!(
            entry.payload["availability"][1]["topic"],
            "svolder/computer_reframe/availability"
        );
    }
}

/// Every string an entry carries that names a topic or an id, however nested.
fn names_of(value: &Json, out: &mut std::collections::BTreeSet<String>) {
    match value {
        Json::String(text) => {
            out.insert(text.clone());
        }
        Json::Array(items) => items.iter().for_each(|item| names_of(item, out)),
        Json::Object(map) => map.values().for_each(|item| names_of(item, out)),
        _ => {}
    }
}

#[test]
fn two_computers_share_no_topic_and_no_id() {
    let eq = EqView {
        crossfeed: Some(xf_view(true, "Bauer", None)),
        speakers: Some(sp_view(true, Some("a"))),
        loudness: Some(LoudnessView {
            on: true,
            strength_pct: 50.0,
            ..Default::default()
        }),
        ..Default::default()
    };
    let other = Computer::new("Desk-2");
    assert_eq!(other.unit(), "computer_desk_2");
    let collect = |computer: &Computer, commands: bool| {
        let mut names = std::collections::BTreeSet::new();
        for entry in entries_of(computer, "svolder", "homeassistant", commands, &eq) {
            names.insert(entry.topic.clone());
            for key in [
                "unique_id",
                "state_topic",
                "command_topic",
                "json_attributes_topic",
                "availability",
                "device",
            ] {
                if let Some(value) = entry.payload.get(key) {
                    names_of(value, &mut names);
                }
            }
        }
        names
    };
    for commands in [true, false] {
        let a = collect(&desk(), commands);
        let b = collect(&other, commands);
        assert!(a.len() > 30, "{a:?}");
        let shared: Vec<_> = a.intersection(&b).collect();
        // Only the fields every device of any computer carries are the same text.
        let allowed = ["svolder", "Computer"];
        assert!(
            shared.iter().all(|name| allowed.contains(&name.as_str())),
            "shared: {shared:?}"
        );
    }
}

#[test]
fn the_legacy_state_topics_are_exactly_what_the_old_computer_kept_retained() {
    let b = "svolder/computer";
    let want: Vec<String> = [
        "availability",
        "computer_eq/state",
        "computer_eq_preset/state",
        "computer_crossfeed/state",
        "computer_crossfeed_preset/state",
        "computer_loudness/state",
        "computer_loudness_strength/state",
        "computer_speakers/state",
        "computer_speakers_set/state",
        "computer_path/state",
        "computer_config/state",
        "computer_speakers_set/attributes",
        "computer_path/attributes",
        "computer_config/attributes",
    ]
    .iter()
    .map(|leaf| format!("{b}/{leaf}"))
    .collect();
    assert_eq!(legacy_state_topics("svolder"), want);
}

fn bypass_view(on: bool) -> BypassView {
    BypassView {
        on,
        ..Default::default()
    }
}

fn levels(total: f64, eq: f64, space: f64) -> ChainLevels {
    ChainLevels {
        total_db: total,
        eq_db: eq,
        space_db: space,
        ..Default::default()
    }
}

fn eq_with(bypass: Option<BypassView>, levels: Option<ChainLevels>) -> EqView {
    EqView {
        bypass,
        levels,
        ..Default::default()
    }
}

#[test]
fn the_bypass_is_a_switch_with_commands_and_a_binary_sensor_without() {
    let eq = eq_with(Some(bypass_view(false)), None);
    let all = entries_of(&desk(), "svolder", "homeassistant", true, &eq);
    let bypass = all.last().expect("the bypass entry");
    assert_eq!(
        bypass.topic,
        "homeassistant/switch/svolder_computer_reframe/computer_bypass/config"
    );
    assert_eq!(bypass.payload["name"], "Bypass");
    assert_eq!(
        bypass.payload["unique_id"],
        "svolder_computer_reframe_computer_bypass"
    );
    assert_eq!(
        bypass.payload["command_topic"],
        "svolder/computer_reframe/computer_bypass/set"
    );
    assert_eq!(
        bypass.payload["state_topic"],
        "svolder/computer_reframe/computer_bypass/state"
    );
    assert_eq!(bypass.payload["payload_on"], "ON");
    assert_eq!(bypass.payload["payload_off"], "OFF");

    let all = entries_of(&desk(), "svolder", "homeassistant", false, &eq);
    let bypass = all.last().expect("the bypass entry");
    assert_eq!(
        bypass.topic,
        "homeassistant/binary_sensor/svolder_computer_reframe/computer_bypass/config"
    );
    assert!(bypass.payload.get("command_topic").is_none());
}

#[test]
fn the_levels_are_three_db_sensors_that_measure() {
    let eq = eq_with(None, Some(levels(-14.8, -7.8, -7.0)));
    for commands in [true, false] {
        let all = entries_of(&desk(), "svolder", "homeassistant", commands, &eq);
        let sensors = &all[all.len() - 3..];
        let want = [
            ("computer_level_chain", "Chain level"),
            ("computer_level_eq", "EQ level"),
            ("computer_level_space", "Space level"),
        ];
        for (entry, (key, name)) in sensors.iter().zip(want) {
            assert_eq!(
                entry.topic,
                format!("homeassistant/sensor/svolder_computer_reframe/{key}/config")
            );
            assert_eq!(entry.payload["name"], name);
            assert_eq!(entry.payload["unit_of_measurement"], "dB");
            assert_eq!(entry.payload["state_class"], "measurement");
            assert_eq!(entry.payload["suggested_display_precision"], 1);
            assert!(entry.payload.get("command_topic").is_none());
            assert_eq!(
                entry.payload["state_topic"],
                format!("svolder/computer_reframe/{key}/state")
            );
        }
    }
}

#[test]
fn a_view_without_the_fields_announces_none_of_them() {
    let all = entries_of(
        &desk(),
        "svolder",
        "homeassistant",
        true,
        &EqView::default(),
    );
    assert!(
        all.iter()
            .all(|e| !e.topic.contains("computer_bypass") && !e.topic.contains("computer_level"))
    );
    // Only the bypass, only the levels.
    let bypass_only = eq_with(Some(bypass_view(true)), None);
    let all = entries_of(&desk(), "svolder", "homeassistant", true, &bypass_only);
    assert_eq!(
        all.iter()
            .filter(|e| e.topic.contains("computer_level"))
            .count(),
        0
    );
    assert_eq!(
        all.iter()
            .filter(|e| e.topic.contains("computer_bypass"))
            .count(),
        1
    );
    let levels_only = eq_with(None, Some(levels(0.0, 0.0, 0.0)));
    let all = entries_of(&desk(), "svolder", "homeassistant", true, &levels_only);
    assert_eq!(
        all.iter()
            .filter(|e| e.topic.contains("computer_level"))
            .count(),
        3
    );
    assert_eq!(
        all.iter()
            .filter(|e| e.topic.contains("computer_bypass"))
            .count(),
        0
    );
    // An empty prefix announces nothing; the sweep still knows every topic, both ways.
    assert!(
        entries_of(
            &desk(),
            "svolder",
            "",
            true,
            &eq_with(Some(bypass_view(true)), Some(levels(0.0, 0.0, 0.0)))
        )
        .is_empty()
    );
    for (commands, component) in [(true, "switch"), (false, "binary_sensor")] {
        let topics = topics_of(&desk(), "homeassistant", commands);
        for want in [
            format!("homeassistant/{component}/svolder_computer_reframe/computer_bypass/config"),
            "homeassistant/sensor/svolder_computer_reframe/computer_level_chain/config".into(),
            "homeassistant/sensor/svolder_computer_reframe/computer_level_eq/config".into(),
            "homeassistant/sensor/svolder_computer_reframe/computer_level_space/config".into(),
        ] {
            assert!(topics.contains(&want), "{want}");
        }
    }
}

#[test]
fn the_bypass_and_level_states_and_commands_parse() {
    assert_eq!(bypass::state(&bypass_view(true)), "ON");
    assert_eq!(bypass::state(&bypass_view(false)), "OFF");
    assert_eq!(
        bypass::levels_state(&levels(-14.84, -7.76, -7.0)),
        ["-14.8", "-7.8", "-7.0"]
    );
    let mut said = levels(-14.84, -7.76, -7.0);
    said.words = "plays \u{2212}14.8 dB in all".into();
    let attributes: serde_json::Value =
        serde_json::from_str(&bypass::levels_attributes(&said)).unwrap();
    assert_eq!(attributes["words"], "plays \u{2212}14.8 dB in all");
    assert_eq!(attributes["untouched"], false);
    // A tiny negative level reads as zero, never `-0.0`.
    assert_eq!(bypass::levels_state(&levels(-0.04, 0.0, 0.0))[0], "0.0");
    assert_eq!(bypass::level_text(3.25), "3.3");
    assert_eq!(parse(" on "), Some(true));
    assert_eq!(parse("OFF"), Some(false));
    assert_eq!(parse("toggle"), None);
}
