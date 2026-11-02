// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The capture file's text, built from what the run gathered.

use std::time::Duration;

use svolder_ble::{Advertisement, FrameDirection, TappedFrame};
use svolder_ipc::ValueObject;
use svolder_profiles::{Profile, Registry};
use svolder_proto::FunctionId;

use super::scrub::Scrub;
use crate::commands::decode::{describe_frame, describe_sent_frame};
use crate::commands::device::value_lines;

/// Everything a capture gathered, section by section; a section the run did not reach is `None`.
pub(super) struct Capture {
    /// `svolder` and its version.
    pub version: String,
    /// When the capture started, as UTC text.
    pub taken: String,
    /// The length of the frame window that was asked for, in seconds.
    pub window: u32,
    /// How long the window really lasted, when it ended before its time.
    pub ended_early: Option<Duration>,
    /// When the window began, on the clock the frames' times are on, when it was reached.
    pub window_from: Option<Duration>,
    /// Doctor's outcomes, as the JSON it prints.
    pub doctor: Option<String>,
    /// Every advertisement heard in the scan, in the order heard.
    pub adverts: Option<Vec<Advertisement>>,
    /// Every reading the device held after the connect.
    pub readings: Option<Vec<ValueObject>>,
    /// The profile the device was read with.
    pub profile: Option<Profile>,
    /// Every frame of the session.
    pub frames: Option<Vec<TappedFrame>>,
    /// Why the capture stopped short of its end, when it did.
    pub ended: Option<String>,
}

/// The text of `capture`.
///
/// Bytes pass through `scrub` as they are written, and the whole text goes through it last, so
/// a unit id or a path a line mentions in words is taken out too.
pub(super) fn render(scrub: &mut Scrub, capture: &Capture) -> String {
    let mut out = String::new();
    header(&mut out, scrub, capture);
    if let Some(doctor) = &capture.doctor {
        section(&mut out, "doctor");
        out.push_str(doctor);
        out.push('\n');
    }
    if let Some(adverts) = &capture.adverts {
        section(&mut out, "adverts");
        if adverts.is_empty() {
            out.push_str("(none heard)\n");
        }
        for advert in adverts {
            advert_lines(&mut out, scrub, advert);
        }
    }
    if let Some(readings) = &capture.readings {
        section(&mut out, "inspect");
        let shown: Vec<ValueObject> = readings
            .iter()
            .map(|reading| hide(scrub, reading))
            .collect();
        for line in value_lines(&shown) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    if let Some(frames) = &capture.frames {
        section(&mut out, "frames");
        let generic = Registry::generic(0);
        let profile = capture.profile.as_ref().unwrap_or(&generic);
        let mut marked = capture.window_from.is_none();
        for frame in frames {
            if !marked && capture.window_from.is_some_and(|from| frame.at >= from) {
                window_line(&mut out, capture.window_from);
                marked = true;
            }
            frame_lines(&mut out, scrub, profile, frame);
        }
        if !marked {
            window_line(&mut out, capture.window_from);
        }
    }
    if let Some(why) = &capture.ended {
        out.push_str(&format!("\n[capture ended: {why}]\n"));
        // A unit the message names, that nothing above showed the scrub, is hidden too.
        scrub.learn_units(why);
    }
    scrub.text(&out)
}

/// The lines before the first section.
fn header(out: &mut String, scrub: &Scrub, capture: &Capture) {
    out.push_str("svolder capture\n");
    out.push_str(&capture.version);
    out.push('\n');
    out.push_str(&format!(
        "taken {} UTC, frame window {} s",
        capture.taken, capture.window
    ));
    if let Some(after) = capture.ended_early {
        out.push_str(&format!(", ended early after {:.1} s", after.as_secs_f64()));
    }
    out.push('\n');
    out.push_str(match scrub.keeps() {
        false => "unit ids, passphrase bytes, the home directory and Bluetooth addresses are scrubbed (--keep-ids keeps them)\n",
        true => "nothing is scrubbed (--keep-ids): unit ids, passphrase bytes, the home directory and Bluetooth addresses are in this file\n",
    });
}

/// A section's heading, after a blank line.
fn section(out: &mut String, name: &str) {
    out.push_str(&format!("\n== {name} ==\n"));
}

/// The lowercase hex of `bytes`, contiguous, as `svolder decode` reads it.
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// One advertisement: its record in hex under its company id, then its parsed fields.
fn advert_lines(out: &mut String, scrub: &mut Scrub, advert: &Advertisement) {
    match &advert.record {
        Some(record) => {
            let shown = scrub.record(record);
            out.push_str(&format!(
                "company 0x{:04x}  {}\n",
                shown.company_id,
                hex(&shown.bytes)
            ));
        }
        None => out.push_str("company none  (no record kept)\n"),
    }
    let data = &advert.manufacturer;
    let product = data.product_name().unwrap_or("unknown product");
    out.push_str(&format!("  product {} {product}\n", data.product_id));
    out.push_str(&format!("  unit {}\n", scrub.unit(data.unit_id)));
    out.push_str(&match (scrub.keeps(), data.encrypted_passphrase) {
        (true, code) => format!("  passphrase {code:08x}\n"),
        (false, 0) => "  passphrase none\n".to_string(),
        (false, _) => "  passphrase present\n".to_string(),
    });
    out.push_str(&format!(
        "  firmware {}  module hardware {} firmware {}  incomplete dfu {}\n",
        data.firmware_build,
        data.module_hardware,
        data.module_firmware,
        if data.incomplete_dfu { "yes" } else { "no" }
    ));
    match advert.rssi {
        Some(rssi) => out.push_str(&format!("  rssi {rssi} dBm\n")),
        None => out.push_str("  rssi none\n"),
    }
}

/// `reading` with its value withheld when it is a function whose value is secret.
fn hide(scrub: &Scrub, reading: &ValueObject) -> ValueObject {
    let secret = reading
        .id
        .parse::<FunctionId>()
        .is_ok_and(|function| scrub.hides(function));
    if !secret {
        return reading.clone();
    }
    ValueObject {
        raw: serde_json::Value::String(HIDDEN.to_string()),
        display: HIDDEN.to_string(),
        value: None,
        unit: None,
        ..reading.clone()
    }
}

/// How a sent frame's first line begins.
const SENT_FIELD: &str = "command frame, passphrase field ";

/// What stands where a secret value was.
const HIDDEN: &str = "(scrubbed)";

/// The line that says the window of the owner's presses began here.
fn window_line(out: &mut String, from: Option<Duration>) {
    if let Some(from) = from {
        out.push_str(&format!(
            "-- frame window from +{}.{:03} s --\n",
            from.as_secs(),
            from.subsec_millis()
        ));
    }
}

/// Whether `line` is a decoded message of function 250 or one of its sub-functions.
fn is_passphrase_line(line: &str) -> bool {
    line.split_whitespace()
        .nth(1)
        .is_some_and(|id| id.split('.').next() == Some("250"))
}

/// One frame: when, which way and its bytes, then what `svolder decode` makes of them.
fn frame_lines(out: &mut String, scrub: &Scrub, profile: &Profile, frame: &TappedFrame) {
    let bytes = scrub.frame(frame.direction, &frame.bytes);
    let way = match frame.direction {
        FrameDirection::Sent => "sent",
        FrameDirection::Received => "received",
    };
    out.push_str(&format!(
        "+{}.{:03} s  {way}  {}\n",
        frame.at.as_secs(),
        frame.at.subsec_millis(),
        hex(&bytes)
    ));
    let lines = match frame.direction {
        FrameDirection::Sent => {
            let mut lines = describe_sent_frame(&bytes, profile);
            // The zeros a scrub leaves must not read as "no passphrase was sent".
            let had_one = frame.bytes.iter().take(4).any(|byte| *byte != 0);
            if !scrub.keeps()
                && had_one
                && let Some(first) = lines.first_mut()
                && first.starts_with(SENT_FIELD)
            {
                *first = "command frame, passphrase field set (scrubbed)".to_string();
            }
            lines
        }
        FrameDirection::Received => describe_frame(&bytes, profile),
    };
    for line in lines {
        // A zero where a passphrase value was must not read as a real zero.
        let mark = if !scrub.keeps() && is_passphrase_line(&line) {
            " (scrubbed)"
        } else {
            ""
        };
        out.push_str(&format!("    {line}{mark}\n"));
    }
}

#[cfg(test)]
mod tests;
