/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's waveform, made from the bands: there is no time-domain signal in the analysis
// stream, so each band's level is one partial of a sine of a fixed, seeded phase, and the 256
// samples are their sum. Pure: the same (bands, seed) gives the same samples.
//
//   waveOf(bands, seed, out?)   `bands` an array of levels 0-1 (the 64 log bins, low to high);
//                               returns `Float32Array(SAMPLES)` of -1 to 1 (written into `out`)

import { hashStr, mulberry } from './rng.js';

/** How many samples a waveform has. */
export const SAMPLES = 256;

/** The highest harmonic of the window a partial lands on (the lowest is 1). */
const TOP_HARMONIC = 64;

/** How loud one full-scale band is, before the sum is clamped. */
const GAIN = 1.6;

const TAU = Math.PI * 2;

export function waveOf(bands, seed, out = new Float32Array(SAMPLES)) {
  const n = bands.length;
  const rng = mulberry(hashStr(`wave:${seed}`));
  out.fill(0);
  if (n === 0) return out;
  const scale = GAIN / Math.sqrt(n);
  for (let j = 0; j < n; j++) {
    // the phase is drawn for every band, so the others keep theirs when one level is zero
    const phase = rng() * TAU;
    const level = bands[j];
    if (!(level > 0)) continue;
    const harmonic = Math.round(TOP_HARMONIC ** (n > 1 ? j / (n - 1) : 0));
    const a = Math.min(1, level) * scale;
    for (let i = 0; i < SAMPLES; i++) out[i] += a * Math.sin((TAU * harmonic * i) / SAMPLES + phase);
  }
  for (let i = 0; i < SAMPLES; i++) out[i] = Math.max(-1, Math.min(1, out[i]));
  return out;
}
