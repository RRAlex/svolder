// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::discovery::configs;
use crate::test_support::{device, profile};
use svolder_profiles::tone::preset::builtins;
use svolder_proto::{Message, Value};

fn names(profile: &Profile) -> Vec<String> {
    let eq = profile.eq.as_ref().expect("a tone control");
    preset::fitting(eq, &[])
        .into_iter()
        .map(|(p, _)| p.name)
        .collect()
}

fn entries_of(profile: &Profile, commands: bool) -> Vec<DiscoveryEntry> {
    configs(
        "svolder",
        "homeassistant",
        profile,
        &device("0102030405060708"),
        commands,
        &names(profile),
    )
}

fn entry<'a>(entries: &'a [DiscoveryEntry], key: &str) -> &'a DiscoveryEntry {
    entries
        .iter()
        .find(|e| e.topic.contains(&format!("/{key}/config")))
        .unwrap_or_else(|| panic!("no entry for {key}"))
}

#[test]
fn a_five_band_profile_gets_two_plus_fifteen_more_entities() {
    let p = profile("gungnir2-v2");
    // The link, the eleven controls with the mode, the preset, the loudness and 5 x 3 bands,
    // then the signal and the two firmware sensors, and with commands the identify button.
    assert_eq!(entries_of(&p, true).len(), 33);
    assert_eq!(entries_of(&p, false).len(), 32);
    assert_eq!(exposed_keys(&p).len(), 17);
}

#[test]
fn a_three_band_profile_gets_two_plus_nine() {
    let p = profile("mimir");
    let without = p.eq.as_ref().map(|eq| eq.bands.len());
    assert_eq!(without, Some(3));
    let entries = entries_of(&p, true);
    let tone: Vec<&str> = entries
        .iter()
        .filter(|e| e.topic.contains("/eq_"))
        .map(|e| e.topic.as_str())
        .collect();
    // The mode, the preset, the loudness and 3 x 3 bands.
    assert_eq!(tone.len(), 12, "{tone:?}");
    assert!(entries.iter().any(|e| e.topic.contains("/eq_band_3_q/")));
    assert!(!entries.iter().any(|e| e.topic.contains("/eq_band_4_")));
}

#[test]
fn a_profile_without_a_tone_control_gets_none_of_them() {
    let p = profile("fake-amp");
    assert!(p.eq.is_none());
    assert!(exposed_keys(&p).is_empty());
    let entries = configs(
        "svolder",
        "homeassistant",
        &p,
        &device("0102030405060708"),
        true,
        &[],
    );
    assert!(!entries.iter().any(|e| e.topic.contains("/eq_")));
}

#[test]
fn the_preset_select_lists_the_fitting_presets_then_custom() {
    let p = profile("gungnir2-v2");
    let mut offered = names(&p);
    offered.push("Evening".to_string());
    let entries = configs(
        "svolder",
        "homeassistant",
        &p,
        &device("0102030405060708"),
        true,
        &offered,
    );
    let select = entry(&entries, "eq_preset");
    assert_eq!(
        select.topic,
        "homeassistant/select/svolder_0102030405060708/eq_preset/config"
    );
    assert_eq!(select.payload["name"], "EQ preset");
    assert_eq!(
        select.payload["command_topic"],
        "svolder/0102030405060708/eq_preset/set"
    );
    let options: Vec<&str> = select.payload["options"]
        .as_array()
        .expect("options")
        .iter()
        .map(|o| o.as_str().expect("a name"))
        .collect();
    let mut want: Vec<&str> = offered.iter().map(String::as_str).collect();
    want.push("custom");
    assert_eq!(options, want);

    let read_only = configs(
        "svolder",
        "homeassistant",
        &p,
        &device("0102030405060708"),
        false,
        &offered,
    );
    let sensor = entry(&read_only, "eq_preset");
    assert!(sensor.topic.contains("/sensor/"));
    assert!(sensor.payload.get("options").is_none());
    assert!(sensor.payload.get("command_topic").is_none());
}

#[test]
fn a_preset_named_custom_is_not_an_option_of_its_own() {
    let offered = vec!["Warm".to_string(), " Custom ".to_string()];
    assert_eq!(preset_options(&offered), ["Warm", "custom"]);
}

#[test]
fn the_loudness_is_a_slider_from_0_to_127() {
    let p = profile("gungnir2-v2");
    let entries = entries_of(&p, true);
    let loudness = entry(&entries, "eq_loudness");
    assert!(loudness.topic.contains("/number/"));
    let body = &loudness.payload;
    assert_eq!(body["name"], "Loudness strength");
    assert_eq!(body["min"], 0.0);
    assert_eq!(body["max"], 127.0);
    assert_eq!(body["step"], 1.0);
    assert_eq!(body["mode"], "slider");
    assert!(body.get("unit_of_measurement").is_none());

    let read_only = entries_of(&p, false);
    let sensor = entry(&read_only, "eq_loudness");
    assert!(sensor.topic.contains("/sensor/"));
    for absent in ["min", "max", "step", "mode", "command_topic"] {
        assert!(sensor.payload.get(absent).is_none(), "{absent}");
    }
}

#[test]
fn every_band_has_a_gain_a_frequency_and_a_q_with_bounds_from_its_own_window() {
    for (name, bands) in [("gungnir2-v2", 5usize), ("mimir", 3)] {
        let p = profile(name);
        let eq = p.eq.as_ref().expect("a tone control");
        let entries = entries_of(&p, true);
        for band in 0..bands {
            let n = band + 1;
            let gain = &entry(&entries, &format!("eq_band_{n}_gain")).payload;
            assert_eq!(gain["name"], format!("Band {n} gain"));
            assert_eq!(gain["unit_of_measurement"], "dB");
            assert_eq!(gain["mode"], "slider");
            assert_eq!(gain["min"], eq.gain_range.0);
            assert_eq!(gain["max"], eq.gain_range.1);
            let step = gain["step"].as_f64().expect("a step");
            // The finest step between two raw gains: below 1 dB on both sides of zero.
            assert!(step > 0.0 && step < 0.5, "{step}");

            let frequency = &entry(&entries, &format!("eq_band_{n}_frequency")).payload;
            assert_eq!(frequency["name"], format!("Band {n} frequency"));
            assert_eq!(frequency["unit_of_measurement"], "Hz");
            assert_eq!(frequency["mode"], "box");
            assert_eq!(frequency["step"], 1.0);
            let low = frequency["min"].as_f64().expect("a minimum");
            let high = frequency["max"].as_f64().expect("a maximum");
            assert!(low <= eq.frequency(band, 0).expect("a band"));
            assert!(high >= eq.frequency(band, 127).expect("a band"));
            assert!(low < high);

            let q = &entry(&entries, &format!("eq_band_{n}_q")).payload;
            assert_eq!(q["name"], format!("Band {n} Q"));
            assert_eq!(q["mode"], "box");
            assert_eq!(q["step"], 0.01);
            assert_eq!(q["min"], eq.q_range.0);
            assert_eq!(q["max"], eq.q_range.1);
            assert!(q.get("unit_of_measurement").is_none());
        }
    }
}

#[test]
fn the_band_entities_are_sensors_with_only_a_unit_read_only() {
    let p = profile("gungnir2-v2");
    let entries = entries_of(&p, false);
    for (param, unit) in [("gain", Some("dB")), ("frequency", Some("Hz")), ("q", None)] {
        let e = entry(&entries, &format!("eq_band_1_{param}"));
        assert!(e.topic.contains("/sensor/"), "{param}");
        for absent in ["min", "max", "step", "mode", "command_topic"] {
            assert!(e.payload.get(absent).is_none(), "{param} {absent}");
        }
        assert_eq!(
            e.payload.get("unit_of_measurement").and_then(Json::as_str),
            unit
        );
    }
}

#[test]
fn every_tone_entity_carries_the_availability_device_and_origin_blocks() {
    let p = profile("gungnir2-v2");
    let entries = entries_of(&p, true);
    let tone: Vec<&DiscoveryEntry> = entries
        .iter()
        .filter(|e| e.topic.contains("/eq_") && !e.topic.contains("/eq_mode/"))
        .collect();
    assert_eq!(tone.len(), 17);
    let volume = &entry(&entries, "volume").payload;
    for e in tone {
        let key = e
            .topic
            .rsplit('/')
            .nth(1)
            .expect("a key in the topic")
            .to_string();
        assert_eq!(
            e.payload["unique_id"],
            format!("svolder_0102030405060708_{key}")
        );
        for shared in ["availability", "availability_mode", "device", "origin"] {
            assert_eq!(e.payload[shared], volume[shared], "{key} {shared}");
        }
        assert_eq!(e.payload["state_topic"], "svolder/0102030405060708/state");
        assert_eq!(
            e.payload["value_template"],
            format!("{{{{ value_json.{key} }}}}")
        );
    }
}

fn reading(p: &Profile, band: usize, param: EqParam, raw: u8) -> Message {
    let id =
        p.eq.as_ref()
            .and_then(|eq| eq.band_id(band, param))
            .expect("a band");
    Message::status(id, Value::U8(raw))
}

#[test]
fn the_state_carries_the_bands_and_loudness_in_units_rounded_to_three_decimals() {
    let p = profile("gungnir2-v2");
    let eq = p.eq.as_ref().expect("a tone control");
    let readings = [
        reading(&p, 1, EqParam::Gain, 127),
        reading(&p, 1, EqParam::Frequency, 64),
        reading(&p, 1, EqParam::Q, 100),
        Message::status(eq.loudness_id(), Value::U8(42)),
    ];
    let state = crate::payload::state(&p, &readings);
    assert_eq!(state["eq_band_2_gain"], eq.gain_range.1);
    assert_eq!(state["eq_band_2_frequency"], round3(eq.bands[1].center));
    assert_eq!(state["eq_band_2_q"], round3(eq.q(100)));
    assert_eq!(state["eq_loudness"], 42);
    // A band parameter that has not been read is left out.
    for absent in ["eq_band_1_gain", "eq_band_2_frequency_x", "eq_band_3_q"] {
        assert!(state.get(absent).is_none(), "{absent}");
    }
    // The preset is the bridge's to say.
    assert!(state.get("eq_preset").is_none());
}

#[test]
fn a_reading_that_is_no_step_is_left_out() {
    let p = profile("gungnir2-v2");
    let eq = p.eq.as_ref().expect("a tone control");
    let readings = [
        reading(&p, 0, EqParam::Gain, 200),
        Message::status(eq.loudness_id(), Value::Bool(true)),
    ];
    let state = crate::payload::state(&p, &readings);
    assert!(state.get("eq_band_1_gain").is_none());
    assert!(state.get("eq_loudness").is_none());
}

/// Every reading of a tone control sitting on `preset`'s own steps.
fn readings_of(p: &Profile, preset: &Preset) -> Vec<Message> {
    let eq = p.eq.as_ref().expect("a tone control");
    let mut readings = vec![
        Message::status(eq.mode_id(), Value::U8(preset.mode as u8)),
        Message::status(eq.loudness_id(), Value::U8(preset.loudness)),
    ];
    for (band, values) in preset.band.iter().enumerate() {
        readings.push(reading(
            p,
            band,
            EqParam::Frequency,
            eq.frequency_raw(band, values.f).expect("a step"),
        ));
        readings.push(reading(p, band, EqParam::Gain, eq.gain_raw(values.g)));
        readings.push(reading(p, band, EqParam::Q, eq.q_raw(values.q)));
    }
    readings
}

fn fitting_of(p: &Profile, user: &[Preset]) -> Vec<Preset> {
    let eq = p.eq.as_ref().expect("a tone control");
    preset::fitting(eq, user)
        .into_iter()
        .map(|(p, _)| p)
        .collect()
}

#[test]
fn the_preset_state_is_left_out_until_the_tone_control_is_reported() {
    let p = profile("gungnir2-v2");
    let eq = p.eq.as_ref().expect("a tone control");
    let fitting = fitting_of(&p, &[]);
    assert_eq!(preset_state(&p, &[], &fitting, None), None);
    let mode_only = [Message::status(eq.mode_id(), Value::U8(1))];
    assert_eq!(preset_state(&p, &mode_only, &fitting, None), None);
    let mut all = readings_of(&p, &fitting[0]);
    all.pop();
    assert_eq!(preset_state(&p, &all, &fitting, None), None, "a Q missing");
}

#[test]
fn the_preset_state_is_the_first_match_else_custom() {
    let p = profile("gungnir2-v2");
    let eq = p.eq.as_ref().expect("a tone control");
    let fitting = fitting_of(&p, &[]);
    let warm = fitting.iter().find(|p| p.name == "Warm").expect("Warm");
    assert_eq!(
        preset_state(&p, &readings_of(&p, warm), &fitting, None).as_deref(),
        Some("Warm")
    );
    let mut off = readings_of(&p, warm);
    off.retain(|m| m.id != eq.band_id(0, EqParam::Gain).expect("a band"));
    off.push(reading(&p, 0, EqParam::Gain, 3));
    assert_eq!(
        preset_state(&p, &off, &fitting, None).as_deref(),
        Some("custom")
    );
}

/// A preset of the user's with the same curve as another, listed after it.
fn twin_of(preset: &Preset, name: &str) -> Preset {
    Preset {
        name: name.to_string(),
        ..preset.clone()
    }
}

#[test]
fn the_preset_last_recalled_wins_while_it_still_matches() {
    let p = profile("gungnir2-v2");
    let fitting = fitting_of(&p, &[]);
    let warm = fitting.iter().find(|p| p.name == "Warm").expect("Warm");
    let user = [twin_of(warm, "Warm copy")];
    let fitting = fitting_of(&p, &user);
    let readings = readings_of(&p, warm);
    assert_eq!(
        preset_state(&p, &readings, &fitting, None).as_deref(),
        Some("Warm"),
        "the first of the two"
    );
    assert_eq!(
        preset_state(&p, &readings, &fitting, Some(" warm COPY ")).as_deref(),
        Some("Warm copy"),
        "the one the bridge recalled"
    );
    let bass = fitting.iter().find(|p| p.name == "Bass").expect("Bass");
    assert_eq!(
        preset_state(&p, &readings_of(&p, bass), &fitting, Some("Warm copy")).as_deref(),
        Some("Bass"),
        "a recalled preset that no longer matches yields the first match"
    );
    assert_eq!(
        preset_state(&p, &readings, &fitting, Some("Deleted")).as_deref(),
        Some("Warm"),
        "a recalled preset no longer listed"
    );
}

#[test]
fn a_preset_named_custom_never_stands_for_the_state() {
    let p = profile("gungnir2-v2");
    let fitting = fitting_of(&p, &[]);
    let warm = fitting.iter().find(|p| p.name == "Warm").expect("Warm");
    let user = [twin_of(warm, "Custom")];
    let readings = readings_of(
        &p,
        &Preset {
            name: "x".into(),
            ..warm.clone()
        },
    );
    // Only a curve that a listed preset other than "custom" has is named.
    let with = fitting_of(&p, &user);
    assert_eq!(
        preset_state(&p, &readings, &with, Some("custom")).as_deref(),
        Some("Warm")
    );
}

#[test]
fn a_band_command_is_any_number_and_keeps_the_value_as_given() {
    let p = profile("gungnir2-v2");
    let band = Target::Band {
        band: 1,
        param: EqParam::Gain,
    };
    assert_eq!(
        parse(&p, band, " -3.5 ").expect("a gain"),
        Command::Band {
            band: 2,
            param: "g",
            value: -3.5
        }
    );
    let frequency = Target::Band {
        band: 4,
        param: EqParam::Frequency,
    };
    assert_eq!(
        parse(&p, frequency, "9000").expect("a frequency"),
        Command::Band {
            band: 5,
            param: "f",
            value: 9000.0
        }
    );
    let q = Target::Band {
        band: 0,
        param: EqParam::Q,
    };
    assert_eq!(
        parse(&p, q, "1.5").expect("a Q"),
        Command::Band {
            band: 1,
            param: "q",
            value: 1.5
        }
    );
}

/// What the device's actor does with a parsed band command: `svolder eq set`'s own build.
fn sent(p: &Profile, command: &Command) -> Value {
    match command {
        Command::Band { band, param, value } => {
            let param: EqParam = param.parse().expect("a parameter word");
            svolder_profiles::eq_set(p, band - 1, param, *value)
                .expect("the actor accepts it")
                .value
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_band_value_snaps_to_the_nearest_step_and_one_past_the_window_clamps_as_the_cli_does() {
    let p = profile("gungnir2-v2");
    let eq = p.eq.as_ref().expect("a tone control");
    let gain = Target::Band {
        band: 0,
        param: EqParam::Gain,
    };
    let near = parse(&p, gain, "2.0").expect("a gain");
    assert_eq!(sent(&p, &near), Value::U8(eq.gain_raw(2.0)));
    let loud = parse(&p, gain, "40").expect("a gain past the range clamps");
    assert_eq!(sent(&p, &loud), Value::U8(127));
    let quiet = parse(&p, gain, "-99").expect("a gain past the range clamps");
    assert_eq!(sent(&p, &quiet), Value::U8(0));
    let frequency = Target::Band {
        band: 0,
        param: EqParam::Frequency,
    };
    let high = parse(&p, frequency, "100000").expect("a frequency past the window clamps");
    assert_eq!(sent(&p, &high), Value::U8(127));
    let low = parse(&p, frequency, "0.5").expect("a frequency past the window clamps");
    assert_eq!(sent(&p, &low), Value::U8(0));
}

#[test]
fn a_band_command_that_is_no_finite_number_is_refused() {
    let p = profile("gungnir2-v2");
    for target in [
        Target::Band {
            band: 0,
            param: EqParam::Gain,
        },
        Target::Band {
            band: 0,
            param: EqParam::Frequency,
        },
        Target::Band {
            band: 0,
            param: EqParam::Q,
        },
    ] {
        for bad in ["loud", "", "1,5", "NaN", "inf", "-inf"] {
            let err = parse(&p, target, bad).expect_err(bad);
            assert!(matches!(err, ProfileError::BadInput { .. }), "{bad}: {err}");
        }
    }
    // A frequency that is not positive has no step either.
    let frequency = Target::Band {
        band: 0,
        param: EqParam::Frequency,
    };
    let why = parse(&p, frequency, "0").expect_err("zero Hz").to_string();
    assert!(
        why.contains("band 1 frequency") && why.contains("window is 20 to"),
        "{why}"
    );
    assert!(parse(&p, frequency, "-20").is_err());
}

#[test]
fn the_loudness_is_a_whole_number_from_0_to_127() {
    let p = profile("gungnir2-v2");
    for (text, want) in [
        ("0", 0u8),
        ("42", 42),
        (" 127 ", 127),
        ("42.4", 42),
        ("42.6", 43),
    ] {
        assert_eq!(
            parse(&p, Target::Loudness, text).expect(text),
            Command::Loudness(want)
        );
    }
    for bad in ["128", "-1", "loud", "", "NaN", "inf"] {
        assert!(parse(&p, Target::Loudness, bad).is_err(), "{bad}");
    }
}

#[test]
fn a_preset_command_is_its_name_without_the_blanks_round_it() {
    let p = profile("gungnir2-v2");
    assert_eq!(
        parse(&p, Target::Preset, "  Bass lift ").expect("a name"),
        Command::Preset("Bass lift".into())
    );
}

#[test]
fn a_profile_without_a_tone_control_refuses_every_tone_command() {
    let p = profile("fake-amp");
    assert!(matches!(
        parse(&p, Target::Loudness, "5"),
        Err(ProfileError::NoEq)
    ));
}

#[test]
fn a_key_names_its_target_without_regard_to_case_and_only_within_the_band_count() {
    let p = profile("mimir");
    assert_eq!(target(&p, "EQ_Preset"), Some(Target::Preset));
    assert_eq!(target(&p, "eq_loudness"), Some(Target::Loudness));
    assert_eq!(
        target(&p, "eq_band_3_q"),
        Some(Target::Band {
            band: 2,
            param: EqParam::Q
        })
    );
    for other in [
        "eq_band_4_q",
        "eq_band_0_q",
        "eq_band_03_q",
        "eq_band_1_x",
        "eq_mode",
        "volume",
    ] {
        assert_eq!(target(&p, other), None, "{other}");
    }
    assert_eq!(target(&profile("fake-amp"), "eq_preset"), None);
}

#[test]
fn the_builtins_the_select_offers_come_from_the_profile() {
    let p = profile("gungnir2-v2");
    let eq = p.eq.as_ref().expect("a tone control");
    let from_builtins: Vec<String> = builtins(eq).into_iter().map(|p| p.name).collect();
    assert_eq!(names(&p), from_builtins);
}
