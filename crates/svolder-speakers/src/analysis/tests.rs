// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const RATE: u32 = 48_000;

fn ir(len: usize, hits: &[(usize, f32)]) -> Vec<f32> {
    let mut taps = vec![0.0; len];
    for &(at, gain) in hits {
        taps[at] += gain;
    }
    taps
}

/// Near speakers at 0 ms (gain 1 and 0.8), far ones at 12 samples = 0.25 ms (gain 0.5 and 0.4).
fn diracs(extra_left: &[(usize, f32)]) -> [Vec<f32>; 4] {
    let mut ll = vec![(0, 1.0)];
    ll.extend_from_slice(extra_left);
    [
        ir(4800, &ll),
        ir(4800, &[(12, 0.5)]),
        ir(4800, &[(12, 0.4)]),
        ir(4800, &[(0, 0.8)]),
    ]
}

fn db(ratio: f64) -> f64 {
    20.0 * ratio.log10()
}

fn near(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn direct_arrivals_sit_at_their_delays_and_levels() {
    let t = analyse(&diracs(&[]), RATE, false);
    let l = &t.arrivals.left;
    assert_eq!(l.len(), 2, "{l:?}");
    assert!(
        near(l[0].ms, 0.0, 1e-9) && near(l[0].db, 0.0, 0.01),
        "{l:?}"
    );
    assert!(
        near(l[1].ms, 0.25, 1e-6) && near(l[1].db, db(0.4), 0.01),
        "{l:?}"
    );
    let r = &t.arrivals.right;
    assert_eq!(r.len(), 2, "{r:?}");
    assert!(
        near(r[0].ms, 0.0, 1e-9) && near(r[0].db, db(0.8), 0.01),
        "{r:?}"
    );
    assert!(
        near(r[1].ms, 0.25, 1e-6) && near(r[1].db, db(0.5), 0.01),
        "{r:?}"
    );
    assert!(t.arrivals.tail.is_none(), "no room has no tail");
}

#[test]
fn times_count_from_the_earliest_direct_arrival_of_both_ears() {
    // Every path starts 100 samples late; the earliest is still 0 ms.
    let late = |taps: &Vec<f32>| {
        let mut out = vec![0.0; 100];
        out.extend_from_slice(taps);
        out
    };
    let base = diracs(&[]);
    let shifted = [
        late(&base[0]),
        late(&base[1]),
        late(&base[2]),
        late(&base[3]),
    ];
    assert_eq!(
        analyse(&shifted, RATE, false).arrivals,
        analyse(&base, RATE, false).arrivals
    );
}

#[test]
fn a_room_adds_reflections_and_a_tail_and_no_room_does_not() {
    // Reflections of the left speaker at 5 ms (gain 0.3), 20 ms (0.1) and one under the floor.
    let set = diracs(&[(240, 0.3), (960, 0.1), (1200, 0.001)]);
    let room = analyse(&set, RATE, true);
    let l = &room.arrivals.left;
    assert_eq!(l.len(), 4, "two direct, two reflections: {l:?}");
    assert!(
        near(l[2].ms, 5.0, 1e-6) && near(l[2].db, db(0.3), 0.01),
        "{l:?}"
    );
    assert!(
        near(l[3].ms, 20.0, 1e-6) && near(l[3].db, db(0.1), 0.01),
        "{l:?}"
    );
    assert!(l.windows(2).all(|w| w[0].ms <= w[1].ms), "sorted");
    let plain = analyse(&set, RATE, false);
    assert_eq!(plain.arrivals.left.len(), 2);
    let tail = room.arrivals.tail.as_ref().expect("a tail with a room");
    assert_eq!(tail.times_ms.len(), TAIL_POINTS);
    assert_eq!(tail.left.len(), TAIL_POINTS);
    assert_eq!(tail.right.len(), TAIL_POINTS);
    assert!(near(tail.times_ms[0], 0.0, 1e-9) && near(tail.times_ms[TAIL_POINTS - 1], 50.0, 1e-9));
    // The 5 ms reflection shows in the envelope near 5 ms.
    let at5 = tail.times_ms.iter().position(|t| *t >= 5.0).unwrap();
    let peak = tail.left[at5.saturating_sub(1)..=at5 + 1]
        .iter()
        .cloned()
        .fold(f64::MIN, f64::max);
    assert!(near(peak, db(0.3), 0.5), "{peak}");
}

#[test]
fn at_most_twelve_reflections_the_strongest() {
    let hits: Vec<(usize, f32)> = (0..20)
        .map(|k| (100 + k * 100, 0.1 + 0.01 * k as f32))
        .collect();
    let t = analyse(&diracs(&hits), RATE, true);
    let l = &t.arrivals.left;
    assert_eq!(l.len(), 2 + MAX_REFLECTIONS, "{l:?}");
    // The weakest reflection kept is the 9th hit (0.18); the first eight were dropped.
    let weakest = l[2..].iter().map(|a| a.db).fold(f64::MAX, f64::min);
    assert!(near(weakest, db(0.18), 0.05), "{weakest}");
}

/// Deterministic white noise under an exponential: 60 dB of energy decay in `rt60` seconds.
fn tail(len: usize, rt60: f64) -> Vec<f32> {
    let mut state = 12345u32;
    (0..len)
        .map(|n| {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let noise = (state >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
            let amp = 10f64.powf(-3.0 * n as f64 / (f64::from(RATE) * rt60));
            noise * amp as f32
        })
        .collect()
}

fn slope(times: &[f64], values: &[f64], from_ms: f64, to_ms: f64) -> f64 {
    let pts: Vec<(f64, f64)> = times
        .iter()
        .zip(values)
        .filter(|(t, _)| **t >= from_ms && **t <= to_ms)
        .map(|(t, v)| (*t, *v))
        .collect();
    let n = pts.len() as f64;
    let (sx, sy) = pts.iter().fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
    let sxy: f64 = pts.iter().map(|(x, y)| x * y).sum();
    let sxx: f64 = pts.iter().map(|(x, _)| x * x).sum();
    (n * sxy - sx * sy) / (n * sxx - sx * sx)
}

#[test]
fn an_exponential_tail_decays_at_its_known_rate_in_every_band() {
    let set = [
        tail(24_000, 0.15),
        tail(24_000, 0.15),
        tail(24_000, 0.15),
        tail(24_000, 0.15),
    ];
    let d = analyse(&set, RATE, true).decay;
    assert!(near(d.end_ms, 500.0, 1e-6), "{}", d.end_ms);
    assert!(d.times_ms.len() <= DECAY_POINTS);
    assert!(near(d.times_ms[0], 0.1, 1e-9));
    assert!(near(*d.times_ms.last().unwrap(), 500.0, 1e-6));
    // 60 dB in 150 ms: -0.4 dB/ms.
    // The low band holds few independent samples of the noise (about 40 in 150 ms), so its slope
    // is the least certain.
    for (name, band, tol) in [
        ("low", &d.low, 0.1),
        ("mid", &d.mid, 0.05),
        ("high", &d.high, 0.05),
    ] {
        assert_eq!(band.len(), d.times_ms.len(), "{name}");
        assert!(near(band[0], 0.0, 1e-9), "{name} starts at 0 dB");
        assert!(
            band.iter().all(|v| v.is_finite() && *v >= DECAY_FLOOR_DB),
            "{name}"
        );
        let s = slope(&d.times_ms, band, 20.0, 90.0);
        assert!(near(s, -0.4, tol), "{name} slope {s}");
    }
}

#[test]
fn a_silent_set_gives_finite_numbers() {
    let silent = [
        vec![0.0; 100],
        vec![0.0; 100],
        vec![0.0; 100],
        vec![0.0; 100],
    ];
    let t = analyse(&silent, RATE, true);
    assert!(t.arrivals.left.is_empty() && t.arrivals.right.is_empty());
    assert!(t.decay.low.iter().all(|v| v.is_finite()));
    let none = analyse(&[vec![], vec![], vec![], vec![]], RATE, true);
    assert!(none.decay.times_ms.is_empty());
}

#[test]
fn the_timing_round_trips_through_json() {
    let t = analyse(&diracs(&[(240, 0.3)]), RATE, true);
    let json = serde_json::to_string(&t).unwrap();
    assert_eq!(serde_json::from_str::<Timing>(&json).unwrap(), t);
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(v["arrivals"]["left"][0]["ms"].is_number());
    assert!(v["decay"]["end_ms"].is_number());
    assert!(v["arrivals"]["tail"]["times_ms"].is_array());
}

/// A 3 kHz ring with a 0.4 ms time constant starting at sample `at`, gain `gain`.
fn ring(len: usize, at: usize, gain: f32) -> Vec<f32> {
    let mut taps = vec![0.0; len];
    for (n, tap) in taps.iter_mut().enumerate().take(len.min(at + 960)).skip(at) {
        let t = (n - at) as f64 / f64::from(RATE);
        let wave = (2.0 * std::f64::consts::PI * 3000.0 * (t + 1.0 / 36_000.0)).sin();
        *tap += gain * (wave * (-t / 0.0004).exp()) as f32;
    }
    taps
}

#[test]
fn a_ringing_reflection_counts_once() {
    // Coloured sound: the direct and two reflections (5 ms at 0.3, 20 ms at 0.1) all ring.
    let mut left = ring(4800, 0, 1.0);
    for (at, gain) in [(240, 0.3), (960, 0.1)] {
        for (a, b) in left.iter_mut().zip(ring(4800, at, gain)) {
            *a += b;
        }
    }
    let set = [
        left,
        ring(4800, 12, 0.5),
        ring(4800, 12, 0.4),
        ring(4800, 0, 0.8),
    ];
    let t = analyse(&set, RATE, true);
    let l = &t.arrivals.left;
    assert_eq!(
        l.len(),
        4,
        "two direct and two reflections, no lobes: {l:?}"
    );
    assert!(
        near(l[2].ms, 5.0, 0.1) && near(l[2].db, db(0.3), 1.0),
        "{l:?}"
    );
    assert!(
        near(l[3].ms, 20.0, 0.1) && near(l[3].db, db(0.1), 1.0),
        "{l:?}"
    );
}

#[test]
fn a_tap_that_is_not_a_number_plays_as_silence() {
    let mut set = diracs(&[(240, 0.3)]);
    set[0][500] = f32::NAN;
    set[2][700] = f32::INFINITY;
    set[3][900] = f32::NEG_INFINITY;
    let t = analyse(&set, RATE, true);
    let tail = t.arrivals.tail.as_ref().unwrap();
    let all = t
        .arrivals
        .left
        .iter()
        .chain(&t.arrivals.right)
        .flat_map(|a| [a.ms, a.db])
        .chain(tail.left.iter().chain(&tail.right).copied())
        .chain(
            t.decay
                .low
                .iter()
                .chain(&t.decay.mid)
                .chain(&t.decay.high)
                .copied(),
        );
    assert!(all.into_iter().all(f64::is_finite));
    // The decay is that of the same set with those taps at zero, not a flat floor.
    let mut zeroed = diracs(&[(240, 0.3)]);
    zeroed[0][500] = 0.0;
    assert_eq!(t.decay, analyse(&zeroed, RATE, true).decay);
    assert!(t.decay.mid.iter().any(|v| *v > DECAY_FLOOR_DB));
}

#[test]
fn a_very_short_set_has_times_inside_its_length_and_no_duplicates() {
    for len in [1usize, 2, 5, 6, 50] {
        let set = [
            ir(len, &[(0, 1.0)]),
            ir(len, &[]),
            ir(len, &[]),
            ir(len, &[(0, 0.5)]),
        ];
        let d = analyse(&set, RATE, false).decay;
        assert!(!d.times_ms.is_empty(), "{len}");
        assert!(
            d.times_ms.iter().all(|t| *t <= d.end_ms + 1e-9),
            "{len}: {d:?}"
        );
        assert!(
            d.times_ms.windows(2).all(|w| w[0] < w[1]),
            "{len}: {:?}",
            d.times_ms
        );
        assert_eq!(d.low.len(), d.times_ms.len());
    }
}

#[test]
fn the_tail_is_floored_at_minus_sixty_like_the_decay() {
    assert_eq!(TAIL_FLOOR_DB, DECAY_FLOOR_DB);
    let t = analyse(&diracs(&[(240, 0.3)]), RATE, true);
    let tail = t.arrivals.tail.unwrap();
    assert!(
        tail.left
            .iter()
            .chain(&tail.right)
            .all(|v| *v >= TAIL_FLOOR_DB)
    );
    assert!(
        tail.left.contains(&TAIL_FLOOR_DB),
        "silent stretches sit on the floor"
    );
}
