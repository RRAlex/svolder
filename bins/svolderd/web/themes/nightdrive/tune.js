/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The tone control on the mountains, as numbers: the map between the scene's pixels and the
// frequency and gain (one axis with the ridge, the floor and the curve), where each band's bubble
// stands, the curve a held drag draws, and the morph that turns the ridge into the curve and back.
// Plain values in and out, no DOM and no clock of its own: the pointer's place is in CSS px of the
// scene, the time is handed in.

import { POINTS, logFreqs, responseDb } from '../../core/tone-curve.js';
import { axisHz, axisX } from './axis.js';
import { inertOf } from './inert.js';

/** How long the curve stays after the pointer last moved over the mountains, and after the last drag
 * end, wheel notch or lifted finger, ms. */
export const GRACE_MS = 1500;

/** The share of the gap to the target the morph keeps each frame at 60 a second. */
const KEEP = 0.88;

/** The band of the scene the pointer turns the mountains into the curve in: the window the curve is drawn in (`gMax` dB
 * above the 0 dB line at `up` of the picture's height above the horizon, `gMax` dB below it at `down` below it: the
 * map's own) and `margin` CSS px beyond each edge, so a bubble at the edge of the window is grabbed whole. */
const WINDOW = { base: 0.08, span: 0.14 };
const BAND = { up: WINDOW.base + WINDOW.span, down: WINDOW.span - WINDOW.base, margin: 26 };

/** The gain range the map shows when the device lists none, dB. */
const GAIN_FALLBACK = 12;

/** The largest gain of a device's steps in dB, `GAIN_FALLBACK` when it lists none. */
export function gMaxOf(steps) {
  const gains = (steps?.gain ?? []).map(([db]) => Math.abs(db)).filter(Number.isFinite);
  const most = Math.max(0, ...gains);
  return most > 0 ? most : GAIN_FALLBACK;
}

/**
 * The map of a scene `w` by `h` with its horizon at `H` (`v`, the height the picture is composed in where the panel
 * covers its foot, is `h` unless said): the frequency at an x, the gain at a y
 * and the y of a gain, 0 dB at `H - 0.08h` and `gMax` dB `0.14h` above and below it. The y of a gain is held to that
 * window (two points on one frequency add up beyond it), which is also the band that takes a touch (`inBand`).
 */
export function mountainMap({
  w, h, v = h, H, gMax,
}) {
  const base = H - WINDOW.base * v;
  const span = WINDOW.span * v;
  return {
    hzAt: (x) => axisHz(x, w),
    dbAt: (y) => ((base - y) * gMax) / span,
    yOfDb: (db) => base - (Math.max(-gMax, Math.min(gMax, db)) * span) / gMax,
  };
}

/** Whether `y` is in the band of the scene that turns the mountains into the curve. */
export const inBand = ({ h, v = h, H }, y) => y > H - BAND.up * v - BAND.margin && y < H + BAND.down * v + BAND.margin;

/** Whether the page can be tuned from the mountains: the viewer may write and the shown device
 * has a tone curve with bands. */
export function tunable(page) {
  const tone = page?.device?.tone;
  return !inertOf(page) && Boolean(tone?.markers?.length && tone.response?.length
    && page.device.tone_steps?.bands?.length);
}

/** The bubbles of `tone` (the model's, or `heldTone`'s) in a scene `w` wide: `[{ band, x, y }]`,
 * each at its band's own frequency and gain; none in Loudness, which has no bands of its own. */
export function bubblesOf(tone, map, w) {
  if (!tone || tone.mode === 'loudness') return [];
  return (tone.markers ?? []).map((m) => ({ band: m.band, x: axisX(m.hz, w), y: map.yOfDb(m.db) }));
}

/** The y of the curve `response` (the dB at the core's log-spaced frequencies) at each x of a
 * scene `w` wide: `x => y`. */
export function curveAt(response, map, w) {
  const last = response.length - 1;
  return (x) => {
    const at = Math.min(last, Math.max(0, (x / w) * last));
    const i = Math.floor(at);
    const j = Math.min(last, i + 1);
    return map.yOfDb(response[i] + (response[j] - response[i]) * (at - i));
  };
}

const FREQS = logFreqs(POINTS);

/** The dB of `response` (the core's 181 log-spaced values) at `hz`, read as the drawn curve is: between the two
 * values either side of it, so a point put there sits on that curve to the pixel. */
function responseAt(response, hz) {
  const last = response.length - 1;
  const at = Math.min(last, Math.max(0, (Math.log(hz / F_MIN) / Math.log(F_MAX / F_MIN)) * last));
  const i = Math.floor(at);
  const j = Math.min(last, i + 1);
  return response[i] + (response[j] - response[i]) * (at - i);
}

/**
 * The computer's tone (the core's `toneOf`) as the mountains draw a tone: `{ mode, markers, response }`, the
 * markers one per point, each at its own frequency (held to the axis) and on the curve, whatever the point's
 * type, and the points themselves (`points`, the core's, held values laid over them). `null` for no tone.
 */
export function computerTone(tone) {
  if (!tone) return null;
  return {
    mode: tone.modes.find((m) => m.on)?.name ?? 'off',
    response: tone.response,
    points: tone.points,
    markers: tone.points.map((p) => {
      const hz = Math.min(F_MAX, Math.max(F_MIN, p.f));
      return { band: p.n, hz, db: responseAt(tone.response, hz) };
    }),
  };
}

/** Whether the computer's EQ can be tuned from the mountains: the viewer may write and its tone has points. */
export const tunableComputer = (page, tone) => !inertOf(page) && Boolean(tone?.markers?.length && tone.response?.length);

/** The value of step `raw` in `rows` (`[[value, words], …]`), or null for one that is not a number. */
function entry(rows, raw) {
  const row = Number.isInteger(raw) ? rows?.[raw] : null;
  return row && typeof row[0] === 'number' && Number.isFinite(row[0]) ? row[0] : null;
}

/** Band `index` (0-based) as `{ f0, gainDb, q }` from its raw steps `[f, g, q]`, or null when a
 * step is unread. */
function bandOf(steps, index, raws) {
  const f0 = entry(steps.bands?.[index]?.f, raws[0]);
  const gainDb = entry(steps.gain, raws[1]);
  const q = entry(steps.q, raws[2]);
  return f0 !== null && gainDb !== null && q !== null && f0 > 0 ? { f0, gainDb, q } : null;
}

/**
 * The bands of `tone` as `[{ f0, gainDb, q }]` while the drags of `held` (a `Map` of band number to
 * `{ f, g, q }` raw steps) are on: the other bands as the model reads them; null when a band is not
 * read in full.
 */
export function bandsOf(tone, steps, held) {
  const bands = [];
  for (const [i, read] of (tone?.readings?.bands ?? []).entries()) {
    const raws = held.get(i + 1);
    const parsed = bandOf(steps, i, raws ? [raws.f, raws.g, raws.q] : read ?? []);
    if (!parsed) return null;
    bands.push(parsed);
  }
  return bands;
}

/**
 * `tone` as it stands while the drags of `held` are on: the markers and the curve the bands make.
 * The model's own tone when a band is not read in full.
 */
export function heldTone(tone, steps, held) {
  const bands = bandsOf(tone, steps, held);
  if (!bands) return tone;
  return {
    ...tone,
    markers: bands.map((b, i) => ({ band: i + 1, hz: b.f0, db: b.gainDb })),
    response: responseDb(bands, FREQS),
  };
}

const F_MIN = 20;
const F_MAX = 20000;
/** The gains under which a band is flat and its diamonds are at the analog width, dB. */
const FLAT_DB = 0.05;
/** Bisection steps on the log of the frequency: far below a pixel. */
const BISECT = 48;

/**
 * Where the bell of one band `{ f0, gainDb, q }` is at half its gain: `{ lo, hi }`, the frequencies
 * below and above `f0` where the core's response of that band alone crosses `gainDb / 2` (an end of
 * the axis, 20 Hz or 20 kHz, where it never does). A band under 0.05 dB has the analog width. A computer's
 * point has a `type` (PK, LSC, HSC), which its response follows.
 */
export function halfPoints({
  f0, gainDb, q, type,
}) {
  if (Math.abs(gainDb) < FLAT_DB) {
    const octaves = Math.asinh(1 / (2 * q)) / Math.LN2;
    return { lo: f0 * 2 ** -octaves, hi: f0 * 2 ** octaves };
  }
  const half = Math.abs(gainDb) / 2;
  // inside the bell: at least half the gain, on the gain's side
  const inside = (hz) => Math.sign(gainDb) * responseDb([{
    f0, gainDb, q, type,
  }], [hz])[0] >= half;
  const find = (end) => {
    if (inside(end)) return end;
    let [out, within] = [end, f0];
    for (let i = 0; i < BISECT; i++) {
      const mid = Math.sqrt(out * within);
      if (inside(mid)) within = mid;
      else out = mid;
    }
    return Math.sqrt(out * within);
  };
  return { lo: find(Math.min(F_MIN, f0)), hi: find(Math.max(F_MAX, f0)) };
}

/**
 * The index of the Q step (`qs`, the Q values) whose half point on `side` ('lo' or 'hi') of the
 * band `{ f0, gainDb }` is nearest `hz` in log distance, the lower index on a tie.
 */
export function qAtDiamond(qs, band, side, hz) {
  let best = 0;
  let least = Infinity;
  qs.forEach((q, i) => {
    const distance = Math.abs(Math.log(halfPoints({ ...band, q })[side] / hz));
    if (distance < least) {
      best = i;
      least = distance;
    }
  });
  return best;
}

/** The two diamonds of `band` `{ f0, gainDb, q }` in a scene `w` wide: `[{ side, x, y }]`, at the
 * bell's half points, on the curve `curve` (x => y; the half gain's own height where there is no
 * curve). */
export function diamondsOf(band, map, w, curve = null) {
  const half = halfPoints(band);
  const at = (hz) => {
    const x = axisX(hz, w);
    return { x, y: curve ? curve(x) : map.yOfDb(band.gainDb / 2) };
  };
  return [{ side: 'lo', ...at(half.lo) }, { side: 'hi', ...at(half.hi) }];
}

/** The words above a held bubble: `250 Hz · +1.5 dB · Q 1.00`, from the steps' own words. */
export function labelOf(steps, band, raws) {
  const f = steps.bands?.[band - 1]?.f?.[raws.f]?.[1];
  const g = steps.gain?.[raws.g]?.[1];
  const q = steps.q?.[raws.q]?.[1];
  return `${f} · ${g} · Q ${q}`;
}

/**
 * The morph from the music to the curve: `m` (0 to 1) eases to the target, which is 1 while the
 * pointer is over the mountains and moved within `GRACE_MS`, while a drag is held, or for `GRACE_MS`
 * after the last touch; and 0 while `allow(false)`. `step(dt, now)` (seconds, ms) returns `m`; the bubbles are drawn above
 * 0.02 and can be grabbed from 0.5.
 */
export function createMorph() {
  let m = 0;
  let over = false;
  let movedAt = -Infinity;
  let held = false;
  let touched = -Infinity;
  let allowed = true;
  let clock = 0;

  const target = () => (allowed && ((over && clock - movedAt < GRACE_MS) || held || clock - touched < GRACE_MS) ? 1 : 0);

  return {
    step(dt, now) {
      clock = now;
      m += (target() - m) * (1 - KEEP ** (dt * 60));
      if (m < 0.0005 && target() === 0) m = 0;
      return m;
    },

    /** The pointer is over the mountains' band (`on`) or is not; `at` is when it moved there. A
     * pointer that rests keeps the curve for the grace only. */
    over(on, at = clock) {
      over = on;
      if (on) {
        movedAt = at;
        clock = Math.max(clock, at);
      }
    },

    /** A wheel notch, a lifted finger or a touch at `now` keeps the curve for the grace. */
    touch(now) {
      clock = now;
      touched = now;
    },

    /** A drag, a diamond or a pinch is on (`on`), or has ended at `now`. */
    held(on, now = clock) {
      held = on;
      clock = now;
      if (!on) touched = now;
    },

    /** Whether the page may be tuned at all; nothing morphs while it may not. */
    allow(on) {
      allowed = on;
    },

    /** Move the clock to `now` without a step (to read `target` at it). */
    at(now) {
      clock = now;
    },

    get m() {
      return m;
    },

    /** Whether `m` is still on its way to the target (not yet settled within 0.01). */
    get moving() {
      return Math.abs(target() - m) > 0.01;
    },

    get target() {
      return target();
    },
  };
}
