// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The typed form of one command or status: a direction, a function address and a value.

use crate::error::ProtoError;
use crate::function::FunctionId;
use crate::header::{Direction, Header};
use crate::value::Value;

/// One command or status: a direction, a function address and a value.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Message {
    /// Who sent it: the host or the device.
    pub direction: Direction,
    /// Which function it addresses.
    pub id: FunctionId,
    /// The payload it carries.
    pub value: Value,
}

impl Message {
    /// Build a Command message, sent by the host.
    pub fn command(id: FunctionId, value: Value) -> Self {
        Self {
            direction: Direction::Command,
            id,
            value,
        }
    }

    /// Build a Status message, sent by the device.
    pub fn status(id: FunctionId, value: Value) -> Self {
        Self {
            direction: Direction::Status,
            id,
            value,
        }
    }

    /// The header byte this message would encode to.
    pub fn header(&self) -> Header {
        Header {
            direction: self.direction,
            value_type: self.value.value_type(),
            bool_value: self.value.bool_value(),
            id_len: self.id.depth() as u8,
        }
    }

    /// Header byte, function-id bytes, value bytes. No length prefix, no checksum.
    pub(crate) fn to_body(&self) -> Vec<u8> {
        let mut body = vec![self.header().encode()];
        body.extend_from_slice(self.id.as_slice());
        body.extend(self.value.to_bytes());
        body
    }

    /// Parse a body as found after a length prefix.
    pub fn from_body(body: &[u8]) -> Result<Self, ProtoError> {
        let (&first, rest) = body.split_first().ok_or(ProtoError::TooShort(body.len()))?;
        let header = Header::decode(first);
        let id_len = usize::from(header.id_len);
        if id_len == 0 {
            return Err(ProtoError::BadFunctionIdLen(0));
        }
        if rest.len() < id_len {
            return Err(ProtoError::TooShort(body.len()));
        }
        let (id_bytes, value_bytes) = rest.split_at(id_len);
        Ok(Message {
            direction: header.direction,
            id: FunctionId::new(id_bytes)?,
            value: Value::from_bytes(header.value_type, header.bool_value, value_bytes)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::ValueType;

    #[test]
    fn header_reflects_value_and_id_depth() {
        let m = Message::command(FunctionId::three(12, 1, 2), Value::U8(64));
        assert_eq!(
            m.header(),
            Header {
                direction: Direction::Command,
                value_type: ValueType::U8,
                bool_value: false,
                id_len: 3,
            }
        );
        let s = Message::status(FunctionId::one(5), Value::Bool(true));
        assert_eq!(s.header().encode(), 0xB1);
    }

    #[test]
    fn serialises_body() {
        let m = Message::command(FunctionId::one(2), Value::U8(3));
        assert_eq!(m.to_body(), vec![0x41, 0x02, 0x03]);

        let b = Message::command(FunctionId::one(4), Value::Bool(true));
        assert_eq!(b.to_body(), vec![0x31, 0x04]);

        let r = Message::command(FunctionId::one(1), Value::Data(vec![]));
        assert_eq!(r.to_body(), vec![0x01, 0x01]);
    }

    #[test]
    fn parses_bodies() {
        assert_eq!(
            Message::from_body(&[0xC1, 0x02, 0x03]).unwrap(),
            Message::status(FunctionId::one(2), Value::U8(3))
        );
        assert_eq!(
            Message::from_body(&[0xB1, 0x05]).unwrap(),
            Message::status(FunctionId::one(5), Value::Bool(true))
        );
        assert_eq!(
            Message::from_body(&[0xC7, 0xD2, 0x02, 0x01, 0x03, 0x00]).unwrap(),
            Message::status(FunctionId::three(210, 2, 1), Value::U16(3))
        );
        assert_eq!(
            Message::from_body(&[0x81, 0x09, 0xAA, 0xBB]).unwrap(),
            Message::status(FunctionId::one(9), Value::Data(vec![0xAA, 0xBB]))
        );
    }

    #[test]
    fn rejects_truncated_bodies() {
        assert_eq!(Message::from_body(&[]), Err(ProtoError::TooShort(0)));
        assert_eq!(
            Message::from_body(&[0x42, 0x07]),
            Err(ProtoError::TooShort(2))
        );
        assert_eq!(
            Message::from_body(&[0x40]),
            Err(ProtoError::BadFunctionIdLen(0))
        );
        assert_eq!(
            Message::from_body(&[0x45, 0x02, 0x03]),
            Err(ProtoError::BadValueSize {
                expected: 2,
                got: 1
            })
        );
    }

    #[test]
    fn round_trips() {
        let cases = [
            Message::command(FunctionId::one(1), Value::Data(vec![])),
            Message::command(FunctionId::two(4, 2), Value::Bool(false)),
            Message::status(FunctionId::three(12, 5, 3), Value::U8(127)),
            Message::status(FunctionId::two(20, 1), Value::U16(1180)),
            Message::command(FunctionId::one(250), Value::U32(0xAF70C907)),
            Message::status(FunctionId::one(9), Value::I64(-5)),
        ];
        for m in cases {
            assert_eq!(Message::from_body(&m.to_body()).unwrap(), m);
        }
    }
}
