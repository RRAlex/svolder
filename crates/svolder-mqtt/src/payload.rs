// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The JSON published on the state and meters topics, in the human units every other front
//! end already shows: volume in dB, toggles as `ON`/`OFF`, selects and readouts by their
//! label.

use serde_json::{Map, Value as Json};

use svolder_profiles::units::{self, Amount};
use svolder_profiles::{Control, ControlKind, Profile};
use svolder_proto::{FunctionId, Message, ValueType};

use crate::discovery::exposed_controls;
use crate::json_number;

/// The payload the bridge and device availability topics carry while up, retained.
pub const ONLINE: &str = "online";
/// The payload the bridge and device availability topics carry while down: the bridge's own
/// last will, and what the daemon publishes for a device it no longer holds.
pub const OFFLINE: &str = "offline";

/// The main firmware build's reading.
pub const FIRMWARE: FunctionId = FunctionId::three(210, 2, 1);
/// The DSP firmware build's reading.
pub const DSP_FIRMWARE: FunctionId = FunctionId::three(210, 2, 2);

/// The text of a firmware build the device reported as function `id`, or `None` before it has:
/// nothing is published until then, so Home Assistant shows the sensor as unknown.
pub fn firmware_state(readings: &[Message], id: FunctionId) -> Option<String> {
    readings
        .iter()
        .find(|m| m.id == id)
        .and_then(|m| m.value.as_u64())
        .map(|build| build.to_string())
}

/// The signal sensor's state: the strength in dBm as a plain number.
pub fn signal_state(dbm: i16) -> String {
    dbm.to_string()
}

/// The signal sensor's attributes: where the strength was measured (`source`, `advert` or
/// `live`) and when the daemon heard it as an RFC 3339 UTC stamp (`heard_at_ms` is unix
/// milliseconds).
pub fn signal_attributes(source: &str, heard_at_ms: u64) -> Json {
    serde_json::json!({"source": source, "heard_at": rfc3339_utc(heard_at_ms)})
}

/// `unix_ms` as `YYYY-MM-DDTHH:MM:SSZ`, in UTC, the milliseconds dropped.
pub fn rfc3339_utc(unix_ms: u64) -> String {
    let secs = unix_ms / 1000;
    let in_day = secs % 86_400;
    // The civil calendar from a day count, after Howard Hinnant's `civil_from_days`.
    let shifted = i64::try_from(secs / 86_400).unwrap_or(i64::MAX / 2) + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_part = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_part + 2) / 5 + 1;
    let month = if month_part < 10 {
        month_part + 3
    } else {
        month_part - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        in_day / 3_600,
        in_day % 3_600 / 60,
        in_day % 60
    )
}

/// The retained state: one key per exposed control but the meters, for every one of
/// them a message in `readings` has already answered, and for a tone control its loudness
/// strength and every band parameter read so far (see [`crate::eq`]). A control nothing has
/// reported yet is left out rather than guessed at. A device with presets of its own carries its
/// preset select's state ([`crate::device_presets::state`]). A tone control's is not here: it
/// depends on which preset was last recalled, which only the bridge knows
/// ([`crate::eq::preset_state`]).
pub fn state(profile: &Profile, readings: &[Message]) -> Json {
    let mut state = build(profile, readings, |kind| kind != ControlKind::Meter);
    if let Json::Object(map) = &mut state {
        crate::eq::add_readings(profile, readings, map);
        crate::device_presets::add_state(profile, readings, map);
    }
    state
}

/// The meters payload: one key per meter that has reported, a meter with a scale as the whole
/// percentage of its live part (0 while the meters are dead), any other in its own units.
pub fn meters(profile: &Profile, readings: &[Message]) -> Json {
    let reading = |id| readings.iter().find(|m: &&Message| m.id == id);
    let live = profile.meters_live(|id| {
        let control = profile.control_by_id(id)?;
        reading(id).map(|m| control.reads_true(&m.value))
    });
    let mut map = Map::new();
    for control in exposed_controls(profile) {
        if control.kind != ControlKind::Meter {
            continue;
        }
        let Some(message) = reading(control.id) else {
            continue;
        };
        let placed = profile.placed(&control, |id| reading(id).and_then(|m| m.value.as_i64()));
        let value = match (placed, message.value.as_i64()) {
            (Some(placed), Some(raw)) if live => {
                Json::from((placed.fraction(raw as f64) * 100.0).round() as i64)
            }
            (Some(_), Some(_)) => Json::from(0),
            (Some(_), None) => Json::Null,
            (None, _) => human_value(profile, readings, &control, message),
        };
        map.insert(control.key.clone(), value);
    }
    Json::Object(map)
}

fn build(profile: &Profile, readings: &[Message], wanted: impl Fn(ControlKind) -> bool) -> Json {
    let mut map = Map::new();
    for control in exposed_controls(profile) {
        if !wanted(control.kind) {
            continue;
        }
        if let Some(message) = readings.iter().find(|m| m.id == control.id) {
            map.insert(
                control.key.clone(),
                human_value(profile, readings, &control, message),
            );
        }
    }
    Json::Object(map)
}

/// One control's current value in its own human units: `ON`/`OFF` for a toggle, a fault or a
/// boolean readout (an integer 1 reads `ON`, and an `only_while` readout reads `OFF` while the
/// control it names does not read true), a value's label (falling back to the raw number) for a select or a
/// readout, and a level's converted number (falling back to the raw number when it has no conversion, and to `null`
/// where the conversion is the digital volume table's silence point — JSON has no way to
/// carry `-inf`). A message of the wrong wire type reports `null` rather than the wrong unit.
fn human_value(
    profile: &Profile,
    readings: &[Message],
    control: &Control,
    message: &Message,
) -> Json {
    match control.kind {
        ControlKind::Toggle | ControlKind::Fault => Json::from(on_off(message.value.bool_value())),
        ControlKind::Readout if control.value_type == ValueType::Bool => {
            let shown = units::shows_true(
                profile,
                control,
                control.reads_true(&message.value),
                |gate| {
                    readings
                        .iter()
                        .find(|m| m.id == gate.id)
                        .map(|m| gate.reads_true(&m.value))
                },
            );
            Json::from(on_off(shown))
        }
        ControlKind::Select | ControlKind::Readout => {
            match message.value.as_u64().and_then(|v| u8::try_from(v).ok()) {
                Some(raw) => match control.value_name(raw) {
                    Some(name) => Json::from(name),
                    None => Json::from(raw),
                },
                None => Json::Null,
            }
        }
        // A converted level publishes its value unrounded; silence has no JSON number, so it is
        // `null`; a level without a conversion, a converted raw outside its table and a meter
        // without a scale publish their raw step counted from the centre.
        ControlKind::Level | ControlKind::Meter => match message.value.as_i64() {
            Some(raw) => match units::amount(control, raw) {
                Some(Amount::Value(value)) if control.convert.is_some() => json_number(value),
                Some(Amount::Silence) => Json::Null,
                _ => Json::from(raw - units::origin(control)),
            },
            None => Json::Null,
        },
    }
}

/// `"ON"`/`"OFF"`, the payload every toggle and fault reads in the state JSON.
fn on_off(on: bool) -> &'static str {
    if on { "ON" } else { "OFF" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::profile;
    use svolder_proto::{FunctionId, Value};

    #[test]
    fn signal_attributes_say_source_and_time() {
        // 2026-10-25T12:34:56.789Z
        let heard = 1_792_931_696_789;
        assert_eq!(
            signal_attributes("advert", heard),
            serde_json::json!({"source": "advert", "heard_at": "2026-10-25T12:34:56Z"})
        );
        assert_eq!(
            signal_attributes("live", heard)["source"],
            "live",
            "the source is carried as given"
        );
        assert_eq!(signal_state(-62), "-62");
        assert_eq!(signal_state(0), "0");
    }

    #[test]
    fn a_stamp_reads_as_rfc_3339_in_utc() {
        assert_eq!(rfc3339_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(
            rfc3339_utc(951_782_400_000),
            "2000-02-29T00:00:00Z",
            "a leap day"
        );
        assert_eq!(rfc3339_utc(1_709_251_199_999), "2024-02-29T23:59:59Z");
        assert_eq!(rfc3339_utc(1_709_251_200_000), "2024-03-01T00:00:00Z");
        assert_eq!(rfc3339_utc(4_102_444_799_000), "2099-12-31T23:59:59Z");
        // 2100 is not a leap year: February ends on the 28th.
        assert_eq!(rfc3339_utc(4_107_542_399_000), "2100-02-28T23:59:59Z");
        assert_eq!(rfc3339_utc(4_107_542_400_000), "2100-03-01T00:00:00Z");
        // The largest stamp still words as a date, and does not panic.
        assert!(rfc3339_utc(u64::MAX).ends_with('Z'));
    }

    #[test]
    fn a_firmware_build_is_published_when_the_device_reports_it() {
        let none: [Message; 0] = [];
        assert_eq!(firmware_state(&none, FIRMWARE), None);
        assert_eq!(firmware_state(&none, DSP_FIRMWARE), None);
        let readings = [Message::status(DSP_FIRMWARE, Value::U16(17))];
        assert_eq!(firmware_state(&readings, FIRMWARE), None);
        assert_eq!(
            firmware_state(&readings, DSP_FIRMWARE).as_deref(),
            Some("17")
        );
        let both = [
            Message::status(FIRMWARE, Value::U16(204)),
            Message::status(DSP_FIRMWARE, Value::U16(17)),
        ];
        assert_eq!(firmware_state(&both, FIRMWARE).as_deref(), Some("204"));
    }

    #[test]
    fn a_level_reports_its_converted_number() {
        let p = profile("gungnir2-v2");
        let readings = [Message::status(FunctionId::one(10), Value::U8(109))];
        let json = state(&p, &readings);
        assert_eq!(json["volume"], -6.021);
    }

    #[test]
    fn a_balance_reports_its_steps_from_centre() {
        let p = profile("gungnir2-v2");
        let state_of = |raw| state(&p, &[Message::status(FunctionId::one(11), Value::U8(raw))]);
        assert_eq!(state_of(64)["balance"], 0, "centred");
        assert_eq!(state_of(70)["balance"], 6, "right of centre");
        assert_eq!(state_of(1)["balance"], -63, "hard left");
        assert_eq!(state_of(127)["balance"], 63, "hard right");
    }

    #[test]
    fn a_freya_2_balance_counts_from_its_own_centre() {
        let p = profile("freya2");
        let readings = [Message::status(FunctionId::two(6, 2), Value::U8(90))];
        assert_eq!(state(&p, &readings)["balance"], -10);
    }

    #[test]
    fn digital_silence_has_no_json_number_so_it_reports_null() {
        let p = profile("gungnir2-v2");
        let readings = [Message::status(FunctionId::one(10), Value::U8(0))];
        let json = state(&p, &readings);
        assert!(json["volume"].is_null());
    }

    #[test]
    fn a_toggle_reports_on_or_off() {
        let p = profile("gungnir2-v2");
        let on = [Message::status(FunctionId::one(5), Value::Bool(true))];
        assert_eq!(state(&p, &on)["mute"], "ON");
        let off = [Message::status(FunctionId::one(5), Value::Bool(false))];
        assert_eq!(state(&p, &off)["mute"], "OFF");
    }

    /// The state line `docs/usage/mqtt.md` quotes for a Gungnir 2 on the fake transport, keys
    /// sorted.
    #[test]
    fn a_fresh_gungnir_2_publishes_the_state_object_the_docs_quote() {
        let p = profile("gungnir2-v2");
        let u8s = [(2, 1), (6, 0), (10, 63), (11, 64)];
        let bools = [(3, false), (4, false), (5, false)];
        let mut readings: Vec<Message> = u8s
            .iter()
            .map(|(id, v)| Message::status(FunctionId::one(*id), Value::U8(*v)))
            .collect();
        readings.extend(
            bools
                .iter()
                .map(|(id, v)| Message::status(FunctionId::one(*id), Value::Bool(*v))),
        );
        readings.extend([
            Message::status(FunctionId::two(7, 1), Value::Bool(false)),
            Message::status(FunctionId::two(7, 2), Value::Bool(false)),
            Message::status(FunctionId::one(8), Value::Bool(false)),
            Message::status(FunctionId::two(12, 50), Value::U8(1)),
        ]);
        assert_eq!(
            state(&p, &readings).to_string(),
            r#"{"balance":0,"eq_mode":"flat","input":"USB 1","mute":"OFF","nos":"OFF","phase":"OFF","sample_rate":"—","spdif_locked":"OFF","usb1_connected":"OFF","usb2_connected":"OFF","volume":-26.089}"#
        );
    }

    #[test]
    fn a_boolean_readout_reports_on_or_off() {
        let p = profile("gungnir2-v2");
        let locked = [Message::status(FunctionId::one(8), Value::Bool(true))];
        assert_eq!(state(&p, &locked)["spdif_locked"], "ON");
        let unlocked = [Message::status(FunctionId::one(8), Value::Bool(false))];
        assert_eq!(state(&p, &unlocked)["spdif_locked"], "OFF");
    }

    #[test]
    fn an_integer_on_a_boolean_readout_reads_on_only_for_one() {
        let p = profile("gungnir2-v2");
        for (raw, shown) in [(0, "OFF"), (1, "ON"), (2, "OFF")] {
            let readings = [Message::status(FunctionId::one(8), Value::U8(raw))];
            assert_eq!(state(&p, &readings)["spdif_locked"], shown, "{raw}");
        }
    }

    #[test]
    fn a_gated_readout_reports_off_while_its_control_reads_off() {
        let source = "schema = 1\nname = \"t\"\n\
            [[control]]\nid = [4]\nkey = \"warming_up\"\nlabel = \"Warming up\"\nkind = \"readout\"\n\
            value_type = \"bool\"\nonly_while = \"power\"\n\
            [[control]]\nid = [10]\nkey = \"power\"\nlabel = \"Power\"\nkind = \"toggle\"\n";
        let p = svolder_profiles::Registry::from_sources(vec![("t".into(), source.into())])
            .unwrap()
            .get("t")
            .unwrap()
            .clone();
        let warming = Message::status(FunctionId::one(4), Value::U8(1));
        let awake = [
            warming.clone(),
            Message::status(FunctionId::one(10), Value::Bool(true)),
        ];
        assert_eq!(state(&p, &awake)["warming_up"], "ON");
        let asleep = [
            warming.clone(),
            Message::status(FunctionId::one(10), Value::Bool(false)),
        ];
        assert_eq!(state(&p, &asleep)["warming_up"], "OFF");
        assert_eq!(
            state(&p, &[warming])["warming_up"],
            "OFF",
            "no reading of the gate"
        );
    }

    #[test]
    fn a_gate_change_alone_changes_a_gated_readouts_state() {
        let source = "schema = 1\nname = \"t\"\n\
            [[control]]\nid = [4]\nkey = \"warming_up\"\nlabel = \"Warming up\"\nkind = \"readout\"\n\
            value_type = \"bool\"\nonly_while = \"power\"\n\
            [[control]]\nid = [10]\nkey = \"power\"\nlabel = \"Power\"\nkind = \"toggle\"\n";
        let p = svolder_profiles::Registry::from_sources(vec![("t".into(), source.into())])
            .unwrap()
            .get("t")
            .unwrap()
            .clone();
        let warming = Message::status(FunctionId::one(4), Value::Bool(true));
        let with_power = |on| {
            [
                warming.clone(),
                Message::status(FunctionId::one(10), Value::Bool(on)),
            ]
        };
        let off = state(&p, &with_power(false));
        let on = state(&p, &with_power(true));
        // The dependent's own reading is the same in both; the published state differs, and
        // the bridge publishes whenever the state text differs from the last one sent.
        assert_eq!(off["warming_up"], "OFF");
        assert_eq!(on["warming_up"], "ON");
        assert_ne!(off.to_string(), on.to_string());
        assert_eq!(on.to_string(), state(&p, &with_power(true)).to_string());
    }

    #[test]
    fn a_fault_reports_on_or_off_too() {
        let p = profile("aegir2");
        let active = [Message::status(FunctionId::two(230, 1), Value::Bool(true))];
        assert_eq!(state(&p, &active)["fault_overcurrent"], "ON");
    }

    #[test]
    fn a_select_reports_its_label() {
        let p = profile("gungnir2-v2");
        let readings = [Message::status(FunctionId::one(2), Value::U8(3))];
        let json = state(&p, &readings);
        assert_eq!(json["input"], "Optical");
    }

    #[test]
    fn a_readout_reports_its_label_too() {
        let p = profile("gungnir2-v2");
        let readings = [Message::status(FunctionId::one(6), Value::U8(2))];
        let json = state(&p, &readings);
        assert_eq!(json["sample_rate"], "48 kHz");
    }

    #[test]
    fn the_tone_control_mode_reports_its_name() {
        let p = profile("gungnir2-v2");
        let eq = p.eq.clone().expect("gungnir2-v2 has a tone control");
        let readings = [Message::status(eq.mode_id(), Value::U8(2))];
        let json = state(&p, &readings);
        assert_eq!(json["eq_mode"], "eq");
    }

    #[test]
    fn a_control_with_no_reading_yet_is_left_out() {
        let p = profile("gungnir2-v2");
        let json = state(&p, &[]);
        assert!(json.get("volume").is_none());
        assert!(json.as_object().expect("an object").is_empty());
    }

    #[test]
    fn state_carries_no_meters_and_meters_carries_only_them() {
        let p = profile("fake-amp");
        let readings = [
            Message::status(FunctionId::two(20, 1), Value::U16(173)),
            Message::status(FunctionId::one(4), Value::Bool(true)),
        ];
        assert!(state(&p, &readings).get("meter_1").is_none());
        let m = meters(&p, &readings);
        // The Aegir 2's scale puts 173 half way up the live part.
        assert_eq!(m["meter_1"], 50);
        assert!(m.get("mute").is_none());
    }

    /// A meter with a scale publishes the whole percentage of its live part, and 0 while the
    /// meters are dead; a meter with none keeps its own units.
    #[test]
    fn a_scaled_meter_reads_as_a_percentage_and_as_nought_while_dead() {
        let p = profile("fake-amp");
        let meter = |raw, switch| {
            let readings = [
                Message::status(FunctionId::two(20, 1), Value::U16(raw)),
                Message::status(FunctionId::two(21, 2), Value::Bool(switch)),
            ];
            meters(&p, &readings)
        };
        let switch = p.control_by_key("power_meters").expect("the switch");
        assert_eq!(switch.id, FunctionId::two(21, 2));
        assert_eq!(meter(173, true)["meter_1"], 50);
        assert_eq!(meter(350, true)["meter_1"], 100);
        assert_eq!(meter(173, false)["meter_1"], 0);

        let mut unscaled = p.clone();
        for control in &mut unscaled.controls {
            control.range = None;
            control.log_scale = None;
        }
        let readings = [Message::status(FunctionId::two(20, 1), Value::U16(512))];
        assert_eq!(meters(&unscaled, &readings)["meter_1"], 512);
    }

    /// The percentage is published while the switch has not reported (unreported counts as
    /// live) and 0 while the amp is in standby or warming up.
    #[test]
    fn a_scaled_meter_reads_as_a_percentage_until_the_amp_sleeps_or_warms_up() {
        let p = profile("aegir2");
        let meter = |others: &[Message]| {
            let mut readings = vec![Message::status(FunctionId::two(20, 1), Value::U16(173))];
            readings.extend_from_slice(others);
            meters(&p, &readings)["meter_1"].clone()
        };
        assert_eq!(meter(&[]), 50, "the switch has not reported");
        let power = |on| Message::status(FunctionId::one(10), Value::Bool(on));
        let mute = |on| Message::status(FunctionId::one(4), Value::Bool(on));
        assert_eq!(meter(&[power(true), mute(false)]), 50, "awake");
        assert_eq!(meter(&[power(false)]), 0, "in standby");
        assert_eq!(meter(&[power(true), mute(true)]), 0, "warming up");
    }

    /// A scaled meter whose reading is not an integer publishes `null`, not a made-up level.
    #[test]
    fn a_scaled_meter_with_a_reading_that_is_not_an_integer_is_null() {
        let p = profile("fake-amp");
        let readings = [Message::status(FunctionId::two(20, 1), Value::Bool(true))];
        assert_eq!(meters(&p, &readings)["meter_1"], Json::Null);
    }
}
