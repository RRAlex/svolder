/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What Night drive reads of the music: the analysis's smoothed bands (0 to 1 over -72 to 0 dBFS, a
// channel each) as the ridge's heights and the floor's 24 column levels, the bass and the highs
// the sky's events watch, and how fast the floor's rows are born.

import { BANDS } from '../../core/viz-model.js';
import { BAND_COLUMN, COLUMNS } from './axis.js';

/** A level (0 to 1 over -72 to 0 dBFS) as the ridge's unit: -60 dB is 0, 0 dB is 1. */
export const ridgeUnit = (v) => Math.min(1, Math.max(0, (72 * v - 12) / 60));

/** How many bands each column has. */
const BANDS_IN = Uint8Array.from({ length: COLUMNS }, (_, k) => BAND_COLUMN.filter((c) => c === k).length);

/** The ridge's unit of each band, the louder channel's, into `out` (240 values). */
export function bandUnits(left, right, out) {
  for (let i = 0; i < BANDS; i++) out[i] = ridgeUnit(Math.max(left[i], right[i]));
  return out;
}

/** The 24 column levels into `out`: the mean of the ridge units of the column's bands. */
export function columnLevels(left, right, out) {
  out.fill(0);
  for (let i = 0; i < BANDS; i++) out[BAND_COLUMN[i]] += ridgeUnit(Math.max(left[i], right[i]));
  for (let k = 0; k < COLUMNS; k++) out[k] /= BANDS_IN[k];
  return out;
}

const mean = (cols, from, to) => {
  let sum = 0;
  for (let k = from; k < to; k++) sum += cols[k];
  return sum / (to - from);
};

/** The bass: columns 0 to 5 (20 to 112 Hz). */
export const bassOf = (cols) => mean(cols, 0, 6);

/** The highs: columns 19 to 23 (4.7 to 20 kHz). */
export const highsOf = (cols) => mean(cols, 19, 24);

/** Whether every column is under 0.02. */
export const silent = (cols) => cols.every((v) => v < 0.02);

/** The rows born a second: twice a beat, between 2 and 6; 3.5 with no tempo. */
export const rowRate = (bpm) => (bpm ? Math.min(6, Math.max(2, (2 * bpm) / 60)) : 3.5);
