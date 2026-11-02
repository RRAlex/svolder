// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! An amp's meters and its power: an arch per meter on its own scale, the state line
//! that says why there are no readings, and the power button with the amp's switches and
//! readouts. Faults are not here: the card above the screen lists every active one.

use svolder_profiles::{ControlKind, Profile};

use super::controls::{self, ControlView};
use super::{Snapshot, knob_controls};

/// The fraction from which a bar is red: the meters' top two segments, and the strip's bars.
pub(crate) const RED_FROM: f64 = 0.93;

/// The meters.
#[derive(Debug)]
pub(crate) struct Meters {
    pub(crate) arches: Vec<Arch>,
    /// Why there are no readings: standby, warming up, meters off, no link.
    pub(crate) state: Option<&'static str>,
}

impl Meters {
    /// The louder meter's fraction, while there is one.
    pub(crate) fn louder(&self) -> Option<f64> {
        self.arches
            .iter()
            .filter_map(|a| a.fraction)
            .max_by(f64::total_cmp)
    }
}

/// One meter's arch.
#[derive(Debug)]
pub(crate) struct Arch {
    pub(crate) key: String,
    /// The channel's name: `L`, `R`, or the meter's label.
    pub(crate) channel: String,
    /// Its reading on its scale, 0 to 1, while it reads.
    pub(crate) fraction: Option<f64>,
    /// The percentage the readout shows, `–` without a reading.
    pub(crate) text: String,
    /// The class-A block's share of the arch.
    pub(crate) class_a: f64,
}

/// The meters of `profile`, when it has any: one arch per meter control with a scale.
pub(crate) fn meters(s: &Snapshot, profile: &Profile) -> Option<Meters> {
    let read = |id| s.raw(id);
    let live = profile.meters_live(|id| {
        let control = profile.control_by_id(id)?;
        Some(control.reads_true(&s.message(id)?.value))
    });
    let arches: Vec<Arch> = profile
        .controls
        .iter()
        .filter(|c| c.kind == ControlKind::Meter)
        .filter_map(|c| {
            let placed = profile.placed(c, read)?;
            let fraction = s
                .raw(c.id)
                .filter(|_| live && s.info.connected)
                .map(|raw| placed.fraction(raw as f64));
            let class_a = placed.class_a();
            Some(Arch {
                key: c.key.clone(),
                channel: c.channel.clone().unwrap_or_else(|| c.label.clone()),
                text: fraction.map_or_else(|| "–".to_string(), |f| format!("{:.0}", f * 100.0)),
                fraction,
                class_a,
            })
        })
        .collect();
    if arches.is_empty() {
        return None;
    }
    let on = |key: &str| s.reads_on(profile, key);
    let state = match () {
        _ if !s.info.connected => Some("No readings without a link"),
        _ if on("power") == Some(false) => Some("Standby: no readings"),
        _ if on("warming_up") == Some(true) => Some("Warming up"),
        _ if on("power_meters") == Some(false) => Some("Power meters are off"),
        _ => None,
    };
    Some(Meters { arches, state })
}

/// The power part.
#[derive(Debug)]
pub(crate) struct PowerView {
    pub(crate) key: String,
    pub(crate) on: Option<bool>,
    /// What a press of the button posts, as JSON.
    pub(crate) json: String,
    /// `On`, `Standby`, `Warming up`, or `–` before a reading.
    pub(crate) word: &'static str,
    /// Whether the amp is warming up (the word in `--warn`).
    pub(crate) warming: bool,
    /// On an amp: its other switches.
    pub(crate) toggles: Vec<ControlView>,
    /// On an amp: its readouts.
    pub(crate) readouts: Vec<ControlView>,
}

/// The power button of a device with a `powers_output` toggle, and on an amp its switches and
/// readouts (the meters' own floors and the readouts the session answers left out).
pub(crate) fn power(s: &Snapshot, profile: &Profile, amp: bool) -> Option<PowerView> {
    let (_, mute) = knob_controls(profile);
    let control = profile
        .controls
        .iter()
        .find(|c| c.kind == ControlKind::Toggle && c.powers_output)?;
    let on = s.message(control.id).map(|m| control.reads_true(&m.value));
    let warming = on == Some(true) && s.reads_on(profile, "warming_up") == Some(true);
    let word = match (on, warming) {
        (None, _) => "–",
        (Some(false), _) => "Standby",
        (Some(true), true) => "Warming up",
        (Some(true), false) => "On",
    };
    let others = |kinds: &[ControlKind]| -> Vec<ControlView> {
        match amp {
            false => Vec::new(),
            true => profile
                .controls
                .iter()
                .filter(|c| c.id != control.id && Some(c.id) != mute.map(|m| m.id))
                .filter(|c| kinds.contains(&c.kind) && !c.auto_off && !profile.sets_a_zero(c))
                .map(|c| controls::view(s, profile, c))
                .collect(),
        }
    };
    Some(PowerView {
        key: control.key.clone(),
        on,
        json: controls::opposite(on),
        word,
        warming,
        toggles: others(&[ControlKind::Toggle]),
        readouts: others(&[ControlKind::Readout]),
    })
}
