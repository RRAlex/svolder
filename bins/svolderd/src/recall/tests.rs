// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use svolder_profiles::Registry;

fn gungnir() -> Profile {
    Registry::builtin()
        .expect("the built-in profiles load")
        .get("gungnir2-v2")
        .expect("gungnir2-v2")
        .clone()
}

fn fitting(profile: &Profile) -> Vec<Preset> {
    let eq = profile.eq.as_ref().expect("a tone control");
    preset::fitting(eq, &[])
        .into_iter()
        .map(|(preset, _)| preset)
        .collect()
}

#[test]
fn a_name_is_found_without_regard_to_case_or_the_blanks_round_it() {
    let profile = gungnir();
    let found = plan("Desk", &profile, &fitting(&profile), "  wARM ", |_| None)
        .unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(found.preset.name, "Warm");
    assert!(
        !found.recall.commands.is_empty(),
        "nothing is read, so every step is written"
    );
}

#[test]
fn an_unknown_name_lists_the_presets_that_fit_the_device() {
    let profile = gungnir();
    let Err(why) = plan("Desk", &profile, &fitting(&profile), "Nope", |_| None) else {
        panic!("planned a preset that is not there");
    };
    assert!(why.starts_with("no preset named \"Nope\" fits Desk; the presets are: "));
    assert!(why.contains("Warm"), "{why}");
}

#[test]
fn the_plan_is_the_one_the_preset_makes_from_the_same_readings() {
    let profile = gungnir();
    let fitting = fitting(&profile);
    let warm = fitting.iter().find(|p| p.name == "Warm").expect("Warm");
    let recalled = plan("Desk", &profile, &fitting, "Warm", |_| None).expect("a plan");
    let direct = warm.eq_writes(&profile, |_| None).expect("a plan");
    assert_eq!(recalled.recall, direct);
}

#[test]
fn a_stop_partway_says_how_many_writes_went_through() {
    assert_eq!(
        stopped_after(2, 7, "the device said no"),
        "stopped after 2 of 7 writes: the device said no"
    );
}

#[test]
fn every_kind_of_recall_command_is_one_actor_command() {
    let band = command(&EqCommand::Band {
        band: 3,
        param: "g",
        value: -2.5,
    });
    assert!(matches!(
        band,
        Command::EqSet { band: 3, ref param, value, no_wait: false } if param == "g" && value == -2.5
    ));
    assert!(matches!(
        command(&EqCommand::Mode("eq")),
        Command::EqMode { ref mode, no_wait: false } if mode == "eq"
    ));
    let named = command(&EqCommand::Loudness {
        key: Some("loudness".into()),
        id: FunctionId::two(14, 5),
        raw: 40,
    });
    assert!(matches!(
        named,
        Command::Set { ref key, ref value, step: 0, no_wait: false } if key == "loudness" && value == "40"
    ));
    let raw = command(&EqCommand::Loudness {
        key: None,
        id: FunctionId::two(14, 5),
        raw: 40,
    });
    assert!(matches!(
        raw,
        Command::Raw { ref id, ref value_type, ref value, no_wait: false }
            if id == "14.5" && value_type == "u8" && value == "40"
    ));
}

fn loki() -> Profile {
    Registry::builtin()
        .expect("the built-in profiles load")
        .get("lokimax")
        .expect("lokimax")
        .clone()
}

#[test]
fn a_setting_is_planned_from_the_readings_and_found_by_any_spelling() {
    let profile = loki();
    let applied = plan_setting("Desk", &profile, " bass BOOST ", |_| None).expect("a plan");
    assert_eq!(applied.name, "Bass Boost");
    assert!(!applied.writes.is_empty());
}

#[test]
fn a_setting_a_device_has_no_name_for_lists_the_ones_it_has() {
    let why = plan_setting("Desk", &loki(), "Nope", |_| None).expect_err("no such setting");
    assert!(why.starts_with("no built-in setting named Nope; the settings are: Flat, "));
}

#[test]
fn a_device_without_settings_is_named_as_the_daemon_knows_it() {
    let why = plan_setting("Desk", &gungnir(), "Flat", |_| None).expect_err("no settings");
    assert!(why.starts_with("Desk has no device presets"), "{why}");
}

#[test]
fn a_setting_write_is_a_waited_raw_u8_on_the_id_the_table_names() {
    let write = SettingWrite {
        key: "band1".into(),
        id: FunctionId::two(6, 1),
        raw: 28,
    };
    assert!(matches!(
        setting_command(&write),
        Command::Raw { ref id, ref value_type, ref value, no_wait: false }
            if id == "6.1" && value_type == "u8" && value == "28"
    ));
}
