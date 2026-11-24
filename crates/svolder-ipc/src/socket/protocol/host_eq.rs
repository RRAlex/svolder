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
    /// Change the crossfeed. Every field but `done` is optional and only the ones given change.
    /// Offer it only while the host's `EqView.crossfeed` is present: a daemon without it cannot
    /// parse the op. The rules (`CrossfeedSettings::change` in svolder-host holds them):
    ///
    /// - an op that names nothing at all is refused; an unknown `model` word or `preset` name is
    ///   refused in words (it is not held to a default), so is a number that is not finite and a
    ///   `delay_ms` for a model that plays none (Mid/Side bass);
    /// - `model` other than the current one loads that model's default preset; the same model
    ///   changes nothing, so a click on the selected model keeps Custom values;
    /// - then `preset` loads that preset's exact numbers (in the new model when both are sent);
    /// - then any `amount_pct`, `crossover_hz` or `delay_ms` makes the values Custom
    ///   (`EqView.crossfeed.preset` becomes none and `edited_from` names the preset); the daemon
    ///   rounds each to its model's step and holds it to the model's range, so a stepper that
    ///   sends 38.34 gets 38; presets stay exact;
    /// - `delay_ms` at or below the view's `delay_floor_ms` (0 included) asks for the model's own
    ///   delay (the view then says `delay_own`); anything above it is the total lag;
    /// - `on` is applied last, so turning on plays the numbers sent with it. A `Crossfeed` that
    ///   only sets `on` to its current value changes nothing.
    ///
    /// The ranges, presets, words and descriptions come from the view's `models`, never from
    /// the sender. A refusal leaves everything as it was.
    Crossfeed {
        /// turn the crossfeed on or off
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on: Option<bool>,
        /// the model's word: `bauer`, `mid-side`, `meier` or `linkwitz`
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        /// the preset's name within the model, as the view lists it
        #[serde(default, skip_serializing_if = "Option::is_none")]
        preset: Option<String>,
        /// the share of the other channel in percent
        #[serde(default, skip_serializing_if = "Option::is_none")]
        amount_pct: Option<f64>,
        /// the crossover in Hz
        #[serde(default, skip_serializing_if = "Option::is_none")]
        crossover_hz: Option<f64>,
        /// the total lag of the other side in ms, never below the model's own
        #[serde(default, skip_serializing_if = "Option::is_none")]
        delay_ms: Option<f64>,
        /// whether this is the last value of a drag (not a drag: the default); a drag is heard
        /// but not saved
        #[serde(default = "done_by_default")]
        done: bool,
    },
    /// Change the virtual speakers. Every field but `done` is optional and only the ones given
    /// change. Offer it only while the host's `EqView.speakers` is present: a daemon without it
    /// cannot parse the op. The rules:
    ///
    /// - an op that names nothing at all is refused;
    /// - `set` names a fetched set (the view's `sets` lists them by id): the choice goes to that
    ///   set, which rebuilds the chain once; an id that is not fetched is refused in words;
    /// - `on` is applied last, so turning on plays the set sent with it; turning it on while the
    ///   crossfeed plays turns the crossfeed off in the same update, and the other way round;
    /// - turning it on with no set chosen is refused in words.
    ///
    /// A refusal leaves everything as it was.
    Speakers {
        /// turn the virtual speakers on or off
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on: Option<bool>,
        /// the id of the fetched set to choose
        #[serde(default, skip_serializing_if = "Option::is_none")]
        set: Option<String>,
        /// whether this is the last value of a change (the default); kept for the shape the
        /// other ops share
        #[serde(default = "done_by_default")]
        done: bool,
    },
    /// Change the computer's loudness: the equal-loudness correction that follows the computer's
    /// volume. Every field but `done` is optional and only the ones given change. Offer it only
    /// while the host's `EqView.loudness` is present: a daemon without it cannot parse the op.
    /// The rules (`LoudnessSettings::change` in svolder-host holds them):
    ///
    /// - an op that names nothing at all is refused; a number that is not finite is refused in
    ///   words;
    /// - `strength_pct` is snapped to its step (5) and held to 0-100, `reference_pct` to its
    ///   step (1) and held to 1-100 (the view's `ranges` say so);
    /// - `reference_now` sets the reference to the computer's volume as last read (refused in
    ///   words while it has not been read) and is applied before `reference_pct`, which then
    ///   wins when both are sent;
    /// - `on` is applied last, so turning on plays the numbers sent with it. A `Loudness` that
    ///   only sets `on` to its current value changes nothing;
    /// - `done` false is a drag: the value is heard but not saved; the last value of the drag
    ///   (and any change that is not a drag) is saved.
    ///
    /// A refusal leaves everything as it was.
    Loudness {
        /// turn the loudness on or off
        #[serde(default, skip_serializing_if = "Option::is_none")]
        on: Option<bool>,
        /// how much of the correction is given back, in percent
        #[serde(default, skip_serializing_if = "Option::is_none")]
        strength_pct: Option<f64>,
        /// the volume at and above which the stage is flat, in percent of the volume scale
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reference_pct: Option<f64>,
        /// set the reference to the computer's volume now
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reference_now: Option<bool>,
        /// whether this is the last value of a drag (not a drag: the default)
        #[serde(default = "done_by_default")]
        done: bool,
    },
    /// Turn the computer's bypass on or off. On turns every stage off as one change and keeps
    /// which ones played; off brings back exactly those. On with nothing playing is refused in
    /// words, and a restore that cannot be made leaves the bypass on. Offer it only while the
    /// host's `EqView.bypass` is present: a daemon without it cannot parse the op.
    Bypass {
        /// whether the bypass is on
        on: bool,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_speakers_op_names_only_what_it_changes() {
        let op: HostEqOp = serde_json::from_str(r#"{"op":"speakers","on":true}"#).unwrap();
        assert_eq!(
            op,
            HostEqOp::Speakers {
                on: Some(true),
                set: None,
                done: true
            }
        );
        let op = HostEqOp::Speakers {
            on: None,
            set: Some("sadie-d2".into()),
            done: true,
        };
        assert_eq!(
            serde_json::to_string(&op).unwrap(),
            r#"{"op":"speakers","set":"sadie-d2","done":true}"#
        );
    }

    #[test]
    fn a_bypass_op_is_on_and_nothing_else() {
        let op: HostEqOp = serde_json::from_str(r#"{"op":"bypass","on":true}"#).unwrap();
        assert_eq!(op, HostEqOp::Bypass { on: true });
        assert_eq!(
            serde_json::to_string(&HostEqOp::Bypass { on: false }).unwrap(),
            r#"{"op":"bypass","on":false}"#
        );
    }

    #[test]
    fn the_other_ops_still_parse_as_they_did() {
        let op: HostEqOp = serde_json::from_str(r#"{"op":"crossfeed","on":false}"#).unwrap();
        assert!(matches!(
            op,
            HostEqOp::Crossfeed {
                on: Some(false),
                ..
            }
        ));
        let op: HostEqOp = serde_json::from_str(r#"{"op":"mode","on":true}"#).unwrap();
        assert_eq!(op, HostEqOp::Mode { on: true });
    }

    #[test]
    fn a_loudness_op_names_only_what_it_changes_and_done_defaults_true() {
        let op: HostEqOp = serde_json::from_str(r#"{"op":"loudness","strength_pct":35}"#).unwrap();
        assert_eq!(
            op,
            HostEqOp::Loudness {
                on: None,
                strength_pct: Some(35.0),
                reference_pct: None,
                reference_now: None,
                done: true
            }
        );
        let op = HostEqOp::Loudness {
            on: Some(true),
            strength_pct: None,
            reference_pct: None,
            reference_now: Some(true),
            done: false,
        };
        let text = serde_json::to_string(&op).unwrap();
        assert_eq!(
            text,
            r#"{"op":"loudness","on":true,"reference_now":true,"done":false}"#
        );
        assert_eq!(serde_json::from_str::<HostEqOp>(&text).unwrap(), op);
    }
}
