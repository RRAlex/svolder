// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use svolder_proto::{Passphrase, Value, encode_command};

fn recall() -> Action {
    Action {
        id: FunctionId::two(7, 2),
        key: "recall".into(),
        label: "Recall preset".into(),
        slots: 3,
        overwrites: false,
        changes: Vec::new(),
    }
}

#[test]
fn an_actions_command_is_empty_data_on_id_plus_slot() {
    let message = recall().command(2).unwrap();
    assert_eq!(message.id, FunctionId::three(7, 2, 2));
    assert_eq!(message.value, Value::Data(Vec::new()));
    assert_eq!(message.direction, svolder_proto::Direction::Command);
    let frame = encode_command(&message, Passphrase::NONE).unwrap();
    // after the four passphrase bytes: length 4 (header and three ids), the header 0x03
    // (Command, Data, three ids), then the ids
    assert_eq!(&frame[4..7], [4, 0, 0x03]);
    assert_eq!(&frame[7..10], [7, 2, 2]);
}

#[test]
fn an_actions_command_refuses_slot_0_and_a_slot_past_slots() {
    let action = recall();
    for slot in [0, 4, 255] {
        assert!(
            matches!(action.command(slot), Err(ProfileError::BadSlot { .. })),
            "{slot}"
        );
    }
    for slot in [1, 2, 3] {
        assert!(action.command(slot).is_ok(), "{slot}");
    }
}

#[test]
fn an_actions_slots_are_worded_as_a_span_or_one_number() {
    assert_eq!(recall().slots_word(), "1-3");
    assert_eq!(
        Action {
            slots: 1,
            ..recall()
        }
        .slots_word(),
        "1"
    );
    assert_eq!(
        Action {
            slots: 255,
            ..recall()
        }
        .slots_word(),
        "1-255"
    );
}
