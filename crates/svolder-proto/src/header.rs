// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The header byte carried by every message: direction, value type and function-id depth.

use std::fmt;
use std::str::FromStr;

use crate::error::ProtoError;

/// Who is speaking: the host (Command) or the device (Status).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Direction {
    /// Sent by the host to the device.
    Command,
    /// Sent by the device to the host.
    Status,
}

/// The value type carried by a message, as encoded in the header byte.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ValueType {
    /// Raw bytes, whatever remains after the function id.
    Data,
    /// A single bit, carried in the header itself.
    Bool,
    /// Unsigned 8-bit integer.
    U8,
    /// Unsigned 16-bit integer, little-endian.
    U16,
    /// Unsigned 32-bit integer, little-endian.
    U32,
    /// Unsigned 64-bit integer, little-endian.
    U64,
    /// Signed 8-bit integer.
    I8,
    /// Signed 16-bit integer, little-endian.
    I16,
    /// Signed 32-bit integer, little-endian.
    I32,
    /// Signed 64-bit integer, little-endian.
    I64,
}

impl ValueType {
    /// Payload size in bytes. `None` for variable-length `Data`; `Some(0)` for `Bool`,
    /// whose value lives in the header.
    pub fn size(self) -> Option<usize> {
        match self {
            ValueType::Data => None,
            ValueType::Bool => Some(0),
            ValueType::U8 | ValueType::I8 => Some(1),
            ValueType::U16 | ValueType::I16 => Some(2),
            ValueType::U32 | ValueType::I32 => Some(4),
            ValueType::U64 | ValueType::I64 => Some(8),
        }
    }

    fn from_bits(byte: u8) -> Self {
        match byte & 0x60 {
            0x20 => ValueType::Bool,
            0x40 => {
                let signed = byte & 0x10 != 0;
                match ((byte >> 2) & 0x03, signed) {
                    (0, false) => ValueType::U8,
                    (0, true) => ValueType::I8,
                    (1, false) => ValueType::U16,
                    (1, true) => ValueType::I16,
                    (2, false) => ValueType::U32,
                    (2, true) => ValueType::I32,
                    (_, false) => ValueType::U64,
                    (_, true) => ValueType::I64,
                }
            }
            _ => ValueType::Data,
        }
    }

    fn bits(self) -> u8 {
        match self {
            ValueType::Data => 0x00,
            ValueType::Bool => 0x20,
            ValueType::U8 => 0x40,
            ValueType::U16 => 0x44,
            ValueType::U32 => 0x48,
            ValueType::U64 => 0x4C,
            ValueType::I8 => 0x50,
            ValueType::I16 => 0x54,
            ValueType::I32 => 0x58,
            ValueType::I64 => 0x5C,
        }
    }
}

impl fmt::Display for ValueType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            ValueType::Data => "data",
            ValueType::Bool => "bool",
            ValueType::U8 => "u8",
            ValueType::U16 => "u16",
            ValueType::U32 => "u32",
            ValueType::U64 => "u64",
            ValueType::I8 => "i8",
            ValueType::I16 => "i16",
            ValueType::I32 => "i32",
            ValueType::I64 => "i64",
        })
    }
}

impl FromStr for ValueType {
    type Err = ProtoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "data" => Ok(ValueType::Data),
            "bool" => Ok(ValueType::Bool),
            "u8" => Ok(ValueType::U8),
            "u16" => Ok(ValueType::U16),
            "u32" => Ok(ValueType::U32),
            "u64" => Ok(ValueType::U64),
            "i8" => Ok(ValueType::I8),
            "i16" => Ok(ValueType::I16),
            "i32" => Ok(ValueType::I32),
            "i64" => Ok(ValueType::I64),
            _ => Err(ProtoError::BadValueTypeText(s.to_string())),
        }
    }
}

/// The first byte of every message.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Header {
    /// Who sent the message.
    pub direction: Direction,
    /// The type of the value that follows the function id.
    pub value_type: ValueType,
    /// Meaningful only when `value_type == ValueType::Bool`.
    pub bool_value: bool,
    /// Number of function-ID bytes that follow, 1 to 3.
    pub id_len: u8,
}

impl Header {
    /// Pack the header fields into their single wire byte.
    pub fn encode(self) -> u8 {
        let mut byte = self.value_type.bits();
        if self.direction == Direction::Status {
            byte |= 0x80;
        }
        if self.value_type == ValueType::Bool && self.bool_value {
            byte |= 0x10;
        }
        byte | self.id_len.min(3)
    }

    /// Never fails: unknown type bits decode as `Data`, so a message of a type this build does
    /// not know is kept, its payload whatever remains.
    pub fn decode(byte: u8) -> Self {
        let value_type = ValueType::from_bits(byte);
        Header {
            direction: if byte & 0x80 != 0 {
                Direction::Status
            } else {
                Direction::Command
            },
            value_type,
            bool_value: value_type == ValueType::Bool && byte & 0x10 != 0,
            id_len: byte & 0x03,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(direction: Direction, value_type: ValueType, bool_value: bool, id_len: u8) -> Header {
        Header {
            direction,
            value_type,
            bool_value,
            id_len,
        }
    }

    #[test]
    fn decodes_documented_examples() {
        assert_eq!(
            Header::decode(0x01),
            h(Direction::Command, ValueType::Data, false, 1)
        );
        assert_eq!(
            Header::decode(0x21),
            h(Direction::Command, ValueType::Bool, false, 1)
        );
        assert_eq!(
            Header::decode(0x31),
            h(Direction::Command, ValueType::Bool, true, 1)
        );
        assert_eq!(
            Header::decode(0x41),
            h(Direction::Command, ValueType::U8, false, 1)
        );
        assert_eq!(
            Header::decode(0x42),
            h(Direction::Command, ValueType::U8, false, 2)
        );
        assert_eq!(
            Header::decode(0x43),
            h(Direction::Command, ValueType::U8, false, 3)
        );
        assert_eq!(
            Header::decode(0x49),
            h(Direction::Command, ValueType::U32, false, 1)
        );
        assert_eq!(
            Header::decode(0xC1),
            h(Direction::Status, ValueType::U8, false, 1)
        );
        assert_eq!(
            Header::decode(0xC7),
            h(Direction::Status, ValueType::U16, false, 3)
        );
        assert_eq!(
            Header::decode(0xB1),
            h(Direction::Status, ValueType::Bool, true, 1)
        );
        assert_eq!(
            Header::decode(0x5D),
            h(Direction::Command, ValueType::I64, false, 1)
        );
    }

    #[test]
    fn unknown_type_bits_decode_as_data() {
        assert_eq!(Header::decode(0x61).value_type, ValueType::Data);
    }

    #[test]
    fn bool_value_bit_is_ignored_for_non_bool() {
        assert!(!Header::decode(0x51).bool_value);
        assert_eq!(Header::decode(0x51).value_type, ValueType::I8);
    }

    #[test]
    fn encodes_documented_examples() {
        assert_eq!(
            h(Direction::Command, ValueType::Data, false, 1).encode(),
            0x01
        );
        assert_eq!(
            h(Direction::Command, ValueType::Bool, true, 1).encode(),
            0x31
        );
        assert_eq!(
            h(Direction::Command, ValueType::U8, false, 1).encode(),
            0x41
        );
        assert_eq!(
            h(Direction::Command, ValueType::U32, false, 1).encode(),
            0x49
        );
        assert_eq!(
            h(Direction::Status, ValueType::U16, false, 3).encode(),
            0xC7
        );
    }

    #[test]
    fn bool_value_is_dropped_when_type_is_not_bool() {
        assert_eq!(h(Direction::Command, ValueType::U8, true, 1).encode(), 0x41);
    }

    #[test]
    fn id_len_is_clamped_to_three() {
        assert_eq!(
            h(Direction::Command, ValueType::Data, false, 7).encode() & 0x03,
            3
        );
    }

    #[test]
    fn round_trips_every_type_and_direction() {
        let types = [
            ValueType::Data,
            ValueType::Bool,
            ValueType::U8,
            ValueType::U16,
            ValueType::U32,
            ValueType::U64,
            ValueType::I8,
            ValueType::I16,
            ValueType::I32,
            ValueType::I64,
        ];
        for direction in [Direction::Command, Direction::Status] {
            for value_type in types {
                for id_len in 1..=3u8 {
                    let original = h(direction, value_type, value_type == ValueType::Bool, id_len);
                    assert_eq!(Header::decode(original.encode()), original, "{original:?}");
                }
            }
        }
    }

    #[test]
    fn value_type_round_trips_through_text() {
        let types = [
            ValueType::Data,
            ValueType::Bool,
            ValueType::U8,
            ValueType::U16,
            ValueType::U32,
            ValueType::U64,
            ValueType::I8,
            ValueType::I16,
            ValueType::I32,
            ValueType::I64,
        ];
        for value_type in types {
            let text = value_type.to_string();
            assert_eq!(text.parse::<ValueType>().unwrap(), value_type, "{text}");
        }
    }

    #[test]
    fn value_type_text_rejects_wrong_case_and_unknown_names() {
        for bad in ["U8", "byte"] {
            assert_eq!(
                bad.parse::<ValueType>(),
                Err(ProtoError::BadValueTypeText(bad.to_string())),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn sizes() {
        assert_eq!(ValueType::Data.size(), None);
        assert_eq!(ValueType::Bool.size(), Some(0));
        assert_eq!(ValueType::U8.size(), Some(1));
        assert_eq!(ValueType::I16.size(), Some(2));
        assert_eq!(ValueType::U32.size(), Some(4));
        assert_eq!(ValueType::I64.size(), Some(8));
    }
}
