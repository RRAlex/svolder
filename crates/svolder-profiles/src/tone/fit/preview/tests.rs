// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::test_support::tone;
use crate::tone::apo::{FilterKind, SourceFilter};
use crate::tone::response::log_freqs;

fn pk(f: f64, g: f64, q: f64) -> SourceFilter {
    SourceFilter {
        kind: FilterKind::Peaking,
        f,
        g,
        q,
    }
}

#[test]
fn the_spans_are_the_runs_past_the_gap() {
    let grid = [100.0, 200.0, 300.0, 400.0, 500.0, 600.0];
    let target = [0.0; 6];
    let fit = [0.0, 3.0, -3.0, 0.5, 0.0, 2.5];
    assert_eq!(
        gap_spans(&grid, &target, &fit),
        vec![
            GapSpan {
                from_hz: 200.0,
                to_hz: 300.0
            },
            GapSpan {
                from_hz: 600.0,
                to_hz: 600.0
            }
        ]
    );
    // Exactly the gap is not past it.
    assert!(gap_spans(&grid, &target, &[GAP_DB; 6]).is_empty());
}

#[test]
fn a_correction_the_bands_hold_has_no_gap_and_the_numbers_of_its_fit() {
    let eq = tone("mesh-eq-5");
    let correction = Correction {
        preamp_db: -3.0,
        filters: vec![pk(100.0, 4.0, 1.0), pk(1000.0, -3.0, 1.0)],
    };
    let grid = log_freqs(181);
    let preview = Preview::device(&correction, &eq, &grid);
    assert_eq!(preview.freqs.len(), 181);
    assert_eq!(preview.target.len(), 181);
    let fit = preview.fit.as_ref().expect("a device has a fit");
    assert_eq!(fit.response.len(), 181);
    assert!(fit.gap.is_empty(), "{:?}", fit.gap);
    assert!(fit.rms_db < 0.2, "{}", fit.rms_db);
    assert_eq!(preview.preamp_db, -3.0);
    let again = Preview::of_fit(&correction, &crate::tone::fit::fit(&eq, &correction), &grid);
    assert_eq!(preview, again);
}

#[test]
fn a_correction_past_the_bands_shows_its_gap() {
    // Eight narrow boosts and cuts cannot be held by three bands.
    let eq = tone("mesh-eq-3");
    let filters = [
        (60.0, 9.0),
        (150.0, -9.0),
        (400.0, 9.0),
        (900.0, -9.0),
        (2000.0, 9.0),
        (4000.0, -9.0),
        (7000.0, 9.0),
    ]
    .iter()
    .map(|&(f, g)| pk(f, g, 6.0))
    .collect();
    let correction = Correction {
        preamp_db: 0.0,
        filters,
    };
    let preview = Preview::device(&correction, &eq, &log_freqs(181));
    let fit = preview.fit.expect("a fit");
    assert!(!fit.gap.is_empty());
    assert!(fit.worst_db > GAP_DB);
    assert!(fit.gap.windows(2).all(|w| w[0].to_hz < w[1].from_hz));
}

#[test]
fn a_preview_of_the_bands_on_the_device_reads_those_bands_not_the_fit() {
    let eq = tone("mesh-eq-5");
    let correction = Correction {
        preamp_db: -3.0,
        filters: vec![pk(100.0, 4.0, 1.0), pk(1000.0, -3.0, 1.0)],
    };
    let grid = log_freqs(181);
    let fitted = crate::tone::fit::fit(&eq, &correction);
    let params: Vec<BandParams> = fitted.bands.iter().map(band_params).collect();
    let held = Preview::of_bands(&correction, &params, &grid);
    let by_fit = Preview::of_fit(&correction, &fitted, &grid);
    let (held_fit, fit) = (held.fit.as_ref().unwrap(), by_fit.fit.as_ref().unwrap());
    assert_eq!(held_fit.gap, fit.gap);
    assert!(
        (held_fit.rms_db - fit.rms_db).abs() < 1e-3,
        "{} {}",
        held_fit.rms_db,
        fit.rms_db
    );
    // a band moved 8 dB off the fit: the response, the gap and the miss follow it
    let mut moved = params;
    moved[0].gain_db += 8.0;
    let edited = Preview::of_bands(&correction, &moved, &grid);
    let edited_fit = edited.fit.as_ref().unwrap();
    assert_ne!(edited_fit.response, held_fit.response);
    assert!(!edited_fit.gap.is_empty());
    assert!(edited_fit.rms_db > held_fit.rms_db + 1.0);
    assert_eq!(edited.target, held.target);
}

#[test]
fn a_fit_preview_reads_the_same_miss_as_the_bands_it_fitted() {
    let eq = tone("gungnir2-v2");
    let correction = Correction {
        preamp_db: -6.0,
        filters: vec![
            pk(24.0, 5.5, 0.8),
            pk(75.0, -3.2, 1.1),
            pk(160.0, 2.4, 0.9),
            pk(420.0, -2.1, 1.4),
            pk(1100.0, 1.7, 2.0),
            pk(2300.0, 3.9, 1.6),
            pk(3800.0, -4.4, 3.1),
            pk(5600.0, 5.2, 2.7),
            pk(7900.0, -6.0, 4.0),
            pk(9400.0, 4.5, 3.3),
        ],
    };
    let grid = log_freqs(181);
    let fitted = crate::tone::fit::fit(&eq, &correction);
    let params: Vec<BandParams> = fitted.bands.iter().map(band_params).collect();
    let held = Preview::of_bands(&correction, &params, &grid);
    let by_fit = Preview::of_fit(&correction, &fitted, &grid);
    assert_eq!(held.fit, by_fit.fit);
    assert_eq!(held.fit_line(), by_fit.fit_line());
}

#[test]
fn the_computer_preview_has_no_fit_and_the_peak_of_the_correction() {
    let correction = Correction {
        preamp_db: -5.0,
        filters: vec![pk(1000.0, 4.0, 1.0)],
    };
    let preview = Preview::computer(&correction, &log_freqs(181));
    assert!(preview.fit.is_none());
    assert!((preview.peak_db - 4.0).abs() < 0.1, "{}", preview.peak_db);
    assert_eq!(preview.target.len(), 181);
}
