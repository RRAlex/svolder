/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's audio features, worked out from the frames the analysis stream sends (a frame decoded
// by core/viz-model.js `decode`): bass, mids and highs as band means, the two channels' loudness,
// the stereo width, flux, the spectral centroid and flatness, and a peak flag. Pure: no DOM, no
// timers, no randomness; time comes in as `dt` seconds, so a feature is the same at any step rate.
//
//   new Features()          `observe(frame)` takes a decoded frame and returns its raw features;
//                           `step(dt, { gate, mute })` moves the smoothed `values` toward them
//   Features.KEYS           the ten features the audio panel keeps a history of
//   isSilent(frame)         whether a frame carries no sound at all (both RMS bytes zero)
//
// A band byte is 0-255 over -72..0 dBFS and 0 means "below that"; the means count it as -120 dB.

import { BANDS, bandCentreHz } from '../../core/viz-model.js';
import { byteAmplitude } from '../../core/vu-model.js';

/** The ten features, in the order the panel lists them. */
export const KEYS = Object.freeze(['bass', 'mid', 'high', 'l', 'r', 'width', 'loud', 'flux', 'cent', 'flat']);

const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);

/** `cur` moved toward `tgt` over `dt` seconds with attack `att` and release `rel` per second. */
const env = (cur, tgt, dt, att, rel) => cur + (tgt - cur) * (1 - Math.exp(-dt * (tgt > cur ? att : rel)));

/** The first and last band whose centre is within [lo, hi] Hz. */
function bandsIn(lo, hi) {
  let first = -1;
  let last = -1;
  for (let i = 0; i < BANDS; i++) {
    const c = bandCentreHz(i);
    if (c >= lo && first < 0) first = i;
    if (c <= hi) last = i;
  }
  return [first, last];
}

/** The ranges the features read. Bass, mids, highs: the dB offsets and divisors map a band mean to
 * 0-1 (the offsets are in `OFFSETS`). */
const RANGES = Object.freeze({
  bass: bandsIn(25, 150),
  mid: bandsIn(150, 2000),
  high: bandsIn(2000, 12000),
  flux: bandsIn(0, 8000),
  cent: bandsIn(30, 14000),
});

/** A band mean in dB plus `offset`, over `span`, is the feature; each fitted on the recorded frames. */
const OFFSETS = Object.freeze({
  bass: [72, 50],
  mid: [82, 50],
  high: [92, 52],
});

/** Flatness is the geometric over the arithmetic mean of the band powers, in dB, mapped from
 * `FLAT_DB[0]` below to 0 over `FLAT_DB[1]` dB (a linear map of the ratio stays under 0.01 on the
 * recorded frames: the bands are log-spaced and the analysis floor is -72 dB). */
const FLAT_DB = Object.freeze([36, 26]);

/** Time constants (seconds) of the flux's two slow means (a per-frame 0.25 and 0.006 at 60 a
 * second: -1/60 / ln(1 - a)). */
const FLUX_FAST_S = -1 / 60 / Math.log(1 - 0.25);
const FLUX_SLOW_S = -1 / 60 / Math.log(1 - 0.006);

/** The most and the least a frame's gap may count for in the flux's means, seconds. */
const MAX_GAP_S = 0.25;
const MIN_GAP_S = 1 / 240;

/** How loud the RMS bytes read: the amplitude is multiplied by this (3.2). */
const LEVEL_GAIN = 3.2;

const dbOf = (byte) => (byte === 0 ? -120 : (byte / 255) * 72 - 72);
const ampOf = (byte) => (byte === 0 ? 0 : 10 ** (dbOf(byte) / 20));
/** The amplitude flatness reads a band at: a band the analysis calls silent stands at its own -72 dB
 * floor (at zero the geometric mean of a sparse spectrum is nothing, and flatness never moves). */
const floorAmp = (byte) => 10 ** (dbOf(Math.max(byte, 1)) / 20);

/** Whether `frame` carries no sound: both RMS bytes are zero. */
export function isSilent(frame) {
  return frame.levels[0] === 0 && frame.levels[1] === 0;
}

export class Features {
  constructor() {
    this.reset();
  }

  /** Back to silence: nothing observed, the means and the smoothed values at rest. */
  reset() {
    this.raw = {
      bass: 0, mid: 0, high: 0, l: 0, r: 0, width: 0.5, loud: 0, flux: 0, cent: 0.4, flat: 0.2, peak: 0,
    };
    this.values = {
      bass: 0.3, mid: 0.3, high: 0.2, l: 0.35, r: 0.35, width: 0.6, loud: 0.35, flux: 0, cent: 0.4, flat: 0.2,
    };
    this.prev = new Float32Array(BANDS);
    this.fluxFast = 0;
    this.fluxSlow = 0;
    this.lastT = null;
  }

  /** The raw features of a decoded `frame`; they stay in `raw` until the next frame. */
  observe(frame) {
    const { left, right, levels } = frame;
    const level = new Float32Array(BANDS);
    for (let i = 0; i < BANDS; i++) level[i] = Math.max(left[i], right[i]);
    const raw = this.raw;
    for (const key of ['bass', 'mid', 'high']) {
      const [lo, hi] = RANGES[key];
      let sum = 0;
      for (let i = lo; i <= hi; i++) sum += dbOf(level[i]);
      const [offset, span] = OFFSETS[key];
      raw[key] = clamp((sum / (hi - lo + 1) + offset) / span);
    }
    raw.l = clamp(byteAmplitude(levels[0]) * LEVEL_GAIN);
    raw.r = clamp(byteAmplitude(levels[1]) * LEVEL_GAIN);
    raw.loud = (raw.l + raw.r) / 2;
    raw.width = this.widthOf(left, right);
    this.spectral(level, frame.t);
    raw.peak = levels[2] === 255 || levels[3] === 255 ? 1 : 0;
    return raw;
  }

  /** The stereo width from the bands: the side's share of the mid's, 0 for mono, 1 for one side. */
  widthOf(left, right) {
    let side = 0;
    let mid = 0;
    for (let i = 0; i < BANDS; i++) {
      const a = ampOf(left[i]);
      const b = ampOf(right[i]);
      side += (a - b) * (a - b);
      mid += (a + b) * (a + b);
    }
    return clamp(Math.sqrt(side / (mid + 1e-9)) * 1.6);
  }

  /** Flux, centroid and flatness of the band magnitudes. */
  spectral(level, t) {
    const raw = this.raw;
    const [fluxLo, fluxHi] = RANGES.flux;
    let rise = 0;
    let total = 0;
    for (let i = Math.max(fluxLo, 0); i <= fluxHi; i++) {
      const m = ampOf(level[i]);
      const d = m - this.prev[i];
      if (d > 0) rise += d;
      total += m;
      this.prev[i] = m;
    }
    const gap = this.lastT === null ? 1 / 30 : clamp((t - this.lastT) / 1000, MIN_GAP_S, MAX_GAP_S);
    this.lastT = t;
    const ratio = rise / (total + 1e-6);
    this.fluxFast += (ratio - this.fluxFast) * (1 - Math.exp(-gap / FLUX_FAST_S));
    this.fluxSlow += (ratio - this.fluxSlow) * (1 - Math.exp(-gap / FLUX_SLOW_S));
    raw.flux = clamp((this.fluxFast / (this.fluxSlow + 1e-4) - 1.25) * 0.9);

    const [lo, hi] = RANGES.cent;
    const span = Math.log(14000 / 30);
    let weighted = 0;
    let sum = 0;
    let logs = 0;
    let power = 0;
    for (let i = lo; i <= hi; i++) {
      const m = ampOf(level[i]);
      const pw = floorAmp(level[i]) ** 2;
      weighted += (m * Math.log(bandCentreHz(i) / 30)) / span;
      sum += m;
      logs += Math.log(pw);
      power += pw;
    }
    const n = hi - lo + 1;
    raw.cent = clamp(sum > 0 ? weighted / sum : 0.4);
    raw.flat = clamp((10 * Math.log10(Math.exp(logs / n) / (power / n) + 1e-12) + FLAT_DB[0]) / FLAT_DB[1]);
  }

  /**
   * Move the smoothed `values` toward the raw features over `dt` seconds (envelopes with an
   * attack and a release per second). `gate` 0 (paused, no sound) pulls them to rest,
   * and `mute` freezes them, as the scene's frost does.
   */
  step(dt, { gate = 1, mute = false } = {}) {
    if (mute) return this.values;
    const v = this.values;
    const r = this.raw;
    v.bass = env(v.bass, r.bass * gate, dt, 30, 5);
    v.mid = env(v.mid, r.mid * gate, dt, 18, 4);
    v.high = env(v.high, r.high * gate, dt, 40, 8);
    v.l = env(v.l, r.l * gate, dt, 20, 5);
    v.r = env(v.r, r.r * gate, dt, 20, 5);
    v.width = env(v.width, gate ? r.width : 0.5, dt, 2, 2);
    v.loud = env(v.loud, r.loud * gate, dt, 0.5, 0.35);
    v.flux = env(v.flux, r.flux * gate, dt, 20, 1.5);
    if (gate > 0.5) {
      v.cent = env(v.cent, r.cent, dt, 4, 2);
      v.flat = env(v.flat, r.flat, dt, 4, 2);
    }
    return v;
  }
}

Features.KEYS = KEYS;
