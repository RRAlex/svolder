// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! A best-effort note for a device whose configured passphrase does not match the one its
//! advertisement carries.
//!
//! The advertisement carries the device's current code, so what a device's entry asks for can be
//! checked against it without connecting. The check is only as good as the advertisement: one
//! BlueZ hands back from its cache can be old, so a note says the codes do not match and never
//! that the configured one is wrong. A note names no digits: the code the advertisement carries
//! stays on the daemon's side of the socket.

use serde::{Deserialize, Serialize};
use svolder_proto::{ManufacturerData, Passphrase};

/// How a device's configured passphrase disagrees with its advertisement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PassphraseNote {
    /// Four digits that differ from the code the advertisement carries.
    Differs,
    /// `none`, while the advertisement carries a code.
    NoneConfigured,
    /// Four digits, while the advertisement carries no code.
    NoCodeAdvertised,
    /// A note this client does not know, from a daemon newer than it: read as this, so the rest
    /// of the reply still decodes.
    #[serde(other)]
    Unknown,
}

impl PassphraseNote {
    /// The note as a predicate on the passphrase, for a pane that names the passphrase itself:
    /// `does not match the code the device advertises`.
    pub fn short(self) -> &'static str {
        match self {
            PassphraseNote::Differs => "does not match the code the device advertises",
            PassphraseNote::NoneConfigured => "is none, but the device advertises a code",
            PassphraseNote::NoCodeAdvertised => "is a code, but the device advertises none",
            PassphraseNote::Unknown => "may not match the code the device advertises",
        }
    }

    /// The note as one clause that names the passphrase, for the lines that follow a device's
    /// name.
    pub fn words(self) -> String {
        format!("the passphrase {}", self.short())
    }

    /// What would make the note go away, as a clause that follows `set`.
    pub fn fix(self) -> &'static str {
        match self {
            PassphraseNote::Differs | PassphraseNote::NoneConfigured | PassphraseNote::Unknown => {
                "`passphrase` in the config to the right code, or to `auto`"
            }
            PassphraseNote::NoCodeAdvertised => {
                "`passphrase` in the config to `none`, or to `auto`"
            }
        }
    }

    /// The note as one line that names `device` and the fix.
    pub fn line(self, device: &str) -> String {
        format!("{device}: {}; set {}", self.words(), self.fix())
    }
}

/// What a device's configured passphrase says against the advertisement the daemon or the
/// session holds for it, or `None` for no note.
///
/// `configured` is the entry's `passphrase`: `auto`, `none` or four digits. `auto` never earns a
/// note, since it follows the advertisement. An advertised value that does not decode as a code
/// gives no verdict either, and neither does a `configured` that is none of the three.
pub fn passphrase_verdict(configured: &str, advert: &ManufacturerData) -> Option<PassphraseNote> {
    if configured == "auto" {
        return None;
    }
    let advertised = advert.passphrase();
    let advertises_code = match advertised.is_none() {
        true => false,
        false if advertised.is_well_formed() => true,
        false => return None,
    };
    let configured = Passphrase::from_text(configured).ok()?;
    match (configured.is_none(), advertises_code) {
        (true, true) => Some(PassphraseNote::NoneConfigured),
        (true, false) => None,
        (false, false) => Some(PassphraseNote::NoCodeAdvertised),
        (false, true) => (configured != advertised).then_some(PassphraseNote::Differs),
    }
}

#[cfg(test)]
mod tests;
