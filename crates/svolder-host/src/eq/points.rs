// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The points of the live computer EQ: the filters a hand can move, and the slot of the live
//! chain each one sits in.
//!
//! A point keeps its slot for as long as it exists, so moving one past another changes no slot
//! and a live update touches only the slot of the point that moved. The table lists the points
//! by frequency: [`Points::sort`] puts them in that order when an edit ends, and never while a
//! drag goes on. Point 1 is always there: a flat curve is one peaking point at 0 dB.

use svolder_profiles::tone::apo::{Correction, FilterKind, SourceFilter};

use super::HostEqState;
use super::live::SLOTS;

/// What a hand may set: a preset's own values outside these play as written, and a hand that
/// edits one field of such a point leaves the others where they are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HandRanges {
    /// Frequency in Hz, from and to.
    pub f: (f64, f64),
    /// Gain in dB.
    pub g: (f64, f64),
    /// Q.
    pub q: (f64, f64),
}

/// The ranges a hand sets points in.
pub const HAND: HandRanges = HandRanges {
    f: (20.0, 20_000.0),
    g: (-12.0, 12.0),
    q: (0.1, 10.0),
};

/// What an edit changed, which says how it is sent: a change of values goes to the slot's
/// parameters at once, a change of type is the same message (every slot takes any type) but is
/// worth telling apart to a screen that redraws the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// The frequency, gain or Q changed (or nothing did).
    Values,
    /// The type changed.
    Kind,
}

/// The points and their slots, in the order the table lists them.
#[derive(Debug, Clone, PartialEq)]
pub struct Points {
    /// The filters, one per point.
    pub filters: Vec<SourceFilter>,
    /// The slot (1 to 16) of each filter, in the same order.
    pub slots: Vec<u8>,
}

impl Points {
    /// The one point a flat curve is: a peak at 1 kHz, 0 dB, Q 1.
    pub fn lone() -> Points {
        Points {
            filters: vec![SourceFilter {
                kind: FilterKind::Peaking,
                f: 1000.0,
                g: 0.0,
                q: 1.0,
            }],
            slots: vec![1],
        }
    }

    /// The points a saved state holds. A state without slots (saved before the live chain), or
    /// whose slots are not one each of 1 to 16, has its filters in slots 1 to n; a state with
    /// no filters is the lone point. More than 16 filters keep the first 16.
    pub fn from_state(state: &HostEqState) -> Points {
        let filters: Vec<SourceFilter> = state.filters.iter().take(SLOTS).cloned().collect();
        if filters.is_empty() {
            return Points::lone();
        }
        let kept = &state.slots[..state.slots.len().min(filters.len())];
        let valid = kept.len() == filters.len()
            && kept.iter().all(|slot| (1..=SLOTS as u8).contains(slot))
            && (1..kept.len()).all(|i| !kept[..i].contains(&kept[i]));
        let slots = match valid {
            true => kept.to_vec(),
            false => (1..=filters.len() as u8).collect(),
        };
        let mut points = Points { filters, slots };
        points.sort();
        points
    }

    /// Put the points in order of frequency, each with its slot; points of one frequency keep
    /// the order they have. Returns whether any point moved.
    pub fn sort(&mut self) -> bool {
        let mut order: Vec<usize> = (0..self.filters.len()).collect();
        order.sort_by(|a, b| self.filters[*a].f.total_cmp(&self.filters[*b].f));
        if order.iter().enumerate().all(|(at, from)| at == *from) {
            return false;
        }
        self.filters = order.iter().map(|i| self.filters[*i].clone()).collect();
        self.slots = order.iter().map(|i| self.slots[*i]).collect();
        true
    }

    /// Add `filter` as the last point, in the lowest free slot, its values held to [`HAND`].
    /// Returns its index, or says the chain is full or a value is not a number.
    pub fn add(&mut self, filter: SourceFilter) -> Result<usize, String> {
        if [filter.f, filter.g, filter.q]
            .iter()
            .any(|value| !value.is_finite())
        {
            return Err("a point's values are numbers".to_string());
        }
        let Some(slot) = (1..=SLOTS as u8).find(|slot| !self.slots.contains(slot)) else {
            return Err(format!("the computer EQ holds {SLOTS} points"));
        };
        self.filters.push(SourceFilter {
            kind: filter.kind,
            f: filter.f.clamp(HAND.f.0, HAND.f.1),
            g: filter.g.clamp(HAND.g.0, HAND.g.1),
            q: filter.q.clamp(HAND.q.0, HAND.q.1),
        });
        self.slots.push(slot);
        Ok(self.filters.len() - 1)
    }

    /// Remove the point at `index`. Returns the slot it frees, which the caller sets back to a
    /// transparent one. The only point left stays: it is set flat (0 dB), and keeps its
    /// frequency, type and Q.
    pub fn remove(&mut self, index: usize) -> Result<u8, String> {
        if index >= self.filters.len() {
            return Err(format!("there is no point {}", index + 1));
        }
        if self.filters.len() == 1 {
            self.filters[0].g = 0.0;
            return Ok(self.slots[0]);
        }
        self.filters.remove(index);
        Ok(self.slots.remove(index))
    }

    /// Set the fields that are given on the point at `index`, each held to [`HAND`]; a field
    /// that is not given keeps its value as written, even one outside the ranges. A value that
    /// is not a number is refused.
    pub fn set(
        &mut self,
        index: usize,
        kind: Option<FilterKind>,
        f: Option<f64>,
        g: Option<f64>,
        q: Option<f64>,
    ) -> Result<Change, String> {
        let count = self.filters.len();
        let Some(filter) = self.filters.get_mut(index) else {
            return Err(format!("there is no point {} of {count}", index + 1));
        };
        if [f, g, q].iter().flatten().any(|value| !value.is_finite()) {
            return Err("a point's values are numbers".to_string());
        }
        let held = |value: f64, (low, high): (f64, f64)| value.clamp(low, high);
        let mut change = Change::Values;
        if let Some(kind) = kind {
            if kind != filter.kind {
                change = Change::Kind;
            }
            filter.kind = kind;
        }
        if let Some(f) = f {
            filter.f = held(f, HAND.f);
        }
        if let Some(g) = g {
            filter.g = held(g, HAND.g);
        }
        if let Some(q) = q {
            filter.q = held(q, HAND.q);
        }
        Ok(change)
    }

    /// The correction these points make, with `stated` as its preamp (the caller lowers it to
    /// what never clips).
    pub fn correction(&self, stated: f64) -> Correction {
        Correction {
            preamp_db: stated,
            filters: self.filters.clone(),
        }
    }
}

#[cfg(test)]
mod tests;
