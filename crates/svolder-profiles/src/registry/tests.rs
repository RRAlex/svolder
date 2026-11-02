// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::meter::{LogScale, Zero};

const BASE: &str = r#"
schema = 1
name = "base"

[[control]]
id = [5]
key = "mute"
label = "Mute"
kind = "toggle"

[[control]]
id = [2]
key = "input"
label = "Input"
kind = "select"
values = { 1 = "One", 2 = "Two" }
"#;

const CHILD: &str = r#"
schema = 1
name = "child"
display = "Child"
product_id = 42
min_firmware = 2
extends = "base"

[[control]]
id = [2]
key = "input"
label = "Source"
kind = "select"
values = { 1 = "A", 2 = "B", 3 = "C" }

[[control]]
id = [10]
key = "volume"
label = "Volume"
kind = "level"
range = [0, 127]
unit = "dB"
convert = { table = "dac-attenuation-db" }

[eq]
id = 12
bands = [40, 160]
fscale = [[0.005554, 0.00544], [0.00536, 0.005435]]
range = 1.95
gain = { range = [-20, 6], zero = 98 }
q = { range = [0.5, 8.0], center = 4.3 }
"#;

const OLDER: &str = r#"
schema = 1
name = "child-v1"
product_id = 42
extends = "base"
"#;

fn registry(pairs: &[(&str, &str)]) -> Registry {
    Registry::from_sources(
        pairs
            .iter()
            .map(|(n, t)| (n.to_string(), t.to_string()))
            .collect(),
    )
    .unwrap()
}

#[test]
fn child_overrides_by_key_and_appends_new_controls() {
    let r = registry(&[("base", BASE), ("child", CHILD)]);
    let c = r.get("child").unwrap();
    assert_eq!(c.display, "Child");
    assert_eq!(c.product_id, Some(42));
    assert_eq!(c.min_firmware, 2);
    let keys: Vec<&str> = c.controls.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(keys, ["mute", "input", "volume"]);
    let input = c.control_by_key("input").unwrap();
    assert_eq!(input.label, "Source");
    assert_eq!(
        input.values,
        vec![(1, "A".into()), (2, "B".into()), (3, "C".into())]
    );
    assert_eq!(
        c.control_by_key("volume").unwrap().convert,
        Some(Convert::Table("dac-attenuation-db".into()))
    );
    assert_eq!(c.eq.as_ref().unwrap().bands.len(), 2);
    assert_eq!(c.eq.as_ref().unwrap().bands[0].fscale_high, 0.00544);
}

#[test]
fn base_defaults() {
    let r = registry(&[("base", BASE)]);
    let b = r.get("base").unwrap();
    assert_eq!(b.display, "base");
    assert_eq!(b.product_id, None);
    assert_eq!(b.min_firmware, 0);
    let mute = b.control_by_key("mute").unwrap();
    assert_eq!(mute.kind, ControlKind::Toggle);
    assert_eq!(mute.value_type, ValueType::Bool);
    assert_eq!(mute.id, FunctionId::one(5));
    assert_eq!(b.control_by_key("input").unwrap().value_type, ValueType::U8);
}

#[test]
fn selects_highest_min_firmware_not_above_device_firmware() {
    let r = registry(&[("base", BASE), ("child", CHILD), ("child-v1", OLDER)]);
    assert_eq!(r.select(42, 0).unwrap().name, "child-v1");
    assert_eq!(r.select(42, 1).unwrap().name, "child-v1");
    assert_eq!(r.select(42, 2).unwrap().name, "child");
    assert_eq!(r.select(42, 9).unwrap().name, "child");
    assert!(r.select(7, 9).is_none());
}

#[test]
fn later_source_with_same_name_merges_over_earlier() {
    let replacement = r#"
schema = 1
name = "base"
display = "Replaced"
"#;
    let r = registry(&[("base", BASE), ("base", replacement)]);
    let b = r.get("base").unwrap();
    assert_eq!(b.display, "Replaced");
    let keys: Vec<&str> = b.controls.iter().map(|c| c.key.as_str()).collect();
    assert_eq!(keys, ["mute", "input"]);
}

/// The built-in sources, with `extra` appended the way a user directory would append it.
fn builtin_plus(extra: &[(&str, &str)]) -> Registry {
    let mut sources: Vec<(String, String)> = BUILTIN
        .iter()
        .map(|(n, t)| (n.to_string(), t.to_string()))
        .collect();
    sources.extend(extra.iter().map(|(n, t)| (n.to_string(), t.to_string())));
    Registry::from_sources(sources).unwrap()
}

#[test]
fn user_file_with_a_builtin_name_merges_into_it() {
    let over = "schema = 1\nname = \"gungnir2-v2\"\n[[control]]\nid = [2]\nkey = \"input\"\nlabel = \"Input\"\nkind = \"select\"\nvalues = { 1 = \"TV\", 2 = \"Streamer\" }\n";
    let r = builtin_plus(&[("gungnir2-v2", over)]);
    let g = r.get("gungnir2-v2").unwrap();
    assert_eq!(g.product_id, Some(2));
    assert_eq!(g.min_firmware, 2);
    assert_eq!(
        g.control_by_key("input").unwrap().values,
        vec![(1, "TV".into()), (2, "Streamer".into())]
    );
    for key in ["phase", "mute", "nos", "sample_rate", "volume", "balance"] {
        assert!(g.control_by_key(key).is_some(), "{key}");
    }
    assert_eq!(r.select(2, 2).unwrap().name, "gungnir2-v2");
}

#[test]
fn the_later_source_wins_a_tie_on_product_and_firmware() {
    let mine = r#"
schema = 1
name = "aaa"
display = "Mine"
product_id = 2
min_firmware = 2
extends = "gungnir2-v2"
"#;
    let r = builtin_plus(&[("aaa", mine)]);
    assert_eq!(r.select(2, 2).unwrap().name, "aaa");
    assert_eq!(r.select(2, 2).unwrap().display, "Mine");
}

#[test]
fn rejects_two_controls_sharing_one_function_id() {
    let clash = "schema = 1\nname = \"x\"\n[[control]]\nid = [5]\nkey = \"mute\"\nlabel = \"Mute\"\nkind = \"toggle\"\n[[control]]\nid = [5]\nkey = \"silence\"\nlabel = \"Silence\"\nkind = \"toggle\"\n";
    assert!(matches!(
        Registry::from_sources(vec![("x".into(), clash.into())]),
        Err(ProfileError::Control {
            reason,
            ..
        }) if reason == "duplicate function id"
    ));
}

#[test]
fn rejects_unknown_parent_schema_and_cycles() {
    let orphan = "schema = 1\nname = \"o\"\nextends = \"missing\"\n";
    assert!(matches!(
        Registry::from_sources(vec![("o".into(), orphan.into())]),
        Err(ProfileError::UnknownParent(_, _))
    ));
    let v2 = "schema = 2\nname = \"s\"\n";
    assert!(matches!(
        Registry::from_sources(vec![("s".into(), v2.into())]),
        Err(ProfileError::Schema(_, 2))
    ));
    let a = "schema = 1\nname = \"a\"\nextends = \"b\"\n";
    let b = "schema = 1\nname = \"b\"\nextends = \"a\"\n";
    assert!(matches!(
        Registry::from_sources(vec![("a".into(), a.into()), ("b".into(), b.into())]),
        Err(ProfileError::ExtendsCycle(_))
    ));
}

#[test]
fn rejects_bad_controls() {
    let bad_id = "schema = 1\nname = \"x\"\n[[control]]\nid = []\nkey = \"k\"\nlabel = \"L\"\nkind = \"toggle\"\n";
    assert!(matches!(
        Registry::from_sources(vec![("x".into(), bad_id.into())]),
        Err(ProfileError::Proto(_))
    ));
    let bad_table = "schema = 1\nname = \"x\"\n[[control]]\nid = [1]\nkey = \"k\"\nlabel = \"L\"\nkind = \"level\"\nrange = [0, 1]\nconvert = { table = \"nope\" }\n";
    assert!(matches!(
        Registry::from_sources(vec![("x".into(), bad_table.into())]),
        Err(ProfileError::UnknownTable(_, _))
    ));
    let no_range = "schema = 1\nname = \"x\"\n[[control]]\nid = [1]\nkey = \"k\"\nlabel = \"L\"\nkind = \"level\"\n";
    assert!(matches!(
        Registry::from_sources(vec![("x".into(), no_range.into())]),
        Err(ProfileError::Control { .. })
    ));
    let bad_value_key = "schema = 1\nname = \"x\"\n[[control]]\nid = [1]\nkey = \"k\"\nlabel = \"L\"\nkind = \"select\"\nvalues = { abc = \"A\" }\n";
    assert!(matches!(
        Registry::from_sources(vec![("x".into(), bad_value_key.into())]),
        Err(ProfileError::Control { .. })
    ));
}

/// A level control whose text is `body` appended to a one-control profile header.
fn level_profile(body: &str) -> Result<Registry, ProfileError> {
    let text = format!(
        "schema = 1\nname = \"x\"\n[[control]]\nid = [1]\nkey = \"k\"\nlabel = \"L\"\nkind = \"level\"\n{body}"
    );
    Registry::from_sources(vec![("x".into(), text)])
}

#[test]
fn rejects_levels_whose_range_does_not_fit_the_value_type() {
    assert!(matches!(
        level_profile("range = [-9223372036854775808, 9223372036854775807]\nvalue_type = \"u8\"\n"),
        Err(ProfileError::Control { .. })
    ));
    assert!(matches!(
        level_profile("range = [10, 5]\n"),
        Err(ProfileError::Control { .. })
    ));
    assert!(matches!(
        level_profile("range = [0, 300]\nvalue_type = \"u8\"\n"),
        Err(ProfileError::Control { .. })
    ));
    assert!(matches!(
        level_profile("range = [0, 1]\nvalue_type = \"bool\"\n"),
        Err(ProfileError::Control { .. })
    ));
    assert!(matches!(
        level_profile("range = [0, 127]\ncenter = 200\n"),
        Err(ProfileError::Control { .. })
    ));
    let ok = level_profile("range = [-100, 100]\nvalue_type = \"i16\"\n").unwrap();
    let control = ok.get("x").unwrap().control_by_key("k").unwrap();
    assert_eq!(control.range, Some((-100, 100)));
    assert_eq!(control.value_type, ValueType::I16);
}

/// A meter control whose text is `body` appended to a one-control profile header.
fn meter_profile(body: &str) -> Result<Registry, ProfileError> {
    let text = format!(
        "schema = 1\nname = \"x\"\n[[control]]\nid = [1]\nkey = \"k\"\nlabel = \"L\"\nkind = \"meter\"\n{body}"
    );
    Registry::from_sources(vec![("x".into(), text)])
}

/// The reason a profile failed to load, for the checks below that name a field.
fn reason(result: Result<Registry, ProfileError>) -> String {
    match result {
        Err(ProfileError::Control { reason, .. }) => reason,
        other => panic!("expected a control error, got {other:?}"),
    }
}

#[test]
fn meter_parses_a_log_scale_with_a_fixed_zero_or_a_floor_to_follow() {
    let r = meter_profile("scale = { zero = 160, full = 350, class_a = 0.25 }\n").unwrap();
    let control = r.get("x").unwrap().control_by_key("k").unwrap();
    assert_eq!(
        control.log_scale,
        Some(LogScale {
            zero: Zero::Fixed(160),
            full: 350,
            class_a: 0.25
        })
    );
    let r = meter_profile("scale = { zero_from = \"floor\", full = 1000 }\n").unwrap();
    let control = r.get("x").unwrap().control_by_key("k").unwrap();
    assert_eq!(
        control.log_scale,
        Some(LogScale {
            zero: Zero::Floor("floor".into()),
            full: 1000,
            class_a: 0.0
        })
    );
}

#[test]
fn rejects_a_scale_that_is_not_on_a_meter_or_rides_with_range_floor_or_tick() {
    let text = "schema = 1\nname = \"x\"\n[[control]]\nid = [1]\nkey = \"k\"\nlabel = \"L\"\nkind = \"readout\"\nvalue_type = \"u16\"\nscale = { zero = 0, full = 10 }\n";
    assert_eq!(
        reason(Registry::from_sources(vec![("x".into(), text.into())])),
        "scale applies only to a meter"
    );
    for extra in ["range = [0, 3000]\n", "floor = 9\n", "tick = 9\n"] {
        assert_eq!(
            reason(meter_profile(&format!(
                "scale = {{ zero = 0, full = 10 }}\n{extra}"
            ))),
            "scale replaces range, floor and tick",
            "{extra}"
        );
    }
}

/// A key a scale table does not have is refused, and the error names it.
#[test]
fn rejects_a_scale_with_an_unknown_key() {
    let error = meter_profile("scale = { zero_form = 1, full = 10 }\n")
        .expect_err("an unknown scale key loaded");
    assert!(
        error.to_string().contains("zero_form"),
        "the error does not name the key: {error}"
    );
}

#[test]
fn rejects_a_scale_with_both_zeros_or_neither() {
    assert_eq!(
        reason(meter_profile(
            "scale = { zero = 1, zero_from = \"f\", full = 10 }\n"
        )),
        "scale needs one of zero and zero_from"
    );
    assert_eq!(
        reason(meter_profile("scale = { full = 10 }\n")),
        "scale needs one of zero and zero_from"
    );
}

#[test]
fn rejects_a_full_scale_a_zero_or_a_class_a_block_out_of_bounds() {
    for (body, why) in [
        (
            "scale = { zero = 0, full = 0 }\n",
            "scale's full must be above 0",
        ),
        (
            "scale = { zero_from = \"f\", full = -5 }\n",
            "scale's full must be above 0",
        ),
        (
            "scale = { zero = 10, full = 10 }\n",
            "scale's zero must be at least 0 and below full",
        ),
        (
            "scale = { zero = -1, full = 10 }\n",
            "scale's zero must be at least 0 and below full",
        ),
        (
            "scale = { zero = 0, full = 70000 }\n",
            "scale exceeds the value_type domain",
        ),
        (
            "scale = { zero = 0, full = 10, class_a = 1.0 }\n",
            "scale's class_a must be at least 0 and below 1",
        ),
        (
            "scale = { zero = 0, full = 10, class_a = -0.1 }\n",
            "scale's class_a must be at least 0 and below 1",
        ),
        (
            "scale = { zero = 0, full = 10, class_a = nan }\n",
            "scale's class_a must be at least 0 and below 1",
        ),
    ] {
        assert_eq!(reason(meter_profile(body)), why, "{body}");
    }
}

/// `zero_from` is checked on a concrete profile, resolved in full, as `only_while` is: a base
/// may name a floor only its children define.
#[test]
fn rejects_a_zero_from_that_names_no_u16_readout() {
    let product = |floor: &str| {
        format!(
            "schema = 1\nname = \"x\"\nproduct_id = 7\n[[control]]\nid = [20, 1]\nkey = \"k\"\nlabel = \"L\"\nkind = \"meter\"\nscale = {{ zero_from = \"floor\", full = 10 }}\n{floor}"
        )
    };
    let load = |text: String| Registry::from_sources(vec![("x".into(), text)]);
    assert_eq!(
        reason(load(product(""))),
        "zero_from names \"floor\", which is not a control"
    );
    let toggle = "[[control]]\nid = [20, 2]\nkey = \"floor\"\nlabel = \"F\"\nkind = \"toggle\"\n";
    assert_eq!(
        reason(load(product(toggle))),
        "zero_from names \"floor\", which is not a u16 readout"
    );
    let narrow = "[[control]]\nid = [20, 2]\nkey = \"floor\"\nlabel = \"F\"\nkind = \"readout\"\n";
    assert_eq!(
        reason(load(product(narrow))),
        "zero_from names \"floor\", which is not a u16 readout"
    );
    let right = "[[control]]\nid = [20, 2]\nkey = \"floor\"\nlabel = \"F\"\nkind = \"readout\"\nvalue_type = \"u16\"\n";
    assert!(load(product(right)).is_ok());
}

#[test]
fn meter_parses_a_floor_and_a_class_a_tick() {
    let r = meter_profile("range = [0, 3000]\nfloor = 900\ntick = 1400\n").unwrap();
    let control = r.get("x").unwrap().control_by_key("k").unwrap();
    assert_eq!(control.floor, Some(900));
    assert_eq!(control.tick, Some(1400));
}

#[test]
fn rejects_a_floor_or_tick_on_a_control_that_is_not_a_meter() {
    let text = "schema = 1\nname = \"x\"\n[[control]]\nid = [1]\nkey = \"k\"\nlabel = \"L\"\nkind = \"toggle\"\nfloor = 900\n";
    assert!(matches!(
        Registry::from_sources(vec![("x".into(), text.into())]),
        Err(ProfileError::Control { .. })
    ));
    let text = "schema = 1\nname = \"x\"\n[[control]]\nid = [1]\nkey = \"k\"\nlabel = \"L\"\nkind = \"select\"\ntick = 100\n";
    assert!(matches!(
        Registry::from_sources(vec![("x".into(), text.into())]),
        Err(ProfileError::Control { .. })
    ));
}

#[test]
fn rejects_a_tick_outside_range_and_a_floor_at_or_above_max() {
    assert!(matches!(
        meter_profile("range = [0, 3000]\ntick = 3001\n"),
        Err(ProfileError::Control { .. })
    ));
    assert!(matches!(
        meter_profile("range = [0, 3000]\ntick = -1\n"),
        Err(ProfileError::Control { .. })
    ));
    assert!(matches!(
        meter_profile("range = [0, 3000]\nfloor = 3000\n"),
        Err(ProfileError::Control { .. })
    ));
    assert!(matches!(
        meter_profile("range = [0, 3000]\nfloor = 3500\n"),
        Err(ProfileError::Control { .. })
    ));
}

/// A floor below the bottom of range would put the empty part of the bar's fill math into
/// negative territory: a reading between range's bottom and the floor would read as filling
/// past the tick from underneath.
#[test]
fn rejects_a_floor_below_the_bottom_of_range() {
    assert!(matches!(
        meter_profile("range = [100, 3000]\nfloor = 99\n"),
        Err(ProfileError::Control { .. })
    ));
    let equal = meter_profile("range = [100, 3000]\nfloor = 100\n").unwrap();
    let control = equal.get("x").unwrap().control_by_key("k").unwrap();
    assert_eq!(control.floor, Some(100));
}

/// A tick below the floor marks a spot on the bar the floor keeps empty, and every
/// reading above it would be reported hot while the bar shows nothing.
#[test]
fn rejects_a_tick_below_the_floor() {
    assert!(matches!(
        meter_profile("range = [0, 3000]\nfloor = 900\ntick = 899\n"),
        Err(ProfileError::Control { .. })
    ));
    let equal = meter_profile("range = [0, 3000]\nfloor = 900\ntick = 900\n").unwrap();
    let control = equal.get("x").unwrap().control_by_key("k").unwrap();
    assert_eq!(control.tick, Some(900));
}

/// Both fields are raw values drawn as a position on the bar `range` sets out, so a
/// meter with neither has nowhere to draw them.
#[test]
fn rejects_a_floor_or_a_tick_on_a_meter_with_no_range() {
    assert!(matches!(
        meter_profile("floor = 900\n"),
        Err(ProfileError::Control { .. })
    ));
    assert!(matches!(
        meter_profile("tick = 1400\n"),
        Err(ProfileError::Control { .. })
    ));
    // A meter with no floor and no tick is drawn from `range` alone, and needs no range
    // either: a readout of the raw value is all it promises.
    assert!(meter_profile("").is_ok());
}

#[test]
fn accepts_a_tick_at_either_end_of_range_and_a_floor_just_below_max() {
    let low = meter_profile("range = [0, 3000]\ntick = 0\n").unwrap();
    assert_eq!(
        low.get("x").unwrap().control_by_key("k").unwrap().tick,
        Some(0)
    );
    let high = meter_profile("range = [0, 3000]\ntick = 3000\n").unwrap();
    assert_eq!(
        high.get("x").unwrap().control_by_key("k").unwrap().tick,
        Some(3000)
    );
    let floor = meter_profile("range = [0, 3000]\nfloor = 2999\n").unwrap();
    assert_eq!(
        floor.get("x").unwrap().control_by_key("k").unwrap().floor,
        Some(2999)
    );
}

#[test]
fn eq_shape_must_match() {
    let bad = r#"
schema = 1
name = "x"
[eq]
id = 12
bands = [40, 160]
fscale = [[0.005554, 0.00544]]
range = 1.95
gain = { range = [-20, 6], zero = 98 }
q = { range = [0.5, 8.0], center = 4.3 }
"#;
    assert!(matches!(
        Registry::from_sources(vec![("x".into(), bad.into())]),
        Err(ProfileError::EqShape(_))
    ));
}

#[test]
fn eq_q_range_must_start_above_zero() {
    for low in ["0.0", "-1.0", "nan"] {
        let bad = format!(
            r#"
schema = 1
name = "x"
[eq]
id = 12
bands = [40]
fscale = [[0.005554, 0.00544]]
range = 1.95
gain = {{ range = [-20, 6], zero = 98 }}
q = {{ range = [{low}, 8.0], center = 4.3 }}
"#
        );
        assert!(
            matches!(
                Registry::from_sources(vec![("x".into(), bad)]),
                Err(ProfileError::EqQRange(ref name)) if name == "x"
            ),
            "a q range from {low} is refused"
        );
    }
}

#[test]
fn eq_ranges_must_be_finite_and_rising() {
    let profile = |gain: &str, q: &str, fscale: &str| {
        format!(
            r#"
schema = 1
name = "x"
[eq]
id = 12
bands = [40]
fscale = [{fscale}]
range = 1.95
gain = {{ range = {gain}, zero = 98 }}
q = {{ range = {q}, center = 4.3 }}
"#
        )
    };
    let good = ("[-20, 6]", "[0.5, 8.0]", "[0.005554, 0.00544]");
    assert!(Registry::from_sources(vec![("x".into(), profile(good.0, good.1, good.2))]).is_ok());
    let bad = [
        ("q reversed", (good.0, "[8.0, 0.5]", good.2)),
        ("q nan", (good.0, "[0.5, nan]", good.2)),
        ("q equal", (good.0, "[2.0, 2.0]", good.2)),
        ("gain reversed", ("[6, -20]", good.1, good.2)),
        ("gain nan", ("[-20, nan]", good.1, good.2)),
        ("gain infinite", ("[-20, inf]", good.1, good.2)),
        ("fscale negative", (good.0, good.1, "[-0.005554, -0.00544]")),
        ("fscale nan", (good.0, good.1, "[nan, 0.00544]")),
        ("fscale zero", (good.0, good.1, "[0.0, 0.0]")),
    ];
    for (name, (gain, q, fscale)) in bad {
        let result = Registry::from_sources(vec![("x".into(), profile(gain, q, fscale))]);
        assert!(
            matches!(result, Err(ProfileError::EqRange { .. })),
            "{name}: {result:?}"
        );
    }
}

#[test]
fn generic_profile_names_the_product() {
    let g = Registry::generic(10);
    assert_eq!(g.display, "Bifrost 3");
    assert_eq!(g.product_id, Some(10));
    assert!(g.controls.is_empty());
    assert_eq!(g.family, Family::Dac);
    assert_eq!(Registry::generic(99).display, "Product 99");
}

#[test]
fn a_family_is_inherited_through_extends_and_defaults_to_dac() {
    let reg = registry(&[
        ("base", "schema = 1\nname = \"base\"\nfamily = \"amp\"\n"),
        (
            "child",
            "schema = 1\nname = \"child\"\nextends = \"base\"\n",
        ),
        ("orphan", "schema = 1\nname = \"orphan\"\n"),
    ]);
    assert_eq!(reg.get("child").unwrap().family, Family::Amp);
    assert_eq!(reg.get("orphan").unwrap().family, Family::Dac);
}

#[test]
fn a_child_may_override_its_family_and_a_bad_family_is_an_error() {
    let reg = registry(&[
        ("base", "schema = 1\nname = \"base\"\nfamily = \"dac\"\n"),
        (
            "child",
            "schema = 1\nname = \"child\"\nextends = \"base\"\nfamily = \"preamp\"\n",
        ),
    ]);
    assert_eq!(reg.get("child").unwrap().family, Family::Preamp);
    let bad = vec![(
        "bad".to_string(),
        "schema = 1\nname = \"bad\"\nfamily = \"toaster\"\n".to_string(),
    )];
    let err = Registry::from_sources(bad).unwrap_err();
    assert!(
        matches!(err, ProfileError::Family(ref p, ref f) if p == "bad" && f == "toaster"),
        "{err}"
    );
}

#[test]
fn every_built_in_profile_has_the_family_its_root_declares() {
    let reg = Registry::builtin().unwrap();
    assert_eq!(reg.get("bifrost3").unwrap().family, Family::Dac);
    assert_eq!(reg.get("tyr").unwrap().family, Family::Amp);
    assert_eq!(reg.get("lyr5").unwrap().family, Family::Preamp);
    assert_eq!(reg.get("freya").unwrap().family, Family::Preamp);
    assert_eq!(reg.get("skoll").unwrap().family, Family::Preamp);
    assert_eq!(reg.get("lokimax").unwrap().family, Family::Preamp);
}

#[test]
fn user_dir_adds_and_replaces() {
    /// The user directory, removed with everything in it when the case lets go of it,
    /// however the case ended.
    struct DirGuard(std::path::PathBuf);
    impl Drop for DirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let guard = DirGuard(
        std::env::temp_dir().join(format!("svolder-profiles-test-{}", std::process::id())),
    );
    let dir = &guard.0;
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("base.toml"), BASE).unwrap();
    std::fs::write(dir.join("child.toml"), CHILD).unwrap();
    std::fs::write(dir.join("notes.txt"), "ignored").unwrap();
    let r = Registry::with_user_dir(dir).unwrap();
    assert!(r.get("child").is_some());
    std::fs::remove_dir_all(dir).unwrap();
    assert!(Registry::with_user_dir(dir).is_ok());
}

fn only_while_source(target: &str, target_def: &str) -> String {
    format!(
        "schema = 1\nname = \"x\"\nproduct_id = 99\n\
         [[control]]\nid = [4]\nkey = \"warming_up\"\nlabel = \"Warming up\"\nkind = \"readout\"\n\
         value_type = \"bool\"\nonly_while = \"{target}\"\n{target_def}"
    )
}

#[test]
fn only_while_resolves_to_a_bool_control() {
    let source = only_while_source(
        "power",
        "[[control]]\nid = [10]\nkey = \"power\"\nlabel = \"Power\"\nkind = \"toggle\"\n",
    );
    let r = Registry::from_sources(vec![("x".into(), source)]).unwrap();
    let control = r.get("x").unwrap().control_by_key("warming_up").unwrap();
    assert_eq!(control.only_while.as_deref(), Some("power"));
    assert!(!control.auto_off);
}

#[test]
fn only_while_naming_an_absent_key_is_a_load_error_naming_both() {
    let source = only_while_source("power", "");
    match Registry::from_sources(vec![("x".into(), source)]) {
        Err(ProfileError::Control { key, reason, .. }) => {
            assert_eq!(key, "warming_up");
            assert!(reason.contains("power"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn only_while_naming_a_non_bool_control_is_a_load_error() {
    let source = only_while_source(
        "input",
        "[[control]]\nid = [2]\nkey = \"input\"\nlabel = \"Input\"\nkind = \"select\"\n",
    );
    match Registry::from_sources(vec![("x".into(), source)]) {
        Err(ProfileError::Control { key, reason, .. }) => {
            assert_eq!(key, "warming_up");
            assert!(reason.contains("input"), "{reason}");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn only_while_and_auto_off_apply_only_to_a_bool_readout() {
    for extra in ["only_while = \"mute\"", "auto_off = true"] {
        let source = format!(
            "schema = 1\nname = \"x\"\n\
             [[control]]\nid = [5]\nkey = \"mute\"\nlabel = \"Mute\"\nkind = \"toggle\"\n{extra}\n"
        );
        assert!(
            matches!(
                Registry::from_sources(vec![("x".into(), source)]),
                Err(ProfileError::Control { .. })
            ),
            "{extra}"
        );
    }
    let source = "schema = 1\nname = \"x\"\n[[control]]\nid = [6]\nkey = \"rate\"\nlabel = \"Rate\"\nkind = \"readout\"\nauto_off = true\n";
    assert!(matches!(
        Registry::from_sources(vec![("x".into(), source.into())]),
        Err(ProfileError::Control { .. })
    ));
}

#[test]
fn a_base_may_name_an_only_while_key_only_its_child_defines() {
    let base = "schema = 1\nname = \"base\"\n\
        [[control]]\nid = [4]\nkey = \"warming_up\"\nlabel = \"Warming up\"\nkind = \"readout\"\n\
        value_type = \"bool\"\nonly_while = \"power\"\n";
    let child = "schema = 1\nname = \"child\"\nproduct_id = 99\nextends = \"base\"\n\
        [[control]]\nid = [10]\nkey = \"power\"\nlabel = \"Power\"\nkind = \"toggle\"\n";
    let r = Registry::from_sources(vec![
        ("base".into(), base.into()),
        ("child".into(), child.into()),
    ])
    .unwrap();
    assert!(
        r.get("child")
            .unwrap()
            .control_by_key("warming_up")
            .is_some()
    );
    let bare = "schema = 1\nname = \"bare\"\nproduct_id = 98\nextends = \"base\"\n";
    match Registry::from_sources(vec![
        ("base".into(), base.into()),
        ("bare".into(), bare.into()),
    ]) {
        Err(ProfileError::Control { profile, key, .. }) => {
            assert_eq!(profile, "bare");
            assert_eq!(key, "warming_up");
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn output_gain_applies_only_to_a_select_and_mutes_output_only_to_a_toggle() {
    for extra in [
        "kind = \"toggle\"\noutput_gain = true",
        "kind = \"select\"\nmutes_output = true",
        "kind = \"select\"\npowers_output = true",
        "kind = \"level\"\nrange = [0, 9]\noutput_gain = true",
        "kind = \"readout\"\nasks_on_change = true",
    ] {
        let source = format!(
            "schema = 1\nname = \"x\"\n[[control]]\nid = [5]\nkey = \"k\"\nlabel = \"K\"\n{extra}\n"
        );
        assert!(
            matches!(
                Registry::from_sources(vec![("x".into(), source)]),
                Err(ProfileError::Control { .. })
            ),
            "{extra}"
        );
    }
    let source = "schema = 1\nname = \"x\"\n[[control]]\nid = [5]\nkey = \"gain\"\nlabel = \"Gain\"\nkind = \"select\"\nvalues = { 1 = \"Low\", 2 = \"High\" }\noutput_gain = true\n[[control]]\nid = [6]\nkey = \"mute\"\nlabel = \"Mute\"\nkind = \"toggle\"\nmutes_output = true\n";
    let r = Registry::from_sources(vec![("x".into(), source.into())]).unwrap();
    let p = r.get("x").unwrap();
    assert!(p.control_by_key("gain").unwrap().output_gain);
    assert!(p.control_by_key("mute").unwrap().mutes_output);
    assert!(!p.control_by_key("gain").unwrap().mutes_output);
}

/// Every built-in profile marks the controls that raise the output the way the MCP tools ask
/// about: each gain select and each mute toggle, and no other control.
#[test]
fn the_built_in_profiles_mark_their_gain_selects_and_mutes() {
    let r = Registry::builtin().unwrap();
    for name in r.names() {
        let p = r.get(name).unwrap();
        for c in &p.controls {
            let gain_like = c.kind == ControlKind::Select
                && (c.key == "gain"
                    || (c.key == "mode"
                        && c.values
                            .iter()
                            .any(|(_, v)| v.contains("Gain") || v == "Passive")));
            assert_eq!(c.output_gain, gain_like, "{name}: {}", c.key);
            let mute_like =
                c.kind == ControlKind::Toggle && (c.key == "mute" || c.key == "headphone_mute");
            assert_eq!(c.mutes_output, mute_like, "{name}: {}", c.key);
            let power_like = c.kind == ControlKind::Toggle && c.key == "power";
            assert_eq!(c.powers_output, power_like, "{name}: {}", c.key);
            let asks = matches!(
                (name, c.key.as_str()),
                ("lyr5", "preout") | ("lokimax", "mode")
            );
            assert_eq!(c.asks_on_change, asks, "{name}: {}", c.key);
        }
    }
}

#[test]
fn a_control_keyed_like_a_bridge_entity_is_a_load_error() {
    for reserved in RESERVED_KEYS {
        for key in [reserved.to_string(), reserved.to_ascii_uppercase()] {
            let source = format!(
                "schema = 1\nname = \"x\"\n\
                 [[control]]\nid = [2]\nkey = \"{key}\"\nlabel = \"Mine\"\nkind = \"select\"\n"
            );
            match Registry::from_sources(vec![("x".into(), source)]) {
                Err(ProfileError::Control {
                    key: named, reason, ..
                }) => {
                    assert_eq!(named, key);
                    assert!(reason.contains("reserved"), "{reason}");
                    assert!(reason.contains("MQTT bridge"), "{reason}");
                }
                other => panic!("{key}: {other:?}"),
            }
        }
    }
}

#[test]
fn the_reserved_keys_are_the_bridges_own() {
    assert_eq!(
        RESERVED_KEYS,
        [
            "link",
            "signal",
            "firmware",
            "dsp_firmware",
            "identify",
            "eq_mode",
            "eq_preset",
            "eq_loudness"
        ]
    );
}

#[test]
fn firmware_subsystems_parse_and_inherit() {
    let parent = "schema = 1\nname = \"parent\"\nfirmware_subsystems = [1, 2]\n";
    let silent = "schema = 1\nname = \"silent\"\nextends = \"parent\"\n";
    let own = "schema = 1\nname = \"own\"\nextends = \"parent\"\nfirmware_subsystems = [3]\n";
    let none = "schema = 1\nname = \"none\"\nextends = \"parent\"\nfirmware_subsystems = []\n";
    let reg = registry(&[
        ("parent", parent),
        ("silent", silent),
        ("own", own),
        ("none", none),
    ]);
    assert_eq!(reg.get("parent").unwrap().firmware_subsystems, [1, 2]);
    assert_eq!(reg.get("silent").unwrap().firmware_subsystems, [1, 2]);
    assert_eq!(reg.get("own").unwrap().firmware_subsystems, [3]);
    assert!(reg.get("none").unwrap().firmware_subsystems.is_empty());
    assert!(
        registry(&[("base", BASE)])
            .get("base")
            .unwrap()
            .firmware_subsystems
            .is_empty()
    );
}

/// `identify` is true unless a profile says false, and a child inherits its parent's.
#[test]
fn identify_defaults_to_true_and_inherits() {
    let parent = "schema = 1\nname = \"parent\"\nidentify = false\n";
    let silent = "schema = 1\nname = \"silent\"\nextends = \"parent\"\n";
    let own = "schema = 1\nname = \"own\"\nextends = \"parent\"\nidentify = true\n";
    let reg = registry(&[
        ("base", BASE),
        ("parent", parent),
        ("silent", silent),
        ("own", own),
    ]);
    assert!(reg.get("base").unwrap().identify);
    assert!(!reg.get("parent").unwrap().identify);
    assert!(!reg.get("silent").unwrap().identify);
    assert!(reg.get("own").unwrap().identify);
    assert!(Registry::generic(2).identify);
}

/// The app offers no identify on a Mesh Card, so no built-in profile of it does; every other
/// built-in does.
#[test]
fn only_the_mesh_card_offers_no_identify() {
    let builtin = Registry::builtin().unwrap();
    let without: Vec<&str> = builtin
        .names()
        .into_iter()
        .filter(|name| !builtin.get(name).unwrap().identify)
        .collect();
    assert_eq!(without, ["meshcard"]);
}

#[test]
fn the_built_in_profiles_name_the_firmware_subsystems_the_app_asks() {
    let builtin = Registry::builtin().unwrap();
    let asked = |name: &str| builtin.get(name).unwrap().firmware_subsystems.clone();
    for name in ["yggdrasil", "gungnir2-v1", "gungnir2-v2", "bifrost3"] {
        assert_eq!(asked(name), [1, 2], "{name}");
    }
    for name in ["mimir", "eitr2", "meshcard"] {
        assert_eq!(asked(name), [1], "{name}");
    }
    for name in [
        "freya",
        "kara",
        "tyr",
        "wotan",
        "skoll",
        "mesh-eq-3",
        "mesh-eq-5",
        "dac-common",
    ] {
        assert!(asked(name).is_empty(), "{name}");
    }
}

#[test]
fn a_firmware_subsystem_list_that_cannot_be_asked_is_a_load_error() {
    let load = |list: &str| {
        Registry::from_sources(vec![(
            "x".into(),
            format!("schema = 1\nname = \"x\"\nfirmware_subsystems = {list}\n"),
        )])
    };
    for (list, reason) in [
        ("[0]", "numbered from 1"),
        ("[1, 2, 1]", "listed once"),
        ("[1, 2, 3, 4, 5]", "at most 4"),
    ] {
        match load(list) {
            Err(ProfileError::FirmwareSubsystems {
                profile,
                value,
                reason: said,
            }) => {
                assert_eq!(profile, "x");
                assert!(!value.is_empty());
                assert!(said.contains(reason), "{list}: {said}");
            }
            other => panic!("{list}: {other:?}"),
        }
    }
    for list in ["[]", "[1]", "[1, 2, 3, 4]"] {
        assert!(load(list).is_ok(), "{list}");
    }
}

fn readout_on(id: &str) -> Result<Registry, ProfileError> {
    let source = format!(
        "schema = 1\nname = \"x\"\n\
         [[control]]\nid = {id}\nkey = \"mine\"\nlabel = \"Mine\"\nkind = \"readout\"\n"
    );
    Registry::from_sources(vec![("x".into(), source)])
}

#[test]
fn a_control_on_the_passphrase_function_is_a_load_error() {
    for id in ["[250]", "[250, 1]", "[250, 1, 2]"] {
        match readout_on(id) {
            Err(ProfileError::Control {
                profile,
                key,
                reason,
            }) => {
                assert_eq!(profile, "x");
                assert_eq!(key, "mine");
                assert!(reason.contains("250"), "{reason}");
                assert!(reason.contains("cannot be a control"), "{reason}");
            }
            other => panic!("{id}: {other:?}"),
        }
    }
}

#[test]
fn a_control_on_the_firmware_update_family_is_a_load_error() {
    for id in ["[210]", "[210, 1]", "[210, 3]", "[210, 4, 1]"] {
        match readout_on(id) {
            Err(ProfileError::Control { reason, .. }) => {
                assert!(reason.contains("210"), "{reason}");
                assert!(reason.contains("cannot be a control"), "{reason}");
            }
            other => panic!("{id}: {other:?}"),
        }
    }
}

#[test]
fn the_firmware_version_readout_is_still_a_control() {
    assert!(readout_on("[210, 2, 1]").is_ok());
}

/// A concrete profile `x` made of `body`, so the checks of a resolved profile run.
fn concrete(body: &str) -> Result<Registry, ProfileError> {
    let text = format!("schema = 1\nname = \"x\"\nproduct_id = 99\n{body}");
    Registry::from_sources(vec![("x".into(), text)])
}

/// The words of a load refusal, whichever error carries them.
fn refusal(result: Result<Registry, ProfileError>) -> String {
    match result {
        Err(error @ (ProfileError::Control { .. } | ProfileError::BuiltinSettings { .. })) => {
            error.to_string()
        }
        other => panic!("expected a load refusal, got {other:?}"),
    }
}

const RECALL_AND_LEVEL: &str = "\
[[control]]\nid = [6, 1, 3]\nkey = \"band1\"\nlabel = \"Band 1\"\nkind = \"level\"\n\
range = [0, 30]\nreads = [[6, 1]]\n\
[[control]]\nid = [3]\nkey = \"mode\"\nlabel = \"Mode\"\nkind = \"select\"\n\
values = { 1 = \"A\", 2 = \"B\" }\n\
[[control]]\nid = [7, 2]\nkey = \"recall\"\nlabel = \"Recall\"\nkind = \"action\"\nslots = 3\n\
changes = [\"band1\"]\n";

/// An action control block with `extra` lines after its `kind`.
fn action(extra: &str) -> String {
    format!(
        "[[control]]\nid = [7, 2]\nkey = \"recall\"\nlabel = \"Recall\"\nkind = \"action\"\n{extra}\n"
    )
}

#[test]
fn an_action_loads_into_the_actions_and_never_into_the_controls() {
    let r = concrete(RECALL_AND_LEVEL).unwrap();
    let profile = r.get("x").unwrap();
    assert!(profile.control_by_key("recall").is_none());
    assert!(profile.controls.iter().all(|c| c.key != "recall"));
    let recall = profile.action_by_key("RECALL").unwrap();
    assert_eq!(recall.id.to_string(), "7.2");
    assert_eq!(recall.label, "Recall");
    assert_eq!(recall.slots, 3);
    assert!(!recall.overwrites);
    assert_eq!(recall.changes, ["band1"]);
    assert_eq!(profile.actions.len(), 1);
}

#[test]
fn an_action_needs_slots_from_1_to_255() {
    for slots in ["", "slots = 0", "slots = 256", "slots = 65535"] {
        let text = refusal(concrete(&action(slots)));
        assert!(text.contains("slots"), "{slots}: {text}");
    }
    for slots in [1, 255] {
        assert!(
            concrete(&action(&format!("slots = {slots}"))).is_ok(),
            "{slots}"
        );
    }
}

#[test]
fn an_action_refuses_a_value_type_range_values_or_convert() {
    for extra in [
        "value_type = \"u8\"",
        "range = [0, 3]",
        "values = { 1 = \"A\" }",
        "convert = { linear = [0.0, 1.0] }",
        "center = 1",
        "unit = \"dB\"",
        "channel = \"left\"",
        "floor = 1",
        "tick = 1",
        "advice = \"x\"",
        "only_while = \"mode\"",
        "auto_off = true",
        "output_gain = true",
        "mutes_output = true",
        "powers_output = true",
        "asks_on_change = true",
        "reads = [[7, 3]]",
    ] {
        let text = refusal(concrete(&action(&format!("slots = 3\n{extra}"))));
        let field = extra.split(' ').next().unwrap();
        assert!(text.contains(field), "{extra}: {text}");
    }
}

#[test]
fn an_action_id_of_three_ids_is_refused() {
    let text = "[[control]]\nid = [7, 2, 1]\nkey = \"recall\"\nlabel = \"Recall\"\nkind = \"action\"\nslots = 3\n";
    assert!(refusal(concrete(text)).contains("three"));
}

#[test]
fn an_action_on_a_restricted_function_is_refused() {
    for id in ["[210]", "[210, 1]", "[250]", "[250, 1]"] {
        let text = format!(
            "[[control]]\nid = {id}\nkey = \"recall\"\nlabel = \"Recall\"\nkind = \"action\"\nslots = 3\n"
        );
        let refused = refusal(concrete(&text));
        assert!(refused.contains("cannot be an action"), "{id}: {refused}");
    }
}

/// A save is asked about before it is sent, and what asks hangs on `overwrites`: a profile cannot
/// make a save that sends without a question, in any spelling of its key.
#[test]
fn an_action_keyed_save_must_overwrite() {
    for key in ["save", "Save", "SAVE"] {
        let text = format!(
            "[[control]]\nid = [7, 1]\nkey = \"{key}\"\nlabel = \"Save\"\nkind = \"action\"\nslots = 3\n"
        );
        let refused = refusal(concrete(&text));
        assert!(
            refused.contains("must set overwrites = true"),
            "{key}: {refused}"
        );
        assert!(
            concrete(&format!("{text}overwrites = true\n")).is_ok(),
            "{key}"
        );
    }
    let other = "[[control]]\nid = [7, 1]\nkey = \"store\"\nlabel = \"Store\"\nkind = \"action\"\nslots = 3\n";
    assert!(
        concrete(other).is_ok(),
        "any other key may leave overwrites off"
    );
}

#[test]
fn an_action_whose_commands_overlap_a_control_is_refused() {
    let cases = [
        // a control under the action's commands, one over the action, one equal to a command
        (
            "[[control]]\nid = [7, 2, 1]\nkey = \"under\"\nlabel = \"U\"\nkind = \"readout\"\n",
            "overlap the id 7.2.1 of under",
        ),
        (
            "[[control]]\nid = [7]\nkey = \"over\"\nlabel = \"O\"\nkind = \"readout\"\n",
            "overlap the id 7 of over",
        ),
        (
            "[[control]]\nid = [7, 2]\nkey = \"same\"\nlabel = \"S\"\nkind = \"readout\"\n",
            "duplicate function id",
        ),
        // a control that reads under or over the action
        (
            "[[control]]\nid = [9]\nkey = \"r\"\nlabel = \"R\"\nkind = \"readout\"\nreads = [[7, 2, 3]]\n",
            "reads 7.2.3, which overlaps the commands of the action recall",
        ),
        (
            "[[control]]\nid = [9]\nkey = \"r\"\nlabel = \"R\"\nkind = \"readout\"\nreads = [[7]]\n",
            "reads 7, which overlaps the commands of the action recall",
        ),
        // a second action on commands that touch the first
        (
            "[[control]]\nid = [7]\nkey = \"save\"\nlabel = \"S\"\nkind = \"action\"\nslots = 3\n\
             overwrites = true\n",
            "overlap those of the action save on 7",
        ),
    ];
    for (other, wording) in cases {
        let text = format!("{}{other}", action("slots = 3"));
        let refused = refusal(concrete(&text));
        assert!(refused.contains(wording), "{wording}: {refused}");
    }
    let apart = "[[control]]\nid = [7, 1]\nkey = \"save\"\nlabel = \"S\"\nkind = \"action\"\n\
        slots = 3\noverwrites = true\n";
    assert!(concrete(&format!("{}{apart}", action("slots = 3"))).is_ok());
}

/// An action's `changes` are the controls a recall makes the device report, and the daemon
/// writes none of them itself: the volume and a control that asks before it changes are never
/// among them.
#[test]
fn an_actions_changes_cannot_name_the_volume_or_the_mode() {
    let controls = "[[control]]\nid = [6, 1, 3]\nkey = \"band1\"\nlabel = \"Band 1\"\nkind = \"level\"\n\
        range = [0, 30]\n\
        [[control]]\nid = [2]\nkey = \"volume\"\nlabel = \"Volume\"\nkind = \"level\"\nrange = [0, 99]\n\
        [[control]]\nid = [3]\nkey = \"mode\"\nlabel = \"Mode\"\nkind = \"select\"\n\
        values = { 1 = \"A\", 2 = \"B\" }\nasks_on_change = true\n";
    let with = |changes: &str| {
        concrete(&format!(
            "{controls}{}",
            action(&format!("slots = 3\nchanges = {changes}"))
        ))
    };
    assert!(with("[\"band1\"]").is_ok());
    let refused = refusal(with("[\"band1\", \"volume\"]"));
    assert!(
        refused.contains("changes names \"volume\", the volume"),
        "{refused}"
    );
    let refused = refusal(with("[\"mode\"]"));
    assert!(
        refused.contains("changes names \"mode\", which asks before it changes"),
        "{refused}"
    );
}

#[test]
fn an_action_in_the_tone_control_family_or_with_a_scale_is_refused() {
    let eq = "[eq]\nid = 7\nbands = [40]\nfscale = [[0.005554, 0.00544]]\nrange = 1.95\n\
        gain = { range = [-20, 6], zero = 98 }\nq = { range = [0.5, 8.0], center = 4.3 }\n";
    let refused = refusal(concrete(&format!("{}{eq}", action("slots = 3"))));
    assert!(
        refused.contains("tone control's function family"),
        "{refused}"
    );
    let scaled = action("slots = 3\nscale = { zero = 1, full = 9 }");
    let refused = refusal(concrete(&scaled));
    assert!(
        refused.contains("scale does not apply to an action"),
        "{refused}"
    );
}

#[test]
fn slots_overwrites_and_changes_apply_only_to_an_action() {
    for extra in ["slots = 3", "overwrites = true", "changes = [\"k\"]"] {
        let text = format!(
            "[[control]]\nid = [1]\nkey = \"k\"\nlabel = \"K\"\nkind = \"toggle\"\n{extra}\n"
        );
        let refused = refusal(concrete(&text));
        let field = extra.split(' ').next().unwrap();
        assert!(
            refused.contains(field) && refused.contains("action"),
            "{refused}"
        );
    }
}

#[test]
fn changes_must_name_controls_of_the_resolved_profile() {
    let base = "schema = 1\nname = \"base\"\n[[control]]\nid = [7, 2]\nkey = \"recall\"\n\
        label = \"Recall\"\nkind = \"action\"\nslots = 3\nchanges = [\"band1\"]\n";
    // an abstract base may name a key only its child defines
    assert!(Registry::from_sources(vec![("base".into(), base.into())]).is_ok());
    let child = "schema = 1\nname = \"child\"\nproduct_id = 99\nextends = \"base\"\n";
    let refused = refusal(Registry::from_sources(vec![
        ("base".into(), base.into()),
        ("child".into(), child.into()),
    ]));
    assert!(refused.contains("band1"), "{refused}");
    let child = format!(
        "{child}[[control]]\nid = [6]\nkey = \"band1\"\nlabel = \"B\"\nkind = \"level\"\nrange = [0, 30]\n"
    );
    let r = Registry::from_sources(vec![("base".into(), base.into()), ("child".into(), child)])
        .unwrap();
    assert_eq!(r.get("child").unwrap().actions[0].changes, ["band1"]);
    // an action key is not a control
    let text = action("slots = 3\nchanges = [\"recall\"]");
    assert!(refusal(concrete(&text)).contains("recall"));
}

#[test]
fn a_child_control_replaces_a_parent_action_of_the_same_key() {
    let parent = format!("schema = 1\nname = \"base\"\n{}", action("slots = 3"));
    let child = "schema = 1\nname = \"child\"\nproduct_id = 99\nextends = \"base\"\n\
        [[control]]\nid = [7, 2]\nkey = \"recall\"\nlabel = \"Recall now\"\nkind = \"action\"\nslots = 2\n";
    let r = Registry::from_sources(vec![
        ("base".into(), parent.clone()),
        ("child".into(), child.into()),
    ])
    .unwrap();
    let profile = r.get("child").unwrap();
    assert_eq!(profile.actions.len(), 1);
    assert_eq!(profile.actions[0].slots, 2);
    assert_eq!(profile.actions[0].label, "Recall now");
    // a control of the key replaces the parent's action, and an action the parent's control
    let as_control = "schema = 1\nname = \"child\"\nproduct_id = 99\nextends = \"base\"\n\
        [[control]]\nid = [7, 5]\nkey = \"recall\"\nlabel = \"Recall\"\nkind = \"readout\"\n";
    let r = Registry::from_sources(vec![
        ("base".into(), parent),
        ("child".into(), as_control.into()),
    ])
    .unwrap();
    let profile = r.get("child").unwrap();
    assert!(profile.actions.is_empty());
    assert!(profile.control_by_key("recall").is_some());
    let parent = "schema = 1\nname = \"base\"\n[[control]]\nid = [7, 5]\nkey = \"recall\"\n\
        label = \"Recall\"\nkind = \"readout\"\n";
    let child = format!(
        "schema = 1\nname = \"child\"\nproduct_id = 99\nextends = \"base\"\n{}",
        action("slots = 3")
    );
    let r = Registry::from_sources(vec![
        ("base".into(), parent.into()),
        ("child".into(), child),
    ])
    .unwrap();
    let profile = r.get("child").unwrap();
    assert!(profile.control_by_key("recall").is_none());
    assert_eq!(profile.actions.len(), 1);
}

#[test]
fn a_reads_entry_under_another_controls_id_is_refused() {
    let text = "[[control]]\nid = [6, 1]\nkey = \"a\"\nlabel = \"A\"\nkind = \"readout\"\n\
        [[control]]\nid = [8]\nkey = \"b\"\nlabel = \"B\"\nkind = \"readout\"\nreads = [[6, 1, 3]]\n";
    assert!(refusal(concrete(text)).contains("reads"));
    let over = "[[control]]\nid = [6, 1, 3]\nkey = \"a\"\nlabel = \"A\"\nkind = \"readout\"\n\
        [[control]]\nid = [8]\nkey = \"b\"\nlabel = \"B\"\nkind = \"readout\"\nreads = [[6, 1]]\n";
    assert!(refusal(concrete(over)).contains("reads"));
    // a control's own id may lie under its own entry
    let own = "[[control]]\nid = [6, 1, 3]\nkey = \"a\"\nlabel = \"A\"\nkind = \"readout\"\n\
        reads = [[6, 1]]\n";
    assert!(concrete(own).is_ok());
    // the tone control's family is no control's to read
    let eq = "[[control]]\nid = [8]\nkey = \"b\"\nlabel = \"B\"\nkind = \"readout\"\nreads = [[12, 1]]\n\
        [eq]\nid = 12\nbands = [40]\nfscale = [[0.005554, 0.00544]]\nrange = 1.95\n\
        gain = { range = [-20, 6], zero = 98 }\nq = { range = [0.5, 8.0], center = 4.3 }\n";
    assert!(refusal(concrete(eq)).contains("tone control"));
}

#[test]
fn two_controls_reading_one_id_are_refused() {
    for second in ["[[6, 1]]", "[[6, 1, 4]]", "[[6]]"] {
        let text = format!(
            "[[control]]\nid = [8]\nkey = \"a\"\nlabel = \"A\"\nkind = \"readout\"\nreads = [[6, 1]]\n\
             [[control]]\nid = [9]\nkey = \"b\"\nlabel = \"B\"\nkind = \"readout\"\nreads = {second}\n"
        );
        assert!(refusal(concrete(&text)).contains("reads"), "{second}");
    }
}

#[test]
fn a_reads_entry_of_a_restricted_function_is_refused() {
    for entry in ["[210]", "[210, 1]", "[250]"] {
        let text = format!(
            "[[control]]\nid = [8]\nkey = \"a\"\nlabel = \"A\"\nkind = \"readout\"\nreads = [{entry}]\n"
        );
        assert!(refusal(concrete(&text)).contains("reads"), "{entry}");
    }
}

/// The Loki Max shaped profile `read_as` is asked of.
fn bands() -> Profile {
    let text = "[[control]]\nid = [6, 1, 3]\nkey = \"band1\"\nlabel = \"B1\"\nkind = \"level\"\n\
        range = [0, 30]\nreads = [[6, 1]]\n\
        [[control]]\nid = [6, 2, 3]\nkey = \"band2\"\nlabel = \"B2\"\nkind = \"level\"\n\
        range = [0, 30]\nreads = [[6, 2]]\n\
        [[control]]\nid = [2]\nkey = \"input\"\nlabel = \"Input\"\nkind = \"select\"\n";
    concrete(text).unwrap().get("x").unwrap().clone()
}

#[test]
fn read_as_maps_a_two_id_and_any_third_id_onto_the_band_control() {
    let profile = bands();
    let band1 = FunctionId::three(6, 1, 3);
    for id in [
        FunctionId::two(6, 1),
        FunctionId::three(6, 1, 3),
        FunctionId::three(6, 1, 9),
    ] {
        assert_eq!(profile.read_as(id), band1, "{id}");
    }
    assert_eq!(
        profile.read_as(FunctionId::two(6, 2)),
        FunctionId::three(6, 2, 3)
    );
}

#[test]
fn read_as_leaves_an_id_nothing_reads_alone() {
    let profile = bands();
    for id in [
        FunctionId::two(6, 3),
        FunctionId::one(6),
        FunctionId::one(2),
        FunctionId::three(7, 1, 3),
    ] {
        assert_eq!(profile.read_as(id), id, "{id}");
    }
}

/// A profile of two bands, a mode and `extra`, to try `[builtin_settings]` on.
fn with_settings(settings: &str) -> Result<Registry, ProfileError> {
    let text = format!(
        "[[control]]\nid = [6, 1, 3]\nkey = \"band1\"\nlabel = \"B1\"\nkind = \"level\"\nrange = [0, 30]\nreads = [[6, 1]]\n\
         [[control]]\nid = [6, 2, 3]\nkey = \"band2\"\nlabel = \"B2\"\nkind = \"level\"\nrange = [0, 30]\nreads = [[6, 2]]\n\
         [[control]]\nid = [3]\nkey = \"mode\"\nlabel = \"Mode\"\nkind = \"select\"\nvalues = {{ 1 = \"A\", 2 = \"B\" }}\n\
         [[control]]\nid = [4]\nkey = \"mute\"\nlabel = \"Mute\"\nkind = \"toggle\"\n\
         {settings}"
    );
    concrete(&text)
}

const SETTINGS: &str = "[builtin_settings]\nkeys = [\"band1\", \"band2\"]\nids = [[6, 1], [6, 2]]\n\
    settings = [{ name = \"Flat\", raw = [15, 15] }, { name = \"Boost\", raw = [28, 21] }]\n";

#[test]
fn a_builtin_setting_can_only_name_level_controls() {
    let profile = with_settings(SETTINGS).unwrap().get("x").unwrap().clone();
    assert_eq!(profile.builtin_settings.as_ref().unwrap().settings.len(), 2);
    for (key, id) in [("mode", "[3]"), ("mute", "[4]"), ("nothing", "[6, 1]")] {
        let text = SETTINGS
            .replace("\"band2\"", &format!("\"{key}\""))
            .replace("[6, 2]", id);
        let refused = refusal(with_settings(&text));
        assert!(
            refused.contains(key) && refused.contains("level"),
            "{key}: {refused}"
        );
    }
}

#[test]
fn a_builtin_setting_cannot_name_the_volume() {
    let text = "[[control]]\nid = [10]\nkey = \"volume\"\nlabel = \"Volume\"\nkind = \"level\"\n\
        range = [0, 30]\nreads = [[10, 1]]\n\
        [builtin_settings]\nkeys = [\"Volume\"]\nids = [[10, 1]]\n\
        settings = [{ name = \"Loud\", raw = [30] }]\n";
    let refused = refusal(concrete(text));
    assert!(
        refused.contains("volume") && refused.contains("never part of"),
        "{refused}"
    );
    // a level of another key is fine
    let band = text.replace("volume", "band1").replace("Volume", "band1");
    assert!(concrete(&band).is_ok());
}

#[test]
fn a_builtin_settings_id_must_read_as_its_keys_control() {
    // 6.2 reads as band2, not band1
    let swapped = SETTINGS.replace("[[6, 1], [6, 2]]", "[[6, 2], [6, 1]]");
    assert!(refusal(with_settings(&swapped)).contains("reads as"));
    // 6.5 is read as nothing
    let nowhere = SETTINGS.replace("[6, 2]]", "[6, 5]]");
    assert!(refusal(with_settings(&nowhere)).contains("reads as"));
    // an id for every key
    let short = SETTINGS.replace("ids = [[6, 1], [6, 2]]", "ids = [[6, 1]]");
    assert!(refusal(with_settings(&short)).contains("ids"));
}

#[test]
fn a_builtin_setting_raw_outside_its_controls_range_is_refused() {
    for raw in ["[31, 15]", "[15, -1]", "[15]", "[15, 15, 15]"] {
        let text = SETTINGS.replace("[28, 21]", raw);
        assert!(refusal(with_settings(&text)).contains("Boost"), "{raw}");
    }
}

#[test]
fn builtin_setting_names_must_be_unique_and_not_blank() {
    for second in ["flat", " FLAT ", "", "   "] {
        let text = SETTINGS.replace("\"Boost\"", &format!("\"{second}\""));
        assert!(refusal(with_settings(&text)).contains("name"), "{second:?}");
    }
    let none = "[builtin_settings]\nkeys = [\"band1\"]\nids = [[6, 1]]\nsettings = []\n";
    assert!(refusal(with_settings(none)).contains("no settings"));
    let repeated = SETTINGS.replace("\"band2\"", "\"BAND1\"");
    assert!(refusal(with_settings(&repeated)).contains("more than once"));
}

#[test]
fn a_profile_with_eq_and_builtin_settings_is_refused() {
    let eq = "[eq]\nid = 12\nbands = [40]\nfscale = [[0.005554, 0.00544]]\nrange = 1.95\n\
        gain = { range = [-20, 6], zero = 98 }\nq = { range = [0.5, 8.0], center = 4.3 }\n";
    let refused = refusal(with_settings(&format!("{eq}{SETTINGS}")));
    assert!(
        refused.contains("[eq]") && refused.contains("[builtin_settings]"),
        "{refused}"
    );
}

#[test]
fn a_childs_builtin_settings_replace_its_parents_whole() {
    let parent = format!(
        "schema = 1\nname = \"base\"\n\
         [[control]]\nid = [6, 1, 3]\nkey = \"band1\"\nlabel = \"B1\"\nkind = \"level\"\nrange = [0, 30]\nreads = [[6, 1]]\n\
         [[control]]\nid = [6, 2, 3]\nkey = \"band2\"\nlabel = \"B2\"\nkind = \"level\"\nrange = [0, 30]\nreads = [[6, 2]]\n{SETTINGS}"
    );
    let child = "schema = 1\nname = \"child\"\nproduct_id = 99\nextends = \"base\"\n\
        [builtin_settings]\nkeys = [\"band1\"]\nids = [[6, 1]]\n\
        settings = [{ name = \"Only\", raw = [20] }]\n";
    let r = Registry::from_sources(vec![
        ("base".into(), parent),
        ("child".into(), child.into()),
    ])
    .unwrap();
    let table = r.get("child").unwrap().builtin_settings.clone().unwrap();
    assert_eq!(table.keys, ["band1"]);
    assert_eq!(table.ids, [FunctionId::two(6, 1)]);
    assert_eq!(table.settings.len(), 1);
    assert_eq!(table.settings[0].name, "Only");
    assert_eq!(table.settings[0].raw, [20]);
}

#[test]
fn an_unknown_key_lists_and_hints_the_action_keys_too() {
    let profile = concrete(RECALL_AND_LEVEL)
        .unwrap()
        .get("x")
        .unwrap()
        .clone();
    match profile.unknown_key("recal") {
        ProfileError::UnknownKey { keys, hint, .. } => {
            assert_eq!(keys, ["band1", "mode", "recall"]);
            assert_eq!(hint.as_deref(), Some("recall"));
        }
        other => panic!("{other:?}"),
    }
}
