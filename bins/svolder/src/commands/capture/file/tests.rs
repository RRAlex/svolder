// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The file's text from data built by hand: no radio, no clock.

use svolder_ble::{FrameDirection, ManufacturerRecord};
use svolder_profiles::Registry;
use svolder_proto::{ManufacturerData, UnitId};

use super::*;

const UNIT: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

fn advert() -> Advertisement {
    let manufacturer = ManufacturerData {
        product_id: 2,
        incomplete_dfu: false,
        unit_id: UnitId::new(UNIT),
        encrypted_passphrase: 0x1234_5678,
        firmware_build: 3,
        module_hardware: 1,
        module_firmware: 2,
    };
    let mut bytes = manufacturer.to_bytes().to_vec();
    bytes[2] = 0xAB;
    Advertisement {
        manufacturer,
        rssi: Some(-61),
        record: Some(ManufacturerRecord {
            company_id: 0xFFFF,
            bytes,
        }),
    }
}

fn reading(id: &str, display: &str) -> ValueObject {
    ValueObject {
        heard_at_ms: None,
        id: id.to_string(),
        key: None,
        label: "?".to_string(),
        value_type: "u32".to_string(),
        raw: serde_json::json!(1234),
        display: display.to_string(),
        value: None,
        unit: None,
    }
}

fn frame(millis: u64, direction: FrameDirection, bytes: &[u8]) -> TappedFrame {
    TappedFrame {
        at: Duration::from_millis(millis),
        direction,
        bytes: bytes.to_vec(),
    }
}

fn whole() -> Capture {
    Capture {
        version: "svolder 9.9.9".to_string(),
        taken: "2026-10-23 12:00:00".to_string(),
        window: 30,
        ended_early: None,
        window_from: None,
        doctor: Some("[]".to_string()),
        adverts: Some(vec![advert()]),
        readings: Some(vec![reading("10", "109")]),
        profile: Some(Registry::generic(0)),
        frames: Some(vec![frame(
            1250,
            FrameDirection::Received,
            &[0x01, 0x02, 0x03],
        )]),
        ended: None,
    }
}

fn text_of(capture: &Capture) -> String {
    render(&mut Scrub::new(false), capture)
}

#[test]
fn the_four_sections_come_in_order_after_the_header() {
    let text = text_of(&whole());
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "svolder capture");
    assert_eq!(lines[1], "svolder 9.9.9");
    assert_eq!(lines[2], "taken 2026-10-23 12:00:00 UTC, frame window 30 s");
    let at = |header: &str| {
        text.find(header)
            .unwrap_or_else(|| panic!("{header} missing"))
    };
    assert!(at("== doctor ==") < at("== adverts =="));
    assert!(at("== adverts ==") < at("== inspect =="));
    assert!(at("== inspect ==") < at("== frames =="));
}

#[test]
fn an_advert_shows_its_record_in_hex_and_its_fields() {
    let text = render(&mut Scrub::new(true), &whole());
    assert!(text.contains("company 0xffff  "), "{text}");
    assert!(text.contains("0102030405060708"), "{text}");
    assert!(text.contains("-61"), "{text}");
    let hidden = text_of(&whole());
    assert!(hidden.contains("00000000000000"), "{hidden}");
    assert!(!hidden.contains("0102030405060708"), "{hidden}");
    assert!(hidden.contains("present"), "{hidden}");
    assert!(!hidden.contains("12345678"), "{hidden}");
}

#[test]
fn a_frame_that_does_not_decode_shows_its_hex_and_its_error() {
    let text = text_of(&whole());
    assert!(
        text.contains("+1.250 s  received  010203\n    does not decode: "),
        "{text}"
    );
}

#[test]
fn a_hidden_reading_shows_scrubbed_and_a_kept_one_its_value() {
    let mut capture = whole();
    capture.readings = Some(vec![reading("250", "1234"), reading("10", "109")]);
    let text = text_of(&capture);
    assert!(text.contains("Status 250 u32 (scrubbed)"), "{text}");
    assert_eq!(
        text.matches("1234").count(),
        1,
        "only the other reading: {text}"
    );
    assert!(text.contains("Status 10 u32 1234"), "{text}");
    let kept = render(&mut Scrub::new(true), &capture);
    assert!(kept.contains("Status 250 u32 1234"), "{kept}");
}

#[test]
fn the_early_end_and_the_ended_line_appear_only_when_they_should() {
    let plain = text_of(&whole());
    assert!(!plain.contains("ended early"), "{plain}");
    assert!(!plain.contains("capture ended:"), "{plain}");

    let mut early = whole();
    early.ended_early = Some(Duration::from_millis(4_500));
    let text = text_of(&early);
    assert!(
        text.lines()
            .nth(2)
            .is_some_and(|line| line.ends_with("frame window 30 s, ended early after 4.5 s")),
        "{text}"
    );
    assert!(!text.contains("capture ended:"), "{text}");

    let mut failed = whole();
    failed.ended = Some("nothing in range".to_string());
    let text = text_of(&failed);
    assert!(
        text.trim_end()
            .ends_with("[capture ended: nothing in range]"),
        "{text}"
    );
}

#[test]
fn a_section_the_run_did_not_reach_is_left_out() {
    let mut capture = whole();
    capture.readings = None;
    capture.frames = None;
    let text = text_of(&capture);
    assert!(text.contains("== adverts =="), "{text}");
    assert!(!text.contains("== inspect =="), "{text}");
    assert!(!text.contains("== frames =="), "{text}");
}

#[test]
fn a_sent_frame_has_its_passphrase_bytes_zeroed_and_decodes_with_the_profile() {
    let mut capture = whole();
    let sent = svolder_proto::encode_command(
        &svolder_proto::Message::command(
            svolder_proto::FunctionId::one(10),
            svolder_proto::Value::U8(5),
        ),
        svolder_proto::Passphrase::from_text("1234").expect("a code"),
    )
    .expect("a frame");
    capture.frames = Some(vec![frame(10, FrameDirection::Sent, &sent)]);
    let text = text_of(&capture);
    let line = text
        .lines()
        .find(|line| line.contains("  sent  "))
        .expect("a sent line");
    assert!(line.contains("  sent  00000000"), "{line}");
    assert!(
        text.contains("\n    "),
        "the decoded lines are indented: {text}"
    );
}

#[test]
fn a_scan_that_heard_nothing_says_so() {
    let mut capture = whole();
    capture.adverts = Some(Vec::new());
    let text = text_of(&capture);
    assert!(text.contains("== adverts ==\n(none heard)\n"), "{text}");
}

#[test]
fn a_scrubbed_sent_frame_says_whether_it_had_a_passphrase() {
    let sent = |code: &str| {
        svolder_proto::encode_command(
            &svolder_proto::Message::command(
                svolder_proto::FunctionId::one(10),
                svolder_proto::Value::U8(5),
            ),
            svolder_proto::Passphrase::from_text(code).expect("a code"),
        )
        .expect("a frame")
    };
    let with = sent("1234");
    let without = svolder_proto::encode_command(
        &svolder_proto::Message::command(
            svolder_proto::FunctionId::one(10),
            svolder_proto::Value::U8(5),
        ),
        svolder_proto::Passphrase::NONE,
    )
    .expect("a frame");
    let mut capture = whole();
    capture.frames = Some(vec![
        frame(1, FrameDirection::Sent, &with),
        frame(2, FrameDirection::Sent, &without),
    ]);
    let text = text_of(&capture);
    let fields: Vec<&str> = text
        .lines()
        .filter(|line| line.contains("command frame, passphrase field"))
        .collect();
    assert_eq!(
        fields,
        [
            "    command frame, passphrase field set (scrubbed)",
            "    command frame, passphrase field 0x00000000"
        ]
    );
    let kept = render(&mut Scrub::new(true), &capture);
    let real = format!(
        "passphrase field {:#010x}",
        u32::from_le_bytes(with[..4].try_into().expect("four bytes"))
    );
    assert!(kept.contains(&real), "{kept}");
    assert!(!kept.contains("(scrubbed)"), "{kept}");
}

#[test]
fn a_scrubbed_250_does_not_read_as_a_real_zero() {
    let secret = svolder_proto::Message::status(
        svolder_proto::FunctionId::one(250),
        svolder_proto::Value::U32(0xAF70_C907),
    );
    let other = svolder_proto::Message::status(
        svolder_proto::FunctionId::one(2),
        svolder_proto::Value::U8(3),
    );
    let heard = svolder_proto::encode_notification(&[other, secret]).expect("a frame");
    let sent = svolder_proto::encode_command(
        &svolder_proto::Message::command(
            svolder_proto::FunctionId::one(250),
            svolder_proto::Value::U32(0xAF70_C907),
        ),
        svolder_proto::Passphrase::NONE,
    )
    .expect("a frame");
    let mut capture = whole();
    capture.frames = Some(vec![
        frame(1, FrameDirection::Received, &heard),
        frame(2, FrameDirection::Sent, &sent),
    ]);
    let text = text_of(&capture);
    let secret_lines: Vec<&str> = text.lines().filter(|l| l.contains(" 250 ")).collect();
    assert_eq!(secret_lines.len(), 2, "{text}");
    assert!(
        secret_lines.iter().all(|l| l.ends_with(" (scrubbed)")),
        "{text}"
    );
    let plain: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("    ") && l.contains(" 2 u8 "))
        .collect();
    assert_eq!(plain.len(), 1, "{text}");
    assert!(!plain[0].contains("scrubbed"), "{text}");

    let kept = render(&mut Scrub::new(true), &capture);
    assert!(
        kept.lines()
            .filter(|l| l.contains(" 250 "))
            .all(|l| !l.contains("scrubbed")),
        "{kept}"
    );
}

#[test]
fn the_start_of_the_frame_window_is_marked_among_the_frames() {
    let mut capture = whole();
    capture.window_from = Some(Duration::from_millis(1_000));
    capture.frames = Some(vec![
        frame(500, FrameDirection::Received, &[1, 2, 3]),
        frame(1_250, FrameDirection::Received, &[4, 5, 6]),
    ]);
    let text = text_of(&capture);
    let lines: Vec<&str> = text.lines().filter(|l| !l.starts_with("    ")).collect();
    let marker = lines
        .iter()
        .position(|l| *l == "-- frame window from +1.000 s --")
        .unwrap_or_else(|| panic!("no marker: {text}"));
    assert!(lines[marker - 1].starts_with("+0.500 s"), "{text}");
    assert!(lines[marker + 1].starts_with("+1.250 s"), "{text}");

    capture.window_from = None;
    assert!(!text_of(&capture).contains("-- frame window"));

    capture.window_from = Some(Duration::from_millis(9_000));
    let late = text_of(&capture);
    assert!(
        late.trim_end()
            .ends_with("-- frame window from +9.000 s --"),
        "a window that heard nothing is marked after the last frame: {late}"
    );
}
