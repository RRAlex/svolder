// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! End-to-end vectors from docs/protocol.md, exercised through the public API only.

use svolder_proto::{
    Direction, FunctionClass, FunctionId, ManufacturerData, Message, Passphrase, UnitId, Value,
    decode_notification, encode_command, function,
};

fn hex(s: &str) -> Vec<u8> {
    hex::decode(s).unwrap()
}

#[test]
fn worked_example_freya_select_input_three() {
    let m = Message::command(FunctionId::one(2), Value::U8(3));
    assert_eq!(
        encode_command(&m, Passphrase::NONE).unwrap(),
        hex("000000000300410203b6ff")
    );
}

#[test]
fn passphrase_field_changes_with_every_frame() {
    let p = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
    let a = encode_command(&Message::command(FunctionId::one(2), Value::U8(3)), p).unwrap();
    let b = encode_command(&Message::command(FunctionId::one(2), Value::U8(4)), p).unwrap();
    assert_ne!(&a[..4], &b[..4]);
    assert_eq!(&a[..4], &hex("39affa35")[..]);
}

#[test]
fn refresh_is_a_data_command_with_empty_payload() {
    let frame = encode_command(
        &Message::command(function::REFRESH, Value::Data(vec![])),
        Passphrase::NONE,
    )
    .unwrap();
    assert_eq!(frame, hex("0000000002000101fbff"));
}

#[test]
fn a_device_status_dump_decodes_in_order() {
    let msgs = decode_notification(&hex("0300c102030200b1057efe"))
        .unwrap()
        .messages;
    assert_eq!(msgs.len(), 2);
    assert!(msgs.iter().all(|m| m.direction == Direction::Status));
    assert_eq!(msgs[0].id.to_string(), "2");
    assert_eq!(msgs[0].value.as_u64(), Some(3));
    assert_eq!(msgs[1].id.to_string(), "5");
    assert_eq!(msgs[1].value, Value::Bool(true));
}

#[test]
fn advertisement_gives_everything_needed_to_connect() {
    let d = ManufacturerData::parse(&hex(
        "02000000000001020304050607088a53f037030001070000000000",
    ))
    .unwrap();
    assert_eq!(d.product_name(), Some("Gungnir 2"));
    assert_eq!(d.unit_id, "0102030405060708".parse::<UnitId>().unwrap());
    assert_eq!(d.firmware_build, 3);
    assert_eq!(d.passphrase().digits(), [1, 2, 3, 4]);
    assert!(!d.in_bootloader());
}

#[test]
fn firmware_update_family_is_unsendable_through_the_public_api() {
    for text in ["210", "210.1", "210.3.1", "210.3.2"] {
        let id: FunctionId = text.parse().unwrap();
        assert_eq!(id.class(), FunctionClass::Restricted);
        assert!(
            encode_command(&Message::command(id, Value::Data(vec![])), Passphrase::NONE).is_err(),
            "{text}"
        );
    }
    let query: FunctionId = "210.2.1".parse().unwrap();
    assert_eq!(query.class(), FunctionClass::Normal);
    assert_eq!(function::PASSPHRASE.class(), FunctionClass::Sensitive);
}
