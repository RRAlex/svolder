// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The data form of a correction's preview: the target, the fit and where the fit misses it, on
//! a grid of frequencies the caller gives, so a front end draws numbers and works none out.
//!
//! A device's preview ([`Preview::device`]) fits the correction to the tone control's bands; the
//! computer's ([`Preview::computer`]) has no fit, only the correction as it plays.

use crate::model::EqSpec;
use crate::tone::apo::Correction;
use crate::tone::fit::{Fit, fit, fit_words, miss};
use crate::tone::preset::PresetBand;
use crate::tone::response::{BandParams, correction_db, correction_peak_db, peak_db, response_db};

/// The least the fit may miss the target by, in dB, before the gap between them counts.
pub const GAP_DB: f64 = 2.0;

/// A run of the grid where the fit misses the target by more than [`GAP_DB`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GapSpan {
    /// The first frequency of the run, in Hz.
    pub from_hz: f64,
    /// The last frequency of the run, in Hz.
    pub to_hz: f64,
}

/// What a fit adds to a preview.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct FitCurve {
    /// The fitted bands' response in dB on the grid, without the preamp.
    pub response: Vec<f64>,
    /// Where the fit misses the target by more than [`GAP_DB`], lowest frequency first.
    pub gap: Vec<GapSpan>,
    /// The root-mean-square miss in dB over 20 Hz to 10 kHz, on the preview's grid.
    pub rms_db: f64,
    /// The largest miss in dB, as a magnitude, on the preview's grid.
    pub worst_db: f64,
    /// Where the largest miss is, in Hz, on the preview's grid.
    pub worst_hz: f64,
}

/// A correction on a grid.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Preview {
    /// The grid, in Hz, the curves are on.
    pub freqs: Vec<f64>,
    /// The correction's response in dB on the grid, without its preamp.
    pub target: Vec<f64>,
    /// The correction's own preamp in dB.
    pub preamp_db: f64,
    /// How far the curve boosts at most, in dB: the fitted bands' for a device (the volume to
    /// lower by), the correction's own for the computer.
    pub peak_db: f64,
    /// The fit; `None` for the computer, which plays the correction itself.
    pub fit: Option<FitCurve>,
}

impl Preview {
    /// The preview of `correction` for the tone control `eq` on `grid`: the correction is
    /// fitted to the device's bands, and the fit is read on the grid.
    pub fn device(correction: &Correction, eq: &EqSpec, grid: &[f64]) -> Preview {
        Preview::of_fit(correction, &fit(eq, correction), grid)
    }

    /// [`Preview::device`] for a fit already worked out. The miss is read on `grid`, as
    /// [`Preview::of_bands`] reads it, not taken from the fit's own finer grid.
    pub fn of_fit(correction: &Correction, fitted: &Fit, grid: &[f64]) -> Preview {
        let target = correction_db(correction, grid);
        let bands: Vec<BandParams> = fitted.bands.iter().map(band_params).collect();
        let response = response_db(&bands, grid);
        let gap = gap_spans(grid, &target, &response);
        let (rms_db, worst_db, worst_hz) = miss(grid, &target, &response);
        Preview {
            freqs: grid.to_vec(),
            target,
            preamp_db: correction.preamp_db,
            peak_db: fitted.peak_db,
            fit: Some(FitCurve {
                response,
                gap,
                rms_db,
                worst_db,
                worst_hz,
            }),
        }
    }

    /// The preview of `correction` against `bands`, the bands standing on a device now (not the
    /// fit's own): the response, the gap and the miss are those of `bands` on `grid`.
    pub fn of_bands(correction: &Correction, bands: &[BandParams], grid: &[f64]) -> Preview {
        let target = correction_db(correction, grid);
        let response = response_db(bands, grid);
        let gap = gap_spans(grid, &target, &response);
        let (rms_db, worst_db, worst_hz) = miss(grid, &target, &response);
        Preview {
            freqs: grid.to_vec(),
            target,
            preamp_db: correction.preamp_db,
            peak_db: peak_db(bands),
            fit: Some(FitCurve {
                response,
                gap,
                rms_db,
                worst_db,
                worst_hz,
            }),
        }
    }

    /// The fit as words ([`fit_words`]), or `None` for the computer's preview.
    pub fn fit_line(&self) -> Option<String> {
        let fit = self.fit.as_ref()?;
        Some(fit_words(
            fit.rms_db,
            fit.worst_db,
            fit.worst_hz,
            self.peak_db,
        ))
    }

    /// The preview of `correction` as the computer plays it on `grid`: no fit.
    pub fn computer(correction: &Correction, grid: &[f64]) -> Preview {
        Preview {
            freqs: grid.to_vec(),
            target: correction_db(correction, grid),
            preamp_db: correction.preamp_db,
            peak_db: correction_peak_db(correction),
            fit: None,
        }
    }
}

fn band_params(band: &PresetBand) -> BandParams {
    BandParams {
        f0: band.f,
        gain_db: band.g,
        q: band.q,
    }
}

/// The runs of `grid` where `fit` and `target` differ by more than [`GAP_DB`].
fn gap_spans(grid: &[f64], target: &[f64], fit: &[f64]) -> Vec<GapSpan> {
    let mut spans = Vec::new();
    let mut open: Option<(f64, f64)> = None;
    for ((&hz, &target), &fit) in grid.iter().zip(target).zip(fit) {
        match ((fit - target).abs() > GAP_DB, &mut open) {
            (true, Some((_, to))) => *to = hz,
            (true, None) => open = Some((hz, hz)),
            (false, _) => {
                if let Some((from_hz, to_hz)) = open.take() {
                    spans.push(GapSpan { from_hz, to_hz });
                }
            }
        }
    }
    if let Some((from_hz, to_hz)) = open {
        spans.push(GapSpan { from_hz, to_hz });
    }
    spans
}

#[cfg(test)]
mod tests;
