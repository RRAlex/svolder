// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use serde_json::json;

use super::*;

fn desk() -> Computer {
    Computer::new("Reframe")
}

const NAMES: [&str; 3] = ["Late night", "Desk", "Movie"];

#[test]
fn commands_on_give_a_select_with_the_names_and_custom_and_a_text() {
    let all = entries(&desk(), "svolder", "homeassistant", true, &NAMES);
    assert_eq!(all.len(), 2);
    assert_eq!(
        all[0].topic,
        "homeassistant/select/svolder_computer_reframe/scene/config"
    );
    assert_eq!(
        all[0].payload["options"],
        json!(["Late night", "Desk", "Movie", "custom"])
    );
    assert_eq!(
        all[0].payload["command_topic"],
        "svolder/computer_reframe/scene/set"
    );
    assert_eq!(
        all[0].payload["state_topic"],
        "svolder/computer_reframe/scene/state"
    );
    assert_eq!(
        all[0].payload["json_attributes_topic"],
        "svolder/computer_reframe/scene/attributes"
    );
    assert_eq!(
        all[1].topic,
        "homeassistant/text/svolder_computer_reframe/scene_save/config"
    );
    assert_eq!(
        all[1].payload["command_topic"],
        "svolder/computer_reframe/scene_save/set"
    );
    assert_eq!(all[1].payload["max"], 40);
    assert_eq!(all[1].payload["min"], 1);
    assert!(all[1].payload.get("pattern").is_none());
    assert!(all[1].payload.get("state_topic").is_none());
}

#[test]
fn commands_off_give_a_sensor_alone_with_no_command_and_no_text() {
    let all = entries(&desk(), "svolder", "homeassistant", false, &NAMES);
    assert_eq!(all.len(), 1);
    assert_eq!(
        all[0].topic,
        "homeassistant/sensor/svolder_computer_reframe/scene/config"
    );
    for field in ["command_topic", "options"] {
        assert!(all[0].payload.get(field).is_none(), "{field}");
    }
    assert!(all[0].payload.get("json_attributes_topic").is_some());
}

#[test]
fn the_pair_is_on_the_computers_device_and_lists_only_the_bridge_as_available() {
    for entry in entries(&desk(), "svolder", "homeassistant", true, &NAMES) {
        assert_eq!(
            entry.payload["device"]["identifiers"],
            json!(["svolder_computer_reframe"])
        );
        assert_eq!(entry.payload["device"]["model"], "Computer");
        assert_eq!(
            entry.payload["availability"],
            json!([{"topic": "svolder/bridge/reframe/availability"}])
        );
    }
}

#[test]
fn the_topics_follow_the_commands_and_an_empty_prefix_publishes_nothing() {
    assert_eq!(topics_of(&desk(), "homeassistant", true).len(), 2);
    assert_eq!(topics_of(&desk(), "homeassistant", false).len(), 1);
    assert!(topics_of(&desk(), "", true).is_empty());
    assert!(entries(&desk(), "svolder", "", true, &NAMES).is_empty());
    for commands in [true, false] {
        let made: Vec<String> = entries(&desk(), "svolder", "homeassistant", commands, &NAMES)
            .into_iter()
            .map(|e| e.topic)
            .collect();
        assert_eq!(made, topics_of(&desk(), "homeassistant", commands));
    }
}

#[test]
fn the_options_follow_the_names_and_never_list_custom_twice() {
    assert_eq!(options::<&str>(&[]), ["custom"]);
    assert_eq!(options(&["Desk", "Custom "]), ["Desk", "Custom "]);
    let renamed = entries(
        &desk(),
        "svolder",
        "homeassistant",
        true,
        &["Night", "Movie"],
    );
    assert_eq!(
        renamed[0].payload["options"],
        json!(["Night", "Movie", "custom"])
    );
}

#[test]
fn the_state_is_the_scene_on_else_custom() {
    assert_eq!(state(Some("Desk")), "Desk");
    assert_eq!(state(None), "custom");
}

#[test]
fn a_choice_names_a_scene_by_the_daemons_rule_and_custom_is_no_scene() {
    assert_eq!(
        parse_choice(&NAMES, " movie "),
        Some(Choice::Scene("Movie".into()))
    );
    assert_eq!(
        parse_choice(&["desk", "Desk"], "Desk"),
        Some(Choice::Scene("Desk".into())),
        "the exact text first"
    );
    assert_eq!(parse_choice(&NAMES, "Custom"), Some(Choice::Custom));
    assert_eq!(parse_choice(&NAMES, "Party"), None);
    assert_eq!(parse_choice(&NAMES, ""), None);
}

#[test]
fn a_save_takes_the_trimmed_text_and_nothing_blank() {
    assert_eq!(parse_save("  Quiet "), Some("Quiet".into()));
    assert_eq!(parse_save("   "), None);
}

#[test]
fn the_attributes_carry_the_contract_and_leave_out_what_is_not_there() {
    let scenes = json!([{"name": "Desk", "parts": [{"who": "computer", "present": true}]}]);
    let bare: serde_json::Value =
        serde_json::from_str(&attributes(scenes.clone(), true, None, None, None)).unwrap();
    assert_eq!(
        bare,
        json!({"scenes": scenes, "known": true, "recalled": null})
    );
    let outcome = json!({"name": "Desk", "parts": [{"who": "computer", "status": "done"}]});
    let saved = json!({"name": "Desk", "error": "a scene named Desk exists"});
    let full: serde_json::Value = serde_json::from_str(&attributes(
        scenes.clone(),
        false,
        Some("Desk"),
        Some(&outcome),
        Some(&saved),
    ))
    .unwrap();
    assert_eq!(
        full,
        json!({"scenes": scenes, "known": false, "recalled": "Desk",
               "outcome": outcome, "saved": saved})
    );
}
