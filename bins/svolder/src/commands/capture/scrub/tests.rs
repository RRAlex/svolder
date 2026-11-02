// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use svolder_proto::{
    Message, Passphrase, Value, decode_command, decode_notification, encode_command,
    encode_notification, function::PASSPHRASE_REJECTED,
};

use super::*;

fn unit(last: u8) -> UnitId {
    UnitId::new([1, 2, 3, 4, 5, 6, 7, last])
}

fn placeholder(n: u8) -> UnitId {
    UnitId::new([0, 0, 0, 0, 0, 0, 0, n])
}

/// A 27-byte manufacturer record of `unit`, its passphrase bytes `aa bb cc dd`.
fn record_of(unit: UnitId) -> ManufacturerRecord {
    let mut bytes = vec![0x11; 27];
    bytes[6..14].copy_from_slice(&unit.bytes());
    bytes[14..18].copy_from_slice(&[0xAA, 0xBB, 0xCC, 0xDD]);
    ManufacturerRecord {
        company_id: 0xFFFF,
        bytes,
    }
}

/// A sent frame never carries the passphrase field, and still decodes.
#[test]
fn a_sent_frame_loses_its_passphrase_field() {
    let message = Message::command(FunctionId::one(2), Value::U8(3));
    let passphrase = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let frame = encode_command(&message, passphrase).unwrap();
    assert_ne!(frame[..4], [0, 0, 0, 0], "the case needs a field to lose");

    let scrubbed = Scrub::new(false).frame(FrameDirection::Sent, &frame);

    assert_eq!(scrubbed[..4], [0, 0, 0, 0]);
    assert_eq!(scrubbed[4..], frame[4..]);
    let decoded = decode_command(&scrubbed).expect("the scrubbed frame still decodes");
    assert_eq!(decoded.body.messages, vec![message]);
}

/// The value of a function-250 message goes, in a command and in both statuses, and nothing
/// else in the frame changes.
#[test]
fn a_passphrase_message_loses_its_value_both_ways() {
    let scrub = Scrub::new(false);
    let secret = Message::command(FunctionId::one(250), Value::U32(0xAF70_C907));
    let frame = encode_command(&secret, Passphrase::NONE).unwrap();
    let scrubbed = scrub.frame(FrameDirection::Sent, &frame);
    let decoded = decode_command(&scrubbed).expect("the command still decodes");
    assert_eq!(
        decoded.body.messages,
        vec![Message::command(FunctionId::one(250), Value::U32(0))]
    );

    let heard = encode_notification(&[
        Message::status(FunctionId::one(2), Value::U8(3)),
        Message::status(FunctionId::one(250), Value::U32(0x1234_5678)),
        Message::status(PASSPHRASE_REJECTED, Value::U32(0x0BAD_F00D)),
        Message::status(FunctionId::one(5), Value::Bool(true)),
    ])
    .unwrap();
    let scrubbed = scrub.frame(FrameDirection::Received, &heard);
    let decoded = decode_notification(&scrubbed).expect("the notification still decodes");
    assert_eq!(
        decoded.messages,
        vec![
            Message::status(FunctionId::one(2), Value::U8(3)),
            Message::status(FunctionId::one(250), Value::U32(0)),
            Message::status(PASSPHRASE_REJECTED, Value::U32(0)),
            Message::status(FunctionId::one(5), Value::Bool(true)),
        ]
    );
    assert_eq!(decoded.dropped, vec![]);
    assert!(scrub.hides(FunctionId::one(250)));
    assert!(scrub.hides(PASSPHRASE_REJECTED));
    assert!(!scrub.hides(FunctionId::one(2)));
}

/// Units are numbered in the order they are shown, in records and in text, either case.
#[test]
fn units_become_placeholders_in_order() {
    let mut scrub = Scrub::new(false);
    assert_eq!(scrub.unit(unit(8)), placeholder(1));
    assert_eq!(scrub.unit(unit(9)), placeholder(2));
    assert_eq!(scrub.unit(unit(8)), placeholder(1));

    let text = scrub.text("a 0102030405060708 b 0102030405060709 c 0102030405060708 d");
    assert_eq!(
        text,
        "a 0000000000000001 b 0000000000000002 c 0000000000000001 d"
    );
    let mut lower = Scrub::new(false);
    lower.unit(unit(0xAB));
    assert_eq!(
        lower.text("0102030405060AB 01020304050607ab 01020304050607AB"),
        "0102030405060AB 0000000000000001 0000000000000001"
    );

    let scrubbed = scrub.record(&record_of(unit(9)));
    assert_eq!(scrubbed.company_id, 0xFFFF);
    assert_eq!(scrubbed.bytes[6..14], placeholder(2).bytes());
    assert_eq!(scrubbed.bytes[14..18], [0, 0, 0, 0]);
    let original = record_of(unit(9));
    assert_eq!(scrubbed.bytes[..6], original.bytes[..6]);
    assert_eq!(scrubbed.bytes[18..], original.bytes[18..]);
}

/// A neighbour's device heard in the scan is numbered after the owner's.
#[test]
fn a_neighbours_unit_is_scrubbed_after_the_owners() {
    let mut scrub = Scrub::new(false);
    scrub.unit(unit(1));
    let neighbour = scrub.record(&record_of(unit(7)));
    assert_eq!(neighbour.bytes[6..14], placeholder(2).bytes());
    assert_eq!(scrub.text("0102030405060707"), "0000000000000002");
}

/// The home directory and Bluetooth addresses leave the text.
#[test]
fn the_home_and_addresses_are_replaced() {
    let mut scrub = Scrub::new(false).with_home("/home/alex");
    let text = scrub.text(
        "config /home/alex/.config/svolder/config.toml; /home/alexander stays; \
         adapter AA:BB:CC:DD:EE:FF and aa:bb:cc:dd:ee:ff, then 11:22:33:44:55:66; \
         not 11:22:33:44:55:66:77",
    );
    assert_eq!(
        text,
        "config ~/.config/svolder/config.toml; /home/alexander stays; \
         adapter 00:00:00:00:00:01 and 00:00:00:00:00:01, then 00:00:00:00:00:02; \
         not 11:22:33:44:55:66:77"
    );
}

/// With the ids kept, nothing changes anywhere.
#[test]
fn keeping_changes_nothing() {
    let mut scrub = Scrub::new(true).with_home("/home/alex");
    assert!(scrub.keeps());
    assert_eq!(scrub.unit(unit(8)), unit(8));
    let record = record_of(unit(8));
    assert_eq!(scrub.record(&record), record);
    let frame = encode_command(
        &Message::command(FunctionId::one(250), Value::U32(7)),
        Passphrase::from_digits([1, 2, 3, 4]).unwrap(),
    )
    .unwrap();
    assert_eq!(scrub.frame(FrameDirection::Sent, &frame), frame);
    assert!(!scrub.hides(FunctionId::one(250)));
    let text = "0102030405060708 /home/alex AA:BB:CC:DD:EE:FF";
    assert_eq!(scrub.text(text), text);
}

/// A frame whose walk finds no message boundaries keeps its bytes: a sent one loses only its
/// field.
#[test]
fn a_frame_that_does_not_decode_keeps_its_bytes() {
    let scrub = Scrub::new(false);
    let junk = [
        0x39, 0xAF, 0xFA, 0x35, 0xFF, 0xFF, 0x41, 0x02, 0x03, 0x00, 0x00,
    ];
    assert!(decode_command(&junk).is_err());
    let sent = scrub.frame(FrameDirection::Sent, &junk);
    assert_eq!(sent[..4], [0, 0, 0, 0]);
    assert_eq!(sent[4..], junk[4..]);
    let received = scrub.frame(FrameDirection::Received, &junk);
    assert_eq!(received, junk);
    assert_eq!(scrub.frame(FrameDirection::Sent, &[1, 2]), [0, 0]);
}

/// A notification whose checksum is wrong still loses its passphrase value; its checksum no
/// longer carries the value's byte sum, and the frame still fails to decode. Without a 250 the
/// checksum is left as heard.
#[test]
fn a_frame_with_a_bad_checksum_still_loses_its_passphrase_value() {
    let mut heard = encode_notification(&[
        Message::status(FunctionId::one(2), Value::U8(3)),
        Message::status(FunctionId::one(250), Value::U32(0x1234_5678)),
    ])
    .unwrap();
    let end = heard.len() - 2;
    heard[end] ^= 0xFF;
    assert!(decode_notification(&heard).is_err());

    let scrubbed = Scrub::new(false).frame(FrameDirection::Received, &heard);

    let value = heard.len() - 2 - 4..heard.len() - 2;
    assert_eq!(scrubbed[value.clone()], [0, 0, 0, 0]);
    assert_eq!(scrubbed[..value.start], heard[..value.start]);
    assert_ne!(scrubbed[end..], heard[end..], "the heard sum is gone");
    assert!(decode_notification(&scrubbed).is_err(), "still a bad frame");

    let mut plain =
        encode_notification(&[Message::status(FunctionId::one(2), Value::U8(3))]).unwrap();
    let last = plain.len() - 1;
    plain[last] ^= 0xFF;
    assert_eq!(
        Scrub::new(false).frame(FrameDirection::Received, &plain),
        plain
    );
}

/// A notification cut short in the middle of a 250 loses the part of the value it has.
#[test]
fn a_cut_passphrase_notification_keeps_none_of_the_value() {
    let heard = encode_notification(&[
        Message::status(FunctionId::one(2), Value::U8(3)),
        Message::status(FunctionId::one(250), Value::U32(0x70AF_C907)),
    ])
    .unwrap();
    let value = [0x07, 0xC9, 0xAF, 0x70];
    for cut in 1..=4 {
        let short = &heard[..heard.len() - cut];
        let kept = if cut <= 2 { 4 } else { 6 - cut };
        let scrubbed = Scrub::new(false).frame(FrameDirection::Received, short);
        assert_eq!(scrubbed.len(), short.len());
        assert!(
            !scrubbed.windows(kept).any(|w| w == &value[..kept]),
            "cut by {cut}: {scrubbed:02x?}"
        );
        assert_eq!(scrubbed[..8], short[..8], "the message before is as heard");
        assert!(
            scrubbed[VALUE_AT..].iter().all(|byte| *byte == 0),
            "cut by {cut}: everything from the value on is zero: {scrubbed:02x?}"
        );
    }
}

/// Where the 250 message's value starts in the frame the cut tests build: after the first
/// message (5 bytes), the second's length (2) and its header and function id (2).
const VALUE_AT: usize = 9;

/// A frame cut short in a 250's header keeps that header and its function id as heard: the
/// would-be checksum bytes are not written over them.
#[test]
fn a_cut_passphrase_notification_keeps_its_header_and_function_id() {
    let heard = encode_notification(&[
        Message::status(FunctionId::one(2), Value::U8(3)),
        Message::status(FunctionId::one(250), Value::U32(0x70AF_C907)),
    ])
    .unwrap();
    for cut in 1..=heard.len() - VALUE_AT {
        let short = &heard[..heard.len() - cut];
        let scrubbed = Scrub::new(false).frame(FrameDirection::Received, short);
        let kept = VALUE_AT.min(short.len());
        assert_eq!(
            scrubbed[..kept],
            short[..kept],
            "cut by {cut}: {scrubbed:02x?}"
        );
    }
}

/// A 250 is scrubbed in every width, and after a message that does not parse.
#[test]
fn every_width_of_a_passphrase_value_goes_and_so_does_one_after_a_bad_message() {
    let scrub = Scrub::new(false);
    for (heard, zero) in [
        (Value::U8(9), Value::U8(0)),
        (Value::U16(0x1234), Value::U16(0)),
        (Value::U64(0x1234_5678_9ABC), Value::U64(0)),
        (Value::Data(vec![1, 2, 3]), Value::Data(vec![0, 0, 0])),
    ] {
        let frame = encode_notification(&[Message::status(FunctionId::one(250), heard)]).unwrap();
        let scrubbed = scrub.frame(FrameDirection::Received, &frame);
        let decoded = decode_notification(&scrubbed).expect("still decodes");
        assert_eq!(
            decoded.messages,
            vec![Message::status(FunctionId::one(250), zero)]
        );
    }

    // A u32 message with one value byte does not parse; the 250 behind it is still scrubbed.
    let mut payload = vec![0x03, 0x00, 0xC9, 0x05, 0xAA, 0x06, 0x00, 0xC9, 0xFA];
    payload.extend([0x07, 0xC9, 0xAF, 0x70]);
    payload.extend(checksum(&payload).to_le_bytes());
    let before = decode_notification(&payload).unwrap();
    assert_eq!(before.dropped.len(), 1);
    let scrubbed = scrub.frame(FrameDirection::Received, &payload);
    let after = decode_notification(&scrubbed).unwrap();
    assert_eq!(after.dropped.len(), 1);
    assert_eq!(
        after.messages,
        vec![Message::status(FunctionId::one(250), Value::U32(0))]
    );
}

/// A record too short to hold the unit id and passphrase loses what is there of them.
#[test]
fn a_short_record_loses_what_it_has_of_the_unit_and_passphrase() {
    let mut scrub = Scrub::new(false);
    let short = |len: usize| ManufacturerRecord {
        company_id: 1,
        bytes: vec![0x55; len],
    };
    assert_eq!(scrub.record(&short(5)).bytes, vec![0x55; 5]);
    assert_eq!(
        scrub.record(&short(10)).bytes,
        [vec![0x55; 6], vec![0; 4]].concat()
    );
    assert_eq!(
        scrub.record(&short(16)).bytes,
        [vec![0x55; 6], vec![0, 0, 0, 0, 0, 0, 0, 1, 0, 0]].concat()
    );
}

/// Full stops and colons next to the home and addresses.
#[test]
fn the_text_boundaries_hold() {
    let mut scrub = Scrub::new(false).with_home("/home/alex");
    assert_eq!(
        scrub.text("in /home/alex. And /home/alex.rc, /home/alex: AA:BB:CC:DD:EE:FF: down"),
        "in ~. And /home/alex.rc, ~: 00:00:00:00:00:01: down"
    );
    assert_eq!(
        scrub.text("11:22:33:44:55:66:77 AA:BB:CC:DD:EE:FF:"),
        "11:22:33:44:55:66:77 00:00:00:00:00:01:"
    );
}

/// A BlueZ object path names the device by its address with underscores; the address goes, and
/// is the same placeholder as the colon form of it.
#[test]
fn an_address_in_a_bluez_object_path_goes_too() {
    let mut scrub = Scrub::new(false);
    let text = scrub.text(
        "Error: /org/bluez/hci0/dev_AA_BB_CC_DD_EE_01 gone; AA:BB:CC:DD:EE:01 and dev_11_22_33_44_55_66_77",
    );
    assert_eq!(
        text,
        "Error: /org/bluez/hci0/dev_00_00_00_00_00_01 gone; 00:00:00:00:00:01 and dev_11_22_33_44_55_66_77"
    );
}

/// A unit id that turns up only in a line of words is learned from it, as a standalone word.
#[test]
fn a_unit_in_words_is_learned_by_its_standalone_word() {
    let mut scrub = Scrub::new(false);
    scrub.learn_units("no device 1111222233334444 seen; x5555666677778888 and 99998888777766665");
    assert_eq!(
        scrub.text("1111222233334444 x5555666677778888 99998888777766665"),
        "0000000000000001 x5555666677778888 99998888777766665"
    );
    let mut keeping = Scrub::new(true);
    keeping.learn_units("no device 1111222233334444 seen");
    assert_eq!(keeping.text("1111222233334444"), "1111222233334444");
}
