// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! The tone control's response curve: what a set of bands does to the frequency response.
//!
//! Each band is one peaking biquad, as the device's own DSP runs it, and the curve is the sum
//! of their magnitudes in dB over log-spaced frequencies. [`correction_db`] sums a headphone
//! correction's peaking and shelf filters the same way. Nothing here draws anything: a
//! screen samples the curve for the columns it has and hands the points to a chart, and
//! [`peak_db`] with [`threshold_step`] say how far the curve boosts and from which volume step
//! that boost can clip. [`clipping_peak`] says, from a device's own readings, whether there is
//! a boost to warn about at all: every front end asks it, so none of them can disagree.

use svolder_proto::FunctionId;

use crate::convert::{EqMode, EqParam};
use crate::model::EqSpec;
use crate::tone::apo::{Correction, FilterKind};

/// One band as a peaking filter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BandParams {
    /// Centre frequency in Hz.
    pub f0: f64,
    /// Gain at the centre, in dB.
    pub gain_db: f64,
    /// How narrow the band is.
    pub q: f64,
}

/// The sample rate the biquads are worked out at.
const SAMPLE_RATE_HZ: f64 = 48_000.0;
/// The lowest frequency the curve covers.
pub const F_MIN: f64 = 20.0;
/// The highest.
pub const F_MAX: f64 = 20_000.0;
/// How many log-spaced points [`peak_db`] samples the curve at before it refines.
const PEAK_SAMPLES: usize = 1024;
/// How wide, in octaves, the window each band's centre is refined in.
const PEAK_WINDOW_OCTAVES: f64 = 0.5;
/// Golden-section steps per refinement: each keeps 0.618 of the window, so 60 of them leave
/// far less than a hertz anywhere in the audio band.
const PEAK_STEPS: usize = 60;
/// The smallest peak [`clipping_peak`] warns about, in dB: the least that reads as `+0.1` at the
/// one decimal every front end prints it to. A smaller one — two overlapping bands of opposite
/// sign reach such peaks — would say `peaks at +0.0 dB`, which reads as no peak at all.
pub const PEAK_SHOWN_DB: f64 = 0.05;

/// Summed magnitude response in dB of peaking biquads at 48 kHz, evaluated at `freqs`.
pub fn response_db(bands: &[BandParams], freqs: &[f64]) -> Vec<f64> {
    let biquads: Vec<Biquad> = bands.iter().map(Biquad::peaking).collect();
    freqs.iter().map(|&f| summed_db(&biquads, f)).collect()
}

/// Summed magnitude response in dB of a correction's filters, peaking and shelves, at 48 kHz,
/// evaluated at `freqs`. The preamp is not part of the shape and is left out.
pub fn correction_db(correction: &Correction, freqs: &[f64]) -> Vec<f64> {
    let biquads = correction_biquads(correction, SAMPLE_RATE_HZ);
    freqs.iter().map(|&f| summed_db(&biquads, f)).collect()
}

/// The biquad of each of a correction's filters, worked out for a stream of `rate` Hz.
fn correction_biquads(correction: &Correction, rate: f64) -> Vec<Biquad> {
    correction
        .filters
        .iter()
        .map(|filter| match filter.kind {
            FilterKind::Peaking => Biquad::peaking_at(
                &BandParams {
                    f0: filter.f,
                    gain_db: filter.g,
                    q: filter.q,
                },
                rate,
            ),
            FilterKind::LowShelf => Biquad::low_shelf_at(filter.f, filter.g, filter.q, rate),
            FilterKind::HighShelf => Biquad::high_shelf_at(filter.f, filter.g, filter.q, rate),
        })
        .collect()
}

/// The cookbook coefficients `[b0, b1, b2, a1, a2]` of one filter for a stream of `rate_hz`,
/// divided by `a0`, which is 1 after it. The frequency is held below the Nyquist limit, where
/// the cookbook's filters stop being filters; a Q that is not above 0 gives the identity.
pub fn coefficients(kind: FilterKind, f: f64, g: f64, q: f64, rate_hz: u32) -> [f64; 5] {
    let rate = f64::from(rate_hz.max(1));
    let f = f.min(rate * 0.499);
    let biquad = match kind {
        FilterKind::Peaking => Biquad::peaking_at(
            &BandParams {
                f0: f,
                gain_db: g,
                q,
            },
            rate,
        ),
        FilterKind::LowShelf => Biquad::low_shelf_at(f, g, q, rate),
        FilterKind::HighShelf => Biquad::high_shelf_at(f, g, q, rate),
    };
    let a0 = biquad.a[0];
    [
        biquad.b[0] / a0,
        biquad.b[1] / a0,
        biquad.b[2] / a0,
        biquad.a[1] / a0,
        biquad.a[2] / a0,
    ]
}

/// `n` log-spaced frequencies from 20 Hz to 20 kHz.
pub fn log_freqs(n: usize) -> Vec<f64> {
    if n < 2 {
        return vec![F_MIN; n];
    }
    let last = (n - 1) as f64;
    let decades = (F_MAX / F_MIN).log10();
    (0..n)
        .map(|i| F_MIN * 10f64.powf(decades * i as f64 / last))
        .collect()
}

/// The highest the summed curve rises between 20 Hz and 20 kHz, in dB: 0.0 or below for a
/// flat or cut-only curve, and 0.0 for no bands at all.
///
/// The curve is sampled at 1024 log-spaced points, then refined by a golden-section search in
/// a half-octave window centred on each band's `f0` and in the two sample intervals either
/// side of the highest sample, so two overlapping bands are measured where they actually add
/// rather than where a sample happens to fall.
pub fn peak_db(bands: &[BandParams]) -> f64 {
    let biquads: Vec<Biquad> = bands.iter().map(Biquad::peaking).collect();
    refined_peak(&biquads, bands.iter().map(|band| band.f0), SAMPLE_RATE_HZ)
}

/// The highest a correction's filters (peaking and shelves, without the preamp) rise between
/// 20 Hz and 20 kHz, in dB, found the way [`peak_db`] finds it: a narrow boost between two
/// samples is measured at its true height. 0.0 or below for a flat or cut-only correction.
pub fn correction_peak_db(correction: &Correction) -> f64 {
    correction_peak_db_at(correction, SAMPLE_RATE_HZ as u32)
}

/// [`correction_peak_db`] for a stream of `rate_hz`: the filters are worked out, and the curve
/// is read, at that rate, so a shelf near Nyquist is measured where it plays.
pub fn correction_peak_db_at(correction: &Correction, rate_hz: u32) -> f64 {
    let rate = f64::from(rate_hz.max(1));
    let biquads = correction_biquads(correction, rate);
    refined_peak(
        &biquads,
        correction.filters.iter().map(|filter| filter.f),
        rate,
    )
}

/// The lowest frequency the played band is sampled from, in Hz: under the 1 Hz the host holds
/// a filter's frequency to, so the bump a high-Q low shelf makes below its corner is inside.
const PLAYED_LOW_HZ: f64 = 0.1;
/// How close to Nyquist the played band's sampled grid ends, as a fraction of the rate; the
/// curve is read at Nyquist itself exactly as well.
const PLAYED_HIGH_OF_RATE: f64 = 0.4999;
/// Log-spaced samples over the played band: about 650 a decade.
const PLAYED_SAMPLES: usize = 4096;
/// Samples spaced logarithmically by their distance from Nyquist, from `pi * 1e-7` to `pi / 2`
/// radians: a peaking filter near Nyquist is only a few hertz wide, so the grid must be dense
/// there in distance, not in frequency.
const PLAYED_NYQUIST_SAMPLES: usize = 2048;

/// The highest a correction's filters rise anywhere the chain plays, in dB, with every
/// coefficient as the f64 cookbook gives it. See [`played_peak_db_of`].
pub fn correction_played_peak_db(correction: &Correction, rate_hz: u32) -> f64 {
    let sets: Vec<[f64; 5]> = correction
        .filters
        .iter()
        .map(|f| coefficients(f.kind, f.f, f.g, f.q, rate_hz))
        .collect();
    let centres: Vec<f64> = correction.filters.iter().map(|f| f.f).collect();
    played_peak_db_of(&sets, &centres, rate_hz)
}

/// The highest the filters whose coefficients are `sets` (`[b0, b1, b2, a1, a2]`, `a0` being 1)
/// rise anywhere the chain plays, in dB: from 0.1 Hz to just under Nyquist at `rate_hz`, with DC
/// and Nyquist read exactly, since a shelf's asymptotes and a resonant shelf's bump sit outside
/// 20 Hz to 20 kHz. The samples are a dense log grid in frequency, a second grid log-spaced in
/// the distance from Nyquist, and each filter's own centre in `centres_hz`; every local maximum
/// of them is refined by a golden-section search between its neighbours, so a narrow peak
/// between two samples is measured at its true height. Below 0.0 only for a cut-only set; 0.0
/// for none.
///
/// This is what the computer EQ's preamp covers. The DAC's displayed boost
/// ([`correction_peak_db_at`]) stays the 20 Hz to 20 kHz window the official app draws.
pub fn played_peak_db_of(sets: &[[f64; 5]], centres_hz: &[f64], rate_hz: u32) -> f64 {
    use std::f64::consts::PI;
    let rate = f64::from(rate_hz.max(1));
    let at = |w: f64| played_db(sets, w);
    let mut ws: Vec<f64> = vec![0.0, PI];
    let (low, high) = (PLAYED_LOW_HZ.log10(), (rate * PLAYED_HIGH_OF_RATE).log10());
    let last = (PLAYED_SAMPLES - 1) as f64;
    ws.extend(
        (0..PLAYED_SAMPLES)
            .map(|i| 2.0 * PI * 10f64.powf(low + (high - low) * i as f64 / last) / rate),
    );
    let (near, far) = ((PI * 1e-7).log10(), (PI / 2.0).log10());
    let last = (PLAYED_NYQUIST_SAMPLES - 1) as f64;
    ws.extend(
        (0..PLAYED_NYQUIST_SAMPLES).map(|i| PI - 10f64.powf(near + (far - near) * i as f64 / last)),
    );
    ws.extend(
        centres_hz
            .iter()
            .filter(|f| f.is_finite() && **f > 0.0)
            .map(|f| (2.0 * PI * f / rate).min(PI)),
    );
    ws.sort_by(f64::total_cmp);
    ws.dedup();
    let db: Vec<f64> = ws.iter().map(|&w| at(w)).collect();
    let mut peak = db.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let last = ws.len() - 1;
    for i in 0..=last {
        let rises = i == 0 || db[i] > db[i - 1];
        if rises && (i == last || db[i] >= db[i + 1]) {
            let (from, to) = (ws[i.saturating_sub(1)], ws[(i + 1).min(last)]);
            if from < to {
                peak = peak.max(golden_max(at, from, to));
            }
        }
    }
    peak
}

/// The summed response of `sets` at angular frequency `w`, in dB, worked out in `sin^2(w/2)`
/// below a quarter of the rate and in `cos^2(w/2)` above, so a response near DC or Nyquist is
/// not lost to cancellation.
fn played_db(sets: &[[f64; 5]], w: f64) -> f64 {
    let half = w / 2.0;
    let (sign, x) = match half <= std::f64::consts::FRAC_PI_4 {
        true => (1.0, half.sin().powi(2)),
        false => (-1.0, half.cos().powi(2)),
    };
    let power = |p0: f64, p1: f64, p2: f64| {
        let sum = p0 + p1 + p2;
        sum * sum - 4.0 * (p0 * p1 + 4.0 * p0 * p2 + p1 * p2) * x + 16.0 * p0 * p2 * x * x
    };
    sets.iter()
        .map(|c| {
            let numerator = power(c[0], sign * c[1], c[2]);
            let denominator = power(1.0, sign * c[3], c[4]);
            10.0 * (numerator / denominator).log10()
        })
        .sum()
}

/// The peak of the summed `biquads`: the highest of 1024 log-spaced samples, refined by a
/// golden-section search in the two sample intervals either side of it and in a half-octave
/// window round each of `centres`.
fn refined_peak(biquads: &[Biquad], centres: impl Iterator<Item = f64>, rate: f64) -> f64 {
    let at = |log_hz: f64| summed_db_at(biquads, 10f64.powf(log_hz), rate);
    let grid: Vec<f64> = log_freqs(PEAK_SAMPLES)
        .into_iter()
        .map(f64::log10)
        .collect();
    let (best, mut peak) = grid.iter().map(|&log_hz| at(log_hz)).enumerate().fold(
        (0, f64::NEG_INFINITY),
        |(i, max), (j, db)| match db > max {
            true => (j, db),
            false => (i, max),
        },
    );
    let (low, high) = (F_MIN.log10(), F_MAX.log10());
    let half_window = PEAK_WINDOW_OCTAVES / 2.0 * 2f64.log10();
    let mut windows = vec![(
        grid[best.saturating_sub(1)],
        grid[(best + 1).min(grid.len() - 1)],
    )];
    windows.extend(
        centres
            .filter(|f0| *f0 > 0.0 && f0.is_finite())
            .map(|f0| {
                let centre = f0.log10();
                (
                    (centre - half_window).max(low),
                    (centre + half_window).min(high),
                )
            })
            .filter(|(from, to)| from < to),
    );
    for (from, to) in windows {
        peak = peak.max(golden_max(at, from, to));
    }
    peak
}

/// Every band of `eq` as the response math takes it, from the raw readings `raw` gives: `None`
/// while any band has not reported its frequency, its gain and its Q, or reads a value no byte
/// holds.
pub fn bands_of(eq: &EqSpec, raw: impl Fn(FunctionId) -> Option<i64>) -> Option<Vec<BandParams>> {
    let step = |band: usize, param: EqParam| -> Option<u8> {
        u8::try_from(raw(eq.band_id(band, param)?)?).ok()
    };
    (0..eq.bands.len())
        .map(|band| {
            Some(BandParams {
                f0: eq.frequency(band, step(band, EqParam::Frequency)?)?,
                gain_db: eq.gain_db(step(band, EqParam::Gain)?),
                q: eq.q(step(band, EqParam::Q)?),
            })
        })
        .collect()
}

/// How far the curve `eq`'s readings describe boosts, in dB, when that boost can clip: the
/// summed curve's [`peak_db`] while the tone control reads EQ mode, every band has reported in
/// full, and the peak is at least [`PEAK_SHOWN_DB`].
///
/// `None` otherwise: in Flat mode no band applies, in Loudness mode the boost is the device's
/// own unmeasured contour, a curve not read in full is not guessed at, a flat or cut-only
/// curve clips at no volume, and a peak too small to read as a tenth of a dB is not worth a
/// warning that would print as `+0.0`. The CLI's `peak` line and the TUI's zone and note all
/// ask this, so none of them can disagree.
pub fn clipping_peak(eq: &EqSpec, raw: impl Fn(FunctionId) -> Option<i64>) -> Option<f64> {
    let mode = u8::try_from(raw(eq.mode_id())?)
        .ok()
        .and_then(EqMode::from_raw)?;
    if mode != EqMode::Eq {
        return None;
    }
    let peak = peak_db(&bands_of(eq, &raw)?);
    (peak >= PEAK_SHOWN_DB).then_some(peak)
}

/// The highest step of an attenuation table whose dB is at or below `-peak_db`: the loudest
/// volume at which a curve that boosts by `peak_db` stays clear of clipping on a full-scale
/// master. Every step above it can clip.
///
/// `None` for a peak at or under 0 dB, which clips at no volume, and for a peak that is not
/// a number. The table is the profile's `dac-attenuation-db`, ascending from silence at index
/// 0 to 0 dB at index 127.
pub fn threshold_step(peak_db: f64, table: &[f64]) -> Option<usize> {
    if peak_db <= 0.0 {
        return None;
    }
    // A peak that is not a number compares below nothing, so it finds no step either.
    table.iter().rposition(|&db| db <= -peak_db)
}

/// The highest value `f` takes between `from` and `to`, found by golden-section search: the
/// window shrinks round the higher of two probes until it is too narrow to matter.
fn golden_max(f: impl Fn(f64) -> f64, mut from: f64, mut to: f64) -> f64 {
    let ratio = (5f64.sqrt() - 1.0) / 2.0;
    let mut left = to - ratio * (to - from);
    let mut right = from + ratio * (to - from);
    let (mut at_left, mut at_right) = (f(left), f(right));
    for _ in 0..PEAK_STEPS {
        if at_left < at_right {
            from = left;
            (left, at_left) = (right, at_right);
            right = from + ratio * (to - from);
            at_right = f(right);
        } else {
            to = right;
            (right, at_right) = (left, at_left);
            left = to - ratio * (to - from);
            at_left = f(left);
        }
    }
    at_left.max(at_right)
}

/// The sum of every biquad's lift or cut at `hz`, in dB.
fn summed_db(biquads: &[Biquad], hz: f64) -> f64 {
    summed_db_at(biquads, hz, SAMPLE_RATE_HZ)
}

/// The same for a stream of `rate` Hz.
fn summed_db_at(biquads: &[Biquad], hz: f64, rate: f64) -> f64 {
    let w = 2.0 * std::f64::consts::PI * hz / rate;
    biquads.iter().map(|biquad| biquad.magnitude_db(w)).sum()
}

/// One band's biquad coefficients, the denominator's `a0` divided out of neither half since
/// the magnitude is a ratio.
struct Biquad {
    /// Numerator coefficients, `b0`, `b1`, `b2`.
    b: [f64; 3],
    /// Denominator coefficients, `a0`, `a1`, `a2`.
    a: [f64; 3],
}

impl Biquad {
    /// A filter that changes nothing: what a Q that is not above 0 stands for, where the
    /// cookbook's `alpha` would divide by it.
    fn identity() -> Biquad {
        Biquad {
            b: [1.0, 0.0, 0.0],
            a: [1.0, 0.0, 0.0],
        }
    }

    /// The peaking filter one band describes.
    fn peaking(band: &BandParams) -> Biquad {
        Biquad::peaking_at(band, SAMPLE_RATE_HZ)
    }

    /// The peaking filter one band describes, worked out for a stream of `rate` Hz.
    fn peaking_at(band: &BandParams, rate: f64) -> Biquad {
        if !usable_q(band.q) {
            return Biquad::identity();
        }
        let amplitude = 10f64.powf(band.gain_db / 40.0);
        let w0 = 2.0 * std::f64::consts::PI * band.f0 / rate;
        let alpha = w0.sin() / (2.0 * band.q);
        let cos_w0 = w0.cos();
        Biquad {
            b: [
                1.0 + alpha * amplitude,
                -2.0 * cos_w0,
                1.0 - alpha * amplitude,
            ],
            a: [
                1.0 + alpha / amplitude,
                -2.0 * cos_w0,
                1.0 - alpha / amplitude,
            ],
        }
    }

    /// The cookbook's low shelf for a stream of `rate` Hz, its Q the cookbook's Q.
    fn low_shelf_at(f0: f64, gain_db: f64, q: f64, rate: f64) -> Biquad {
        if !usable_q(q) {
            return Biquad::identity();
        }
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f64::consts::PI * f0 / rate;
        let cos_w0 = w0.cos();
        let beta = 2.0 * a.sqrt() * (w0.sin() / (2.0 * q));
        Biquad {
            b: [
                a * ((a + 1.0) - (a - 1.0) * cos_w0 + beta),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cos_w0),
                a * ((a + 1.0) - (a - 1.0) * cos_w0 - beta),
            ],
            a: [
                (a + 1.0) + (a - 1.0) * cos_w0 + beta,
                -2.0 * ((a - 1.0) + (a + 1.0) * cos_w0),
                (a + 1.0) + (a - 1.0) * cos_w0 - beta,
            ],
        }
    }

    /// The cookbook's high shelf for a stream of `rate` Hz, its Q the cookbook's Q.
    fn high_shelf_at(f0: f64, gain_db: f64, q: f64, rate: f64) -> Biquad {
        if !usable_q(q) {
            return Biquad::identity();
        }
        let a = 10f64.powf(gain_db / 40.0);
        let w0 = 2.0 * std::f64::consts::PI * f0 / rate;
        let cos_w0 = w0.cos();
        let beta = 2.0 * a.sqrt() * (w0.sin() / (2.0 * q));
        Biquad {
            b: [
                a * ((a + 1.0) + (a - 1.0) * cos_w0 + beta),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cos_w0),
                a * ((a + 1.0) + (a - 1.0) * cos_w0 - beta),
            ],
            a: [
                (a + 1.0) - (a - 1.0) * cos_w0 + beta,
                2.0 * ((a - 1.0) - (a + 1.0) * cos_w0),
                (a + 1.0) - (a - 1.0) * cos_w0 - beta,
            ],
        }
    }

    /// How much the filter lifts or cuts at digital frequency `w`, in dB.
    fn magnitude_db(&self, w: f64) -> f64 {
        self.magnitude_db_at(trig(w))
    }

    /// The same at a frequency whose [`trig`] is already worked out.
    fn magnitude_db_at(&self, trig: [f64; 4]) -> f64 {
        let (numerator, denominator) = (polar(self.b, trig), polar(self.a, trig));
        if denominator <= 0.0 {
            return 0.0;
        }
        10.0 * (numerator / denominator).log10()
    }
}

/// `cos w`, `sin w`, `cos 2w` and `sin 2w`.
fn trig(w: f64) -> [f64; 4] {
    [w.cos(), w.sin(), (2.0 * w).cos(), (2.0 * w).sin()]
}

/// A set of frequencies with their trigonometry worked out once, for a fit that evaluates one
/// band at the same points thousands of times.
pub(crate) struct Grid {
    trig: Vec<[f64; 4]>,
}

impl Grid {
    /// The grid for `freqs` in Hz.
    pub(crate) fn new(freqs: &[f64]) -> Grid {
        Grid {
            trig: freqs
                .iter()
                .map(|f| trig(2.0 * std::f64::consts::PI * f / SAMPLE_RATE_HZ))
                .collect(),
        }
    }

    /// One band's response in dB at every point: what [`response_db`] gives for it alone.
    pub(crate) fn band_db(&self, band: &BandParams) -> Vec<f64> {
        let biquad = Biquad::peaking(band);
        self.trig
            .iter()
            .map(|&t| biquad.magnitude_db_at(t))
            .collect()
    }
}

/// Whether a Q can stand in the cookbook's `alpha`: above 0, which no NaN is.
fn usable_q(q: f64) -> bool {
    q > 0.0
}

/// The squared magnitude of `c0 + c1·e^-jw + c2·e^-2jw`, given `w`'s [`trig`].
fn polar(c: [f64; 3], [cos1, sin1, cos2, sin2]: [f64; 4]) -> f64 {
    let real = c[0] + c[1] * cos1 + c[2] * cos2;
    let imaginary = -(c[1] * sin1 + c[2] * sin2);
    real * real + imaginary * imaginary
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::registry::Registry;

    /// The five bands of the built-in five-band tone control, all at 0 dB.
    fn flat() -> Vec<BandParams> {
        [40.0, 160.0, 632.0, 2515.0, 10_000.0]
            .into_iter()
            .map(|f0| BandParams {
                f0,
                gain_db: 0.0,
                q: 4.3,
            })
            .collect()
    }

    #[test]
    fn flat_bands_give_zero_db_everywhere() {
        let freqs = log_freqs(64);
        for (f, db) in freqs.iter().zip(response_db(&flat(), &freqs)) {
            assert!(db.abs() < 1e-9, "{db} dB at {f} Hz, expected none");
        }
    }

    #[test]
    fn one_band_peaks_at_its_centre_with_its_gain() {
        let band = [BandParams {
            f0: 1000.0,
            gain_db: 6.0,
            q: 1.0,
        }];
        let db = response_db(&band, &[20.0, 1000.0]);
        assert!(
            (db[1] - 6.0).abs() < 0.05,
            "expected about +6 dB at its centre, got {}",
            db[1]
        );
        assert!(
            db[0].abs() < 0.5,
            "expected next to nothing at 20 Hz, got {}",
            db[0]
        );
    }

    #[test]
    fn log_freqs_spans_20_to_20k() {
        let freqs = log_freqs(100);
        assert_eq!(freqs.len(), 100);
        assert!((freqs[0] - 20.0).abs() < 1e-9, "starts at {}", freqs[0]);
        assert!((freqs[99] - 20_000.0).abs() < 1e-6, "ends at {}", freqs[99]);
        assert!(
            freqs.windows(2).all(|pair| pair[1] > pair[0]),
            "the frequencies do not rise"
        );
        // Log-spaced: every step multiplies by the same factor.
        let step = freqs[1] / freqs[0];
        assert!(
            freqs
                .windows(2)
                .all(|pair| (pair[1] / pair[0] - step).abs() < 1e-9),
            "the frequencies are not log-spaced"
        );
    }

    /// The highest point of the curve on a 100 000-point log grid, to check [`peak_db`] by.
    fn brute_peak(bands: &[BandParams]) -> f64 {
        response_db(bands, &log_freqs(100_000))
            .into_iter()
            .fold(f64::NEG_INFINITY, f64::max)
    }

    fn band(f0: f64, gain_db: f64, q: f64) -> BandParams {
        BandParams { f0, gain_db, q }
    }

    #[test]
    fn two_overlapping_boosts_peak_above_either_where_a_dense_scan_finds_it() {
        let bands = [band(1000.0, 4.0, 1.0), band(1400.0, 4.0, 1.0)];
        let peak = peak_db(&bands);
        assert!(
            peak > 4.0,
            "two +4 dB bands this close add up past 4 dB, got {peak}"
        );
        let brute = brute_peak(&bands);
        assert!(
            (peak - brute).abs() < 0.05,
            "peak {peak} dB, a 100 000-point scan finds {brute} dB"
        );
    }

    #[test]
    fn wider_curves_peak_where_a_dense_scan_finds_them() {
        let curves = [
            vec![band(40.0, 4.0, 0.7), band(160.0, 4.0, 0.7)],
            vec![
                band(40.0, 3.0, 0.7),
                band(160.0, 1.5, 0.7),
                band(632.0, 0.0, 4.3),
                band(2515.0, 0.0, 4.3),
                band(10_000.0, 2.0, 0.7),
            ],
            vec![band(300.0, -6.0, 1.0), band(3000.0, 5.0, 8.0)],
            vec![band(18_000.0, 6.0, 0.5)],
            // At the edges: a band on 20 Hz itself, one centred past 20 kHz whose window is
            // cut to the range, and a wide pair spilling over both ends.
            vec![band(20.0, 6.0, 8.0)],
            vec![band(22_000.0, 6.0, 8.0)],
            vec![band(20.0, 5.0, 0.5), band(22_000.0, 4.0, 0.5)],
        ];
        for bands in &curves {
            let (peak, brute) = (peak_db(bands), brute_peak(bands));
            assert!(
                (peak - brute).abs() < 0.05,
                "peak {peak} dB, a 100 000-point scan finds {brute} dB"
            );
        }
    }

    #[test]
    fn a_narrow_peak_between_two_samples_is_found_beside_a_higher_sample() {
        // A wide band sits on one of the 1024 samples, and a narrow one a little higher
        // midway between two samples near 2 kHz, where the grid sees it lower than the wide
        // one. Only the window round the narrow band's centre finds its peak.
        let grid = log_freqs(PEAK_SAMPLES);
        let near = |hz: f64| grid.iter().position(|&f| f >= hz).unwrap();
        let wide = grid[near(50.0)];
        let above = near(2000.0);
        let narrow = (grid[above - 1] * grid[above]).sqrt();
        let bands = [band(wide, 5.99, 1.0), band(narrow, 6.0, 8.0)];
        let sampled = response_db(&bands, &grid);
        let (best, _) =
            sampled
                .iter()
                .enumerate()
                .fold((0, f64::NEG_INFINITY), |(i, max), (j, &db)| {
                    match db > max {
                        true => (j, db),
                        false => (i, max),
                    }
                });
        assert_eq!(
            best,
            near(50.0),
            "the grid's highest sample is the wide band's"
        );

        let peak = peak_db(&bands);
        assert!(
            peak > 6.0 - 1e-6,
            "the narrow band peaks at +6 dB, got {peak}"
        );
        let brute = brute_peak(&bands);
        assert!(
            (peak - brute).abs() < 1e-4,
            "peak {peak} dB, a 100 000-point scan finds {brute} dB"
        );
    }

    #[test]
    fn a_peak_between_two_bands_is_found_where_no_centre_is() {
        // Two pairs of wide bands, each pair half an octave either side of the point where it
        // sums highest, so both peaks lie outside every band's window; the pair at 3 kHz
        // tops the one at 100 Hz by a few hundredths of a dB. Only a dense grid picks the
        // right one, and only the refinement round its highest sample measures it.
        let pair = |centre: f64, gain_db: f64| {
            let apart = 2f64.sqrt();
            [
                band(centre / apart, gain_db, 0.7),
                band(centre * apart, gain_db, 0.7),
            ]
        };
        let bands: Vec<BandParams> = pair(100.0, 4.0)
            .into_iter()
            .chain(pair(3000.0, 4.02))
            .collect();
        let (peak, brute) = (peak_db(&bands), brute_peak(&bands));
        assert!(
            (peak - brute).abs() < 1e-6,
            "peak {peak} dB, a 100 000-point scan finds {brute} dB"
        );
    }

    #[test]
    fn a_narrow_band_peaks_at_its_own_gain() {
        let peak = peak_db(&[band(2515.0, 6.0, 8.0)]);
        assert!((peak - 6.0).abs() < 1e-6, "expected +6 dB, got {peak}");
    }

    #[test]
    fn a_flat_or_cut_only_curve_peaks_at_or_under_zero() {
        assert_eq!(peak_db(&flat()), 0.0);
        assert_eq!(peak_db(&[]), 0.0);
        let cut = [band(100.0, -6.0, 0.7), band(5000.0, -3.0, 2.0)];
        let peak = peak_db(&cut);
        assert!(peak <= 0.0, "a cut-only curve peaked at {peak} dB");
    }

    #[test]
    fn the_threshold_is_the_loudest_step_at_or_under_minus_the_peak() {
        let table = crate::db_tables::table("dac-attenuation-db").unwrap();
        assert_eq!(
            threshold_step(6.0, table),
            Some(109),
            "step 109 is -6.021 dB"
        );
        assert_eq!(threshold_step(6.021, table), Some(109), "at counts");
        assert_eq!(threshold_step(0.1, table), Some(126));
        assert_eq!(
            threshold_step(200.0, table),
            Some(0),
            "only silence is quiet enough"
        );
        assert_eq!(threshold_step(0.0, table), None);
        assert_eq!(threshold_step(-2.0, table), None);
        assert_eq!(threshold_step(f64::NAN, table), None);
    }

    /// The five-band tone control of the built-in Gungnir 2 profile.
    fn five_bands() -> EqSpec {
        Registry::builtin()
            .unwrap()
            .get("gungnir2-v2")
            .unwrap()
            .eq
            .clone()
            .unwrap()
    }

    /// A device's raw readings: every band of `eq` at its centre frequency and centre Q with
    /// the gains `gains` raw, in `mode`.
    fn readings(eq: &EqSpec, gains: &[u8], mode: EqMode) -> HashMap<FunctionId, i64> {
        let mut raw = HashMap::new();
        for (band, gain) in gains.iter().enumerate() {
            raw.insert(eq.band_id(band, EqParam::Frequency).unwrap(), 64);
            raw.insert(eq.band_id(band, EqParam::Gain).unwrap(), i64::from(*gain));
            raw.insert(eq.band_id(band, EqParam::Q).unwrap(), 64);
        }
        raw.insert(eq.mode_id(), i64::from(mode as u8));
        raw
    }

    #[test]
    fn a_boost_in_eq_mode_is_the_peak_to_warn_about() {
        let eq = five_bands();
        // Band 2 at raw 127 is +6 dB; the rest at 0 dB.
        let raw = readings(&eq, &[98, 127, 98, 98, 98], EqMode::Eq);
        let read = |id| raw.get(&id).copied();
        let bands = bands_of(&eq, read).expect("every band reads");
        assert_eq!(bands.len(), 5);
        assert_eq!(bands[1].gain_db, 6.0);
        assert_eq!(bands[1].f0, 160.0);
        let peak = clipping_peak(&eq, read).expect("a +6 dB band boosts");
        assert_eq!(peak, peak_db(&bands));
        assert!((peak - 6.0).abs() < 0.01, "peak {peak}");
    }

    #[test]
    fn no_peak_to_warn_about_outside_eq_mode_or_without_a_boost() {
        let eq = five_bands();
        let boosted = [98, 127, 98, 98, 98];
        for mode in [EqMode::Flat, EqMode::Loudness] {
            let raw = readings(&eq, &boosted, mode);
            assert_eq!(
                clipping_peak(&eq, |id| raw.get(&id).copied()),
                None,
                "{mode:?}"
            );
        }
        // A flat curve and a cut-only one in EQ mode.
        for gains in [[98; 5], [98, 60, 98, 90, 98]] {
            let raw = readings(&eq, &gains, EqMode::Eq);
            assert_eq!(
                clipping_peak(&eq, |id| raw.get(&id).copied()),
                None,
                "{gains:?}"
            );
        }
        // No mode read, a mode no tone control has, and one band short of a Q.
        let mut raw = readings(&eq, &boosted, EqMode::Eq);
        raw.remove(&eq.mode_id());
        assert_eq!(clipping_peak(&eq, |id| raw.get(&id).copied()), None);
        raw.insert(eq.mode_id(), 9);
        assert_eq!(clipping_peak(&eq, |id| raw.get(&id).copied()), None);
        let mut raw = readings(&eq, &boosted, EqMode::Eq);
        raw.remove(&eq.band_id(3, EqParam::Q).unwrap());
        assert_eq!(bands_of(&eq, |id| raw.get(&id).copied()), None);
        assert_eq!(clipping_peak(&eq, |id| raw.get(&id).copied()), None);
        raw.insert(eq.band_id(3, EqParam::Q).unwrap(), 300);
        assert_eq!(
            bands_of(&eq, |id| raw.get(&id).copied()),
            None,
            "no byte holds 300"
        );
    }

    /// Two overlapping bands of opposite sign can peak a few hundredths of a dB above 0: band 1
    /// at +0.21 dB on 20 Hz and band 2 at -0.82 dB on 82 Hz, both at Q 0.5, peak at about
    /// +0.03 dB. That would print as `+0.0`, so it is no peak to warn about; the next gain step
    /// up on band 1, +0.41 dB, is one.
    #[test]
    fn a_peak_too_small_to_read_is_none_to_warn_about() {
        let eq = five_bands();
        let mut raw = readings(&eq, &[99, 94, 98, 98, 98], EqMode::Eq);
        for band in [0, 1] {
            raw.insert(eq.band_id(band, EqParam::Frequency).unwrap(), 0);
            raw.insert(eq.band_id(band, EqParam::Q).unwrap(), 0);
        }
        let read = |raw: &HashMap<FunctionId, i64>, id| raw.get(&id).copied();
        let bands = bands_of(&eq, |id| read(&raw, id)).expect("every band reads");
        for (band, hz) in [(0, 20.0), (1, 82.0)] {
            assert!((bands[band].f0 - hz).abs() < 0.05, "{}", bands[band].f0);
            assert_eq!(bands[band].q, 0.5);
        }
        assert!(
            (bands[0].gain_db - 0.21).abs() < 0.01,
            "{}",
            bands[0].gain_db
        );
        assert!(
            (bands[1].gain_db + 0.82).abs() < 0.01,
            "{}",
            bands[1].gain_db
        );
        let peak = peak_db(&bands);
        assert!(peak > 0.0 && peak < PEAK_SHOWN_DB, "peak {peak}");
        assert_eq!(clipping_peak(&eq, |id| read(&raw, id)), None);

        raw.insert(eq.band_id(0, EqParam::Gain).unwrap(), 100);
        let shown = clipping_peak(&eq, |id| read(&raw, id)).expect("a +0.41 dB band boosts");
        assert!(shown >= PEAK_SHOWN_DB, "peak {shown}");
    }

    use crate::tone::apo::SourceFilter;

    fn correction(filters: Vec<SourceFilter>) -> Correction {
        Correction {
            preamp_db: -7.0,
            filters,
        }
    }

    fn source(kind: FilterKind, f: f64, g: f64, q: f64) -> SourceFilter {
        SourceFilter { kind, f, g, q }
    }

    /// Values from an independent evaluation of the cookbook's coefficients at 48 kHz.
    #[test]
    fn a_low_shelf_matches_the_cookbook() {
        let c = correction(vec![source(FilterKind::LowShelf, 1000.0, 6.0, 0.71)]);
        let db = correction_db(&c, &[1.0, 20.0, 250.0, 1000.0, 4000.0, 10_000.0, 23_999.0]);
        for (got, want) in db.iter().zip([6.0, 6.0, 5.978, 3.0, 0.0202, 0.0, 0.0]) {
            assert!((got - want).abs() < 0.001, "got {got} dB, want {want} dB");
        }
    }

    #[test]
    fn a_high_shelf_matches_the_cookbook() {
        let c = correction(vec![source(FilterKind::HighShelf, 4000.0, -5.0, 0.5)]);
        let db = correction_db(&c, &[1.0, 400.0, 1000.0, 4000.0, 16_000.0, 23_999.0]);
        for (got, want) in db.iter().zip([0.0, -0.0479, -0.2856, -2.5, -4.8816, -5.0]) {
            assert!((got - want).abs() < 0.001, "got {got} dB, want {want} dB");
        }
    }

    #[test]
    fn a_shelf_gives_its_full_gain_on_its_side_and_half_at_the_corner() {
        for q in [0.3, 0.7, 2.0] {
            let low = correction(vec![source(FilterKind::LowShelf, 105.0, 4.1, q)]);
            let db = correction_db(&low, &[1.0, 105.0, 24_000.0]);
            assert!((db[0] - 4.1).abs() < 0.01, "DC {} at Q {q}", db[0]);
            assert!((db[1] - 2.05).abs() < 0.01, "corner {} at Q {q}", db[1]);
            assert!(db[2].abs() < 0.01, "Nyquist {} at Q {q}", db[2]);
            let high = correction(vec![source(FilterKind::HighShelf, 8000.0, -6.0, q)]);
            let db = correction_db(&high, &[1.0, 8000.0, 24_000.0]);
            assert!(db[0].abs() < 0.01, "DC {} at Q {q}", db[0]);
            assert!((db[1] + 3.0).abs() < 0.01, "corner {} at Q {q}", db[1]);
            assert!((db[2] + 6.0).abs() < 0.01, "Nyquist {} at Q {q}", db[2]);
        }
    }

    #[test]
    fn a_correction_sums_its_filters_and_leaves_the_preamp_out() {
        let peak = source(FilterKind::Peaking, 1000.0, 3.0, 1.0);
        let shelf = source(FilterKind::LowShelf, 100.0, 2.0, 0.7);
        let freqs = log_freqs(32);
        let both = correction_db(&correction(vec![peak.clone(), shelf.clone()]), &freqs);
        let a = correction_db(&correction(vec![peak.clone()]), &freqs);
        let b = correction_db(&correction(vec![shelf]), &freqs);
        for i in 0..freqs.len() {
            assert!((both[i] - a[i] - b[i]).abs() < 1e-9);
        }
        let as_band = response_db(&[band(1000.0, 3.0, 1.0)], &freqs);
        for i in 0..freqs.len() {
            assert!(
                (a[i] - as_band[i]).abs() < 1e-12,
                "a peaking filter is a band"
            );
        }
    }

    #[test]
    fn a_q_at_or_below_zero_is_no_filter_and_never_nan() {
        let freqs = log_freqs(16);
        for q in [0.0, -1.0, f64::NAN] {
            let curve = response_db(&[band(1000.0, 6.0, q)], &freqs);
            assert!(curve.iter().all(|db| *db == 0.0), "{curve:?} at Q {q}");
            assert_eq!(peak_db(&[band(1000.0, 6.0, q)]), 0.0);
            for kind in [
                FilterKind::LowShelf,
                FilterKind::HighShelf,
                FilterKind::Peaking,
            ] {
                let c = correction(vec![source(kind, 1000.0, 6.0, q)]);
                assert!(correction_db(&c, &freqs).iter().all(|db| *db == 0.0));
            }
        }
    }

    #[test]
    fn coefficients_follow_the_rate_and_stay_below_nyquist() {
        let [b0, b1, b2, a1, a2] = coefficients(FilterKind::Peaking, 1000.0, 6.0, 1.0, 48_000);
        assert!((b0 - 1.043953).abs() < 1e-6 && (a1 - b1).abs() < 1e-12);
        assert!((b2 - 0.867722).abs() < 1e-6 && (a2 - 0.911675).abs() < 1e-6);
        let slower = coefficients(FilterKind::Peaking, 1000.0, 6.0, 1.0, 44_100);
        assert!((slower[0] - b0).abs() > 1e-4);
        // A frequency past Nyquist still gives a stable filter: both roots inside the circle.
        let [_, _, _, a1, a2] = coefficients(FilterKind::HighShelf, 30_000.0, 3.0, 1.0, 44_100);
        assert!(a2.abs() < 1.0 && a1.abs() < 1.0 + a2);
        assert_eq!(
            coefficients(FilterKind::Peaking, 1000.0, 6.0, 0.0, 48_000),
            [1.0, 0.0, 0.0, 0.0, 0.0]
        );
    }

    #[test]
    fn a_shelf_near_nyquist_is_measured_at_the_rate_it_plays_at() {
        use crate::tone::apo::SourceFilter;
        let shelf = Correction {
            preamp_db: 0.0,
            filters: vec![SourceFilter {
                kind: FilterKind::HighShelf,
                f: 21_000.0,
                g: 12.0,
                q: 0.7,
            }],
        };
        let at_48k = correction_peak_db_at(&shelf, 48_000);
        let at_44k = correction_peak_db_at(&shelf, 44_100);
        assert_eq!(at_48k, correction_peak_db(&shelf));
        assert!(
            (at_48k - at_44k).abs() > 0.05,
            "{at_48k} dB at 48 kHz against {at_44k} dB at 44.1 kHz"
        );
    }

    #[test]
    fn a_narrow_peak_just_under_nyquist_is_measured_at_its_height() {
        let at_48k = correction(vec![
            source(FilterKind::Peaking, 23_890.0, 27.0, 20.0),
            source(FilterKind::HighShelf, 21_810.0, 30.0, 0.06),
        ]);
        let peak = correction_played_peak_db(&at_48k, 48_000);
        assert!((peak - 50.63).abs() < 0.05, "{peak} dB, true 50.63");
        let at_44k = correction(vec![
            source(FilterKind::LowShelf, 23_746.0, 5.0, 0.82),
            source(FilterKind::Peaking, 22_005.9, 22.0, 20.0),
        ]);
        let peak = correction_played_peak_db(&at_44k, 44_100);
        assert!((peak - 24.5).abs() < 0.05, "{peak} dB, true 24.5");
    }
}
