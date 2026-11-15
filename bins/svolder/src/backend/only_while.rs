// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The booleans a readout's `only_while` names, kept across a stream of single statuses.

use super::*;

/// What a stream of single statuses knows of the booleans a readout's `only_while` can name,
/// so each status can be shown with the derived value a full listing shows, and a readout can
/// be shown again when the control it depends on changes under it.
#[derive(Debug, Default)]
pub(crate) struct OnlyWhile {
    /// Whether the last reading of each function, by id, read true.
    truth: std::collections::BTreeMap<String, bool>,
    /// The last reading of each gated readout, by id, and the derived value shown for it.
    readouts: std::collections::BTreeMap<String, (ValueObject, bool)>,
}

impl OnlyWhile {
    /// Remember every reading in a listing, whose gated readouts are already derived.
    pub(crate) fn note_all(&mut self, profile: &Profile, values: &[ValueObject]) {
        for vo in values {
            self.truth.insert(vo.id.clone(), vo.reads_true());
        }
        for vo in values {
            if let Some(control) = gated(profile, vo) {
                let shown = units::shows_true(profile, control, vo.reads_true(), |gate| {
                    self.truth.get(&gate.id.to_string()).copied()
                });
                self.readouts.insert(vo.id.clone(), (vo.clone(), shown));
            }
        }
    }

    /// Remember one status and show it as false when it is a gated readout whose control last
    /// read false or has not been heard. Returns the readouts that name this status's control
    /// and whose derived value it changed, each already derived, to be shown after it.
    pub(crate) fn apply(&mut self, profile: &Profile, vo: &mut ValueObject) -> Vec<ValueObject> {
        self.truth.insert(vo.id.clone(), vo.reads_true());
        if let Some(control) = gated(profile, vo) {
            let shown = units::shows_true(profile, control, vo.reads_true(), |gate| {
                self.truth.get(&gate.id.to_string()).copied()
            });
            vo.display = shown.to_string();
            self.readouts.insert(vo.id.clone(), (vo.clone(), shown));
        }
        let Some(key) = vo.key.as_deref() else {
            return Vec::new();
        };
        let open = vo.reads_true();
        let mut changed = Vec::new();
        for control in &profile.controls {
            let names_this = control
                .only_while
                .as_deref()
                .is_some_and(|gate| gate.eq_ignore_ascii_case(key));
            if !names_this {
                continue;
            }
            if let Some((last, shown)) = self.readouts.get_mut(&control.id.to_string()) {
                let now = units::shows_true(profile, control, last.reads_true(), |_| Some(open));
                if now != *shown {
                    *shown = now;
                    last.display = now.to_string();
                    changed.push(last.clone());
                }
            }
        }
        changed
    }
}

/// The control of a reading that is a readout an `only_while` gates.
fn gated<'p>(profile: &'p Profile, vo: &ValueObject) -> Option<&'p Control> {
    let control = profile.control_by_key(vo.key.as_deref()?)?;
    profile.gate_of(control).map(|_| control)
}

/// Show a readout that has an `only_while` as false while the control it names does not read
/// true. The raw value stays as the device sent it.
pub(super) fn apply_only_while(profile: &Profile, values: &mut [ValueObject]) {
    let gated: Vec<(usize, bool)> = values
        .iter()
        .enumerate()
        .filter_map(|(i, vo)| {
            let control = gated(profile, vo)?;
            let shown = units::shows_true(profile, control, vo.reads_true(), |gate| {
                let id = gate.id.to_string();
                Some(values.iter().any(|g| g.id == id && g.reads_true()))
            });
            Some((i, shown))
        })
        .collect();
    for (i, on) in gated {
        values[i].display = on.to_string();
    }
}
