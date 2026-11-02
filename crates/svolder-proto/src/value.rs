// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Typed message payloads and their conversion to and from wire bytes.

use std::fmt;

use crate::error::ProtoError;
use crate::header::ValueType;

/// A message payload. Integers are little-endian on the wire; a `Bool` has no
/// payload bytes because its value travels in the header.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum Value {
    /// Raw bytes, whatever remains after the function id.
    Data(Vec<u8>),
    /// A single bit, carried in the header itself.
    Bool(bool),
    /// Unsigned 8-bit integer.
    U8(u8),
    /// Unsigned 16-bit integer.
    U16(u16),
    /// Unsigned 32-bit integer.
    U32(u32),
    /// Unsigned 64-bit integer.
    U64(u64),
    /// Signed 8-bit integer.
    I8(i8),
    /// Signed 16-bit integer.
    I16(i16),
    /// Signed 32-bit integer.
    I32(i32),
    /// Signed 64-bit integer.
    I64(i64),
}

impl Value {
    /// The [`ValueType`] this value would encode as.
    pub fn value_type(&self) -> ValueType {
        match self {
            Value::Data(_) => ValueType::Data,
            Value::Bool(_) => ValueType::Bool,
            Value::U8(_) => ValueType::U8,
            Value::U16(_) => ValueType::U16,
            Value::U32(_) => ValueType::U32,
            Value::U64(_) => ValueType::U64,
            Value::I8(_) => ValueType::I8,
            Value::I16(_) => ValueType::I16,
            Value::I32(_) => ValueType::I32,
            Value::I64(_) => ValueType::I64,
        }
    }

    /// The header's boolean bit: true only for `Value::Bool(true)`.
    pub fn bool_value(&self) -> bool {
        matches!(self, Value::Bool(true))
    }

    /// The payload bytes as carried on the wire, little-endian.
    pub fn to_bytes(&self) -> Vec<u8> {
        match self {
            Value::Data(d) => d.clone(),
            Value::Bool(_) => Vec::new(),
            Value::U8(v) => vec![*v],
            Value::U16(v) => v.to_le_bytes().to_vec(),
            Value::U32(v) => v.to_le_bytes().to_vec(),
            Value::U64(v) => v.to_le_bytes().to_vec(),
            Value::I8(v) => v.to_le_bytes().to_vec(),
            Value::I16(v) => v.to_le_bytes().to_vec(),
            Value::I32(v) => v.to_le_bytes().to_vec(),
            Value::I64(v) => v.to_le_bytes().to_vec(),
        }
    }

    /// Build a value from the header's type, its boolean bit, and the payload bytes.
    pub fn from_bytes(
        value_type: ValueType,
        bool_value: bool,
        bytes: &[u8],
    ) -> Result<Self, ProtoError> {
        if let Some(expected) = value_type.size()
            && bytes.len() != expected
        {
            return Err(ProtoError::BadValueSize {
                expected,
                got: bytes.len(),
            });
        }
        // Sizes are verified above, so each slice-to-array conversion below cannot fail.
        let arr2 = || -> [u8; 2] { [bytes[0], bytes[1]] };
        let arr4 = || -> [u8; 4] { [bytes[0], bytes[1], bytes[2], bytes[3]] };
        let arr8 = || -> [u8; 8] {
            [
                bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
            ]
        };
        Ok(match value_type {
            ValueType::Data => Value::Data(bytes.to_vec()),
            ValueType::Bool => Value::Bool(bool_value),
            ValueType::U8 => Value::U8(bytes[0]),
            ValueType::U16 => Value::U16(u16::from_le_bytes(arr2())),
            ValueType::U32 => Value::U32(u32::from_le_bytes(arr4())),
            ValueType::U64 => Value::U64(u64::from_le_bytes(arr8())),
            ValueType::I8 => Value::I8(bytes[0] as i8),
            ValueType::I16 => Value::I16(i16::from_le_bytes(arr2())),
            ValueType::I32 => Value::I32(i32::from_le_bytes(arr4())),
            ValueType::I64 => Value::I64(i64::from_le_bytes(arr8())),
        })
    }

    /// Build a value of `value_type` from the text a user typed.
    ///
    /// `bool` accepts `on`/`off`, `true`/`false` and `1`/`0`, case-insensitively; `data`
    /// accepts hex bytes separated by whitespace or `:`, and an empty text for an empty payload;
    /// every integer type parses as a decimal `i64` and is range-checked into the target width,
    /// except `u64`, parsed directly so values above `i64::MAX` still work.
    pub fn from_text(value_type: ValueType, text: &str) -> Result<Value, ProtoError> {
        match value_type {
            ValueType::Bool => match text.trim().to_ascii_lowercase().as_str() {
                "on" | "true" | "1" => Ok(Value::Bool(true)),
                "off" | "false" | "0" => Ok(Value::Bool(false)),
                _ => Err(bad_text(format!(
                    "{text:?} is not on/off, true/false, or 1/0"
                ))),
            },
            ValueType::Data => parse_hex(text).map(Value::Data),
            ValueType::U64 => text
                .trim()
                .parse::<u64>()
                .map(Value::U64)
                .map_err(|_| bad_text(format!("{text:?} is not a u64"))),
            ValueType::U8 => Ok(Value::U8(ranged(parse_i64(text)?, value_type)?)),
            ValueType::U16 => Ok(Value::U16(ranged(parse_i64(text)?, value_type)?)),
            ValueType::U32 => Ok(Value::U32(ranged(parse_i64(text)?, value_type)?)),
            ValueType::I8 => Ok(Value::I8(ranged(parse_i64(text)?, value_type)?)),
            ValueType::I16 => Ok(Value::I16(ranged(parse_i64(text)?, value_type)?)),
            ValueType::I32 => Ok(Value::I32(ranged(parse_i64(text)?, value_type)?)),
            ValueType::I64 => Ok(Value::I64(parse_i64(text)?)),
        }
    }

    /// Unsigned integers as `u64`; `None` for anything else.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::U8(v) => Some(u64::from(*v)),
            Value::U16(v) => Some(u64::from(*v)),
            Value::U32(v) => Some(u64::from(*v)),
            Value::U64(v) => Some(*v),
            _ => None,
        }
    }

    /// Any integer as `i64` when it fits; `None` for `Data`, `Bool`, and out-of-range `U64`.
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Value::I8(v) => Some(i64::from(*v)),
            Value::I16(v) => Some(i64::from(*v)),
            Value::I32(v) => Some(i64::from(*v)),
            Value::I64(v) => Some(*v),
            Value::U64(v) => i64::try_from(*v).ok(),
            other => other.as_u64().and_then(|u| i64::try_from(u).ok()),
        }
    }
}

/// The error a piece of text earns when it does not name a value of its type.
fn bad_text(message: String) -> ProtoError {
    ProtoError::BadValueText(message)
}

/// Parse `text` as a decimal `i64`.
fn parse_i64(text: &str) -> Result<i64, ProtoError> {
    text.trim()
        .parse()
        .map_err(|_| bad_text(format!("{text:?} is not an integer")))
}

/// Narrow an `i64` into a smaller integer type, naming the target type on failure.
fn ranged<T: TryFrom<i64>>(n: i64, value_type: ValueType) -> Result<T, ProtoError> {
    T::try_from(n).map_err(|_| bad_text(format!("{n} does not fit in a {value_type}")))
}

/// Parse hex bytes, with whitespace or `:` allowed between them.
fn parse_hex(text: &str) -> Result<Vec<u8>, ProtoError> {
    let clean: String = text
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ':')
        .collect();
    if !clean.len().is_multiple_of(2) || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(bad_text(format!("{text:?} is not hex")));
    }
    (0..clean.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&clean[i..i + 2], 16)
                .map_err(|_| bad_text(format!("{text:?} is not hex")))
        })
        .collect()
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Data(d) => {
                for (i, b) in d.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" ")?;
                    }
                    write!(f, "{b:02x}")?;
                }
                Ok(())
            }
            Value::Bool(b) => write!(f, "{b}"),
            Value::U8(v) => write!(f, "{v}"),
            Value::U16(v) => write!(f, "{v}"),
            Value::U32(v) => write!(f, "{v}"),
            Value::U64(v) => write!(f, "{v}"),
            Value::I8(v) => write!(f, "{v}"),
            Value::I16(v) => write!(f, "{v}"),
            Value::I32(v) => write!(f, "{v}"),
            Value::I64(v) => write!(f, "{v}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reports_type_and_bool_bit() {
        assert_eq!(Value::Bool(true).value_type(), ValueType::Bool);
        assert!(Value::Bool(true).bool_value());
        assert!(!Value::Bool(false).bool_value());
        assert!(!Value::U8(1).bool_value());
        assert_eq!(Value::I32(-1).value_type(), ValueType::I32);
        assert_eq!(Value::Data(vec![]).value_type(), ValueType::Data);
    }

    #[test]
    fn serialises_little_endian() {
        assert_eq!(Value::Bool(true).to_bytes(), Vec::<u8>::new());
        assert_eq!(Value::U8(3).to_bytes(), vec![3]);
        assert_eq!(Value::U16(0x0201).to_bytes(), vec![1, 2]);
        assert_eq!(Value::U32(0x04030201).to_bytes(), vec![1, 2, 3, 4]);
        assert_eq!(
            Value::U64(0x0807060504030201).to_bytes(),
            vec![1, 2, 3, 4, 5, 6, 7, 8]
        );
        assert_eq!(Value::I8(-1).to_bytes(), vec![0xFF]);
        assert_eq!(Value::I16(-2).to_bytes(), vec![0xFE, 0xFF]);
        assert_eq!(Value::Data(vec![9, 8]).to_bytes(), vec![9, 8]);
    }

    #[test]
    fn parses_every_type() {
        assert_eq!(
            Value::from_bytes(ValueType::Bool, true, &[]),
            Ok(Value::Bool(true))
        );
        assert_eq!(
            Value::from_bytes(ValueType::U8, false, &[3]),
            Ok(Value::U8(3))
        );
        assert_eq!(
            Value::from_bytes(ValueType::U16, false, &[1, 2]),
            Ok(Value::U16(0x0201))
        );
        assert_eq!(
            Value::from_bytes(ValueType::U32, false, &[1, 2, 3, 4]),
            Ok(Value::U32(0x04030201))
        );
        assert_eq!(
            Value::from_bytes(ValueType::U64, false, &[1, 2, 3, 4, 5, 6, 7, 8]),
            Ok(Value::U64(0x0807060504030201))
        );
        assert_eq!(
            Value::from_bytes(ValueType::I8, false, &[0xFF]),
            Ok(Value::I8(-1))
        );
        assert_eq!(
            Value::from_bytes(ValueType::I16, false, &[0xFE, 0xFF]),
            Ok(Value::I16(-2))
        );
        assert_eq!(
            Value::from_bytes(ValueType::I32, false, &[0xFF; 4]),
            Ok(Value::I32(-1))
        );
        assert_eq!(
            Value::from_bytes(ValueType::I64, false, &[0xFF; 8]),
            Ok(Value::I64(-1))
        );
        assert_eq!(
            Value::from_bytes(ValueType::Data, false, &[7, 7]),
            Ok(Value::Data(vec![7, 7]))
        );
        assert_eq!(
            Value::from_bytes(ValueType::Data, false, &[]),
            Ok(Value::Data(vec![]))
        );
    }

    #[test]
    fn rejects_wrong_payload_size() {
        assert_eq!(
            Value::from_bytes(ValueType::U8, false, &[]),
            Err(ProtoError::BadValueSize {
                expected: 1,
                got: 0
            })
        );
        assert_eq!(
            Value::from_bytes(ValueType::U16, false, &[1, 2, 3]),
            Err(ProtoError::BadValueSize {
                expected: 2,
                got: 3
            })
        );
        assert_eq!(
            Value::from_bytes(ValueType::Bool, true, &[1]),
            Err(ProtoError::BadValueSize {
                expected: 0,
                got: 1
            })
        );
    }

    #[test]
    fn round_trips_through_bytes() {
        let values = [
            Value::Data(vec![1, 2, 3]),
            Value::Bool(false),
            Value::U8(255),
            Value::U16(65535),
            Value::U32(1 << 31),
            Value::U64(u64::MAX),
            Value::I8(-128),
            Value::I16(-32768),
            Value::I32(i32::MIN),
            Value::I64(i64::MIN),
        ];
        for v in values {
            let back = Value::from_bytes(v.value_type(), v.bool_value(), &v.to_bytes()).unwrap();
            assert_eq!(back, v);
        }
    }

    #[test]
    fn integer_views() {
        assert_eq!(Value::U8(7).as_u64(), Some(7));
        assert_eq!(Value::U64(u64::MAX).as_u64(), Some(u64::MAX));
        assert_eq!(Value::I8(-3).as_u64(), None);
        assert_eq!(Value::Bool(true).as_u64(), None);
        assert_eq!(Value::I8(-3).as_i64(), Some(-3));
        assert_eq!(Value::U32(5).as_i64(), Some(5));
        assert_eq!(Value::U64(u64::MAX).as_i64(), None);
        assert_eq!(Value::Data(vec![]).as_i64(), None);
    }

    /// An empty text is an empty `data` payload: the shape of a command with nothing to carry.
    #[test]
    fn empty_text_is_an_empty_data_payload() {
        assert_eq!(
            Value::from_text(ValueType::Data, ""),
            Ok(Value::Data(Vec::new()))
        );
        assert_eq!(
            Value::from_text(ValueType::Data, " "),
            Ok(Value::Data(Vec::new()))
        );
        assert_eq!(
            Value::from_text(ValueType::Data, ":"),
            Ok(Value::Data(Vec::new()))
        );
        // Only `data` takes an empty text; a number has nothing to parse.
        assert!(Value::from_text(ValueType::U8, "").is_err());
    }

    #[test]
    fn parses_text_for_every_type() {
        assert_eq!(
            Value::from_text(ValueType::Data, "0a:ff 10"),
            Ok(Value::Data(vec![0x0a, 0xff, 0x10]))
        );
        for on in ["on", "TRUE", "1"] {
            assert_eq!(Value::from_text(ValueType::Bool, on), Ok(Value::Bool(true)));
        }
        for off in ["off", "False", "0"] {
            assert_eq!(
                Value::from_text(ValueType::Bool, off),
                Ok(Value::Bool(false))
            );
        }
        assert_eq!(Value::from_text(ValueType::U8, "255"), Ok(Value::U8(255)));
        assert_eq!(
            Value::from_text(ValueType::U16, "65535"),
            Ok(Value::U16(65535))
        );
        assert_eq!(
            Value::from_text(ValueType::U32, " 7 "),
            Ok(Value::U32(7)),
            "surrounding space is trimmed"
        );
        assert_eq!(Value::from_text(ValueType::I8, "-128"), Ok(Value::I8(-128)));
        assert_eq!(
            Value::from_text(ValueType::I16, "-32768"),
            Ok(Value::I16(-32768))
        );
        assert_eq!(Value::from_text(ValueType::I32, "-1"), Ok(Value::I32(-1)));
        assert_eq!(
            Value::from_text(ValueType::I64, "-9223372036854775808"),
            Ok(Value::I64(i64::MIN))
        );
    }

    #[test]
    fn rejects_text_outside_the_target_width() {
        assert!(matches!(
            Value::from_text(ValueType::U8, "300"),
            Err(ProtoError::BadValueText(_))
        ));
        assert!(matches!(
            Value::from_text(ValueType::I8, "-200"),
            Err(ProtoError::BadValueText(_))
        ));
        assert!(matches!(
            Value::from_text(ValueType::Data, "zz"),
            Err(ProtoError::BadValueText(_))
        ));
        assert!(matches!(
            Value::from_text(ValueType::Data, "abc"),
            Err(ProtoError::BadValueText(_)),
            // an odd digit count is not a whole number of bytes
        ));
        assert!(matches!(
            Value::from_text(ValueType::Bool, "maybe"),
            Err(ProtoError::BadValueText(_))
        ));
        assert!(matches!(
            Value::from_text(ValueType::U16, "twelve"),
            Err(ProtoError::BadValueText(_))
        ));
    }

    #[test]
    fn u64_takes_values_above_i64_max() {
        assert_eq!(
            Value::from_text(ValueType::U64, "18446744073709551615"),
            Ok(Value::U64(u64::MAX))
        );
    }

    #[test]
    fn displays_human_form() {
        assert_eq!(Value::Bool(true).to_string(), "true");
        assert_eq!(Value::U8(108).to_string(), "108");
        assert_eq!(Value::I16(-6).to_string(), "-6");
        assert_eq!(Value::Data(vec![0x0A, 0xFF]).to_string(), "0a ff");
        assert_eq!(Value::Data(vec![]).to_string(), "");
    }
}
