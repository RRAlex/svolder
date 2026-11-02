// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Device profiles for Schiit Forkbeard products.
//!
//! A profile names the functions a device exposes, gives each a stable key, a kind
//! (select, toggle, level, readout, meter, fault) and the conversions between raw wire
//! values and human units. Profiles are TOML files: built-in ones are embedded, user
//! files in `~/.config/svolder/profiles/` add or override.
//!
//! | Group | Modules |
//! |---|---|
//! | Conversions | [`convert`] (raw to units), [`units`] (what a person reads), [`db_tables`] (dB tables), [`codec`] (statuses to named readings, a user's words to commands), [`meter`] (a reading on its bar) |
//! | Histories | [`history`] (series and windows) |
//! | Device rules | [`roles`] (which control does what), [`safety`] (the output-safety rule), [`device_presets`] (a device's slots and built-in settings) |
//! | Profiles | [`model`] (resolved), [`registry`] (loading, matching), [`builtin`] (embedded), `schema` (the TOML shapes), [`error`] |
//! | Tone control | [`tone`] (presets, response curves, correction fitting, APO text) |

#![warn(missing_docs)]

pub mod builtin;
pub mod codec;
pub mod convert;
pub mod db_tables;
pub mod device_presets;
pub mod error;
pub mod history;
pub mod meter;
pub mod model;
pub mod registry;
pub mod roles;
pub mod safety;
pub(crate) mod schema;
pub mod tone;
pub mod units;

#[cfg(test)]
mod test_support;

pub use codec::{
    Reading, apply_input_labels, describe, eq_loudness, eq_mode, eq_set, intent, relative,
};
pub use convert::{EqMode, EqParam};
pub use error::ProfileError;
pub use meter::{LogScale, Placed, Scale, Zero};
pub use model::{
    Action, BuiltinSetting, BuiltinSettings, Control, ControlKind, Convert, EqBand, EqSpec, Family,
    FlagWords, Profile,
};
pub use registry::{RESERVED_KEYS, Registry};
