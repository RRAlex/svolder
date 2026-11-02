// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared fixtures for the crate's own unit tests.

/// A hex string of pairs of digits into the bytes it spells.
pub(crate) fn hex(s: &str) -> Vec<u8> {
    hex::decode(s).unwrap()
}
