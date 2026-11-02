// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Function addresses (`2`, `7.1`, `210.2.1`) and the safety class they belong to.

use std::fmt;
use std::str::FromStr;

use crate::error::ProtoError;

/// A function address: one to three bytes, written `2`, `7.1` or `210.2.1`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FunctionId {
    bytes: [u8; 3],
    depth: u8,
}

/// What sending a command to a function may do to the device.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FunctionClass {
    /// A setting. Safe to send.
    Normal,
    /// Changes the passphrase. Only the dedicated command may send it.
    Sensitive,
    /// Firmware update control or data. Never sent by this software.
    Restricted,
}

/// Ask the device to report every function it has.
pub const REFRESH: FunctionId = FunctionId::one(1);
/// Ask the device to identify itself visually.
pub const IDENTIFY: FunctionId = FunctionId::one(200);
/// Set the device passphrase (UInt32, encoded).
pub const PASSPHRASE: FunctionId = FunctionId::one(250);
/// First byte of the firmware-update family.
pub const FIRMWARE_FAMILY: u8 = 210;
/// Second byte of the read-only firmware version query, `210.2.<subsystem>`.
pub const FIRMWARE_VERSION_QUERY: u8 = 2;
/// First byte of fault reports, `230.1` over-current, `230.2` over-temperature.
pub const FAULT_FAMILY: u8 = 230;
/// The status a device answers a passphrase-changing command with when it refused it.
pub const PASSPHRASE_REJECTED: FunctionId = FunctionId::two(250, 255);

impl FunctionId {
    /// Build a function id from its one to three address bytes.
    pub fn new(ids: &[u8]) -> Result<Self, ProtoError> {
        if ids.is_empty() || ids.len() > 3 {
            return Err(ProtoError::BadFunctionIdLen(ids.len()));
        }
        let mut bytes = [0u8; 3];
        bytes[..ids.len()].copy_from_slice(ids);
        Ok(Self {
            bytes,
            depth: ids.len() as u8,
        })
    }

    /// A one-byte function address, `a`.
    pub const fn one(a: u8) -> Self {
        Self {
            bytes: [a, 0, 0],
            depth: 1,
        }
    }

    /// A two-byte function address, `a.b`.
    pub const fn two(a: u8, b: u8) -> Self {
        Self {
            bytes: [a, b, 0],
            depth: 2,
        }
    }

    /// A three-byte function address, `a.b.c`.
    pub const fn three(a: u8, b: u8, c: u8) -> Self {
        Self {
            bytes: [a, b, c],
            depth: 3,
        }
    }

    /// The address bytes, one to three of them.
    pub fn as_slice(&self) -> &[u8] {
        &self.bytes[..self.depth()]
    }

    /// Number of bytes in the address, 1 to 3.
    pub fn depth(&self) -> usize {
        usize::from(self.depth)
    }

    /// The first (and, for a depth-1 id, only) address byte.
    pub fn first(&self) -> u8 {
        self.bytes[0]
    }

    /// The second address byte, present at depth 2 and 3.
    pub fn second(&self) -> Option<u8> {
        (self.depth >= 2).then_some(self.bytes[1])
    }

    /// The third address byte, present at depth 3.
    pub fn third(&self) -> Option<u8> {
        (self.depth >= 3).then_some(self.bytes[2])
    }

    /// What sending a command to this function may do to the device.
    pub fn class(&self) -> FunctionClass {
        match self.first() {
            FIRMWARE_FAMILY if self.second() == Some(FIRMWARE_VERSION_QUERY) => {
                FunctionClass::Normal
            }
            FIRMWARE_FAMILY => FunctionClass::Restricted,
            x if x == PASSPHRASE.first() => FunctionClass::Sensitive,
            _ => FunctionClass::Normal,
        }
    }

    /// Why a command to this function is turned away, or `None` when one may be built.
    ///
    /// The one gate every front end and the daemon check before they build a frame, so all
    /// of them refuse the firmware-update family and the passphrase function in the same
    /// words: the passphrase is changed through the dedicated verb, never through `raw` or
    /// a profile key that happens to address function 250.
    pub fn refusal(&self) -> Option<ProtoError> {
        match self.class() {
            FunctionClass::Normal => None,
            FunctionClass::Restricted => Some(ProtoError::Restricted(self.to_string())),
            FunctionClass::Sensitive => Some(ProtoError::Sensitive(self.to_string())),
        }
    }
}

impl fmt::Display for FunctionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, b) in self.as_slice().iter().enumerate() {
            if i > 0 {
                f.write_str(".")?;
            }
            write!(f, "{b}")?;
        }
        Ok(())
    }
}

impl FromStr for FunctionId {
    type Err = ProtoError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let bad = || ProtoError::BadFunctionIdText(s.to_string());
        let parts: Vec<u8> = s
            .split('.')
            .map(|p| {
                if p.is_empty() || !p.bytes().all(|b| b.is_ascii_digit()) {
                    return Err(bad());
                }
                p.parse::<u8>().map_err(|_| bad())
            })
            .collect::<Result<_, _>>()?;
        FunctionId::new(&parts).map_err(|_| bad())
    }
}

/// A function id travels as its dotted text, `"210.2.1"`.
#[cfg(feature = "serde")]
impl serde::Serialize for FunctionId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for FunctionId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = FunctionId;

            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a dotted function id such as \"210.2.1\"")
            }

            fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<FunctionId, E> {
                text.parse().map_err(E::custom)
            }
        }

        deserializer.deserialize_str(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_from_slices_of_valid_length() {
        assert_eq!(FunctionId::new(&[2]).unwrap(), FunctionId::one(2));
        assert_eq!(FunctionId::new(&[7, 1]).unwrap(), FunctionId::two(7, 1));
        assert_eq!(
            FunctionId::new(&[210, 2, 1]).unwrap(),
            FunctionId::three(210, 2, 1)
        );
    }

    #[test]
    fn rejects_empty_and_too_long() {
        assert_eq!(FunctionId::new(&[]), Err(ProtoError::BadFunctionIdLen(0)));
        assert_eq!(
            FunctionId::new(&[1, 2, 3, 4]),
            Err(ProtoError::BadFunctionIdLen(4))
        );
    }

    #[test]
    fn exposes_parts() {
        let id = FunctionId::three(210, 2, 1);
        assert_eq!(id.depth(), 3);
        assert_eq!(id.as_slice(), &[210, 2, 1]);
        assert_eq!(id.first(), 210);
        assert_eq!(id.second(), Some(2));
        assert_eq!(id.third(), Some(1));
        assert_eq!(FunctionId::one(5).second(), None);
        assert_eq!(FunctionId::two(7, 1).third(), None);
    }

    #[test]
    fn displays_dotted() {
        assert_eq!(FunctionId::one(2).to_string(), "2");
        assert_eq!(FunctionId::two(7, 1).to_string(), "7.1");
        assert_eq!(FunctionId::three(210, 2, 1).to_string(), "210.2.1");
    }

    #[test]
    fn parses_dotted() {
        assert_eq!("2".parse::<FunctionId>().unwrap(), FunctionId::one(2));
        assert_eq!("7.1".parse::<FunctionId>().unwrap(), FunctionId::two(7, 1));
        assert_eq!(
            "210.2.1".parse::<FunctionId>().unwrap(),
            FunctionId::three(210, 2, 1)
        );
    }

    #[test]
    fn rejects_bad_text() {
        for bad in ["", "1.2.3.4", "x", "300", "1..2", "1.", "+2.1"] {
            assert_eq!(
                bad.parse::<FunctionId>(),
                Err(ProtoError::BadFunctionIdText(bad.to_string())),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn classifies_firmware_family_as_restricted_except_version_query() {
        assert_eq!(FunctionId::one(210).class(), FunctionClass::Restricted);
        assert_eq!(FunctionId::two(210, 1).class(), FunctionClass::Restricted);
        assert_eq!(
            FunctionId::three(210, 3, 1).class(),
            FunctionClass::Restricted
        );
        assert_eq!(FunctionId::two(210, 2).class(), FunctionClass::Normal);
        assert_eq!(FunctionId::three(210, 2, 1).class(), FunctionClass::Normal);
    }

    #[test]
    fn refuses_the_firmware_family_and_the_passphrase_function() {
        let restricted = FunctionId::two(210, 1).refusal().expect("210.1 is refused");
        assert_eq!(
            restricted,
            ProtoError::Restricted("210.1".to_string()),
            "{restricted}"
        );
        assert!(restricted.to_string().contains("firmware-update family"));
        let sensitive = PASSPHRASE.refusal().expect("250 is refused");
        assert_eq!(
            sensitive.to_string(),
            "function 250 changes the passphrase; use `svolder passphrase set`"
        );
        // Everything else may be built, the read-only firmware version query included.
        assert!(FunctionId::one(10).refusal().is_none());
        assert!(FunctionId::three(210, 2, 1).refusal().is_none());
        assert!(REFRESH.refusal().is_none());
        assert!(IDENTIFY.refusal().is_none());
    }

    #[test]
    fn classifies_passphrase_as_sensitive_and_others_as_normal() {
        assert_eq!(PASSPHRASE.class(), FunctionClass::Sensitive);
        assert_eq!(REFRESH.class(), FunctionClass::Normal);
        assert_eq!(IDENTIFY.class(), FunctionClass::Normal);
        assert_eq!(
            FunctionId::two(FAULT_FAMILY, 1).class(),
            FunctionClass::Normal
        );
    }
}
