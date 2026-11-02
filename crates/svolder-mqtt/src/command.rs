// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Turn one MQTT command payload into the [`Message`] the daemon's existing set path sends,
//! going through the same control lookup, unit conversion and firmware-family refusal
//! `svolder set` does rather than re-deriving any of it.

use svolder_profiles::{
    ControlKind, EqMode, Profile, ProfileError, eq_mode, intent, roles,
    safety::{self, Rules, Situation},
    units::{self, Outside},
};
use svolder_proto::Message;

use crate::discovery::EQ_MODE_KEY;

/// A command parsed from an MQTT payload, ready to send.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedCommand {
    /// The message to send the device.
    pub message: Message,
    /// Whether `max_volume_db` pulled a `volume` command down to the cap.
    pub clamped: bool,
    /// Whether the cap is below every step of the volume, so the command went to the quietest.
    pub quietest: bool,
}

/// Parse `payload` arriving on `key`'s command topic: a number for a level, snapped to its
/// nearest step, `on`/`off` (in any case `svolder set` accepts) for a toggle, and a value's
/// label or its raw number for a select — the tone-control mode's `flat`/`eq`/`loudness`
/// among them, since it is not a real profile control. A key naming no control, a control
/// that is not writable, or one in the firmware-update family is refused before anything is
/// built, the same refusal `svolder set` gives.
///
/// `max_volume_db`, when given, caps a `volume` command in its own units (dB): a value above
/// it, or one whose nearest step is, is sent as the step at or below the cap, and reported
/// through `clamped` so the caller can log it.
pub fn parse(
    profile: &Profile,
    key: &str,
    payload: &str,
    max_volume_db: Option<f64>,
) -> Result<ParsedCommand, ProfileError> {
    if key.eq_ignore_ascii_case(EQ_MODE_KEY) {
        return parse_eq_mode(profile, payload);
    }
    let control = profile
        .control_by_key(key)
        .ok_or_else(|| profile.unknown_key(key))?;
    if let Some(refusal) = control.id.refusal() {
        return Err(refusal.into());
    }
    if control.kind != ControlKind::Level {
        let message = intent(profile, key, payload)?;
        return Ok(ParsedCommand {
            message,
            clamped: false,
            quietest: false,
        });
    }
    let bad = || ProfileError::BadInput {
        key: control.key.clone(),
        input: payload.to_string(),
    };
    let value: f64 = payload.trim().parse().map_err(|_| bad())?;
    if !value.is_finite() {
        return Err(bad());
    }
    // A converted level snaps to its nearest step, clamped to its ends; one without a conversion
    // counts from its centre and is refused past either end. The cap comes first, so a value
    // above it is snapped at the cap.
    let at = roles::volume_cap(control, max_volume_db).map_or(value, |cap| value.min(cap));
    let raw = units::raw_for(control, at, Outside::Clamp).map_err(|_| bad())?;
    // The cap: a value above `max_volume_db`, or one whose nearest step is, is sent as the
    // step at or below it, a lowering from above it included.
    let situation = Situation {
        reading: None,
        base: None,
        floor_db: None,
        asked_db: Some(value),
    };
    let judged = safety::judge(control, &situation, raw, &Rules::mqtt(max_volume_db));
    if judged.capped.is_some_and(|capped| capped.refused) {
        return Err(bad());
    }
    // Reuse `intent`'s own range check and its typed `Message` build for the computed raw
    // value, rather than re-deriving either here.
    let message = intent(profile, key, &judged.target.to_string())?;
    Ok(ParsedCommand {
        message,
        clamped: judged.capped.is_some(),
        quietest: judged.capped.is_some_and(|capped| capped.quietest),
    })
}

/// Parse the tone-control mode's own command: `flat`, `eq` or `loudness`.
fn parse_eq_mode(profile: &Profile, payload: &str) -> Result<ParsedCommand, ProfileError> {
    let eq = profile.eq.as_ref().ok_or(ProfileError::NoEq)?;
    if let Some(refusal) = eq.mode_id().refusal() {
        return Err(refusal.into());
    }
    let mode: EqMode = payload.parse()?;
    let message = eq_mode(profile, mode)?;
    Ok(ParsedCommand {
        message,
        clamped: false,
        quietest: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::profile;
    use svolder_profiles::{Control, Convert, Family, convert, roles::capped_step};
    use svolder_proto::{FunctionId, Message, Value, ValueType};

    #[test]
    fn a_bare_number_snaps_to_the_nearest_raw_step() {
        let p = profile("gungnir2-v2");
        // -6.1 dB sits closer to raw 109's -6.021 than to raw 108's -6.355.
        let parsed = parse(&p, "volume", "-6.1", None).expect("a valid volume command");
        assert_eq!(parsed.message.id, FunctionId::one(10));
        assert_eq!(parsed.message.value, Value::U8(109));
        assert!(!parsed.clamped);
    }

    #[test]
    fn a_balance_command_counts_from_centre_and_refuses_past_either_end() {
        let p = profile("gungnir2-v2");
        let raw = |payload| parse(&p, "balance", payload, None).map(|c| c.message.value);
        assert_eq!(raw("0").expect("centred"), Value::U8(64));
        assert_eq!(raw("-63").expect("hard left"), Value::U8(1));
        assert_eq!(raw("63").expect("hard right"), Value::U8(127));
        // Past either end is refused, not clamped into range.
        for past in ["64", "-64", "200"] {
            let err = raw(past).unwrap_err();
            assert!(
                matches!(err, ProfileError::BadInput { .. }),
                "{past}: {err}"
            );
        }
    }

    #[test]
    fn max_volume_db_caps_a_volume_command_and_reports_the_clamp() {
        let p = profile("gungnir2-v2");
        let parsed = parse(&p, "volume", "-2.0", Some(-10.0)).expect("a clamped command");
        assert!(parsed.clamped);
        // -10.0 dB snaps to raw 97's -10.034, the nearest table entry to the cap.
        assert_eq!(parsed.message.value, Value::U8(97));
        let unclamped = parse(&p, "volume", "-20.0", Some(-10.0)).expect("under the cap");
        assert!(!unclamped.clamped);
    }

    #[test]
    fn a_cap_below_every_step_sends_the_quietest_step_and_says_so() {
        let p = profile("freya2");
        let parsed = parse(&p, "volume", "-40", Some(-200.0)).expect("the quietest step");
        assert_eq!(parsed.message.value, Value::U8(0));
        assert!(parsed.clamped && parsed.quietest);
        let capped = parse(&p, "volume", "-40", Some(-10.0)).expect("under the cap");
        assert!(!capped.quietest);
    }

    #[test]
    fn a_value_past_the_end_of_a_range_with_no_conversion_is_capped_not_refused() {
        let mut p = profile("gungnir2-v2");
        let volume = p
            .controls
            .iter_mut()
            .find(|c| c.key == "volume")
            .expect("gungnir2-v2 has a volume");
        volume.convert = None;
        volume.unit = None;
        volume.range = Some((0, 100));
        let origin = roles::origin(volume);
        let parsed =
            parse(&p, "volume", "150", Some(50.0 - origin as f64)).expect("capped, not refused");
        assert!(parsed.clamped);
        assert_eq!(parsed.message.value, Value::U8(50));
    }

    #[test]
    fn a_value_below_a_narrow_table_is_capped_not_refused() {
        let mut p = profile("gungnir2-v2");
        let volume = p
            .controls
            .iter_mut()
            .find(|c| c.key == "volume")
            .expect("gungnir2-v2 has a volume");
        volume.range = Some((0, 100));
        let parsed = parse(&p, "volume", "0", Some(-30.0)).expect("capped, not refused");
        assert!(parsed.clamped);
        // The highest step of the 0 to 100 table at or below -30 dB.
        assert_eq!(parsed.message.value, Value::U8(55));
    }

    #[test]
    fn a_volume_command_is_never_sent_above_the_cap() {
        let p = profile("gungnir2-v2");
        let volume = p
            .control_by_key("volume")
            .expect("gungnir2-v2 has a volume");
        let value = |m: &Message| convert::raw_to_display(volume, m.value.as_i64().unwrap());
        // -9.8 dB is nearest raw 98 (-9.700), above a -9.8 cap: the command goes out as raw 97
        // (-10.034), the step at or below it.
        let parsed = parse(&p, "volume", "-9.8", Some(-9.8)).expect("a capped command");
        assert!(parsed.clamped);
        assert_eq!(parsed.message.value, Value::U8(97));
        for cap in [-30.0, -9.8, -10.0, -0.5] {
            for asked in ["0", "-5", "-9.9", "-10", "-29.9"] {
                let parsed = parse(&p, "volume", asked, Some(cap)).expect("a volume command");
                let sent = value(&parsed.message).expect("a step's value");
                assert!(
                    sent <= cap,
                    "{asked} dB under a {cap} dB cap was sent as {sent} dB"
                );
            }
        }
    }

    #[test]
    fn the_capped_step_is_the_highest_at_or_below_the_cap() {
        let p = profile("gungnir2-v2");
        let volume = p
            .control_by_key("volume")
            .expect("gungnir2-v2 has a volume");
        assert_eq!(capped_step(volume, -10.0), Some(97));
        assert_eq!(capped_step(volume, -9.8), Some(97));
        assert_eq!(capped_step(volume, 0.0), Some(127));
        let balance = p
            .control_by_key("balance")
            .expect("gungnir2-v2 has a balance");
        assert_eq!(
            capped_step(balance, 0.6),
            Some(64),
            "a balance's steps from centre"
        );
        assert_eq!(capped_step(balance, -64.0), None, "nothing is that low");
    }

    #[test]
    fn the_cap_never_touches_a_control_that_is_not_volume() {
        let p = profile("gungnir2-v2");
        // balance has no unit at all; a cap meant for volume must never reach it.
        let parsed = parse(&p, "balance", "63", Some(-10.0)).expect("balance is not capped");
        assert!(!parsed.clamped);
        assert_eq!(parsed.message.value, Value::U8(127));
    }

    #[test]
    fn a_toggle_takes_on_and_off() {
        let p = profile("gungnir2-v2");
        let on = parse(&p, "mute", "ON", None).expect("ON is a valid toggle payload");
        assert_eq!(on.message.value, Value::Bool(true));
        let off = parse(&p, "mute", "off", None).expect("off is a valid toggle payload too");
        assert_eq!(off.message.value, Value::Bool(false));
    }

    #[test]
    fn a_select_takes_its_label() {
        let p = profile("gungnir2-v2");
        let parsed = parse(&p, "input", "Optical", None).expect("a valid select label");
        assert_eq!(parsed.message.value, Value::U8(3));
    }

    #[test]
    fn the_tone_control_mode_takes_its_three_names() {
        let p = profile("gungnir2-v2");
        let eq = p.eq.clone().expect("gungnir2-v2 has a tone control");
        let parsed = parse(&p, "eq_mode", "loudness", None).expect("a valid mode name");
        assert_eq!(parsed.message.id, eq.mode_id());
        assert_eq!(parsed.message.value, Value::U8(3));
    }

    #[test]
    fn an_unknown_key_is_refused() {
        let p = profile("gungnir2-v2");
        let err = parse(&p, "nonesuch", "1", None).unwrap_err();
        assert!(matches!(err, ProfileError::UnknownKey { .. }));
    }

    #[test]
    fn a_key_in_the_firmware_family_is_refused_before_anything_is_built() {
        let mut p = profile("gungnir2-v2");
        p.controls.push(Control {
            id: FunctionId::two(210, 5),
            key: "not_a_real_control".to_string(),
            label: "Should never exist".to_string(),
            kind: ControlKind::Toggle,
            value_type: ValueType::Bool,
            values: Vec::new(),
            range: None,
            center: None,
            unit: None,
            convert: None,
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
        });
        let err = parse(&p, "not_a_real_control", "ON", None).unwrap_err();
        assert!(err.to_string().contains("firmware-update family"), "{err}");
    }

    #[test]
    fn a_readout_is_refused_as_not_writable() {
        let p = profile("gungnir2-v2");
        let err = parse(&p, "sample_rate", "48 kHz", None).unwrap_err();
        assert!(matches!(err, ProfileError::NotWritable(_)));
    }

    #[test]
    fn garbage_input_is_refused_the_same_way_svolder_set_refuses_it() {
        let p = profile("gungnir2-v2");
        let err = parse(&p, "volume", "not a number", None).unwrap_err();
        assert!(matches!(err, ProfileError::BadInput { .. }));
        let mute_err = parse(&p, "mute", "sideways", None).unwrap_err();
        assert!(matches!(mute_err, ProfileError::BadInput { .. }));
    }

    /// A linear-converted level (no built-in profile has one, so this control is built by
    /// hand) is capped in its own units too, just like the table-converted `volume`.
    #[test]
    fn a_linear_level_named_volume_is_capped_the_same_way() {
        let mut p = Profile {
            name: "linear-test".to_string(),
            display: "Linear test".to_string(),
            product_id: None,
            min_firmware: 0,
            controls: Vec::new(),
            eq: None,
            family: Family::Dac,
            firmware_subsystems: Vec::new(),
            identify: true,
            actions: Vec::new(),
            builtin_settings: None,
        };
        p.controls.push(Control {
            id: FunctionId::one(10),
            key: "volume".to_string(),
            label: "Volume".to_string(),
            kind: ControlKind::Level,
            value_type: ValueType::U8,
            values: Vec::new(),
            range: Some((0, 127)),
            center: None,
            unit: Some("dB".to_string()),
            convert: Some(Convert::Linear {
                min: -80.0,
                max: 0.0,
            }),
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
        });
        let parsed = parse(&p, "volume", "-5.0", Some(-10.0)).expect("clamped to -10 dB");
        assert!(parsed.clamped);
        // -10 dB is 7/8 of the way from -80 to 0, so 0.875 * 127 = 111.125, rounding to 111.
        assert_eq!(parsed.message.value, Value::U8(111));
    }
}
