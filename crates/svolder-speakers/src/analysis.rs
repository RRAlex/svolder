// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The timing of a set as the Arrivals and Decay pictures draw it: when the sound reaches each
//! ear and how fast the energy dies away.
//!
//! Worked out once from the four impulse responses (`LL`, `LR`, `RL`, `RR`, in that order) and
//! kept by the caller; nothing here is cheap enough to run on every view.

use serde::{Deserialize, Serialize};

/// The most points a decay curve has.
pub const DECAY_POINTS: usize = 100;
/// The points of the tail envelope.
pub const TAIL_POINTS: usize = 64;
/// The span of the early reflections and of the tail after the first arrival, in milliseconds.
pub const EARLY_MS: f64 = 50.0;
/// The most early reflections kept per ear.
pub const MAX_REFLECTIONS: usize = 12;
/// How far below an ear's loudest arrival a reflection may be, in dB.
pub const REFLECTION_FLOOR_DB: f64 = -30.0;
/// The decay curve's floor, in dB.
pub const DECAY_FLOOR_DB: f64 = -60.0;
/// The tail envelope's floor, in dB (the decay's floor too).
pub const TAIL_FLOOR_DB: f64 = -60.0;
/// A reflection hides every weaker peak within this many milliseconds (a coloured sound rings
/// for about that long), and no reflection sits this close after a direct arrival.
pub const RING_MS: f64 = 1.5;

/// One sound reaching an ear.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Arrival {
    /// When, in milliseconds after the earliest direct arrival of both ears.
    pub ms: f64,
    /// How loud, in dB below the loudest arrival of both ears (0 for that one).
    pub db: f64,
}

/// The ear's envelope after the direct sound (With room only).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Tail {
    /// The times in milliseconds, linear from 0 to [`EARLY_MS`].
    pub times_ms: Vec<f64>,
    /// The left ear's envelope in dB below the loudest arrival.
    pub left: Vec<f64>,
    /// The right ear's.
    pub right: Vec<f64>,
}

/// What each ear receives with both speakers playing the same signal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Arrivals {
    /// The left ear's arrivals, sorted by time.
    pub left: Vec<Arrival>,
    /// The right ear's.
    pub right: Vec<Arrival>,
    /// The envelope after the direct sound; `None` for a set with no room.
    pub tail: Option<Tail>,
}

/// How the energy dies away, per band, in dB below the start.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Decay {
    /// The times in milliseconds, log-spaced from 0.1 ms to `end_ms`.
    pub times_ms: Vec<f64>,
    /// Below 250 Hz.
    pub low: Vec<f64>,
    /// 250 Hz to 4 kHz.
    pub mid: Vec<f64>,
    /// Above 4 kHz.
    pub high: Vec<f64>,
    /// The length of the impulse responses as played, in milliseconds.
    pub end_ms: f64,
}

/// The timing data of a set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Timing {
    /// When the sound reaches each ear.
    pub arrivals: Arrivals,
    /// How the energy decays.
    pub decay: Decay,
}

/// The timing of the four impulse responses `irs` at `rate` Hz; `with_room` keeps the early
/// reflections and the tail, and a set with no room keeps only the direct arrivals.
pub fn analyse(irs: &[Vec<f32>; 4], rate: u32, with_room: bool) -> Timing {
    // A tap that is not a number plays as silence: every number below stays finite.
    let clean = irs.each_ref().map(|taps| {
        taps.iter()
            .map(|t| if t.is_finite() { *t } else { 0.0 })
            .collect::<Vec<f32>>()
    });
    let irs = &clean;
    let rate = f64::from(rate.max(1));
    let arrivals = arrivals(irs, rate, with_room);
    let len = irs.iter().map(Vec::len).max().unwrap_or(0);
    Timing {
        arrivals,
        decay: decay(irs, rate, len),
    }
}

fn round(value: f64, places: i32) -> f64 {
    let k = 10f64.powi(places);
    (value * k).round() / k
}

/// The index and gain of a path's direct arrival: the first sample at a quarter of the path's
/// peak or more, moved up to the local top of that rise.
fn direct(taps: &[f32]) -> Option<(usize, f64)> {
    let peak = taps.iter().fold(0.0f32, |m, t| m.max(t.abs()));
    if peak <= 0.0 || !peak.is_finite() {
        return None;
    }
    let mut at = taps.iter().position(|t| t.abs() >= peak * 0.25)?;
    while at + 1 < taps.len() && taps[at + 1].abs() > taps[at].abs() {
        at += 1;
    }
    Some((at, f64::from(taps[at].abs())))
}

struct Ear {
    /// Index and amplitude of every arrival kept.
    hits: Vec<(usize, f64)>,
    /// The ear's signal with both speakers playing.
    signal: Vec<f64>,
}

fn arrivals(irs: &[Vec<f32>; 4], rate: f64, with_room: bool) -> Arrivals {
    let len = irs.iter().map(Vec::len).max().unwrap_or(0);
    let at = |ch: usize, n: usize| f64::from(irs[ch].get(n).copied().unwrap_or(0.0));
    let directs: Vec<Option<(usize, f64)>> = irs.iter().map(|taps| direct(taps)).collect();
    let first = directs.iter().flatten().map(|(n, _)| *n).min();
    let Some(first) = first else {
        return Arrivals::default();
    };
    let guard = (rate * RING_MS / 1000.0).ceil() as usize;
    let window = (rate * EARLY_MS / 1000.0).round() as usize;
    let mut ears: Vec<Ear> = Vec::new();
    // The left ear hears LL and RL (channels 0 and 2), the right ear LR and RR (1 and 3).
    for paths in [[0, 2], [1, 3]] {
        let signal: Vec<f64> = (0..len)
            .map(|n| at(paths[0], n) + at(paths[1], n))
            .collect();
        let mut hits: Vec<(usize, f64)> = paths.iter().filter_map(|p| directs[*p]).collect();
        if with_room {
            let known: Vec<usize> = hits.iter().map(|(n, _)| *n).collect();
            let loudest = signal
                .iter()
                .fold(0.0f64, |m, v| m.max(v.abs()))
                .max(hits.iter().fold(0.0, |m, (_, a)| f64::max(m, *a)));
            let floor = loudest * 10f64.powf(REFLECTION_FLOOR_DB / 20.0);
            let last = (first + window).min(len.saturating_sub(1));
            let mut found: Vec<(usize, f64)> = Vec::new();
            for n in first..=last {
                let a = signal[n].abs();
                let before = if n > 0 { signal[n - 1].abs() } else { 0.0 };
                let after = signal.get(n + 1).map_or(0.0, |v| v.abs());
                let after_the_directs = known.iter().all(|k| n > k + guard);
                if a >= floor && a > 0.0 && a > before && a >= after && after_the_directs {
                    found.push((n, a));
                }
            }
            // The strongest first; a weaker peak within a ring's length of one kept is its lobe.
            found.sort_by(|x, y| y.1.total_cmp(&x.1).then(x.0.cmp(&y.0)));
            let mut kept: Vec<(usize, f64)> = Vec::new();
            for peak in found {
                if kept.len() == MAX_REFLECTIONS {
                    break;
                }
                if kept.iter().all(|(n, _)| n.abs_diff(peak.0) > guard) {
                    kept.push(peak);
                }
            }
            hits.extend(kept);
        }
        hits.sort_by_key(|(n, _)| *n);
        ears.push(Ear { hits, signal });
    }
    let loudest = ears
        .iter()
        .flat_map(|ear| ear.hits.iter().map(|(_, a)| *a))
        .fold(0.0f64, f64::max);
    if loudest <= 0.0 {
        return Arrivals::default();
    }
    let rel = |amp: f64, floor: f64| round((20.0 * (amp / loudest).log10()).max(floor), 2);
    let list = |ear: &Ear| -> Vec<Arrival> {
        ear.hits
            .iter()
            .map(|(n, a)| Arrival {
                ms: round((*n - first) as f64 * 1000.0 / rate, 4),
                db: rel(*a, -120.0),
            })
            .collect()
    };
    let tail = with_room.then(|| {
        let times_ms: Vec<f64> = (0..TAIL_POINTS)
            .map(|k| round(EARLY_MS * k as f64 / (TAIL_POINTS - 1) as f64, 4))
            .collect();
        let envelope = |ear: &Ear| -> Vec<f64> {
            let bin = EARLY_MS / (TAIL_POINTS - 1) as f64;
            times_ms
                .iter()
                .map(|t| {
                    let lo = ((t - bin / 2.0).max(0.0) * rate / 1000.0).round() as usize + first;
                    let hi = ((t + bin / 2.0) * rate / 1000.0).round() as usize + first;
                    let peak = (lo..=hi)
                        .filter_map(|n| ear.signal.get(n))
                        .fold(0.0f64, |m, v| m.max(v.abs()));
                    rel(peak.max(1e-12), TAIL_FLOOR_DB)
                })
                .collect()
        };
        Tail {
            left: envelope(&ears[0]),
            right: envelope(&ears[1]),
            times_ms,
        }
    });
    Arrivals {
        left: list(&ears[0]),
        right: list(&ears[1]),
        tail,
    }
}

/// A second-order Butterworth section (low or high pass) at `hz`.
fn biquad(low: bool, hz: f64, rate: f64) -> [f64; 5] {
    let w = 2.0 * std::f64::consts::PI * hz.min(rate * 0.45) / rate;
    let (sin, cos) = w.sin_cos();
    let alpha = sin / std::f64::consts::SQRT_2;
    let a0 = 1.0 + alpha;
    let (b0, b1) = match low {
        true => ((1.0 - cos) / 2.0, 1.0 - cos),
        false => ((1.0 + cos) / 2.0, -(1.0 + cos)),
    };
    [
        b0 / a0,
        b1 / a0,
        b0 / a0,
        -2.0 * cos / a0,
        (1.0 - alpha) / a0,
    ]
}

fn run(coef: &[f64; 5], input: &[f64]) -> Vec<f64> {
    let [b0, b1, b2, a1, a2] = *coef;
    let (mut x1, mut x2, mut y1, mut y2) = (0.0, 0.0, 0.0, 0.0);
    input
        .iter()
        .map(|&x| {
            let y = b0 * x + b1 * x1 + b2 * x2 - a1 * y1 - a2 * y2;
            (x2, x1, y2, y1) = (x1, x, y1, y);
            y
        })
        .collect()
}

/// Schroeder's backward-integrated energy of `bands`' four paths summed, in dB from the first
/// point, on `times_ms`.
fn schroeder(paths: &[Vec<f64>], times_ms: &[f64], rate: f64) -> Vec<f64> {
    let len = paths.iter().map(Vec::len).max().unwrap_or(0);
    let mut energy = vec![0.0f64; len + 1];
    for n in (0..len).rev() {
        let now: f64 = paths.iter().map(|p| p.get(n).map_or(0.0, |v| v * v)).sum();
        energy[n] = energy[n + 1] + now;
    }
    let index = |t: f64| ((t * rate / 1000.0).round() as usize).min(len.saturating_sub(1));
    let start = times_ms.first().map_or(0.0, |t| energy[index(*t)]);
    times_ms
        .iter()
        .enumerate()
        .map(|(i, t)| match (i, start > 0.0) {
            (0, _) => 0.0,
            (_, false) => DECAY_FLOOR_DB,
            (_, true) => {
                let db = 10.0 * (energy[index(*t)] / start).max(1e-30).log10();
                round(db.max(DECAY_FLOOR_DB), 2)
            }
        })
        .collect()
}

fn decay(irs: &[Vec<f32>; 4], rate: f64, len: usize) -> Decay {
    if len == 0 {
        return Decay::default();
    }
    let end_ms = len as f64 * 1000.0 / rate;
    let start_ms = 0.1f64;
    let index = |t: f64| ((t * rate / 1000.0).round() as usize).min(len - 1);
    let times_ms: Vec<f64> = match end_ms > start_ms {
        true => {
            let mut times: Vec<f64> = Vec::with_capacity(DECAY_POINTS);
            let mut last_index = None;
            for k in 0..DECAY_POINTS {
                let t = round(
                    start_ms * (end_ms / start_ms).powf(k as f64 / (DECAY_POINTS - 1) as f64),
                    4,
                );
                // Points on one sample say the same: keep the first (the last point stays).
                match (last_index == Some(index(t)), k == DECAY_POINTS - 1) {
                    (false, _) => times.push(t),
                    (true, true) => *times.last_mut().unwrap() = t,
                    (true, false) => continue,
                }
                last_index = Some(index(t));
            }
            times
        }
        false => vec![round(end_ms, 4)],
    };
    let paths: Vec<Vec<f64>> = irs
        .iter()
        .map(|taps| taps.iter().map(|t| f64::from(*t)).collect())
        .collect();
    let band = |coefs: &[[f64; 5]]| -> Vec<f64> {
        let filtered: Vec<Vec<f64>> = paths
            .iter()
            .map(|path| coefs.iter().fold(path.clone(), |sig, c| run(c, &sig)))
            .collect();
        schroeder(&filtered, &times_ms, rate)
    };
    let lp250 = biquad(true, 250.0, rate);
    let hp250 = biquad(false, 250.0, rate);
    let lp4k = biquad(true, 4000.0, rate);
    let hp4k = biquad(false, 4000.0, rate);
    Decay {
        low: band(&[lp250]),
        mid: band(&[hp250, lp4k]),
        high: band(&[hp4k]),
        end_ms: round(end_ms, 4),
        times_ms,
    }
}

#[cfg(test)]
mod tests;
