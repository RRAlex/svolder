// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared fixtures for the crate's own unit tests.

use svolder_proto::UnitId;

/// A hex string of pairs of digits into the bytes it spells.
pub(crate) fn hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

/// A unit id out of its sixteen hex digits.
pub(crate) fn unit(text: &str) -> UnitId {
    text.parse().expect("a valid unit id")
}
