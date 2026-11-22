// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use super::*;
use crate::test_support::builtin;
use crate::tone::preset::{builtins, plan_recall};
use crate::{EqParam, Registry};

/// Readings for every id of `profile` a scene reads: each held control on its first value, the
/// tone control flat, the built-in bands at 15.
fn readings(profile: &Profile) -> HashMap<FunctionId, i64> {
    let mut map = HashMap::new();
    for control in held_controls(profile) {
        let raw = match control.kind {
            ControlKind::Select => i64::from(control.values[0].0),
            ControlKind::Level => control.range.expect("a level has a range").0,
            _ => 0,
        };
        map.insert(control.id, raw);
    }
    if let Some(eq) = &profile.eq {
        let flat = &builtins(eq)[0];
        for write in plan_recall(eq, |_| None, flat).unwrap().writes {
            let raw = match write {
                preset::Write::Band { raw, .. } => i64::from(raw),
                preset::Write::Mode(mode) => i64::from(mode as u8),
                preset::Write::Loudness(raw) => i64::from(raw),
            };
            map.insert(write.id(eq).unwrap(), raw);
        }
        map.insert(eq.loudness_id(), 0);
    }
    if let Some(table) = &profile.builtin_settings {
        for key in &table.keys {
            map.insert(profile.control_by_key(key).unwrap().id, 15);
        }
    }
    map
}

fn reader(map: &HashMap<FunctionId, i64>) -> impl Fn(FunctionId) -> Option<i64> + '_ {
    move |id| map.get(&id).copied()
}

fn keys(profile: &Profile) -> Vec<String> {
    held_controls(profile)
        .iter()
        .map(|c| c.key.clone())
        .collect()
}

fn part(profile: &Profile, map: &HashMap<FunctionId, i64>) -> DevicePart {
    let fitting = profile.eq.as_ref().map(builtins).unwrap_or_default();
    capture(profile, "u1", "Desk", &fitting, None, reader(map)).expect("every reading is there")
}

/// What every shipped profile holds in a scene, in profile order (the controls of
/// tests/builtin.rs `EXPECTED` that are a select, toggle or level and not a volume, a gain, a
/// power, a mute, a control that asks, the tone's own or the amps' meter switch). A profile that
/// gains a held control fails here, so a person decides each time.
const HELD: &[(&str, &[&str])] = &[
    ("aegir2", &[]),
    ("amp-common", &[]),
    ("amp-standby", &[]),
    ("amp-stereo", &[]),
    ("bifrost3", &["phase", "nos", "balance", "input"]),
    ("dac-common", &["phase"]),
    ("eitr2", &["phase", "balance"]),
    ("fake-amp", &[]),
    ("fake-five", &[]),
    ("fake-mono", &[]),
    ("freya", &["input"]),
    ("freya2", &["input", "balance"]),
    ("gjallarhorn", &[]),
    ("gungnir2-v1", &["phase", "input", "nos"]),
    ("gungnir2-v2", &["phase", "nos", "balance", "input"]),
    (
        "hati",
        &[
            "input",
            "subsonic",
            "load_capacitance",
            "load_resistance",
            "mono",
        ],
    ),
    ("kara", &["input"]),
    ("kraken", &[]),
    ("lokimax", &["input"]),
    ("lyr5", &["input", "mode"]),
    ("mesh-eq-3", &["phase", "balance"]),
    ("mesh-eq-5", &["phase", "nos", "balance"]),
    ("meshcard", &["phase", "balance", "nos"]),
    ("mimir", &["phase", "balance", "input", "nos"]),
    (
        "phono-common",
        &["input", "subsonic", "load_capacitance", "load_resistance"],
    ),
    ("preamp-common", &["mode"]),
    ("saga2", &["input"]),
    (
        "skoll",
        &["input", "subsonic", "load_capacitance", "load_resistance"],
    ),
    (
        "stjarna",
        &[
            "input",
            "subsonic",
            "load_capacitance",
            "load_resistance",
            "mono",
        ],
    ),
    ("tyr", &[]),
    ("vidar2", &[]),
    ("wotan", &["input", "feedback"]),
    ("yggdrasil", &["phase", "nos", "balance", "input"]),
];

#[test]
fn every_shipped_profile_holds_exactly_these_controls() {
    let registry = Registry::builtin().unwrap();
    let mut names = registry.names();
    names.sort_unstable();
    let rows: Vec<&str> = HELD.iter().map(|(name, _)| *name).collect();
    assert_eq!(names, rows, "a profile needs a row");
    for (name, want) in HELD {
        assert_eq!(keys(registry.get(name).unwrap()), *want, "{name}");
    }
}

#[test]
fn no_profile_holds_a_volume_a_gain_power_mute_or_readout() {
    let registry = Registry::builtin().unwrap();
    for name in registry.names() {
        let profile = registry.get(name).unwrap();
        for control in held_controls(profile) {
            assert!(
                matches!(
                    control.kind,
                    ControlKind::Select | ControlKind::Toggle | ControlKind::Level
                ),
                "{name}: {} is a {:?}",
                control.key,
                control.kind
            );
            assert!(!roles::is_volume(control), "{name}: volume held");
            assert!(!control.output_gain, "{name}: {} is a gain", control.key);
            assert!(!control.powers_output, "{name}: {} powers", control.key);
            assert!(!control.mutes_output, "{name}: {} mutes", control.key);
            assert!(!control.asks_on_change, "{name}: {} asks", control.key);
            assert!(!control.auto_off, "{name}: {} auto-off", control.key);
            if let Some(eq) = &profile.eq {
                assert!(
                    !eq.owns(control.id),
                    "{name}: {} is the tone's",
                    control.key
                );
            }
            if let Some(table) = &profile.builtin_settings {
                assert!(
                    !table.keys.contains(&control.key),
                    "{name}: {} is a raw band",
                    control.key
                );
            }
        }
    }
}

#[test]
fn a_capture_plans_to_nothing_and_matches() {
    for name in ["bifrost3", "lokimax"] {
        let profile = builtin(name);
        let map = readings(&profile);
        let part = part(&profile, &map);
        assert_eq!(part.udid, "u1");
        assert_eq!(part.name, "Desk");
        let plan = plan(&profile, &part, reader(&map));
        assert_eq!(plan, DevicePlan::default(), "{name}");
        assert_eq!(matches(&profile, &part, reader(&map)), Some(true), "{name}");
    }
}

#[test]
fn a_capture_holds_the_tone_named_after_the_preset_on_or_custom() {
    let profile = builtin("bifrost3");
    let map = readings(&profile);
    let part = part(&profile, &map);
    let tone = part.tone.as_ref().expect("a tone part");
    assert_eq!(tone.name, "Flat");
    assert!(tone.source.is_none());
    assert!(part.raw_bands.is_none());
    let none = capture(&profile, "u1", "Desk", &[], None, reader(&map)).unwrap();
    assert_eq!(none.tone.unwrap().name, "Custom");

    let loki = builtin("lokimax");
    let part = self::part(&loki, &readings(&loki));
    assert!(part.tone.is_none());
    assert_eq!(part.raw_bands.as_ref().unwrap().len(), 6);
    assert_eq!(part.controls, [("input".to_string(), part.controls[0].1)]);
}

#[test]
fn an_unread_band_or_control_is_unread_and_matches_none() {
    let profile = builtin("bifrost3");
    let map = readings(&profile);
    let part = part(&profile, &map);
    let eq = profile.eq.as_ref().unwrap();
    let band = eq.band_id(2, EqParam::Gain).unwrap();
    let held = held_controls(&profile)[0].id;
    for lost in [band, held] {
        let mut map = map.clone();
        map.remove(&lost);
        let got = capture(&profile, "u1", "Desk", &[], None, reader(&map));
        assert!(got.is_err(), "{lost:?}");
        assert_eq!(matches(&profile, &part, reader(&map)), None, "{lost:?}");
    }
    let loki = builtin("lokimax");
    let mut map = readings(&loki);
    map.remove(&loki.control_by_key("band3").unwrap().id);
    assert!(capture(&loki, "u1", "Loki", &[], None, reader(&map)).is_err());
}

#[test]
fn a_moved_setting_is_not_on_and_is_planned_back() {
    let profile = builtin("bifrost3");
    let mut map = readings(&profile);
    let part = part(&profile, &map);
    let phase = profile.control_by_key("phase").unwrap();
    map.insert(phase.id, 1);
    assert_eq!(matches(&profile, &part, reader(&map)), Some(false));
    let plan = plan(&profile, &part, reader(&map));
    assert_eq!(plan.sets, [("phase".to_string(), 0)]);
    assert!(plan.tone.is_none() && plan.skipped.is_empty());
}

#[test]
fn a_value_the_profile_lacks_a_level_out_of_range_or_a_lost_key_is_skipped_never_clamped() {
    let profile = builtin("bifrost3");
    let map = readings(&profile);
    let mut part = part(&profile, &map);
    let balance = profile.control_by_key("balance").unwrap();
    let (_, high) = balance.range.unwrap();
    for (key, raw) in &mut part.controls {
        match key.as_str() {
            "input" => *raw = 99,
            "balance" => *raw = high + 50,
            _ => {}
        }
    }
    part.controls.push(("tube_glow".to_string(), 1));
    let plan = plan(&profile, &part, reader(&map));
    assert!(plan.sets.is_empty(), "{:?}", plan.sets);
    assert_eq!(plan.skipped.len(), 3, "{:?}", plan.skipped);
    for key in ["input", "balance", "tube_glow"] {
        assert!(
            plan.skipped.iter().any(|words| words.starts_with(key)),
            "{key} in {:?}",
            plan.skipped
        );
    }
}

#[test]
fn a_held_key_that_is_now_the_volume_is_skipped() {
    let profile = builtin("bifrost3");
    let map = readings(&profile);
    let mut part = part(&profile, &map);
    part.controls.push(("volume".to_string(), 0));
    let plan = plan(&profile, &part, reader(&map));
    assert!(plan.sets.iter().all(|(key, _)| key != "volume"));
    assert_eq!(plan.skipped.len(), 1);
}

#[test]
fn a_tone_part_of_another_band_count_is_skipped_in_the_presets_words() {
    let profile = builtin("bifrost3");
    let map = readings(&profile);
    let mut part = part(&profile, &map);
    let tone = part.tone.as_mut().unwrap();
    tone.band.pop();
    tone.bands -= 1;
    let eq = profile.eq.as_ref().unwrap();
    let words = plan_recall(eq, reader(&map), part.tone.as_ref().unwrap())
        .unwrap_err()
        .to_string();
    let plan = plan(&profile, &part, reader(&map));
    assert!(plan.tone.is_none());
    assert_eq!(plan.skipped, [words]);
    assert_eq!(matches(&profile, &part, reader(&map)), Some(false));
}

#[test]
fn the_tone_plan_is_the_preset_the_writer_recalls_in_its_own_order() {
    let profile = builtin("bifrost3");
    let eq = profile.eq.as_ref().unwrap();
    let map = readings(&profile);
    let mut part = part(&profile, &map);
    let smiley = builtins(eq)
        .into_iter()
        .find(|p| p.name == "Bass")
        .expect("a built-in Bass");
    part.tone = Some(Preset {
        source: None,
        ..smiley.clone()
    });
    let got = plan(&profile, &part, reader(&map));
    let tone = got.tone.expect("a curve to recall");
    assert_eq!(tone, smiley);
    // Nothing in the scene reorders it: the writes are plan_recall's of the same preset.
    assert_eq!(
        plan_recall(eq, reader(&map), &tone).unwrap(),
        plan_recall(eq, reader(&map), &smiley).unwrap()
    );
    assert!(
        !plan_recall(eq, reader(&map), &tone)
            .unwrap()
            .writes
            .is_empty()
    );
}

#[test]
fn raw_bands_plan_cuts_first_and_leave_equal_bands_out() {
    let loki = builtin("lokimax");
    let mut map = readings(&loki);
    let mut part = part(&loki, &map);
    // The device reads band 5 at 23 and band 1 at 15; the part holds Flat, so band 5 is cut.
    map.insert(loki.control_by_key("band5").unwrap().id, 23);
    for (key, raw) in part.raw_bands.as_mut().unwrap() {
        if key == "band1" {
            *raw = 28;
        }
    }
    let plan = plan(&loki, &part, reader(&map));
    assert_eq!(
        plan.raw,
        Some(vec![("band5".to_string(), 15), ("band1".to_string(), 28)])
    );
    assert!(plan.tone.is_none() && plan.sets.is_empty());
    assert_eq!(matches(&loki, &part, reader(&map)), Some(false));
}

#[test]
fn a_raw_band_the_device_lacks_or_a_step_out_of_range_is_skipped() {
    let loki = builtin("lokimax");
    let map = readings(&loki);
    let mut part = part(&loki, &map);
    let bands = part.raw_bands.as_mut().unwrap();
    bands[0].1 = 9999;
    bands.push(("band9".to_string(), 15));
    let plan = plan(&loki, &part, reader(&map));
    assert!(plan.raw.is_none());
    assert_eq!(plan.skipped.len(), 2, "{:?}", plan.skipped);
    assert!(plan.skipped[0].starts_with("band1"));
    assert!(plan.skipped[1].starts_with("band9"));
}

/// The readings of `readings` moved off their defaults: every select on its last value, every
/// level on its top step, a tone control on the Bass curve.
fn moved(profile: &Profile) -> HashMap<FunctionId, i64> {
    let mut map = readings(profile);
    for control in held_controls(profile) {
        match control.kind {
            ControlKind::Select => {
                map.insert(control.id, i64::from(control.values.last().unwrap().0));
            }
            ControlKind::Level => {
                map.insert(control.id, control.range.unwrap().1);
            }
            _ => {
                map.insert(control.id, 1);
            }
        }
    }
    if let Some(eq) = &profile.eq {
        let bass = builtins(eq).into_iter().find(|p| p.name == "Bass").unwrap();
        for write in plan_recall(eq, |_| None, &bass).unwrap().writes {
            let raw = match write {
                preset::Write::Band { raw, .. } => i64::from(raw),
                preset::Write::Mode(mode) => i64::from(mode as u8),
                preset::Write::Loudness(raw) => i64::from(raw),
            };
            map.insert(write.id(eq).unwrap(), raw);
        }
    }
    map
}

#[test]
fn a_capture_away_from_the_defaults_round_trips() {
    for name in ["bifrost3", "lokimax", "hati"] {
        let profile = builtin(name);
        let map = moved(&profile);
        let part = part(&profile, &map);
        if name == "bifrost3" {
            assert_eq!(part.tone.as_ref().unwrap().name, "Bass");
        }
        assert_eq!(
            plan(&profile, &part, reader(&map)),
            DevicePlan::default(),
            "{name}"
        );
        assert_eq!(matches(&profile, &part, reader(&map)), Some(true), "{name}");
    }
}

#[test]
fn a_level_is_planned_at_both_ends_of_its_range_and_skipped_one_beyond() {
    let profile = builtin("bifrost3");
    let mut map = readings(&profile);
    let part = part(&profile, &map);
    let balance = profile.control_by_key("balance").unwrap();
    let (low, high) = balance.range.unwrap();
    map.insert(balance.id, (low + high) / 2);
    let with = |step: i64| {
        let mut part = part.clone();
        part.controls
            .iter_mut()
            .find(|(k, _)| k == "balance")
            .unwrap()
            .1 = step;
        plan(&profile, &part, reader(&map))
    };
    for edge in [low, high] {
        let got = with(edge);
        assert_eq!(got.sets, [("balance".to_string(), edge)], "{edge}");
        assert!(got.skipped.is_empty(), "{edge}");
    }
    for beyond in [low - 1, high + 1] {
        let got = with(beyond);
        assert!(got.sets.is_empty(), "{beyond}");
        assert_eq!(got.skipped.len(), 1, "{beyond}");
    }
}

#[test]
fn a_toggle_step_of_two_is_skipped() {
    let profile = builtin("bifrost3");
    let map = readings(&profile);
    let mut part = part(&profile, &map);
    part.controls
        .iter_mut()
        .find(|(k, _)| k == "phase")
        .unwrap()
        .1 = 2;
    let plan = plan(&profile, &part, reader(&map));
    assert!(plan.sets.is_empty());
    assert_eq!(plan.skipped.len(), 1);
    assert!(plan.skipped[0].starts_with("phase"));
}

#[test]
fn a_loki_with_an_unread_band_matches_none() {
    let loki = builtin("lokimax");
    let mut map = readings(&loki);
    let part = part(&loki, &map);
    map.remove(&loki.control_by_key("band3").unwrap().id);
    assert_eq!(matches(&loki, &part, reader(&map)), None);
}

#[test]
fn unread_names_what_is_missing() {
    let profile = builtin("bifrost3");
    let map = readings(&profile);
    let eq = profile.eq.as_ref().unwrap();
    let mut no_band = map.clone();
    no_band.remove(&eq.band_id(0, EqParam::Gain).unwrap());
    let got = capture(&profile, "u1", "Desk", &[], None, reader(&no_band));
    assert_eq!(got.unwrap_err().what, "the tone control");
    let mut no_phase = map.clone();
    no_phase.remove(&profile.control_by_key("phase").unwrap().id);
    let got = capture(&profile, "u1", "Desk", &[], None, reader(&no_phase));
    let why = got.unwrap_err();
    assert_eq!(why.what, "phase");
    assert_eq!(why.to_string(), "phase has not been read yet");
    let loki = builtin("lokimax");
    let mut no_band = readings(&loki);
    no_band.remove(&loki.control_by_key("band3").unwrap().id);
    let got = capture(&loki, "u1", "Loki", &[], None, reader(&no_band));
    assert_eq!(got.unwrap_err().what, "band3");
}

#[test]
fn the_part_is_named_after_the_preset_the_device_side_shows_as_on() {
    let profile = builtin("bifrost3");
    let map = readings(&profile);
    let eq = profile.eq.as_ref().unwrap();
    let mut copy = builtins(eq)[0].clone();
    copy.name = "Mine".to_string();
    let fitting = [builtins(eq)[0].clone(), copy];
    let name = |recalled| {
        capture(&profile, "u1", "Desk", &fitting, recalled, reader(&map))
            .unwrap()
            .tone
            .unwrap()
            .name
    };
    assert_eq!(name(None), "Flat");
    assert_eq!(name(Some("Mine")), "Mine");
    assert_eq!(name(Some("Bass")), "Flat");
}

#[test]
fn sets_come_in_profile_order_whatever_the_parts_order() {
    let profile = builtin("bifrost3");
    let mut map = readings(&profile);
    let mut part = part(&profile, &map);
    for control in held_controls(&profile) {
        if control.kind != ControlKind::Select {
            let now = map[&control.id];
            map.insert(control.id, if now == 0 { 1 } else { 0 });
        }
    }
    part.controls.reverse();
    let plan = plan(&profile, &part, reader(&map));
    let got: Vec<&str> = plan.sets.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(got, ["phase", "nos", "balance"]);
}

#[test]
fn a_tone_part_on_a_device_without_one_is_skipped_as_tone() {
    let loki = builtin("lokimax");
    let bifrost = builtin("bifrost3");
    let mut part = part(&loki, &readings(&loki));
    part.tone = self::part(&bifrost, &readings(&bifrost)).tone;
    let plan = plan(&loki, &part, reader(&readings(&loki)));
    assert_eq!(plan.skipped.len(), 1);
    assert!(plan.skipped[0].starts_with("tone:"), "{:?}", plan.skipped);
}

/// A device reports a toggle as a boolean, which `Value::as_i64` does not read: through
/// [`raw_of`] it is the step 0 or 1, so a part holding a toggle is captured, planned and matched.
#[test]
fn a_toggle_reported_as_a_boolean_reads_as_a_step() {
    let profile = builtin("bifrost3");
    let toggle = held_controls(&profile)
        .into_iter()
        .find(|control| control.kind == ControlKind::Toggle)
        .expect("a held toggle")
        .clone();
    // Every reading as the device reports it: toggles as booleans, the rest as integers.
    let reported: Vec<Message> = readings(&profile)
        .into_iter()
        .map(|(id, raw)| match profile.control_by_id(id) {
            Some(control) if control.kind == ControlKind::Toggle => {
                Message::status(id, Value::Bool(id == toggle.id))
            }
            _ => Message::status(id, Value::U8(u8::try_from(raw).expect("a small step"))),
        })
        .collect();
    let raw = raw_of(&reported);
    assert_eq!(raw(toggle.id), Some(1), "an on toggle is step 1");
    let part = capture(&profile, "0102030405060708", "Desk", &[], None, &raw)
        .expect("a toggle reported as a boolean is read");
    assert!(
        part.controls.contains(&(toggle.key.clone(), 1)),
        "{:?}",
        part.controls
    );
    assert_eq!(matches(&profile, &part, &raw), Some(true));
    assert!(plan(&profile, &part, &raw).sets.is_empty(), "already there");
    let mut flipped = part.clone();
    for (key, step) in &mut flipped.controls {
        if *key == toggle.key {
            *step = 0;
        }
    }
    assert_eq!(
        plan(&profile, &flipped, &raw).sets,
        vec![(toggle.key.clone(), 0)]
    );
    assert_eq!(matches(&profile, &flipped, &raw), Some(false));
}
