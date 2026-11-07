// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! Which side the volume and the tone control are shown and changed for: the device's own, or
//! the computer's. One switch flips every screen that has one; flipping writes nothing.
//!
//! The computer side has a mode (`Off` or `EQ`) and a table of points, `a` adding one and `x`
//! removing the focused one. Every change goes out as an [`Intent::HostEq`].

use svolder_host::eq::{EqView, PointView};
use svolder_ipc::HostEqOp;

use super::{App, Intent, Screen, ToneRow};

/// Which side is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Side {
    /// The device's own volume and tone control.
    #[default]
    Dac,
    /// The computer's.
    Computer,
}

/// The gain one press of an arrow on a point moves it, in dB.
const GAIN_STEP: f64 = 0.5;
/// What one press of `[` or `]` multiplies or divides a point's Q by.
pub(super) const Q_FACTOR: f64 = 1.1;
/// The frequency a new point starts at, in Hz.
const NEW_F: f64 = 1000.0;
/// The Q a new point starts with.
const NEW_Q: f64 = 1.0;
/// The hand ranges: frequency, gain and Q.
pub(super) const F_RANGE: (f64, f64) = (20.0, 20_000.0);
pub(super) const G_RANGE: (f64, f64) = (-12.0, 12.0);
pub(super) const Q_RANGE: (f64, f64) = (0.1, 10.0);

impl App {
    /// Whether the switch is drawn: the computer's controls are there to flip to, or say why
    /// they are not.
    pub fn computer_offered(&self) -> bool {
        self.tui.host_audio && self.host.computer.offered()
    }

    /// The tone control the shared preset list is built for: the device's own, and on the
    /// computer's side a device with none (a Loki Max) is given the Gungnir 2's five bands, as
    /// `svolder host eq apply` does. Nothing built from it is written to the device.
    pub(crate) fn preset_spec(&self) -> Option<svolder_profiles::EqSpec> {
        if let Some(eq) = self.profile.eq.clone() {
            return Some(eq);
        }
        if !self.on_computer() {
            return None;
        }
        static FALLBACK: std::sync::OnceLock<Option<svolder_profiles::EqSpec>> =
            std::sync::OnceLock::new();
        FALLBACK
            .get_or_init(|| {
                svolder_profiles::Registry::builtin()
                    .ok()?
                    .get("gungnir2-v2")?
                    .eq
                    .clone()
            })
            .clone()
    }

    /// Whether the computer's side is the one shown.
    pub fn on_computer(&self) -> bool {
        self.side == Side::Computer && self.computer_offered()
    }

    /// The computer EQ as last heard.
    pub(crate) fn computer_eq(&self) -> Option<&EqView> {
        self.host.computer.eq.as_deref()
    }

    /// The name of the preset on the computer while the computer EQ is on and its points are
    /// still that preset's.
    pub(crate) fn computer_preset(&self) -> Option<String> {
        self.computer_eq()
            .filter(|eq| eq.on)
            .and_then(|eq| eq.preset.clone())
    }

    /// The computer's view changed: a preset the DAC was flattened for is on the computer now, so
    /// a later refusal is no refusal of that move.
    pub(crate) fn note_computer_view(&mut self) {
        let applied = self.computer_preset();
        if self.presets.computer_flattened.is_some()
            && applied.as_ref() == self.presets.computer_flattened.as_ref()
        {
            self.presets.computer_flattened = None;
        }
    }

    /// The computer's points as the correction they play, for a preset saved from them.
    pub(crate) fn computer_correction(&self) -> Option<svolder_profiles::tone::apo::Correction> {
        use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};
        let eq = self.computer_eq()?;
        let filters = self
            .computer_points()
            .iter()
            .map(|point| {
                Some(SourceFilter {
                    kind: point.kind.parse::<FilterKind>().ok()?,
                    f: point.f,
                    g: point.g,
                    q: point.q,
                })
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Correction {
            preamp_db: eq.preamp_db,
            filters,
        })
    }

    /// The computer's points, with the one a drag holds drawn where the pointer put it.
    pub(crate) fn computer_points(&self) -> Vec<PointView> {
        let mut points = self
            .computer_eq()
            .map(|eq| eq.points.clone())
            .unwrap_or_default();
        if let Some(drag) = self.pointer.curve_drag.as_ref()
            && let Some([f, g, q]) = drag.point.as_deref().and_then(|held| held.shown)
            && let Some(index) = super::curve_drag::point_index(&points, drag)
            && let Some(point) = points.get_mut(index)
        {
            (point.f, point.g, point.q) = (f, g, q);
        }
        points
    }

    /// The focused point, held inside the table.
    pub(crate) fn focused_point(&self) -> usize {
        self.tone
            .point
            .min(self.computer_points().len().saturating_sub(1))
    }

    /// Flip to the other side, writing nothing but the last value of a computer drag in progress.
    /// Where the computer's controls are not there, the
    /// toast says why.
    pub(crate) fn flip_side(&mut self) -> Vec<Intent> {
        if !self.computer_offered() {
            self.warn("computer controls are not available here");
            return Vec::new();
        }
        if let Some(why) = self.host.computer.unavailable().map(str::to_string) {
            self.warn(why);
            return Vec::new();
        }
        // A computer point still held goes out as the last value of its drag, the one it is
        // already playing, so the daemon keeps the points; nothing else is written.
        let held = self
            .pointer
            .curve_drag
            .as_ref()
            .is_some_and(|drag| drag.point.is_some());
        let intents = match held {
            true => super::curve_drag::release_point(self, None),
            false => Vec::new(),
        };
        self.pointer.curve_drag = None;
        self.tone.edit_line = None;
        self.side = match self.side {
            Side::Dac => Side::Computer,
            Side::Computer => Side::Dac,
        };
        self.tone.row = match (self.side, self.tone.row) {
            (Side::Computer, ToneRow::Band | ToneRow::Loudness) => ToneRow::Point(self.tone.point),
            (Side::Dac, ToneRow::Point(_)) => ToneRow::Band,
            (_, row) => row,
        };
        self.frame_flags.screen_changed = true;
        intents
    }

    /// Focus point `index` of the computer's table.
    pub(crate) fn focus_point(&mut self, index: usize) {
        self.tone.point = index;
        self.tone.row = ToneRow::Point(index);
        if self.screen == Screen::Device {
            self.pane = super::Pane::Tone;
        }
    }

    /// The computer EQ turned `on` or off, or the other way from now when `None`.
    pub(crate) fn computer_mode(&self, on: Option<bool>) -> Vec<Intent> {
        let on = on.unwrap_or_else(|| !self.computer_eq().is_some_and(|eq| eq.on));
        vec![Intent::eq(HostEqOp::Mode { on })]
    }

    /// Step the computer's mode, `Off` before `EQ`.
    pub(crate) fn step_computer_mode(&self, dir: i64) -> Vec<Intent> {
        let on = self.computer_eq().is_some_and(|eq| eq.on);
        match (on, dir > 0) {
            (false, true) | (true, false) => self.computer_mode(Some(dir > 0)),
            _ => Vec::new(),
        }
    }

    /// Add a point after the last: a peaking point at 1 kHz, flat, as wide as Q 1.
    pub(crate) fn add_point(&mut self) -> Vec<Intent> {
        let Some(eq) = self.computer_eq() else {
            return Vec::new();
        };
        if eq.points.len() >= usize::from(eq.max.max(1)) {
            let max = eq.max;
            self.warn(format!("the computer EQ holds {max} points"));
            return Vec::new();
        }
        let at = eq.points.len();
        self.focus_point(at);
        vec![Intent::eq(HostEqOp::Add {
            kind: "PK".into(),
            f: NEW_F,
            g: 0.0,
            q: NEW_Q,
        })]
    }

    /// Remove the focused point; the only point left is set flat instead (0 dB, its frequency,
    /// type and Q as they are), and stays.
    pub(crate) fn remove_point(&mut self) -> Vec<Intent> {
        let points = self.computer_points();
        let count = points.len();
        let at = self.focused_point();
        let slot = points.get(at).map_or(0, |point| point.slot);
        if count > 1 {
            self.focus_point(at.saturating_sub(usize::from(at + 1 == count)));
        }
        vec![Intent::eq(HostEqOp::Remove {
            index: wire(at),
            slot: held_slot(slot),
        })]
    }

    /// The computer's table was replaced: the focus stays on the point it was on. The daemon
    /// lists the points by frequency when an edit ends, so the point may be elsewhere in the new
    /// list, with new values when the edit was its own.
    pub(crate) fn follow_point(&mut self, before: &[PointView]) {
        let after = self.computer_points();
        let at = self.tone.point;
        let to = followed(before, &after, at);
        if to == at {
            return;
        }
        self.tone.point = to;
        if matches!(self.tone.row, ToneRow::Point(_)) {
            self.tone.row = ToneRow::Point(to);
        }
    }

    /// Cycle point `at`'s type: PK, LSC, HSC and round.
    pub(crate) fn cycle_point_type(&self, at: usize) -> Vec<Intent> {
        let Some(point) = self.computer_points().get(at).cloned() else {
            return Vec::new();
        };
        let next = match point.kind.as_str() {
            "PK" => "LSC",
            "LSC" => "HSC",
            _ => "PK",
        };
        vec![Intent::eq(HostEqOp::Set {
            index: wire(at),
            slot: held_slot(point.slot),
            kind: Some(next.to_string()),
            f: None,
            g: None,
            q: None,
            done: true,
        })]
    }

    /// Put every point at 0 dB.
    pub(crate) fn flatten_computer(&self) -> Vec<Intent> {
        self.computer_points()
            .iter()
            .enumerate()
            .filter(|(_, point)| point.g != 0.0)
            .map(|(at, point)| set_point(at, point.slot, [None, Some(0.0), None], true))
            .collect()
    }

    /// Step the focused point's gain, `dir` times half a dB.
    pub(crate) fn nudge_point_gain(&self, at: usize, dir: i64) -> Vec<Intent> {
        let Some(point) = self.computer_points().get(at).cloned() else {
            return Vec::new();
        };
        let g = (point.g + GAIN_STEP * dir.signum() as f64).clamp(G_RANGE.0, G_RANGE.1);
        match g == point.g {
            true => Vec::new(),
            false => vec![set_point(at, point.slot, [None, Some(g), None], true)],
        }
    }

    /// Move the focused point's frequency by `octaves`.
    pub(crate) fn step_point_frequency(&self, octaves: f64) -> Vec<Intent> {
        let at = self.focused_point();
        let Some(point) = self.computer_points().get(at).cloned() else {
            return Vec::new();
        };
        let f = (point.f * 2f64.powf(octaves)).clamp(F_RANGE.0, F_RANGE.1);
        vec![set_point(at, point.slot, [Some(f), None, None], true)]
    }

    /// Multiply point `at`'s Q by `factor`.
    pub(crate) fn step_point_q(&self, at: usize, factor: f64) -> Vec<Intent> {
        let Some(point) = self.computer_points().get(at).cloned() else {
            return Vec::new();
        };
        let q = (point.q * factor).clamp(Q_RANGE.0, Q_RANGE.1);
        vec![set_point(at, point.slot, [None, None, Some(q)], true)]
    }

    /// What the Q keys multiply by.
    pub(crate) fn q_factor(up: bool) -> f64 {
        match up {
            true => Q_FACTOR,
            false => 1.0 / Q_FACTOR,
        }
    }

    /// The tone-control screen's keys on the computer side.
    pub(super) fn computer_tone_key(
        &mut self,
        code: crossterm::event::KeyCode,
    ) -> Option<Vec<Intent>> {
        use crossterm::event::KeyCode;
        let at = self.focused_point();
        Some(match code {
            KeyCode::Char('a') => self.add_point(),
            KeyCode::Char('x') => self.remove_point(),
            KeyCode::Char('f') => self.computer_mode(None),
            KeyCode::Char('0') => self.flatten_computer(),
            KeyCode::Char(',') => self.step_point_frequency(-1.0 / 12.0),
            KeyCode::Char('.') => self.step_point_frequency(1.0 / 12.0),
            KeyCode::Char('[') => self.step_point_q(at, Self::q_factor(false)),
            KeyCode::Char(']') => self.step_point_q(at, Self::q_factor(true)),
            KeyCode::Char(digit @ '1'..='9') => {
                let index = digit as usize - '1' as usize;
                if index < self.computer_points().len() {
                    self.focus_point(index);
                }
                Vec::new()
            }
            KeyCode::Char('y') => self.cycle_point_type(at),
            KeyCode::Enter => {
                self.open_point_line(at, 0);
                Vec::new()
            }
            _ => return None,
        })
    }
}

/// Where the point at `at` of `before` is listed in `after`: where it kept its values, else the
/// one point whose values are new when it is the one whose values went (an edit of its own), else
/// where it was. A point just added (`at` one past the end) is the one that is new.
pub(crate) fn followed(before: &[PointView], after: &[PointView], at: usize) -> usize {
    // Points that carry their slot are followed by it: it is theirs for as long as they exist.
    if before.iter().chain(after).all(|point| point.slot != 0) {
        let last = after.len().saturating_sub(1);
        return match before.get(at) {
            Some(held) => after
                .iter()
                .position(|point| point.slot == held.slot)
                .unwrap_or(at.min(last)),
            None => after
                .iter()
                .position(|point| before.iter().all(|old| old.slot != point.slot))
                .filter(|_| at == before.len())
                .unwrap_or(at),
        };
    }
    let without = |list: &[PointView], other: &[PointView]| {
        let mut left: Vec<&PointView> = other.iter().collect();
        list.iter()
            .enumerate()
            .filter(
                |(_, point)| match left.iter().position(|held| held == point) {
                    Some(found) => {
                        left.remove(found);
                        false
                    }
                    None => true,
                },
            )
            .map(|(index, _)| index)
            .collect::<Vec<_>>()
    };
    let (gone, new) = (without(before, after), without(after, before));
    let Some(held) = before.get(at) else {
        // a point added: it is the one that is new, wherever the frequency lists it
        return match (at == before.len(), new.as_slice()) {
            (true, [added]) if after.len() == before.len() + 1 => *added,
            _ => at,
        };
    };
    if after.get(at) == Some(held) {
        return at;
    }
    let nearest = after
        .iter()
        .enumerate()
        .filter(|(_, point)| *point == held)
        .map(|(index, _)| index)
        .min_by_key(|index| index.abs_diff(at));
    if let Some(index) = nearest {
        return index;
    }
    match (gone.as_slice(), new.as_slice()) {
        ([gone], [new]) if *gone == at => *new,
        _ => at.min(after.len().saturating_sub(1)),
    }
}

/// A point index as the wire counts it, from 1.
pub(crate) fn wire(index: usize) -> u8 {
    u8::try_from(index + 1).unwrap_or(u8::MAX)
}

/// The slot a request names its point by: the one the table lists it with, `None` when the
/// daemon gave none (0), and the index holds.
pub(crate) fn held_slot(slot: u8) -> Option<u8> {
    (slot != 0).then_some(slot)
}

/// One `Set` of a point's `[f, g, q]`, those given, for the point at `index` that holds `slot`:
/// the daemon changes the point in that slot even when another front end has re-sorted the table
/// since, and refuses the change when the point is gone.
pub(crate) fn set_point(index: usize, slot: u8, [f, g, q]: [Option<f64>; 3], done: bool) -> Intent {
    Intent::eq(HostEqOp::Set {
        index: wire(index),
        slot: held_slot(slot),
        kind: None,
        f,
        g,
        q,
        done,
    })
}

#[cfg(test)]
mod tests;
