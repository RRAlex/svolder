// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The rates and shapes at which the stage can play, against a dense evaluation.

use super::*;

/// The coefficients `[b0, b1, b2, a1, a2]` of one held filter from the audio-EQ cookbook,
/// written out here and divided by `a0`, then rounded to f32 as PipeWire runs them.
fn played_coefficients(filter: &SourceFilter, rate_hz: u32) -> [f64; 5] {
    let filter = live::held(filter, rate_hz);
    let rate = f64::from(rate_hz);
    let a = 10f64.powf(filter.g / 40.0);
    let w0 = 2.0 * std::f64::consts::PI * filter.f / rate;
    let (c, s) = (w0.cos(), w0.sin());
    let alpha = s / (2.0 * filter.q);
    let beta = 2.0 * a.sqrt() * alpha;
    let (b, d) = match filter.kind {
        FilterKind::Peaking => (
            [1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a],
            [1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a],
        ),
        FilterKind::LowShelf => (
            [
                a * ((a + 1.0) - (a - 1.0) * c + beta),
                2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                a * ((a + 1.0) - (a - 1.0) * c - beta),
            ],
            [
                (a + 1.0) + (a - 1.0) * c + beta,
                -2.0 * ((a - 1.0) + (a + 1.0) * c),
                (a + 1.0) + (a - 1.0) * c - beta,
            ],
        ),
        FilterKind::HighShelf => (
            [
                a * ((a + 1.0) + (a - 1.0) * c + beta),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                a * ((a + 1.0) + (a - 1.0) * c - beta),
            ],
            [
                (a + 1.0) - (a - 1.0) * c + beta,
                2.0 * ((a - 1.0) - (a + 1.0) * c),
                (a + 1.0) - (a - 1.0) * c - beta,
            ],
        ),
    };
    [
        b[0] / d[0],
        b[1] / d[0],
        b[2] / d[0],
        d[1] / d[0],
        d[2] / d[0],
    ]
    .map(|value| f64::from(value as f32))
}

/// Whether the poles of `1 + a1 z^-1 + a2 z^-2` are inside the unit circle.
fn is_stable(c: &[f64; 5]) -> bool {
    c[4].abs() < 1.0 && c[3].abs() < 1.0 + c[4]
}

/// The sum in dB of the filters' responses at `w`, by `|p0 + p1 z + p2 z^2|^2` written in
/// `sin^2(w/2)` below half of Nyquist's w and in `cos^2(w/2)` above.
fn summed_db(sets: &[[f64; 5]], w: f64) -> f64 {
    let (sign, x) = match w <= std::f64::consts::FRAC_PI_2 {
        true => (1.0, (w / 2.0).sin().powi(2)),
        false => (-1.0, (w / 2.0).cos().powi(2)),
    };
    let power = |p: [f64; 3]| {
        let sum = p[0] + p[1] + p[2];
        sum * sum - 4.0 * (p[0] * p[1] + 4.0 * p[0] * p[2] + p[1] * p[2]) * x
            + 16.0 * p[0] * p[2] * x * x
    };
    sets.iter()
        .map(|c| {
            10.0 * (power([c[0], sign * c[1], c[2]]) / power([1.0, sign * c[3], c[4]])).log10()
        })
        .sum()
}

/// The highest the played (f32) filters rise anywhere from 0.01 Hz to Nyquist, DC and Nyquist
/// included, and where: a log grid in frequency, a second log grid in the distance from
/// Nyquist and each filter's centre, with every local maximum zoomed in on by repeated scans.
fn dense_peak(filters: &[SourceFilter], rate_hz: u32) -> (f64, f64) {
    use std::f64::consts::PI;
    let rate = f64::from(rate_hz);
    let sets: Vec<[f64; 5]> = filters
        .iter()
        .map(|f| played_coefficients(f, rate_hz))
        .collect();
    let total = |w: f64| summed_db(&sets, w);
    let mut ws = vec![0.0, PI];
    let (low, high) = (0.01f64, rate / 2.0 * 0.99999);
    let steps = 8192;
    ws.extend(
        (0..=steps)
            .map(|i| 2.0 * PI * low * (high / low).powf(f64::from(i) / f64::from(steps)) / rate),
    );
    let nyquist_steps = 4096;
    let (near, far) = (PI * 1e-8, PI / 2.0);
    ws.extend(
        (0..=nyquist_steps)
            .map(|i| PI - near * (far / near).powf(f64::from(i) / f64::from(nyquist_steps))),
    );
    ws.extend(
        filters
            .iter()
            .map(|f| (2.0 * PI * live::held(f, rate_hz).f / rate).min(PI)),
    );
    ws.sort_by(f64::total_cmp);
    ws.dedup();
    let values: Vec<f64> = ws.iter().map(|&w| total(w)).collect();
    let last = ws.len() - 1;
    let mut best = (f64::NEG_INFINITY, 0.0);
    for i in 0..=last {
        if values[i] > best.0 {
            best = (values[i], ws[i]);
        }
        let top = i == 0 || values[i] > values[i - 1];
        if !(top && (i == last || values[i] >= values[i + 1])) {
            continue;
        }
        let (mut from, mut to) = (ws[i.saturating_sub(1)], ws[(i + 1).min(last)]);
        for _ in 0..4 {
            let scan = 64;
            let mut here = (f64::NEG_INFINITY, from);
            for k in 0..=scan {
                let w = from + (to - from) * f64::from(k) / f64::from(scan);
                let value = total(w);
                if value > here.0 {
                    here = (value, w);
                }
            }
            if here.0 > best.0 {
                best = here;
            }
            let width = (to - from) / f64::from(scan);
            (from, to) = ((here.1 - width).max(0.0), (here.1 + width).min(PI));
        }
    }
    (best.0, best.1 * rate / (2.0 * PI))
}

fn filter(kind: FilterKind, f: f64, g: f64, q: f64) -> SourceFilter {
    SourceFilter { kind, f, g, q }
}

/// A cookbook low shelf at 20 Hz, +12 dB, Q 10 resonates well above its stated gain below its
/// corner, at about 14 Hz.
#[test]
fn the_dense_evaluator_sees_the_low_shelf_resonance_at_14_hz() {
    let (peak, at) = dense_peak(&[filter(FilterKind::LowShelf, 20.0, 12.0, 10.0)], 48_000);
    assert!((29.0..30.5).contains(&peak), "{peak} dB at {at} Hz");
    assert!((12.0..16.0).contains(&at), "{at} Hz");
}

/// The evaluator reads a peak five hertz wide next to Nyquist at its true height.
#[test]
fn the_dense_evaluator_sees_a_narrow_peak_beside_nyquist() {
    let (peak, at) = dense_peak(
        &[
            filter(FilterKind::Peaking, 23_890.0, 27.0, 20.0),
            filter(FilterKind::HighShelf, 21_810.0, 30.0, 0.06),
        ],
        48_000,
    );
    assert!((50.4..50.8).contains(&peak), "{peak} dB at {at} Hz");
}

/// What a stack must satisfy: refused, or its preamp plus the peak of the played (f32)
/// coefficients is at most +0.01 dB. A stack with an unstable filter must be refused, and
/// none may be accepted below the preamp floor. Returns the line to report when it does not.
fn over_unity(name: &str, rate: u32, filters: &[SourceFilter]) -> Option<String> {
    let correction = Correction {
        preamp_db: 0.0,
        filters: filters.to_vec(),
    };
    let unstable = filters
        .iter()
        .any(|f| !is_stable(&played_coefficients(f, rate)));
    match check_playable(&correction, rate) {
        Err(_) => None,
        Ok(()) if unstable => Some(format!("{rate} {name}: an unstable filter was accepted")),
        Ok(()) => {
            let preamp = played_preamp(&correction, rate);
            let (peak, at) = dense_peak(filters, rate);
            (preamp + peak > 0.01).then(|| {
                format!("{rate} {name}: preamp {preamp} dB, played peak {peak} dB at {at} Hz")
            })
        }
    }
}

/// The preamp plus the curve as played over the whole band the chain plays, DC to Nyquist, is
/// never above unity: for the corners a hand can set and the extremes a preset can hold, alone
/// and sixteen deep, at the rates the chain runs at.
#[test]
fn the_preamp_keeps_the_whole_played_band_under_unity() {
    let kinds = [
        FilterKind::LowShelf,
        FilterKind::HighShelf,
        FilterKind::Peaking,
    ];
    let mut cases: Vec<(String, SourceFilter)> = vec![];
    for kind in kinds {
        for f in [20.0, 20_000.0] {
            for q in [0.1, 0.707, 10.0] {
                cases.push((
                    format!("{kind:?} {f} Hz +12 Q{q}"),
                    filter(kind, f, 12.0, q),
                ));
            }
        }
    }
    for (kind, f, g, q) in [
        (FilterKind::Peaking, 10.0, 30.0, 20.0),
        (FilterKind::HighShelf, 24_000.0, 30.0, 0.707),
        (FilterKind::HighShelf, 24_000.0, 30.0, 20.0),
        (FilterKind::LowShelf, 10.0, 30.0, 0.707),
        (FilterKind::LowShelf, 10.0, 30.0, 20.0),
        (FilterKind::Peaking, 20.0, 12.0, 10.0),
    ] {
        cases.push((
            format!("{kind:?} {f} Hz {g:+} Q{q} (preset)"),
            filter(kind, f, g, q),
        ));
    }
    let mut over: Vec<String> = vec![];
    for rate in [44_100u32, 48_000, 88_200, 96_000, 192_000] {
        for (name, one) in &cases {
            for count in [1usize, 16] {
                let filters = vec![one.clone(); count];
                over.extend(over_unity(&format!("x{count} {name}"), rate, &filters));
            }
        }
    }
    assert!(
        over.is_empty(),
        "{} cases over unity:\n{}",
        over.len(),
        over.join("\n")
    );
}

/// Two narrow filters next to Nyquist that a sparse grid steps over.
#[test]
fn a_narrow_peak_beside_nyquist_is_covered_by_the_preamp() {
    let at_48k = [
        filter(FilterKind::Peaking, 23_890.0, 27.0, 20.0),
        filter(FilterKind::HighShelf, 21_810.0, 12.0, 0.06),
    ];
    let at_44k = [
        filter(FilterKind::LowShelf, 23_746.0, 5.0, 0.82),
        filter(FilterKind::Peaking, 23_928.0, 22.0, 20.0),
    ];
    for (name, rate, filters) in [
        ("48 kHz pair", 48_000, at_48k),
        ("44.1 kHz pair", 44_100, at_44k),
    ] {
        let correction = Correction {
            preamp_db: 0.0,
            filters: filters.to_vec(),
        };
        assert_eq!(
            check_playable(&correction, rate).map_err(|e| e.what),
            Ok(()),
            "{name}"
        );
        assert_eq!(over_unity(name, rate, &filters), None);
        let (peak, _) = dense_peak(&filters, rate);
        let preamp = played_preamp(&correction, rate);
        assert!(
            peak > 20.0 && preamp <= -peak,
            "{name}: {preamp} dB for a {peak} dB peak"
        );
    }
}

/// PipeWire clamps a biquad coefficient to plus or minus 10 (read back from a private
/// instance: 10.5 reads 10, -31.6 reads -10), so a filter that needs more is refused.
#[test]
fn a_filter_with_a_coefficient_past_ten_is_refused() {
    let one = |kind, f, g, q| Correction {
        preamp_db: 0.0,
        filters: vec![filter(kind, f, g, q)],
    };
    let failure =
        check_playable(&one(FilterKind::HighShelf, 1000.0, 30.0, 0.707), 48_000).unwrap_err();
    assert!(
        failure.what.contains("point 1 boosts more than"),
        "{}",
        failure.what
    );
    // What a hand can set stays inside.
    for kind in [
        FilterKind::LowShelf,
        FilterKind::HighShelf,
        FilterKind::Peaking,
    ] {
        for rate in [44_100, 48_000] {
            for f in [20.0, 1000.0, 20_000.0] {
                let correction = one(kind, f, 12.0, 0.707);
                assert_eq!(
                    check_playable(&correction, rate).map_err(|e| e.what),
                    Ok(()),
                    "{kind:?} {f} Hz at {rate}"
                );
            }
        }
    }
}

/// A seeded xorshift stream of numbers in `[0, 1)`.
struct Stream(u64);

impl Stream {
    fn next(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }

    fn log(&mut self, from: f64, to: f64) -> f64 {
        (from.ln() + (to.ln() - from.ln()) * self.next()).exp()
    }

    fn filter(&mut self, rate: f64) -> SourceFilter {
        let kind = match (self.next() * 3.0) as u8 {
            0 => FilterKind::Peaking,
            1 => FilterKind::LowShelf,
            _ => FilterKind::HighShelf,
        };
        let f = match self.next() {
            zone if zone < 0.3 => self.log(10.0, 40.0),
            zone if zone < 0.65 => self.log((0.42 * rate).min(21_000.0), 24_000.0),
            _ => self.log(10.0, 24_000.0),
        };
        let g = match self.next() {
            edge if edge < 0.3 => 30.0 * if self.next() < 0.5 { 1.0 } else { -1.0 },
            _ => (self.next() * 2.0 - 1.0) * 30.0,
        };
        let q = match self.next() {
            edge if edge < 0.3 => 20.0,
            _ => self.log(0.02, 20.0),
        };
        filter(kind, f, g, q)
    }
}

/// Random mixed stacks, as many filters as a preset or a file can bring, at every rate: each is
/// refused or is kept under unity as played.
#[test]
fn random_mixed_stacks_stay_under_unity_as_played() {
    let mut stream = Stream(0x9e37_79b9_7f4a_7c15);
    let rates = [44_100u32, 48_000, 88_200, 96_000, 192_000];
    let mut over: Vec<String> = vec![];
    let mut accepted = 0;
    for case in 0..2500 {
        let rate = rates[case % rates.len()];
        let count = 1 + (stream.next() * 16.0) as usize;
        // Filters the chain can play one by one, so a stack is not refused for one of them.
        let mut filters: Vec<SourceFilter> = vec![];
        while filters.len() < count {
            let one = stream.filter(f64::from(rate));
            let alone = Correction {
                preamp_db: 0.0,
                filters: vec![one.clone()],
            };
            if check_playable(&alone, rate).is_ok() {
                filters.push(one);
            }
        }
        let correction = Correction {
            preamp_db: 0.0,
            filters: filters.clone(),
        };
        accepted += usize::from(check_playable(&correction, rate).is_ok());
        assert!(
            svolder_profiles::tone::apo::validate(&correction).is_ok(),
            "{filters:?}"
        );
        over.extend(over_unity(
            &format!("case {case} {filters:?}"),
            rate,
            &filters,
        ));
    }
    assert!(
        accepted >= 1500,
        "only {accepted} of 2500 stacks were accepted"
    );
    assert!(
        over.is_empty(),
        "{} stacks over unity:\n{}",
        over.len(),
        over.join("\n")
    );
}

/// What PipeWire cannot hold: a filter that is not stable in its f32 coefficients at the
/// running rate, or a curve that needs a preamp below the floor.
#[tokio::test]
async fn a_curve_the_chain_cannot_play_is_refused_before_anything_changes() {
    let f = Fixture::new();
    let host = f.host(Manager::Systemd);
    let mut unstable = request("Unstable");
    unstable.rate_hz = Some(96_000);
    unstable.correction.filters = vec![filter(FilterKind::LowShelf, 10.0, 30.0, 20.0)];
    let failure = host.apply(unstable).await.unwrap_err();
    assert!(
        failure.what.contains("point 1 would not be stable"),
        "{}",
        failure.what
    );
    let mut too_deep = request("Deep");
    too_deep.correction.filters = vec![filter(FilterKind::LowShelf, 20.0, 12.0, 10.0); 16];
    let failure = host.apply(too_deep).await.unwrap_err();
    assert!(
        failure
            .what
            .contains("the computer EQ plays curves up to 120 dB"),
        "{}",
        failure.what
    );
    assert_eq!(f.written(), Vec::<String>::new());
    assert_eq!(f.systemctl_writes(), Vec::<String>::new());
}

/// The same refusals, asked of the check itself, and what it lets through.
#[test]
fn check_playable_refuses_unstable_f32_filters_and_curves_below_the_floor() {
    let one = |kind, f, g, q| Correction {
        preamp_db: 0.0,
        filters: vec![filter(kind, f, g, q)],
    };
    assert!(check_playable(&one(FilterKind::LowShelf, 10.0, 30.0, 20.0), 96_000).is_err());
    assert!(check_playable(&one(FilterKind::LowShelf, 10.0, 30.0, 0.707), 192_000).is_err());
    assert!(check_playable(&one(FilterKind::LowShelf, 10.0, 30.0, 0.707), 88_200).is_err());
    assert!(check_playable(&one(FilterKind::Peaking, 10.0, 30.0, 20.0), 192_000).is_err());
    assert!(check_playable(&one(FilterKind::HighShelf, 10.0, -30.0, 0.707), 192_000).is_err());
    for (rate, kind, f, g, q) in [
        (88_200, FilterKind::LowShelf, 20.0, 12.0, 0.707),
        (96_000, FilterKind::Peaking, 20.0, 1.5, 10.0),
        (192_000, FilterKind::Peaking, 100.0, 12.0, 10.0),
        (48_000, FilterKind::Peaking, 1000.0, 3.0, 1.0),
    ] {
        assert_eq!(
            check_playable(&one(kind, f, g, q), rate).map_err(|e| e.what),
            Ok(()),
            "{kind:?} {f} Hz {g} dB Q{q} at {rate}"
        );
    }
    let deep = Correction {
        preamp_db: 0.0,
        filters: vec![filter(FilterKind::LowShelf, 20.0, 12.0, 10.0); 16],
    };
    assert!(check_playable(&deep, 48_000).is_err());
}

/// Every corner of what a hand sets, at every rate the sink can run at, is playable.
#[test]
fn every_hand_corner_is_playable_at_every_rate() {
    use crate::eq::HAND;
    for rate in [44_100, 48_000, 88_200, 96_000, 176_400, 192_000] {
        for kind in [
            FilterKind::Peaking,
            FilterKind::LowShelf,
            FilterKind::HighShelf,
        ] {
            for f in [HAND.f.0, 31.5, 1000.0, HAND.f.1] {
                for g in [HAND.g.0, HAND.g.1] {
                    for q in [HAND.q.0, 0.707, HAND.q.1] {
                        let correction = Correction {
                            preamp_db: 0.0,
                            filters: vec![filter(kind, f, g, q)],
                        };
                        assert_eq!(
                            check_playable(&correction, rate).map_err(|e| e.what),
                            Ok(()),
                            "{kind:?} {f} Hz {g} dB Q{q} at {rate}"
                        );
                    }
                }
            }
        }
    }
}

/// A filter of 0 dB is the identity: never refused, however low its frequency and sharp its Q,
/// so a lone point can always be removed.
#[test]
fn a_filter_of_zero_db_is_playable_at_any_rate() {
    for rate in [44_100, 48_000, 96_000, 176_400, 192_000] {
        for kind in [
            FilterKind::Peaking,
            FilterKind::LowShelf,
            FilterKind::HighShelf,
        ] {
            for (f, q) in [(1.0, 100.0), (10.0, 20.0), (20.0, 0.02), (20.0, 20.0)] {
                let correction = Correction {
                    preamp_db: 0.0,
                    filters: vec![filter(kind, f, 0.0, q)],
                };
                assert_eq!(
                    check_playable(&correction, rate).map_err(|e| e.what),
                    Ok(()),
                    "{kind:?} {f} Hz 0 dB Q{q} at {rate}"
                );
            }
        }
    }
}

/// A wide peak past the coefficient range is a peak, not a shelf, in the refusal.
#[test]
fn the_coefficient_refusal_names_the_point_and_does_not_call_a_peak_a_shelf() {
    let correction = Correction {
        preamp_db: 0.0,
        filters: vec![
            filter(FilterKind::Peaking, 1000.0, 0.0, 1.0),
            filter(FilterKind::Peaking, 1000.0, 30.0, 0.02),
        ],
    };
    let failure = check_playable(&correction, 48_000).unwrap_err();
    assert!(
        failure.what.starts_with("point 2 boosts more than"),
        "{failure}"
    );
    assert!(!failure.what.contains("shelf") && !failure.what.contains("filter"));
}
