// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_proto::Value;

use super::*;
use crate::registry::Registry;
use crate::test_support::builtin;

/// Every built-in profile with a volume control.
const WITH_VOLUME: &[&str] = &[
    "bifrost3",
    "eitr2",
    "freya",
    "freya2",
    "gungnir2-v2",
    "kara",
    "lyr5",
    "mesh-eq-3",
    "mesh-eq-5",
    "meshcard",
    "mimir",
    "saga2",
    "yggdrasil",
];

#[test]
fn the_volume_is_found_on_exactly_these_profiles() {
    let registry = Registry::builtin().expect("built-in profiles");
    let mut found = Vec::new();
    for name in registry.names() {
        let p = registry.get(name).unwrap();
        for c in &p.controls {
            let by_key = c.key.eq_ignore_ascii_case("volume");
            assert_eq!(is_volume(c), by_key, "{name}: {}", c.key);
        }
        if volume(p).is_some() {
            found.push(name);
        }
    }
    assert_eq!(found, WITH_VOLUME);
}

/// One line per built-in volume and balance control: its caps' results.
fn cap_lines() -> String {
    let registry = Registry::builtin().expect("built-in profiles");
    let mut out = String::new();
    for name in registry.names() {
        let p = registry.get(name).unwrap();
        for c in &p.controls {
            if c.kind != ControlKind::Level || !(is_volume(c) || c.key == "balance") {
                continue;
            }
            let (lo, hi) = c.range.unwrap();
            let mut line = format!("{name} {} {lo}..{hi}", c.key);
            line += &format!(" lo={:?} hi={:?}", step_value(c, lo), step_value(c, hi));
            let floor = step_value(c, lo).unwrap_or(0.0);
            for cap in [0.0, -0.04, -9.95, -30.0, floor] {
                line += &format!(" | {cap}: {:?}", capped_step(c, cap));
            }
            out += &line;
            out.push('\n');
        }
    }
    out
}

#[test]
fn the_step_arithmetic_over_every_built_in_range() {
    let lines = cap_lines();
    assert_eq!(lines, CAP_LINES);
}

/// One line per built-in level: its origin.
fn origin_lines() -> String {
    let registry = Registry::builtin().expect("built-in profiles");
    let mut out = String::new();
    for name in registry.names() {
        for c in &registry.get(name).unwrap().controls {
            if c.kind == ControlKind::Level {
                out += &format!("{name} {} {}\n", c.key, origin(c));
            }
        }
    }
    out
}

#[test]
fn the_origin_of_every_built_in_level() {
    let lines = origin_lines();
    assert_eq!(lines, ORIGIN_LINES);
}

const CAP_LINES: &str = "\
bifrost3 volume 0..127 lo=Some(-inf) hi=Some(0.0) | 0: Some(127) | -0.04: Some(126) | -9.95: Some(97) | -30: Some(55) | -inf: Some(0)
bifrost3 balance 1..127 lo=Some(-63.0) hi=Some(63.0) | 0: Some(64) | -0.04: Some(63) | -9.95: Some(54) | -30: Some(34) | -63: Some(1)
eitr2 volume 0..127 lo=Some(-inf) hi=Some(0.0) | 0: Some(127) | -0.04: Some(126) | -9.95: Some(97) | -30: Some(55) | -inf: Some(0)
eitr2 balance 1..127 lo=Some(-63.0) hi=Some(63.0) | 0: Some(64) | -0.04: Some(63) | -9.95: Some(54) | -30: Some(34) | -63: Some(1)
freya volume 0..127 lo=Some(-80.0) hi=Some(0.0) | 0: Some(127) | -0.04: Some(126) | -9.95: Some(111) | -30: Some(79) | -80: Some(0)
freya2 volume 0..127 lo=Some(-80.0) hi=Some(0.0) | 0: Some(127) | -0.04: Some(126) | -9.95: Some(111) | -30: Some(79) | -80: Some(0)
freya2 balance 0..200 lo=Some(-100.0) hi=Some(100.0) | 0: Some(100) | -0.04: Some(99) | -9.95: Some(90) | -30: Some(70) | -100: Some(0)
gungnir2-v2 volume 0..127 lo=Some(-inf) hi=Some(0.0) | 0: Some(127) | -0.04: Some(126) | -9.95: Some(97) | -30: Some(55) | -inf: Some(0)
gungnir2-v2 balance 1..127 lo=Some(-63.0) hi=Some(63.0) | 0: Some(64) | -0.04: Some(63) | -9.95: Some(54) | -30: Some(34) | -63: Some(1)
kara volume 0..127 lo=Some(-80.0) hi=Some(0.0) | 0: Some(127) | -0.04: Some(126) | -9.95: Some(111) | -30: Some(79) | -80: Some(0)
lyr5 volume 0..63 lo=Some(-64.0) hi=Some(0.0) | 0: Some(63) | -0.04: Some(62) | -9.95: Some(53) | -30: Some(33) | -64: Some(0)
mesh-eq-3 volume 0..127 lo=Some(-inf) hi=Some(0.0) | 0: Some(127) | -0.04: Some(126) | -9.95: Some(97) | -30: Some(55) | -inf: Some(0)
mesh-eq-3 balance 1..127 lo=Some(-63.0) hi=Some(63.0) | 0: Some(64) | -0.04: Some(63) | -9.95: Some(54) | -30: Some(34) | -63: Some(1)
mesh-eq-5 volume 0..127 lo=Some(-inf) hi=Some(0.0) | 0: Some(127) | -0.04: Some(126) | -9.95: Some(97) | -30: Some(55) | -inf: Some(0)
mesh-eq-5 balance 1..127 lo=Some(-63.0) hi=Some(63.0) | 0: Some(64) | -0.04: Some(63) | -9.95: Some(54) | -30: Some(34) | -63: Some(1)
meshcard volume 0..127 lo=Some(-inf) hi=Some(0.0) | 0: Some(127) | -0.04: Some(126) | -9.95: Some(97) | -30: Some(55) | -inf: Some(0)
meshcard balance 1..127 lo=Some(-63.0) hi=Some(63.0) | 0: Some(64) | -0.04: Some(63) | -9.95: Some(54) | -30: Some(34) | -63: Some(1)
mimir volume 0..127 lo=Some(-inf) hi=Some(0.0) | 0: Some(127) | -0.04: Some(126) | -9.95: Some(97) | -30: Some(55) | -inf: Some(0)
mimir balance 1..127 lo=Some(-63.0) hi=Some(63.0) | 0: Some(64) | -0.04: Some(63) | -9.95: Some(54) | -30: Some(34) | -63: Some(1)
saga2 volume 0..63 lo=Some(-64.0) hi=Some(0.0) | 0: Some(63) | -0.04: Some(62) | -9.95: Some(53) | -30: Some(33) | -64: Some(0)
yggdrasil volume 0..127 lo=Some(-inf) hi=Some(0.0) | 0: Some(127) | -0.04: Some(126) | -9.95: Some(97) | -30: Some(55) | -inf: Some(0)
yggdrasil balance 1..127 lo=Some(-63.0) hi=Some(63.0) | 0: Some(64) | -0.04: Some(63) | -9.95: Some(54) | -30: Some(34) | -63: Some(1)
\
";
const ORIGIN_LINES: &str = "\
bifrost3 volume 0
bifrost3 balance 64
eitr2 volume 0
eitr2 balance 64
freya volume 0
freya2 volume 0
freya2 balance 100
gungnir2-v2 volume 0
gungnir2-v2 balance 64
kara volume 0
lokimax band1 0
lokimax band2 0
lokimax band3 0
lokimax band4 0
lokimax band5 0
lokimax band6 0
lyr5 volume 0
mesh-eq-3 volume 0
mesh-eq-3 balance 64
mesh-eq-5 volume 0
mesh-eq-5 balance 64
meshcard volume 0
meshcard balance 64
mimir volume 0
mimir balance 64
saga2 volume 0
yggdrasil volume 0
yggdrasil balance 64
";

#[test]
fn the_volume_cap_applies_to_the_volume_alone() {
    let p = builtin("gungnir2-v2");
    let volume = p.control_by_key("volume").unwrap();
    let balance = p.control_by_key("balance").unwrap();
    let mute = p.control_by_key("mute").unwrap();
    assert_eq!(volume_cap(volume, Some(-10.0)), Some(-10.0));
    assert_eq!(volume_cap(volume, None), None);
    assert_eq!(volume_cap(balance, Some(-10.0)), None);
    assert_eq!(volume_cap(mute, Some(-10.0)), None);
    assert!(is_volume(volume) && !is_volume(balance) && !is_volume(mute));
}

#[test]
fn a_volume_keyed_control_that_is_not_a_level_is_not_the_volume() {
    let mut p = builtin("gungnir2-v2");
    let mut odd = p.control_by_key("mute").unwrap().clone();
    odd.key = "Volume".into();
    assert!(!is_volume(&odd));
    p.controls.retain(|c| c.key != "volume");
    p.controls.insert(0, odd);
    assert!(volume(&p).is_none());
}

#[test]
fn a_relative_step_refuses_before_it_looks_for_a_reading() {
    let p = builtin("gungnir2-v2");
    let words = |key, current| {
        relative_step(&p, key, current, 1)
            .map(|m| m.map(|m| m.value))
            .map_err(|e| e.to_string())
    };
    assert!(
        words("nope", Some(5))
            .unwrap_err()
            .starts_with("no control named nope; this device has: ")
    );
    assert!(
        words("nope", None)
            .unwrap_err()
            .starts_with("no control named nope; ")
    );
    for current in [None, Some(1)] {
        assert_eq!(
            words("mute", current).unwrap_err(),
            "control mute has no range to step through"
        );
    }
    // A level with no range is turned away the same way, with or without a reading.
    let mut rangeless = p.clone();
    rangeless
        .controls
        .iter_mut()
        .find(|c| c.key == "volume")
        .expect("the profile has a volume")
        .range = None;
    for current in [None, Some(63)] {
        let refused = relative_step(&rangeless, "volume", current, 1).unwrap_err();
        assert_eq!(
            refused.to_string(),
            "control volume has no range to step through"
        );
    }
    assert_eq!(words("volume", None), Ok(None));
    assert_eq!(words("volume", Some(63)), Ok(Some(Value::U8(64))));
    assert_eq!(words("volume", Some(127)), Ok(Some(Value::U8(127))));
}

const VOLUME_RANGE: (i64, i64) = (0, 127);

#[test]
fn a_step_with_no_last_goes_on_from_the_reading() {
    assert_eq!(next_step(None, 63, 1, VOLUME_RANGE), 64);
    assert_eq!(next_step(None, 63, -1, VOLUME_RANGE), 62);
    assert_eq!(next_step(None, 63, 4, VOLUME_RANGE), 67);
}

#[test]
fn a_step_after_an_absolute_last_below_the_reading_goes_on_from_it() {
    assert_eq!(next_step(Some((50, false)), 120, 1, VOLUME_RANGE), 51);
    assert_eq!(next_step(Some((50, false)), 120, -1, VOLUME_RANGE), 49);
}

#[test]
fn an_absolute_last_within_two_steps_above_the_reading_is_gone_on_from() {
    assert_eq!(next_step(Some((65, false)), 63, 1, VOLUME_RANGE), 66);
    assert_eq!(next_step(Some((65, false)), 63, 2, VOLUME_RANGE), 67);
}

#[test]
fn an_absolute_last_more_than_two_steps_above_the_reading_falls_back_to_the_reading() {
    assert_eq!(next_step(Some((66, false)), 63, 1, VOLUME_RANGE), 64);
    assert_eq!(next_step(Some((120, false)), 63, 1, VOLUME_RANGE), 64);
}

#[test]
fn a_chained_last_is_held_at_two_steps_above_the_reading() {
    assert_eq!(next_step(Some((64, true)), 63, 1, VOLUME_RANGE), 65);
    assert_eq!(next_step(Some((65, true)), 63, 1, VOLUME_RANGE), 66);
    assert_eq!(next_step(Some((66, true)), 63, 1, VOLUME_RANGE), 66);
    assert_eq!(next_step(Some((90, true)), 63, 1, VOLUME_RANGE), 66);
}

#[test]
fn a_lowering_is_never_above_the_reading_less_its_step() {
    assert_eq!(next_step(Some((101, true)), 100, -1, VOLUME_RANGE), 99);
    assert_eq!(next_step(Some((101, false)), 100, -1, VOLUME_RANGE), 99);
    assert_eq!(next_step(Some((98, true)), 100, -1, VOLUME_RANGE), 97);
}

#[test]
fn a_step_stays_inside_the_range() {
    assert_eq!(next_step(None, 127, 1, VOLUME_RANGE), 127);
    assert_eq!(next_step(None, 0, -1, VOLUME_RANGE), 0);
    assert_eq!(next_step(Some((127, true)), 126, 1, VOLUME_RANGE), 127);
    assert_eq!(next_step(Some((0, false)), 5, -9, VOLUME_RANGE), 0);
}

#[test]
fn equal_steps_under_the_cap_go_to_the_lower_raw() {
    let mut volume = builtin("gungnir2-v2")
        .control_by_key("volume")
        .expect("a volume")
        .clone();
    volume.range = Some((0, 10));
    volume.convert = Some(crate::model::Convert::Linear {
        min: -5.0,
        max: -5.0,
    });
    assert_eq!(capped_step(&volume, 0.0), Some(0));
}

#[test]
fn stepping_an_action_key_is_refused_in_the_actions_own_words() {
    let profile = builtin("lokimax");
    assert!(matches!(
        steppable(&profile, "recall"),
        Err(ProfileError::ActionNotSteppable(ref key)) if key == "recall"
    ));
    assert!(matches!(
        relative_step(&profile, "save", Some(1), 1),
        Err(ProfileError::ActionNotSteppable(_))
    ));
}
