/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The camera over Night drive's floor: where a point of the road's plane falls on the scene. The
// floor is seen from high, so its rows fall steeply toward the viewer and the far rows are wider
// than they would be seen from lower down. `z` is 1 at the nearest row (the bottom of the scene)
// and 0 at the horizon; `x` is across the road in columns' halves (the spectrum's columns span
// -half to half). Everything is in the scene's CSS pixels.

/** The camera's numbers: the perspective `K` and `spread`, the rows of the floor, the half width of
 * the spectrum's columns and of the plain grid beside them, and the grid's line step. */
export const CAMERA = Object.freeze({
  K: 8, spread: 0.24, rows: 22, half: 6, side: 30, sideStep: 0.5,
});

/** The scene's `[x, y]` of the road's point (`x`, `z`) in a scene `w` wide and `h` tall whose horizon
 * is at `H`. */
export function project(w, h, H, x, z) {
  const d = 1 + (1 - z) * CAMERA.K;
  return [w / 2 + (x * w * CAMERA.spread) / d, H + (h - H) / d];
}

/** The horizon, as a fraction of the scene's height: lower on a phone held upright, whose scene is
 * the top half of the page, so the sun and the mountains have room above the road. */
export function horizonOf({ phone = false, portrait = false } = {}) {
  return phone && portrait ? 0.62 : 0.56;
}
