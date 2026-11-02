// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::HashMap;

use super::*;
use crate::registry::Registry;
use crate::test_support::tone;
use crate::tone::apo::Correction;

/// A device's readings, by function.
#[derive(Default)]
struct Device(HashMap<FunctionId, i64>);

impl Device {
    /// Every band of `eq` at `f`, `g` and `q`, in `mode` at loudness `loudness`.
    fn at(eq: &EqSpec, [f, g, q]: [u8; 3], mode: EqMode, loudness: u8) -> Device {
        let mut device = Device::default();
        for band in 0..eq.bands.len() {
            device.set(eq, band, [f, g, q]);
        }
        device.0.insert(eq.mode_id(), i64::from(mode as u8));
        device.0.insert(eq.loudness_id(), i64::from(loudness));
        device
    }

    /// One band at `f`, `g` and `q`.
    fn set(&mut self, eq: &EqSpec, band: usize, raws: [u8; 3]) {
        for (param, raw) in [EqParam::Frequency, EqParam::Gain, EqParam::Q]
            .into_iter()
            .zip(raws)
        {
            self.0
                .insert(eq.band_id(band, param).unwrap(), i64::from(raw));
        }
    }

    fn read(&self) -> impl Fn(FunctionId) -> Option<i64> + '_ {
        |id| self.0.get(&id).copied()
    }

    /// The device after every write of `plan`.
    fn apply(&mut self, eq: &EqSpec, plan: &Plan) {
        for write in &plan.writes {
            self.0.insert(write.id(eq).unwrap(), i64::from(write.raw()));
        }
    }
}

/// One band's plan is ordered as a recall's is: a gain coming down before the frequency and
/// Q, one going up after them, and a parameter already on its step left out. An unread gain
/// counts as coming down to a cut and going up to a boost. A band the tone control does not
/// have plans nothing.
#[test]
fn a_band_plan_orders_one_band_as_a_recall_does() {
    let eq = tone("mesh-eq-5");
    let mut device = Device::at(&eq, [64, 98, 64], EqMode::Eq, 0);
    let (cut, boost) = (eq.gain_raw(-3.0), eq.gain_raw(3.0));
    assert_eq!(
        band_plan(&eq, device.read(), 1, [Some(85), Some(cut), Some(64)]),
        [gain(1, cut), frequency(1, 85)]
    );
    assert_eq!(
        band_plan(&eq, device.read(), 1, [Some(85), Some(boost), Some(17)]),
        [frequency(1, 85), q(1, 17), gain(1, boost)]
    );
    assert!(band_plan(&eq, device.read(), 1, [Some(64), Some(98), None]).is_empty());
    assert!(band_plan(&eq, device.read(), 5, [Some(85), None, None]).is_empty());

    device.0.remove(&eq.band_id(1, EqParam::Gain).unwrap());
    assert_eq!(
        band_plan(&eq, device.read(), 1, [Some(85), Some(cut), None]),
        [gain(1, cut), frequency(1, 85)]
    );
    assert_eq!(
        band_plan(&eq, device.read(), 1, [Some(85), Some(boost), None]),
        [frequency(1, 85), gain(1, boost)]
    );
}

fn gain(band: usize, raw: u8) -> Write {
    Write::Band {
        band,
        param: EqParam::Gain,
        raw,
    }
}

fn frequency(band: usize, raw: u8) -> Write {
    Write::Band {
        band,
        param: EqParam::Frequency,
        raw,
    }
}

fn q(band: usize, raw: u8) -> Write {
    Write::Band {
        band,
        param: EqParam::Q,
        raw,
    }
}

/// Flat on `eq`, with some bands changed.
fn preset(eq: &EqSpec, mode: EqMode, loudness: u8, changes: &[(usize, PresetBand)]) -> Preset {
    let mut preset = builtins(eq).remove(0);
    preset.name = "test".into();
    preset.mode = mode;
    preset.loudness = loudness;
    for &(band, values) in changes {
        preset.band[band] = values;
    }
    preset
}

#[test]
fn every_builtin_fits_both_tone_controls() {
    for profile in ["mesh-eq-5", "mesh-eq-3"] {
        let eq = tone(profile);
        let presets = builtins(&eq);
        let names: Vec<&str> = presets.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, BUILTIN_NAMES, "{profile}");
        assert_eq!(names, builtin_names(eq.bands.len()), "{profile}");
        for preset in &presets {
            let what = format!("{profile} {}", preset.name);
            assert_eq!(usize::from(preset.bands), eq.bands.len(), "{what}");
            assert_eq!(preset.band.len(), eq.bands.len(), "{what}");
            assert_eq!((preset.mode, preset.loudness), (EqMode::Eq, 0), "{what}");
            for (band, values) in preset.band.iter().enumerate() {
                let (low, high) = (
                    eq.frequency(band, 0).unwrap(),
                    eq.frequency(band, 127).unwrap(),
                );
                assert!(
                    (low..=high).contains(&values.f),
                    "{what} band {band}: {} Hz outside {low}..{high}",
                    values.f
                );
                let (gain_min, gain_max) = eq.gain_range;
                assert!(
                    (gain_min..=gain_max).contains(&values.g),
                    "{what} band {band}"
                );
                let (q_min, q_max) = eq.q_range;
                assert!((q_min..=q_max).contains(&values.q), "{what} band {band}");
            }
            let plan = plan_recall(&eq, |_| None, preset).unwrap();
            assert_eq!(plan.clamped, [], "{what}");
        }
    }
}

#[test]
fn the_builtins_carry_the_spec_values_on_the_mesh_tone_controls() {
    let neutral = |f| band(f, 0.0, 4.3);
    let five: Vec<Vec<PresetBand>> = builtins(&tone("mesh-eq-5"))
        .into_iter()
        .map(|p| p.band)
        .collect();
    assert_eq!(
        five,
        [
            vec![
                neutral(40.0),
                neutral(160.0),
                neutral(632.0),
                neutral(2515.0),
                neutral(10_000.0),
            ],
            vec![
                band(40.0, 3.0, 0.7),
                band(160.0, 1.5, 0.7),
                neutral(632.0),
                neutral(2515.0),
                band(10_000.0, 2.0, 0.7),
            ],
            vec![
                band(40.0, 4.0, 0.7),
                band(160.0, 2.0, 0.7),
                neutral(632.0),
                neutral(2515.0),
                neutral(10_000.0),
            ],
            vec![
                neutral(40.0),
                neutral(160.0),
                neutral(632.0),
                band(3000.0, 2.5, 1.4),
                neutral(10_000.0),
            ],
        ]
    );
    let three: Vec<Vec<PresetBand>> = builtins(&tone("mesh-eq-3"))
        .into_iter()
        .map(|p| p.band)
        .collect();
    assert_eq!(
        three,
        [
            vec![neutral(63.0), neutral(630.0), neutral(6300.0)],
            vec![band(63.0, 3.0, 0.7), neutral(630.0), band(6300.0, 2.0, 0.7)],
            vec![band(63.0, 4.0, 0.7), neutral(630.0), neutral(6300.0)],
            vec![neutral(63.0), neutral(630.0), band(3000.0, 2.5, 1.0)],
        ]
    );
}

#[test]
fn the_flat_builtin_is_every_band_at_its_centre() {
    for profile in ["mesh-eq-5", "mesh-eq-3"] {
        let eq = tone(profile);
        let flat = &builtins(&eq)[0];
        for (band, values) in flat.band.iter().enumerate() {
            assert_eq!(
                eq.frequency_raw(band, values.f),
                Some(64),
                "{profile} {band}"
            );
            assert_eq!(eq.gain_raw(values.g), eq.gain_zero, "{profile} {band}");
            assert_eq!(eq.q_raw(values.q), 64, "{profile} {band}");
        }
    }
}

#[test]
fn flat_and_the_neutral_bands_sit_on_the_devices_own_centres() {
    let mut eq = tone("mesh-eq-5");
    eq.bands[3].center = 2520.0;
    eq.q_center = 3.9;
    let presets = builtins(&eq);
    for preset in &presets[..3] {
        assert_eq!(preset.band[3], band(2520.0, 0.0, 3.9), "{}", preset.name);
        assert_eq!(eq.frequency_raw(3, preset.band[3].f), Some(64));
        assert_eq!(eq.q_raw(preset.band[3].q), 64);
    }
    assert_eq!(presets[0].band[0], band(40.0, 0.0, 3.9));
    // Presence moves band 4 to its own fixed frequency whatever the centre.
    assert_eq!(presets[3].band[3], band(3000.0, 2.5, 1.4));
}

#[test]
fn another_band_count_gets_flat_alone_on_its_own_centres() {
    let mut eq = tone("mesh-eq-5");
    eq.bands.pop();
    eq.bands[0].center = 50.0;
    let presets = builtins(&eq);
    assert_eq!(presets.len(), 1);
    assert_eq!(presets[0].name, "Flat");
    assert_eq!(presets[0].bands, 4);
    assert_eq!(
        presets[0].band,
        [
            band(50.0, 0.0, 4.3),
            band(160.0, 0.0, 4.3),
            band(632.0, 0.0, 4.3),
            band(2515.0, 0.0, 4.3),
        ]
    );
    assert_eq!(builtin_names(4), ["Flat"]);
    assert_eq!(builtin_names(5), BUILTIN_NAMES);
    assert_eq!(builtin_names(3), BUILTIN_NAMES);
}

#[test]
fn builtin_names_are_matched_in_any_case() {
    assert!(is_builtin("Flat"));
    assert!(is_builtin("BASS"));
    assert!(is_builtin("warm"));
    assert!(!is_builtin("Warmer"));
    assert!(!is_builtin("HD650 warm"));
    assert!(
        is_builtin(" flat\t"),
        "the blanks round a name count for nothing"
    );
}

/// A name is kept without the blanks round it; a blank one, one longer than forty
/// characters and one holding a control character are refused, each with its own words.
#[test]
fn a_name_is_trimmed_and_a_blank_long_or_control_one_is_refused() {
    assert_eq!(checked_name(" HD650 warm "), Ok("HD650 warm"));
    assert_eq!(checked_name("\tHD650  warm\n"), Ok("HD650  warm"));
    assert_eq!(checked_name(""), Err(NameError::Blank));
    assert_eq!(checked_name(" \t "), Err(NameError::Blank));
    for inside in ["new\nline", "tab\there", "bell\u{7}", "del\u{7f}"] {
        assert_eq!(checked_name(inside), Err(NameError::Control), "{inside:?}");
    }
    let longest = "é".repeat(NAME_MAX);
    assert_eq!(
        checked_name(&format!("  {longest}  ")),
        Ok(longest.as_str())
    );
    assert_eq!(
        checked_name(&"x".repeat(NAME_MAX + 1)),
        Err(NameError::TooLong(NAME_MAX + 1))
    );
    assert_eq!(NameError::Blank.to_string(), "a preset needs a name");
    assert_eq!(
        NameError::TooLong(41).to_string(),
        "a preset's name is at most 40 characters, not 41"
    );
    assert!(NameError::Control.to_string().contains("newline"));

    assert!(same_name(" HD650 Warm ", "hd650 warm"));
    assert!(!same_name("HD650 warm", "HD650  warm"));
}

#[test]
fn a_mixed_curve_lowers_then_reshapes_then_raises_then_switches_to_eq() {
    // The device's strength, 10, is left alone: an EQ preset does not set it.
    let eq = tone("mesh-eq-5");
    // Band 1 boosted, band 2 cut, everything else at the centre, in Flat mode.
    let mut device = Device::at(&eq, [64, eq.gain_zero, 64], EqMode::Flat, 10);
    device.set(&eq, 0, [64, eq.gain_raw(4.0), 64]);
    device.set(&eq, 1, [64, eq.gain_raw(-6.0), 64]);
    let preset = preset(
        &eq,
        EqMode::Eq,
        0,
        &[
            (0, band(40.0, 0.0, 4.3)),
            (1, band(160.0, 2.0, 4.3)),
            (2, band(800.0, 0.0, 4.3)),
            (3, band(2515.0, 0.0, 1.0)),
            (4, band(10_000.0, -3.0, 4.3)),
        ],
    );
    let plan = plan_recall(&eq, device.read(), &preset).unwrap();
    assert_eq!(
        plan.writes,
        [
            gain(0, eq.gain_zero),
            gain(4, eq.gain_raw(-3.0)),
            frequency(2, eq.frequency_raw(2, 800.0).unwrap()),
            q(3, eq.q_raw(1.0)),
            gain(1, eq.gain_raw(2.0)),
            Write::Mode(EqMode::Eq),
        ]
    );
    assert_eq!(plan.clamped, []);
}

#[test]
fn leaving_eq_switches_the_mode_first() {
    let eq = tone("mesh-eq-3");
    let device = Device::at(&eq, [64, eq.gain_zero, 64], EqMode::Eq, 0);
    let boost = [(0, band(63.0, 3.0, 4.3))];

    let flat = plan_recall(&eq, device.read(), &preset(&eq, EqMode::Flat, 20, &boost)).unwrap();
    assert_eq!(
        flat.writes,
        [Write::Mode(EqMode::Flat), gain(0, eq.gain_raw(3.0))],
        "a Flat preset leaves the strength alone"
    );

    let loud = plan_recall(
        &eq,
        device.read(),
        &preset(&eq, EqMode::Loudness, 20, &boost),
    )
    .unwrap();
    assert_eq!(
        loud.writes,
        [
            Write::Loudness(20),
            Write::Mode(EqMode::Loudness),
            gain(0, eq.gain_raw(3.0)),
        ],
        "the strength is set before the device enters Loudness"
    );
}

#[test]
fn a_strength_change_alone_goes_where_the_mode_would() {
    let eq = tone("mesh-eq-5");
    let device = Device::at(&eq, [64, eq.gain_zero, 64], EqMode::Loudness, 10);
    let plan = plan_recall(
        &eq,
        device.read(),
        &preset(&eq, EqMode::Loudness, 40, &[(0, band(40.0, -2.0, 4.3))]),
    )
    .unwrap();
    assert_eq!(
        plan.writes,
        [Write::Loudness(40), gain(0, eq.gain_raw(-2.0))]
    );
}

#[test]
fn a_gain_with_no_reading_goes_by_where_it_ends() {
    let eq = tone("mesh-eq-3");
    let mut device = Device::at(&eq, [64, eq.gain_zero, 64], EqMode::Eq, 0);
    device.0.remove(&eq.band_id(0, EqParam::Gain).unwrap());
    device.0.remove(&eq.band_id(2, EqParam::Gain).unwrap());
    let preset = preset(
        &eq,
        EqMode::Eq,
        0,
        &[(0, band(63.0, 2.0, 4.3)), (2, band(6300.0, -2.0, 4.3))],
    );
    let plan = plan_recall(&eq, device.read(), &preset).unwrap();
    assert_eq!(
        plan.writes,
        [gain(2, eq.gain_raw(-2.0)), gain(0, eq.gain_raw(2.0))]
    );
}

#[test]
fn recalling_onto_the_same_curve_writes_nothing() {
    for profile in ["mesh-eq-5", "mesh-eq-3"] {
        let eq = tone(profile);
        for preset in builtins(&eq) {
            let mut device = Device::at(&eq, [10, 20, 30], EqMode::Loudness, 90);
            let first = plan_recall(&eq, device.read(), &preset).unwrap();
            assert!(!first.writes.is_empty(), "{profile} {}", preset.name);
            assert!(
                !matches(&eq, device.read(), &preset),
                "{profile} {}",
                preset.name
            );
            device.apply(&eq, &first);
            let again = plan_recall(&eq, device.read(), &preset).unwrap();
            assert_eq!(again, Plan::default(), "{profile} {}", preset.name);
            assert!(
                matches(&eq, device.read(), &preset),
                "{profile} {}",
                preset.name
            );
        }
    }
}

#[test]
fn a_reading_that_is_no_step_is_written_over() {
    let eq = tone("mesh-eq-3");
    let flat = &builtins(&eq)[0];
    let mut device = Device::at(&eq, [64, eq.gain_zero, 64], EqMode::Eq, 0);
    device
        .0
        .insert(eq.band_id(1, EqParam::Q).unwrap(), 64 + 256);
    let plan = plan_recall(&eq, device.read(), flat).unwrap();
    assert_eq!(plan.writes, [q(1, 64)]);
}

#[test]
fn out_of_range_values_are_clamped_and_reported() {
    let eq = tone("mesh-eq-5");
    let preset = preset(
        &eq,
        EqMode::Eq,
        200,
        &[
            (0, band(5.0, 0.0, 4.3)),
            (1, band(160.0, 9.0, 0.2)),
            (4, band(22_000.0, -30.0, 12.0)),
        ],
    );
    let plan = plan_recall(&eq, |_| None, &preset).unwrap();
    let reports: Vec<String> = plan.clamped.iter().map(ToString::to_string).collect();
    assert_eq!(
        reports,
        [
            "band 1 frequency 5 Hz clamped to 20.0 Hz",
            "band 2 gain 9 dB clamped to +6.0 dB",
            "band 2 Q 0.2 clamped to 0.5",
            "band 5 frequency 22000 Hz clamped to 19497 Hz",
            "band 5 gain -30 dB clamped to -20.0 dB",
            "band 5 Q 12 clamped to 8.0",
        ],
        "an EQ preset's strength is neither written nor reported"
    );
    assert!(matches!(
        plan.clamped[3],
        Clamp::Band { band: 4, param: EqParam::Frequency, asked, got }
            if asked == 22_000.0 && (got - eq.frequency(4, 127).unwrap()).abs() < 1e-9
    ));
    // The recall goes on at the nearest steps.
    for write in [
        frequency(0, 0),
        gain(1, 127),
        q(1, 0),
        frequency(4, 127),
        gain(4, 0),
        q(4, 127),
    ] {
        assert!(
            plan.writes.contains(&write),
            "{write:?} missing from {:?}",
            plan.writes
        );
    }
    assert!(
        !plan.writes.iter().any(|w| matches!(w, Write::Loudness(_))),
        "{:?}",
        plan.writes
    );

    let mut loud = preset.clone();
    loud.mode = EqMode::Loudness;
    let plan = plan_recall(&eq, |_| None, &loud).unwrap();
    assert_eq!(
        plan.clamped.last().map(ToString::to_string).as_deref(),
        Some("loudness 200 clamped to 127")
    );
    assert_eq!(plan.writes.first(), Some(&Write::Loudness(127)));
}

#[test]
fn a_clamp_names_the_edge_it_moved_to_precisely() {
    let report = |profile: &str, band_index: usize, hz: f64| {
        let eq = tone(profile);
        let mut values = builtins(&eq)[0].band[band_index];
        values.f = hz;
        let plan = plan_recall(
            &eq,
            |_| None,
            &preset(&eq, EqMode::Eq, 0, &[(band_index, values)]),
        )
        .unwrap();
        plan.clamped
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
    };
    assert_eq!(
        report("mesh-eq-5", 3, 1000.0),
        ["band 4 frequency 1000 Hz clamped to 1276 Hz"]
    );
    assert_eq!(
        report("mesh-eq-5", 2, 19_560.0),
        ["band 3 frequency 19560 Hz clamped to 1232 Hz"]
    );
    assert_eq!(
        report("mesh-eq-5", 4, 4000.0),
        ["band 5 frequency 4000 Hz clamped to 5075 Hz"]
    );
    assert_eq!(
        report("mesh-eq-3", 2, 25_000.0),
        ["band 3 frequency 25000 Hz clamped to 18854 Hz"]
    );
    assert_eq!(
        report("mesh-eq-3", 0, 10.0),
        ["band 1 frequency 10 Hz clamped to 20.7 Hz"]
    );
}

#[test]
fn a_frequency_rounded_at_a_window_edge_is_not_a_clamp() {
    let eq = tone("mesh-eq-5");
    // The lowest step of band 1 is a hair above 20 Hz, and the top of band 5 a hair
    // under 19.5 kHz; both round to a value just outside.
    let edges = [
        (0, eq.frequency(0, 0).unwrap().floor()),
        (4, eq.frequency(4, 127).unwrap().ceil()),
    ];
    for (band_index, hz) in edges {
        let mut changes = vec![];
        let mut values = builtins(&eq)[0].band[band_index];
        values.f = hz;
        changes.push((band_index, values));
        let plan = plan_recall(&eq, |_| None, &preset(&eq, EqMode::Eq, 0, &changes)).unwrap();
        assert_eq!(plan.clamped, [], "{hz} Hz on band {band_index}");
    }
}

#[test]
fn a_frequency_at_or_under_zero_is_clamped_to_the_lowest_step() {
    let eq = tone("mesh-eq-3");
    let plan = plan_recall(
        &eq,
        |_| None,
        &preset(&eq, EqMode::Eq, 0, &[(1, band(0.0, 0.0, 4.3))]),
    )
    .unwrap();
    assert!(plan.writes.contains(&frequency(1, 0)), "{:?}", plan.writes);
    assert_eq!(plan.clamped.len(), 1, "{:?}", plan.clamped);
}

#[test]
fn a_value_that_is_not_a_number_is_an_error() {
    let eq = tone("mesh-eq-5");
    for values in [
        band(f64::NAN, 0.0, 4.3),
        band(40.0, f64::INFINITY, 4.3),
        band(40.0, 0.0, f64::NEG_INFINITY),
    ] {
        let preset = preset(&eq, EqMode::Eq, 0, &[(0, values)]);
        let err = plan_recall(&eq, |_| None, &preset).unwrap_err();
        assert!(matches!(err, ProfileError::Preset { .. }), "{err:?}");
        assert!(err.to_string().contains("band 1"), "{err}");
        assert!(!matches(&eq, |_| None, &preset));
    }
}

#[test]
fn a_band_count_mismatch_is_an_error() {
    let five = tone("mesh-eq-5");
    let three_band = &builtins(&tone("mesh-eq-3"))[2];
    let err = plan_recall(&five, |_| None, three_band).unwrap_err();
    assert_eq!(
        err.to_string(),
        "preset \"Bass\": it is for 3 bands and this tone control has 5"
    );
    assert!(!matches(&five, |_| None, three_band));

    let mut short = builtins(&five).remove(0);
    short.band.pop();
    let err = plan_recall(&five, |_| None, &short).unwrap_err();
    assert_eq!(
        err.to_string(),
        "preset \"Flat\": it lists 4 bands under bands = 5"
    );
}

#[test]
fn a_curve_read_back_from_the_device_is_on() {
    for profile in ["mesh-eq-5", "mesh-eq-3"] {
        let eq = tone(profile);
        // Every step of every parameter, so the lowest band's sub-hertz steps are in.
        for step in 0..=127u8 {
            let raws = [step, 127 - step, step / 2 + 30];
            let device = Device::at(&eq, raws, EqMode::Eq, step);
            let preset = from_readings(&eq, device.read(), "mine").unwrap();
            assert_eq!(usize::from(preset.bands), eq.bands.len());
            assert!(
                matches(&eq, device.read(), &preset),
                "{profile} at {raws:?}: {preset:?}"
            );
            assert_eq!(
                plan_recall(&eq, device.read(), &preset).unwrap(),
                Plan::default(),
                "{profile} at {raws:?}"
            );
        }
    }
}

#[test]
fn a_curve_read_back_is_rounded() {
    let eq = tone("mesh-eq-5");
    let mut device = Device::at(&eq, [64, eq.gain_zero, 64], EqMode::Loudness, 33);
    device.set(&eq, 3, [80, eq.gain_raw(2.5), eq.q_raw(1.4)]);
    device.set(&eq, 4, [100, 60, 3]);
    let preset = from_readings(&eq, device.read(), "HD650 warm").unwrap();
    assert_eq!(preset.name, "HD650 warm");
    assert_eq!((preset.mode, preset.loudness), (EqMode::Loudness, 33));
    assert_eq!(preset.band[0], band(40.0, 0.0, 4.3));
    assert_eq!(preset.band[3].g, 2.5);
    assert_eq!(preset.band[3].q, 1.39);
    for values in &preset.band[1..] {
        // At these steps every band above the first moves more than 1 Hz a step, so
        // whole hertz keep them.
        assert_eq!(values.f.fract(), 0.0, "{values:?}");
        assert_eq!((values.g * 10.0).round() / 10.0, values.g, "{values:?}");
        assert_eq!((values.q * 100.0).round() / 100.0, values.q, "{values:?}");
    }
}

#[test]
fn the_lowest_band_keeps_a_decimal_where_whole_hertz_would_change_the_step() {
    let eq = tone("mesh-eq-5");
    let device = Device::at(&eq, [10, eq.gain_zero, 64], EqMode::Eq, 0);
    let preset = from_readings(&eq, device.read(), "low").unwrap();
    let hz = preset.band[0].f;
    assert_eq!(eq.frequency_raw(0, hz), Some(10), "{hz}");
    assert_eq!(
        (hz * 10.0).round() / 10.0,
        hz,
        "one decimal is enough: {hz}"
    );
    assert_ne!(
        hz.fract(),
        0.0,
        "whole hertz would move band 1 off step 10: {hz}"
    );
}

#[test]
fn a_small_cut_is_read_back_as_itself_never_as_negative_zero() {
    // A tone control whose gain steps are 1/64 dB, so the step under 0 dB is -0.016 dB,
    // which rounds to -0.0 at the first decimal.
    let mut eq = tone("mesh-eq-3");
    eq.gain_range = (-1.0, 1.0);
    eq.gain_zero = 64;
    for step in 0..=127u8 {
        let device = Device::at(&eq, [64, step, 64], EqMode::Eq, 0);
        let preset = from_readings(&eq, device.read(), "fine").unwrap();
        for values in &preset.band {
            assert!(
                !(values.g == 0.0 && values.g.is_sign_negative()),
                "step {step} read back as -0.0"
            );
            assert_eq!(eq.gain_raw(values.g), step, "{}", values.g);
        }
    }
    let device = Device::at(&eq, [64, 63, 64], EqMode::Eq, 0);
    let preset = from_readings(&eq, device.read(), "fine").unwrap();
    assert_eq!(preset.band[0].g, -0.02);
}

#[test]
fn a_curve_is_not_read_back_until_every_reading_is_in() {
    let eq = tone("mesh-eq-3");
    let full = Device::at(&eq, [64, eq.gain_zero, 64], EqMode::Eq, 0);
    assert!(from_readings(&eq, full.read(), "x").is_some());

    let mut missing = Device::at(&eq, [64, eq.gain_zero, 64], EqMode::Eq, 0);
    missing.0.remove(&eq.band_id(2, EqParam::Q).unwrap());
    assert_eq!(from_readings(&eq, missing.read(), "x"), None);

    let mut no_mode = Device::at(&eq, [64, eq.gain_zero, 64], EqMode::Eq, 0);
    no_mode.0.insert(eq.mode_id(), 7);
    assert_eq!(from_readings(&eq, no_mode.read(), "x"), None);

    let mut past_the_top = Device::at(&eq, [64, eq.gain_zero, 64], EqMode::Eq, 0);
    past_the_top.0.insert(eq.loudness_id(), 128);
    assert_eq!(from_readings(&eq, past_the_top.read(), "x"), None);
}

#[test]
fn flatten_lowers_the_boosts_before_it_raises_the_cuts() {
    let eq = tone("mesh-eq-5");
    let mut device = Device::at(&eq, [64, eq.gain_zero, 64], EqMode::Flat, 5);
    device.set(&eq, 0, [64, eq.gain_raw(-4.0), 64]);
    device.set(&eq, 2, [64, eq.gain_raw(3.0), 64]);
    device.0.remove(&eq.band_id(3, EqParam::Gain).unwrap());
    device.set(&eq, 4, [64, eq.gain_raw(5.0), 64]);
    let plan = flatten(&eq, device.read());
    assert_eq!(
        plan.writes,
        [
            gain(2, eq.gain_zero),
            gain(3, eq.gain_zero),
            gain(4, eq.gain_zero),
            gain(0, eq.gain_zero),
        ]
    );
    assert_eq!(plan.clamped, []);
    device.apply(&eq, &plan);
    assert_eq!(flatten(&eq, device.read()), Plan::default());
}

#[test]
fn flatten_puts_every_band_back_on_its_centre_and_centre_q() {
    let eq = tone("mesh-eq-5");
    let mut device = Device::at(&eq, [70, eq.gain_zero, 20], EqMode::Loudness, 33);
    device.set(&eq, 1, [90, eq.gain_raw(3.0), 20]);
    device.set(&eq, 3, [40, eq.gain_raw(-2.0), 100]);
    let plan = flatten(&eq, device.read());
    assert_eq!(plan.clamped, []);
    assert!(
        plan.writes
            .iter()
            .all(|write| matches!(write, Write::Band { .. })),
        "{:?}",
        plan.writes
    );
    // Gains down first, frequency and Q in band order, gains up last.
    assert_eq!(plan.writes.first(), Some(&gain(1, eq.gain_zero)));
    assert_eq!(plan.writes.last(), Some(&gain(3, eq.gain_zero)));
    device.apply(&eq, &plan);
    let flat = &builtins(&eq)[0];
    assert!(matches(
        &eq,
        device.read(),
        &Preset {
            mode: EqMode::Loudness,
            loudness: 33,
            ..flat.clone()
        }
    ));
    for band in 0..5 {
        let now = |param| device.0[&eq.band_id(band, param).unwrap()];
        assert_eq!(now(EqParam::Frequency), 64, "band {band}");
        assert_eq!(now(EqParam::Gain), i64::from(eq.gain_zero), "band {band}");
        assert_eq!(now(EqParam::Q), 64, "band {band}");
    }
    assert_eq!(device.0[&eq.mode_id()], i64::from(EqMode::Loudness as u8));
    assert_eq!(device.0[&eq.loudness_id()], 33);
    assert_eq!(flatten(&eq, device.read()), Plan::default());
}

#[test]
fn the_strength_counts_only_for_a_preset_in_loudness_mode() {
    let eq = tone("mesh-eq-5");
    let centred = [64, eq.gain_zero, 64];
    for mode in [EqMode::Eq, EqMode::Flat] {
        let preset = preset(&eq, mode, 40, &[]);
        let mut device = Device::at(&eq, centred, mode, 90);
        assert!(matches(&eq, device.read(), &preset), "{mode:?}");
        device.0.remove(&eq.loudness_id());
        assert!(
            matches(&eq, device.read(), &preset),
            "{mode:?} with no strength reading"
        );
        // Nor does a recall touch the device's strength.
        let plan = plan_recall(&eq, device.read(), &preset).unwrap();
        assert_eq!(plan, Plan::default(), "{mode:?}");
    }
    let loudness = preset(&eq, EqMode::Loudness, 40, &[]);
    let other = Device::at(&eq, centred, EqMode::Loudness, 90);
    assert!(!matches(&eq, other.read(), &loudness));
    let same = Device::at(&eq, centred, EqMode::Loudness, 40);
    assert!(matches(&eq, same.read(), &loudness));
    let mut unread = same;
    unread.0.remove(&eq.loudness_id());
    assert!(!matches(&eq, unread.read(), &loudness));
}

#[test]
fn a_preset_is_off_in_another_mode_or_a_step_away() {
    let eq = tone("mesh-eq-3");
    let centred = [64, eq.gain_zero, 64];
    let preset = preset(&eq, EqMode::Eq, 0, &[]);
    assert!(matches(
        &eq,
        Device::at(&eq, centred, EqMode::Eq, 0).read(),
        &preset
    ));
    for mode in [EqMode::Flat, EqMode::Loudness] {
        let device = Device::at(&eq, centred, mode, 0);
        assert!(!matches(&eq, device.read(), &preset), "{mode:?}");
    }
    let mut unread = Device::at(&eq, centred, EqMode::Eq, 0);
    unread.0.remove(&eq.mode_id());
    assert!(!matches(&eq, unread.read(), &preset), "no mode reading");
    for param in [EqParam::Frequency, EqParam::Gain, EqParam::Q] {
        let mut device = Device::at(&eq, centred, EqMode::Eq, 0);
        let id = eq.band_id(1, param).unwrap();
        *device.0.get_mut(&id).unwrap() += 1;
        // A band at 0 dB shapes nothing, so only its gain can take the preset off.
        assert_eq!(
            matches(&eq, device.read(), &preset),
            param != EqParam::Gain,
            "{param:?} a step off"
        );
    }
}

#[test]
fn writes_name_their_functions() {
    let eq = tone("mesh-eq-5");
    assert_eq!(gain(4, 98).id(&eq), eq.band_id(4, EqParam::Gain));
    assert_eq!(gain(5, 98).id(&eq), None);
    assert_eq!(Write::Mode(EqMode::Eq).id(&eq), Some(eq.mode_id()));
    assert_eq!(Write::Mode(EqMode::Eq).raw(), 2);
    assert_eq!(Write::Loudness(9).id(&eq), Some(eq.loudness_id()));
    assert_eq!(Write::Loudness(9).raw(), 9);
}

/// A band write stands for the value its raw step converts to through the tone control's
/// own profile, and goes back to the same step; a mode or loudness write has no such value.
#[test]
fn a_band_write_stands_for_the_value_its_step_converts_to() {
    let eq = tone("mesh-eq-5");
    let band = |param, raw| Write::Band {
        band: 4,
        param,
        raw,
    };
    let hz = band(EqParam::Frequency, 70)
        .value(&eq)
        .expect("a frequency");
    assert_eq!(Some(hz), eq.frequency(4, 70));
    assert_eq!(eq.frequency_raw(4, hz), Some(70));
    let db = band(EqParam::Gain, 110).value(&eq).expect("a gain");
    assert_eq!((db, eq.gain_raw(db)), (eq.gain_db(110), 110));
    let q = band(EqParam::Q, 20).value(&eq).expect("a Q");
    assert_eq!((q, eq.q_raw(q)), (eq.q(20), 20));
    let missing = Write::Band {
        band: 5,
        param: EqParam::Frequency,
        raw: 64,
    };
    assert_eq!(missing.value(&eq), None, "a band the tone control lacks");
    assert_eq!(Write::Mode(EqMode::Eq).value(&eq), None);
    assert_eq!(Write::Loudness(9).value(&eq), None);
    let words: Vec<&str> = [EqParam::Frequency, EqParam::Gain, EqParam::Q]
        .into_iter()
        .map(EqParam::word)
        .collect();
    assert_eq!(words, ["f", "g", "q"]);
    for word in words {
        assert_eq!(
            word.parse::<EqParam>().map(EqParam::word).ok(),
            Some(word),
            "{word} does not read back as itself"
        );
    }
}

#[cfg(feature = "serde")]
#[test]
fn a_preset_block_parses_from_toml() {
    #[derive(serde::Deserialize)]
    struct File {
        preset: Vec<Preset>,
    }
    let text = r#"
            [[preset]]
            name = "HD650 warm"
            bands = 5
            mode = "eq"
            loudness = 0
            band = [
              { f = 40, g = 3.0, q = 0.7 },
              { f = 160, g = 1.5, q = 0.7 },
              { f = 632, g = 0.0, q = 4.3 },
              { f = 2515, g = 0.0, q = 4.3 },
              { f = 10000, g = 2.0, q = 0.7 },
            ]

            [[preset]]
            name = "Loud"
            bands = 3
            mode = "Loudness"
            loudness = 60
            band = [{ f = 63, g = 0, q = 4.3 }, { f = 630, g = 0, q = 4.3 }, { f = 6300, g = 0, q = 4.3 }]
        "#;
    let file: File = toml::from_str(text).unwrap();
    assert_eq!(file.preset[0].name, "HD650 warm");
    assert_eq!(file.preset[0].mode, EqMode::Eq);
    assert_eq!(file.preset[0].band[4], band(10_000.0, 2.0, 0.7));
    assert_eq!(file.preset[1].mode, EqMode::Loudness);
    assert_eq!(file.preset[1].loudness, 60);

    let bad = text.replace("\"Loudness\"", "\"boomy\"");
    let err = toml::from_str::<File>(&bad).err().unwrap().to_string();
    assert!(err.contains("boomy"), "{err}");

    let json = serde_json::to_value(&file.preset[0]).unwrap();
    assert_eq!(json["mode"], "eq");
    assert_eq!(json["band"][0]["f"], 40.0);
}

/// The five-band Flat curve on a device, then bands 1 and 5 moved to another Q and frequency.
fn flat_with_moved_bands(eq: &EqSpec) -> (Device, Preset) {
    let flat = builtins(eq).into_iter().next().unwrap();
    assert_eq!(flat.name, "Flat");
    let mut device = Device::at(eq, [64, eq.gain_zero, 64], EqMode::Eq, 0);
    device.apply(eq, &plan_recall(eq, device.read(), &flat).unwrap());
    let f = |band, param| device.0[&eq.band_id(band, param).unwrap()];
    let (f4, q0) = (f(4, EqParam::Frequency), f(0, EqParam::Q));
    let (f4, q0) = (
        u8::try_from(f4 - 5).unwrap(),
        u8::try_from(q0 - 20).unwrap(),
    );
    let g = |band| u8::try_from(device.0[&eq.band_id(band, EqParam::Gain).unwrap()]).unwrap();
    let (g0, g4) = (g(0), g(4));
    let f0 = u8::try_from(device.0[&eq.band_id(0, EqParam::Frequency).unwrap()]).unwrap();
    let q4 = u8::try_from(device.0[&eq.band_id(4, EqParam::Q).unwrap()]).unwrap();
    device.set(eq, 0, [f0, g0, q0]);
    device.set(eq, 4, [f4, g4, q4]);
    (device, flat)
}

#[test]
fn a_band_at_zero_db_matches_whatever_its_frequency_and_q() {
    let eq = tone("mesh-eq-5");
    let (device, flat) = flat_with_moved_bands(&eq);
    assert!(matches(&eq, device.read(), &flat));
    for other in builtins(&eq).iter().skip(1) {
        assert!(!matches(&eq, device.read(), other), "{}", other.name);
    }
    // Loading Flat there still writes the frequency and Q back to the centres.
    let plan = plan_recall(&eq, device.read(), &flat).unwrap();
    assert_eq!(plan.writes.len(), 2, "{plan:?}");
}

#[test]
fn a_band_with_gain_still_needs_its_frequency_and_q() {
    let eq = tone("mesh-eq-5");
    for builtin in builtins(&eq).iter().skip(1) {
        let mut device = Device::at(&eq, [64, eq.gain_zero, 64], EqMode::Eq, 0);
        device.apply(&eq, &plan_recall(&eq, device.read(), builtin).unwrap());
        assert!(matches(&eq, device.read(), builtin), "{}", builtin.name);
        let band = builtin
            .band
            .iter()
            .position(|b| b.g != 0.0)
            .expect("a shaped band");
        let id = eq.band_id(band, EqParam::Q).unwrap();
        *device.0.get_mut(&id).unwrap() += 1;
        assert!(!matches(&eq, device.read(), builtin), "{}", builtin.name);
    }
}

fn profile(name: &str) -> Profile {
    Registry::builtin().unwrap().get(name).unwrap().clone()
}

#[test]
fn eq_writes_are_the_plan_as_commands() {
    let profile = profile("gungnir2-v2");
    let eq = profile.eq.clone().unwrap();
    let device = Device::at(&eq, [64, 64, 64], EqMode::Flat, 0);
    let warm = builtins(&eq)
        .into_iter()
        .find(|p| p.name == "Warm")
        .unwrap();
    let recall = warm.eq_writes(&profile, device.read()).unwrap();
    assert_eq!(recall.plan, plan_recall(&eq, device.read(), &warm).unwrap());
    assert_eq!(recall.commands.len(), recall.plan.writes.len());
    assert!(matches!(
        recall.commands.last(),
        Some(EqCommand::Mode("eq"))
    ));
    assert!(matches!(
        recall.commands.first(),
        Some(EqCommand::Band { band: 1.., .. })
    ));
}

#[test]
fn a_profile_without_a_tone_control_has_no_eq_writes() {
    let with = profile("gungnir2-v2");
    let eq = with.eq.clone().unwrap();
    let preset = builtins(&eq).remove(0);
    let without = profile("fake-mono");
    assert!(without.eq.is_none());
    assert!(matches!(
        preset.eq_writes(&without, |_| None),
        Err(ProfileError::NoEq)
    ));
}

#[test]
fn fitting_lists_the_built_ins_then_the_users_of_the_same_band_count() {
    let eq = tone("gungnir2-v2");
    let mut own = builtins(&eq).remove(1);
    own.name = "mine".to_string();
    let mut other = own.clone();
    other.name = "three".to_string();
    other.bands = 3;
    let listed = fitting(&eq, &[own, other]);
    let names: Vec<(&str, bool)> = listed.iter().map(|(p, b)| (p.name.as_str(), *b)).collect();
    assert_eq!(
        names,
        [
            ("Flat", true),
            ("Warm", true),
            ("Bass", true),
            ("Presence", true),
            ("mine", false)
        ]
    );
}

/// An AutoEq-like correction: a bass shelf, peaks and notches, a treble shelf.
fn correction() -> Correction {
    use crate::tone::apo::{FilterKind, SourceFilter};
    let filter = |kind, f, g, q| SourceFilter { kind, f, g, q };
    Correction {
        preamp_db: -4.0,
        filters: vec![
            filter(FilterKind::LowShelf, 105.0, 5.5, 0.7),
            filter(FilterKind::Peaking, 200.0, -2.1, 0.9),
            filter(FilterKind::Peaking, 1500.0, -3.0, 2.0),
            filter(FilterKind::Peaking, 3200.0, 4.2, 2.5),
            filter(FilterKind::HighShelf, 10_000.0, -4.0, 0.7),
        ],
    }
}

/// An imported preset named `Imported`.
fn imported() -> Preset {
    Preset {
        name: "Imported".into(),
        bands: 0,
        mode: EqMode::Eq,
        loudness: 0,
        band: Vec::new(),
        source: Some(Imported {
            origin: Some("test".into()),
            correction: correction(),
        }),
    }
}

/// An imported preset fits tone controls of three and five bands, whatever its own band count,
/// and every tone control lists it after the built-ins.
#[test]
fn an_imported_preset_fits_three_and_five_bands() {
    let preset = imported();
    assert!(preset.is_imported());
    for name in ["mesh-eq-3", "mesh-eq-5", "gungnir2-v2"] {
        let eq = tone(name);
        let fitted = preset.for_device(&eq);
        assert_eq!(usize::from(fitted.bands), eq.bands.len());
        assert_eq!(
            fitted.band,
            fit_for(&eq, preset.source.as_ref().unwrap()).bands
        );
        assert_eq!(
            (fitted.mode, fitted.loudness, fitted.is_imported()),
            (EqMode::Eq, 0, false)
        );
        assert_eq!(fitted.name, "Imported");
        let listed = fitting(&eq, std::slice::from_ref(&preset));
        assert_eq!(
            listed
                .last()
                .map(|(p, built_in)| (p.name.as_str(), *built_in)),
            Some(("Imported", false))
        );
    }
    let own = builtins(&tone("mesh-eq-5")).remove(1);
    assert!(matches!(own_for_device(&own), Cow::Borrowed(_)));
}

fn own_for_device(preset: &Preset) -> Cow<'_, Preset> {
    preset.for_device(&tone("mesh-eq-5"))
}

/// A recall writes the fit's bands without a clamp, and the device then matches the preset.
#[test]
fn recalling_an_imported_preset_writes_its_fit_with_no_clamp_and_is_then_on() {
    let preset = imported();
    for name in ["mesh-eq-3", "mesh-eq-5"] {
        let eq = tone(name);
        let mut device = Device::at(&eq, [64, 98, 64], EqMode::Flat, 0);
        assert!(!matches(&eq, device.read(), &preset));
        let plan = plan_recall(&eq, device.read(), &preset).unwrap();
        assert!(plan.clamped.is_empty(), "{:?}", plan.clamped);
        assert_eq!(plan.writes.last(), Some(&Write::Mode(EqMode::Eq)));
        device.apply(&eq, &plan);
        assert!(matches(&eq, device.read(), &preset));
        let again = plan_recall(&eq, device.read(), &preset).unwrap();
        assert!(again.writes.is_empty(), "{:?}", again.writes);
    }
}

/// The same correction and layout share one cached fit; another layout has its own.
#[test]
fn the_fit_cache_returns_the_same_arc_for_the_same_key() {
    let preset = imported();
    let source = preset.source.as_ref().unwrap();
    let (five, three) = (tone("mesh-eq-5"), tone("mesh-eq-3"));
    let first = fit_for(&five, source);
    assert!(Arc::ptr_eq(&first, &fit_for(&five, source)));
    assert!(Arc::ptr_eq(&first, &fit_for(&five, &source.clone())));
    let other = fit_for(&three, source);
    assert!(!Arc::ptr_eq(&first, &other));
    assert_eq!(other.bands.len(), 3);
    let mut changed = source.clone();
    changed.correction.preamp_db = -1.0;
    assert!(!Arc::ptr_eq(&first, &fit_for(&five, &changed)));
}

/// More than 64 distinct fits stay cached, so a list of imports never refits per frame.
#[test]
fn the_fit_cache_holds_more_than_sixty_four_fits() {
    let preset = imported();
    let source = preset.source.as_ref().unwrap();
    let five = tone("mesh-eq-5");
    let fits: Vec<(Imported, Arc<Fit>)> = (0..70)
        .map(|i| {
            let mut own = source.clone();
            own.correction.preamp_db = -10.0 - f64::from(i) * 0.01;
            let fit = fit_for(&five, &own);
            (own, fit)
        })
        .collect();
    for (own, fit) in &fits {
        assert!(Arc::ptr_eq(fit, &fit_for(&five, own)), "refitted");
    }
}

/// A device that has reported nothing, or only part of its curve, never panics a match or a
/// recall, and a recall writes every band.
#[test]
fn an_imported_preset_on_a_device_with_missing_readings_never_panics_and_writes_every_band() {
    let preset = imported();
    let eq = tone("mesh-eq-5");
    let nothing = Device::default();
    assert!(!matches(&eq, nothing.read(), &preset));
    let plan = plan_recall(&eq, nothing.read(), &preset).unwrap();
    let written = |param| {
        (0..5)
            .filter(|&band| {
                plan.writes
                    .iter()
                    .any(|w| matches!(w, Write::Band { band: b, param: p, .. } if *b == band && *p == param))
            })
            .count()
    };
    for param in [EqParam::Frequency, EqParam::Gain, EqParam::Q] {
        assert_eq!(written(param), 5, "{param:?}");
    }
    assert!(plan.writes.contains(&Write::Mode(EqMode::Eq)));

    let mut partial = Device::default();
    partial.set(&eq, 2, [64, 98, 64]);
    assert!(!matches(&eq, partial.read(), &preset));
    assert!(plan_recall(&eq, partial.read(), &preset).is_ok());
}

/// A tone control with no bands has nothing to fit an imported preset to.
#[test]
fn an_imported_preset_does_not_fit_a_tone_control_without_bands() {
    let mut eq = tone("mesh-eq-5");
    eq.bands.clear();
    let preset = imported();
    assert!(matches!(preset.for_device(&eq), Cow::Borrowed(_)));
    assert!(plan_recall(&eq, Device::default().read(), &preset).is_err());
    assert!(!matches(&eq, Device::default().read(), &preset));
}

/// A file import's name is the file name without its extension and its ` ParametricEQ`, cut at
/// 40 characters and trimmed.
#[test]
fn a_default_name_drops_parametriceq_and_is_cut_to_the_limit() {
    assert_eq!(
        default_name("Sennheiser HD 650 ParametricEQ"),
        "Sennheiser HD 650"
    );
    assert_eq!(default_name("HD 650 parametriceq"), "HD 650");
    assert_eq!(default_name("  HD 650  "), "HD 650");
    assert_eq!(default_name("ParametricEQ"), "ParametricEQ");
    assert_eq!(default_name("Ünï ParametricEQ"), "Ünï");
    let long = format!("{} tail ParametricEQ", "a".repeat(38));
    let name = default_name(&long);
    assert_eq!(name, format!("{} t", "a".repeat(38)));
    assert!(name.chars().count() <= NAME_MAX);
    let spaced = format!("{} {}", "b".repeat(39), "c".repeat(10));
    assert_eq!(default_name(&spaced), "b".repeat(39));
}

/// A clamp names the value it moved to the way the tone screen's band table reads it: a gain to
/// the hundredth a device step lands on, a frequency to a tenth of a hertz below 100 Hz.
#[test]
fn a_clamp_reads_a_gain_and_a_frequency_as_the_tone_screen_does() {
    let clamp = |param, asked, got| Clamp::Band {
        band: 0,
        param,
        asked,
        got,
    };
    assert_eq!(
        clamp(EqParam::Gain, 9.0, 1.45).to_string(),
        "band 1 gain 9 dB clamped to +1.45 dB"
    );
    assert_eq!(
        clamp(EqParam::Gain, 9.0, 6.0).to_string(),
        "band 1 gain 9 dB clamped to +6.0 dB"
    );
    assert_eq!(
        clamp(EqParam::Frequency, 5.0, 40.37).to_string(),
        "band 1 frequency 5 Hz clamped to 40.4 Hz"
    );
    assert_eq!(
        clamp(EqParam::Frequency, 5.0, 160.4).to_string(),
        "band 1 frequency 5 Hz clamped to 160 Hz"
    );
}

// ---- which preset is on, a new name, a recall by name ----

/// A five-band device reading Flat's curve: every band on its centre, at 0 dB and the centre Q,
/// in EQ mode.
fn flat_device(eq: &EqSpec) -> Device {
    Device::at(eq, [64, eq.gain_zero, 64], EqMode::Eq, 0)
}

/// A user's preset with Flat's curve, under `name`.
fn twin(eq: &EqSpec, name: &str) -> Preset {
    let mut preset = builtins(eq).into_iter().next().expect("Flat");
    preset.name = name.to_string();
    preset
}

/// The name of the preset `on` found, or how it found none.
fn named(on: On<'_>) -> String {
    match on {
        On::Preset(preset) => preset.name.clone(),
        other => format!("{other:?}"),
    }
}

#[test]
fn nothing_is_on_until_the_mode_and_every_band_have_read_a_step() {
    let eq = tone("mesh-eq-5");
    let fitting = builtins(&eq);
    let mut device = flat_device(&eq);
    assert_eq!(named(on(&eq, device.read(), &fitting, None)), "Flat");
    device.0.remove(&eq.band_id(4, EqParam::Q).unwrap());
    assert_eq!(on(&eq, device.read(), &fitting, None), On::Unread);
    let mut device = flat_device(&eq);
    device.0.insert(eq.mode_id(), 300);
    assert_eq!(
        on(&eq, device.read(), &fitting, None),
        On::Unread,
        "not a step"
    );
}

#[test]
fn the_preset_last_recalled_wins_while_it_still_matches_else_the_first_that_does() {
    let eq = tone("mesh-eq-5");
    let mut fitting = builtins(&eq);
    fitting.push(twin(&eq, "Zero"));
    let device = flat_device(&eq);
    assert_eq!(named(on(&eq, device.read(), &fitting, None)), "Flat");
    assert_eq!(
        named(on(&eq, device.read(), &fitting, Some(" zero "))),
        "Zero"
    );
    assert_eq!(
        named(on(&eq, device.read(), &fitting, Some("Warm"))),
        "Flat",
        "Warm is off"
    );
    assert_eq!(
        named(on(&eq, device.read(), &fitting, Some("gone"))),
        "Flat",
        "not listed"
    );
}

#[test]
fn a_curve_no_preset_matches_is_custom_and_a_preset_named_custom_never_stands_for_itself() {
    let eq = tone("mesh-eq-5");
    let mut device = flat_device(&eq);
    device.set(&eq, 2, [64, eq.gain_raw(3.0), 64]);
    assert_eq!(on(&eq, device.read(), &builtins(&eq), None), On::Custom);
    let device = flat_device(&eq);
    let named_custom = vec![twin(&eq, "Custom")];
    assert_eq!(
        on(&eq, device.read(), &named_custom, Some("custom")),
        On::Custom
    );
}

/// A legacy preset named `custom` is skipped by `on`, whatever it matches: the next preset that
/// matches is the one the device is on.
#[test]
fn on_skips_a_legacy_preset_named_custom_for_the_next_one_that_matches() {
    let eq = tone("mesh-eq-5");
    let device = flat_device(&eq);
    let fitting = vec![twin(&eq, "custom"), twin(&eq, "Mine")];
    match on(&eq, device.read(), &fitting, None) {
        On::Preset(preset) => assert_eq!(preset.name, "Mine"),
        other => panic!("{other:?}"),
    }
}

/// A preset already named `custom` may keep that name, in any spelling, and no other preset may
/// take it.
#[test]
fn a_legacy_custom_keeps_its_name_but_no_other_preset_takes_it() {
    assert_eq!(new_name("Custom", ["custom"], Some("custom")), Ok("Custom"));
    assert_eq!(
        new_name("custom", ["custom", "Mine"], Some("Mine")),
        Err(NameRefusal::Reserved("custom".into()))
    );
}

#[test]
fn a_new_name_is_checked_then_refused_as_a_built_in_custom_or_taken() {
    let existing = ["HD650 warm", "Mine"];
    assert_eq!(new_name("  Fresh ", existing, None), Ok("Fresh"));
    assert_eq!(new_name("custom 3", existing, None), Ok("custom 3"));
    assert_eq!(
        new_name("  ", existing, None),
        Err(NameRefusal::Name(NameError::Blank))
    );
    assert_eq!(
        new_name("bass", existing, None),
        Err(NameRefusal::BuiltIn("bass".into()))
    );
    assert_eq!(
        new_name("CUSTOM", existing, None),
        Err(NameRefusal::Reserved("CUSTOM".into()))
    );
    assert_eq!(
        new_name("hd650 WARM", existing, None),
        Err(NameRefusal::Taken("HD650 warm".into())),
        "named as the other preset spells it"
    );
    assert_eq!(
        new_name("MINE", existing, Some("mine")),
        Ok("MINE"),
        "its own name"
    );
    assert_eq!(
        new_name("Mine", existing, Some("HD650 warm")),
        Err(NameRefusal::Taken("Mine".into()))
    );
}

#[test]
fn each_name_refusal_has_one_wording() {
    assert_eq!(
        NameRefusal::BuiltIn("Flat".into()).to_string(),
        "\"Flat\" is a built-in preset's name"
    );
    assert_eq!(
        NameRefusal::Taken("Mine".into()).to_string(),
        "a preset named \"Mine\" already exists"
    );
    assert_eq!(
        NameRefusal::Reserved("custom".into()).to_string(),
        "\"custom\" stands for a curve no preset matches, so no preset can take it"
    );
    assert_eq!(
        NameRefusal::Name(NameError::Blank).to_string(),
        "a preset needs a name"
    );
}

#[test]
fn a_preset_is_recalled_by_its_name_in_any_case_and_an_unknown_one_lists_the_others() {
    let profile = Registry::builtin()
        .unwrap()
        .get("gungnir2-v2")
        .unwrap()
        .clone();
    let eq = profile.eq.clone().unwrap();
    let listed: Vec<Preset> = fitting(&eq, &[])
        .into_iter()
        .map(|(preset, _)| preset)
        .collect();
    let (found, recall) = recall_named(&profile, &listed, "  wARM ", |_| None).expect("a plan");
    assert_eq!(found.name, "Warm");
    assert_eq!(recall, found.eq_writes(&profile, |_| None).unwrap());
    assert_eq!(
        find(&listed, "PRESENCE").map(|p| p.name.as_str()),
        Some("Presence")
    );
    match recall_named(&profile, &listed, "Nope", |_| None) {
        Err(RecallError::NotFound {
            name,
            fitting: names,
        }) => {
            assert_eq!(name, "Nope");
            assert!(names.contains(&"Warm".to_string()), "{names:?}");
        }
        other => panic!("{other:?}"),
    }
}
