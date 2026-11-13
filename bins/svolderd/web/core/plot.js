/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The shared plots as SVG strings: the response plot (the tone curve's frame, a 0 dB line, a
// frequency grid, a curve and its fill, shaded spans of the frequency axis) and a sparkline over
// time. Pure data in, markup out: no DOM, no state, and only the tone curve's maps imported, so
// node loads it as the browser does. It draws what it is given and fits nothing. Every colour and
// stroke is the caller's class (`cls`); a caller-supplied class is escaped. Response data is
// `{ hz, db }` points (a curve on a grid), a span `{ from_hz, to_hz }`, a sample `{ t, v }`.

import {
  F_MAX, F_MIN, POINTS, VIEW_HEIGHT, VIEW_WIDTH, linePath, logFreqs, xOf, yOf,
} from './tone-curve.js';
import { fixedEven, hzWords } from './words.js';

/** The frequencies a plot's axis is written at (Hz) and how they read. */
export const FREQ_MARKS = [[60, '60'], [250, '250'], [1000, '1k'], [4000, '4k'], [12000, '12k']];

const ESCAPES = {
  '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', '\'': '&#39;',
};
const esc = (text) => String(text ?? '').replace(/[&<>"']/g, (c) => ESCAPES[c]);
/** ` class="…"` for `cls` (escaped), or nothing for no class. */
const klass = (cls) => (cls ? ` class="${esc(cls)}"` : '');
const one = (value) => fixedEven(value, 1);
const isNum = (value) => typeof value === 'number' && Number.isFinite(value);
const inView = (hz) => isNum(hz) && hz >= F_MIN && hz <= F_MAX;

/** The fraction of the plot's width (0 to 1) at frequency `hz`. */
export const freqAt = (hz) => xOf(hz) / VIEW_WIDTH;

/** The plot's `<svg>` around `body`: the tone curve's view box, stretched to its element. */
export function frame(body, { cls } = {}) {
  return `<svg${klass(cls)} viewBox="0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}" preserveAspectRatio="none" aria-hidden="true">`
    + `${body}</svg>`;
}

/** The 0 dB line across the plot. */
export const zeroLine = (cls) => `<path${klass(cls)} d="M0 ${yOf(0)}H${VIEW_WIDTH}"/>`;

/** One vertical line, full height, for each frequency of `hzs` the plot shows. */
export function gridLines(hzs, cls) {
  return (hzs ?? []).filter(inView)
    .map((hz) => `<path${klass(cls)} d="M${one(xOf(hz))} 0V${VIEW_HEIGHT}"/>`).join('');
}

/** The path through the response `response`: a finite dB for each of the tone curve's frequencies
 * (a value that is not a number prints as `NaN`; `pathThrough` is the one that leaves such out). */
export const curveD = (response) => linePath(response.map(yOf));

/** The same path closed down to the 0 dB line, the shade between the curve and it. */
export function fillD(response) {
  const zero = yOf(0);
  return `${curveD(response)}L${VIEW_WIDTH} ${zero}L0 ${zero}Z`;
}

/** `d` (escaped) as a `<path>` of class `cls`. */
export const path = (d, cls) => `<path${klass(cls)} d="${esc(d)}"/>`;

/** The path through `points` (`[{ hz, db }]`, any grid); the points that are not numbers are left
 * out, a gain past the plot's range is held at its edge. Nothing for no point. */
export function pathThrough(points) {
  return (points ?? []).filter((p) => isNum(p?.hz) && p.hz > 0 && isNum(p?.db))
    .map((p, i) => `${i === 0 ? 'M' : 'L'}${one(xOf(Math.min(Math.max(p.hz, F_MIN), F_MAX)))} ${one(yOf(p.db))}`)
    .join('');
}

/** A span's ends held to the plot's frequencies, or nothing for one that does not reach into it. */
function clipped(span) {
  if (!isNum(span?.from_hz) || !isNum(span?.to_hz)) return null;
  const from = Math.max(span.from_hz, F_MIN);
  const to = Math.min(span.to_hz, F_MAX);
  return to > from ? { from, to } : null;
}

/** Each of `spans` (`[{ from_hz, to_hz }]`) as a full-height band across the plot. */
export function bandSpans(spans, cls) {
  return (spans ?? []).map(clipped).filter(Boolean).map(({ from, to }) => {
    const x = xOf(from);
    return `<rect${klass(cls)} x="${one(x)}" y="0" width="${one(xOf(to) - x)}" height="${VIEW_HEIGHT}"/>`;
  }).join('');
}

/** The dB of `curve` (`[{ hz, db }]` by rising frequency) at `hz`, straight between its two
 * neighbours on the log axis; nothing outside the curve. */
function dbOn(curve, hz) {
  for (let i = 0; i < curve.length; i++) {
    if (curve[i].hz === hz) return curve[i].db;
    if (curve[i].hz > hz) {
      if (i === 0) return null;
      const lo = curve[i - 1];
      const at = Math.log(hz / lo.hz) / Math.log(curve[i].hz / lo.hz);
      return lo.db + (curve[i].db - lo.db) * at;
    }
  }
  return null;
}

/** For each of `spans`, the area between `target` and `fit` (`[{ hz, db }]` on one grid) inside the
 * span as one closed path of class `cls`: along the target, back along the fit. A span outside the
 * plot or the curves is left out. */
export function gapSpans({ spans, target, fit, cls }) {
  const upper = (target ?? []).filter((p) => isNum(p?.hz) && p.hz > 0 && isNum(p?.db));
  const lower = (fit ?? []).filter((p) => isNum(p?.hz) && p.hz > 0 && isNum(p?.db));
  if (!upper.length || !lower.length) return '';
  const reach = [Math.max(upper[0].hz, lower[0].hz), Math.min(upper.at(-1).hz, lower.at(-1).hz)];
  return (spans ?? []).map(clipped).filter(Boolean).map((s) => ({
    from: Math.max(s.from, reach[0]), to: Math.min(s.to, reach[1]),
  })).filter(({ from, to }) => to > from).map(({ from, to }) => {
    const hzs = [from, ...upper.map((p) => p.hz).filter((hz) => hz > from && hz < to), to];
    const top = hzs.map((hz) => ({ hz, db: dbOn(upper, hz) }));
    const bottom = hzs.map((hz) => ({ hz, db: dbOn(lower, hz) })).reverse();
    return `<path${klass(cls)} d="${pathThrough(top)}${pathThrough(bottom).replace('M', 'L')}Z"/>`;
  }).join('');
}

const OVERLAY_FREQS = logFreqs(POINTS);
const onGrid = (db) => (Array.isArray(db) ? OVERLAY_FREQS.map((hz, i) => ({ hz, db: db[i] })) : []);

/**
 * The shapes of a tone view's overlay (`tone.overlay`, the daemon's numbers on the tone curve's 181
 * points): `target`, the target's line of class `cls.target`, and `gap`, the area between the fit
 * and the target over the daemon's own `gap` spans (`[from_hz, to_hz]`) in `cls.gap`. Both are
 * empty for no overlay, and `gap` is empty without a fit or a span: nothing is worked out here.
 */
export function overlayShapes(overlay, cls) {
  if (!overlay) return { target: '', gap: '' };
  const target = onGrid(overlay.target);
  const d = pathThrough(target);
  return {
    target: d ? path(d, cls?.target) : '',
    gap: gapSpans({
      spans: (overlay.gap ?? []).map(([from_hz, to_hz]) => ({ from_hz, to_hz })),
      target,
      fit: onGrid(overlay.fit),
      cls: cls?.gap,
    }),
  };
}

/** What the overlay says of the fit, `fit 0.9 dB` (the daemon's RMS miss); nothing without one. */
export const fitWords = (overlay) => (isNum(overlay?.rms_db) ? `fit ${one(overlay.rms_db)} dB` : '');

/** The daemon's largest miss in words, `3.1 dB at 4.2 kHz`; nothing without one. */
function worstWords(overlay) {
  const { worst_db: db, worst_hz: hz } = overlay ?? {};
  if (!isNum(db) || !isNum(hz) || hz <= 0) return '';
  return `${one(db)} dB at ${hzWords(hz)}`;
}

/** The overlay in a sentence for a reader who cannot see the graph: the fit, the worst miss and what the
 * dotted line and the shade are (`gap_db` is the daemon's own threshold); nothing without a fit. */
export function overlayNote(overlay) {
  const fit = fitWords(overlay);
  if (!fit) return '';
  const worst = worstWords(overlay);
  const key = isNum(overlay.gap_db)
    ? ` The dotted line is the target; the shade is where the bands miss it by more than ${Number(overlay.gap_db)} dB.`
    : ' The dotted line is the target.';
  return `${fit}${worst ? `, worst ${worst}` : ''}.${key}`;
}

/**
 * A small time series as an `<svg>` of `width` by `height` (its own box, stretched to its element):
 * the line through `points` (`[{ t, v }]`) with its class `${cls}-line`, a dot (`${cls}-dot`) for a
 * sample standing alone. A sample with no number for `v` is missing and breaks the line, as does a
 * step in `t` over `gap`. The time runs over `from` to `to` (the samples' own span when not given;
 * a sample outside is not drawn), the value over `min` to `max` (the samples' own when not given;
 * a value outside is held to the edge), `pad` in from the top and bottom. A flat series sits
 * mid-height. No samples draws the empty box. A `width`, `height` or `pad` that is not a number
 * takes its default. The box is stretched to its element, so style the line with
 * `vector-effect: non-scaling-stroke` and a dot is an ellipse when the stretch is uneven.
 */
export function sparkline({
  points, width: wide, height: high, pad: inset, min, max, from, to, gap, cls,
}) {
  const width = isNum(wide) && wide > 0 ? wide : 120;
  const height = isNum(high) && high > 0 ? high : 32;
  const pad = isNum(inset) && inset >= 0 && inset * 2 < height ? inset : Math.min(2, height / 4);
  const kept = (points ?? []).filter((p) => isNum(p?.t) && (!isNum(from) || p.t >= from)
    && (!isNum(to) || p.t <= to)).sort((a, b) => a.t - b.t);
  const live = kept.filter((p) => isNum(p.v));
  const t0 = isNum(from) ? from : kept[0]?.t;
  const t1 = isNum(to) ? to : kept.at(-1)?.t;
  let lo = Infinity;
  let hi = -Infinity;
  for (const p of live) {
    lo = Math.min(lo, p.v);
    hi = Math.max(hi, p.v);
  }
  if (isNum(min)) lo = min;
  if (isNum(max)) hi = max;
  const x = (t) => (t1 > t0 ? width * (t - t0) / (t1 - t0) : width / 2);
  const y = (v) => {
    if (!Number.isFinite(hi - lo) || !(hi > lo)) return height / 2;
    return pad + (1 - (Math.min(Math.max(v, lo), hi) - lo) / (hi - lo)) * (height - 2 * pad);
  };
  const runs = [];
  let last = null;
  for (const p of kept) {
    if (!isNum(p.v)) {
      last = null;
    } else {
      if (last === null || (isNum(gap) && p.t - last > gap)) runs.push([]);
      runs.at(-1).push(p);
      last = p.t;
    }
  }
  const own = cls ? esc(cls) : '';
  const named = (part) => (cls ? ` class="${own}${part}"` : '');
  const lines = runs.filter((r) => r.length > 1)
    .map((r) => r.map((p, i) => `${i === 0 ? 'M' : 'L'}${one(x(p.t))} ${one(y(p.v))}`).join('')).join('');
  const dots = runs.filter((r) => r.length === 1)
    .map(([p]) => `<circle${named('-dot')} cx="${one(x(p.t))}" cy="${one(y(p.v))}" r="1.5"/>`).join('');
  const body = (lines ? `<path${named('-line')} d="${lines}"/>` : '') + dots;
  return `<svg${klass(cls)} viewBox="0 0 ${width} ${height}" preserveAspectRatio="none" aria-hidden="true">`
    + `${body}</svg>`;
}
