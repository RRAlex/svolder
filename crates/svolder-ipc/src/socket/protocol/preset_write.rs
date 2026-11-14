// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The user's preset library as a family of writes: `Request::PresetWrite { op }`.
//!
//! While a daemon runs it is the one writer of the `[[preset]]` blocks for the front ends that
//! send this request (the TUI's preset list): it judges the name with the core's one rule
//! ([`svolder_profiles::tone::preset::new_name`]), writes the file under its lock and reloads,
//! exactly as the web page's save, rename and delete do, so a taken name is refused in the same
//! words on every surface. A daemon that has the family lists the feature
//! [`FEATURE_PRESET_WRITE`](super::FEATURE_PRESET_WRITE).
//!
//! Each op is answered with [`Response::PresetWritten`](super::Response::PresetWritten), the
//! preset's name as the file now spells it, or with an error whose [`Reason`](super::Reason) is
//! `taken`, `bad_name`, `missing`, `read_only`, `file_busy`, `invalid` or `stopping`.

use serde::{Deserialize, Serialize};
use svolder_profiles::tone::preset::Preset;

/// One write of the preset library, as a `preset_write` request carries it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum PresetOp {
    /// Save `preset` under its own name. A name the user's library already has is refused
    /// unless `force`, which writes the preset over that one.
    Save {
        /// the preset (boxed: the op rides in every `Request`)
        preset: Box<Preset>,
        /// write over the user's preset of that name
        #[serde(default)]
        force: bool,
    },
    /// Write `preset` over the user's preset of its name, which must be there: a name the library
    /// does not have is refused as `missing`, and a curve read off a device never replaces an
    /// imported preset.
    Update {
        /// the preset (boxed: the op rides in every `Request`)
        preset: Box<Preset>,
    },
    /// Rename one of the user's presets, leaving its curve as it is. A built-in's name is
    /// refused on either side.
    Rename {
        /// the preset's name now
        from: String,
        /// the name it takes
        to: String,
    },
    /// Delete one of the user's presets. A built-in is refused.
    Delete {
        /// the preset's name
        name: String,
    },
}
