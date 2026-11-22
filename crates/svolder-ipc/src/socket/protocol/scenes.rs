// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Scenes as a family of requests: `Request::Scene { op }`, and the view of them every front end
//! reads: `Event::Scenes`.
//!
//! A scene is a named set of sound settings: the computer's and each configured device's. While
//! a daemon runs it keeps the scenes: a front end sends an op and reads the [`ScenesView`], and
//! judges nothing itself (which scene is on, which parts a scene has, what a name may be). A
//! daemon that has the family lists the feature
//! [`FEATURE_SCENES`](super::FEATURE_SCENES); a front end offers scenes only while its daemon
//! does.
//!
//! Each op is answered with [`Response::Scene`](super::Response::Scene): the scene's name as the
//! file now spells it, and, for a recall, how each part fared; a save lists each configured
//! device that is not connected as a part left out of the scene. A refusal is an error whose
//! [`Reason`](super::Reason) is `taken`, `bad_name`, `missing`, `read_only`, `file_busy`,
//! `invalid`, `stopping`, `internal`, `busy` (a device still reading or applying a change) or
//! `nothing_to_save` (no device is connected and the computer's controls are not there); a recall
//! that would play one preset on a device and on the computer at once is answered with
//! [`Response::Ask`](super::Response::Ask) until it is sent again with `confirm`.

use serde::{Deserialize, Serialize};

/// One write or recall of the scenes, as a `scene` request carries it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum SceneOp {
    /// Save the sound settings as they are now under `name`: the computer's and those of every
    /// connected device. A name the scenes already have is refused unless `force`, which saves
    /// over that scene. Refused in words while a device's settings are unread.
    Save {
        /// the scene's name
        name: String,
        /// save over the scene of that name
        #[serde(default)]
        force: bool,
    },
    /// Recall the scene: every part of a connected device and the computer's part, each by its
    /// own safe order. A device that is not connected keeps what it has, and the answer says so.
    Recall {
        /// the scene's name
        name: String,
        /// answer the daemon's question (the preset the scene leaves on a device and on the
        /// computer would play twice) with yes
        #[serde(default)]
        confirm: bool,
    },
    /// Rename a scene, leaving what it holds as it is.
    Rename {
        /// the scene's name now
        from: String,
        /// the name it takes
        to: String,
    },
    /// Delete a scene.
    Delete {
        /// the scene's name
        name: String,
    },
}

/// The scenes as the daemon holds them and which one is on: what `Event::Scenes` carries, and
/// the one source every front end lists, marks and offers from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScenesView {
    /// every scene, in the file's order
    pub scenes: Vec<SceneRow>,
    /// the scene that is on, by the daemon's one rule; absent while none is, and while it is
    /// not known (see `known`)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on: Option<String>,
    /// the scene last recalled by any front end, whether it is on or not any more, kept in the
    /// daemon's state home so it outlives a restart; absent when none has been, or when no
    /// scene has that name now
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recalled: Option<String>,
    /// whether `on` is settled: false while a device is still reading a setting that could
    /// change which scene is on, when `on` says nothing yet
    pub known: bool,
}

/// One scene in a [`ScenesView`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SceneRow {
    /// the scene's name
    pub name: String,
    /// what the scene holds, one row per part
    pub parts: Vec<PartRow>,
}

/// One part of a scene in a [`ScenesView`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartRow {
    /// who the part belongs to: `computer`, or the device's configured name now (the name it
    /// was saved under, for a device no longer configured)
    pub who: String,
    /// whether the part can be recalled now: the computer's controls are there, or the device
    /// is connected
    pub present: bool,
}

/// How one part of a recalled scene fared.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PartOutcome {
    /// who the part belongs to, as [`PartRow::who`] says it
    pub who: String,
    /// what became of the part
    pub status: PartStatus,
    /// the words for what was left out or refused (`Kara is not connected: its part was left
    /// as it is`, a setting the device no longer has); empty for a part that was set whole
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<String>,
}

/// What became of one part of a recalled scene.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartStatus {
    /// Every setting of the part was set.
    Done,
    /// Some settings were set and the notes name the ones that were left out.
    Partial,
    /// The part was left as it is: the device is not connected, or the part was refused
    /// whole before anything changed.
    Left,
    /// The part was started and failed; the notes say how.
    Failed,
    /// A status a later daemon gives that this build does not know.
    #[serde(other)]
    Unknown,
}
