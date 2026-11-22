// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The computer's part of a scene: what the computer plays, as stated, kept as a value.

use serde::{Deserialize, Serialize};
use svolder_profiles::tone::apo::SourceFilter;

use super::control::{EqView, kind_word};
use super::crossfeed::held;
use super::points::Points;
use super::{CrossfeedSettings, CrossfeedView, LoudnessSettings};

/// How far two values may differ and still be the same setting: the files and the wire keep
/// every number as it was set, so only a rounding on the way may separate them.
const SAME: f64 = 1e-6;

/// The computer's part of a scene: what it plays, as stated, never the level a hold plays it at.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComputerPart {
    /// Whether the EQ chain is on.
    pub eq_on: bool,
    /// The EQ's preset name, or `custom` once a hand changed the points.
    pub name: String,
    /// Where the correction came from, as free text; absent for a band preset or a file.
    pub origin: Option<String>,
    /// The stated preamp in dB: what the EQ asks for, not the level a hold plays it at.
    pub preamp_db: f64,
    /// The filters, in order, as the EQ state keeps them.
    pub filters: Vec<SourceFilter>,
    /// The loudness settings, with their own on flag; absent when loudness was never set.
    pub loudness: Option<LoudnessSettings>,
    /// Whether crossfeed plays.
    pub crossfeed_on: bool,
    /// The crossfeed's model, preset and numbers, kept while it is off.
    pub crossfeed: Option<CrossfeedSettings>,
    /// Whether the virtual speakers play.
    pub speakers_on: bool,
    /// The id of the virtual-speakers set, kept while they are off.
    pub speakers: Option<String>,
}

impl ComputerPart {
    /// Whether `view` plays this part: the EQ's on switch and, while it is on, its points and
    /// stated preamp; for each stage its on switch and, while it is on, its settings; the set of
    /// the virtual speakers while they play. A setting kept while its stage is off changes
    /// nothing that is heard, so it is not compared. A view from a peer that lacks a stage
    /// counts as that stage off.
    pub fn matches(&self, view: &EqView) -> bool {
        self.eq_matches(view)
            && self.crossfeed_matches(view)
            && self.speakers_match(view)
            && self.loudness_matches(view)
    }

    fn eq_matches(&self, view: &EqView) -> bool {
        if self.eq_on != view.on {
            return false;
        }
        if !self.eq_on {
            return true;
        }
        // The controller keeps its points in frequency order (a stable sort), and a curve with
        // none plays the one flat point: a part written by hand is read the same way.
        let mut filters = match self.filters.is_empty() {
            true => Points::lone().filters,
            false => self.filters.clone(),
        };
        filters.sort_by(|a, b| a.f.total_cmp(&b.f));
        same(self.preamp_db, view.stated_preamp_db)
            && filters.len() == view.points.len()
            && filters.iter().zip(&view.points).all(|(filter, point)| {
                kind_word(filter.kind) == point.kind
                    && same(filter.f, point.f)
                    && same(filter.g, point.g)
                    && same(filter.q, point.q)
            })
    }

    fn crossfeed_matches(&self, view: &EqView) -> bool {
        let playing = view
            .crossfeed
            .as_ref()
            .is_some_and(|crossfeed| crossfeed.on);
        if self.crossfeed_on != playing {
            return false;
        }
        match (&self.crossfeed, &view.crossfeed) {
            (Some(settings), Some(view)) if self.crossfeed_on => {
                let settings = held(settings);
                crossfeed_same(
                    &CrossfeedView::of(&settings, true, 48_000, None, None),
                    view,
                    // The delay is the model's own when it is at or under the floor the view
                    // works out at the rate the output runs at.
                    settings.delay_ms.is_none_or(|ms| ms <= view.delay_floor_ms),
                    settings.delay_ms.unwrap_or(0.0),
                )
            }
            (None, _) if self.crossfeed_on => false,
            _ => true,
        }
    }

    fn speakers_match(&self, view: &EqView) -> bool {
        let playing = view.speakers.as_ref().is_some_and(|speakers| speakers.on);
        if self.speakers_on != playing {
            return false;
        }
        !self.speakers_on
            || view
                .speakers
                .as_ref()
                .is_some_and(|speakers| speakers.set == self.speakers)
    }

    fn loudness_matches(&self, view: &EqView) -> bool {
        let wanted = self.loudness.filter(|settings| settings.on);
        let playing = view.loudness.as_ref().filter(|loudness| loudness.on);
        match (wanted, playing) {
            (None, None) => true,
            (Some(settings), Some(loudness)) => {
                let settings = settings.held();
                same(settings.strength_pct, loudness.strength_pct)
                    && same(settings.reference_pct, loudness.reference_pct)
            }
            _ => false,
        }
    }
}

/// Whether two numbers are the same setting.
fn same(a: f64, b: f64) -> bool {
    (a - b).abs() <= SAME
}

/// Whether the crossfeed `view` holds the settings of `part` (the model, the preset it came from,
/// the amount, the crossover) with the delay `own` (the model's own) or `delay_ms` above it.
fn crossfeed_same(part: &CrossfeedView, view: &CrossfeedView, own: bool, delay_ms: f64) -> bool {
    part.model == view.model
        && part.preset == view.preset
        && part.edited_from == view.edited_from
        && same(part.amount_pct, view.amount_pct)
        && same(part.crossover_hz, view.crossover_hz)
        && own == view.delay_own
        && (own || same(delay_ms, view.delay_ms))
}
