// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_proto::FunctionId;

use super::*;
use crate::Registry;

/// The Aegir 2's scale.
const AEGIR: Placed = Placed::Log {
    zero: 160,
    full: 350,
    class_a: 0.25,
};

/// `a` and `b` within half a hundredth: the worked examples are given to two places.
fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.005
}

#[test]
fn the_worked_examples_land_where_the_scale_puts_them() {
    assert!(
        near(AEGIR.fraction(161.0), 0.132),
        "{}",
        AEGIR.fraction(161.0)
    );
    assert!(
        near(AEGIR.fraction(173.0), 0.50),
        "{}",
        AEGIR.fraction(173.0)
    );
    let vidar = Placed::Log {
        zero: 50,
        full: 1000,
        class_a: 0.10,
    };
    assert!(near(vidar.fraction(80.0), 0.50), "{}", vidar.fraction(80.0));
    let kraken = Placed::Log {
        zero: 30,
        full: 600,
        class_a: 0.10,
    };
    assert!(
        near(kraken.fraction(53.0), 0.50),
        "{}",
        kraken.fraction(53.0)
    );
    let tyr_without_a_floor = Placed::Log {
        zero: 0,
        full: 1000,
        class_a: 0.10,
    };
    assert!(near(tyr_without_a_floor.fraction(31.0), 0.50));
}

#[test]
fn a_log_scale_is_empty_at_and_below_its_zero_and_full_at_and_above_full_scale() {
    for raw in [0.0, 100.0, 160.0] {
        assert_eq!(AEGIR.fraction(raw), 0.0, "{raw}");
    }
    for raw in [350.0, 351.0, 65_535.0] {
        assert_eq!(AEGIR.fraction(raw), 1.0, "{raw}");
    }
}

#[test]
fn raw_at_inverts_fraction_over_the_whole_span() {
    assert_eq!(AEGIR.raw_at(0.0), 160);
    assert_eq!(AEGIR.raw_at(1.0), 350);
    assert_eq!(AEGIR.raw_at(0.5), 173);
    for raw in 160..=350 {
        assert_eq!(AEGIR.raw_at(AEGIR.fraction(raw as f64)), raw, "{raw}");
    }
    assert_eq!(
        AEGIR.raw_at(-1.0),
        160,
        "below the scale clamps to its zero"
    );
    assert_eq!(AEGIR.raw_at(2.0), 350, "above it clamps to full scale");
    assert_eq!(AEGIR.raw_at(f64::NAN), 160, "no level is the zero");
}

#[test]
fn a_zero_at_or_past_full_scale_leaves_the_bar_empty() {
    for zero in [1000, 1200] {
        let placed = Placed::Log {
            zero,
            full: 1000,
            class_a: 0.10,
        };
        assert_eq!(placed.fraction(5_000.0), 0.0, "{zero}");
        assert_eq!(placed.raw_at(0.7), zero, "{zero}");
    }
}

#[test]
fn the_zero_sits_half_again_above_the_floor_and_at_least_ten_above_it() {
    assert_eq!(zero_from_floor(None), 0);
    assert_eq!(zero_from_floor(Some(0)), 10);
    assert_eq!(zero_from_floor(Some(20)), 30);
    assert_eq!(zero_from_floor(Some(21)), 31);
    assert_eq!(zero_from_floor(Some(22)), 33);
    assert_eq!(zero_from_floor(Some(23)), 34);
    assert_eq!(zero_from_floor(Some(100)), 150);
}

#[test]
fn the_floors_that_leave_a_bar_empty() {
    for (full, first_empty) in [(1000, 667), (160, 107), (420, 280)] {
        assert!(zero_from_floor(Some(first_empty - 1)) < full, "{full}");
        assert!(zero_from_floor(Some(first_empty)) >= full, "{full}");
    }
}

#[test]
fn a_linear_scale_runs_from_its_floor_and_marks_its_tick() {
    let placed = Placed::Linear {
        from: 8,
        to: 1023,
        tick: Some(700),
    };
    assert_eq!(placed.fraction(8.0), 0.0);
    assert_eq!(placed.fraction(0.0), 0.0);
    assert_eq!(placed.fraction(1023.0), 1.0);
    assert!(near(placed.tick().unwrap(), (700.0 - 8.0) / 1015.0));
    assert_eq!(placed.raw_at(0.5), 515, "truncated toward the bottom");
    assert_eq!(placed.class_a(), 0.0);
    assert_eq!(
        AEGIR.tick(),
        None,
        "a log scale's mark is its class-A block"
    );
    assert_eq!(AEGIR.class_a(), 0.25);
}

/// A one-amp profile: the meter switch, power, warming up, a meter on `20.1` with `scale` and
/// a floor readout on `20.2`.
fn amp(scale: &str) -> crate::Profile {
    let text = format!(
        r#"
schema = 1
name = "amp"
product_id = 9000

[[control]]
id = [21, 2]
key = "power_meters"
label = "Power meters"
kind = "toggle"

[[control]]
id = [10]
key = "power"
label = "Power"
kind = "toggle"

[[control]]
id = [4]
key = "warming_up"
label = "Warming up"
kind = "readout"
value_type = "bool"
only_while = "power"

[[control]]
id = [20, 1]
key = "meter_1"
label = "Meter"
kind = "meter"
scale = {scale}

[[control]]
id = [20, 2]
key = "meter_floor"
label = "Meter floor"
kind = "readout"
value_type = "u16"
"#
    );
    Registry::from_sources(vec![("amp".into(), text)])
        .expect("the profile loads")
        .get("amp")
        .expect("it resolves")
        .clone()
}

fn id(text: &str) -> FunctionId {
    text.parse().expect("a function id")
}

#[test]
fn a_floor_readout_settles_the_zero_and_none_leaves_it_at_nought() {
    let profile = amp(r#"{ zero_from = "meter_floor", full = 1000, class_a = 0.10 }"#);
    let meter = profile.control_by_key("meter_1").unwrap();
    let floor_at = |m: Option<i64>| move |at: FunctionId| (at == id("20.2")).then_some(m).flatten();
    assert_eq!(
        profile.placed(meter, floor_at(None)),
        Some(Placed::Log {
            zero: 0,
            full: 1000,
            class_a: 0.10
        })
    );
    assert_eq!(
        profile.placed(meter, floor_at(Some(100))),
        Some(Placed::Log {
            zero: 150,
            full: 1000,
            class_a: 0.10
        })
    );
    let floor = profile.control_by_key("meter_floor").unwrap();
    assert_eq!(profile.zero_source(meter).map(|c| c.id), Some(floor.id));
    assert!(profile.sets_a_zero(floor));
    assert!(!profile.sets_a_zero(profile.control_by_key("warming_up").unwrap()));
}

#[test]
fn a_fixed_zero_needs_no_reading_and_a_control_with_no_scale_places_nothing() {
    let profile = amp("{ zero = 160, full = 350, class_a = 0.25 }");
    let meter = profile.control_by_key("meter_1").unwrap();
    assert_eq!(profile.placed(meter, |_| None), Some(AEGIR));
    assert_eq!(profile.zero_source(meter), None);
    let toggle = profile.control_by_key("power").unwrap();
    assert_eq!(profile.placed(toggle, |_| None), None);
}

#[test]
fn the_meters_are_live_unless_switched_off_asleep_or_warming_up() {
    let profile = amp("{ zero = 160, full = 350 }");
    let states = |meters: Option<bool>, power: Option<bool>, warming: Option<bool>| {
        move |at: FunctionId| match at.to_string().as_str() {
            "21.2" => meters,
            "10" => power,
            "4" => warming,
            _ => None,
        }
    };
    assert!(
        profile.meters_live(states(None, None, None)),
        "nothing reported yet"
    );
    assert!(profile.meters_live(states(Some(true), Some(true), Some(false))));
    assert!(
        !profile.meters_live(states(Some(false), Some(true), Some(false))),
        "switched off"
    );
    assert!(
        !profile.meters_live(states(Some(true), Some(false), None)),
        "in standby"
    );
    assert!(
        !profile.meters_live(states(Some(true), Some(true), Some(true))),
        "warming up"
    );
}

#[test]
fn a_profile_with_no_meter_switch_always_has_live_meters() {
    let text = "schema = 1\nname = \"bare\"\nproduct_id = 9001\n[[control]]\nid = [20, 1]\nkey = \"meter_1\"\nlabel = \"M\"\nkind = \"meter\"\nscale = { zero = 0, full = 100 }\n";
    let registry = Registry::from_sources(vec![("bare".into(), text.into())]).unwrap();
    assert!(registry.get("bare").unwrap().meters_live(|_| Some(false)));
}
