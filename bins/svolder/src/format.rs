// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Rendering one reading: a status line, one JSON object, or the bare value `get` prints —
//! and the words a device's link is shown in wherever a front end lists devices.

use svolder_ipc::ValueObject;
use svolder_profiles::Control;
use svolder_profiles::units::{self, Amount};

/// Print one reading: a status line, or one JSON object with `--json`.
pub fn print_value(vo: &ValueObject, json: bool) {
    if json {
        match serde_json::to_string(vo) {
            Ok(text) => println!("{text}"),
            Err(e) => eprintln!("svolder: {e}"),
        }
        return;
    }
    println!("{}", line(vo));
}

/// A full status line: `Status 10 u8 109   Volume = 109 (-6.0 dB)`. A reading no control
/// claims has no name to print, so the line stops after its raw value.
pub(crate) fn line(vo: &ValueObject) -> String {
    let named = if vo.label != "?" {
        format!("   {} = {}", vo.label, vo.display)
    } else {
        String::new()
    };
    format!("Status {} {} {}{named}", vo.id, vo.value_type, raw_text(vo))
}

/// The raw value as the wire wrote it: a JSON string is the `data` type's hex bytes, which
/// belong on the line unquoted; numbers and booleans print as they are.
fn raw_text(vo: &ValueObject) -> String {
    match &vo.raw {
        serde_json::Value::String(hex) => hex.clone(),
        other => other.to_string(),
    }
}

/// The bare value `get` prints for a reading of `control`: a converted level's value to a tenth
/// with its unit (`-6.0 dB`, `-0.0` written `0.0`), and `-inf dB` at silence, the form a float
/// parser reads, whichever transport the reading came over; else the human display
/// (`3 (Input 3)`, `on`).
pub fn bare_value(control: &Control, vo: &ValueObject) -> String {
    let amount = vo.raw.as_i64().and_then(|raw| units::amount(control, raw));
    match (amount, &vo.unit) {
        (Some(Amount::Silence), Some(unit)) => format!("-inf {unit}"),
        (Some(amount @ Amount::Value(_)), Some(unit)) if control.convert.is_some() => {
            units::amount_text(amount, Some(unit))
        }
        _ => vo.display.clone(),
    }
}

/// A link that is down, in `down`'s words, and when the daemon tries it again while it is
/// waiting between attempts: `not connected, next attempt in 15 s`.
///
/// The daemon backs its reconnect off while a device stays away, so how long the wait has grown
/// to is the difference between a device that has just dropped and one that has been gone all
/// day. Every listing that shows a device's link says it in these words, whatever word it uses
/// for the link being down.
pub fn link_down(down: &str, retry_in_secs: Option<u64>) -> String {
    match retry_in_secs {
        Some(secs) => format!("{down}, next attempt in {secs} s"),
        None => down.to_string(),
    }
}

/// What the link's flags say in words: `passphrase refused` when the device has refused the
/// passphrase on the link the daemon holds, `readings stale` when the readings held are older
/// than the link's last drop, both joined by ` · ` when both are so, and `None` for neither.
/// Every listing that words a device's link puts these after the link's own state, as
/// `connected · passphrase refused` or `disconnected · readings stale`.
pub fn link_flags(stale: bool, refused: bool) -> Option<String> {
    let flags: Vec<&str> = [
        refused.then_some("passphrase refused"),
        stale.then_some("readings stale"),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!flags.is_empty()).then(|| flags.join(" \u{b7} "))
}

/// What is said of an identify request: that it went out, or that one went out within the last
/// two seconds and this one sent nothing. The CLI prints it and the TUI shows it on its status
/// line. It says `sent`, never that the unit blinked: nothing here can see the light.
pub fn identify_line(device: &str, sent: bool) -> String {
    let gap = svolder_ble::IDENTIFY_GAP.as_secs();
    match sent {
        true => format!("identify sent to {device}"),
        false => format!("identify just sent to {device}; wait {gap} s"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use svolder_profiles::{Registry, describe};
    use svolder_proto::{FunctionId, Message, Value};

    /// The gungnir profile every reading here is described through.
    fn gungnir() -> svolder_profiles::Profile {
        Registry::builtin()
            .expect("the built-in profiles load")
            .get("gungnir2-v2")
            .expect("gungnir2-v2 is built in")
            .clone()
    }

    /// One reading of the gungnir profile, as a backend hands it over.
    fn reading(id: FunctionId, value: Value) -> ValueObject {
        let m = Message::status(id, value);
        ValueObject::from_reading(&describe(&gungnir(), &m), &m)
    }

    #[test]
    fn a_named_reading_carries_its_label_and_display() {
        let vo = reading(FunctionId::one(10), Value::U8(109));
        assert_eq!(line(&vo), "Status 10 u8 109   Volume = 109 (-6.0 dB)");
        assert_eq!(
            bare_value(gungnir().control_by_key("volume").unwrap(), &vo),
            "-6.0 dB"
        );
    }

    #[test]
    fn a_boolean_prints_as_the_wire_wrote_it() {
        let vo = reading(FunctionId::one(5), Value::Bool(true));
        assert_eq!(line(&vo), "Status 5 bool true   Mute = on");
        assert_eq!(
            bare_value(gungnir().control_by_key("mute").unwrap(), &vo),
            "on",
            "no unit, so the display stands"
        );
    }

    /// `get` prints silence as `-inf dB` whichever transport the reading came over: in-process it
    /// carries `-inf`, through the daemon's socket a JSON `null`.
    #[test]
    fn silence_reads_minus_inf_db_over_either_transport() {
        let profile = gungnir();
        let volume = profile.control_by_key("volume").expect("a volume");
        let direct = reading(FunctionId::one(10), Value::U8(0));
        assert_eq!(direct.value, Some(f64::NEG_INFINITY));
        let through_the_daemon = ValueObject {
            value: None,
            ..direct.clone()
        };
        assert_eq!(bare_value(volume, &direct), "-inf dB");
        assert_eq!(bare_value(volume, &through_the_daemon), "-inf dB");
    }

    /// A level just under zero reads `0.0 dB`, as `display` writes it, never `-0.0 dB`.
    #[test]
    fn a_value_that_rounds_to_minus_zero_reads_zero() {
        let mut volume = gungnir().control_by_key("volume").unwrap().clone();
        volume.convert = Some(svolder_profiles::Convert::Linear {
            min: -0.04,
            max: -0.04,
        });
        let vo = reading(FunctionId::one(10), Value::U8(127));
        assert_eq!(bare_value(&volume, &vo), "0.0 dB");
    }

    #[test]
    fn a_link_that_is_down_says_when_the_daemon_tries_it_again() {
        assert_eq!(
            link_down("not connected", Some(15)),
            "not connected, next attempt in 15 s"
        );
        assert_eq!(
            link_down("disconnected", Some(2)),
            "disconnected, next attempt in 2 s"
        );
        assert_eq!(link_down("disconnected", None), "disconnected");
    }

    /// The flags are worded exactly as the web page words them, with a middle dot.
    #[test]
    fn link_flags_are_worded_with_a_middle_dot() {
        assert_eq!(link_flags(false, false), None);
        assert_eq!(
            format!("connected \u{b7} {}", link_flags(false, true).unwrap()),
            "connected \u{b7} passphrase refused"
        );
        assert_eq!(
            format!("disconnected \u{b7} {}", link_flags(true, false).unwrap()),
            "disconnected \u{b7} readings stale"
        );
        assert_eq!(
            link_flags(true, true).as_deref(),
            Some("passphrase refused \u{b7} readings stale")
        );
    }

    #[test]
    fn hex_data_prints_without_its_json_quotes() {
        let vo = reading(FunctionId::one(7), Value::Data(vec![0xaa, 0xbb]));
        assert_eq!(line(&vo), "Status 7 data aa bb");
    }
}
