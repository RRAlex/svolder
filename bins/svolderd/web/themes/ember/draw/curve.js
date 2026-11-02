/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The tone control's curve as the graph draws it, in the view box 0 0 1000 260: the line, the
// area under it, the bands' curve as a ghost, the peak's line and a point for each band. The
// model carries dB values (`tone.response`, 181 of them) and the maps to the view box are
// `core/tone-curve.js`'s, so the page and the drag share them. Numbers are strings of one decimal.

import {
  linePath, POINTS, xOf, yOf,
} from '../../../core/tone-curve.js';

/** The area's closing: down to 0 dB and back along it. */
const CLOSE = 'L1000.0 130.0L0.0 130.0Z';

/**
 * The curve of `tone` (`{ mode, response, markers, peak }`):
 * - `line`: the bands in EQ, flat in Flat and before a reading, empty in Loudness;
 * - `area`: the line closed down to 0 dB, empty with no line;
 * - `ghost`: the bands' curve in Flat and Loudness, `null` without it;
 * - `ys`: the line's 181 y values, comma-separated;
 * - `peak`: `{ y, words }` of the summed peak, or `null`;
 * - `markers`: `[{ band, x, y }]`, a point per band at the level the model says.
 */
export function curve(tone) {
  const { mode, response } = tone;
  const bands = response ? response.map(yOf) : null;
  const flat = Array(POINTS).fill(yOf(0));
  let line = flat;
  let ghost = null;
  if (bands && mode === 'eq') line = bands;
  else if (bands && mode === 'loudness') [line, ghost] = [null, linePath(bands)];
  else if (bands) ghost = linePath(bands);
  const path = line ? linePath(line) : '';
  return {
    line: path,
    area: path ? `${path}${CLOSE}` : '',
    ghost,
    ys: line ? line.map((y) => y.toFixed(1)).join(',') : '',
    peak: tone.peak ? { y: yOf(tone.peak.db).toFixed(1), words: tone.peak.words } : null,
    markers: tone.markers.map((m) => ({
      band: m.band, x: xOf(m.hz).toFixed(1), y: yOf(m.db).toFixed(1),
    })),
  };
}
