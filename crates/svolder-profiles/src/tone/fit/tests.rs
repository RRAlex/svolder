// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;
use crate::test_support::tone;
use crate::tone::apo::SourceFilter;

fn five() -> EqSpec {
    tone("mesh-eq-5")
}

fn three() -> EqSpec {
    tone("mesh-eq-3")
}

fn pk(f: f64, g: f64, q: f64) -> SourceFilter {
    SourceFilter {
        kind: FilterKind::Peaking,
        f,
        g,
        q,
    }
}

fn lsc(f: f64, g: f64, q: f64) -> SourceFilter {
    SourceFilter {
        kind: FilterKind::LowShelf,
        f,
        g,
        q,
    }
}

fn hsc(f: f64, g: f64, q: f64) -> SourceFilter {
    SourceFilter {
        kind: FilterKind::HighShelf,
        f,
        g,
        q,
    }
}

fn correction(filters: Vec<SourceFilter>) -> Correction {
    Correction {
        preamp_db: -3.0,
        filters,
    }
}

/// An AutoEq-like correction: a bass shelf, peaks and notches through the mids and a treble
/// shelf.
fn autoeq_like() -> Correction {
    correction(vec![
        lsc(105.0, 5.5, 0.7),
        pk(200.0, -2.1, 0.9),
        pk(600.0, 1.8, 1.2),
        pk(1500.0, -3.0, 2.0),
        pk(3200.0, 4.2, 2.5),
        pk(6200.0, -5.1, 3.0),
        pk(9000.0, 1.9, 2.0),
        hsc(10_000.0, -4.0, 0.7),
    ])
}

/// Every value of every band is on one of the device's raw steps and inside the band's window.
fn assert_on_device(eq: &EqSpec, fit: &Fit, what: &str) {
    assert_eq!(fit.bands.len(), eq.bands.len(), "{what}");
    for (band, b) in fit.bands.iter().enumerate() {
        assert!(
            b.f.is_finite() && b.g.is_finite() && b.q.is_finite(),
            "{what}: {b:?}"
        );
        let raw = eq.frequency_raw(band, b.f).unwrap();
        assert_eq!(
            eq.frequency(band, raw),
            Some(b.f),
            "{what}: band {band} f {}",
            b.f
        );
        assert_eq!(
            eq.gain_db(eq.gain_raw(b.g)),
            b.g,
            "{what}: band {band} g {}",
            b.g
        );
        assert_eq!(eq.q(eq.q_raw(b.q)), b.q, "{what}: band {band} q {}", b.q);
        let (low, high) = (
            eq.frequency(band, 0).unwrap(),
            eq.frequency(band, 127).unwrap(),
        );
        assert!(
            (low..=high).contains(&b.f),
            "{what}: band {band} f {} outside {low}..{high}",
            b.f
        );
        assert!(
            (eq.gain_range.0..=eq.gain_range.1).contains(&b.g),
            "{what}: {b:?}"
        );
        assert!(
            (eq.q_range.0..=eq.q_range.1).contains(&b.q),
            "{what}: {b:?}"
        );
    }
    for number in [fit.rms_db, fit.worst_db, fit.worst_hz, fit.peak_db] {
        assert!(number.is_finite(), "{what}: {fit:?}");
    }
}

/// The five-band centres, as the flat preset has them.
fn centres(eq: &EqSpec) -> Vec<f64> {
    (0..eq.bands.len())
        .map(|band| eq.frequency(band, 64).unwrap())
        .collect()
}

#[test]
fn a_correction_that_is_one_filter_per_band_maps_straight_across() {
    let eq = five();
    let c = correction(vec![
        pk(2510.0, -3.7, 3.3),
        pk(45.3, 3.1, 0.83),
        pk(8010.0, 2.6, 1.9),
        pk(503.0, 4.3, 2.1),
        pk(150.0, -2.2, 1.37),
    ]);
    let fit = fit(&eq, &c);
    assert_on_device(&eq, &fit, "direct");
    // Sorted ascending, filter i is band i, each moved by no more than a step.
    for (band, (want_f, want_g, want_q)) in [
        (45.3, 3.1, 0.83),
        (150.0, -2.2, 1.37),
        (503.0, 4.3, 2.1),
        (2510.0, -3.7, 3.3),
        (8010.0, 2.6, 1.9),
    ]
    .into_iter()
    .enumerate()
    {
        let b = fit.bands[band];
        assert!(
            (b.f / want_f - 1.0).abs() < 0.007,
            "band {band}: {} Hz for {want_f}",
            b.f
        );
        assert!(
            (b.g - want_g).abs() <= 0.11,
            "band {band}: {} dB for {want_g}",
            b.g
        );
        assert!(
            (b.q - want_q).abs() <= 0.05,
            "band {band}: Q {} for {want_q}",
            b.q
        );
    }
    assert!(fit.rms_db <= 0.15, "direct mapping RMS {}", fit.rms_db);
}

#[test]
fn fewer_filters_than_bands_leave_the_spare_bands_flat_on_their_centres() {
    let eq = five();
    let fit = fit(
        &eq,
        &correction(vec![pk(2510.0, -3.7, 3.3), pk(150.0, 2.2, 1.0)]),
    );
    assert_on_device(&eq, &fit, "spare");
    let middle = centres(&eq);
    for band in [0, 2, 4] {
        let b = fit.bands[band];
        assert_eq!(b.f, middle[band], "band {band}");
        assert!(b.g.abs() < 1e-9, "band {band} gain {}", b.g);
        assert_eq!(b.q, eq.q_center, "band {band}");
    }
    assert!((fit.bands[1].g - 2.2).abs() <= 0.11 && (fit.bands[3].g + 3.7).abs() <= 0.11);
    assert!(fit.rms_db <= 0.15, "RMS {}", fit.rms_db);
}

#[test]
fn a_three_band_device_maps_three_filters_directly() {
    let eq = three();
    let fit = fit(
        &eq,
        &correction(vec![
            pk(47.3, 3.1, 0.83),
            pk(503.0, 4.3, 2.1),
            pk(5010.0, 2.6, 1.9),
        ]),
    );
    assert_on_device(&eq, &fit, "direct 3");
    assert!(fit.rms_db <= 0.15, "RMS {}", fit.rms_db);
}

#[test]
fn five_off_grid_peaks_fit_within_a_third_of_a_db() {
    let eq = five();
    let c = correction(vec![
        pk(33.37, 4.13, 1.77),
        pk(171.3, -3.07, 0.93),
        pk(777.7, 2.51, 3.11),
        pk(3333.0, -4.49, 2.43),
        pk(12_345.0, 3.03, 1.21),
    ]);
    let fit = fit(&eq, &c);
    assert_on_device(&eq, &fit, "off grid");
    assert!(fit.rms_db <= 0.3, "RMS {}", fit.rms_db);
}

#[test]
fn two_peaks_in_one_window_run_the_solver_and_still_fit_well() {
    let eq = five();
    let c = correction(vec![
        pk(500.0, 4.0, 2.0),
        pk(700.0, -3.0, 2.0),
        pk(100.0, 2.0, 1.0),
        pk(2500.0, -3.7, 3.3),
        pk(8010.0, 2.6, 1.9),
    ]);
    let fit = fit(&eq, &c);
    assert_on_device(&eq, &fit, "two in one");
    assert!(fit.rms_db <= 0.8, "RMS {}", fit.rms_db);
}

#[test]
fn an_autoeq_like_mix_fits_five_bands_and_three_bands_within_their_bounds() {
    let c = autoeq_like();
    let five = five();
    let on_five = fit(&five, &c);
    assert_on_device(&five, &on_five, "autoeq 5");
    assert!(on_five.rms_db <= 1.5, "5 bands: {}", on_five.line());
    let three = three();
    let on_three = fit(&three, &c);
    assert_on_device(&three, &on_three, "autoeq 3");
    assert!(on_three.rms_db <= 1.6, "3 bands: {}", on_three.line());
    assert!(on_five.rms_db <= on_three.rms_db, "more bands fit no worse");
}

#[test]
fn a_flat_correction_fits_flat() {
    for eq in [five(), three()] {
        let fit = fit(&eq, &correction(vec![pk(100.0, 0.0, 1.0); 7]));
        assert_on_device(&eq, &fit, "flat");
        assert!(fit.rms_db < 0.05 && fit.worst_db < 0.1, "{}", fit.line());
        assert!(fit.bands.iter().all(|b| b.g == 0.0), "{:?}", fit.bands);
        assert_eq!(fit.peak_db, 0.0);
    }
}

#[test]
fn awkward_corrections_still_give_a_valid_fit() {
    let cases = [
        ("above every window", vec![pk(22_000.0, 6.0, 1.0)]),
        (
            "in the three-band gaps",
            vec![pk(200.0, 5.0, 2.0), pk(1_900.0, -5.0, 2.0)],
        ),
        ("gain beyond the device", vec![pk(1000.0, 12.0, 1.0)]),
        (
            "shelves alone",
            vec![lsc(60.0, 8.0, 0.7), hsc(9000.0, -8.0, 0.7)],
        ),
        ("a filter at 10 Hz", vec![pk(10.0, 20.0, 20.0)]),
        (
            "only cuts",
            vec![pk(1000.0, -30.0, 0.05), pk(1000.0, -30.0, 20.0)],
        ),
    ];
    for (name, filters) in cases {
        let c = correction(filters);
        for eq in [five(), three()] {
            let fit = fit(&eq, &c);
            assert_on_device(&eq, &fit, name);
            assert_eq!(fit, super::fit(&eq, &c), "{name}: a second run differs");
        }
    }
}

/// A tiny linear congruential generator: the same numbers on every run.
struct Lcg(u64);

impl Lcg {
    /// The next number in `[0, 1)`.
    fn next(&mut self) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    fn between(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.next()
    }

    fn log_between(&mut self, low: f64, high: f64) -> f64 {
        self.between(low.ln(), high.ln()).exp()
    }
}

fn synthetic(rng: &mut Lcg) -> Correction {
    let count = 1 + (rng.next() * 12.0) as usize;
    correction(
        (0..count)
            .map(|_| SourceFilter {
                kind: match rng.next() {
                    r if r < 0.15 => FilterKind::LowShelf,
                    r if r < 0.3 => FilterKind::HighShelf,
                    _ => FilterKind::Peaking,
                },
                f: rng.log_between(20.0, 16_000.0),
                g: rng.between(-9.0, 9.0),
                q: rng.log_between(0.4, 5.0),
            })
            .collect(),
    )
}

#[test]
fn two_hundred_seeded_corrections_fit_validly_and_the_same_every_time() {
    let mut rng = Lcg(0x5eed);
    let (five, three) = (five(), three());
    for case in 0..200 {
        let c = synthetic(&mut rng);
        let (eq, name) = match case % 2 {
            0 => (&five, "five"),
            _ => (&three, "three"),
        };
        let what = format!("case {case} on {name}");
        let first = fit(eq, &c);
        assert_on_device(eq, &first, &what);
        assert_eq!(first, fit(eq, &c), "{what}: a second run differs");
        assert!(first.line().starts_with("fit "), "{what}: {}", first.line());
    }
}

#[test]
fn a_fit_is_never_worse_than_leaving_the_bands_flat() {
    let mut rng = Lcg(0xf1a7);
    let eq = five();
    for case in 0..20 {
        let c = synthetic(&mut rng);
        let fit = fit(&eq, &c);
        let freqs = log_freqs(POINTS);
        let target = correction_db(&c, &freqs);
        let flat_rms = (target
            .iter()
            .zip(&freqs)
            .filter(|(_, f)| **f <= TRUSTED_HZ)
            .map(|(t, _)| t * t)
            .sum::<f64>()
            / freqs.iter().filter(|f| **f <= TRUSTED_HZ).count() as f64)
            .sqrt();
        assert!(
            fit.rms_db <= flat_rms + 0.05,
            "case {case}: {} vs flat {flat_rms}",
            fit.rms_db
        );
    }
}

#[test]
fn the_logistic_maps_into_the_range_and_back() {
    for axis in [
        Axis {
            low: 20.0,
            high: 78.0,
            log: true,
        },
        Axis {
            low: -20.0,
            high: 6.0,
            log: false,
        },
    ] {
        for x in [-1000.0, -5.0, 0.0, 2.5, 1000.0] {
            let v = axis.value(x);
            assert!((axis.low..=axis.high).contains(&v), "{v} from {x}");
        }
        for t in [0.1, 0.5, 0.9] {
            let v = match axis.log {
                true => (axis.low.ln() + t * (axis.high.ln() - axis.low.ln())).exp(),
                false => axis.low + t * (axis.high - axis.low),
            };
            assert!((axis.value(axis.free(v)) / v - 1.0).abs() < 1e-9);
        }
        assert!(axis.free(axis.low).is_finite() && axis.free(axis.high).is_finite());
    }
}

fn result(rms_db: f64, worst_db: f64, worst_hz: f64, peak_db: f64) -> Fit {
    Fit {
        bands: Vec::new(),
        rms_db,
        worst_db,
        worst_hz,
        peak_db,
    }
}

#[test]
fn the_line_names_the_error_and_the_volume_to_lower_only_for_a_real_boost() {
    assert_eq!(
        result(0.93, 2.44, 8000.0, 3.5).line(),
        "fit 0.9 dB RMS · worst 2.4 dB at 8.0 kHz · lower volume 3.5 dB"
    );
    assert_eq!(
        result(0.93, 2.44, 8000.0, 0.0).line(),
        "fit 0.9 dB RMS · worst 2.4 dB at 8.0 kHz"
    );
    assert_eq!(
        result(0.93, 2.44, 8000.0, -1.0).line(),
        "fit 0.9 dB RMS · worst 2.4 dB at 8.0 kHz"
    );
    assert!(
        result(0.0, 0.0, 20.0, PEAK_SHOWN_DB)
            .line()
            .ends_with("lower volume 0.1 dB")
    );
    assert!(
        !result(0.0, 0.0, 20.0, PEAK_SHOWN_DB - 0.001)
            .line()
            .contains("lower")
    );
    assert_eq!(
        result(0.0, 0.0, 40.4, 0.0).line(),
        "fit 0.0 dB RMS · worst 0.0 dB at 40.4 Hz"
    );
    assert_eq!(
        result(1.0, 1.0, 523.0, 0.0).line(),
        "fit 1.0 dB RMS · worst 1.0 dB at 523 Hz"
    );
}

#[test]
fn the_direct_path_is_taken_only_for_one_peaking_filter_per_band() {
    let eq = five();
    let direct = |filters: Vec<SourceFilter>| {
        let c = correction(filters);
        Search::new(&eq, &c).direct().is_some()
    };
    let base = vec![
        pk(45.3, 3.1, 0.83),
        pk(150.0, -2.2, 1.37),
        pk(503.0, 4.3, 2.1),
    ];
    assert!(direct(base.clone()));
    let with = |extra: SourceFilter| {
        let mut filters = base.clone();
        filters.push(extra);
        filters
    };
    assert!(direct(with(pk(2510.0, -3.7, 3.3))));
    assert!(!direct(with(lsc(2510.0, -3.7, 0.7))), "a shelf");
    assert!(
        !direct(with(pk(520.0, 1.0, 1.0))),
        "two filters in one window"
    );
    assert!(!direct(with(pk(2510.0, 7.0, 3.3))), "a gain out of range");
    assert!(!direct(with(pk(2510.0, 1.0, 9.0))), "a Q out of range");
    assert!(!direct(with(pk(2510.0, 1.0, 0.4))), "a Q below range");
    assert!(
        !direct(with(pk(80.0, 1.0, 1.0))),
        "a frequency between windows"
    );
    let mut six = base;
    six.extend([
        pk(2510.0, 1.0, 1.0),
        pk(8010.0, 1.0, 1.0),
        pk(12_000.0, 1.0, 1.0),
    ]);
    assert!(!direct(six), "more filters than bands");
}

#[test]
fn a_reversed_or_nan_axis_never_panics() {
    for (a, b) in [(8.0, 0.5), (0.5, f64::NAN), (f64::NAN, f64::NAN)] {
        let axis = Axis::new(a, b, true);
        let _ = axis.clamp(3.0);
        let _ = axis.value(0.3);
    }
    let axis = Axis::new(8.0, 0.5, false);
    assert_eq!((axis.low, axis.high), (0.5, 8.0));
}
