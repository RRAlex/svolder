/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The tone curve of the DAC's first screen: the EQ plotted on the core's own plot (1000 by 260, so
// the pointer maps to a frequency and a gain with the core's maps), a dashed line at 0 dB, the
// curve with its shade, a point on each band and the frequencies under it. The points are the
// buttons the drag grabs. Pure: the model in, markup and numbers out.

import {
  POINTS, linePath, logFreqs, markerPoint, responseDb, xOf, yOf,
} from '../../../core/tone-curve.js';
import { WORDS } from '../words.js';
import { esc } from './esc.js';

export const VIEW_WIDTH = 1000;
export const VIEW_HEIGHT = 260;
const FREQS = logFreqs(POINTS);
/** The frequencies written under the curve (Hz) and how they read. */
const MARKS = [[60, '60'], [250, '250'], [1000, '1k'], [4000, '4k'], [12000, '12k']];

/** The points on the graph, one per band the model marks: where it stands as a fraction of the plot
 * (`x`, `y` from 0 to 1, from the top left). */
export function pointsOf(tone) {
  return (tone?.markers ?? []).map((m) => ({
    band: m.band, x: xOf(m.hz) / VIEW_WIDTH, y: yOf(m.db) / VIEW_HEIGHT,
  }));
}

/** The curve's path in the plot's own box. */
export const curvePath = (response) => linePath(response.map(yOf));

/** The shade between the curve and the 0 dB line, closed along it. */
export function shadePath(response) {
  const zero = yOf(0);
  return `${curvePath(response)}L${VIEW_WIDTH} ${zero}L0 ${zero}Z`;
}

/** The value of step `raw` in `rows` (`[[value, words], …]`), or nothing for one that is not a
 * number. */
function entry(rows, raw) {
  const row = Number.isInteger(raw) ? rows?.[raw] : null;
  return row && typeof row[0] === 'number' && Number.isFinite(row[0]) ? row[0] : null;
}

/** Band `index` (0-based) as `{ f0, gainDb, q }` from its raw steps (`[f, g, q]`), or nothing when
 * a step is unread. */
function bandOf(steps, index, raws) {
  const f0 = entry(steps.bands?.[index]?.f, raws[0]);
  const gainDb = entry(steps.gain, raws[1]);
  const q = entry(steps.q, raws[2]);
  return f0 !== null && gainDb !== null && q !== null && f0 > 0 ? { f0, gainDb, q } : null;
}

/**
 * The graph while band `band` (1-based) is held at `raws` (`{ f, g, q }`): the path of the curve the
 * bands make and the held band's point as fractions of the plot, or `null` when a band is not read
 * in full (nothing is drawn then).
 */
export function heldGraph(tone, steps, band, raws) {
  const bands = [];
  for (const [i, read] of (tone?.readings?.bands ?? []).entries()) {
    const own = i === band - 1 ? [raws.f, raws.g, raws.q] : read;
    const parsed = bandOf(steps, i, own ?? []);
    if (!parsed) return null;
    bands.push(parsed);
  }
  const at = markerPoint('eq', bands[band - 1]);
  const response = responseDb(bands, FREQS);
  return {
    d: curvePath(response),
    shade: shadePath(response),
    point: { x: at.x / VIEW_WIDTH, y: at.y / VIEW_HEIGHT },
  };
}

/** The graph: its zero line, the shaded curve, a point for each band (none in Loudness) and the
 * frequencies under it. */
function graph(tone, device, inert) {
  const own = tone.mode === 'eq' || tone.mode === 'flat';
  const steps = device.tone_steps;
  const points = own ? pointsOf(tone).map((p) => {
    const r = tone.bands?.rows?.find((x) => x.band === p.band) ?? {};
    const raw = tone.readings?.bands?.[p.band - 1]?.[1];
    return `<button type="button" id="sg-pt-${p.band}" class="sg-pt${inert ? ' quiet' : ''}"`
      + ` data-band="${p.band}" data-x="${p.x.toFixed(5)}" data-y="${p.y.toFixed(5)}" role="slider"`
      + ` aria-label="${esc(WORDS.point(p.band, r.f, r.g))}" aria-valuemin="0"`
      + ` aria-valuemax="${Math.max(0, (steps?.gain?.length ?? 1) - 1)}"`
      + `${Number.isInteger(raw) ? ` aria-valuenow="${raw}"` : ''} aria-valuetext="${esc(r.g)}"`
      + `${inert ? ' aria-disabled="true"' : ''}></button>`;
  }).join('') : '';
  return plot({ id: 'sg-graph', response: tone.response ?? [], faint: tone.mode === 'flat', points });
}

/** The plot: the zero line, the shaded curve (faint while the curve is not the one playing), the
 * buttons of `points` over it and the frequencies under it. The computer's graph is drawn by it too. */
export function plot({ id, response, faint, points }) {
  const dim = faint ? ' faint' : '';
  const freqs = MARKS.map(([hz, text]) => `<span class="sg-flab" data-at="${(xOf(hz) / VIEW_WIDTH).toFixed(5)}">`
    + `${text}</span>`).join('');
  return `<div id="${id}" class="sg-graph"><svg viewBox="0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}"`
    + ' preserveAspectRatio="none" aria-hidden="true">'
    + `<path class="sg-zero" d="M0 ${yOf(0)}H${VIEW_WIDTH}"/>`
    + `<path class="sg-shade${dim}" d="${shadePath(response)}"/>`
    + `<path class="sg-curve${dim}" d="${curvePath(response)}"/></svg>${points}`
    + `<div class="sg-freqs" aria-hidden="true">${freqs}</div></div>`;
}

/** The tone box for the shown device; nothing for a device with no tone control. */
export function toneBox(device) {
  const t = device?.tone;
  if (!t) return '';
  return `<span class="sg-lbl">${esc(WORDS.tone)}</span>${graph(t, device, Boolean(device.inert))}`;
}
