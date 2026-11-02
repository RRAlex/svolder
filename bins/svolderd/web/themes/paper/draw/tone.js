/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The tone section: the EQ plotted like a measurement (a dashed line at 0 dB, the curve in print,
// a pencil point on each band), the mode and the preset written in the heading as words that open
// lists, the loudness strength with its two steppers, and the band table with a `−` and a `+`
// around every value. The graph's box is the core's plot (1000 by 260) so the pointer maps to a
// frequency and a gain with the core's own maps. Pure: the model in, markup and numbers out.

import {
  POINTS, linePath, logFreqs, markerPoint, responseDb, xOf, yOf,
} from '../../../core/tone-curve.js';
import { presetName, presetOf } from '../notes.js';
import { WORDS } from '../words.js';
import { esc } from './esc.js';
import { word } from './word.js';

const VIEW_WIDTH = 1000;
const VIEW_HEIGHT = 260;
const FREQS = logFreqs(POINTS);
const PARAMS = ['f', 'g', 'q'];

/** The points on the graph, one per band the model marks: where it stands as a fraction of the
 * plot (`x`, `y` from 0 to 1, from the top left). */
export function pointsOf(tone) {
  return (tone?.markers ?? []).map((m) => ({
    band: m.band, x: xOf(m.hz) / VIEW_WIDTH, y: yOf(m.db) / VIEW_HEIGHT,
  }));
}

/** The curve's path in the plot's own box. */
export function curvePath(response) {
  return linePath(response.map(yOf));
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
 * The graph while band `band` (1-based) is held at `raws` (`{ f, g, q }`): the path of the curve
 * the bands make and the held band's point as fractions of the plot, or `null` when a band is not
 * read in full (nothing is drawn then).
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
  return {
    d: curvePath(responseDb(bands, FREQS)),
    point: { x: at.x / VIEW_WIDTH, y: at.y / VIEW_HEIGHT },
  };
}

/** The stepper words round a value: `−`, the value, `+`. `id` makes each button findable after the
 * section is drawn again, so the focus can be put back. */
function stepper(inert, { id, act, label, text, extra = '' }) {
  const one = (step, sign, name) => word(act, sign, inert,
    ` id="${id}${step < 0 ? 'm' : 'p'}" data-step="${step}"${extra} aria-label="${esc(name(step))}"`);
  return `${one(-1, WORDS.handMinus, label.lower)}<span class="p-val">${esc(text)}</span>`
    + `${one(1, '+', label.raise)}`;
}

/** One band's row of the table: its number and its three values, each with its steppers. */
function row(inert, r) {
  const cell = (param) => `<td>${stepper(inert, {
    id: `p-b${r.band}${param}`,
    act: 'band',
    label: {
      lower: () => WORDS.lower(r.band, param), raise: () => WORDS.raise(r.band, param),
    },
    text: r[param],
    extra: ` data-band="${r.band}" data-param="${param}"`,
  })}</td>`;
  return `<tr data-band="${r.band}"><th scope="row">${esc(r.band)}</th>${PARAMS.map(cell).join('')}</tr>`;
}

/** The graph: its zero line, the curve and a point for each band (none in Loudness). */
function graph(tone, device, inert) {
  const dashed = tone.mode === 'flat' ? ' faint' : '';
  const own = tone.mode === 'eq' || tone.mode === 'flat';
  const steps = device.tone_steps;
  const points = own ? pointsOf(tone).map((p) => {
    const r = tone.bands?.rows?.find((x) => x.band === p.band) ?? {};
    const raw = tone.readings?.bands?.[p.band - 1]?.[1];
    return `<button type="button" id="p-pt-${p.band}" class="pt${inert ? ' quiet' : ''}"`
      + ` data-band="${p.band}" data-x="${p.x.toFixed(5)}" data-y="${p.y.toFixed(5)}" role="slider"`
      + ` aria-label="${esc(WORDS.point(p.band, r.f, r.g))}" aria-valuemin="0"`
      + ` aria-valuemax="${Math.max(0, (steps?.gain?.length ?? 1) - 1)}"`
      + `${Number.isInteger(raw) ? ` aria-valuenow="${raw}"` : ''} aria-valuetext="${esc(r.g)}"`
      + `${inert ? ' aria-disabled="true"' : ''}></button>`;
  }).join('') : '';
  return `<div id="p-graph" class="p-graph"><svg viewBox="0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}"`
    + ' preserveAspectRatio="none" aria-hidden="true">'
    + `<path class="p-zero" d="M0 ${yOf(0)}H${VIEW_WIDTH}"/>`
    + `<path class="p-curve${dashed}" d="${curvePath(tone.response ?? [])}"/></svg>${points}</div>`;
}

/** The tone section for the shown device; nothing for a device with no tone control. */
export function tone(_page, device, _view) {
  const t = device?.tone;
  if (!t) return '';
  const inert = Boolean(device.inert);
  const mode = t.modes?.find((m) => m.on)?.label ?? t.mode ?? '';
  const on = presetOf(device.presets);
  const preset = on ? presetName(on) : WORDS.noPreset;
  const opens = [word('mode-list', mode, inert, ' id="p-mode" aria-haspopup="listbox"')];
  if (device.presets) {
    opens.push(word('preset-list', preset, inert, ' id="p-preset" aria-haspopup="listbox"'));
  }
  const strength = t.mode === 'loudness' && t.strength !== null && t.strength !== undefined
    ? `<p class="p-strength">${esc(WORDS.strength)} ${stepper(inert, {
      id: 'p-loud',
      act: 'loudness',
      label: { lower: () => WORDS.lowerLoudness, raise: () => WORDS.raiseLoudness },
      text: t.strength,
    })}</p>`
    : '';
  const heads = WORDS.bandHeads.map((h) => `<th scope="col">${esc(h)}</th>`).join('');
  const rows = (t.bands?.rows ?? []).map((r) => row(inert, r)).join('');
  return `<h2 class="p-label">${esc(WORDS.toneLabel)} &middot; ${opens.join(' ')}</h2>${strength}`
    + `${graph(t, device, inert)}`
    + `<table id="p-bands" class="p-bands${t.bands?.faint ? ' faint' : ''}"><thead><tr>${heads}</tr></thead>`
    + `<tbody>${rows}</tbody></table>`;
}
