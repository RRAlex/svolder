// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What a block of samples is drawn as: a radix-2 FFT, the [`BANDS`] log-spaced bands from
//! [`LOW_HZ`] to [`HIGH_HZ`], and the RMS level, both on one dB scale that puts [`FLOOR_DB`]
//! at 0.0 and full scale at 1.0 ([`scale`]).

use std::f64::consts::PI;

/// How many bands the spectrum has.
pub const BANDS: usize = 16;

/// The lowest band's lower edge, in Hz.
pub const LOW_HZ: f32 = 40.0;

/// The highest band's upper edge, in Hz.
pub const HIGH_HZ: f32 = 16_000.0;

/// The level, in dB below full scale, that [`scale`] puts at 0.0: anything at or below it
/// reads as silence.
pub const FLOOR_DB: f32 = -60.0;

/// Transform `re` + i·`im` in place into its discrete Fourier transform, forward and
/// unnormalised: `X[k] = Σ x[n]·e^(−2πikn/N)`.
///
/// # Panics
///
/// When the two halves differ in length, or the length is not a power of two.
pub fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    assert_eq!(
        n,
        im.len(),
        "the real and imaginary halves differ in length"
    );
    assert!(
        n.is_power_of_two(),
        "a radix-2 FFT takes a power of two, not {n}"
    );
    if n < 2 {
        return;
    }
    // The butterflies below read their inputs in bit-reversed order.
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = i.reverse_bits() >> (usize::BITS - bits);
        if j > i {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let half = len / 2;
        let step = -2.0 * PI / len as f64;
        for k in 0..half {
            let (sin, cos) = (step * k as f64).sin_cos();
            let (wr, wi) = (cos as f32, sin as f32);
            let mut start = 0;
            while start < n {
                let a = start + k;
                let b = a + half;
                let tr = re[b] * wr - im[b] * wi;
                let ti = re[b] * wi + im[b] * wr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                start += len;
            }
        }
        len *= 2;
    }
}

/// Where an amplitude (1.0 = full scale) sits on the drawing scale: its level in dB mapped
/// linearly from [`FLOOR_DB`] → 0.0 to 0 dB → 1.0, and clamped to `0.0..=1.0`. Zero, a
/// negative amplitude or NaN reads 0.0.
pub fn scale(amplitude: f32) -> f32 {
    if amplitude.is_nan() || amplitude <= 0.0 {
        return 0.0;
    }
    let db = 20.0 * amplitude.log10();
    ((db - FLOOR_DB) / -FLOOR_DB).clamp(0.0, 1.0)
}

/// The RMS of `samples` on the drawing scale ([`scale`]): a full-scale square wave reads 1.0,
/// a full-scale sine −3 dB, silence and an empty block 0.0.
pub fn level(samples: impl IntoIterator<Item = f32>) -> f32 {
    let (sum, count) = samples
        .into_iter()
        .fold((0.0f64, 0usize), |(sum, count), s| {
            (sum + f64::from(s) * f64::from(s), count + 1)
        });
    if count == 0 {
        return 0.0;
    }
    scale((sum / count as f64).sqrt() as f32)
}

/// The [`BANDS`] + 1 band edges in Hz, from [`LOW_HZ`] to [`HIGH_HZ`], each a constant ratio
/// above the one before; band `i` runs from edge `i` up to edge `i + 1`.
pub fn band_edges() -> [f32; BANDS + 1] {
    let ratio = f64::from(HIGH_HZ / LOW_HZ);
    std::array::from_fn(|i| (f64::from(LOW_HZ) * ratio.powf(i as f64 / BANDS as f64)) as f32)
}

/// The spectrum of one block: a Hann window, the FFT, and for each band the loudest of its
/// bins, or, for a band narrower than a bin (the second at 48 kHz and 1024 frames), the
/// spectrum at its centre, on the drawing scale. A full-scale sine reads about 1.0 in the band
/// that holds it.
///
/// Built once for a sample rate and a block length, it keeps its window, where each band
/// reads the spectrum and its working buffers, so a block costs the transform and little else.
#[derive(Debug, Clone)]
pub struct Spectrum {
    /// The periodic Hann window, one weight per sample of a block.
    window: Vec<f32>,
    /// Where each band reads the spectrum.
    readings: [Reading; BANDS],
    /// The working halves the transform runs in.
    re: Vec<f32>,
    im: Vec<f32>,
}

/// Where one band reads the spectrum.
#[derive(Debug, Clone)]
enum Reading {
    /// The loudest of the bins from a start to an end (exclusive).
    Bins(usize, usize),
    /// The spectrum at the band's centre, which falls between two bins: the cosine and the
    /// sine of each sample's phase at that frequency, which the windowed block is summed
    /// against.
    Centre { cos: Vec<f32>, sin: Vec<f32> },
}

impl Spectrum {
    /// A spectrum of `frames`-sample blocks taken at `rate` Hz.
    ///
    /// A band reads the loudest of the bins whose centre frequency falls inside it. A band
    /// narrower than a bin, as the lowest ones are at a short block, holds no bin's centre: it
    /// reads the spectrum at its own centre, worked out there directly, which is the exact
    /// value between the two bins around it, so two neighbouring narrow bands never read the
    /// same bin. A tone away from a band's edges reads loudest in the band that holds it; one
    /// within a few hertz of an edge can read loudest in the band beside it, since a bin is
    /// wider than the lowest bands.
    ///
    /// # Panics
    ///
    /// When `frames` is not a power of two.
    pub fn new(rate: u32, frames: usize) -> Self {
        assert!(
            frames.is_power_of_two(),
            "a spectrum block is a power of two, not {frames}"
        );
        let width = f64::from(rate) / frames as f64;
        let last = frames / 2;
        let edges = band_edges();
        let readings = std::array::from_fn(|band| {
            let (low, high) = (f64::from(edges[band]), f64::from(edges[band + 1]));
            let start = ((low / width).ceil() as usize).min(last);
            let end = ((high / width).ceil() as usize).min(last + 1);
            if start < end {
                return Reading::Bins(start, end);
            }
            // Where the band's centre falls, in bins.
            let centre = (low * high).sqrt() / width;
            let (cos, sin) = (0..frames)
                .map(|n| {
                    let (sin, cos) = (2.0 * PI * centre * n as f64 / frames as f64).sin_cos();
                    (cos as f32, sin as f32)
                })
                .unzip();
            Reading::Centre { cos, sin }
        });
        let window = (0..frames)
            .map(|n| (0.5 - 0.5 * (2.0 * PI * n as f64 / frames as f64).cos()) as f32)
            .collect();
        Self {
            window,
            readings,
            re: vec![0.0; frames],
            im: vec![0.0; frames],
        }
    }

    /// The bands of one block of mono samples. A shorter block is padded with silence and a
    /// longer one cut to the length the spectrum was built for.
    pub fn bands(&mut self, samples: &[f32]) -> [f32; BANDS] {
        let frames = self.window.len();
        for (n, slot) in self.re.iter_mut().enumerate() {
            *slot = samples.get(n).copied().unwrap_or(0.0) * self.window[n];
        }
        // A sine of amplitude A lands in its bin as A·N/2, times the Hann window's coherent
        // gain of 1/2: this brings it back to A.
        let gain = 4.0 / frames as f32;
        let mut loudest = [0.0f32; BANDS];
        // The bands read at their centre are summed from the windowed block before the
        // transform overwrites it.
        for (band, reading) in self.readings.iter().enumerate() {
            if let Reading::Centre { cos, sin } = reading {
                let (re, im) = self
                    .re
                    .iter()
                    .zip(cos.iter().zip(sin))
                    .fold((0.0f32, 0.0f32), |(re, im), (x, (c, s))| {
                        (re + x * c, im - x * s)
                    });
                loudest[band] = re.hypot(im) * gain;
            }
        }
        self.im.fill(0.0);
        fft(&mut self.re, &mut self.im);
        for (band, reading) in self.readings.iter().enumerate() {
            if let Reading::Bins(start, end) = *reading {
                loudest[band] = (start..end)
                    .map(|k| self.re[k].hypot(self.im[k]) * gain)
                    .fold(0.0f32, f32::max);
            }
        }
        loudest.map(scale)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A deterministic sequence in `-1.0..1.0`, so a case can compare against a direct DFT.
    fn noise(len: usize) -> Vec<f32> {
        let mut state = 0x2545_f491_u32;
        (0..len)
            .map(|_| {
                state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (state >> 8) as f32 / (1u32 << 23) as f32 - 1.0
            })
            .collect()
    }

    fn sine(frequency: f32, amplitude: f32, rate: u32, frames: usize) -> Vec<f32> {
        (0..frames)
            .map(|n| {
                amplitude * (2.0 * std::f32::consts::PI * frequency * n as f32 / rate as f32).sin()
            })
            .collect()
    }

    fn band_of(frequency: f32) -> usize {
        let edges = band_edges();
        (0..BANDS)
            .find(|&band| edges[band] <= frequency && frequency < edges[band + 1])
            .expect("the frequency is inside the spectrum")
    }

    fn loudest(bands: &[f32; BANDS]) -> usize {
        (0..BANDS)
            .max_by(|&a, &b| bands[a].total_cmp(&bands[b]))
            .unwrap()
    }

    #[test]
    fn an_impulse_transforms_to_a_flat_spectrum() {
        let mut re = vec![0.0; 8];
        let mut im = vec![0.0; 8];
        re[0] = 1.0;
        fft(&mut re, &mut im);
        assert!(re.iter().all(|&r| (r - 1.0).abs() < 1e-6), "{re:?}");
        assert!(im.iter().all(|&i| i.abs() < 1e-6), "{im:?}");
    }

    #[test]
    fn the_transform_matches_a_direct_dft() {
        let n = 64;
        let input = noise(n);
        let mut re = input.clone();
        let mut im = vec![0.0; n];
        fft(&mut re, &mut im);
        for k in 0..n {
            let (mut dr, mut di) = (0.0f64, 0.0f64);
            for (t, &x) in input.iter().enumerate() {
                let angle = -2.0 * PI * (k * t) as f64 / n as f64;
                dr += f64::from(x) * angle.cos();
                di += f64::from(x) * angle.sin();
            }
            assert!(
                (f64::from(re[k]) - dr).abs() < 1e-3,
                "bin {k}: {} vs {dr}",
                re[k]
            );
            assert!(
                (f64::from(im[k]) - di).abs() < 1e-3,
                "bin {k}: {} vs {di}",
                im[k]
            );
        }
    }

    #[test]
    #[should_panic(expected = "power of two")]
    fn a_length_that_is_not_a_power_of_two_is_refused() {
        fft(&mut [0.0; 12], &mut [0.0; 12]);
    }

    #[test]
    fn sixteen_bands_span_40_hz_to_16_khz_at_one_ratio() {
        let edges = band_edges();
        assert_eq!(edges.len(), 17);
        assert!((edges[0] - 40.0).abs() < 1e-3, "{}", edges[0]);
        assert!((edges[16] - 16_000.0).abs() < 0.5, "{}", edges[16]);
        let ratio = edges[1] / edges[0];
        for pair in edges.windows(2) {
            assert!((pair[1] / pair[0] - ratio).abs() < 1e-4, "{pair:?}");
        }
    }

    #[test]
    fn a_sine_peaks_in_the_band_that_holds_its_frequency() {
        let mut spectrum = Spectrum::new(48_000, 1024);
        for frequency in [
            50.0, 75.0, 100.0, 440.0, 1_000.0, 3_000.0, 10_000.0, 15_000.0,
        ] {
            let bands = spectrum.bands(&sine(frequency, 0.5, 48_000, 1024));
            let peak = band_of(frequency);
            assert_eq!(loudest(&bands), peak, "{frequency} Hz: {bands:?}");
            for (band, &value) in bands.iter().enumerate() {
                if band.abs_diff(peak) >= 3 {
                    assert!(
                        value < bands[peak] - 0.3,
                        "{frequency} Hz leaks {value} into band {band}: {bands:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn a_full_scale_sine_reads_full_and_half_scale_six_db_down() {
        let mut spectrum = Spectrum::new(48_000, 1024);
        // On a bin's centre, so the window loses nothing to the bin's edge.
        let frequency = 48_000.0 / 1024.0 * 21.0;
        let full = spectrum.bands(&sine(frequency, 1.0, 48_000, 1024));
        assert!((full[band_of(frequency)] - 1.0).abs() < 0.01, "{full:?}");
        let half = spectrum.bands(&sine(frequency, 0.5, 48_000, 1024));
        let expected = 1.0 - 6.0206 / 60.0;
        assert!(
            (half[band_of(frequency)] - expected).abs() < 0.01,
            "{half:?}"
        );
    }

    #[test]
    fn silence_draws_every_band_empty() {
        let mut spectrum = Spectrum::new(48_000, 1024);
        assert_eq!(spectrum.bands(&[0.0; 1024]), [0.0; BANDS]);
        assert_eq!(spectrum.bands(&[]), [0.0; BANDS]);
    }

    #[test]
    fn the_two_lowest_bands_read_the_spectrum_apart() {
        // At 48 kHz and 1024 frames a bin is 46.9 Hz wide, wider than the second band, whose
        // centre falls between the first bin and the second.
        let mut spectrum = Spectrum::new(48_000, 1024);
        let low = spectrum.bands(&sine(50.0, 0.5, 48_000, 1024));
        assert!(low[0] > low[1], "50 Hz: {low:?}");
        let high = spectrum.bands(&sine(75.0, 0.5, 48_000, 1024));
        assert!(high[1] > high[0] && high[1] > high[2], "75 Hz: {high:?}");
    }

    #[test]
    fn the_scale_runs_from_minus_60_db_to_full_scale() {
        assert_eq!(scale(1.0), 1.0);
        assert_eq!(scale(2.0), 1.0);
        assert!((scale(0.1) - 2.0 / 3.0).abs() < 1e-5, "{}", scale(0.1));
        assert!(scale(0.001).abs() < 1e-5, "{}", scale(0.001));
        assert_eq!(scale(0.0001), 0.0);
        assert_eq!(scale(0.0), 0.0);
        assert_eq!(scale(-1.0), 0.0);
        assert_eq!(scale(f32::NAN), 0.0);
    }

    #[test]
    fn the_level_is_the_rms_on_the_scale() {
        assert_eq!(level([1.0, -1.0, 1.0, -1.0]), 1.0);
        // A sine's RMS is 1/√2 of its peak: 3 dB down.
        let sine = sine(1_000.0, 1.0, 48_000, 4800);
        let expected = 1.0 - 3.0103 / 60.0;
        assert!((level(sine) - expected).abs() < 1e-3);
        assert!((level([0.1, -0.1]) - 2.0 / 3.0).abs() < 1e-5);
        assert_eq!(level([0.0; 32]), 0.0);
        assert_eq!(level(std::iter::empty()), 0.0);
    }
}
