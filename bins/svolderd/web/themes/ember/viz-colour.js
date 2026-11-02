/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The visualizer's colours, as numbers: the base scale for sound both channels share, the tint
// that shows where the channels differ, and the colours of the graph overlay. Pure functions, no
// DOM and no canvas, so the canvas layer and the node harness (`scripts/web-checks/node/viz.mjs`)
// run the same code.
//
//   base(v)        `[r, g, b]` (integers) on the scale from near black through the page's warm
//                  grey to cream, for a level `v` of 0 to 1 (clamped; not a number reads 0)
//   tint(l, r)     `{ to, amount }`: the colour the channels' difference leans to (`TINT_LEFT`
//                  or `TINT_RIGHT`, null when equal) and how far, 0 to 1
//   pixel(l, r)    `[r, g, b]` for a point where the left channel is at `l` and the right at `r`
//                  (each 0 to 1): the base colour of the louder one, mixed toward the tint
//   alpha(l, r)   how opaque that point is: 0 for silence, rising to 1 at `1 / ALPHA_GAIN` of the
//                 range, so the spectrogram's silence is transparent and quiet music mostly opaque
//   BASE_STOPS, ALPHA_GAIN, TINT_LEFT, TINT_RIGHT, CURVE_LEFT, CURVE_RIGHT, CURVE_FILL_ALPHA

/** The base scale: `[level, [r, g, b]]` from silence to full scale; between two the colour is
 * the straight mix. Level 0 is what a new waterfall starts filled with. */
export const BASE_STOPS = Object.freeze([
  [0, Object.freeze([12, 12, 12])],
  [0.35, Object.freeze([45, 43, 44])],
  [0.65, Object.freeze([123, 119, 116])],
  [0.85, Object.freeze([202, 198, 191])],
  [1, Object.freeze([252, 248, 240])],
]);

/** The tint where the left channel is louder, cyan. */
export const TINT_LEFT = Object.freeze([40, 220, 255]);
/** The tint where the right channel is louder, orange. */
export const TINT_RIGHT = Object.freeze([255, 135, 27]);

/** The left channel's curve in the graph overlay. */
export const CURVE_LEFT = Object.freeze([80, 230, 255]);
/** The right channel's curve in the graph overlay. */
export const CURVE_RIGHT = Object.freeze([255, 150, 50]);
/** The alpha of each curve's own fill (plain alpha, never added, so two never sum to white). */
export const CURVE_FILL_ALPHA = 0.1;

/** How fast a spectrogram point turns opaque: its alpha is the louder channel's level times this,
 * at most 1, so a level of a quarter of the scale (quiet music) is already fully drawn and
 * silence is transparent. */
export const ALPHA_GAIN = 4;

/** How far the channels' difference is amplified before it is clamped: a gap of 1/6 of the range
 * is the whole tint. */
const TINT_GAIN = 6;
/** How far a level is lifted before the scale reads it: the loudest of the two, a little up. */
const BASE_LIFT = 1.12;
/** How far the loud level is lifted before it weights the tint: below 2/3 the tint is less. */
const TINT_LEVEL_GAIN = 1.5;

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));
const level = (v) => (Number.isFinite(v) ? clamp(v, 0, 1) : 0);

/** The scale's colour at `v`, unrounded. */
function baseRaw(v) {
  const at = level(v);
  for (let i = 1; i < BASE_STOPS.length; i++) {
    const [hi, to] = BASE_STOPS[i];
    if (at <= hi) {
      const [lo, from] = BASE_STOPS[i - 1];
      const k = (at - lo) / (hi - lo);
      return from.map((c, j) => c + (to[j] - c) * k);
    }
  }
  return [...BASE_STOPS[BASE_STOPS.length - 1][1]];
}

/** The base colour at level `v` of 0 to 1, as integers. */
export function base(v) {
  return baseRaw(v).map(Math.round);
}

/**
 * How the channels differ at one point: `d` is the gap `(l - r) * 6` clamped to -1..1; the tint
 * leans to `TINT_LEFT` when the left is louder and to `TINT_RIGHT` when the right is, by
 * `min(1, |d|)` times `min(1, max(l, r) * 1.5)`, so quiet sound is tinted less. Equal channels
 * give `{ to: null, amount: 0 }`.
 */
export function tint(l, r) {
  const left = level(l);
  const right = level(r);
  const d = clamp((left - right) * TINT_GAIN, -1, 1);
  if (d === 0) return { to: null, amount: 0 };
  const amount = Math.min(1, Math.abs(d)) * Math.min(1, Math.max(left, right) * TINT_LEVEL_GAIN);
  return { to: d > 0 ? TINT_LEFT : TINT_RIGHT, amount };
}

/** The colour of a point where the channels read `l` and `r`: the base colour of the louder at
 * 1.12 times its level, mixed toward the tint by the tint's amount; integers. */
export function pixel(l, r) {
  const from = baseRaw(Math.max(level(l), level(r)) * BASE_LIFT);
  const { to, amount } = tint(l, r);
  if (!to) return from.map(Math.round);
  return from.map((c, i) => Math.round(c + (to[i] - c) * amount));
}

/** How opaque a spectrogram point is where the channels read `l` and `r`: the louder level times
 * `ALPHA_GAIN`, clamped to 0..1 (0 for silence). */
export function alpha(l, r) {
  return Math.min(1, Math.max(level(l), level(r)) * ALPHA_GAIN);
}
