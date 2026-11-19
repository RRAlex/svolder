// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use svolder_profiles::tone::response::coefficients;

/// The rates the proofs run at.
const RATES: [u32; 10] = [
    44_100, 48_000, 88_200, 96_000, 176_400, 192_000, 352_800, 384_000, 705_600, 768_000,
];

/// ISO 226:2003 table 1: frequency, alpha_f, L_U, T_f.
const ISO: [(f64, f64, f64, f64); 29] = [
    (20.0, 0.532, -31.6, 78.1),
    (25.0, 0.506, -27.2, 68.7),
    (31.5, 0.480, -23.0, 59.5),
    (40.0, 0.455, -19.1, 51.1),
    (50.0, 0.432, -15.9, 44.0),
    (63.0, 0.409, -13.0, 37.5),
    (80.0, 0.387, -10.3, 31.5),
    (100.0, 0.367, -8.1, 26.5),
    (125.0, 0.349, -6.2, 22.1),
    (160.0, 0.330, -4.5, 17.9),
    (200.0, 0.315, -3.1, 14.4),
    (250.0, 0.301, -2.0, 11.4),
    (315.0, 0.288, -1.1, 8.6),
    (400.0, 0.276, -0.4, 6.2),
    (500.0, 0.267, 0.0, 4.4),
    (630.0, 0.259, 0.3, 3.0),
    (800.0, 0.253, 0.5, 2.2),
    (1000.0, 0.250, 0.0, 2.4),
    (1250.0, 0.246, -2.7, 3.5),
    (1600.0, 0.244, -4.1, 1.7),
    (2000.0, 0.243, -1.0, -1.3),
    (2500.0, 0.243, 1.7, -4.2),
    (3150.0, 0.243, 2.5, -6.0),
    (4000.0, 0.242, 1.2, -5.4),
    (5000.0, 0.242, -2.1, -1.5),
    (6300.0, 0.245, -7.1, 6.0),
    (8000.0, 0.254, -11.2, 12.6),
    (10000.0, 0.271, -10.7, 13.9),
    (12500.0, 0.301, -3.1, 12.3),
];

/// The sound pressure level of the contour at `phon` for one row of the table.
fn contour(row: &(f64, f64, f64, f64), phon: f64) -> f64 {
    let (_, alpha, l_u, t_f) = *row;
    let a = 4.47e-3 * (10f64.powf(0.025 * phon) - 1.15)
        + (0.4 * 10f64.powf((t_f + l_u) / 10.0 - 9.0)).powf(alpha);
    10.0 / alpha * a.log10() - l_u + 94.0
}

/// The standard's difference curve at the 80 phon reference: what the contour `d` dB lower needs
/// over the reference contour, less the `d` the volume took.
fn standard(row: &(f64, f64, f64, f64), d: f64) -> f64 {
    contour(row, 80.0 - d) - (contour(row, 80.0) - d)
}

fn settings(strength: f64, reference: f64) -> LoudnessSettings {
    LoudnessSettings {
        on: true,
        strength_pct: strength,
        reference_pct: reference,
    }
}

fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}

/// The largest gain of the stage's sections over DC to Nyquist, in dB, by the preamp's own
/// evaluator.
fn peak_db(stage: &Stage, rate: u32) -> f64 {
    played_peak_db_of(&stage.sections, &[], rate)
}

/// The stage's gain at exactly DC, in dB: `(b0 + b1) / (1 + a1)` of each section.
fn dc_db(stage: &Stage) -> f64 {
    stage
        .sections
        .iter()
        .map(|c| 20.0 * ((c[0] + c[1]) / (1.0 + c[3])).abs().log10())
        .sum()
}

#[test]
fn the_standards_table_gives_its_own_anchors() {
    let one_khz = &ISO[17];
    assert!(near(contour(one_khz, 40.0), 40.0, 0.02));
    assert!(standard(one_khz, 25.0).abs() < 0.02);
    // The 40 phon contour at 20 Hz, as the standard prints it.
    assert!(near(contour(&ISO[0], 40.0), 99.85, 0.05));
}

#[test]
fn the_shelves_follow_the_standard_within_the_fit_budget() {
    // Budget in dB by depth (the analog fit's own error is 1.20, 1.43 and 2.70 at 10, 20, 40;
    // no four constants do better than 2.70 at 40 under the 6 dB cap on the top shelf).
    let budgets = [(10.0, 1.25), (20.0, 1.5), (40.0, 2.75)];
    for (d, budget) in budgets {
        for rate in [44_100, 48_000, 96_000, 192_000] {
            let worst = ISO
                .iter()
                .map(|row| {
                    // The stage is normalised to 0 dB at DC; the model is not.
                    let played = response(d, row.0, rate) + LOW_GAIN_PER_DB * d;
                    (played - standard(row, d)).abs()
                })
                .fold(0.0, f64::max);
            assert!(
                worst <= budget,
                "d {d} at {rate}: {worst:.2} dB over {budget}"
            );
        }
    }
}

#[test]
fn half_the_strength_follows_half_the_standards_curve() {
    // Strength 50 % at 40 dB below the reference plays d_e 20; the target is 0.5 x B(f, 40). The
    // contours are not exactly linear in the depth (B(f, 20) and 0.5 B(f, 40) differ by up to
    // 0.23 dB), so the budget is the 1.5 dB of d_e 20 plus that gap: 1.6 dB.
    for rate in [44_100, 48_000, 96_000, 192_000] {
        let worst = ISO
            .iter()
            .map(|row| {
                let played = response(20.0, row.0, rate) + LOW_GAIN_PER_DB * 20.0;
                (played - 0.5 * standard(row, 40.0)).abs()
            })
            .fold(0.0, f64::max);
        assert!(worst <= 1.6, "at {rate}: {worst:.2} dB over 1.6");
    }
}

#[test]
fn the_middle_stands_where_the_readout_says() {
    for d in [5.0, 13.68, 27.0, 40.0] {
        let (g_lo, _) = shelf_gains_db(d);
        let at_1k = response(d, 1_000.0, 96_000);
        // The shelves' tails give the middle a small bump over -G_lo.
        assert!(at_1k > -g_lo && at_1k < -g_lo + 0.8, "d {d}: {at_1k}");
        let bass = response(d, 20.0, 96_000);
        assert!(bass <= 0.0 && bass > -2.5, "d {d}: bass stays, {bass}");
    }
}

#[test]
fn the_stage_never_raises_any_frequency_at_any_rate_or_depth() {
    for rate in RATES {
        for step in 0..=400 {
            let d_e = f64::from(step) * 0.1;
            let played = stage(d_e, rate);
            let peak = peak_db(&played, rate);
            assert!(peak <= 1e-9, "d_e {d_e} at {rate}: peak {peak}");
            let dc = dc_db(&played);
            assert!(dc <= 1e-9, "d_e {d_e} at {rate}: DC {dc}");
            // The rounding to f32 costs the DC gain at most this much (0.0004 dB measured at
            // 768 kHz, where a section's b0 and b1 nearly cancel).
            assert!(dc >= -0.001, "d_e {d_e} at {rate}: DC {dc}");
            for set in played.sections {
                assert!(set.iter().all(|c| c.abs() <= COEFFICIENT_LIMIT));
                assert!(
                    set[3].abs() < 1.0 - 1e-7,
                    "d_e {d_e} at {rate}: pole {}",
                    set[3]
                );
                assert!(set.iter().all(|c| f64::from(*c as f32) == *c), "f32 values");
            }
        }
    }
}

#[test]
fn a_stage_worked_out_at_one_rate_is_cut_only_at_every_other_rate() {
    for from in RATES {
        for d_e in [0.1, 6.8, 13.7, 24.0, 40.0] {
            let played = stage(d_e, from);
            for to in RATES {
                let peak = peak_db(&played, to);
                assert!(
                    peak <= 1e-9,
                    "worked at {from}, played at {to}, d_e {d_e}: {peak}"
                );
            }
        }
    }
}

#[test]
fn the_f32_stage_is_within_005_db_of_the_design_from_20_hz_to_20_khz() {
    for rate in RATES {
        for d_e in [1.0, 9.0, 13.7, 20.0, 31.0, 40.0] {
            let played = stage(d_e, rate);
            for i in 0..CURVE_POINTS {
                let hz = curve_hz(i);
                let gap = (played.response_db(hz, rate) - response(d_e, hz, rate)).abs();
                assert!(gap <= 0.05, "d_e {d_e} at {rate} Hz {hz:.0}: {gap:.4} dB");
            }
        }
    }
}

#[test]
fn no_depth_is_the_exact_identity() {
    for rate in RATES {
        assert_eq!(stage(0.0, rate), Stage::identity());
        assert!(stage(0.0, rate).is_identity());
        assert_eq!(stage(-3.0, rate), Stage::identity());
        assert_eq!(stage(f64::NAN, rate), Stage::identity());
        // Infinite depth is the deepest, still cut-only.
        assert_eq!(stage(f64::INFINITY, rate), stage(MAX_DEPTH_DB, rate));
        assert!(!stage(0.1, rate).is_identity());
    }
    assert_eq!(Stage::identity().sections, [[1.0, 0.0, 0.0, 0.0, 0.0]; 2]);
    assert!(curve(0.0, 48_000).iter().all(|point| point.1 == 0.0));
    assert_eq!(response(0.0, 100.0, 48_000), 0.0);
}

#[test]
fn the_depth_is_the_volume_below_the_reference_scaled_by_the_strength() {
    // 50 % volume is a gain of 0.125: 18.06 dB below full volume.
    assert!(near(depth_db(0.125, &settings(100.0, 100.0)), 18.06, 0.005));
    assert!(near(depth_db(0.125, &settings(50.0, 100.0)), 9.03, 0.005));
    // 35 % volume, reference 100, strength 50: the worked example.
    assert!(near(
        depth_db(0.35f64.powi(3), &settings(50.0, 100.0)),
        13.68,
        0.005
    ));
    // Volume 0 and a gain that is not a number give the full depth.
    assert_eq!(depth_db(0.0, &settings(100.0, 100.0)), 40.0);
    assert_eq!(depth_db(f64::NAN, &settings(100.0, 100.0)), 40.0);
    assert_eq!(depth_db(1e-12, &settings(50.0, 100.0)), 20.0);
    // At and over the reference: nothing.
    assert_eq!(depth_db(1.0, &settings(100.0, 100.0)), 0.0);
    assert_eq!(depth_db(2.0, &settings(100.0, 100.0)), 0.0);
    assert_eq!(depth_db(0.5f64.powi(3), &settings(100.0, 50.0)), 0.0);
    assert!(near(
        depth_db(0.25f64.powi(3), &settings(100.0, 50.0)),
        60.0 * 2f64.log10(),
        1e-9
    ));
    // Strength 0 plays nothing.
    assert_eq!(depth_db(0.01, &settings(0.0, 100.0)), 0.0);
}

#[test]
fn the_depth_is_quantised_to_a_tenth_of_a_decibel() {
    assert_eq!(quantise_db(13.68), 13.7);
    assert_eq!(quantise_db(0.04), 0.0);
    assert_eq!(quantise_db(0.06), 0.1);
    assert_eq!(quantise_db(40.0), 40.0);
}

#[test]
fn settings_default_off_half_strength_flat_only_at_full_volume() {
    let defaults = LoudnessSettings::default();
    assert!(!defaults.on);
    assert_eq!(
        (defaults.strength_pct, defaults.reference_pct),
        (50.0, 100.0)
    );
    let parsed: LoudnessSettings = serde_json::from_str("{}").unwrap();
    assert_eq!(parsed, defaults);
    let parsed: LoudnessSettings = serde_json::from_str(r#"{"on":true}"#).unwrap();
    assert_eq!(
        parsed,
        LoudnessSettings {
            on: true,
            ..defaults
        }
    );
}

#[test]
fn a_change_snaps_and_holds_its_numbers() {
    let base = LoudnessSettings::default();
    let next = base
        .change(LoudnessChange {
            on: Some(true),
            strength_pct: Some(37.0),
            reference_pct: Some(62.4),
        })
        .unwrap();
    assert_eq!(next, settings(35.0, 62.0));
    let next = base
        .change(LoudnessChange {
            strength_pct: Some(250.0),
            reference_pct: Some(-4.0),
            ..LoudnessChange::default()
        })
        .unwrap();
    assert_eq!((next.strength_pct, next.reference_pct), (100.0, 1.0));
    assert!(!next.on, "on is left alone unless named");
    let next = base
        .change(LoudnessChange {
            reference_pct: Some(0.2),
            ..LoudnessChange::default()
        })
        .unwrap();
    assert_eq!(next.reference_pct, 1.0);
    // Nothing named: the settings as they are.
    assert_eq!(base.change(LoudnessChange::default()).unwrap(), base);
    // Values out of range on disk are held on the way through.
    let wild = LoudnessSettings {
        on: true,
        strength_pct: f64::NAN,
        reference_pct: 900.0,
    };
    assert_eq!(wild.held(), settings(50.0, 100.0));
    assert_eq!(
        wild.change(LoudnessChange::default()).unwrap(),
        settings(50.0, 100.0)
    );
}

#[test]
fn a_change_that_is_not_a_number_is_refused_in_words() {
    let base = settings(50.0, 80.0);
    for (change, word) in [
        (
            LoudnessChange {
                strength_pct: Some(f64::NAN),
                ..LoudnessChange::default()
            },
            "strength",
        ),
        (
            LoudnessChange {
                on: Some(true),
                reference_pct: Some(f64::INFINITY),
                ..LoudnessChange::default()
            },
            "reference",
        ),
    ] {
        let failure = base.change(change).unwrap_err();
        assert!(
            failure.what.contains(word) && failure.what.contains("not a number"),
            "{failure}"
        );
        assert!(failure.next.contains("from"), "{failure}");
    }
}

#[test]
fn the_reference_now_is_the_volume_that_was_read() {
    let base = settings(50.0, 100.0);
    let level = |volume: f64, muted| {
        Some(Level {
            linear: volume.powi(3),
            muted,
        })
    };
    assert_eq!(
        base.reference_now(level(0.35, false))
            .unwrap()
            .reference_pct,
        35.0
    );
    assert_eq!(
        base.reference_now(level(0.35, true)).unwrap().reference_pct,
        35.0
    );
    assert_eq!(
        base.reference_now(level(1.5, false)).unwrap().reference_pct,
        100.0
    );
    assert_eq!(
        base.reference_now(level(0.0, false)).unwrap().reference_pct,
        1.0
    );
    let failure = base.reference_now(None).unwrap_err();
    assert_eq!(failure.what, "the computer's volume has not been read yet");
    let nan = Some(Level {
        linear: f64::NAN,
        muted: false,
    });
    assert!(base.reference_now(nan).is_err());
}

#[test]
fn the_stage_never_asks_the_preamp_for_more() {
    // The preamp covers the chain's own peak; the loudness stage multiplies it by a response at
    // most 0 dB everywhere, so the product's peak is never above the chain's without it.
    use svolder_profiles::tone::apo::FilterKind;
    for rate in [44_100, 48_000, 96_000, 384_000] {
        // A real EQ: +6 dB low shelf at 60 Hz, +4 dB peak at 3 kHz, a -3 dB high shelf at 9 kHz.
        let chain: Vec<[f64; 5]> = vec![
            coefficients(FilterKind::LowShelf, 60.0, 6.0, 0.7, rate),
            coefficients(FilterKind::Peaking, 3_000.0, 4.0, 1.0, rate),
            coefficients(FilterKind::HighShelf, 9_000.0, -3.0, 0.7, rate),
        ];
        let alone = played_peak_db_of(&chain, &[], rate);
        for d_e in [0.0, 20.0, 40.0] {
            let mut with = chain.clone();
            with.extend(stage(d_e, rate).sections);
            let peak = played_peak_db_of(&with, &[], rate);
            assert!(
                peak <= alone + 1e-6,
                "{rate} d_e {d_e}: {peak} over {alone}"
            );
        }
    }
}

#[test]
fn the_curve_has_61_log_spaced_points_and_is_the_played_stage() {
    let points = curve(13.7, 48_000);
    assert_eq!(points.len(), CURVE_POINTS);
    assert!(near(points[0].0, 20.0, 1e-9));
    assert!(near(points[60].0, 20_000.0, 1e-6));
    assert!(near(points[20].0, 200.0, 1e-6));
    assert!(points.windows(2).all(|pair| pair[1].0 > pair[0].0));
    assert!(points.iter().all(|point| point.1 <= 1e-9));
    let played = stage(13.7, 48_000);
    assert_eq!(points[30].1, played.response_db(points[30].0, 48_000));
}

#[test]
fn the_view_reads_the_worked_example() {
    let level = Some(Level {
        linear: 0.35f64.powi(3),
        muted: false,
    });
    let view = LoudnessView::of(&settings(50.0, 100.0), level, 13.7, 48_000, None, None);
    assert!(view.on && view.following);
    assert_eq!((view.strength_pct, view.reference_pct), (50.0, 100.0));
    assert_eq!(view.volume_pct, Some(35.0));
    assert_eq!(view.depth_db, 13.7);
    assert_eq!(
        (view.lift_db, view.mids_db, view.treble_db),
        (7.3, -7.3, 2.1)
    );
    assert_eq!(view.curve.hz.len(), CURVE_POINTS);
    assert_eq!(view.curve.db.len(), CURVE_POINTS);
    assert_eq!(
        view.ranges.strength_pct,
        Range {
            min: 0.0,
            max: 100.0,
            step: 5.0
        }
    );
    assert_eq!(
        view.ranges.reference_pct,
        Range {
            min: 1.0,
            max: 100.0,
            step: 1.0
        }
    );
}

#[test]
fn the_view_at_the_reference_is_flat() {
    let level = Some(Level {
        linear: 1.0,
        muted: false,
    });
    let view = LoudnessView::of(&settings(50.0, 100.0), level, 0.0, 48_000, None, None);
    assert_eq!(
        (view.depth_db, view.lift_db, view.treble_db),
        (0.0, 0.0, 0.0)
    );
    assert!(view.mids_db == 0.0);
    assert!(view.curve.db.iter().all(|db| *db == 0.0));
    assert!(view.following);
}

#[test]
fn the_view_does_not_follow_while_off_muted_unread_or_unavailable() {
    let on = settings(50.0, 100.0);
    let loud = Some(Level {
        linear: 0.1,
        muted: false,
    });
    let view = |settings: &LoudnessSettings, level, unavailable: Option<&str>| {
        LoudnessView::of(
            settings,
            level,
            5.0,
            48_000,
            None,
            unavailable.map(str::to_string),
        )
    };
    assert!(view(&on, loud, None).following);
    assert!(!view(&LoudnessSettings { on: false, ..on }, loud, None).following);
    assert!(
        !view(
            &on,
            Some(Level {
                linear: 0.1,
                muted: true
            }),
            None
        )
        .following
    );
    assert!(!view(&on, None, None).following);
    assert_eq!(view(&on, None, None).volume_pct, None);
    let waiting = view(&on, loud, Some("a chain this old plays no loudness"));
    assert!(!waiting.following);
    assert_eq!(
        waiting.unavailable.as_deref(),
        Some("a chain this old plays no loudness")
    );
}

#[test]
fn a_view_without_its_fields_parses_and_one_with_them_round_trips() {
    let empty: LoudnessView = serde_json::from_str("{}").unwrap();
    assert_eq!(empty, LoudnessView::default());
    assert!(empty.volume_pct.is_none());
    assert_eq!(empty.ranges, LoudnessRanges::default());
    let level = Some(Level {
        linear: 0.2,
        muted: false,
    });
    let full = LoudnessView::of(
        &settings(75.0, 90.0),
        level,
        21.3,
        96_000,
        Some("note".into()),
        None,
    );
    let json = serde_json::to_string(&full).unwrap();
    assert_eq!(serde_json::from_str::<LoudnessView>(&json).unwrap(), full);
}

#[test]
fn a_flat_view_has_no_negative_zero_in_its_json() {
    let view = LoudnessView::of(&LoudnessSettings::default(), None, 0.0, 48_000, None, None);
    let text = serde_json::to_string(&view).unwrap();
    assert!(!text.contains("-0.0"), "{text}");
    assert!(text.contains("\"mids_db\":0.0"), "{text}");
}
