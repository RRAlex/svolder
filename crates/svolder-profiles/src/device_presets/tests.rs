// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::test_support::builtin;

const FLAT: [i64; 6] = [15; 6];
const SMILEY: [i64; 6] = [23, 16, 8, 8, 16, 23];

fn loki() -> Profile {
    builtin("lokimax")
}

/// A reader of the six bands: `bands[b - 1]` for the band control `6.b.3`, nothing for any other
/// id.
fn reading(bands: [Option<i64>; 6]) -> impl Fn(FunctionId) -> Option<i64> {
    move |id| match id.as_slice() {
        [6, band @ 1..=6, 3] => bands[usize::from(*band) - 1],
        _ => None,
    }
}

fn read(raw: [i64; 6]) -> impl Fn(FunctionId) -> Option<i64> {
    reading(raw.map(Some))
}

/// The bands of a plan, in the order it writes them, as (band, raw).
fn order(plan: &SettingPlan) -> Vec<(u8, i64)> {
    plan.writes
        .iter()
        .map(|write| (write.id.second().expect("a band id"), write.raw))
        .collect()
}

#[test]
fn only_a_profile_with_slots_or_settings_offers_device_presets() {
    assert!(offered(&loki()));
    assert!(!offered(&builtin("fake-amp")));
    assert!(!offered(&builtin("gungnir2-v2")));
}

#[test]
fn bass_boost_from_smiley_writes_the_cuts_before_the_boosts() {
    let plan = plan(&loki(), "Bass Boost", read(SMILEY)).unwrap();
    assert_eq!(plan.name, "Bass Boost");
    // Bass Boost is 28 21 17 15 15 15: bands 5 and 6 go down from 16 and 23; bands 1 to 4 go up.
    assert_eq!(
        order(&plan),
        [(5, 15), (6, 15), (1, 28), (2, 21), (3, 17), (4, 15)]
    );
}

#[test]
fn a_band_already_on_its_value_is_left_out() {
    // From Flat, Bass Boost leaves bands 4 to 6 where they are.
    let plan = plan(&loki(), "Bass Boost", read(FLAT)).unwrap();
    assert_eq!(order(&plan), [(1, 28), (2, 21), (3, 17)]);
}

#[test]
fn a_setting_already_on_plans_no_writes() {
    let plan = plan(&loki(), "Smiley", read(SMILEY)).unwrap();
    assert!(plan.writes.is_empty());
}

#[test]
fn an_unread_band_is_written_with_the_cuts_when_its_target_is_at_or_below_0_db() {
    // Smiley is 23 16 8 8 16 23. Nothing has been read: bands 3 and 4 (8) go with the cuts,
    // bands 2 and 5 (16, above the 0 dB step 15) and 1 and 6 with the boosts.
    let plan = plan(&loki(), "Smiley", reading([None; 6])).unwrap();
    assert_eq!(
        order(&plan),
        [(3, 8), (4, 8), (1, 23), (2, 16), (5, 16), (6, 23)]
    );
    // A target on the 0 dB step itself is a cut too: Flat writes all six, none of them boosts.
    let flat = super::plan(&loki(), "Flat", reading([None; 6])).unwrap();
    assert_eq!(flat.writes.len(), 6);
    // One unread band among read ones is written, and the read ones on their value are not.
    let mut bands = FLAT.map(Some);
    bands[2] = None;
    let plan = super::plan(&loki(), "Flat", reading(bands)).unwrap();
    assert_eq!(order(&plan), [(3, 15)]);
}

#[test]
fn a_setting_writes_the_two_id_form_its_table_names() {
    let plan = plan(&loki(), "Loudness", read(FLAT)).unwrap();
    assert!(!plan.writes.is_empty());
    for write in &plan.writes {
        let band = write.id.second().unwrap();
        assert_eq!(write.id, FunctionId::two(6, band));
        assert_eq!(write.key, format!("band{band}"));
    }
}

#[test]
fn a_setting_name_is_found_without_regard_to_case_or_blanks() {
    let profile = loki();
    for name in ["Bass Boost", "bass boost", " BASS BOOST ", "\tBass Boost\n"] {
        assert_eq!(find(&profile, name).unwrap().name, "Bass Boost", "{name:?}");
        assert_eq!(
            plan(&profile, name, read(FLAT)).unwrap().name,
            "Bass Boost",
            "{name:?}"
        );
    }
    assert!(find(&profile, "Bass  Boost").is_none());
    assert!(find(&profile, "").is_none());
}

#[test]
fn an_unknown_setting_is_refused_listing_the_settings() {
    let error = plan(&loki(), "Warm", read(FLAT)).unwrap_err();
    assert_eq!(
        error.to_string(),
        "no built-in setting named Warm; the settings are: Flat, Bass Boost, Treble Boost, \
         Smiley, Loudness"
    );
    assert!(matches!(error, ProfileError::NoSetting { ref names, .. } if names.len() == 5));
}

#[test]
fn a_device_without_built_in_settings_has_no_device_presets() {
    let dac = builtin("bifrost3");
    let error = plan(&dac, "Flat", |_| None).unwrap_err();
    assert_eq!(
        error.to_string(),
        "Bifrost 3 has no device presets; svolder eq list lists its tone-control presets"
    );
    assert!(find(&dac, "Flat").is_none());
    assert!(matching(&dac, |_| Some(15)).is_none());
    assert!(!rewrites(&dac, FunctionId::two(6, 1)));
    let without_tone = ProfileError::NoDevicePresets {
        device: "Kara".into(),
        tone_control: false,
    };
    assert_eq!(without_tone.to_string(), "Kara has no device presets");
}

#[test]
fn matching_names_the_setting_every_band_reads_and_none_while_a_band_is_unread() {
    let profile = loki();
    for setting in &profile.builtin_settings.as_ref().unwrap().settings {
        let raw: [i64; 6] = setting.raw.clone().try_into().unwrap();
        let found = matching(&profile, read(raw)).expect("a setting");
        assert_eq!(found.name, setting.name);
    }
    // A curve no setting has, and one band off a setting.
    assert!(matching(&loki(), read([15, 15, 15, 15, 15, 14])).is_none());
    assert!(matching(&loki(), read([1, 2, 3, 4, 5, 6])).is_none());
    // One unread band: none, though the other five read Flat.
    let mut bands = FLAT.map(Some);
    bands[5] = None;
    assert!(matching(&loki(), reading(bands)).is_none());
    assert!(matching(&loki(), reading([None; 6])).is_none());
}

#[test]
fn the_loki_max_recall_can_raise_a_band_to_12_db() {
    let ceiling = recall_ceiling_db(&loki()).expect("a ceiling");
    assert!((ceiling - 12.0).abs() < 1e-9, "{ceiling}");
    // A device without a recall action has no ceiling.
    assert_eq!(recall_ceiling_db(&builtin("bifrost3")), None);
}

#[test]
fn a_band_a_setting_writes_is_rewritten_by_its_control_id_and_by_its_write_id() {
    let profile = loki();
    for band in 1..=6 {
        assert!(rewrites(&profile, FunctionId::three(6, band, 3)), "{band}");
        assert!(rewrites(&profile, FunctionId::two(6, band)), "{band}");
    }
    assert!(!rewrites(&profile, FunctionId::three(6, 7, 3)));
    assert!(!rewrites(&profile, FunctionId::new(&[3]).unwrap()));
    assert!(!rewrites(&profile, FunctionId::three(7, 2, 1)));
}

#[test]
fn slots_are_named_and_parsed_by_number_or_name() {
    assert_eq!(slot_name(2), "Preset 2");
    for text in ["2", " 2 ", "Preset 2", "preset 2", " PRESET 2 "] {
        assert_eq!(parse_slot(text, 3), Some(2), "{text:?}");
    }
    for text in [
        "0", "4", "300", "-1", "+2", "", "two", "Preset", "Preset 4", "Preset2",
    ] {
        assert_eq!(parse_slot(text, 3), None, "{text:?}");
    }
}

#[test]
fn no_builtin_setting_or_action_of_the_loki_max_writes_the_mode() {
    let profile = loki();
    let mode = profile.control_by_key("mode").expect("the mode").id;
    let writes_the_mode = |id: FunctionId| id.as_slice().starts_with(mode.as_slice());

    // The table names no control that is the mode, and no id under it.
    let table = profile.builtin_settings.as_ref().unwrap();
    assert!(table.keys.iter().all(|key| key != "mode"));
    assert!(table.ids.iter().all(|id| !writes_the_mode(*id)));
    assert!(!rewrites(&profile, mode));

    // Whatever the bands read, no planned write of any setting is on the mode.
    let readers: [Box<dyn Fn(FunctionId) -> Option<i64>>; 4] = [
        Box::new(reading([None; 6])),
        Box::new(read([0; 6])),
        Box::new(read([30; 6])),
        Box::new(read(SMILEY)),
    ];
    for setting in &table.settings {
        for reader in &readers {
            let plan = plan(&profile, &setting.name, reader).unwrap();
            for write in plan.writes {
                assert!(!writes_the_mode(write.id), "{}: {}", setting.name, write.id);
                assert_ne!(write.key, "mode");
            }
        }
    }

    // No slot of any action is a command on the mode, and the recall changes no mode reading.
    assert!(!profile.actions.is_empty());
    for action in &profile.actions {
        assert!(
            action.changes.iter().all(|key| key != "mode"),
            "{}",
            action.key
        );
        for slot in 1..=action.slots {
            let command = action.command(slot).unwrap();
            assert!(!writes_the_mode(command.id), "{} {slot}", action.key);
        }
    }
}
