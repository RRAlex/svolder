/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The tone control's curve maths in the page, a port of the core's (`tone/response.rs`) and of
// the graph's maps (`web/view/tone.rs`): what a set of bands does to the frequency response, the
// peak that can clip, its words, and the viewBox's x and y for a frequency and a gain. Pure
// numbers in, numbers or strings out: no DOM, no import, so node loads it as the browser does.
// `scripts/web-checks/node/tone-curve.mjs` holds it to the core's own numbers (a fixture a Rust
// test writes), so the page and the TUI draw one curve.

export const POINTS = 181;
export const RANGE_DB = 13;
export const F_MIN = 20;
export const F_MAX = 20000;
export const PEAK_SHOWN_DB = 0.05;

const SAMPLE_RATE_HZ = 48000;
const PEAK_SAMPLES = 1024;
const PEAK_WINDOW_OCTAVES = 0.5;
const PEAK_STEPS = 60;
const VIEW_WIDTH = 1000;

// `n` log-spaced frequencies from 20 Hz to 20 kHz.
export function logFreqs(n) {
  if (n < 2) return new Array(Math.max(n, 0)).fill(F_MIN);
  const decades = Math.log10(F_MAX / F_MIN);
  return Array.from({ length: n }, (_, i) => F_MIN * 10 ** (decades * i / (n - 1)));
}

// One band's biquad, as the device's DSP runs it: a peak, or with `type` `LSC` or `HSC` the
// cookbook's low or high shelf (its Q the cookbook's). A Q that is not above 0 changes nothing.
function biquad({ f0, gainDb, q, type }) {
  if (!(q > 0)) return { b: [1, 0, 0], a: [1, 0, 0] };
  const amplitude = 10 ** (gainDb / 40);
  const w0 = 2 * Math.PI * f0 / SAMPLE_RATE_HZ;
  const cos = Math.cos(w0);
  if (type === 'LSC' || type === 'HSC') {
    const beta = 2 * Math.sqrt(amplitude) * (Math.sin(w0) / (2 * q));
    const plus = amplitude + 1;
    const minus = amplitude - 1;
    if (type === 'LSC') {
      return {
        b: [amplitude * (plus - minus * cos + beta), 2 * amplitude * (minus - plus * cos),
          amplitude * (plus - minus * cos - beta)],
        a: [plus + minus * cos + beta, -2 * (minus + plus * cos), plus + minus * cos - beta],
      };
    }
    return {
      b: [amplitude * (plus + minus * cos + beta), -2 * amplitude * (minus + plus * cos),
        amplitude * (plus + minus * cos - beta)],
      a: [plus - minus * cos + beta, 2 * (minus - plus * cos), plus - minus * cos - beta],
    };
  }
  const alpha = Math.sin(w0) / (2 * q);
  return {
    b: [1 + alpha * amplitude, -2 * cos, 1 - alpha * amplitude],
    a: [1 + alpha / amplitude, -2 * cos, 1 - alpha / amplitude],
  };
}

// The squared magnitude of c0 + c1 e^-jw + c2 e^-2jw.
function polar(c, w) {
  const real = c[0] + c[1] * Math.cos(w) + c[2] * Math.cos(2 * w);
  const imaginary = -(c[1] * Math.sin(w) + c[2] * Math.sin(2 * w));
  return real * real + imaginary * imaginary;
}

// The sum of every biquad's lift or cut at `hz`, in dB.
function summedDb(biquads, hz) {
  const w = 2 * Math.PI * hz / SAMPLE_RATE_HZ;
  let sum = 0;
  for (const { b, a } of biquads) {
    const denominator = polar(a, w);
    if (!(denominator <= 0)) sum += 10 * Math.log10(polar(b, w) / denominator);
  }
  return sum;
}

// Summed magnitude response in dB of the bands `[{ f0, gainDb, q, type? }]` at `freqs`: a band's
// `type` is `PK` (the default), `LSC` or `HSC`.
export function responseDb(bands, freqs) {
  const biquads = bands.map(biquad);
  return freqs.map((hz) => summedDb(biquads, hz));
}

// The larger of two numbers, a NaN giving way to the other.
function higher(a, b) {
  return b > a || Number.isNaN(a) ? b : a;
}

// The highest `f` takes between `from` and `to`, by golden-section search.
function goldenMax(f, from, to) {
  const ratio = (Math.sqrt(5) - 1) / 2;
  let left = to - ratio * (to - from);
  let right = from + ratio * (to - from);
  let atLeft = f(left);
  let atRight = f(right);
  for (let step = 0; step < PEAK_STEPS; step++) {
    if (atLeft < atRight) {
      from = left;
      left = right;
      atLeft = atRight;
      right = from + ratio * (to - from);
      atRight = f(right);
    } else {
      to = right;
      right = left;
      atRight = atLeft;
      left = to - ratio * (to - from);
      atLeft = f(left);
    }
  }
  return higher(atLeft, atRight);
}

// The highest the summed curve rises between 20 Hz and 20 kHz, in dB: 1024 log-spaced samples,
// refined by golden-section search in the two sample intervals either side of the highest sample
// and in a half-octave window round each band's centre.
export function peakDb(bands) {
  const biquads = bands.map(biquad);
  const at = (logHz) => summedDb(biquads, 10 ** logHz);
  const grid = logFreqs(PEAK_SAMPLES).map(Math.log10);
  let best = 0;
  let peak = -Infinity;
  grid.forEach((logHz, j) => {
    const db = at(logHz);
    if (db > peak) {
      best = j;
      peak = db;
    }
  });
  const low = Math.log10(F_MIN);
  const high = Math.log10(F_MAX);
  const halfWindow = PEAK_WINDOW_OCTAVES / 2 * Math.log10(2);
  const windows = [[grid[Math.max(best - 1, 0)], grid[Math.min(best + 1, grid.length - 1)]]];
  for (const { f0 } of bands) {
    if (!(f0 > 0 && Number.isFinite(f0))) continue;
    const centre = Math.log10(f0);
    const from = Math.max(centre - halfWindow, low);
    const to = Math.min(centre + halfWindow, high);
    if (from < to) windows.push([from, to]);
  }
  for (const [from, to] of windows) peak = higher(peak, goldenMax(at, from, to));
  return peak;
}

// How far the curve boosts, in dB, when that boost can clip: the peak in EQ mode when it is at
// least PEAK_SHOWN_DB, otherwise null (Flat applies no band, Loudness is the device's own
// contour, and a smaller peak would print as +0.0).
export function clippingPeak(mode, bands) {
  if (mode !== 'eq' || !bands) return null;
  const peak = peakDb(bands);
  return peak >= PEAK_SHOWN_DB ? peak : null;
}

// The peak's words: `peak +1.5 dB`, a minus written U+2212.
export function peakWords(db) {
  const negative = db < 0 || Object.is(db, -0);
  return `peak ${negative ? '−' : '+'}${Math.abs(db).toFixed(1)} dB`;
}

// The viewBox x of a frequency.
export function xOf(hz) {
  return VIEW_WIDTH * Math.log(hz / F_MIN) / Math.log(F_MAX / F_MIN);
}

// The frequency at a viewBox x, the inverse of xOf, not clamped.
export function hzAt(x) {
  return F_MIN * Math.exp(x / VIEW_WIDTH * Math.log(F_MAX / F_MIN));
}

// The viewBox y of a dB value, clamped to the plot's range.
export function yOf(db) {
  return 10 * (RANGE_DB - Math.min(Math.max(db, -RANGE_DB), RANGE_DB));
}

// The dB at a viewBox y, the inverse of yOf's unclamped map, not clamped.
export function dbAt(y) {
  return RANGE_DB - y / 10;
}

// A band's marker in the viewBox: its frequency's x and its own gain's y in EQ and in Flat, 0 dB
// in Loudness and while the mode is unread.
export function markerPoint(mode, band) {
  const own = mode === 'eq' || mode === 'flat';
  return { x: xOf(band.f0), y: yOf(own ? band.gainDb : 0) };
}

const FREQS = logFreqs(POINTS);

// `M x y L x y …` through the viewBox y values `ys` at the curve's 181 frequencies.
export function linePath(ys) {
  return ys.slice(0, POINTS)
    .map((y, i) => `${i === 0 ? 'M' : 'L'}${xOf(FREQS[i]).toFixed(1)} ${y.toFixed(1)}`)
    .join('');
}
