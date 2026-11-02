// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_proto::{FunctionId, ValueType};

use super::*;
use crate::EqParam;
use crate::registry::Registry;
use crate::test_support::{builtin, control};

/// A level on `range` with `convert` and `unit`, and no centre.
fn level(range: (i64, i64), convert: Option<Convert>, unit: Option<&str>) -> Control {
    Control {
        id: FunctionId::one(10),
        key: "level".into(),
        label: "Level".into(),
        kind: ControlKind::Level,
        value_type: ValueType::U8,
        values: vec![],
        range: Some(range),
        center: None,
        unit: unit.map(str::to_string),
        convert,
        channel: None,
        floor: None,
        tick: None,
        log_scale: None,
        advice: None,
        only_while: None,
        auto_off: false,
        output_gain: false,
        mutes_output: false,
        powers_output: false,
        asks_on_change: false,
        reads: Vec::new(),
    }
}

#[test]
fn a_level_reads_in_its_unit_after_a_space_and_silence_at_the_bottom() {
    let volume = control("gungnir2-v2", "volume");
    assert_eq!(text(&volume, 109).as_deref(), Some("-6.0 dB"));
    assert_eq!(text(&volume, 127).as_deref(), Some("0.0 dB"));
    assert_eq!(text(&volume, 0).as_deref(), Some("silence"));
    assert_eq!(amount(&volume, 0), Some(Amount::Silence));
    assert_eq!(amount(&volume, 109), Some(Amount::Value(-6.021)));
}

#[test]
fn minus_zero_reads_as_zero() {
    assert_eq!(amount_text(Amount::Value(-0.0), Some("dB")), "0.0 dB");
    assert_eq!(amount_text(Amount::Value(-0.04), Some("dB")), "0.0 dB");
    assert_eq!(amount_text(Amount::Value(-0.06), Some("dB")), "-0.1 dB");
    assert_eq!(amount_text(Amount::Silence, Some("dB")), "silence");
    assert_eq!(amount_text(Amount::Value(-6.021), None), "-6.0");
}

#[test]
fn a_centred_level_reads_as_its_distance_from_the_centre() {
    let balance = control("gungnir2-v2", "balance");
    assert_eq!(origin(&balance), 64);
    assert_eq!(text(&balance, 64).as_deref(), Some("0"));
    assert_eq!(text(&balance, 70).as_deref(), Some("6"));
    assert_eq!(text(&balance, 59).as_deref(), Some("-5"));
    assert_eq!(amount(&balance, 70), Some(Amount::Value(6.0)));
    assert_eq!(
        text(&control("freya2", "balance"), 0).as_deref(),
        Some("-100")
    );
}

#[test]
fn a_converted_level_without_a_unit_reads_to_a_tenth() {
    let bare = level(
        (0, 127),
        Some(Convert::Linear {
            min: -80.0,
            max: 0.0,
        }),
        None,
    );
    assert_eq!(text(&bare, 127).as_deref(), Some("0.0"));
    assert_eq!(text(&bare, 0).as_deref(), Some("-80.0"));
}

#[test]
fn a_raw_outside_the_table_has_no_wording() {
    let mut volume = control("gungnir2-v2", "volume");
    assert_eq!(text(&volume, 128), None);
    assert_eq!(amount(&volume, -1), None);
    volume.range = Some((0, 1_000_000));
    assert_eq!(text(&volume, 500_000), None, "past the table's end");
}

#[test]
fn only_a_level_has_an_amount_or_a_wording() {
    let mute = control("gungnir2-v2", "mute");
    assert_eq!(amount(&mute, 1), None);
    assert_eq!(text(&mute, 1), None);
}

#[test]
fn a_one_value_range_reads_its_one_value_and_takes_any_other() {
    let one = level(
        (5, 5),
        Some(Convert::Linear {
            min: -10.0,
            max: 0.0,
        }),
        Some("dB"),
    );
    assert_eq!(text(&one, 5).as_deref(), Some("-10.0 dB"));
    let b = bounds(&one).expect("bounds");
    assert_eq!((b.raw, b.min, b.max, b.step), ((5, 5), -10.0, 0.0, 0.0));
    assert_eq!(raw_for(&one, -3.0, Outside::Clamp), Ok(5));
}

#[test]
fn no_range_means_no_bounds_and_no_raw() {
    let mut volume = control("gungnir2-v2", "volume");
    volume.range = None;
    assert_eq!(bounds(&volume), None);
    assert_eq!(
        raw_for(&volume, -6.0, Outside::Clamp),
        Err(Refusal::NoRange)
    );
}

#[test]
fn the_bounds_are_the_numbers_home_assistant_is_given() {
    let b = bounds(&control("gungnir2-v2", "volume")).expect("bounds");
    assert_eq!(
        (b.raw, b.min, b.max, b.step),
        ((0, 127), -96.33, 0.0, 0.334)
    );
    assert_eq!(b.unit.as_deref(), Some("dB"));
    assert!(!b.centred);
    assert_eq!(b.silence, Some(0));
    let b = bounds(&control("gungnir2-v2", "balance")).expect("bounds");
    assert_eq!((b.raw, b.min, b.max, b.step), ((1, 127), -63.0, 63.0, 1.0));
    assert!(b.centred);
    assert_eq!(b.silence, None);
    let b = bounds(&control("freya2", "volume")).expect("bounds");
    assert_eq!((b.min, b.max, b.step), (-80.0, 0.0, 0.63));
    assert_eq!(b.silence, None, "a linear volume bottoms out at a number");
}

#[test]
fn spread_takes_the_finite_values_rounded_to_three_places() {
    assert_eq!(
        spread(vec![f64::NEG_INFINITY, -1.0004, 0.0, -0.5]),
        Some((-1.0, 0.0, 0.5))
    );
    assert_eq!(spread(vec![2.0]), Some((2.0, 2.0, 1.0)));
    assert_eq!(spread(vec![f64::NEG_INFINITY]), None);
}

#[test]
fn not_a_number_and_the_infinities_are_refused_before_anything_else() {
    let volume = control("gungnir2-v2", "volume");
    let saga = control("saga2", "volume");
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for outside in [Outside::Clamp, Outside::Refuse] {
            assert_eq!(
                raw_for(&volume, value, outside),
                Err(Refusal::NotFinite),
                "{value}"
            );
            assert_eq!(
                raw_for(&saga, value, outside),
                Err(Refusal::NotFinite),
                "{value}"
            );
        }
    }
}

#[test]
fn a_converted_level_clamps_or_refuses_past_its_ends_as_asked() {
    let volume = control("gungnir2-v2", "volume");
    assert_eq!(raw_for(&volume, -6.0, Outside::Clamp), Ok(109));
    assert_eq!(
        raw_for(&volume, -200.0, Outside::Clamp),
        Ok(1),
        "never silence"
    );
    assert_eq!(raw_for(&volume, 5.0, Outside::Clamp), Ok(127));
    assert_eq!(
        raw_for(&volume, 5.0, Outside::Refuse),
        Err(Refusal::Outside {
            min: -96.33,
            max: 0.0
        })
    );
    assert_eq!(raw_for(&volume, -96.33, Outside::Refuse), Ok(1));
}

#[test]
fn a_level_without_a_conversion_counts_from_its_centre_and_always_refuses_past_its_ends() {
    let balance = control("gungnir2-v2", "balance");
    assert_eq!(raw_for(&balance, 0.0, Outside::Clamp), Ok(64));
    assert_eq!(raw_for(&balance, -63.0, Outside::Refuse), Ok(1));
    assert_eq!(raw_for(&balance, 6.4, Outside::Clamp), Ok(70));
    for past in [64.0, -64.0] {
        for outside in [Outside::Clamp, Outside::Refuse] {
            assert_eq!(
                raw_for(&balance, past, outside),
                Err(Refusal::Outside {
                    min: -63.0,
                    max: 63.0
                }),
                "{past}"
            );
        }
    }
}

#[test]
fn a_number_reads_with_or_without_its_unit_in_any_case() {
    for text in ["-6", "-6dB", "-6 dB", " -6 DB ", "-6.0 db"] {
        assert_eq!(number_in(text, Some("dB")), Some(-6.0), "{text}");
    }
    assert_eq!(number_in("6", None), Some(6.0));
    assert_eq!(
        number_in("-6 dB", None),
        None,
        "a unit the control does not have"
    );
    for text in ["loud", "inf dB", "NaN", ""] {
        assert_eq!(number_in(text, Some("dB")), None, "{text}");
    }
}

#[test]
fn a_gated_readout_shows_true_only_while_it_and_its_gate_read_true() {
    let tyr = builtin("tyr");
    let warming = tyr.control_by_key("warming_up").expect("a gated readout");
    let gate = |reads: Option<bool>| move |_: &Control| reads;
    assert!(shows_true(&tyr, warming, true, gate(Some(true))));
    assert!(!shows_true(&tyr, warming, true, gate(Some(false))));
    assert!(
        !shows_true(&tyr, warming, true, gate(None)),
        "a gate not heard is closed"
    );
    assert!(!shows_true(&tyr, warming, false, gate(Some(true))));
    let gungnir = builtin("gungnir2-v2");
    let mute = gungnir.control_by_key("mute").expect("a toggle");
    assert!(shows_true(&gungnir, mute, true, gate(None)), "no gate");
}

#[test]
fn an_amount_is_a_number_for_a_program_or_none_at_silence() {
    assert_eq!(Amount::of(f64::NEG_INFINITY), Amount::Silence);
    assert_eq!(Amount::of(-6.021), Amount::Value(-6.021));
    assert_eq!(Amount::Silence.number(), None);
    assert_eq!(Amount::Value(6.0).number(), Some(6.0));
}

/// A band's values read the way the TUI's rows read them, with their units: a frequency to the
/// tenth below 100 Hz and in whole hertz above, a gain signed to the hundredth a step lands on
/// (a whole dB to the tenth), and Q to the hundredth.
#[test]
fn band_values_read_with_their_units_to_the_devices_step() {
    let registry = Registry::builtin().expect("the built-in profiles load");
    let eq = registry
        .get("gungnir2-v2")
        .and_then(|p| p.eq.clone())
        .expect("a tone control");
    let text = |band, param, raw| band_text(&eq, band, param, raw).expect("a text");
    assert_eq!(text(0, EqParam::Frequency, 64), "40.0 Hz");
    assert_eq!(
        text(0, EqParam::Frequency, 65),
        format!(
            "{} Hz",
            crate::convert::frequency_text(eq.frequency(0, 65).unwrap())
        )
    );
    assert_eq!(text(1, EqParam::Frequency, 64), "160 Hz");
    assert_eq!(text(0, EqParam::Gain, 98), "+0.0 dB");
    assert_eq!(text(0, EqParam::Gain, 127), "+6.0 dB");
    assert_eq!(text(0, EqParam::Gain, 110), "+2.48 dB");
    assert_eq!(text(0, EqParam::Gain, 97), "-0.20 dB");
    assert_eq!(text(2, EqParam::Q, 0), "0.50");
    assert_eq!(text(2, EqParam::Q, 64), "4.30");
    assert_eq!(band_text(&eq, 0, EqParam::Gain, 300), None);
    assert_eq!(band_text(&eq, 9, EqParam::Frequency, 64), None);
}
