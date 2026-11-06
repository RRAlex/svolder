/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The one frequency axis of Night drive's scene: 20 Hz at the left edge, 20 kHz at the right, the
// log scale between, shared by the ridge, the floor's columns, the tone curve and the bubbles. The
// floor's 24 columns are that axis cut into 24 equal parts; a band of the analysis goes to the
// column its centre falls in.

import { BANDS, bandCentreHz } from '../../core/viz-model.js';

/** How many columns the floor has. */
export const COLUMNS = 24;

const F_MIN = 20;
const F_MAX = 20000;
const SPAN = Math.log(F_MAX / F_MIN);

/** The place of `hz` along an axis `w` wide. */
export const axisX = (hz, w) => (w * Math.log(hz / F_MIN)) / SPAN;

/** The frequency at place `x` of an axis `w` wide: the other way of `axisX`. */
export const axisHz = (x, w) => F_MIN * Math.exp((x / w) * SPAN);

/** The column each band of the analysis goes to (a centre past 20 kHz goes to the last). */
export const BAND_COLUMN = Uint8Array.from({ length: BANDS }, (_, i) => Math.min(
  COLUMNS - 1,
  Math.max(0, Math.floor(axisX(bandCentreHz(i), 1) * COLUMNS)),
));
