/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The mountains' ridge as heights: the analysis's levels smoothed across log-frequency, read at every
// few pixels along the axis, with a slow swell under them that is all the ridge does in silence. The
// drawing (draw/road.js) turns the heights into a smooth curve; nothing here touches a canvas.

import { BANDS, bandCentreHz } from '../../core/viz-model.js';
import { axisHz } from './axis.js';

/** The ridge's numbers. `step`: the px between two samples. `scale`: the height of a level of 1, as a
 * share of the scene's height. `sigma`, `radius`: the smoothing across frequency, in bands (a band is
 * 1/24 octave, so a sigma of 0.87 is about 1/14 octave across the half height: wide enough to take the
 * alternation of neighbouring bands out, narrow enough to leave a notch of two bands standing). `swell`: the silent
 * ridge's wave: `amp` and `wave` (its wavelength) as shares of the scene's height and width, `period`
 * the seconds it takes to drift one wavelength, `gives` the level at which the music has taken the
 * ridge over entirely. `drift`: how fast the swell drifts at each motion level. */
export const RIDGE = Object.freeze({
  step: 3,
  scale: 0.2,
  sigma: 0.87,
  radius: 2,
  swell: Object.freeze({ amp: 0.012, wave: 0.5, period: 10, gives: 0.15 }),
  drift: Object.freeze({ full: 1, calm: 0.5, still: 0 }),
});

const KERNEL = Float32Array.from({ length: 2 * RIDGE.radius + 1 }, (_, i) => Math.exp(-((i - RIDGE.radius) ** 2) / (2 * RIDGE.sigma ** 2)));

/** The levels `units` (one per band, 0 to 1) smoothed across frequency into `out`. */
export function smoothUnits(units, out) {
  const { radius } = RIDGE;
  for (let i = 0; i < BANDS; i++) {
    let sum = 0;
    let weight = 0;
    for (let j = Math.max(0, i - radius); j <= Math.min(BANDS - 1, i + radius); j++) {
      const k = KERNEL[j - i + radius];
      sum += units[j] * k;
      weight += k;
    }
    out[i] = sum / weight;
  }
  return out;
}

/** The swell's phase (in wavelengths) `dt` seconds on, at the motion level `motion`. */
export const driftSwell = (phase, dt, motion) => phase + (dt / RIDGE.swell.period) * (RIDGE.drift[motion] ?? 1);

const FIRST = bandCentreHz(0);

/** The smoothed level at place `x` of an axis `w` wide, between the bands' centres. */
function levelAt(soft, w, x) {
  const at = Math.min(BANDS - 1, Math.max(0, Math.log2(axisHz(x, w) / FIRST) * 24));
  const i = Math.min(BANDS - 2, Math.floor(at));
  return soft[i] + (soft[i + 1] - soft[i]) * (at - i);
}

/** The ridge's height in px above the horizon at each sample into `out`: the music's, and the
 * swell's while the music is low. `soft` is `smoothUnits`'s, `phase` the swell's. Returns how many
 * samples there are; sample `i` is at `x = i * RIDGE.step`, the last at `w`. */
export function ridgeHeights({ soft, w, h, phase = 0 }, out) {
  const { step, scale, swell } = RIDGE;
  const n = Math.ceil(w / step) + 1;
  for (let i = 0; i < n; i++) {
    const x = Math.min(w, i * step);
    const level = levelAt(soft, w, x);
    const wave = 0.5 + 0.5 * Math.sin(2 * Math.PI * (x / (w * swell.wave) - phase));
    const gives = Math.max(0, 1 - level / swell.gives);
    out[i] = level * scale * h + wave * swell.amp * h * gives;
  }
  return n;
}
