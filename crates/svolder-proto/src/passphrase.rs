// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The device passphrase: its digit encoding, and the encrypt/decrypt transform that
//! makes it recoverable from an advertisement or a frame.

use crate::error::ProtoError;
use crate::unit::UnitId;

/// A device passphrase in its encoded 32-bit form. Zero means "no passphrase".
///
/// The encoding is a fixed bit shuffle of four decimal digits; it is not a secret.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Passphrase(u32);

impl Passphrase {
    /// No passphrase set.
    pub const NONE: Passphrase = Passphrase(0);

    /// Encode four decimal digits (0-9 each) into a passphrase.
    pub fn from_digits(digits: [u8; 4]) -> Result<Self, ProtoError> {
        if digits.iter().any(|&d| d > 9) {
            return Err(ProtoError::BadPassphraseDigit);
        }
        let d = |i: usize| u32::from(digits[i] & 0x0F);
        let n = |i: usize| u32::from(!digits[i] & 0x0F);
        let v = d(0)
            | (d(3) << 4)
            | (d(1) << 8)
            | (d(2) << 12)
            | (n(2) << 16)
            | (n(1) << 20)
            | (n(3) << 24)
            | (n(0) << 28);
        Ok(Self(v.rotate_left(2)))
    }

    /// The passphrase a piece of text names: `none` for no passphrase, or exactly four
    /// decimal digits. Surrounding whitespace is ignored; anything else is refused.
    pub fn from_text(text: &str) -> Result<Self, ProtoError> {
        let text = text.trim();
        if text == "none" {
            return Ok(Passphrase::NONE);
        }
        let digits = text.as_bytes();
        if digits.len() != 4 || !digits.iter().all(u8::is_ascii_digit) {
            return Err(ProtoError::BadValueText(format!(
                "{text:?} is not a passphrase; use four digits or `none`"
            )));
        }
        Self::from_digits([
            digits[0] - b'0',
            digits[1] - b'0',
            digits[2] - b'0',
            digits[3] - b'0',
        ])
    }

    /// Wrap an already-encoded 32-bit value.
    pub fn from_encoded(encoded: u32) -> Self {
        Self(encoded)
    }

    /// The encoded 32-bit form.
    pub fn encoded(self) -> u32 {
        self.0
    }

    /// True for [`Passphrase::NONE`]: no passphrase set.
    pub fn is_none(self) -> bool {
        self.0 == 0
    }

    /// The four digits, in the order the user typed them.
    pub fn digits(self) -> [u8; 4] {
        let v = self.0.rotate_right(2);
        [
            (v & 0x0F) as u8,
            ((v >> 8) & 0x0F) as u8,
            ((v >> 12) & 0x0F) as u8,
            ((v >> 4) & 0x0F) as u8,
        ]
    }

    /// XOR with the key, swap the 16-bit halves, invert.
    pub fn encrypt(encoded: u32, key: u32) -> u32 {
        !(encoded ^ key).rotate_right(16)
    }

    /// Inverse of `encrypt`.
    pub fn decrypt(encrypted: u32, key: u32) -> u32 {
        (!encrypted).rotate_right(16) ^ key
    }

    /// Key for an outgoing frame: the last four bytes of the length-prefixed body plus
    /// checksum, read little-endian. A slice shorter than four bytes is zero-padded on
    /// the left.
    pub fn frame_key(body_with_checksum: &[u8]) -> u32 {
        let n = body_with_checksum.len();
        let tail = &body_with_checksum[n.saturating_sub(4)..];
        let mut key = [0u8; 4];
        key[4 - tail.len()..].copy_from_slice(tail);
        u32::from_le_bytes(key)
    }

    /// Encrypt this passphrase for one outgoing frame, keyed on that frame's own tail.
    pub fn encrypt_for_frame(self, body_with_checksum: &[u8]) -> u32 {
        Self::encrypt(self.0, Self::frame_key(body_with_checksum))
    }

    /// Key for the advertised passphrase field, derived from the unit id.
    pub fn advert_key(unit: UnitId) -> u32 {
        let u = unit.to_u64_le();
        let byte = |shift: u32| u32::from((u >> shift) as u8);
        (byte(32) << 16) | (byte(16) << 24) | (byte(0) << 8) | byte(56)
    }

    /// Recover the passphrase from the advertisement. Zero means none.
    pub fn from_advert(encrypted: u32, unit: UnitId) -> Self {
        if encrypted == 0 {
            Self::NONE
        } else {
            Self(Self::decrypt(encrypted, Self::advert_key(unit)))
        }
    }

    /// True when the encoded value is a valid four-digit encoding: every digit nibble is
    /// 0-9 and every complement nibble matches its digit's complement. `NONE` is not
    /// well formed.
    pub fn is_well_formed(self) -> bool {
        let v = self.0.rotate_right(2);
        let d0 = v & 0x0F;
        let d1 = (v >> 8) & 0x0F;
        let d2 = (v >> 12) & 0x0F;
        let d3 = (v >> 4) & 0x0F;
        if d0 > 9 || d1 > 9 || d2 > 9 || d3 > 9 {
            return false;
        }
        let n2 = (v >> 16) & 0x0F;
        let n1 = (v >> 20) & 0x0F;
        let n3 = (v >> 24) & 0x0F;
        let n0 = (v >> 28) & 0x0F;
        n2 == !d2 & 0x0F && n1 == !d1 & 0x0F && n3 == !d3 & 0x0F && n0 == !d0 & 0x0F
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNIT: UnitId = UnitId::new([1, 2, 3, 4, 5, 6, 7, 8]);

    #[test]
    fn encodes_and_decodes_digits() {
        let p = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
        assert_eq!(p.encoded(), 0xAF70C907);
        assert_eq!(p.digits(), [1, 2, 3, 4]);
        assert!(!p.is_none());
    }

    #[test]
    fn all_zero_digits_are_a_real_passphrase() {
        let p = Passphrase::from_digits([0, 0, 0, 0]).unwrap();
        assert_eq!(p.encoded(), 0xFFFC0003);
        assert!(!p.is_none());
        assert_eq!(p.digits(), [0, 0, 0, 0]);
    }

    #[test]
    fn every_digit_combination_round_trips() {
        for a in 0..10u8 {
            for b in 0..10u8 {
                for c in 0..10u8 {
                    for d in 0..10u8 {
                        let p = Passphrase::from_digits([a, b, c, d]).unwrap();
                        assert_eq!(p.digits(), [a, b, c, d]);
                        assert!(!p.is_none());
                        assert!(p.is_well_formed());
                    }
                }
            }
        }
    }

    #[test]
    fn well_formed_check_rejects_none_and_arbitrary_values() {
        assert!(!Passphrase::NONE.is_well_formed());
        assert!(!Passphrase::from_encoded(0x12345678).is_well_formed());
    }

    #[test]
    fn text_names_none_or_four_digits() {
        assert_eq!(Passphrase::from_text("none").unwrap(), Passphrase::NONE);
        assert_eq!(
            Passphrase::from_text("1234").unwrap().digits(),
            [1, 2, 3, 4]
        );
        assert_eq!(
            Passphrase::from_text(" 0000 ").unwrap().digits(),
            [0, 0, 0, 0],
            "surrounding space is not part of the passphrase"
        );
        for bad in ["", "123", "12345", "12a4", "auto", "None"] {
            assert!(
                matches!(Passphrase::from_text(bad), Err(ProtoError::BadValueText(_))),
                "{bad:?} is not a passphrase"
            );
        }
    }

    #[test]
    fn rejects_non_decimal_digits() {
        assert_eq!(
            Passphrase::from_digits([1, 2, 3, 10]),
            Err(ProtoError::BadPassphraseDigit)
        );
    }

    #[test]
    fn encrypt_and_decrypt_are_inverse() {
        let key = 0xFFB60302;
        let enc = Passphrase::encrypt(0xAF70C907, key);
        assert_eq!(enc, 0x35FAAF39);
        assert_eq!(Passphrase::decrypt(enc, key), 0xAF70C907);
        for (p, k) in [
            (0u32, 0u32),
            (1, u32::MAX),
            (0xDEADBEEF, 0x12345678),
            (u32::MAX, 0),
        ] {
            assert_eq!(Passphrase::decrypt(Passphrase::encrypt(p, k), k), p);
        }
    }

    #[test]
    fn frame_key_is_the_little_endian_tail() {
        let body = [0x03, 0x00, 0x41, 0x02, 0x03, 0xB6, 0xFF];
        assert_eq!(Passphrase::frame_key(&body), 0xFFB60302);
        let p = Passphrase::from_digits([1, 2, 3, 4]).unwrap();
        assert_eq!(p.encrypt_for_frame(&body), 0x35FAAF39);
    }

    #[test]
    fn advert_key_from_unit_id() {
        assert_eq!(Passphrase::advert_key(UNIT), 0x03050108);
    }

    #[test]
    fn recovers_passphrase_from_advert() {
        assert_eq!(
            Passphrase::from_advert(0x37F0538A, UNIT).digits(),
            [1, 2, 3, 4]
        );
        assert_eq!(Passphrase::from_advert(0, UNIT), Passphrase::NONE);
    }
}
