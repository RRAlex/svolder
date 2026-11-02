// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the daemon's unit tests share across modules: a built-in profile by name and a command
//! as the tests' stand-ins log it, and the one way a test writes a script it runs. The web UI's
//! own helpers are in `web::test_support`.

#[path = "../../../crates/svolder-host/tests/common/script.rs"]
mod script;

pub(crate) use script::install_script;
use svolder_profiles::{Profile, Registry};

use crate::device::Command;

/// The built-in profile called `name`.
pub(crate) fn profile_named(name: &str) -> Profile {
    Registry::builtin()
        .expect("the built-in profiles load")
        .get(name)
        .expect("a built-in")
        .clone()
}

/// A command as the stand-ins log it.
pub(crate) fn shown(command: &Command) -> String {
    match command {
        Command::Set { key, value, .. } => format!("set {key} {value}"),
        Command::EqSet {
            band, param, value, ..
        } => format!("eq {band} {param} {value}"),
        Command::EqMode { mode, .. } => format!("mode {mode}"),
        Command::Raw { id, value, .. } => format!("raw {id} {value}"),
        Command::Identify => "identify".to_string(),
        _ => "another command".to_string(),
    }
}
