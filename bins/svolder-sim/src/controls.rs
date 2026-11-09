// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The readings a person can change on the running simulator: whether USB is connected, whether
//! S/PDIF is locked and the sample rate the device reads. They are set at the start by flags and
//! changed during a session by lines on stdin (`usb 1`, `spdif 0`, `rate 44100`), and each
//! change is pushed to the connected central as a Status, as the device does when the thing it
//! reads changes.

use svolder_fake::FakeDevice;

/// One change to a reading of the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Knob {
    /// The `usb_connected` readout.
    Usb(bool),
    /// The `spdif_locked` readout.
    SpdifLocked(bool),
    /// The `sample_rate` readout, in hertz.
    SampleRate(u32),
}

/// The words `usb`, `spdif` and `rate` take, for the usage line printed with an error.
pub(crate) const USAGE: &str = "usb on|off|1|0, spdif on|off|1|0, rate <hertz, such as 44100>";

/// An on/off word: `on`, `off`, `1`, `0`, `true` or `false`, in any case.
pub(crate) fn parse_switch(word: &str) -> Result<bool, String> {
    match word.trim().to_ascii_lowercase().as_str() {
        "on" | "1" | "true" => Ok(true),
        "off" | "0" | "false" => Ok(false),
        other => Err(format!("`{other}` is not on or off")),
    }
}

/// A sample rate: hertz (`44100`) or kilohertz with a `k` (`44.1k`, `44.1kHz`).
pub(crate) fn parse_rate(word: &str) -> Result<u32, String> {
    let typed = word.trim();
    let word = typed.to_ascii_lowercase();
    let word = word.strip_suffix("hz").unwrap_or(&word);
    let bad = || format!("`{typed}` is not a sample rate, such as 44100 or 44.1k");
    match word.strip_suffix('k') {
        Some(khz) => {
            let khz: f64 = khz.parse().map_err(|_| bad())?;
            Ok((khz * 1000.0).round() as u32)
        }
        None => word.parse().map_err(|_| bad()),
    }
}

/// One line of stdin as a change; `None` for a blank line or a comment (`#`).
pub(crate) fn parse_line(line: &str) -> Result<Option<Knob>, String> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return Ok(None);
    }
    let mut words = line.split_whitespace();
    let (command, value) = (words.next().unwrap_or_default(), words.next());
    let (Some(value), None) = (value, words.next()) else {
        return Err(format!("`{line}` needs one value; try {USAGE}"));
    };
    Ok(Some(match command.to_ascii_lowercase().as_str() {
        "usb" => Knob::Usb(parse_switch(value)?),
        "spdif" => Knob::SpdifLocked(parse_switch(value)?),
        "rate" => Knob::SampleRate(parse_rate(value)?),
        other => return Err(format!("no command `{other}`; try {USAGE}")),
    }))
}

/// Make the change on the device and give back the Status frame to push, `None` when the
/// reading already held the value.
pub(crate) fn apply(device: &mut FakeDevice, knob: Knob) -> Result<Option<Vec<u8>>, String> {
    match knob {
        Knob::Usb(on) => device.set_flag("usb_connected", on),
        Knob::SpdifLocked(on) => device.set_flag("spdif_locked", on),
        Knob::SampleRate(hz) => device.set_sample_rate(hz),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use svolder_proto::{FunctionId, Passphrase, Value, decode_notification};

    fn bifrost3() -> FakeDevice {
        let profile = svolder_profiles::Registry::builtin()
            .expect("the built-in profiles")
            .get("bifrost3")
            .expect("the Bifrost 3")
            .clone();
        FakeDevice::from_profile(
            &profile,
            "0102030405060708".parse().expect("a unit id"),
            3,
            Passphrase::NONE,
        )
    }

    #[test]
    fn a_line_is_a_command_and_a_value() {
        assert_eq!(parse_line("usb 1"), Ok(Some(Knob::Usb(true))));
        assert_eq!(parse_line("  USB off "), Ok(Some(Knob::Usb(false))));
        assert_eq!(parse_line("spdif 0"), Ok(Some(Knob::SpdifLocked(false))));
        assert_eq!(parse_line("rate 44100"), Ok(Some(Knob::SampleRate(44_100))));
        assert_eq!(parse_line("rate 44.1k"), Ok(Some(Knob::SampleRate(44_100))));
        assert_eq!(parse_line("rate 96kHz"), Ok(Some(Knob::SampleRate(96_000))));
        assert!(parse_rate("Fast").unwrap_err().contains("`Fast`"));
        assert_eq!(parse_line(""), Ok(None));
        assert_eq!(parse_line("# a comment"), Ok(None));
    }

    #[test]
    fn a_bad_line_says_what_is_wrong_in_one_line() {
        for line in ["usb", "usb maybe", "rate fast", "rate 1 2", "volume 3"] {
            let err = parse_line(line).expect_err(line);
            assert!(!err.contains('\n'), "{err}");
            assert!(err.contains('`'), "{err}");
        }
        assert!(parse_line("volume 3").unwrap_err().contains("usb on|off"));
    }

    #[test]
    fn the_switch_words() {
        for on in ["on", "1", "true", "ON"] {
            assert_eq!(parse_switch(on), Ok(true));
        }
        for off in ["off", "0", "false"] {
            assert_eq!(parse_switch(off), Ok(false));
        }
        assert!(parse_switch("2").is_err());
    }

    /// Each change reaches the device as the Status of its reading: the notification a connected
    /// central gets.
    #[test]
    fn a_change_gives_the_status_of_its_reading() {
        let mut device = bifrost3();
        for (knob, id, value) in [
            (Knob::Usb(true), 7, Value::Bool(true)),
            (Knob::SpdifLocked(true), 8, Value::U8(1)),
            (Knob::SampleRate(96_000), 6, Value::U8(4)),
        ] {
            let frame = apply(&mut device, knob)
                .expect("applies")
                .expect("a change");
            let n = decode_notification(&frame).expect("a notification");
            assert_eq!(n.messages.len(), 1);
            assert_eq!(n.messages[0].id, FunctionId::one(id));
            assert_eq!(n.messages[0].value, value);
        }
        assert_eq!(apply(&mut device, Knob::Usb(true)), Ok(None));
    }

    #[test]
    fn a_profile_without_the_reading_refuses() {
        let profile = svolder_profiles::Registry::builtin()
            .expect("the built-in profiles")
            .get("gungnir2-v2")
            .expect("a profile")
            .clone();
        let mut device = FakeDevice::from_profile(
            &profile,
            "0102030405060708".parse().expect("a unit id"),
            3,
            Passphrase::NONE,
        );
        let err = apply(&mut device, Knob::Usb(true)).unwrap_err();
        assert!(err.contains("usb_connected"), "{err}");
    }
}
