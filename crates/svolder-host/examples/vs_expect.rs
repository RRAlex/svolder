// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the virtual speakers' stage should do to a tone, from the engine's own response
//! function, and the synthetic set the checks play through: the oracle of `make host-e2e`
//! (scripts/host-e2e/analyse.py).
//!
//! - `vs_expect --write <path>` writes the fixture, a 4-channel float32 WAV at 48 kHz (a Dirac
//!   per path with its own gain and lag, then a short decaying noise "room"). It is no dataset
//!   file; the run imports it as its own set and the daemon converts and stores it.
//! - `vs_expect <rate> <wav> <hz>...` reads a stored set's WAV and prints one JSON object per
//!   frequency, `{"hz", "ll": [re, im], "lr": ..., "rl": ..., "rr": ...}`: the complex gains
//!   of the four paths on a sink running at `rate` (see
//!   [`svolder_host::eq::speakers_response`]).

use svolder_host::eq::{Irs, speakers_response};

/// The fixture's rate.
const FIXTURE_RATE: u32 = 48_000;
/// The fixture's length: 0.1 s.
const FIXTURE_TAPS: usize = 4_800;

/// One path of the fixture: where its Dirac sits, its gain, and the noise tail's seed.
const PATHS: [(usize, f32, u32); 4] = [
    (0, 0.5, 11),  // LL: the first Dirac at sample 0, so the converter must add the lead-in
    (20, 0.3, 23), // LR: the far ear, 20 samples (0.4 ms) later and quieter
    (24, 0.2, 37), // RL
    (4, 0.45, 53), // RR
];

/// The next value of a small linear congruential generator, in -1 to 1 (fixed seeds, so the
/// fixture is the same file every run).
fn noise(state: &mut u32) -> f32 {
    *state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
    (*state >> 8) as f32 / (1u32 << 23) as f32 - 1.0
}

/// The fixture's four channels.
fn fixture() -> Vec<Vec<f32>> {
    PATHS
        .iter()
        .map(|&(at, gain, seed)| {
            let mut state = seed;
            let mut taps = vec![0.0f32; FIXTURE_TAPS];
            for (n, tap) in taps.iter_mut().enumerate().skip(160) {
                *tap = 0.015 * noise(&mut state) * (-(n as f32) / 900.0).exp();
            }
            taps[at] += gain;
            taps
        })
        .collect()
}

/// The fixture as a WAV file's bytes.
fn fixture_wav() -> Vec<u8> {
    svolder_speakers::wav::write(FIXTURE_RATE, &fixture())
}

/// The JSON line for one frequency.
fn line(irs: &Irs, rate: u32, hz: f64) -> String {
    let r = speakers_response(irs, rate, hz);
    format!(
        "{{\"hz\": {hz}, \"ll\": [{}, {}], \"lr\": [{}, {}], \"rl\": [{}, {}], \"rr\": [{}, {}]}}",
        r.ll[0], r.ll[1], r.lr[0], r.lr[1], r.rl[0], r.rl[1], r.rr[0], r.rr[1]
    )
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() == 2 && args[0] == "--write" {
        if let Err(why) = std::fs::write(&args[1], fixture_wav()) {
            eprintln!("vs_expect: {}: {why}", args[1]);
            std::process::exit(1);
        }
        return;
    }
    if args.len() < 3 {
        eprintln!("usage: vs_expect --write <path> | vs_expect <rate> <wav> <hz>...");
        std::process::exit(2);
    }
    let number = |text: &str| -> f64 {
        text.parse().unwrap_or_else(|_| {
            eprintln!("vs_expect: {text:?} is not a number");
            std::process::exit(2)
        })
    };
    let rate = number(&args[0]) as u32;
    let bytes = std::fs::read(&args[1]).unwrap_or_else(|why| {
        eprintln!("vs_expect: {}: {why}", args[1]);
        std::process::exit(1)
    });
    let irs = Irs::from_wav(&bytes).unwrap_or_else(|why| {
        eprintln!("vs_expect: {}: {why}", args[1]);
        std::process::exit(1)
    });
    for hz in &args[2..] {
        println!("{}", line(&irs, rate, number(hz)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixture_is_four_channels_the_engine_reads_with_each_path_s_dirac() {
        let irs = Irs::from_wav(&fixture_wav()).expect("the fixture is a 4-channel float WAV");
        assert_eq!(irs.rate_hz, FIXTURE_RATE);
        for (channel, &(at, gain, _)) in PATHS.iter().enumerate() {
            assert_eq!(irs.taps[channel].len(), FIXTURE_TAPS);
            assert!(
                (irs.taps[channel][at] - gain).abs() < 0.01,
                "path {channel}"
            );
            assert!(
                irs.taps[channel][..160]
                    .iter()
                    .enumerate()
                    .all(|(n, &t)| { n == at || t == 0.0 })
            );
        }
        assert_eq!(fixture_wav(), fixture_wav(), "the same file every run");
    }

    #[test]
    fn a_line_carries_the_four_complex_gains_and_a_dc_tone_is_the_tap_sum() {
        let irs = Irs::from_wav(&fixture_wav()).unwrap();
        let text = line(&irs, FIXTURE_RATE, 1.0);
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        let sum: f32 = irs.taps[0].iter().sum();
        let re = value["ll"][0].as_f64().unwrap();
        assert!((re - f64::from(sum)).abs() < 0.02, "{re} vs {sum}");
        for key in ["hz", "ll", "lr", "rl", "rr"] {
            assert!(value.get(key).is_some(), "{key}");
        }
    }
}
