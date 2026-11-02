// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared fixtures for the crate's own unit tests.

use svolder_profiles::{Profile, Registry};

use crate::discovery::Device;

/// A built-in profile by name, owned so a test can hold it past the registry that loaded it.
pub(crate) fn profile(name: &str) -> Profile {
    Registry::builtin()
        .expect("the built-in profiles load")
        .get(name)
        .expect("profile exists")
        .clone()
}

/// The Living room DAC, a unit of `unit_id` on firmware 3.
pub(crate) fn device(unit_id: &str) -> Device<'_> {
    Device {
        name: "Living room DAC",
        unit_id,
        firmware: Some(3),
    }
}
