// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Shared fixtures for the crate's own unit tests: the built-in profiles and what they hold.

use crate::{Control, EqSpec, Profile, Registry};

/// One built-in profile.
pub(crate) fn builtin(name: &str) -> Profile {
    Registry::builtin()
        .expect("the built-in profiles load")
        .get(name)
        .expect("a built-in profile")
        .clone()
}

/// The control `key` of the built-in profile `profile`.
pub(crate) fn control(profile: &str, key: &str) -> Control {
    builtin(profile)
        .control_by_key(key)
        .expect("a control")
        .clone()
}

/// The tone control of one of the built-in profiles.
pub(crate) fn tone(profile: &str) -> EqSpec {
    builtin(profile).eq.expect("a tone control")
}
