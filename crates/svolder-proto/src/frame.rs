// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Framing: turning [`Message`]s into the bytes written to and read from the BLE
//! characteristics, on both the host side and the device side.

use crate::checksum::checksum;
use crate::error::ProtoError;
use crate::function::FunctionClass;
use crate::header::Direction;
use crate::message::Message;
use crate::passphrase::Passphrase;

/// Build the bytes to write to the TX characteristic for one command.
///
/// Refuses functions in the firmware-update family, so no caller can send them, and
/// bodies longer than the u16 length prefix can describe.
pub fn encode_command(message: &Message, passphrase: Passphrase) -> Result<Vec<u8>, ProtoError> {
    if message.id.class() == FunctionClass::Restricted {
        return Err(ProtoError::Restricted(message.id.to_string()));
    }
    let body = message.to_body();
    let len = u16::try_from(body.len()).map_err(|_| ProtoError::TooLong(body.len()))?;
    let mut framed = len.to_le_bytes().to_vec();
    framed.extend(body);
    let ck = checksum(&framed);
    framed.extend_from_slice(&ck.to_le_bytes());
    let passphrase_field = if passphrase.is_none() {
        0
    } else {
        passphrase.encrypt_for_frame(&framed)
    };
    let mut frame = passphrase_field.to_le_bytes().to_vec();
    frame.extend(framed);
    Ok(frame)
}

/// Build the bytes a device sends in one RX notification carrying these status messages.
///
/// Used by the simulator and by tests; every message must be a Status.
pub fn encode_notification(messages: &[Message]) -> Result<Vec<u8>, ProtoError> {
    let mut framed = Vec::new();
    for message in messages {
        if message.direction != Direction::Status {
            return Err(ProtoError::NotStatus(message.id.to_string()));
        }
        let body = message.to_body();
        let len = u16::try_from(body.len()).map_err(|_| ProtoError::TooLong(body.len()))?;
        framed.extend(len.to_le_bytes());
        framed.extend(body);
    }
    let ck = checksum(&framed);
    framed.extend_from_slice(&ck.to_le_bytes());
    Ok(framed)
}

/// A command frame as received by a device: the passphrase field and the messages after it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedCommand {
    /// The encrypted passphrase field exactly as sent; 0 means the host sent none.
    pub passphrase_field: u32,
    /// The messages carried after the passphrase field, decoded the same way as a notification.
    pub body: Notification,
}

/// Parse a frame written to the TX characteristic. Used by the simulator and by `svolder decode`.
pub fn decode_command(bytes: &[u8]) -> Result<DecodedCommand, ProtoError> {
    if bytes.len() < 6 {
        return Err(ProtoError::TooShort(bytes.len()));
    }
    let passphrase_field = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    let body = decode_notification(&bytes[4..])?;
    Ok(DecodedCommand {
        passphrase_field,
        body,
    })
}

/// The messages carried by one RX notification, plus the ones that did not parse.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Notification {
    /// Messages in wire order.
    pub messages: Vec<Message>,
    /// One entry per message that failed to parse, in wire order. A `BadLength` entry is
    /// always last because parsing stops there.
    pub dropped: Vec<ProtoError>,
}

/// Parse one notification from the RX characteristic.
///
/// Frame-level problems (too short, checksum mismatch) reject the whole notification.
/// Inside a valid frame, a message that fails to parse is recorded in `dropped` and
/// parsing continues with the next one; a declared length that runs past the available
/// bytes is recorded and parsing stops, keeping the messages already parsed: the leniency
/// `docs/protocol.md` section 3 describes.
pub fn decode_notification(bytes: &[u8]) -> Result<Notification, ProtoError> {
    if bytes.len() < 2 {
        return Err(ProtoError::TooShort(bytes.len()));
    }
    let (payload, tail) = bytes.split_at(bytes.len() - 2);
    let received = u16::from_le_bytes([tail[0], tail[1]]);
    let computed = checksum(payload);
    if computed != received {
        return Err(ProtoError::ChecksumMismatch { computed, received });
    }
    let mut notification = Notification::default();
    let mut rest = payload;
    // A trailing single byte before the checksum is ignored: a message needs at least its two
    // length bytes, so reading stops when fewer than two remain.
    while rest.len() >= 2 {
        let declared = usize::from(u16::from_le_bytes([rest[0], rest[1]]));
        let after_len = &rest[2..];
        if declared == 0 || declared > after_len.len() {
            notification.dropped.push(ProtoError::BadLength {
                declared,
                available: after_len.len(),
            });
            break;
        }
        let (body, remaining) = after_len.split_at(declared);
        match Message::from_body(body) {
            Ok(m) => notification.messages.push(m),
            Err(e) => notification.dropped.push(e),
        }
        rest = remaining;
    }
    Ok(notification)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::function::FunctionId;
    use crate::test_support::hex;
    use crate::value::Value;

    #[test]
    fn encodes_without_passphrase() {
        let m = Message::command(FunctionId::one(2), Value::U8(3));
        assert_eq!(
            encode_command(&m, Passphrase::NONE).unwrap(),
            hex("000000000300410203b6ff")
        );
    }

    #[test]
    fn encodes_with_passphrase_keyed_on_the_tail() {
        let m = Message::command(FunctionId::one(2), Value::U8(3));
        let p = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
        assert_eq!(
            encode_command(&m, p).unwrap(),
            hex("39affa350300410203b6ff")
        );
    }

    #[test]
    fn encodes_refresh_and_bool_commands() {
        let refresh = Message::command(FunctionId::one(1), Value::Data(vec![]));
        let frame = encode_command(&refresh, Passphrase::NONE).unwrap();
        assert_eq!(&frame[4..8], &hex("02000101")[..]);
        assert_eq!(
            u16::from_le_bytes([frame[8], frame[9]]),
            checksum(&frame[4..8])
        );

        let mute = Message::command(FunctionId::one(5), Value::Bool(true));
        let frame = encode_command(&mute, Passphrase::NONE).unwrap();
        assert_eq!(&frame[4..8], &hex("02003105")[..]);
    }

    #[test]
    fn refuses_firmware_update_functions() {
        for id in [
            FunctionId::one(210),
            FunctionId::two(210, 1),
            FunctionId::three(210, 3, 1),
        ] {
            let m = Message::command(id, Value::Bool(true));
            assert_eq!(
                encode_command(&m, Passphrase::NONE),
                Err(ProtoError::Restricted(id.to_string())),
                "{id}"
            );
        }
    }

    #[test]
    fn refuses_bodies_longer_than_the_length_prefix() {
        let m = Message::command(FunctionId::one(9), Value::Data(vec![0; 65_534]));
        assert_eq!(
            encode_command(&m, Passphrase::NONE),
            Err(ProtoError::TooLong(65_536))
        );
        let ok = Message::command(FunctionId::one(9), Value::Data(vec![0; 65_533]));
        assert!(encode_command(&ok, Passphrase::NONE).is_ok());
    }

    #[test]
    fn allows_firmware_version_query_and_passphrase_set() {
        let q = Message::command(FunctionId::three(210, 2, 1), Value::Data(vec![]));
        assert!(encode_command(&q, Passphrase::NONE).is_ok());
        let p = Message::command(FunctionId::one(250), Value::U32(0xAF70C907));
        assert!(encode_command(&p, Passphrase::NONE).is_ok());
    }

    #[test]
    fn decodes_two_packed_messages() {
        let n = decode_notification(&hex("0300c102030200b1057efe")).unwrap();
        assert_eq!(
            n.messages,
            vec![
                Message::status(FunctionId::one(2), Value::U8(3)),
                Message::status(FunctionId::one(5), Value::Bool(true)),
            ]
        );
        assert_eq!(n.dropped, vec![]);
    }

    #[test]
    fn decodes_firmware_version_status() {
        let n = decode_notification(&hex("0600c7d2020103005afe")).unwrap();
        assert_eq!(
            n.messages,
            vec![Message::status(FunctionId::three(210, 2, 1), Value::U16(3))]
        );
    }

    #[test]
    fn rejects_bad_checksum() {
        assert_eq!(
            decode_notification(&hex("0400c102037dfe")),
            Err(ProtoError::ChecksumMismatch {
                computed: checksum(&hex("0400c10203")),
                received: 0xFE7D
            })
        );
    }

    #[test]
    fn rejects_short_and_overlong_declared_lengths() {
        assert_eq!(decode_notification(&[0xFF]), Err(ProtoError::TooShort(1)));
        // declared length 9 but only 3 bytes follow before the checksum
        let payload = hex("0900c10203");
        let mut frame = payload.clone();
        frame.extend(checksum(&payload).to_le_bytes());
        assert_eq!(
            decode_notification(&frame),
            Ok(Notification {
                messages: vec![],
                dropped: vec![ProtoError::BadLength {
                    declared: 9,
                    available: 3
                }],
            })
        );
        // declared length 0
        let payload = hex("0000");
        let mut frame = payload.clone();
        frame.extend(checksum(&payload).to_le_bytes());
        assert_eq!(
            decode_notification(&frame),
            Ok(Notification {
                messages: vec![],
                dropped: vec![ProtoError::BadLength {
                    declared: 0,
                    available: 0
                }],
            })
        );
    }

    #[test]
    fn empty_payload_with_valid_checksum_yields_no_messages() {
        assert_eq!(
            decode_notification(&hex("ffff")),
            Ok(Notification::default())
        );
    }

    #[test]
    fn keeps_good_messages_around_a_bad_one() {
        // good U8 status, then a U16 header with only one payload byte, then a good Bool status
        let payload = hex("0300c102030300c502030200b105");
        let mut frame = payload.clone();
        frame.extend(checksum(&payload).to_le_bytes());
        let n = decode_notification(&frame).unwrap();
        assert_eq!(
            n.messages,
            vec![
                Message::status(FunctionId::one(2), Value::U8(3)),
                Message::status(FunctionId::one(5), Value::Bool(true)),
            ]
        );
        assert_eq!(
            n.dropped,
            vec![ProtoError::BadValueSize {
                expected: 2,
                got: 1
            }]
        );
    }

    #[test]
    fn stops_at_a_bad_length_but_keeps_earlier_messages() {
        let payload = hex("0300c102030900c1");
        let mut frame = payload.clone();
        frame.extend(checksum(&payload).to_le_bytes());
        let n = decode_notification(&frame).unwrap();
        assert_eq!(n.messages.len(), 1);
        assert_eq!(
            n.dropped,
            vec![ProtoError::BadLength {
                declared: 9,
                available: 1
            }]
        );
    }

    #[test]
    fn command_body_round_trips_through_notification_decoder() {
        let m = Message::command(FunctionId::three(12, 2, 1), Value::U8(63));
        let frame = encode_command(&m, Passphrase::NONE).unwrap();
        // Strip the passphrase prefix: what remains has the notification shape.
        assert_eq!(decode_notification(&frame[4..]).unwrap().messages, vec![m]);
    }

    #[test]
    fn command_round_trips_through_decode_command() {
        let m = Message::command(FunctionId::one(2), Value::U8(3));
        let p = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
        let frame = encode_command(&m, p).unwrap();
        let decoded = decode_command(&frame).unwrap();
        assert_eq!(decoded.passphrase_field, p.encrypt_for_frame(&frame[4..]));
        assert_eq!(decoded.body.messages, vec![m]);
    }

    #[test]
    fn encodes_two_status_messages_as_a_notification() {
        let messages = [
            Message::status(FunctionId::one(2), Value::U8(3)),
            Message::status(FunctionId::one(5), Value::Bool(true)),
        ];
        assert_eq!(
            encode_notification(&messages).unwrap(),
            hex("0300c102030200b1057efe")
        );
    }

    #[test]
    fn refuses_to_encode_a_command_as_a_notification() {
        let m = Message::command(FunctionId::one(2), Value::U8(3));
        assert_eq!(
            encode_notification(&[m]),
            Err(ProtoError::NotStatus("2".to_string()))
        );
    }

    #[test]
    fn decode_command_rejects_frames_shorter_than_six_bytes() {
        assert_eq!(decode_command(&[0; 5]), Err(ProtoError::TooShort(5)));
    }
}
