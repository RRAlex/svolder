// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::layout::Rect;
use svolder_ipc::TuiConfig;
use svolder_profiles::tone::response::{self, BandParams};

use crate::tui::app::App;
use crate::tui::test_support::presets::imported;
use crate::tui::test_support::profile;

use super::*;

/// An app whose tone control the preset `Phones` is loaded on, and the curve cells at 80×24.
fn app_and_curve() -> (App, Rect) {
    let mut app = App::new(
        profile(),
        "gungnir".into(),
        &TuiConfig::default(),
        Vec::new(),
    );
    app.presets.saved = vec![imported("Phones")];
    (
        app,
        crate::tui::screens::tone::curve_area(Rect::new(0, 0, 80, 24), 5),
    )
}

/// Flat bands: no band to sum.
const FLAT: [BandParams; 1] = [BandParams {
    f0: 1000.0,
    gain_db: 0.0,
    q: 1.0,
}];

/// The gap is in exactly the columns where the miss exceeds [`GAP_DB`], worked out from the
/// correction and the bands directly, and each such column is filled between the two curves.
#[test]
fn the_gap_is_in_the_columns_where_the_miss_exceeds_the_threshold() {
    let (app, curve) = app_and_curve();
    let view = app.target_view("Phones").expect("an imported preset");
    let overlay = overlay(&view, &FLAT, curve);
    let source = app.presets.saved[0].source.as_ref().unwrap();
    let mut want = std::collections::BTreeSet::new();
    for x in curve.x..curve.right() {
        let hz = frequency_at(curve, x);
        let target = response::correction_db(&source.correction, &[hz])[0];
        if target.abs() > GAP_DB + 0.05 {
            want.insert(x);
        }
    }
    let got: std::collections::BTreeSet<u16> = overlay
        .gap
        .iter()
        .map(|&(log_hz, _)| {
            (0..curve.width)
                .map(|n| curve.x + n)
                .min_by(|a, b| {
                    let off = |x: &u16| (frequency_at(curve, *x).log10() - log_hz).abs();
                    off(a).total_cmp(&off(b))
                })
                .expect("a column")
        })
        .collect();
    assert!(!want.is_empty());
    for x in &want {
        assert!(
            got.contains(x),
            "column {x} misses by more than 2 dB and has no fill"
        );
    }
    for x in &got {
        let hz = frequency_at(curve, *x);
        let target = response::correction_db(&source.correction, &[hz])[0];
        assert!(
            target.abs() > GAP_DB - 0.05,
            "column {x} is filled at a miss of {target}"
        );
    }
}

/// A fill never leaves the chart: a miss running off the top or the bottom ends at the edge,
/// and a target off the chart's gains has no dotted point.
#[test]
fn values_off_the_chart_clip_as_the_curve_does() {
    let (app, curve) = app_and_curve();
    let view = app.target_view("Phones").expect("an imported preset");
    let overlay = overlay(&view, &FLAT, curve);
    for &(_, db) in overlay.gap.iter().chain(&overlay.target) {
        assert!((GAIN_FLOOR_DB..=GAIN_CEILING_DB).contains(&db), "{db}");
    }
}

/// One point per column for the target, however wide the chart: nothing grows with the work
/// of a frame but the columns it draws.
#[test]
fn the_target_has_one_point_per_column() {
    let (app, _) = app_and_curve();
    let view = app.target_view("Phones").expect("an imported preset");
    for width in [80, 126, 200] {
        let curve = crate::tui::screens::tone::curve_area(Rect::new(0, 0, width, 40), 5);
        let drawn = overlay(&view, &FLAT, curve);
        assert!(drawn.target.len() <= usize::from(curve.width), "{width}");
        assert!(
            drawn.target.len() >= usize::from(curve.width) - 2,
            "{width}"
        );
    }
}
