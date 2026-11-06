/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The heat colours of the floor's cells: a ramp from a dark violet through pink and orange to a pale
// yellow, leaning toward a warm ramp for bass-heavy music and a cool one for bright music. A ramp is
// cut into `STEPS` colours once, as strings the canvas takes, so a frame builds none.

/** The ramps, each six `[r, g, b]` from the quietest to the hottest. */
export const RAMP = Object.freeze([[18, 6, 40], [90, 20, 140], [210, 40, 180], [255, 90, 120], [255, 170, 60], [255, 235, 120]]);
export const RAMP_WARM = Object.freeze([[22, 5, 20], [120, 16, 50], [215, 40, 60], [255, 100, 50], [255, 170, 50], [255, 230, 120]]);
export const RAMP_COOL = Object.freeze([[10, 6, 40], [60, 30, 150], [160, 50, 210], [255, 70, 200], [140, 200, 255], [210, 250, 255]]);

/** How many colours a ramp is cut into, and how many positions between the plain ramp and each end
 * the lean is rounded to. */
export const STEPS = 64;
const LEANS = 32;

const mix = (a, b, f) => a.map((v, i) => Math.round(v + (b[i] - v) * f));

/** The ramp leaning `x` of the way (-1 all the way to the warm ramp, 1 to the cool one). */
export function rampFor(x) {
  const lean = Math.max(-1, Math.min(1, x));
  if (lean === 0) return RAMP;
  const end = lean < 0 ? RAMP_WARM : RAMP_COOL;
  return RAMP.map((c, i) => mix(c, end[i], Math.abs(lean)));
}

/** The `[r, g, b]` of a level `v` (0 to 1) on `ramp`. */
export function heatOf(v, ramp = RAMP) {
  const at = Math.min(0.999, Math.max(0, v)) * (ramp.length - 1);
  const i = Math.floor(at);
  return mix(ramp[i], ramp[i + 1], at - i);
}

const cache = new Map();

/** The `STEPS` colour strings of the ramp leaning `x`, rounded to a 32nd; the same array for the
 * same rounded lean. Read one with `colours[heatIndex(v)]`. */
export function heatColours(x = 0) {
  const key = Math.round(Math.max(-1, Math.min(1, x)) * LEANS);
  let colours = cache.get(key);
  if (!colours) {
    const ramp = rampFor(key / LEANS);
    colours = Array.from({ length: STEPS }, (_, i) => `rgb(${heatOf((i + 0.5) / STEPS, ramp).join(',')})`);
    cache.set(key, colours);
  }
  return colours;
}

/** The index in `heatColours` of the level `v`. */
export const heatIndex = (v) => Math.min(STEPS - 1, Math.max(0, Math.floor(v * STEPS)));
