// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

const N: usize = 32;

/// A set of `positions` (az, el, distance) of two receivers, each response a Dirac at
/// `10 + 100 * measurement + 5 * receiver` of gain `1 + receiver`.
fn set(positions: &[(f64, f64, f64)]) -> Sofa {
    let m = positions.len();
    let mut ir = vec![0.0; m * 2 * N];
    for k in 0..m {
        for r in 0..2 {
            ir[(k * 2 + r) * N + 5 * r + k] = 1.0 + r as f32 + 0.01 * k as f32;
        }
    }
    Sofa {
        rate: 48000.0,
        shape: (m, 2, N),
        ir,
        positions: positions
            .iter()
            .map(|&(az_deg, el_deg, distance_m)| Direction {
                az_deg,
                el_deg,
                distance_m,
            })
            .collect(),
        delay: vec![0.0; 2],
        swap_ears: false,
    }
}

fn at(taps: &[f32]) -> (usize, f32) {
    let i = taps.iter().position(|v| *v != 0.0).unwrap();
    (i, taps[i])
}

#[test]
fn the_two_speakers_come_from_the_measurements_nearest_their_angles() {
    // 0: front, 1: 30, 2: 330, 3: 90.
    let sofa = set(&[
        (0.0, 0.0, 1.5),
        (30.0, 0.0, 1.5),
        (330.0, 0.0, 1.5),
        (90.0, 0.0, 1.5),
    ]);
    let (irs, angles) = pick(&sofa, 30.0, 330.0, DEFAULT_TOLERANCE).unwrap();
    // LL and LR from measurement 1 (left ear = receiver 0, right ear = receiver 1).
    assert_eq!(at(&irs.ch[0]), (1, 1.01));
    assert_eq!(at(&irs.ch[1]), (6, 2.01));
    // RL and RR from measurement 2.
    assert_eq!(at(&irs.ch[2]), (2, 1.02));
    assert_eq!(at(&irs.ch[3]), (7, 2.02));
    assert_eq!(angles.left.az_deg, 30.0);
    assert_eq!(angles.right.az_deg, 330.0);
    assert_eq!(irs.rate, 48000);
}

#[test]
fn the_nearest_not_the_first_and_a_wrap_around_zero() {
    let sofa = set(&[
        (25.0, 0.0, 1.5),
        (33.0, 0.0, 1.5),
        (328.0, 0.0, 1.5),
        (352.0, 0.0, 1.5),
    ]);
    let (_, angles) = pick(&sofa, 30.0, 330.0, DEFAULT_TOLERANCE).unwrap();
    assert_eq!(angles.left.az_deg, 33.0);
    assert_eq!(angles.right.az_deg, 328.0);
    let wrap = set(&[(358.0, 0.0, 1.5), (180.0, 0.0, 1.5)]);
    let (_, angles) = pick(&wrap, 359.0, 359.0, DEFAULT_TOLERANCE).unwrap();
    assert_eq!(angles.left.az_deg, 358.0);
}

#[test]
fn the_horizon_wins_over_a_raised_measurement_at_the_same_azimuth() {
    let sofa = set(&[(30.0, 17.5, 1.5), (30.0, 0.0, 1.5), (330.0, 0.0, 1.5)]);
    let (_, angles) = pick(&sofa, 30.0, 330.0, DEFAULT_TOLERANCE).unwrap();
    assert_eq!(angles.left.el_deg, 0.0);
}

#[test]
fn among_equal_directions_the_distance_nearest_one_and_a_half_metres_wins() {
    let sofa = set(&[
        (30.0, 0.0, 1.0),
        (30.0, 0.0, 1.4),
        (30.0, 0.0, 3.0),
        (330.0, 0.0, 2.0),
    ]);
    let (_, angles) = pick(&sofa, 30.0, 330.0, DEFAULT_TOLERANCE).unwrap();
    assert_eq!(angles.left.distance_m, 1.4);
}

#[test]
fn nothing_within_the_tolerance_is_refused_with_the_nearest_found() {
    let sofa = set(&[(0.0, 0.0, 1.5), (60.0, 0.0, 1.5), (300.0, 0.0, 1.5)]);
    let error = pick(&sofa, 30.0, 330.0, DEFAULT_TOLERANCE).unwrap_err();
    assert!(
        matches!(error, ConvertError::NoMeasurement { wanted, tolerance, .. }
        if wanted == 30.0 && tolerance == 10.0)
    );
    let words = error.to_string();
    assert!(words.contains("within 10 degrees of 30 degrees"), "{words}");
    // A set with only the speakers at 45 is read when asked for 45.
    let wide = set(&[(45.0, 0.0, 1.5), (315.0, 0.0, 1.5)]);
    assert!(pick(&wide, 45.0, 315.0, 1.0).is_ok());
    assert!(pick(&wide, 30.0, 330.0, DEFAULT_TOLERANCE).is_err());
}

#[test]
fn a_delay_shifts_a_response_by_whole_samples() {
    let mut sofa = set(&[(30.0, 0.0, 1.5), (330.0, 0.0, 1.5)]);
    sofa.delay = vec![3.0, 7.4];
    let (irs, _) = pick(&sofa, 30.0, 330.0, DEFAULT_TOLERANCE).unwrap();
    assert_eq!(at(&irs.ch[0]).0, 3);
    assert_eq!(at(&irs.ch[1]).0, 5 + 7);
    assert_eq!(at(&irs.ch[2]).0, 1 + 3);
    assert_eq!(at(&irs.ch[3]).0, 6 + 7);
    assert_eq!(irs.ch[0].len(), N + 3);
}

#[test]
fn a_delay_per_measurement_and_receiver_is_indexed_by_both() {
    let mut sofa = set(&[(30.0, 0.0, 1.5), (330.0, 0.0, 1.5)]);
    // measurement 0: receivers 0, 1; measurement 1: receivers 0, 1.
    sofa.delay = vec![1.0, 2.0, 4.0, 8.0];
    let (irs, _) = pick(&sofa, 30.0, 330.0, DEFAULT_TOLERANCE).unwrap();
    assert_eq!(at(&irs.ch[0]).0, 1);
    assert_eq!(at(&irs.ch[1]).0, 5 + 2);
    assert_eq!(at(&irs.ch[2]).0, 1 + 4);
    assert_eq!(at(&irs.ch[3]).0, 6 + 8);
}

#[test]
fn receivers_in_the_other_order_are_swapped_to_left_and_right() {
    let mut sofa = set(&[(30.0, 0.0, 1.5), (330.0, 0.0, 1.5)]);
    sofa.swap_ears = true;
    let (irs, _) = pick(&sofa, 30.0, 330.0, DEFAULT_TOLERANCE).unwrap();
    // The left ear is receiver 1 now: its Dirac is at 5 + k with gain 2 + 0.01 k.
    assert_eq!(at(&irs.ch[0]), (5, 2.0));
    assert_eq!(at(&irs.ch[1]), (0, 1.0));
}

#[test]
fn cartesian_positions_become_a_direction() {
    let left = direction("cartesian", [0.866, 0.5, 0.0]);
    assert!((left.az_deg - 30.0).abs() < 0.01 && left.el_deg == 0.0);
    let right = direction("cartesian", [0.866, -0.5, 0.0]);
    assert!((right.az_deg - 330.0).abs() < 0.01);
    let up = direction("cartesian", [1.0, 0.0, 1.0]);
    assert!((up.el_deg - 45.0).abs() < 1e-9 && (up.distance_m - 2f64.sqrt()).abs() < 1e-9);
    let sph = direction("spherical", [-30.0, 5.0, 1.2]);
    assert_eq!((sph.az_deg, sph.el_deg, sph.distance_m), (330.0, 5.0, 1.2));
    assert_eq!(cartesian("spherical", [90.0, 0.0, 2.0])[1].round(), 2.0);
}

#[test]
fn bytes_that_are_not_a_sofa_file_are_refused_in_words() {
    assert!(matches!(
        parse(b"not hdf5 at all"),
        Err(ConvertError::NotSofa(_))
    ));
}

#[test]
fn a_crafted_rate_is_refused_before_it_sizes_anything() {
    for rate in [1e12, f64::NAN, f64::INFINITY, 100.0, -48000.0] {
        let mut sofa = set(&[(30.0, 0.0, 1.5), (330.0, 0.0, 1.5)]);
        sofa.rate = rate;
        sofa.delay = vec![5e9, 5e9];
        assert!(
            matches!(
                pick(&sofa, 30.0, 330.0, DEFAULT_TOLERANCE),
                Err(ConvertError::Rate(_))
            ),
            "{rate}"
        );
    }
}

#[test]
fn a_crafted_delay_is_refused_not_allocated() {
    for delay in [f64::NAN, f64::INFINITY, 1e12, 200_000.0] {
        let mut sofa = set(&[(30.0, 0.0, 1.5), (330.0, 0.0, 1.5)]);
        sofa.delay = vec![delay, 0.0];
        assert!(
            matches!(
                pick(&sofa, 30.0, 330.0, DEFAULT_TOLERANCE),
                Err(ConvertError::Layout(_))
            ),
            "{delay}"
        );
    }
    // The longest allowed delay at the highest rate is still taken.
    let mut sofa = set(&[(30.0, 0.0, 1.5), (330.0, 0.0, 1.5)]);
    sofa.rate = 384_000.0;
    sofa.delay = vec![192_000.0, 0.0];
    assert!(pick(&sofa, 30.0, 330.0, DEFAULT_TOLERANCE).is_ok());
}

#[test]
fn crafted_dimensions_are_refused_not_overflowed_or_indexed_past_the_data() {
    let base = set(&[(30.0, 0.0, 1.5), (330.0, 0.0, 1.5)]);
    for shape in [
        (usize::MAX / 2, 2, 4),
        (2, 2, usize::MAX / 2),
        (2, 2, N * 1000),
        (2, 2, MAX_SAMPLES + 1),
        (MAX_MEASUREMENTS + 1, 2, 4),
    ] {
        let mut sofa = base.clone();
        sofa.shape = shape;
        assert!(
            matches!(
                pick(&sofa, 30.0, 330.0, DEFAULT_TOLERANCE),
                Err(ConvertError::Layout(_))
            ),
            "{shape:?}"
        );
    }
}
