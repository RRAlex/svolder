// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later

//! What the sink plays, analysed for the visualizer: per channel [`BANDS`] bands at 1/24 octave
//! from [`LOW_HZ`] up, one byte each, the channels' RMS and peak levels, a beat flag, the tempo
//! and where the beat is, in a [`Frame`] [`FRAME_RATE`] times a second.
//!
//! An [`Analyser`] takes the interleaved stereo samples as they come and gives back the frames
//! they complete. Every frame reads the latest [`WINDOW`] samples of each channel through a Hann
//! window and one transform ([`fft::fft`]), so a band's value does not depend on how the
//! samples were cut up on their way in. It holds the window, where each band reads the
//! spectrum and its working buffers, so a frame costs two transforms and a few thousand
//! multiplications.
//!
//! A band's byte is its amplitude in dB, mapped linearly from [`BYTE_FLOOR_DB`] to 0 and
//! 255 to 0 dBFS ([`to_byte`]); the range is tuned for how music looks and no front end may take
//! it as fixed. A band is the power of the bins whose centre falls inside it, summed, as a
//! sine of amplitude A reads A in the band that holds it. Below about 200 Hz a band is
//! narrower than a bin: it reads the power at its centre, interpolated between the two bins
//! around it, so neighbouring low bands read the spectrum apart rather than the same bin.
//!
//! The beat is found in the 20–150 Hz range alone, in the time domain: each channel through a 20 Hz
//! high-pass and a 150 Hz low-pass of four poles (their energies add, so bass out of phase between
//! the channels does not cancel), the RMS of what comes through for every frame, and the rise of
//! that RMS from frame to frame as the onset curve. A transform of [`WINDOW`] samples smears an
//! onset over about 170 ms, so its flux would put every beat late; this one lands within a frame. A
//! frame is a beat when the rise stands out from the last second's (`Onsets::detect`); the tempo is
//! the strongest period, from 60 to 180 BPM, of the autocorrelation of the last 8 s of the onset
//! curve (the double tempo when its period's peak stands close to it), the phase is the time since
//! the last beat over the beat period (0 before a first beat), and after a second of silence in the
//! range the tempo and the phase are cleared.
//!
//! A [`Frame`] can be turned back into the TUI's [`Block`] ([`Frame::block`]) so a TUI joined to
//! a daemon draws daemon frames with the code that draws its own captures.

use std::collections::VecDeque;
use std::f64::consts::PI;
use std::sync::OnceLock;

use crate::capture::Block;
use crate::fft::{self, fft};

/// How many bands a channel has.
pub const BANDS: usize = 240;

/// The lowest band's lower edge, in Hz. Band `i` runs from `LOW_HZ · 2^(i/24)` to the next
/// edge, so the last one ends at 20 480 Hz; at a rate whose Nyquist is lower, the bands above it
/// read 0.
pub const LOW_HZ: f32 = 20.0;

/// Frames per second.
pub const FRAME_RATE: u32 = 30;

/// The samples, per channel, each frame's spectrum is taken over.
pub const WINDOW: usize = 8192;

/// The level, in dB below full scale, that [`to_byte`] puts at 0: a band at or below it is 0.
/// A tuning constant, not a promise to any front end.
pub const BYTE_FLOOR_DB: f32 = -72.0;

/// The slowest tempo [`Frame::bpm`] gives.
pub const MIN_BPM: f32 = 60.0;

/// The fastest tempo [`Frame::bpm`] gives.
pub const MAX_BPM: f32 = 180.0;

/// The longest time, in seconds, the tempo is worked out over.
const TEMPO_SECONDS: usize = 8;

/// The shortest time, in seconds, of onsets before a tempo is given.
const TEMPO_MIN_SECONDS: usize = 3;

/// The frames the beat threshold looks back over: one second.
const THRESHOLD_FRAMES: usize = FRAME_RATE as usize;

/// The fewest frames between two beats: 8, about 0.27 s, which keeps one kick that takes two
/// frames to rise from counting twice and is faster than [`MAX_BPM`].
const REFRACTORY_FRAMES: u32 = 8;

/// The RMS of the 20–150 Hz range (amplitude, 1.0 = full scale) below which the range counts as
/// silent: about 66 dB under full scale.
const SILENT_RMS: f32 = 0.0005;

/// How many silent frames in a row, one second, end the tempo: the front ends schedule beats from
/// it, and nothing should pulse over silence.
const SILENCE_FRAMES: u32 = FRAME_RATE;

/// The rise in the low band's RMS (amplitude, 1.0 = full scale) below which nothing is a beat
/// however quiet the last second was: about 54 dB under full scale.
const MIN_RISE: f32 = 0.002;

/// The frames, from the first, whose rise is not counted: the filters start from rest, and
/// whatever the capture begins in the middle of looks like a step to them.
const SETTLING_FRAMES: u32 = 3;

/// How many standard deviations above the last second's mean a rise has to stand.
const RISE_DEVIATIONS: f32 = 1.2;

/// The autocorrelation peak, over the curve's own energy, below which there is no tempo.
const MIN_PERIODICITY: f32 = 0.15;

/// How close, as a share of the strongest peak's, the autocorrelation at half that lag has to
/// stand for the double tempo to be the one given: the integer lags of the onset curve can put
/// a fast tempo's own peak a little under its half's.
const OCTAVE_SHARE: f32 = 0.8;

/// A change of tempo smaller than this many BPM is not reported, so the figure holds still.
const BPM_HOLD: f32 = 2.0;

/// The power of a Hann window's bins that a sine spreads over, in units of the one at its peak:
/// its equivalent noise bandwidth of 1.5 bins.
const HANN_NOISE_BINS: f32 = 1.5;

/// One analysed [`FRAME_RATE`]th of a second of what the sink plays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    /// When the frame was captured, in milliseconds on the Unix clock
    /// ([`unix_ms`](crate::unix_ms)). An [`Analyser`] leaves it 0; whoever reads the capture
    /// stamps it.
    pub capture_ms: u64,
    /// The left channel's bands, each a byte on the scale [`to_byte`] describes.
    pub left: [u8; BANDS],
    /// The right channel's bands.
    pub right: [u8; BANDS],
    /// The levels over the frame's own [`FRAME_RATE`]th of a second, as bytes: left RMS, right
    /// RMS, left peak, right peak.
    pub levels: [u8; 4],
    /// Whether a beat starts in this frame.
    pub beat: bool,
    /// The tempo, once the last few seconds hold one.
    pub bpm: Option<u16>,
    /// How far along the beat period this frame is, 0 at the beat to 255 just before the next;
    /// 0 without a tempo.
    pub phase: u8,
}

impl Frame {
    /// The TUI's [`Block`] for this frame: the [`BANDS`] re-binned to the sixteen of
    /// [`fft::BANDS`], and the RMS levels, all on the drawing scale ([`fft::scale`]).
    ///
    /// A TUI band takes the loudest of the bands whose centre falls inside it, each the mean
    /// power of the two channels: that equals a mix of a mono signal and sits 3 dB under a
    /// hard-panned one. The bytes were rounded to a 0.28 dB step on the way here, which the
    /// drawing never shows.
    pub fn block(&self) -> Block {
        let mut loudest = [0.0f32; fft::BANDS];
        for (band, target) in tui_bands().iter().enumerate() {
            if let Some(target) = *target {
                let (l, r) = (
                    byte_amplitude(self.left[band]),
                    byte_amplitude(self.right[band]),
                );
                let mean = ((l * l + r * r) / 2.0).sqrt();
                loudest[target] = loudest[target].max(mean);
            }
        }
        Block {
            bands: loudest.map(fft::scale),
            left: fft::scale(byte_amplitude(self.levels[0])),
            right: fft::scale(byte_amplitude(self.levels[1])),
        }
    }
}

/// The byte an amplitude (1.0 = full scale) is sent as: its level in dB mapped linearly from
/// [`BYTE_FLOOR_DB`] → 0 to 0 dB → 255 and clamped. Zero, a negative amplitude or NaN is 0.
pub fn to_byte(amplitude: f32) -> u8 {
    if amplitude.is_nan() || amplitude <= 0.0 {
        return 0;
    }
    let db = 20.0 * amplitude.log10();
    ((db - BYTE_FLOOR_DB) / -BYTE_FLOOR_DB * 255.0)
        .round()
        .clamp(0.0, 255.0) as u8
}

/// The amplitude a byte stands for; 0 is silence, not the floor.
fn byte_amplitude(byte: u8) -> f32 {
    if byte == 0 {
        return 0.0;
    }
    let db = f32::from(byte) / 255.0 * -BYTE_FLOOR_DB + BYTE_FLOOR_DB;
    10f32.powf(db / 20.0)
}

/// The [`BANDS`] + 1 band edges in Hz, a 1/24 octave apart from [`LOW_HZ`]; band `i` runs from
/// edge `i` up to edge `i + 1`.
pub fn band_edges() -> [f32; BANDS + 1] {
    std::array::from_fn(|i| (f64::from(LOW_HZ) * (i as f64 / 24.0).exp2()) as f32)
}

/// For each of the [`BANDS`], the band of the TUI's sixteen its centre falls in, if any.
fn tui_bands() -> &'static [Option<usize>; BANDS] {
    static MAP: OnceLock<[Option<usize>; BANDS]> = OnceLock::new();
    MAP.get_or_init(|| {
        let tui = fft::band_edges();
        std::array::from_fn(|band| {
            let centre = f64::from(LOW_HZ) * ((band as f64 + 0.5) / 24.0).exp2();
            (0..fft::BANDS).find(|&t| f64::from(tui[t]) <= centre && centre < f64::from(tui[t + 1]))
        })
    })
}

/// Where one band reads the power spectrum.
#[derive(Debug, Clone, Copy)]
enum Reading {
    /// The sum of the bins from a start to an end (exclusive).
    Sum(usize, usize),
    /// Between bin `.0` and the next, `.1` of the way.
    Between(usize, f32),
    /// Above the highest frequency there is.
    Beyond,
}

/// Turns interleaved stereo samples into [`Frame`]s.
#[derive(Debug)]
pub struct Analyser {
    /// The sample rate, in Hz.
    rate: u32,
    /// The periodic Hann window, [`WINDOW`] weights.
    window: Vec<f32>,
    /// Where each band reads the spectrum.
    readings: Vec<Reading>,
    /// The latest [`WINDOW`] samples of each channel, a ring that `at` is the oldest of.
    ring: [Vec<f32>; 2],
    at: usize,
    /// The working halves of the transform.
    re: Vec<f32>,
    im: Vec<f32>,
    /// The power of each bin of the channel being read, on the amplitude scale of the band
    /// bytes.
    power: Vec<f32>,
    /// What came in since the last frame: the sum of squares, the peak and the count, per
    /// channel.
    squares: [f64; 2],
    peaks: [f32; 2],
    count: u32,
    /// 30 per sample; a frame is due each time it passes the rate, which spreads the frames
    /// evenly over rates that 30 does not divide.
    due: u32,
    onsets: Onsets,
}

impl Analyser {
    /// An analyser of samples taken at `rate_hz`.
    pub fn new(rate_hz: u32) -> Self {
        let rate = rate_hz.max(1);
        let width = f64::from(rate) / WINDOW as f64;
        let last = WINDOW / 2;
        let edges = band_edges();
        let readings = (0..BANDS)
            .map(|band| {
                let (low, high) = (f64::from(edges[band]), f64::from(edges[band + 1]));
                let start = ((low / width).ceil() as usize).min(last + 1);
                let end = ((high / width).ceil() as usize).min(last + 1);
                if start < end {
                    return Reading::Sum(start, end);
                }
                let centre = (low * high).sqrt() / width;
                if centre >= last as f64 {
                    return Reading::Beyond;
                }
                Reading::Between(centre.floor() as usize, centre.fract() as f32)
            })
            .collect();
        let window = (0..WINDOW)
            .map(|n| (0.5 - 0.5 * (2.0 * PI * n as f64 / WINDOW as f64).cos()) as f32)
            .collect();
        Self {
            rate,
            window,
            readings,
            ring: [vec![0.0; WINDOW], vec![0.0; WINDOW]],
            at: 0,
            re: vec![0.0; WINDOW],
            im: vec![0.0; WINDOW],
            power: vec![0.0; WINDOW / 2 + 1],
            squares: [0.0; 2],
            peaks: [0.0; 2],
            count: 0,
            due: 0,
            onsets: Onsets::new(rate),
        }
    }

    /// Take `interleaved` samples (left, right, left, …) and give the frames they complete,
    /// [`FRAME_RATE`] a second whatever the size of the pieces they come in. A sample that is
    /// not a number counts as silence; a trailing half frame is dropped.
    pub fn push(&mut self, interleaved: &[f32]) -> Vec<Frame> {
        let mut frames = Vec::new();
        for pair in interleaved.chunks_exact(2) {
            let sample = |s: f32| if s.is_finite() { s } else { 0.0 };
            let (l, r) = (sample(pair[0]), sample(pair[1]));
            self.ring[0][self.at] = l;
            self.ring[1][self.at] = r;
            self.at = (self.at + 1) % WINDOW;
            for (channel, s) in [l, r].into_iter().enumerate() {
                self.squares[channel] += f64::from(s) * f64::from(s);
                self.peaks[channel] = self.peaks[channel].max(s.abs());
            }
            self.count += 1;
            self.onsets.sample(l, r);
            self.due += FRAME_RATE;
            if self.due >= self.rate {
                self.due -= self.rate;
                frames.push(self.frame());
            }
        }
        frames
    }

    /// The frame of what has come in so far.
    fn frame(&mut self) -> Frame {
        let left = self.bands(0);
        let right = self.bands(1);
        let count = f64::from(self.count.max(1));
        let rms = |channel: usize| to_byte((self.squares[channel] / count).sqrt() as f32);
        let levels = [
            rms(0),
            rms(1),
            to_byte(self.peaks[0]),
            to_byte(self.peaks[1]),
        ];
        self.squares = [0.0; 2];
        self.peaks = [0.0; 2];
        self.count = 0;
        let (beat, bpm, phase) = self.onsets.frame();
        Frame {
            capture_ms: 0,
            left,
            right,
            levels,
            beat,
            bpm,
            phase,
        }
    }

    /// The band bytes of one channel's latest [`WINDOW`] samples.
    fn bands(&mut self, channel: usize) -> [u8; BANDS] {
        let ring = &self.ring[channel];
        for (n, slot) in self.re.iter_mut().enumerate() {
            *slot = ring[(self.at + n) % WINDOW] * self.window[n];
        }
        self.im.fill(0.0);
        fft(&mut self.re, &mut self.im);
        // A sine of amplitude A lands in its bin as A·N/2, times the Hann window's coherent gain
        // of 1/2; the power it spreads over the bins around it is 1.5 times its peak bin's.
        let gain = 4.0 / WINDOW as f32;
        let norm = gain * gain / HANN_NOISE_BINS;
        for (k, power) in self.power.iter_mut().enumerate() {
            *power = (self.re[k] * self.re[k] + self.im[k] * self.im[k]) * norm;
        }
        let mut bands = [0u8; BANDS];
        for (band, reading) in self.readings.iter().enumerate() {
            let power = match *reading {
                Reading::Sum(start, end) => self.power[start..end].iter().sum(),
                Reading::Between(k, t) => self.power[k] * (1.0 - t) + self.power[k + 1] * t,
                Reading::Beyond => 0.0,
            };
            bands[band] = to_byte(power.sqrt());
        }
        bands
    }
}

/// A second-order filter, in `f64` since its poles sit close to the unit circle at 20 Hz.
#[derive(Debug, Clone, Copy)]
struct Biquad {
    b: [f64; 3],
    a: [f64; 2],
    x: [f64; 2],
    y: [f64; 2],
}

impl Biquad {
    /// A Butterworth low-pass (`high` false) or high-pass at `hz`, for samples at `rate`.
    fn new(rate: u32, hz: f64, high: bool) -> Self {
        let w = 2.0 * PI * hz / f64::from(rate);
        let alpha = w.sin() / (2.0 * std::f64::consts::FRAC_1_SQRT_2);
        let c = w.cos();
        let a0 = 1.0 + alpha;
        let b = if high {
            let edge = 1.0 + c;
            [edge / 2.0, -edge, edge / 2.0]
        } else {
            let edge = 1.0 - c;
            [edge / 2.0, edge, edge / 2.0]
        };
        Self {
            b: b.map(|v| v / a0),
            a: [-2.0 * c / a0, (1.0 - alpha) / a0],
            x: [0.0; 2],
            y: [0.0; 2],
        }
    }

    fn run(&mut self, input: f64) -> f64 {
        let y = self.b[0] * input + self.b[1] * self.x[0] + self.b[2] * self.x[1]
            - self.a[0] * self.y[0]
            - self.a[1] * self.y[1];
        self.x = [input, self.x[0]];
        self.y = [y, self.y[0]];
        y
    }
}

/// The 20–150 Hz range of one channel: a 20 Hz high-pass, then a 150 Hz low-pass in two stages.
#[derive(Debug, Clone, Copy)]
struct Range {
    high_pass: Biquad,
    low_pass: [Biquad; 2],
}

impl Range {
    fn new(rate: u32) -> Self {
        Self {
            high_pass: Biquad::new(rate, 20.0, true),
            low_pass: [Biquad::new(rate, 150.0, false); 2],
        }
    }

    fn run(&mut self, input: f64) -> f64 {
        let [first, second] = &mut self.low_pass;
        second.run(first.run(self.high_pass.run(input)))
    }
}

/// Finds the beats and the tempo from the 20–150 Hz range of what comes in.
#[derive(Debug)]
struct Onsets {
    /// The filters of the left and the right channel.
    range: [Range; 2],
    /// The sum of squares of what came through both since the last frame, and how many samples.
    sum: f64,
    count: u32,
    /// The RMS of the last frame.
    previous: f32,
    /// The rise of the RMS, one per frame, the newest last: the onset curve.
    curve: VecDeque<f32>,
    /// Frames since the last beat; `None` before the first one, and again after silence.
    since_beat: Option<u32>,
    /// The scratch the tempo's mean-free copy of the curve is made in.
    centred: Vec<f32>,
    /// How many frames in a row the range has been silent.
    quiet: u32,
    /// The tempo given so far.
    bpm: Option<f32>,
    /// Frames still to go before the filters have settled and a rise means something.
    settling: u32,
}

impl Onsets {
    fn new(rate: u32) -> Self {
        Self {
            range: [Range::new(rate); 2],
            sum: 0.0,
            count: 0,
            previous: 0.0,
            curve: VecDeque::with_capacity(TEMPO_SECONDS * FRAME_RATE as usize + 1),
            since_beat: None,
            centred: Vec::with_capacity(TEMPO_SECONDS * FRAME_RATE as usize),
            quiet: 0,
            bpm: None,
            settling: SETTLING_FRAMES,
        }
    }

    /// Take one sample of each channel. Each is filtered on its own and their energies add, so
    /// bass that is out of phase between the channels does not cancel.
    fn sample(&mut self, left: f32, right: f32) {
        let [l, r] = &mut self.range;
        let (l, r) = (l.run(f64::from(left)), r.run(f64::from(right)));
        self.sum += (l * l + r * r) / 2.0;
        self.count += 1;
    }

    /// End a frame: whether it holds a beat, the tempo and the phase.
    fn frame(&mut self) -> (bool, Option<u16>, u8) {
        let rms = (self.sum / f64::from(self.count.max(1))).sqrt() as f32;
        self.sum = 0.0;
        self.count = 0;
        let rise = if self.settling > 0 {
            self.settling -= 1;
            0.0
        } else {
            (rms - self.previous).max(0.0)
        };
        self.previous = rms;
        let beat = self.detect(rise);
        self.quiet = if rms < SILENT_RMS {
            self.quiet.saturating_add(1)
        } else {
            0
        };
        if self.quiet >= SILENCE_FRAMES {
            // Nothing to follow: forget the tempo, so a new one is found from what plays next.
            self.curve.clear();
            self.bpm = None;
            self.since_beat = None;
            return (false, None, 0);
        }
        self.curve.push_back(rise);
        if self.curve.len() > TEMPO_SECONDS * FRAME_RATE as usize {
            self.curve.pop_front();
        }
        self.since_beat = if beat {
            Some(0)
        } else {
            self.since_beat.map(|frames| frames.saturating_add(1))
        };
        if let Some(found) = tempo(&self.curve, &mut self.centred) {
            if self.bpm.is_none_or(|held| (held - found).abs() >= BPM_HOLD) {
                self.bpm = Some(found);
            }
        } else {
            self.bpm = None;
        }
        // Without a beat seen yet there is nothing to count from: the phase is 0.
        let phase = match (self.bpm, self.since_beat) {
            (Some(bpm), Some(frames)) => {
                let period = FRAME_RATE as f32 * 60.0 / bpm;
                ((frames as f32 / period).fract() * 255.0) as u8
            }
            _ => 0,
        };
        (beat, self.bpm.map(|bpm| bpm.round() as u16), phase)
    }

    /// Whether `rise` is a beat: more than [`MIN_RISE`], more than the last second's mean by
    /// [`RISE_DEVIATIONS`] of its deviations, the first of its kind for [`REFRACTORY_FRAMES`].
    fn detect(&self, rise: f32) -> bool {
        if rise < MIN_RISE
            || self
                .since_beat
                .is_some_and(|frames| frames < REFRACTORY_FRAMES)
        {
            return false;
        }
        let recent = self.curve.iter().rev().take(THRESHOLD_FRAMES);
        let n = recent.len().max(1) as f32;
        let mean = recent.clone().sum::<f32>() / n;
        let variance = recent.map(|v| (v - mean) * (v - mean)).sum::<f32>() / n;
        rise > mean + RISE_DEVIATIONS * variance.sqrt()
    }
}

/// The tempo, in BPM, of the onset curve: its autocorrelation's strongest lag between
/// [`MAX_BPM`] and [`MIN_BPM`], with the double tempo preferred when its peak stands within
/// [`OCTAVE_SHARE`] of that one's, refined by the parabola through the peak and its
/// neighbours; `None` for a curve too short or too flat to have one.
fn tempo(curve: &VecDeque<f32>, centred: &mut Vec<f32>) -> Option<f32> {
    let frames = curve.len();
    if frames < TEMPO_MIN_SECONDS * FRAME_RATE as usize {
        return None;
    }
    let mean = curve.iter().sum::<f32>() / frames as f32;
    centred.clear();
    centred.extend(curve.iter().map(|v| v - mean));
    let energy: f32 = centred.iter().map(|v| v * v).sum();
    if energy <= f32::EPSILON {
        return None;
    }
    let rate = FRAME_RATE as f32 * 60.0;
    let (shortest, longest) = ((rate / MAX_BPM).ceil() as usize, (rate / MIN_BPM) as usize);
    let correlation =
        |lag: usize| -> f32 { (lag..frames).map(|i| centred[i] * centred[i - lag]).sum() };
    let (mut lag, mut best) = (shortest..=longest)
        .map(|lag| (lag, correlation(lag)))
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    if best < MIN_PERIODICITY * energy {
        return None;
    }
    // The octave check: a fast tempo's peak can fall a little under its half's, so the half lag
    // (the double tempo) is taken when its peak stands close to the strongest.
    let half = lag / 2;
    let doubled = (half.saturating_sub(1).max(shortest)..=(half + 1).min(longest))
        .filter(|&near| near * 2 > lag.saturating_sub(2) && near < lag)
        .map(|near| (near, correlation(near)))
        .max_by(|a, b| a.1.total_cmp(&b.1));
    if let Some((near, peak)) = doubled
        && peak >= OCTAVE_SHARE * best
    {
        (lag, best) = (near, peak);
    }
    let mut found = lag as f32;
    // The neighbours may lie outside the range: at its edge the peak is still bent by them.
    if lag > 1 && lag + 1 < frames {
        let (before, after) = (correlation(lag - 1), correlation(lag + 1));
        let bend = before - 2.0 * best + after;
        if bend < 0.0 {
            found += 0.5 * (before - after) / bend;
        }
    }
    Some((rate / found).clamp(MIN_BPM, MAX_BPM))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fft::{self, scale};
    use std::f32::consts::PI;

    const RATE: u32 = 48_000;

    /// Interleaved stereo: a sine at `frequency`, `left` and `right` its amplitude per channel.
    fn stereo(frequency: f32, left: f32, right: f32, seconds: f32) -> Vec<f32> {
        (0..(RATE as f32 * seconds) as usize)
            .flat_map(|n| {
                let s = (2.0 * PI * frequency * n as f32 / RATE as f32).sin();
                [left * s, right * s]
            })
            .collect()
    }

    /// A mono kick drum every `60 / bpm` seconds: a 60 Hz sine that dies away in about 100 ms,
    /// over a steady 1 kHz tone the beat detector has to ignore.
    fn kicks(bpm: f32, seconds: f32) -> Vec<f32> {
        let period = (RATE as f32 * 60.0 / bpm) as usize;
        (0..(RATE as f32 * seconds) as usize)
            .flat_map(|n| {
                let t = (n % period) as f32 / RATE as f32;
                let kick = 0.8 * (2.0 * PI * 60.0 * t).sin() * (-t / 0.05).exp();
                let hiss = 0.05 * (2.0 * PI * 1_000.0 * n as f32 / RATE as f32).sin();
                [kick + hiss, kick + hiss]
            })
            .collect()
    }

    fn band_of(frequency: f32) -> usize {
        let edges = band_edges();
        (0..BANDS)
            .find(|&band| edges[band] <= frequency && frequency < edges[band + 1])
            .expect("the frequency is inside the bands")
    }

    fn loudest(bands: &[u8; BANDS]) -> usize {
        (0..BANDS).max_by_key(|&band| bands[band]).unwrap()
    }

    /// The frames of `samples`, pushed in one go.
    fn frames(samples: &[f32]) -> Vec<Frame> {
        Analyser::new(RATE).push(samples)
    }

    #[test]
    fn a_sine_lands_in_its_band() {
        let all = frames(&stereo(1_000.0, 0.5, 0.0, 0.5));
        let frame = all.last().expect("frames");
        let band = band_of(1_000.0);
        assert_eq!(loudest(&frame.left), band, "{:?}", frame.left);
        // Half scale is 6 dB down: 66 of the 72 dB of the range.
        let expected = 255.0 * (72.0 - 6.02) / 72.0;
        assert!(
            (f32::from(frame.left[band]) - expected).abs() < 8.0,
            "{} against {expected}",
            frame.left[band]
        );
        // The far side of the spectrum is quiet, and the right channel holds nothing at all.
        assert!(frame.left[band - 40] < 20 && frame.left[band + 40] < 20);
        assert!(frame.right.iter().all(|&b| b == 0), "{:?}", frame.right);
        let swapped = frames(&stereo(1_000.0, 0.0, 0.5, 0.5));
        let frame = swapped.last().unwrap();
        assert_eq!(loudest(&frame.right), band);
        assert!(frame.left.iter().all(|&b| b == 0));
    }

    #[test]
    fn a_sine_lands_in_its_band_across_the_range() {
        for frequency in [60.0, 100.0, 440.0, 3_000.0, 10_000.0, 18_000.0] {
            let all = frames(&stereo(frequency, 0.5, 0.5, 0.5));
            let frame = all.last().unwrap();
            let found = loudest(&frame.left);
            assert!(
                found.abs_diff(band_of(frequency)) <= 1,
                "{frequency} Hz: band {found} against {}",
                band_of(frequency)
            );
        }
    }

    #[test]
    fn the_levels_are_rms_and_peak_per_channel() {
        let all = frames(&stereo(1_000.0, 0.5, 0.25, 0.5));
        let [rms_l, rms_r, peak_l, peak_r] = all.last().unwrap().levels;
        let byte = |db: f32| 255.0 * (72.0 + db) / 72.0;
        let near = |got: u8, db: f32| (f32::from(got) - byte(db)).abs() < 4.0;
        assert!(near(rms_l, -9.03), "{rms_l}");
        assert!(near(rms_r, -15.05), "{rms_r}");
        assert!(near(peak_l, -6.02), "{peak_l}");
        assert!(near(peak_r, -12.04), "{peak_r}");
    }

    #[test]
    fn silence_is_all_zero_and_no_beat() {
        let all = frames(&vec![0.0; RATE as usize * 2 * 2]);
        assert_eq!(all.len(), 60);
        for frame in &all {
            assert_eq!(frame.left, [0; BANDS]);
            assert_eq!(frame.right, [0; BANDS]);
            assert_eq!(frame.levels, [0; 4]);
            assert!(!frame.beat);
            assert_eq!(frame.bpm, None);
            assert_eq!(frame.phase, 0);
        }
    }

    #[test]
    fn a_sample_that_is_not_a_number_counts_as_silence() {
        let mut samples = vec![0.0; RATE as usize / 2];
        samples[10] = f32::NAN;
        samples[1_001] = f32::INFINITY;
        let all = frames(&samples);
        assert!(
            all.iter()
                .all(|f| f.levels == [0; 4] && f.left == [0; BANDS])
        );
    }

    #[test]
    fn a_120_bpm_kick_gives_beats_and_tempo() {
        let all = frames(&kicks(120.0, 4.0));
        let beats = all.iter().filter(|f| f.beat).count();
        // Eight kicks in four seconds: the detector sees all of them, once each.
        assert!((7..=8).contains(&beats), "{beats} beats");
        let bpm = all.last().unwrap().bpm.expect("a tempo after four seconds");
        assert!(bpm.abs_diff(120) <= 2, "{bpm} bpm");
        // The beat frames are the kicks, half a second (15 frames) apart.
        let at: Vec<usize> = (0..all.len()).filter(|&i| all[i].beat).collect();
        for pair in at.windows(2) {
            assert!((14..=16).contains(&(pair[1] - pair[0])), "{at:?}");
        }
        // The phase is 0 on a beat and grows towards the next.
        let last = *at.last().unwrap();
        assert_eq!(all[last].phase, 0);
        assert!(all[last + 7].phase > 100 && all[last + 7].phase < 140);
    }

    #[test]
    fn other_tempos_come_out_within_two_bpm() {
        for bpm in [64.0, 87.0, 90.0, 128.0, 160.0, 174.0] {
            let all = frames(&kicks(bpm, 8.0));
            let found = all
                .last()
                .unwrap()
                .bpm
                .expect("a tempo after eight seconds");
            assert!(
                f32::from(found.abs_diff(bpm as u16)) <= 2.0,
                "{bpm}: {found}"
            );
        }
    }

    /// Music, then digital silence: the tempo and the phase end within about a second of the
    /// low band falling silent, and nothing pulses over the silence.
    #[test]
    fn the_tempo_ends_within_a_second_of_the_music_stopping() {
        let mut samples = kicks(120.0, 4.0);
        let music = samples.len() / 2;
        samples.extend(vec![0.0; RATE as usize * 2 * 3]);
        let all = frames(&samples);
        assert_eq!(all.len(), 7 * 30);
        assert!(all[119].bpm.is_some(), "no tempo while the music plays");
        // The last kick, at 3.5 s, has died away by 4.0 s; a second later nothing is reported.
        let silent_from = music / 1600 + 30;
        for (i, frame) in all.iter().enumerate().skip(silent_from + 5) {
            assert_eq!(
                (frame.beat, frame.bpm, frame.phase),
                (false, None, 0),
                "frame {i}"
            );
        }
        // And a new tempo is found from what plays next.
        samples.extend(kicks(90.0, 6.0));
        let all = frames(&samples);
        let bpm = all.last().unwrap().bpm.expect("a tempo again");
        assert!(bpm.abs_diff(90) <= 2, "{bpm}");
    }

    /// A tempo with no beat flagged (every kick under the rise floor) has no beat to count from:
    /// the phase is 0.
    #[test]
    fn the_phase_is_zero_until_a_beat_has_been_seen() {
        let quiet: Vec<f32> = kicks(120.0, 6.0).iter().map(|s| s * 0.004).collect();
        let all = frames(&quiet);
        assert!(all.iter().all(|f| !f.beat), "a beat under the floor");
        assert!(
            all.last().unwrap().bpm.is_some(),
            "the quiet kicks still give a tempo"
        );
        assert!(all.iter().all(|f| f.phase == 0));
    }

    /// Wide bass, the two channels in opposite phase, still beats: it is the energy of each
    /// channel that counts, not their mix, which is silent.
    #[test]
    fn out_of_phase_kicks_are_still_beats() {
        let flipped: Vec<f32> = kicks(120.0, 4.0)
            .chunks_exact(2)
            .flat_map(|pair| [pair[0], -pair[1]])
            .collect();
        let all = frames(&flipped);
        let beats = all.iter().filter(|f| f.beat).count();
        assert!((7..=8).contains(&beats), "{beats} beats");
        let bpm = all.last().unwrap().bpm.expect("a tempo");
        assert!(bpm.abs_diff(120) <= 2, "{bpm}");
    }

    #[test]
    fn a_steady_tone_is_not_a_beat() {
        let all = frames(&stereo(1_000.0, 0.5, 0.5, 3.0));
        assert!(all.iter().all(|f| !f.beat));
        assert!(all.iter().all(|f| f.bpm.is_none()));
    }

    #[test]
    fn frames_come_at_30_per_second() {
        let samples = stereo(440.0, 0.3, 0.3, 2.0);
        assert_eq!(frames(&samples).len(), 60);
        // However the samples are cut up before they are pushed.
        let mut analyser = Analyser::new(RATE);
        let pushed: usize = samples
            .chunks(2 * 1_000)
            .map(|chunk| analyser.push(chunk).len())
            .sum();
        assert_eq!(pushed, 60);
        // At a rate that 30 does not divide, the count still comes out.
        let mut odd = Analyser::new(44_100);
        let one_second = vec![0.0; 44_100 * 2];
        assert_eq!(odd.push(&one_second).len(), 30);
    }

    #[test]
    fn mono_gives_equal_channels() {
        let all = frames(&stereo(1_000.0, 0.4, 0.4, 0.5));
        for frame in &all {
            assert_eq!(frame.left, frame.right);
            assert_eq!(frame.levels[0], frame.levels[1]);
            assert_eq!(frame.levels[2], frame.levels[3]);
        }
        assert!(all.last().unwrap().left.iter().any(|&b| b > 100));
    }

    #[test]
    fn a_frame_rebins_to_the_tuis_sixteen_bands() {
        let all = frames(&stereo(1_000.0, 0.5, 0.5, 0.5));
        let block = all.last().unwrap().block();
        let edges = fft::band_edges();
        let loudest = (0..fft::BANDS)
            .max_by(|&a, &b| block.bands[a].total_cmp(&block.bands[b]))
            .unwrap();
        assert!(edges[loudest] <= 1_000.0 && 1_000.0 < edges[loudest + 1]);
        assert!((block.bands[loudest] - scale(0.5)).abs() < 0.1, "{block:?}");
        assert!(
            (block.left - scale(0.5 / 2f32.sqrt())).abs() < 0.02,
            "{block:?}"
        );
        assert!(
            (block.right - scale(0.5 / 2f32.sqrt())).abs() < 0.02,
            "{block:?}"
        );
        // Panned hard left, it still lands in that band, and only the left level reads.
        let all = frames(&stereo(1_000.0, 0.5, 0.0, 0.5));
        let block = all.last().unwrap().block();
        let loudest = (0..fft::BANDS)
            .max_by(|&a, &b| block.bands[a].total_cmp(&block.bands[b]))
            .unwrap();
        assert!(edges[loudest] <= 1_000.0 && 1_000.0 < edges[loudest + 1]);
        assert!(block.left > 0.5 && block.right == 0.0, "{block:?}");

        let silent = frames(&vec![0.0; RATE as usize]);
        let block = silent.last().unwrap().block();
        assert_eq!(block.bands, [0.0; fft::BANDS]);
        assert_eq!((block.left, block.right), (0.0, 0.0));
    }
}
