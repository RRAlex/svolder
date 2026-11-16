// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The four impulse responses of a pair of virtual speakers and what is done to them before they
//! play: the common lead-in fit, the length cap, the level and the numbers a screen shows.
//!
//! The channels are `LL` (left speaker to the left ear), `LR` (left speaker to the right ear),
//! `RL` and `RR`, in that order everywhere.

use crate::error::ConvertError;

/// How many points the curve has: 20 per decade from 20 Hz to 20 kHz, ends included.
pub const CURVE_POINTS: usize = 61;

/// The longest an impulse response plays, in seconds; longer ones are cut with a fade.
pub const MAX_SECONDS: f64 = 0.5;

/// The level every set is scaled to: its worst gain, in dB.
pub const TARGET_DB: f64 = -0.1;

/// The four impulse responses at one sample rate.
#[derive(Debug, Clone, PartialEq)]
pub struct Irs {
    /// The sample rate in Hz.
    pub rate: u32,
    /// `LL`, `LR`, `RL`, `RR`.
    pub ch: [Vec<f32>; 4],
}

/// A complex number: one channel's response at one frequency.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Cx {
    /// The real part.
    pub re: f64,
    /// The imaginary part.
    pub im: f64,
}

/// Where a speaker's measurement was taken from, as found in the source.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Direction {
    /// Azimuth in degrees, 0 to the front, counter-clockwise, in 0 to 360.
    pub az_deg: f64,
    /// Elevation in degrees.
    pub el_deg: f64,
    /// Distance in metres.
    pub distance_m: f64,
}

/// The two speakers' directions as found.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Angles {
    /// The left speaker.
    pub left: Direction,
    /// The right speaker.
    pub right: Direction,
}

/// The magnitude of each of the four paths, in dB on [`CURVE_POINTS`] log-spaced frequencies.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Curve {
    /// The frequencies in Hz, 20 Hz to 20 kHz.
    pub hz: Vec<f64>,
    /// Left speaker to the left ear, dB (floored at -120), to 0.01 dB.
    pub ll_db: Vec<f64>,
    /// Left speaker to the right ear.
    pub lr_db: Vec<f64>,
    /// Right speaker to the left ear.
    pub rl_db: Vec<f64>,
    /// Right speaker to the right ear.
    pub rr_db: Vec<f64>,
}

/// What was done to a set and what it measures.
#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    /// The samples of each impulse response.
    pub taps: usize,
    /// The length in milliseconds.
    pub taps_ms: f64,
    /// Whether the responses were longer than [`MAX_SECONDS`] and cut.
    pub cut: bool,
    /// How many leading samples the common lead-in fit took off.
    pub trimmed: usize,
    /// How many zero samples were put in front of all four responses so the earliest onset sits
    /// 1 ms into them (0 when `trimmed` took samples off instead).
    pub padded: usize,
    /// The gain applied to every response, in dB.
    pub scale_db: f64,
    /// The worst gain after scaling over the whole band, 0 Hz to the Nyquist rate: `max over f of
    /// max(|LL|+|RL|, |LR|+|RR|)`, in dB, found to within 0.04 dB.
    pub worst_gain_db: f64,
    /// How much quieter (negative) or louder a centred pink-weighted signal is than with no
    /// virtual speakers, in dB.
    pub loudness_db: f64,
    /// The four magnitudes.
    pub curve: Curve,
}

/// The response of `irs` at `hz` (a frequency at or above the Nyquist rate is clamped under it).
pub fn response_at(irs: &[Vec<f32>; 4], rate: u32, hz: f64) -> [Cx; 4] {
    let rate = f64::from(rate.max(1));
    let w = 2.0 * std::f64::consts::PI * hz.min(rate * 0.499) / rate;
    irs.each_ref().map(|taps| dft(taps, w))
}

/// The sum of `taps[n] e^(-j w n)`, the rotation advanced by one complex product a tap.
fn dft(taps: &[f32], w: f64) -> Cx {
    let (step_im, step_re) = (-w).sin_cos();
    let (mut re, mut im) = (0.0, 0.0);
    let (mut rot_re, mut rot_im) = (1.0f64, 0.0f64);
    for (n, &tap) in taps.iter().enumerate() {
        let tap = f64::from(tap);
        re += tap * rot_re;
        im += tap * rot_im;
        let next_re = rot_re * step_re - rot_im * step_im;
        let next_im = rot_re * step_im + rot_im * step_re;
        rot_re = next_re;
        rot_im = next_im;
        // The product drifts by about 1e-16 a step; a fresh rotation every 4096 taps keeps it
        // below 1e-12 over any response this crate holds.
        if n % 4096 == 4095 {
            let (s, c) = (-w * (n + 1) as f64).sin_cos();
            (rot_re, rot_im) = (c, s);
        }
    }
    Cx { re, im }
}

fn magnitude(c: Cx) -> f64 {
    c.re.hypot(c.im)
}

/// The 1/48-octave grid from 20 Hz to 20 kHz (the Nyquist rate bounds it) the loudness is worked
/// out on.
fn level_grid(rate: u32) -> Vec<f64> {
    let top = 20000.0f64.min(f64::from(rate) * 0.499);
    (0..)
        .map(|k| 20.0 * 2f64.powf(f64::from(k) / 48.0))
        .take_while(|hz| *hz <= top * (1.0 + 1e-9))
        .collect()
}

/// The mean power of a centred signal over the level grid (linear).
fn centred_power(irs: &Irs) -> f64 {
    let (mut power, mut points) = (0.0f64, 0.0f64);
    for hz in level_grid(irs.rate) {
        let [ll, lr, rl, rr] = response_at(&irs.ch, irs.rate, hz);
        let left = Cx {
            re: ll.re + rl.re,
            im: ll.im + rl.im,
        };
        let right = Cx {
            re: lr.re + rr.re,
            im: lr.im + rr.im,
        };
        power += (magnitude(left).powi(2) + magnitude(right).powi(2)) / 2.0;
        points += 1.0;
    }
    power / points.max(1.0)
}

/// How many times finer than the responses' own resolution the worst gain is looked for: at 16
/// times, a peak between two points is under the points' maximum by at most 0.04 dB.
const OVERSAMPLING: usize = 16;

/// The worst gain (linear) of `irs` over the whole band, 0 Hz to the Nyquist rate:
/// `max over f of max(|LL|+|RL|, |LR|+|RR|)`, found on a uniform grid of a zero-padded FFT.
fn worst_gain(irs: &Irs) -> f64 {
    let longest = irs.ch.iter().map(Vec::len).max().unwrap_or(0).max(1);
    let size = (longest * OVERSAMPLING).next_power_of_two().max(1024);
    let [ll, lr, rl, rr] = irs
        .ch
        .each_ref()
        .map(|taps| spectrum_magnitudes(taps, size));
    (0..=size / 2)
        .map(|k| (ll[k] + rl[k]).max(lr[k] + rr[k]))
        .fold(0.0, f64::max)
}

/// The magnitude of the FFT of `taps` zero-padded to `size` (a power of two), bins 0 to size/2.
fn spectrum_magnitudes(taps: &[f32], size: usize) -> Vec<f64> {
    let mut re = vec![0.0f64; size];
    let mut im = vec![0.0f64; size];
    for (slot, tap) in re.iter_mut().zip(taps) {
        *slot = f64::from(*tap);
    }
    fft(&mut re, &mut im);
    (0..=size / 2).map(|k| re[k].hypot(im[k])).collect()
}

/// An in-place radix-2 FFT of a power-of-two length.
fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let half = len / 2;
        // One twiddle table per stage, each factor from its own angle: no drift.
        let table: Vec<(f64, f64)> = (0..half)
            .map(|k| (-2.0 * std::f64::consts::PI * k as f64 / len as f64).sin_cos())
            .collect();
        for start in (0..n).step_by(len) {
            for (k, &(sin, cos)) in table.iter().enumerate() {
                let (a, b) = (start + k, start + k + half);
                let (tr, ti) = (re[b] * cos - im[b] * sin, re[b] * sin + im[b] * cos);
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

fn db(gain: f64) -> f64 {
    20.0 * gain.log10()
}

fn curve(irs: &Irs) -> Curve {
    let db01 = |gain: f64| ((20.0 * gain.max(1e-6).log10()) * 100.0).round() / 100.0;
    let mut curve = Curve::default();
    for i in 0..CURVE_POINTS {
        let hz = 20.0 * 10f64.powf(i as f64 / 20.0);
        let [ll, lr, rl, rr] = response_at(&irs.ch, irs.rate, hz);
        curve.hz.push((hz * 100.0).round() / 100.0);
        curve.ll_db.push(db01(magnitude(ll)));
        curve.lr_db.push(db01(magnitude(lr)));
        curve.rl_db.push(db01(magnitude(rl)));
        curve.rr_db.push(db01(magnitude(rr)));
    }
    curve
}

/// The index of the first sample of `taps` at or above a tenth (-20 dB) of its peak, or `None`
/// for silence.
fn onset(taps: &[f32]) -> Option<usize> {
    let peak = taps.iter().fold(0.0f32, |peak, v| peak.max(v.abs()));
    (peak > 0.0).then(|| taps.iter().position(|v| v.abs() >= peak * 0.1))?
}

/// Give the earliest onset exactly 1 ms of lead-in, the same count on all four responses so the
/// time and level differences between them stay: drop the samples before it, less 1 ms, or put
/// zeros in front when there is less than 1 ms. Returns `(dropped, padded)`.
fn fit_lead_in(irs: &mut Irs) -> Result<(usize, usize), ConvertError> {
    let earliest = irs.ch.iter().filter_map(|taps| onset(taps)).min();
    let earliest = earliest.ok_or(ConvertError::Silent)?;
    let lead_in = (f64::from(irs.rate) * 0.001).round() as usize;
    let drop = earliest.saturating_sub(lead_in);
    let pad = lead_in.saturating_sub(earliest);
    for taps in &mut irs.ch {
        taps.drain(..drop.min(taps.len()));
        if pad > 0 {
            taps.splice(..0, std::iter::repeat_n(0.0, pad));
        }
    }
    Ok((drop, pad))
}

/// Cut the responses to [`MAX_SECONDS`] with a 10 ms half-cosine fade at the end; whether they
/// were cut.
fn cap_length(irs: &mut Irs) -> bool {
    let cap = (f64::from(irs.rate) * MAX_SECONDS).round() as usize;
    if irs.ch.iter().all(|taps| taps.len() <= cap) {
        return false;
    }
    let fade = (f64::from(irs.rate) * 0.01).round() as usize;
    for taps in &mut irs.ch {
        taps.resize(cap, 0.0);
        for k in 0..fade.min(cap) {
            let weight = 0.5 * (1.0 + (std::f64::consts::PI * (k + 1) as f64 / fade as f64).cos());
            let at = cap - fade + k;
            taps[at] = (f64::from(taps[at]) * weight) as f32;
        }
    }
    true
}

/// Fit the lead-in, cap, scale and measure `irs`.
pub fn prepare(mut irs: Irs) -> Result<(Irs, Report), ConvertError> {
    if !(8000..=384_000).contains(&irs.rate) {
        return Err(ConvertError::Rate(irs.rate));
    }
    if irs.ch.iter().flatten().any(|tap| !tap.is_finite()) {
        return Err(ConvertError::NotNumbers);
    }
    let (trimmed, padded) = fit_lead_in(&mut irs)?;
    let longest = irs.ch.iter().map(Vec::len).max().unwrap_or(0);
    for taps in &mut irs.ch {
        taps.resize(longest, 0.0);
    }
    let cut = cap_length(&mut irs);
    let worst = worst_gain(&irs);
    if worst <= 0.0 || !worst.is_finite() {
        return Err(ConvertError::Silent);
    }
    let scale = 10f64.powf(TARGET_DB / 20.0) / worst;
    for taps in &mut irs.ch {
        for tap in taps.iter_mut() {
            *tap = (f64::from(*tap) * scale) as f32;
        }
    }
    // The numbers are those of the f32 samples that play, after the scale.
    let (worst, power) = (worst_gain(&irs), centred_power(&irs));
    let taps = irs.ch[0].len();
    let report = Report {
        taps,
        taps_ms: taps as f64 * 1000.0 / f64::from(irs.rate),
        cut,
        trimmed,
        padded,
        scale_db: db(scale),
        worst_gain_db: db(worst),
        loudness_db: 10.0 * power.log10(),
        curve: curve(&irs),
    };
    Ok((irs, report))
}

#[cfg(test)]
mod tests;
