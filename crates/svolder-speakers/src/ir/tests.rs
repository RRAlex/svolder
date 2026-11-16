// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

use super::*;

/// `n` zeros with `(index, gain)` taps set.
fn taps(n: usize, set: &[(usize, f32)]) -> Vec<f32> {
    let mut v = vec![0.0; n];
    for &(i, g) in set {
        v[i] = g;
    }
    v
}

fn irs(rate: u32, ch: [Vec<f32>; 4]) -> Irs {
    Irs { rate, ch }
}

/// The scale `prepare` gives a set whose worst gain is 1.0.
fn unit_scale() -> f32 {
    10f64.powf(TARGET_DB / 20.0) as f32
}

fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 1e-6, "{a} vs {b}");
}

/// The worst gain over the 1/48-octave grid, worked out here through `response_at`.
fn worst_db(irs: &Irs) -> f64 {
    let mut worst = 0.0f64;
    let mut hz = 20.0;
    while hz <= 20000.0 {
        let [ll, lr, rl, rr] = response_at(&irs.ch, irs.rate, hz).map(|c| c.re.hypot(c.im));
        worst = worst.max(ll + rl).max(lr + rr);
        hz *= 2f64.powf(1.0 / 48.0);
    }
    20.0 * worst.log10()
}

#[test]
fn a_response_has_the_gain_and_the_phase_of_its_delay() {
    let ch = [
        taps(64, &[(10, 0.5)]),
        vec![0.0; 64],
        vec![0.0; 64],
        vec![0.0; 64],
    ];
    let r = response_at(&ch, 48000, 1000.0);
    assert!((r[0].re.hypot(r[0].im) - 0.5).abs() < 1e-9);
    let phase = -2.0 * std::f64::consts::PI * 1000.0 * 10.0 / 48000.0;
    assert!((r[0].re - 0.5 * phase.cos()).abs() < 1e-9);
    assert!((r[0].im - 0.5 * phase.sin()).abs() < 1e-9);
    assert_eq!(r[1], Cx::default());
}

#[test]
fn diracs_keep_their_positions_and_gain_ratios_and_the_worst_gain_is_the_target() {
    // LL 10 / 0.5, LR 14 / 0.25, RL 20 / 0.125, RR 30 / 0.75: the worst gain is LR + RR = 1.0.
    let input = irs(
        48000,
        [
            taps(200, &[(10, 0.5)]),
            taps(200, &[(14, 0.25)]),
            taps(200, &[(20, 0.125)]),
            taps(200, &[(30, 0.75)]),
        ],
    );
    let (out, report) = prepare(input).unwrap();
    let s = unit_scale();
    close(out.ch[0][48], 0.5 * s);
    close(out.ch[1][52], 0.25 * s);
    close(out.ch[2][58], 0.125 * s);
    close(out.ch[3][68], 0.75 * s);
    let rest: f32 = out
        .ch
        .iter()
        .map(|c| c.iter().map(|v| v.abs()).sum::<f32>())
        .sum();
    assert!((rest - (1.625 * s)).abs() < 1e-5, "nothing else is set");
    assert_eq!(
        report.trimmed, 0,
        "an onset inside the 1 ms lead-in is not trimmed"
    );
    assert_eq!(
        report.padded, 38,
        "it is padded up to 1 ms (48 samples) instead"
    );
    assert!((report.scale_db - TARGET_DB).abs() < 1e-9);
    assert!(
        (report.worst_gain_db - TARGET_DB).abs() < 0.01,
        "{}",
        report.worst_gain_db
    );
    assert!((worst_db(&out) - TARGET_DB).abs() < 0.01);
    assert_eq!(report.taps, 238);
    assert!(!report.cut);
}

#[test]
fn the_onset_trim_is_common_to_the_four_and_leaves_a_millisecond() {
    let input = irs(
        48000,
        [
            taps(1000, &[(148, 1.0)]),
            taps(1000, &[(152, 1.0)]),
            taps(1000, &[(158, 1.0)]),
            taps(1000, &[(168, 1.0)]),
        ],
    );
    let (out, report) = prepare(input).unwrap();
    // The earliest onset is 148; 1 ms is 48 samples; 100 are dropped from all four.
    assert_eq!(report.trimmed, 100);
    assert_eq!(report.taps, 900);
    let at = |c: usize| out.ch[c].iter().position(|v| v.abs() > 0.1).unwrap();
    assert_eq!([at(0), at(1), at(2), at(3)], [48, 52, 58, 68]);
}

#[test]
fn a_first_impulse_at_sample_zero_gets_a_millisecond_of_lead_in_with_the_lags_kept() {
    let input = irs(
        48000,
        [
            taps(1000, &[(0, 0.5)]),
            taps(1000, &[(4, 0.25)]),
            taps(1000, &[(10, 0.125)]),
            taps(1000, &[(20, 0.75)]),
        ],
    );
    let (out, report) = prepare(input).unwrap();
    let s = unit_scale();
    assert_eq!((report.trimmed, report.padded), (0, 48));
    assert_eq!(report.taps, 1048);
    let at = |c: usize| out.ch[c].iter().position(|v| v.abs() > 0.0).unwrap();
    assert_eq!([at(0), at(1), at(2), at(3)], [48, 52, 58, 68]);
    close(out.ch[0][48], 0.5 * s);
    close(out.ch[3][68], 0.75 * s);
}

#[test]
fn an_onset_already_a_millisecond_in_is_neither_trimmed_nor_padded() {
    let input = irs(
        48000,
        [
            taps(500, &[(48, 1.0)]),
            taps(500, &[(50, 1.0)]),
            taps(500, &[(52, 1.0)]),
            taps(500, &[(60, 1.0)]),
        ],
    );
    let (_, report) = prepare(input).unwrap();
    assert_eq!((report.trimmed, report.padded), (0, 0));
    assert_eq!(report.taps, 500);
}

#[test]
fn an_early_precursor_below_minus_20_db_is_not_the_onset() {
    let input = irs(
        48000,
        [
            taps(2000, &[(100, 0.05), (500, 1.0)]),
            taps(2000, &[(520, 1.0)]),
            taps(2000, &[(540, 1.0)]),
            taps(2000, &[(560, 1.0)]),
        ],
    );
    let (_, report) = prepare(input).unwrap();
    assert_eq!(report.trimmed, 500 - 48);
}

#[test]
fn a_channel_with_no_signal_does_not_hold_the_trim_back() {
    let input = irs(
        48000,
        [
            taps(1000, &[(300, 1.0)]),
            vec![0.0; 1000],
            taps(1000, &[(310, 1.0)]),
            vec![0.0; 1000],
        ],
    );
    let (_, report) = prepare(input).unwrap();
    assert_eq!(report.trimmed, 300 - 48);
}

#[test]
fn responses_longer_than_half_a_second_are_cut_with_a_fade() {
    let mut a = taps(30000, &[(0, 1.0)]);
    a[29999] = 1.0;
    a[23700] = 1.0;
    a[20000] = 1.0;
    let input = irs(
        48000,
        [a, vec![0.0; 30000], vec![0.0; 30000], vec![0.0; 30000]],
    );
    let (out, report) = prepare(input).unwrap();
    assert!(report.cut);
    assert_eq!(report.taps, 24000);
    assert!((report.taps_ms - 500.0).abs() < 1e-9);
    let s = out.ch[0][48];
    assert!(out.ch.iter().all(|c| c.len() == 24000));
    assert!(
        (out.ch[0][20048] / s - 1.0).abs() < 1e-3,
        "before the fade is untouched"
    );
    assert_eq!(out.ch[0][23999], 0.0, "the last sample is faded to zero");
    // 23700 is inside the last 480 samples: attenuated, and the fade never rises.
    assert!(out.ch[0][23748].abs() < s * 0.99 && out.ch[0][23748] > 0.0);
}

#[test]
fn the_fade_is_a_half_cosine_over_ten_milliseconds() {
    let input = irs(
        48000,
        [
            vec![1.0; 30000],
            vec![1.0; 30000],
            vec![1.0; 30000],
            vec![1.0; 30000],
        ],
    );
    let (out, _) = prepare(input).unwrap();
    let s = out.ch[0][48];
    let first_faded = 24000 - 480;
    close(out.ch[0][first_faded - 1], s);
    for k in 0..480 {
        let want = 0.5 * (1.0 + (std::f64::consts::PI * (k as f64 + 1.0) / 480.0).cos());
        let got = f64::from(out.ch[0][first_faded + k] / s);
        assert!((got - want).abs() < 1e-5, "k={k}: {got} vs {want}");
    }
}

#[test]
fn unequal_lengths_are_padded_to_the_longest() {
    let input = irs(
        48000,
        [
            taps(100, &[(1, 1.0)]),
            taps(300, &[(2, 1.0)]),
            taps(50, &[(3, 1.0)]),
            taps(10, &[(4, 1.0)]),
        ],
    );
    let (out, report) = prepare(input).unwrap();
    // The earliest onset is 1, so 47 zeros go in front: 347 is the longest.
    assert!(out.ch.iter().all(|c| c.len() == 347));
    assert_eq!(report.taps, 347);
}

#[test]
fn the_loudness_change_is_the_mean_power_of_a_centred_signal() {
    // LL = 1 and RR = 0.5, nothing crossed: one ear gets 1, the other 0.5, mean power 0.625,
    // after the scale of -0.1 dB.
    let input = irs(
        48000,
        [
            taps(64, &[(0, 1.0)]),
            vec![0.0; 64],
            vec![0.0; 64],
            taps(64, &[(0, 0.5)]),
        ],
    );
    let (_, report) = prepare(input).unwrap();
    let want = 10.0 * 0.625f64.log10() + TARGET_DB;
    assert!(
        (report.loudness_db - want).abs() < 0.01,
        "{} vs {want}",
        report.loudness_db
    );
}

#[test]
fn the_curve_has_the_crossfeed_grid_and_each_paths_gain() {
    let input = irs(
        48000,
        [
            taps(64, &[(0, 1.0)]),
            taps(64, &[(0, 0.5)]),
            taps(64, &[(0, 0.25)]),
            taps(64, &[(0, 0.125)]),
        ],
    );
    let (_, report) = prepare(input).unwrap();
    let c = &report.curve;
    assert_eq!(c.hz.len(), CURVE_POINTS);
    assert_eq!(c.hz[0], 20.0);
    assert_eq!(c.hz[60], 20000.0);
    assert_eq!(c.hz[20], 200.0);
    for (series, gain) in [
        (&c.ll_db, 1.0f64),
        (&c.lr_db, 0.5),
        (&c.rl_db, 0.25),
        (&c.rr_db, 0.125),
    ] {
        assert_eq!(series.len(), CURVE_POINTS);
        // Worst gain: LL + RL = 1.25, LR + RR = 0.625: the scale is TARGET_DB - 20 log10(1.25).
        let want = 20.0 * gain.log10() + TARGET_DB - 20.0 * 1.25f64.log10();
        assert!(
            series.iter().all(|v| (v - want).abs() < 0.011),
            "{series:?} vs {want}"
        );
    }
}

#[test]
fn a_room_like_set_lands_on_the_target() {
    // A deterministic decaying noise for each path.
    let mut state = 12345u32;
    let mut noise = |n: usize, decay: f32| -> Vec<f32> {
        (0..n)
            .map(|i| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                let r = (state >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
                r * (-(i as f32) * decay).exp()
            })
            .collect()
    };
    let input = irs(
        44100,
        [
            noise(8000, 0.002),
            noise(8000, 0.002),
            noise(8000, 0.003),
            noise(8000, 0.001),
        ],
    );
    let (out, report) = prepare(input).unwrap();
    // The band's true worst is the target; the 1/48-octave points only sit at or under it.
    assert!(worst_db(&out) <= TARGET_DB + 0.01, "{}", worst_db(&out));
    assert!(
        (report.worst_gain_db - TARGET_DB).abs() < 0.1,
        "{}",
        report.worst_gain_db
    );
}

#[test]
fn silence_and_odd_rates_are_refused() {
    let silent = irs(
        48000,
        [vec![0.0; 10], vec![0.0; 10], vec![0.0; 10], vec![0.0; 10]],
    );
    assert_eq!(prepare(silent), Err(ConvertError::Silent));
    let slow = irs(
        4000,
        [vec![1.0; 10], vec![0.0; 10], vec![0.0; 10], vec![1.0; 10]],
    );
    assert_eq!(prepare(slow), Err(ConvertError::Rate(4000)));
}

/// A Dirac at 0 plus a Hann-windowed tone of `hz` over 0.4 s, in LL.
fn dirac_and_tone(hz: f64) -> Irs {
    let n = 19200;
    let mut ll = vec![0.0f32; 24000];
    ll[0] = 1.0;
    for i in 0..n {
        let hann = 0.5 * (1.0 - (2.0 * std::f64::consts::PI * i as f64 / (n - 1) as f64).cos());
        ll[1000 + i] +=
            (hann * (2.0 * std::f64::consts::PI * hz * i as f64 / 48000.0).sin()) as f32;
    }
    irs(
        48000,
        [ll, vec![0.0; 24000], vec![0.0; 24000], vec![0.0; 24000]],
    )
}

/// The largest `|LL| + |RL|` or `|LR| + |RR|` over `hz`.
fn worst_over(out: &Irs, hz: &[f64]) -> f64 {
    hz.iter()
        .map(|hz| {
            let [ll, lr, rl, rr] = response_at(&out.ch, out.rate, *hz).map(|c| c.re.hypot(c.im));
            (ll + rl).max(lr + rr)
        })
        .fold(0.0, f64::max)
}

#[test]
fn a_narrow_peak_above_20_khz_never_boosts() {
    let (out, report) = prepare(dirac_and_tone(22000.0)).unwrap();
    let hz: Vec<f64> = (0..400).map(|i| 21500.0 + f64::from(i) * 2.5).collect();
    assert!(worst_over(&out, &hz) <= 1.0, "{}", worst_over(&out, &hz));
    assert!(
        (report.worst_gain_db - TARGET_DB).abs() < 0.1,
        "{}",
        report.worst_gain_db
    );
}

#[test]
fn a_narrow_peak_between_two_grid_points_never_boosts() {
    let (out, _) = prepare(dirac_and_tone(10021.0)).unwrap();
    let hz: Vec<f64> = (0..400).map(|i| 9900.0 + f64::from(i) * 0.6).collect();
    assert!(worst_over(&out, &hz) <= 1.0, "{}", worst_over(&out, &hz));
}

#[test]
fn a_room_like_tail_never_boosts_anywhere_in_the_band() {
    let mut state = 99u32;
    let mut tail = |amp: f32| -> Vec<f32> {
        (0..24000)
            .map(|i| {
                state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                let r = (state >> 8) as f32 / (1u32 << 24) as f32 - 0.5;
                amp * r * (-(i as f32) * 6.9 / 19200.0).exp()
            })
            .collect()
    };
    let mut ll = tail(0.2);
    ll[0] = 1.0;
    let mut rr = tail(0.2);
    rr[0] = 1.0;
    let (out, _) = prepare(irs(48000, [ll, tail(0.2), tail(0.2), rr])).unwrap();
    let hz: Vec<f64> = (0..3000).map(|i| 7.3 + f64::from(i) * 7.9).collect();
    assert!(worst_over(&out, &hz) <= 1.0, "{}", worst_over(&out, &hz));
}

#[test]
fn values_that_are_not_numbers_are_refused_in_their_own_words() {
    for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        let mut ll = taps(500, &[(5, 1.0)]);
        ll[300] = bad;
        let input = irs(
            48000,
            [
                ll,
                taps(500, &[(6, 1.0)]),
                vec![0.0; 500],
                taps(500, &[(7, 1.0)]),
            ],
        );
        let error = prepare(input).unwrap_err();
        assert!(error.to_string().contains("not numbers"), "{bad}: {error}");
    }
}
