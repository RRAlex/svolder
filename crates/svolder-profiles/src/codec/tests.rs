// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use std::collections::BTreeMap;

use super::*;
use crate::registry::Registry;

const DAC: &str = r#"
schema = 1
name = "dac"
product_id = 2

[[control]]
id = [2]
key = "input"
label = "Input"
kind = "select"
values = { 1 = "USB", 2 = "Coax", 3 = "Optical", 4 = "Coax 2" }

[[control]]
id = [5]
key = "mute"
label = "Mute"
kind = "toggle"

[[control]]
id = [10]
key = "volume"
label = "Volume"
kind = "level"
range = [0, 127]
unit = "dB"
convert = { table = "dac-attenuation-db" }

[[control]]
id = [11]
key = "balance"
label = "Balance"
kind = "level"
range = [0, 127]
center = 64

[[control]]
id = [6]
key = "sample_rate"
label = "Sample rate"
kind = "readout"
values = { 0 = "—", 2 = "48 kHz" }

[[control]]
id = [20, 1]
key = "meter_l"
label = "Meter L"
kind = "meter"
channel = "L"

[[control]]
id = [230, 1]
key = "fault_overcurrent"
label = "Over-current"
kind = "fault"
advice = "Power-cycle the device."

[eq]
id = 12
bands = [40, 160, 632, 2520, 10000]
fscale = [[0.005554, 0.00544], [0.00536, 0.005435], [0.005435, 0.005435], [0.005435, 0.005435], [0.005435, 0.005435]]
range = 1.95
gain = { range = [-20, 6], zero = 98 }
q = { range = [0.5, 8.0], center = 4.3 }
"#;

fn dac() -> Profile {
    Registry::from_sources(vec![("dac".into(), DAC.into())])
        .unwrap()
        .get("dac")
        .unwrap()
        .clone()
}

fn status(id: FunctionId, value: Value) -> Message {
    Message::status(id, value)
}

#[test]
fn describes_each_kind() {
    let p = dac();
    let r = describe(&p, &status(FunctionId::one(2), Value::U8(3)));
    assert_eq!(r.key.as_deref(), Some("input"));
    assert_eq!(r.label, "Input");
    assert_eq!(r.display, "3 (Optical)");
    assert_eq!(
        describe(&p, &status(FunctionId::one(2), Value::U8(9))).display,
        "9"
    );

    assert_eq!(
        describe(&p, &status(FunctionId::one(5), Value::Bool(true))).display,
        "on"
    );
    assert_eq!(
        describe(&p, &status(FunctionId::one(5), Value::Bool(false))).display,
        "off"
    );

    let v = describe(&p, &status(FunctionId::one(10), Value::U8(109)));
    assert_eq!(v.display, "109 (-6.0 dB)");
    assert_eq!(v.unit.as_deref(), Some("dB"));
    assert_eq!(
        describe(&p, &status(FunctionId::one(10), Value::U8(0))).display,
        "0 (silence)"
    );
    assert_eq!(
        describe(&p, &status(FunctionId::one(11), Value::U8(64))).display,
        "0"
    );

    assert_eq!(
        describe(&p, &status(FunctionId::one(6), Value::U8(2))).display,
        "2 (48 kHz)"
    );
    assert_eq!(
        describe(&p, &status(FunctionId::two(20, 1), Value::U16(1180))).display,
        "1180"
    );
    assert_eq!(
        describe(&p, &status(FunctionId::two(230, 1), Value::Bool(true))).display,
        "active"
    );
    assert_eq!(
        describe(&p, &status(FunctionId::two(230, 1), Value::Bool(false))).display,
        "clear"
    );
}

#[test]
fn describes_eq_functions() {
    let p = dac();
    let f = describe(&p, &status(FunctionId::three(12, 2, 2), Value::U8(64)));
    assert_eq!(f.label, "EQ band 2 frequency");
    assert_eq!(f.display, "160 Hz");
    assert_eq!(f.key, None);
    let g = describe(&p, &status(FunctionId::three(12, 1, 3), Value::U8(49)));
    assert_eq!(g.label, "EQ band 1 gain");
    assert_eq!(g.display, "-10.0 dB");
    let q = describe(&p, &status(FunctionId::three(12, 3, 1), Value::U8(64)));
    assert_eq!(q.label, "EQ band 3 Q");
    assert_eq!(q.display, "4.30");
    let m = describe(&p, &status(FunctionId::two(12, 50), Value::U8(2)));
    assert_eq!(m.label, "Tone control mode");
    assert_eq!(m.display, "eq");
    let l = describe(&p, &status(FunctionId::two(12, 52), Value::U8(40)));
    assert_eq!(l.label, "Loudness");
    assert_eq!(l.display, "40");
}

#[test]
fn each_eq_parameter_reads_on_its_own_sub_id() {
    let p = dac();
    let eq = p.eq.as_ref().expect("the dac has tone control");
    // Distinct raw values, so a band's parameters cannot be swapped unseen: 1 is Q,
    // 2 is frequency, 3 is gain.
    let q = describe(&p, &status(FunctionId::three(12, 2, 1), Value::U8(5)));
    assert_eq!(q.label, "EQ band 2 Q");
    assert_eq!(q.value, Some(eq.q(5)));
    let f = describe(&p, &status(FunctionId::three(12, 2, 2), Value::U8(30)));
    assert_eq!(f.label, "EQ band 2 frequency");
    assert_eq!(f.value, eq.frequency(1, 30));
    let g = describe(&p, &status(FunctionId::three(12, 2, 3), Value::U8(110)));
    assert_eq!(g.label, "EQ band 2 gain");
    assert_eq!(g.value, Some(eq.gain_db(110)));
    assert_eq!(q.display, "0.80");
    assert_eq!(f.display, "112 Hz");
    assert_eq!(g.display, "+2.48 dB");
    // Below 100 Hz a frequency reads to the tenth, which a whole-hertz format would round off.
    let low = describe(&p, &status(FunctionId::three(12, 1, 2), Value::U8(65)));
    assert_eq!(low.display, "40.4 Hz");
}

#[test]
fn converted_values_come_back_as_numbers() {
    let p = dac();
    assert_eq!(
        describe(&p, &status(FunctionId::one(10), Value::U8(109))).value,
        Some(-6.021)
    );
    assert_eq!(
        describe(&p, &status(FunctionId::one(11), Value::U8(64))).value,
        None,
        "no conversion"
    );
    assert_eq!(
        describe(&p, &status(FunctionId::three(12, 2, 2), Value::U8(64))).value,
        Some(160.0)
    );
    assert_eq!(
        describe(&p, &status(FunctionId::three(12, 1, 3), Value::U8(49))).value,
        Some(-10.0)
    );
    assert_eq!(
        describe(&p, &status(FunctionId::three(12, 3, 1), Value::U8(64))).value,
        Some(4.3)
    );
}

#[cfg(feature = "serde")]
#[test]
fn a_reading_round_trips_through_json() {
    let p = dac();
    let r = describe(&p, &status(FunctionId::three(12, 1, 3), Value::U8(49)));
    let text = serde_json::to_string(&r).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(parsed["id"], "12.1.3");
    assert_eq!(serde_json::from_str::<Reading>(&text).unwrap(), r);
}

#[test]
fn a_value_of_the_wrong_type_is_reported_raw() {
    let p = dac();
    let r = describe(&p, &status(FunctionId::two(230, 1), Value::U8(1)));
    assert_eq!(r.display, "1");
    assert_eq!(r.value, None);
    assert_eq!(r.key.as_deref(), Some("fault_overcurrent"));
    assert_eq!(r.label, "Over-current");
    assert_eq!(r.kind, Some(ControlKind::Fault));
}

#[test]
fn unknown_ids_are_raw() {
    let p = dac();
    let r = describe(&p, &status(FunctionId::two(7, 1), Value::U8(0)));
    assert_eq!(r.key, None);
    assert_eq!(r.label, "?");
    assert_eq!(r.kind, None);
    assert_eq!(r.display, "0");
    assert_eq!(r.id.to_string(), "7.1");
}

#[test]
fn configured_input_labels_replace_and_extend_the_profile() {
    let mut p = dac();
    let mut inputs = BTreeMap::new();
    inputs.insert(1, "TV".to_string());
    inputs.insert(9, "Turntable".to_string());
    apply_input_labels(&mut p, &inputs);
    let control = p.control_by_key("input").unwrap();
    assert_eq!(
        control.value_name(1),
        Some("TV"),
        "a listed value is relabelled"
    );
    assert_eq!(
        control.value_name(2),
        Some("Coax"),
        "the rest are left alone"
    );
    assert_eq!(
        control.value_name(9),
        Some("Turntable"),
        "an unlisted value is added"
    );
    assert!(
        control.values.windows(2).all(|w| w[0].0 < w[1].0),
        "values stay sorted: {:?}",
        control.values
    );
}

#[test]
fn input_labels_are_a_no_op_without_labels_or_an_input_control() {
    let mut p = dac();
    let before = p.clone();
    apply_input_labels(&mut p, &BTreeMap::new());
    assert_eq!(p, before, "an empty table changes nothing");

    let mut generic = Registry::generic(1);
    let mut inputs = BTreeMap::new();
    inputs.insert(1, "TV".to_string());
    apply_input_labels(&mut generic, &inputs);
    assert!(
        generic.controls.is_empty(),
        "a profile with no input control is left alone"
    );
}

#[test]
fn toggle_intents() {
    let p = dac();
    for on in ["on", "ON", "true", "1", "yes"] {
        assert_eq!(
            intent(&p, "mute", on).unwrap(),
            Message::command(FunctionId::one(5), Value::Bool(true)),
            "{on}"
        );
    }
    for off in ["off", "false", "0", "no"] {
        assert_eq!(
            intent(&p, "MUTE", off).unwrap(),
            Message::command(FunctionId::one(5), Value::Bool(false)),
            "{off}"
        );
    }
    assert!(matches!(
        intent(&p, "mute", "maybe"),
        Err(ProfileError::BadInput { .. })
    ));
}

#[test]
fn select_intents_accept_numbers_and_names() {
    let p = dac();
    let want = Message::command(FunctionId::one(2), Value::U8(3));
    assert_eq!(intent(&p, "input", "3").unwrap(), want);
    assert_eq!(intent(&p, "input", "optical").unwrap(), want);
    assert_eq!(intent(&p, "input", "OPT").unwrap(), want);
    assert_eq!(
        intent(&p, "input", "coax").unwrap(),
        Message::command(FunctionId::one(2), Value::U8(2)),
        "exact wins"
    );
    assert!(matches!(
        intent(&p, "input", "7"),
        Err(ProfileError::BadInput { .. })
    ));
    assert!(
        matches!(
            intent(&p, "input", "co"),
            Err(ProfileError::BadInput { .. })
        ),
        "ambiguous prefix"
    );
}

#[test]
fn level_intents_accept_raw_units_and_extremes() {
    let p = dac();
    let id = FunctionId::one(10);
    assert_eq!(
        intent(&p, "volume", "100").unwrap(),
        Message::command(id, Value::U8(100))
    );
    assert_eq!(
        intent(&p, "volume", "-6dB").unwrap(),
        Message::command(id, Value::U8(109))
    );
    assert_eq!(
        intent(&p, "volume", "-6 dB").unwrap(),
        Message::command(id, Value::U8(109))
    );
    assert_eq!(
        intent(&p, "volume", "max").unwrap(),
        Message::command(id, Value::U8(127))
    );
    assert_eq!(
        intent(&p, "volume", "min").unwrap(),
        Message::command(id, Value::U8(0))
    );
    assert!(matches!(
        intent(&p, "volume", "200"),
        Err(ProfileError::BadInput { .. })
    ));
    assert!(
        matches!(
            intent(&p, "balance", "-1dB"),
            Err(ProfileError::BadInput { .. })
        ),
        "no conversion"
    );
    assert_eq!(
        intent(&p, "balance", "64").unwrap(),
        Message::command(FunctionId::one(11), Value::U8(64))
    );
}

#[test]
fn read_only_and_unknown_keys() {
    let p = dac();
    assert!(matches!(
        intent(&p, "sample_rate", "2"),
        Err(ProfileError::NotWritable(_))
    ));
    assert!(matches!(
        intent(&p, "meter_l", "2"),
        Err(ProfileError::NotWritable(_))
    ));
    assert!(matches!(
        intent(&p, "nope", "2"),
        Err(ProfileError::UnknownKey { .. })
    ));
}

#[test]
fn eq_intents() {
    let p = dac();
    assert_eq!(
        eq_set(&p, 1, EqParam::Frequency, 160.0).unwrap(),
        Message::command(FunctionId::three(12, 2, 2), Value::U8(64))
    );
    assert_eq!(
        eq_set(&p, 0, EqParam::Gain, -10.0).unwrap(),
        Message::command(FunctionId::three(12, 1, 3), Value::U8(49))
    );
    assert_eq!(
        eq_set(&p, 4, EqParam::Q, 2.1).unwrap(),
        Message::command(FunctionId::three(12, 5, 1), Value::U8(27))
    );
    assert!(matches!(
        eq_set(&p, 5, EqParam::Q, 2.1),
        Err(ProfileError::BadBand(5))
    ));
    assert_eq!(
        eq_mode(&p, EqMode::Loudness).unwrap(),
        Message::command(FunctionId::two(12, 50), Value::U8(3))
    );
    assert_eq!(
        eq_loudness(&p, 40).unwrap(),
        Message::command(FunctionId::two(12, 52), Value::U8(40))
    );
    let no_eq = Registry::generic(1);
    assert!(matches!(
        eq_mode(&no_eq, EqMode::Flat),
        Err(ProfileError::NoEq)
    ));
}

#[test]
fn intent_rejects_non_finite_level_input() {
    let p = dac();
    assert!(matches!(
        intent(&p, "volume", "NaN dB"),
        Err(ProfileError::BadInput { .. })
    ));
}

#[test]
fn relative_steps_and_clamps_a_level() {
    let p = dac();
    let id = FunctionId::one(10);
    assert_eq!(
        relative(&p, "volume", 100, 5).unwrap(),
        Message::command(id, Value::U8(105))
    );
    assert_eq!(
        relative(&p, "volume", 100, -5).unwrap(),
        Message::command(id, Value::U8(95))
    );
    assert_eq!(
        relative(&p, "volume", 125, 10).unwrap(),
        Message::command(id, Value::U8(127)),
        "clamped at the top of the range"
    );
    assert_eq!(
        relative(&p, "volume", 2, -10).unwrap(),
        Message::command(id, Value::U8(0)),
        "clamped at the bottom of the range"
    );
    assert_eq!(
        relative(&p, "volume", 0, i64::MIN).unwrap(),
        Message::command(id, Value::U8(0)),
        "a step that would overflow saturates instead"
    );
}

#[test]
fn relative_refuses_what_it_cannot_step() {
    let p = dac();
    assert!(matches!(
        relative(&p, "mute", 0, 1),
        Err(ProfileError::NotSteppable(_)),
        // a toggle has no range to move within
    ));
    assert!(matches!(
        relative(&p, "nope", 0, 1),
        Err(ProfileError::UnknownKey { .. })
    ));
    assert!(matches!(
        relative(&p, "sample_rate", 0, 1),
        Err(ProfileError::NotSteppable(_))
    ));
}

#[test]
fn eq_set_rejects_non_finite_value() {
    let p = dac();
    assert!(matches!(
        eq_set(&p, 0, EqParam::Gain, f64::NAN),
        Err(ProfileError::BadInput { .. })
    ));
}

const AMP: &str = r#"
schema = 1
name = "amp"

[[control]]
id = [4]
key = "warming_up"
label = "Warming up"
kind = "readout"
value_type = "bool"
only_while = "power"

[[control]]
id = [10]
key = "power"
label = "Power"
kind = "toggle"

[[control]]
id = [21, 1]
key = "temperature_reporting"
label = "Temperature reporting"
kind = "readout"
value_type = "bool"
auto_off = true
"#;

fn amp() -> Profile {
    Registry::from_sources(vec![("amp".into(), AMP.into())])
        .unwrap()
        .get("amp")
        .unwrap()
        .clone()
}

#[test]
fn an_integer_on_a_bool_readout_reads_true_only_for_one() {
    let p = amp();
    let id = FunctionId::one(4);
    for (value, display) in [
        (Value::U8(0), "false"),
        (Value::U8(1), "true"),
        (Value::U8(2), "false"),
        (Value::Bool(true), "true"),
        (Value::Bool(false), "false"),
    ] {
        assert_eq!(
            describe(&p, &status(id, value.clone())).display,
            display,
            "{value:?}"
        );
    }
    let control = p.control_by_id(id).unwrap();
    assert!(control.reads_true(&Value::U8(1)));
    assert!(!control.reads_true(&Value::U8(0)));
    assert!(!control.reads_true(&Value::U8(2)));
    assert!(control.reads_true(&Value::Bool(true)));
}

#[test]
fn an_auto_off_readout_is_not_writable() {
    let p = amp();
    let control = p.control_by_key("temperature_reporting").unwrap();
    assert!(control.auto_off);
    assert!(!control.kind.writable());
    assert!(matches!(
        intent(&p, "temperature_reporting", "off"),
        Err(ProfileError::NotWritable(_))
    ));
}

/// An unknown key's message lists every key the profile has.
#[test]
fn an_unknown_key_lists_the_profiles_keys() {
    let p = dac();
    let message = intent(&p, "nope", "2").unwrap_err().to_string();
    assert!(message.starts_with("no control named nope; this device has: input, mute, "));
    assert!(!message.contains("did you mean"), "{message}");
}

/// A key within edit distance two of one the profile has names it; one further off names none.
#[test]
fn an_unknown_key_near_a_real_one_names_it() {
    let p = dac();
    let message = p.unknown_key("volme").to_string();
    assert!(message.ends_with("; did you mean volume?"), "{message}");
    assert!(
        !p.unknown_key("volumetric")
            .to_string()
            .contains("did you mean")
    );
}

/// A key the app 1.11 sync renamed names its successor when the profile has it.
#[test]
fn a_renamed_key_names_its_successor() {
    let mut p = dac();
    let mut power = p.control_by_key("mute").unwrap().clone();
    power.key = "power".to_string();
    p.controls.push(power);
    let message = p.unknown_key("standby").to_string();
    assert!(message.ends_with("; did you mean power?"), "{message}");
    // The successor is only named when the profile has it.
    assert!(
        !dac()
            .unknown_key("standby")
            .to_string()
            .contains("did you mean")
    );
}

/// A short key is not matched to a key two edits away, which would be nearly any of them; a
/// rename still is.
#[test]
fn a_short_key_gets_no_near_miss_suggestion() {
    let p = dac();
    assert!(!p.unknown_key("foo").to_string().contains("did you mean"));
    assert!(
        p.unknown_key("mut")
            .to_string()
            .contains("did you mean mute?")
    );
}

/// The words a device with no tone control is refused in, which the CLI, the daemon and the
/// MQTT bridge all show as the error's own text.
#[test]
fn no_tone_control_is_said_in_the_words_every_front_end_shows() {
    assert_eq!(
        ProfileError::NoEq.to_string(),
        "this device has no tone control"
    );
}

fn loki() -> Profile {
    crate::test_support::builtin("lokimax")
}

#[test]
fn intent_on_an_action_key_builds_its_empty_data_command() {
    let profile = loki();
    let message = intent(&profile, "recall", "2").unwrap();
    assert_eq!(message.id, FunctionId::three(7, 2, 2));
    assert_eq!(message.value, Value::Data(Vec::new()));
    assert_eq!(message.direction, svolder_proto::Direction::Command);
    // The key is matched without regard to case, as a control's is.
    let save = intent(&profile, "SAVE", "1").unwrap();
    assert_eq!(save.id, FunctionId::three(7, 1, 1));
    assert_eq!(save.value, Value::Data(Vec::new()));
}

#[test]
fn intent_on_an_action_accepts_a_slot_name() {
    let profile = loki();
    for input in ["Preset 3", "preset 3", " PRESET 3 ", " 3 "] {
        let message = intent(&profile, "recall", input).unwrap();
        assert_eq!(message.id, FunctionId::three(7, 2, 3), "{input:?}");
    }
}

#[test]
fn intent_refuses_a_slot_outside_the_actions_slots() {
    let profile = loki();
    for input in [
        "0", "4", "255", "Preset 4", "Preset 0", "up", "down", "min", "max", "", "x",
    ] {
        let error = intent(&profile, "recall", input).unwrap_err();
        assert!(
            matches!(error, ProfileError::BadSlot { ref key, slots: 3 } if key == "recall"),
            "{input:?}: {error}"
        );
        assert_eq!(error.to_string(), "recall takes a slot from 1 to 3");
    }
}

#[test]
fn stepping_an_action_is_refused() {
    let profile = loki();
    let error = relative(&profile, "recall", 0, 1).unwrap_err();
    assert!(matches!(error, ProfileError::ActionNotSteppable(ref key) if key == "recall"));
    assert_eq!(
        error.to_string(),
        "recall is an action: it takes a slot, never a step"
    );
    // An unknown key is still an unknown key.
    assert!(matches!(
        relative(&profile, "nothing", 0, 1),
        Err(ProfileError::UnknownKey { .. })
    ));
}

#[test]
fn an_action_has_no_reading_in_one_wording() {
    let error = ProfileError::NoReading {
        key: "recall".into(),
        device_word: "1-3".into(),
    };
    assert_eq!(
        error.to_string(),
        "recall is an action: it is sent, never read; svolder set recall 1-3 sends it"
    );
}
