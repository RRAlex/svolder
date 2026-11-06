/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The floor's rows: each is the 24 column levels at the moment it was born at the horizon. A row is
// born `rate` times a second while the road rolls, and the 22 newest are kept, row 0 the newest.
// `frac` is how far the rows have rolled toward the viewer since the last birth. `peak` is the loudest
// cell (as the fill averages it) the floor has been fed lately: it rises with the music at once and relaxes toward the current
// loudest cell over `PEAK_TAU` seconds, so the heat is read against the floor's own recent range.

import { COLUMNS } from './axis.js';
import { CAMERA } from './camera.js';

/** The time constant of the floor's peak, in seconds. */
export const PEAK_TAU = 60;

/** The floor of `rows` rows. `advance(dt, rate, cols, rolling)` rolls it by `dt` seconds (nothing
 * when it is not `rolling`); `row(k)` is row `k` or null where none was born. */
export function createFloor({ rows = CAMERA.rows } = {}) {
  const ring = Array.from({ length: rows }, () => new Float32Array(COLUMNS));
  let whole = 0;
  let frac = 0;
  let peak = 0;
  return {
    get whole() { return whole; },
    get peak() { return peak; },
    get frac() { return frac; },

    advance(dt, rate, cols, rolling) {
      // the loudest cell as the fill draws it: a cell is the mean of the four crossings round it, each the mean of
      // two columns, so a row of these is (left + 2 * column + right) / 4 (the road's edge counts as silence)
      let loudest = 0;
      for (let i = 0; i < cols.length; i++) {
        const cell = ((i > 0 ? cols[i - 1] : 0) + 2 * cols[i] + (i < cols.length - 1 ? cols[i + 1] : 0)) / 4;
        if (cell > loudest) loudest = cell;
      }
      peak = loudest >= peak ? loudest : peak + (loudest - peak) * (1 - Math.exp(-dt / PEAK_TAU));
      if (!rolling) return;
      frac += dt * rate;
      while (frac >= 1) {
        frac -= 1;
        ring[whole % rows].set(cols);
        whole++;
      }
    },

    row(k) {
      if (!Number.isInteger(k) || k < 0 || k >= rows || k >= whole) return null;
      return ring[(whole - 1 - k) % rows];
    },

    clear() {
      whole = 0;
      frac = 0;
      peak = 0;
    },
  };
}
