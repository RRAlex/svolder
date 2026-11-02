// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_proto::{FunctionId, Message, Passphrase, Value, encode_command, encode_notification};

use super::*;

fn generic() -> Profile {
    Registry::generic(0)
}

#[test]
fn describe_frame_matches_what_decode_prints() {
    let command = Message::command(FunctionId::one(9), Value::U8(3));
    let frame = encode_command(
        &command,
        Passphrase::from_digits([1, 2, 3, 4]).expect("four digits"),
    )
    .expect("a command frame");
    let lines = describe_frame(&frame, &generic());
    let (field, notification) = decode_any(&frame).expect("decodes");
    let built = message_lines(&notification, &generic());
    assert_eq!(
        lines,
        text_lines(field, &notification, &built, &dropped_errors(&notification))
    );
    assert!(
        lines[0].starts_with("command frame, passphrase field 0x"),
        "{lines:?}"
    );
    assert!(lines[1].starts_with("Command 9 "), "{lines:?}");

    let status = Message::status(FunctionId::one(9), Value::U8(3));
    let frame = encode_notification(&[status]).expect("a notification frame");
    let lines = describe_frame(&frame, &generic());
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("Status 9 "), "{lines:?}");
}

#[test]
fn a_frame_that_does_not_decode_says_why() {
    let status = Message::status(FunctionId::one(9), Value::U8(3));
    let mut frame = encode_notification(&[status]).expect("a notification frame");
    let last = frame.len() - 1;
    frame[last] ^= 0xFF;
    let lines = describe_frame(&frame, &generic());
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("does not decode: "), "{lines:?}");
}

#[test]
fn a_sent_frame_with_no_passphrase_is_a_command_frame() {
    let command = Message::command(FunctionId::one(9), Value::U8(3));
    let frame = encode_command(&command, Passphrase::NONE).expect("a command frame");
    assert_eq!(&frame[..4], &[0, 0, 0, 0]);
    let lines = describe_sent_frame(&frame, &generic());
    assert_eq!(lines.len(), 2, "{lines:?}");
    assert_eq!(lines[0], "command frame, passphrase field 0x00000000");
    assert!(lines[1].starts_with("Command 9 "), "{lines:?}");

    let lines = describe_sent_frame(&frame[..5], &generic());
    assert_eq!(lines.len(), 1);
    assert!(lines[0].starts_with("does not decode: "), "{lines:?}");
}

/// A command frame whose passphrase field is zero, as every scrubbed frame is, reads as the
/// command it is: its zero field is no notification length.
#[test]
fn a_command_frame_with_a_zero_field_decodes_as_a_command() {
    let command = Message::command(FunctionId::one(9), Value::U8(3));
    let frame = encode_command(&command, Passphrase::NONE).expect("a command frame");
    let (field, notification) = decode_any(&frame).expect("it decodes");
    assert_eq!(field, Some(0));
    assert_eq!(notification.messages, vec![command]);
    let lines = describe_frame(&frame, &generic());
    assert_eq!(lines[0], "command frame, passphrase field 0x00000000");
    assert!(lines[1].starts_with("Command 9 "), "{lines:?}");
}
