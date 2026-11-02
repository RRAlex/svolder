// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The one's-complement checksum that trails every frame.

/// One's complement of the wrapping 16-bit sum of all bytes.
///
/// The device appends this, little-endian, after every notification, and expects
/// it after every command's length-prefixed body.
pub fn checksum(bytes: &[u8]) -> u16 {
    !bytes
        .iter()
        .fold(0u16, |acc, &b| acc.wrapping_add(u16::from(b)))
}

#[cfg(test)]
mod tests {
    use super::checksum;

    #[test]
    fn empty_input_is_all_ones() {
        assert_eq!(checksum(&[]), 0xFFFF);
    }

    #[test]
    fn single_byte() {
        assert_eq!(checksum(&[0xFF]), 0xFF00);
    }

    #[test]
    fn small_sum() {
        assert_eq!(checksum(&[1, 2, 3]), 0xFFF9);
    }

    #[test]
    fn freya_select_input_body() {
        // len=3
        assert_eq!(checksum(&[0x03, 0x00, 0x41, 0x02, 0x03]), 0xFFB6);
    }

    #[test]
    fn sum_wraps_at_sixteen_bits() {
        let bytes = [0xFF; 300];
        // 300 * 255 = 76500 = 0x12AD4; wrapping to 16 bits gives 0x2AD4
        assert_eq!(checksum(&bytes), !0x2AD4);
    }
}
