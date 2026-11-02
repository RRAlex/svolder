// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::discovery::configs;
use crate::test_support::{device, profile};
use serde_json::json;
use svolder_proto::{Message, Value};

fn entries_of(profile: &Profile, commands: bool) -> Vec<DiscoveryEntry> {
    configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        commands,
        &[],
    )
}

fn select_of(entries: &[DiscoveryEntry]) -> &DiscoveryEntry {
    entries
        .iter()
        .find(|e| e.topic.contains("/eq_preset/config"))
        .expect("a preset entity")
}

/// The six bands of a Loki Max as the session reports them, one raw step each.
fn bands(raw: [u8; 6]) -> Vec<Message> {
    let loki = profile("lokimax");
    (1..=6)
        .map(|b| {
            let control = loki
                .control_by_key(&format!("band{b}"))
                .expect("a band control");
            Message::status(control.id, Value::U8(raw[b - 1]))
        })
        .collect()
}

#[test]
fn the_loki_max_select_offers_the_slots_the_builtins_and_custom() {
    let loki = profile("lokimax");
    let entries = entries_of(&loki, true);
    let select = select_of(&entries);
    assert!(
        select.topic.starts_with("homeassistant/select/"),
        "{}",
        select.topic
    );
    assert_eq!(select.payload["name"], json!("Preset"));
    assert_eq!(
        select.payload["options"],
        json!([
            "Preset 1",
            "Preset 2",
            "Preset 3",
            "Flat",
            "Bass Boost",
            "Treble Boost",
            "Smiley",
            "Loudness",
            "custom"
        ])
    );
    assert_eq!(
        select.payload["command_topic"],
        json!("svolder/0102030405060708/eq_preset/set")
    );
    assert_eq!(
        select.payload["value_template"],
        json!("{{ value_json.eq_preset }}")
    );
    // Read-only, it is a sensor with nothing to command.
    let read_only = entries_of(&loki, false);
    let sensor = select_of(&read_only);
    assert!(sensor.topic.starts_with("homeassistant/sensor/"));
    assert!(sensor.payload.get("command_topic").is_none());
    assert!(sensor.payload.get("options").is_none());
    // And none of the tone control's own entities, which the profile has no tone control for.
    for entry in &entries {
        assert!(!entry.topic.contains("/eq_loudness/"), "{}", entry.topic);
        assert!(!entry.topic.contains("/eq_band_"), "{}", entry.topic);
    }
}

#[test]
fn a_profile_with_no_presets_of_its_own_gets_no_entity_here() {
    assert!(options(&profile("fake-amp")).is_empty());
    let dac = profile("gungnir2-v2");
    assert!(!target(&dac, "eq_preset"), "the tone control owns the key");
    let entries = entries_of(&dac, true);
    let selects = entries
        .iter()
        .filter(|e| e.topic.contains("/eq_preset/config"))
        .count();
    assert_eq!(selects, 1);
    assert!(!target(&profile("lokimax"), "recall"));
    assert!(!target(&profile("lokimax"), "save"));
    assert!(target(&profile("lokimax"), "EQ_Preset"));
}

#[test]
fn its_state_is_the_matching_builtin_or_custom_and_none_until_every_band_reads() {
    let loki = profile("lokimax");
    assert_eq!(state(&loki, &[]), None);
    assert_eq!(state(&loki, &bands([15; 6])[..5]), None);
    assert_eq!(state(&loki, &bands([15; 6])).as_deref(), Some("Flat"));
    assert_eq!(
        state(&loki, &bands([23, 16, 8, 8, 16, 23])).as_deref(),
        Some("Smiley")
    );
    assert_eq!(
        state(&loki, &bands([28, 21, 17, 15, 15, 14])).as_deref(),
        Some("custom")
    );
    // The state is in the payload the bridge publishes, once every band has read.
    let published = crate::payload::state(&loki, &bands([28, 21, 17, 15, 15, 15]));
    assert_eq!(published["eq_preset"], json!("Bass Boost"));
    let partial = crate::payload::state(&loki, &bands([15; 6])[..2]);
    assert!(partial.get("eq_preset").is_none());
}

#[test]
fn a_pick_of_preset_2_parses_to_the_slot() {
    let loki = profile("lokimax");
    assert_eq!(parse(&loki, "Preset 2").unwrap(), Command::Slot(2));
    assert_eq!(parse(&loki, "  preset 3 ").unwrap(), Command::Slot(3));
    assert_eq!(
        parse(&loki, "bass boost").unwrap(),
        Command::Setting("Bass Boost".to_string())
    );
    assert_eq!(
        parse(&loki, "Flat").unwrap(),
        Command::Setting("Flat".to_string())
    );
}

#[test]
fn custom_and_unknown_names_are_refused() {
    let loki = profile("lokimax");
    for text in [
        "custom", "Custom", "Preset 4", "Preset 0", "save", "Save 1", "",
    ] {
        assert!(parse(&loki, text).is_err(), "{text:?}");
    }
    let refused = parse(&loki, "Preset 4").unwrap_err().to_string();
    assert!(
        refused.contains("Preset 1, Preset 2, Preset 3, Flat"),
        "{refused}"
    );
    assert!(parse(&profile("gungnir2-v2"), "Preset 1").is_err());
}
