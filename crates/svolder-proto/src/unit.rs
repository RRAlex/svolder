// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The 8-byte device identifier carried in every advertisement.

use std::fmt;
use std::str::FromStr;

use crate::error::ProtoError;

/// The 8-byte identifier a device advertises. Stable for the life of the unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnitId([u8; 8]);

impl UnitId {
    /// Wrap eight raw bytes, in advertisement order.
    pub const fn new(bytes: [u8; 8]) -> Self {
        Self(bytes)
    }

    /// Build from a slice, failing unless it is exactly eight bytes.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, ProtoError> {
        <[u8; 8]>::try_from(bytes)
            .map(Self)
            .map_err(|_| ProtoError::BadUnitId)
    }

    /// The eight raw bytes, in advertisement order.
    pub fn bytes(&self) -> [u8; 8] {
        self.0
    }

    /// The id read as a little-endian u64, the form used to derive the passphrase key.
    pub fn to_u64_le(&self) -> u64 {
        u64::from_le_bytes(self.0)
    }

    /// Build from the little-endian u64 form.
    pub fn from_u64_le(value: u64) -> Self {
        Self(value.to_le_bytes())
    }
}

impl fmt::Display for UnitId {
    /// Sixteen uppercase hex characters, byte order preserved.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for b in self.0 {
            write!(f, "{b:02X}")?;
        }
        Ok(())
    }
}

impl FromStr for UnitId {
    type Err = ProtoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.len() != 16 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(ProtoError::BadUnitId);
        }
        let mut bytes = [0u8; 8];
        for (i, chunk) in s.as_bytes().chunks(2).enumerate() {
            let text = std::str::from_utf8(chunk).map_err(|_| ProtoError::BadUnitId)?;
            bytes[i] = u8::from_str_radix(text, 16).map_err(|_| ProtoError::BadUnitId)?;
        }
        Ok(Self(bytes))
    }
}

/// A unit id travels as its sixteen uppercase hex characters.
#[cfg(feature = "serde")]
impl serde::Serialize for UnitId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for UnitId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = UnitId;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("sixteen hex characters")
            }

            fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<UnitId, E> {
                text.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: UnitId = UnitId::new([1, 2, 3, 4, 5, 6, 7, 8]);

    #[test]
    fn displays_uppercase_hex_in_byte_order() {
        assert_eq!(SAMPLE.to_string(), "0102030405060708");
        assert_eq!(
            UnitId::new([0xAB, 0, 0, 0, 0, 0, 0, 0xCD]).to_string(),
            "AB000000000000CD"
        );
    }

    #[test]
    fn parses_hex_case_insensitively() {
        assert_eq!("0102030405060708".parse::<UnitId>().unwrap(), SAMPLE);
        assert_eq!(
            "ab00000000000000".parse::<UnitId>().unwrap(),
            UnitId::new([0xAB, 0, 0, 0, 0, 0, 0, 0])
        );
    }

    #[test]
    fn rejects_bad_text() {
        for bad in [
            "",
            "0102",
            "010203040506070",
            "0102030405060708AA",
            "zz02030405060708",
            "+1+2+3+4+5+6+7+8",
        ] {
            assert_eq!(bad.parse::<UnitId>(), Err(ProtoError::BadUnitId), "{bad:?}");
        }
    }

    #[test]
    fn slices_must_be_eight_bytes() {
        assert_eq!(
            UnitId::from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]).unwrap(),
            SAMPLE
        );
        assert_eq!(UnitId::from_slice(&[1, 2, 3]), Err(ProtoError::BadUnitId));
    }

    #[test]
    fn u64_view_is_little_endian() {
        assert_eq!(SAMPLE.to_u64_le(), 0x0807060504030201);
        assert_eq!(UnitId::from_u64_le(0x0807060504030201), SAMPLE);
    }
}
