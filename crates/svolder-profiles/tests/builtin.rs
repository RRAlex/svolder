// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Every built-in profile loads, resolves, and covers the products it claims.

use std::collections::BTreeSet;

use svolder_profiles::{ControlKind, LogScale, Registry, Zero};
use svolder_proto::FunctionId;

#[test]
fn all_builtin_profiles_resolve() {
    let r = Registry::builtin().expect("built-in profiles must parse and resolve");
    assert!(r.names().len() >= 28, "{:?}", r.names());
}

/// The control set shared by Gungnir 2 from firmware 2 onwards and its siblings.
const GUNGNIR2_V2: &str = "3 phase toggle | 5 mute toggle \
     | 4 nos toggle | 6 sample_rate readout | 10 volume level | 11 balance level \
     | 2 input select | 7.1 usb1_connected readout | 7.2 usb2_connected readout \
     | 8 spdif_locked readout";

/// The control set of a stereo power amp that has standby.
const AEGIR2: &str = "21.1 temperature_reporting readout | 21.2 power_meters toggle \
     | 230.1 fault_overcurrent fault | 230.2 fault_overtemp fault | 20.1 meter_1 meter \
     | 20.2 meter_2 meter | 10 power toggle | 4 warming_up readout";

/// Every concrete profile and the exact set of controls it exposes. Each part holds
/// `id key kind` entries separated by `|`; the parts of one profile are joined.
const EXPECTED: &[(&str, &[&str])] = &[
    (
        "gungnir2-v1",
        &["3 phase toggle | 5 mute toggle \
           | 2 input select | 4 nos toggle | 6 sample_rate readout \
           | 7.1 usb1_connected readout | 7.2 usb2_connected readout | 8 spdif_locked readout"],
    ),
    ("gungnir2-v2", &[GUNGNIR2_V2]),
    ("yggdrasil", &[GUNGNIR2_V2]),
    (
        "bifrost3",
        &[
            "3 phase toggle | 5 mute toggle | 4 nos toggle | 6 sample_rate readout \
           | 10 volume level | 11 balance level | 2 input select \
           | 7 usb_connected readout | 8 spdif_locked readout",
        ],
    ),
    (
        "mimir",
        &["3 phase toggle | 5 mute toggle | 6 volume level \
           | 8 balance level | 9 sample_rate readout | 2 input select | 4 nos toggle \
           | 10 usb_connected readout | 11 spdif_locked readout"],
    ),
    (
        "meshcard",
        &["3 phase toggle | 5 mute toggle | 6 volume level \
           | 8 balance level | 9 sample_rate readout | 4 nos toggle \
           | 10 usb_connected readout"],
    ),
    (
        "eitr2",
        &["3 phase toggle | 5 mute toggle | 6 volume level \
           | 8 balance level | 9 sample_rate readout | 10 usb_connected readout"],
    ),
    (
        "freya",
        &[
            "3 mode select | 4 mute toggle | 230.1 fault_overcurrent fault | 2 input select \
           | 6 volume level",
        ],
    ),
    (
        "kara",
        &[
            "3 mode select | 4 mute toggle | 230.1 fault_overcurrent fault | 2 input select \
           | 6 volume level | 5 headphone readout | 230.2 fault_headphone fault",
        ],
    ),
    (
        "saga2",
        &[
            "3 mode select | 4 mute toggle | 230.1 fault_overcurrent fault | 2 input select \
           | 5 headphone readout | 6 volume level | 10 power readout \
           | 230.2 fault_headphone fault",
        ],
    ),
    (
        "lyr5",
        &[
            "2 input select | 3 gain select | 4.1 headphone_mute toggle | 4.2 preout toggle \
           | 6 volume level | 7 mode select | 22 class_state readout \
           | 230.1 fault_overcurrent fault | 230.2 fault_headphone fault",
        ],
    ),
    ("aegir2", &[AEGIR2]),
    ("vidar2", &[AEGIR2]),
    (
        "gjallarhorn",
        &[
            "21.1 temperature_reporting readout | 21.2 power_meters toggle \
           | 230.1.1 fault_overcurrent fault | 230.1.2 fault_overcurrent_r fault \
           | 230.2 fault_overtemp fault | 4 mute toggle | 20.1 meter_1 meter \
           | 20.2 meter_2 meter",
        ],
    ),
    (
        "kraken",
        &[
            AEGIR2,
            "20.3 meter_3 meter | 20.4 meter_4 meter \
             | 20.5 meter_5 meter",
        ],
    ),
    (
        "wotan",
        &[
            "21.1 temperature_reporting readout | 21.2 power_meters toggle \
             | 20.1 meter_1 meter | 20.2 meter_2 meter | 10 power toggle | 4 warming_up readout \
             | 2 input select | 3 feedback toggle | 20.3 meter_floor_r readout \
             | 20.4 meter_floor_l readout | 230.1.1 fault_overcurrent fault \
             | 230.1.2 fault_overcurrent_r fault | 230.2.1 fault_overtemp fault \
             | 230.2.2 fault_overtemp_r fault",
        ],
    ),
    (
        "tyr",
        &[
            "21.1 temperature_reporting readout | 21.2 power_meters toggle \
           | 230.1 fault_overcurrent fault | 230.2.1 fault_overtemp fault | 10 power toggle \
           | 4 warming_up readout | 20.1 meter_1 meter | 20.2 meter_floor readout",
        ],
    ),
    (
        "skoll",
        &["2 input select | 4 mute toggle | 7.2 subsonic toggle \
           | 11.1 load_capacitance select | 11.2 load_resistance select | 3 gain select \
           | 5 headphone readout"],
    ),
    (
        "stjarna",
        &["2 input select | 4 mute toggle | 7.2 subsonic toggle \
           | 11.1 load_capacitance select | 11.2 load_resistance select | 3 gain select \
           | 7.1 mono toggle | 10 power toggle | 201 remote_lock readout"],
    ),
    (
        "lokimax",
        &[
            "2 input select | 3 mode select | 6.1.3 band1 level | 6.2.3 band2 level \
           | 6.3.3 band3 level | 6.4.3 band4 level | 6.5.3 band5 level | 6.6.3 band6 level",
        ],
    ),
    (
        "freya2",
        &[
            "2 input select | 3.1 mode select | 3.2 gain select | 4.1 mute toggle \
           | 6.1 volume level | 6.2 balance level | 10 power toggle \
           | 230.1 fault_overcurrent_input fault | 230.2 fault_overcurrent_output fault",
        ],
    ),
    (
        "hati",
        &["2 input select | 4 mute toggle | 7.2 subsonic toggle \
           | 11.1 load_capacitance select | 11.2 load_resistance select | 3 gain select \
           | 7.1 mono toggle | 10 power toggle | 201 remote_lock readout"],
    ),
    // Not a Schiit product: the amplifier the simulator and the in-process fake serve, which
    // is a stereo amp with no standby.
    (
        "fake-amp",
        &[
            "21.1 temperature_reporting readout | 21.2 power_meters toggle \
           | 230.1 fault_overcurrent fault | 230.2 fault_overtemp fault | 4 mute toggle \
           | 20.1 meter_1 meter | 20.2 meter_2 meter",
        ],
    ),
    (
        "fake-mono",
        &[
            "21.1 temperature_reporting readout | 21.2 power_meters toggle \
           | 230.1 fault_overcurrent fault | 230.2 fault_overtemp fault | 4 mute toggle \
           | 20.1 meter_1 meter | 20.2 meter_floor readout",
        ],
    ),
];

/// The `id key kind` entries of one row of `EXPECTED`, whitespace normalised.
fn expected_set(parts: &[&str]) -> BTreeSet<String> {
    parts
        .iter()
        .flat_map(|part| part.split('|'))
        .map(|entry| entry.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|entry| !entry.is_empty())
        .collect()
}

#[test]
fn every_concrete_profile_has_exactly_these_controls() {
    let r = Registry::builtin().unwrap();
    let mut report = String::new();
    for (name, parts) in EXPECTED {
        let p = r.get(name).unwrap_or_else(|| panic!("no profile {name}"));
        let want = expected_set(parts);
        let have: BTreeSet<String> = p
            .controls
            .iter()
            .map(|c| {
                let kind = format!("{:?}", c.kind).to_ascii_lowercase();
                format!("{} {} {}", c.id, c.key, kind)
            })
            .collect();
        let list = |set: &BTreeSet<String>, other: &BTreeSet<String>| {
            set.difference(other)
                .map(String::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        };
        let (missing, unexpected) = (list(&want, &have), list(&have, &want));
        if !missing.is_empty() {
            report.push_str(&format!("{name}: missing {missing}\n"));
        }
        if !unexpected.is_empty() {
            report.push_str(&format!("{name}: unexpected {unexpected}\n"));
        }
    }
    assert!(report.is_empty(), "\n{report}");
    assert_eq!(
        r.get("kraken")
            .unwrap()
            .control_by_key("meter_1")
            .unwrap()
            .label,
        "Meter 1"
    );
    assert!(r.get("tyr").unwrap().control_by_key("mute").is_none());
}

/// A scale's zero in a table row: a fixed reading, or the key of the floor readout it follows.
enum Z {
    At(i64),
    Follows(&'static str),
}

/// One meter's expected scale: profile, meter key, zero, full-scale reading, class-A fraction.
type Row = (&'static str, &'static str, Z, i64, f64);

const AEGIR_SCALE: (i64, i64, f64) = (160, 350, 0.25);
const TYR_FULL: i64 = 1000;
const KRAKEN_SCALE: (i64, i64, f64) = (30, 600, 0.10);

/// Every amp meter and the logarithmic scale it reads on, by channel.
fn meter_rows() -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::new();
    for key in ["meter_1", "meter_2"] {
        let (zero, full, class_a) = AEGIR_SCALE;
        rows.push(("aegir2", key, Z::At(zero), full, class_a));
        rows.push(("fake-amp", key, Z::At(zero), full, class_a));
        rows.push(("vidar2", key, Z::At(50), 1000, 0.10));
        rows.push(("gjallarhorn", key, Z::At(10), 373, 0.10));
    }
    for key in ["meter_1", "meter_2", "meter_3", "meter_4", "meter_5"] {
        let (zero, full, class_a) = KRAKEN_SCALE;
        rows.push(("kraken", key, Z::At(zero), full, class_a));
        rows.push(("fake-five", key, Z::At(zero), full, class_a));
    }
    rows.push(("tyr", "meter_1", Z::Follows("meter_floor"), TYR_FULL, 0.10));
    rows.push((
        "fake-mono",
        "meter_1",
        Z::Follows("meter_floor"),
        TYR_FULL,
        0.10,
    ));
    rows.push(("wotan", "meter_1", Z::Follows("meter_floor_l"), 160, 0.10));
    rows.push(("wotan", "meter_2", Z::Follows("meter_floor_r"), 420, 0.10));
    rows
}

#[test]
fn every_amp_meter_reads_on_its_own_scale() {
    for (name, key, zero, full, class_a) in meter_rows() {
        let meter = control(name, key);
        let want_zero = match zero {
            Z::At(n) => Zero::Fixed(n),
            Z::Follows(k) => Zero::Floor(k.to_string()),
        };
        let want = LogScale {
            zero: want_zero,
            full,
            class_a,
        };
        assert_eq!(meter.log_scale, Some(want), "{name}: {key}");
        assert_eq!(meter.range, None, "{name}: {key} also has a range");
    }
}

/// A floor-following meter's zero comes from the readout on the id the amp streams its floor on.
#[test]
fn a_floor_following_meter_names_the_readout_on_the_floors_id() {
    let r = Registry::builtin().unwrap();
    let floor_ids = [
        ("tyr", "meter_1", "20.2"),
        ("wotan", "meter_1", "20.4"),
        ("wotan", "meter_2", "20.3"),
        ("fake-mono", "meter_1", "20.2"),
    ];
    for (name, key, id) in floor_ids {
        let profile = r.get(name).unwrap();
        let meter = profile.control_by_key(key).unwrap();
        let source = profile
            .zero_source(meter)
            .unwrap_or_else(|| panic!("{name}: {key} has no zero source"));
        assert_eq!(source.id.to_string(), id, "{name}: {key}");
        assert_eq!(source.value_type, svolder_proto::ValueType::U16);
    }
}

#[test]
fn no_built_in_meter_lacks_a_scale_or_draws_a_line() {
    let r = Registry::builtin().unwrap();
    for name in r.names() {
        let p = r.get(name).unwrap();
        if p.product_id.is_none() {
            continue;
        }
        for meter in p.controls.iter().filter(|c| c.kind == ControlKind::Meter) {
            assert!(
                meter.log_scale.is_some(),
                "{name}: meter {} has no scale",
                meter.key
            );
            assert!(
                meter.range.is_none(),
                "{name}: meter {} draws a line",
                meter.key
            );
        }
    }
}

/// The fake profiles answer to product ids no Schiit device carries.
#[test]
fn the_fake_profiles_keep_to_their_own_product_ids() {
    let r = Registry::builtin().unwrap();
    let fakes = [
        (65534, "fake-amp"),
        (65533, "fake-mono"),
        (65532, "fake-five"),
    ];
    for (product_id, name) in fakes {
        assert_eq!(r.select(product_id, 3).unwrap().name, name);
    }
    for name in r.names().iter().filter(|n| !n.starts_with("fake-")) {
        if let Some(id) = r.get(name).unwrap().product_id {
            assert!(
                fakes.iter().all(|(fake_id, _)| *fake_id != id),
                "{name} claims a fake's product id"
            );
        }
    }
}

#[test]
fn every_released_product_has_a_profile() {
    let r = Registry::builtin().unwrap();
    let released = [
        1, 2, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15, 17, 18, 19, 20, 21, 23, 24, 27,
    ];
    for id in released {
        assert!(r.select(id, 99).is_some(), "product {id}");
    }
    for unreleased in [3, 9, 16, 22] {
        assert!(r.select(unreleased, 99).is_none(), "product {unreleased}");
    }
}

#[test]
fn gungnir_profiles_switch_on_firmware() {
    let r = Registry::builtin().unwrap();
    assert_eq!(r.select(2, 1).unwrap().name, "gungnir2-v1");
    assert_eq!(r.select(2, 2).unwrap().name, "gungnir2-v2");
    assert!(r.select(2, 1).unwrap().eq.is_none());
    assert_eq!(r.select(2, 2).unwrap().eq.as_ref().unwrap().bands.len(), 5);
}

#[test]
fn concrete_profiles_are_well_formed() {
    let r = Registry::builtin().unwrap();
    for name in r.names() {
        let p = r.get(name).unwrap();
        let mut keys = BTreeSet::new();
        let mut ids = BTreeSet::new();
        for c in &p.controls {
            assert!(!c.label.is_empty(), "{name}: {} has no label", c.key);
            assert!(
                keys.insert(c.key.clone()),
                "{name}: duplicate key {}",
                c.key
            );
            assert!(ids.insert(c.id), "{name}: duplicate id {}", c.id);
            if c.kind == ControlKind::Level {
                assert!(c.range.is_some(), "{name}: level {} without range", c.key);
            }
            if c.kind == ControlKind::Fault {
                assert!(c.advice.is_some(), "{name}: fault {} without advice", c.key);
            }
        }
        if p.product_id.is_some() {
            assert_ne!(
                p.display, p.name,
                "{name}: concrete profile needs a display name"
            );
        }
    }
}

/// Looks up one control of a built-in profile by key.
fn control(name: &str, key: &str) -> svolder_profiles::Control {
    let r = Registry::builtin().unwrap();
    r.get(name)
        .unwrap()
        .control_by_key(key)
        .unwrap_or_else(|| panic!("{name} has no {key}"))
        .clone()
}

/// The `values` of a control as `n=name` pairs.
fn values(name: &str, key: &str) -> Vec<String> {
    control(name, key)
        .values
        .iter()
        .map(|(n, v)| format!("{n}={v}"))
        .collect()
}

#[test]
fn bifrost3_has_its_own_controls() {
    let r = Registry::builtin().unwrap();
    let b = r.select(10, 0).unwrap();
    assert_eq!(b.display, "Bifrost 3");
    assert_eq!(b.eq, r.get("mesh-eq-5").unwrap().eq);
    assert_eq!(
        values("bifrost3", "input"),
        ["1=USB", "2=Optical", "3=Coaxial"]
    );
    let usb = control("bifrost3", "usb_connected");
    assert_eq!(usb.id.to_string(), "7");
    assert_eq!(usb.label, "USB connected");
    assert_eq!(usb.value_type, svolder_proto::ValueType::Bool);
    let lock = control("bifrost3", "spdif_locked");
    assert_eq!(lock.id.to_string(), "8");
    assert_eq!(lock.label, "S/PDIF locked");
    assert_eq!(lock.value_type, svolder_proto::ValueType::Bool);
    // The Bifrost 3 reports the lock as an integer, which reads true when it is 1.
    assert!(lock.reads_true(&svolder_proto::Value::U8(1)));
    assert!(!lock.reads_true(&svolder_proto::Value::U8(0)));
    assert!(!lock.reads_true(&svolder_proto::Value::U8(2)));
    assert!(b.control_by_key("usb1_connected").is_none());
    assert!(b.control_by_key("usb2_connected").is_none());
}

#[test]
fn input_names_follow_the_app() {
    let four = ["1=USB 1", "2=USB 2", "3=Optical", "4=Coaxial"];
    for name in ["gungnir2-v1", "gungnir2-v2"] {
        let mut want = four.to_vec();
        want.push("5=AES");
        assert_eq!(values(name, "input"), want, "{name}");
    }
    assert_eq!(
        values("yggdrasil", "input"),
        [
            "1=USB 1",
            "2=USB 2",
            "3=Optical",
            "4=Coaxial",
            "5=BNC",
            "6=AES"
        ]
    );
    assert_eq!(
        values("mimir", "input"),
        ["1=USB 1", "2=Optical", "3=Coaxial", "4=AES"]
    );
    assert_eq!(values("lokimax", "input"), ["1=XLR", "2=RCA"]);
    assert_eq!(values("lokimax", "mode"), ["1=Process", "2=Bypass"]);
}

#[test]
fn the_dacs_carry_the_readouts_the_app_reads() {
    for name in ["gungnir2-v1", "gungnir2-v2", "yggdrasil"] {
        for (key, id) in [
            ("usb1_connected", "7.1"),
            ("usb2_connected", "7.2"),
            ("spdif_locked", "8"),
        ] {
            let c = control(name, key);
            assert_eq!(c.id.to_string(), id, "{name} {key}");
            assert_eq!(c.value_type, svolder_proto::ValueType::Bool);
        }
    }
    for name in ["meshcard", "eitr2"] {
        let c = control(name, "usb_connected");
        assert_eq!(c.id.to_string(), "10", "{name}");
        assert_eq!(c.label, "USB connected");
        assert_eq!(c.value_type, svolder_proto::ValueType::Bool);
    }
    let r = Registry::builtin().unwrap();
    for name in r.names() {
        let p = r.get(name).unwrap();
        if p.family == svolder_profiles::Family::Dac {
            assert!(p.control_by_key("fault_overcurrent").is_none(), "{name}");
        }
    }
    for name in [
        "mesh-eq-5",
        "mesh-eq-3",
        "gungnir2-v2",
        "bifrost3",
        "mimir",
        "meshcard",
        "eitr2",
    ] {
        let c = control(name, "balance");
        assert_eq!(c.range, Some((1, 127)), "{name}");
        assert_eq!(c.center, Some(64), "{name}");
    }
}

#[test]
fn loki_max_has_six_band_levels() {
    let spans = [12.0, 9.0, 6.0, 6.0, 9.0, 12.0];
    let labels = ["20 Hz", "120 Hz", "550 Hz", "2 kHz", "6 kHz", "20 kHz"];
    for b in 0..6 {
        let c = control("lokimax", &format!("band{}", b + 1));
        assert_eq!(c.id.to_string(), format!("6.{}.3", b + 1));
        assert_eq!(c.kind, ControlKind::Level);
        assert_eq!(c.range, Some((0, 30)));
        assert_eq!(c.unit.as_deref(), Some("dB"));
        assert_eq!(c.label, labels[b]);
        assert_eq!(
            c.convert,
            Some(svolder_profiles::Convert::Linear {
                min: -spans[b],
                max: spans[b]
            })
        );
    }
}

#[test]
fn the_loki_max_has_recall_and_save_with_three_slots_on_7_2_and_7_1() {
    let registry = Registry::builtin().unwrap();
    let loki = registry.get("lokimax").unwrap();
    assert_eq!(loki.actions.len(), 2);
    let recall = svolder_profiles::roles::recall_action(loki).expect("a recall action");
    assert_eq!(recall.key, "recall");
    assert_eq!(recall.label, "Recall preset");
    assert_eq!(recall.id.to_string(), "7.2");
    assert_eq!(recall.slots, 3);
    assert!(!recall.overwrites);
    assert_eq!(
        recall.changes,
        ["band1", "band2", "band3", "band4", "band5", "band6"]
    );
    let save = svolder_profiles::roles::save_action(loki).expect("a save action");
    assert_eq!(save.key, "save");
    assert_eq!(save.label, "Save preset");
    assert_eq!(save.id.to_string(), "7.1");
    assert_eq!(save.slots, 3);
    assert!(save.overwrites);
    assert!(save.changes.is_empty());
    assert!(loki.control_by_key("recall").is_none() && loki.control_by_key("save").is_none());
    let (action, slot) = loki.action_of(FunctionId::three(7, 2, 3)).unwrap();
    assert_eq!((action.key.as_str(), slot), ("recall", 3));
    assert!(loki.action_of(FunctionId::three(7, 2, 4)).is_none());
    for name in registry.names() {
        if name != "lokimax" {
            assert!(registry.get(name).unwrap().actions.is_empty(), "{name}");
        }
    }
}

#[test]
fn the_loki_max_builtin_settings_are_the_apps_five_in_its_order_with_its_values() {
    let loki = Registry::builtin().unwrap().get("lokimax").unwrap().clone();
    let table = loki.builtin_settings.expect("a built-in table");
    assert_eq!(
        table.keys,
        ["band1", "band2", "band3", "band4", "band5", "band6"]
    );
    let ids: Vec<String> = table.ids.iter().map(ToString::to_string).collect();
    assert_eq!(ids, ["6.1", "6.2", "6.3", "6.4", "6.5", "6.6"]);
    let settings: Vec<(&str, &[i64])> = table
        .settings
        .iter()
        .map(|s| (s.name.as_str(), s.raw.as_slice()))
        .collect();
    assert_eq!(
        settings,
        [
            ("Flat", &[15, 15, 15, 15, 15, 15][..]),
            ("Bass Boost", &[28, 21, 17, 15, 15, 15][..]),
            ("Treble Boost", &[15, 15, 15, 17, 21, 28][..]),
            ("Smiley", &[23, 16, 8, 8, 16, 23][..]),
            ("Loudness", &[30, 22, 18, 15, 23, 28][..]),
        ]
    );
    assert!(loki.eq.is_none());
    let registry = Registry::builtin().unwrap();
    for name in registry.names() {
        if name != "lokimax" {
            assert!(
                registry.get(name).unwrap().builtin_settings.is_none(),
                "{name}"
            );
        }
    }
}

#[test]
fn the_loki_max_bands_read_their_two_id_form() {
    let loki = Registry::builtin().unwrap().get("lokimax").unwrap().clone();
    for band in 1..=6u8 {
        let control = loki.control_by_key(&format!("band{band}")).unwrap();
        assert_eq!(control.reads, [FunctionId::two(6, band)]);
        for id in [
            FunctionId::two(6, band),
            FunctionId::three(6, band, 3),
            FunctionId::three(6, band, 7),
        ] {
            assert_eq!(loki.read_as(id), control.id, "{id}");
        }
    }
    for id in [
        FunctionId::two(6, 7),
        FunctionId::two(6, 0),
        FunctionId::one(6),
    ] {
        assert_eq!(loki.read_as(id), id, "{id}");
    }
}

#[test]
fn shared_keys_mean_the_same_thing_everywhere() {
    let r = Registry::builtin().unwrap();
    for name in r.names() {
        let p = r.get(name).unwrap();
        if let Some(m) = p.control_by_key("mute") {
            assert_eq!(m.kind, ControlKind::Toggle, "{name}");
        }
        if let Some(v) = p.control_by_key("volume") {
            assert_eq!(v.kind, ControlKind::Level, "{name}");
            assert_eq!(v.unit.as_deref(), Some("dB"), "{name}");
        }
        if let Some(i) = p.control_by_key("input") {
            assert_eq!(i.kind, ControlKind::Select, "{name}");
        }
    }
}

/// A product is named by the codec's table where it knows the id, and by the display of the
/// profile made for it where it does not — which is how the fake amp gets a name at all.
#[test]
fn a_product_the_codec_does_not_know_is_named_by_its_profile() {
    let r = Registry::builtin().unwrap();
    assert_eq!(r.product_name(65534), Some("Fake amp"));
    assert_eq!(r.product_name(65533), Some("Fake mono"));
    assert_eq!(r.product_name(65532), Some("Fake five"));
    assert_eq!(r.product_name(2), Some("Gungnir 2"), "the codec's name");
    assert_eq!(r.product_name(10), Some("Bifrost 3"), "the codec's name");
    assert_eq!(r.product_name(0), None, "no codec entry and no profile");

    // The codec's table wins where it knows the id, whatever a profile for it is called.
    let renamed = Registry::from_sources(vec![(
        "renamed".into(),
        "schema = 1\nname = \"renamed\"\ndisplay = \"My DAC\"\nproduct_id = 2\n".into(),
    )])
    .unwrap();
    assert_eq!(renamed.product_name(2), Some("Gungnir 2"));
}

#[test]
fn the_preamps_name_their_values_as_the_app_does() {
    assert_eq!(
        values("freya", "mode"),
        ["1=Passive", "2=Buffer", "3=Tube Gain"]
    );
    for name in ["kara", "saga2"] {
        assert_eq!(
            values(name, "mode"),
            ["1=Passive", "2=Low Gain", "3=High Gain"],
            "{name}"
        );
    }
    let power = control("saga2", "power");
    assert_eq!(power.label, "Power");
    assert_eq!(power.kind, ControlKind::Readout);

    let r = Registry::builtin().unwrap();
    assert_eq!(
        r.get("lyr5").unwrap().family,
        svolder_profiles::Family::Preamp
    );
    assert_eq!(values("lyr5", "input"), ["1=RCA 1", "2=RCA 2"]);
    assert_eq!(
        values("lyr5", "gain"),
        ["1=Low Gain (1x)", "2=High Gain (7x)"]
    );
    assert_eq!(values("lyr5", "mode"), ["1=Solid State", "2=Tube"]);
    assert_eq!(
        values("lyr5", "class_state"),
        ["1=Class A", "2=Class AB", "3=Warning"]
    );
    let preout = control("lyr5", "preout");
    assert_eq!(preout.id.to_string(), "4.2");
    assert_eq!(preout.label, "Pre-out");
    assert_eq!(preout.kind, ControlKind::Toggle);
    assert!(
        r.get("lyr5")
            .unwrap()
            .control_by_key("preout_mute")
            .is_none()
    );
}

#[test]
fn the_amps_follow_the_app() {
    let r = Registry::builtin().unwrap();
    for name in ["aegir2", "vidar2", "kraken", "wotan", "tyr"] {
        let power = control(name, "power");
        assert_eq!(power.id.to_string(), "10", "{name}");
        assert_eq!(power.label, "Power", "{name}");
        assert_eq!(power.kind, ControlKind::Toggle, "{name}");
        let warm = control(name, "warming_up");
        assert_eq!(warm.id.to_string(), "4", "{name}");
        assert_eq!(warm.label, "Warming up", "{name}");
        assert_eq!(warm.kind, ControlKind::Readout, "{name}");
        assert_eq!(warm.value_type, svolder_proto::ValueType::Bool, "{name}");
        assert_eq!(warm.only_while.as_deref(), Some("power"), "{name}");
        let p = r.get(name).unwrap();
        assert!(p.control_by_key("standby").is_none(), "{name}");
        assert!(p.control_by_key("mute").is_none(), "{name}");
        let t = control(name, "temperature_reporting");
        assert_eq!(t.kind, ControlKind::Readout, "{name}");
        assert_eq!(t.value_type, svolder_proto::ValueType::Bool, "{name}");
        assert!(t.auto_off, "{name}");
    }
    assert!(
        r.get("gjallarhorn")
            .unwrap()
            .control_by_key("power")
            .is_none()
    );
    let mute = control("gjallarhorn", "mute");
    assert_eq!(mute.id.to_string(), "4");
    assert_eq!(mute.kind, ControlKind::Toggle);
    assert_eq!(values("wotan", "input"), ["1=XLR", "2=RCA"]);
}

#[test]
fn the_faults_sit_on_the_ids_the_app_accepts() {
    let ids = |name: &str, key: &str| control(name, key).id.to_string();
    assert_eq!(ids("wotan", "fault_overcurrent"), "230.1.1");
    assert_eq!(ids("wotan", "fault_overcurrent_r"), "230.1.2");
    assert_eq!(ids("wotan", "fault_overtemp"), "230.2.1");
    assert_eq!(ids("wotan", "fault_overtemp_r"), "230.2.2");
    assert_eq!(
        control("wotan", "fault_overcurrent").label,
        "Over-current L"
    );
    assert_eq!(
        control("wotan", "fault_overcurrent_r").label,
        "Over-current R"
    );
    assert_eq!(
        control("wotan", "fault_overtemp").label,
        "Over-temperature L"
    );
    assert_eq!(
        control("wotan", "fault_overtemp_r").label,
        "Over-temperature R"
    );
    assert_eq!(
        control("wotan", "fault_overtemp_r").advice,
        control("aegir2", "fault_overtemp").advice
    );
    assert_eq!(ids("gjallarhorn", "fault_overcurrent"), "230.1.1");
    assert_eq!(ids("gjallarhorn", "fault_overcurrent_r"), "230.1.2");
    assert_eq!(
        control("gjallarhorn", "fault_overcurrent").label,
        "Over-current L"
    );
    assert_eq!(ids("gjallarhorn", "fault_overtemp"), "230.2");
    assert_eq!(ids("tyr", "fault_overtemp"), "230.2.1");
    assert_eq!(ids("tyr", "fault_overcurrent"), "230.1");
    for name in ["aegir2", "vidar2", "kraken"] {
        assert_eq!(ids(name, "fault_overcurrent"), "230.1", "{name}");
        assert_eq!(ids(name, "fault_overtemp"), "230.2", "{name}");
    }
}

#[test]
fn the_fakes_keep_a_mute_of_their_own() {
    for name in ["fake-amp", "fake-five", "fake-mono"] {
        let m = control(name, "mute");
        assert_eq!(m.kind, ControlKind::Toggle, "{name}");
        assert_eq!(m.id.to_string(), "4", "{name}");
    }
}

#[test]
fn the_phonos_name_their_values_per_device() {
    let caps = ["1=50 pF", "2=100 pF", "3=150 pF", "4=200 pF"];
    for name in ["skoll", "stjarna"] {
        assert_eq!(values(name, "load_capacitance"), caps, "{name}");
    }
    assert_eq!(values("skoll", "input"), ["1=Balanced", "2=Single-Ended"]);
    assert_eq!(
        values("skoll", "load_resistance"),
        ["1=10 Ω", "2=50 Ω", "3=100 Ω", "4=150 Ω", "5=47 kΩ"]
    );
    assert_eq!(values("stjarna", "input"), ["1=RCA 1", "2=RCA 2"]);
    assert_eq!(
        values("stjarna", "load_resistance"),
        ["1=10 Ω", "2=50 Ω", "3=100 Ω", "4=150 Ω", "5=200 Ω"]
    );
    let power = control("stjarna", "power");
    assert_eq!(power.id.to_string(), "10");
    assert_eq!(power.label, "Power");
    assert_eq!(power.kind, ControlKind::Toggle);
    let lock = control("stjarna", "remote_lock");
    assert_eq!(lock.id.to_string(), "201");
    assert_eq!(lock.label, "Remote lock");
    assert_eq!(lock.kind, ControlKind::Readout);
    assert_eq!(lock.value_type, svolder_proto::ValueType::Bool);
    let r = Registry::builtin().unwrap();
    for key in ["input", "load_capacitance", "load_resistance"] {
        assert!(
            r.get("phono-common")
                .unwrap()
                .control_by_key(key)
                .unwrap()
                .values
                .is_empty()
        );
    }
}

#[test]
fn freya2_and_hati_follow_the_app() {
    assert_eq!(
        values("freya2", "mode"),
        [
            "0=Passive",
            "1=SS voltage / SS current",
            "2=Tube voltage / SS current",
            "3=SS voltage / Tube current",
            "4=Tube voltage / Tube current"
        ]
    );
    assert_eq!(
        values("freya2", "gain"),
        ["1=Low Gain (1x)", "2=High Gain (4x)"]
    );
    for name in ["freya2", "hati"] {
        let power = control(name, "power");
        assert_eq!(power.id.to_string(), "10", "{name}");
        assert_eq!(power.label, "Power", "{name}");
    }
    assert_eq!(values("hati", "input"), ["1=RCA 1", "2=RCA 2"]);
    assert_eq!(values("hati", "load_capacitance").len(), 8);
    assert_eq!(values("hati", "load_resistance").len(), 17);
    assert_eq!(control("hati", "remote_lock").label, "Remote lock");
}

/// The models with a power state, and which of them also report the warming-up readout that
/// takes part in it.
const POWER_STATE: &[(&str, bool)] = &[
    ("freya2", false),
    ("stjarna", false),
    ("hati", false),
    ("saga2", false),
    ("aegir2", true),
    ("vidar2", true),
    ("kraken", true),
    ("tyr", true),
    ("wotan", true),
];

/// Exactly the models with a power state carry the `power` control, on function 10, and the
/// amplifiers among them carry the `warming_up` readout on function 4; no other profile has
/// either key, since the session reads a device coming back on from them. Base profiles that
/// no product is named after are not counted.
#[test]
fn exactly_the_power_state_models_carry_the_power_keys() {
    let r = Registry::builtin().unwrap();
    for name in r.names() {
        let profile = r.get(name).unwrap();
        // A base profile only names what its products share; the products are what count.
        if profile.product_id.is_none() {
            continue;
        }
        let expected = POWER_STATE.iter().find(|(n, _)| *n == name);
        let power = profile.control_by_key("power");
        let warming = profile.control_by_key("warming_up");
        match expected {
            Some((_, amp)) => {
                assert_eq!(
                    power.map(|c| c.id),
                    Some(FunctionId::one(10)),
                    "{name}: power on function 10"
                );
                if *amp {
                    assert_eq!(
                        warming.map(|c| c.id),
                        Some(FunctionId::one(4)),
                        "{name}: warming_up on function 4"
                    );
                } else {
                    assert!(warming.is_none(), "{name} has no warming_up readout");
                }
            }
            None => {
                assert!(power.is_none(), "{name} has no power state");
                assert!(warming.is_none(), "{name} has no power state");
            }
        }
    }
    for (name, _) in POWER_STATE {
        assert!(r.get(name).is_some(), "{name} is a built-in profile");
    }
}

/// A boolean readout is worded by its key, one rule for every front end: `_connected` keys read
/// `connected` and `not connected`, `_locked` keys `locked` and `unlocked`, the rest `yes`, `no`.
#[test]
fn a_boolean_readout_is_worded_by_its_key() {
    use svolder_profiles::FlagWords;
    let usb = control("bifrost3", "usb_connected").flag_words();
    assert_eq!((usb.on(), usb.off()), ("connected", "not connected"));
    assert_eq!(usb, FlagWords::Connected);
    let spdif = control("bifrost3", "spdif_locked").flag_words();
    assert_eq!((spdif.on(), spdif.off()), ("locked", "unlocked"));
    let plain = control("kara", "headphone").flag_words();
    assert_eq!((plain.on(), plain.off()), ("yes", "no"));
}
