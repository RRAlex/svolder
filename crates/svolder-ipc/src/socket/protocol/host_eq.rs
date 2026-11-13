// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's EQ as a family of requests: `Request::HostEq { op }`.

use serde::{Deserialize, Serialize};

/// One change of the computer's EQ, as a `host_eq` request carries it. Point indexes count from
/// 1, as the bands of a tone control do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum HostEqOp {
    /// Turn the EQ on or off; off keeps the points
    Mode {
        /// whether the EQ is on
        on: bool,
    },
    /// Recall a preset by name: a saved one, or a built-in of the shown device
    Preset {
        /// the preset's name
        name: String,
        /// the shown device, whose own built-ins the name is looked up in
        #[serde(default, skip_serializing_if = "Option::is_none")]
        device: Option<String>,
        /// yes to the question a preset that a device plays too asks: that device is flattened
        /// first
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        confirm: bool,
    },
    /// Put a correction the caller read on the computer, under a name
    Apply {
        /// the name it is shown under
        name: String,
        /// where it came from, when it came from a catalogue
        #[serde(default, skip_serializing_if = "Option::is_none")]
        origin: Option<String>,
        /// the preamp and the filters
        correction: svolder_profiles::tone::apo::Correction,
        /// the sink to put it in front of, when not the default one
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sink: Option<String>,
    },
    /// Change one point; `done` is false while a drag goes on and true on its last value
    Set {
        /// the point, from 1; not looked at when `slot` is given
        index: u8,
        /// the slot (1 to 16) the sender holds for the point, as the host view lists it. When
        /// given, the daemon changes the point in that slot, whatever place the table lists it
        /// in now, and refuses the request when no point holds the slot; absent means the
        /// point is the one at `index`
        #[serde(default, skip_serializing_if = "Option::is_none")]
        slot: Option<u8>,
        /// `PK`, `LSC` or `HSC`
        #[serde(default, skip_serializing_if = "Option::is_none")]
        kind: Option<String>,
        /// frequency in Hz
        #[serde(default, skip_serializing_if = "Option::is_none")]
        f: Option<f64>,
        /// gain in dB
        #[serde(default, skip_serializing_if = "Option::is_none")]
        g: Option<f64>,
        /// Q
        #[serde(default, skip_serializing_if = "Option::is_none")]
        q: Option<f64>,
        /// whether this is the last value of the drag; absent means it is
        #[serde(default = "done_by_default")]
        done: bool,
    },
    /// Add a point
    Add {
        /// `PK`, `LSC` or `HSC`
        kind: String,
        /// frequency in Hz
        f: f64,
        /// gain in dB
        g: f64,
        /// Q
        q: f64,
    },
    /// Remove a point
    Remove {
        /// the point, from 1; not looked at when `slot` is given
        index: u8,
        /// the slot (1 to 16) the sender holds for the point; as for `Set`
        #[serde(default, skip_serializing_if = "Option::is_none")]
        slot: Option<u8>,
    },
    /// Save the points as a preset of the user's, under a name
    SaveAs {
        /// the new preset's name
        name: String,
    },
}

/// A `set` that does not say it goes on is the last of its drag.
fn done_by_default() -> bool {
    true
}
