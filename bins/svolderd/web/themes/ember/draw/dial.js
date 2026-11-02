/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The volume's dial: its geometry in the view box -30 -16 260 220, centre (100, 100), a sweep of
// 270 degrees from -135 with 0 up. The knob's drawing and its repaints (`../knob.js`) both use
// it, so the dial a reading arrives at is the dial the page drew. Pure numbers in, numbers and
// strings out: no DOM. Coordinates are strings of two decimals.

/** The dial's sweep, in degrees. */
export const SWEEP = 270;
/** How many ticks it has. */
export const TICKS = 49;
/** How close, in degrees, a label may come to one kept before it. */
export const LABEL_GAP = 10;

/** A number rounded to the nearest whole, halves away from zero. */
const round = (v) => Math.sign(v) * Math.round(Math.abs(v));

/** The angle of fractional raw step `raw` on a dial from `lo` to `hi`, in degrees, 0 up. */
export function angle(raw, lo, hi) {
  return -SWEEP / 2 + (SWEEP * (raw - lo)) / Math.max(hi - lo, 1);
}

/** The point at `deg` and radius `r` about (100, 100): `[x, y]`, each two decimals. */
export function point(deg, r) {
  const a = (deg * Math.PI) / 180;
  return [(100 + r * Math.sin(a)).toFixed(2), (100 - r * Math.cos(a)).toFixed(2)];
}

/** A line along the radius at `deg`, from `r1` to `r2`: `[x1, y1, x2, y2]`. */
export function spoke(deg, r1, r2) {
  return [...point(deg, r1), ...point(deg, r2)];
}

/** An arc of radius `r`, clockwise from `a1` to `a2` degrees, as an SVG path. */
export function arc(a1, a2, r) {
  const [x1, y1] = point(a1, r);
  const [x2, y2] = point(a2, r);
  const large = a2 - a1 > 180 ? 1 : 0;
  return `M${x1} ${y1}A${r} ${r} 0 ${large} 1 ${x2} ${y2}`;
}

/** The step `raw`'s place in the range from `lo` to `hi`, 0 to 1. */
export function fraction(raw, lo, hi) {
  return (raw - lo) / Math.max(hi - lo, 1);
}

/**
 * The class of the tick at step `raw` when `level` is lit (`null`: none): `off cap` above the
 * cap, `lit red` in the risk zone, `off zone` unlit in it, `lit r0` to `lit r4` up the ramp, or
 * `off`. `dial` is `{ lo, hi, risk, cap }`.
 */
export function tickClass(raw, level, { lo, hi, risk, cap }) {
  const lit = level !== null && raw <= level;
  const zone = risk !== null && raw >= risk;
  if (cap !== null && raw > cap) return 'off cap';
  if (lit && zone) return 'lit red';
  if (zone) return 'off zone';
  if (lit) return `lit r${round(fraction(raw, lo, hi) * 4)}`;
  return 'off';
}

/** The level the dial lights up to: none at silence or before a reading. */
const levelOf = (v) => (v.raw === null || (v.silence && v.raw === v.lo) ? null : v.raw);

/** The dial's ticks, lit up to the level: `[{ raw, x1, y1, x2, y2, class }]`. */
export function ticks(volume) {
  const { lo, hi } = volume;
  const level = levelOf(volume);
  return Array.from({ length: TICKS }, (_, i) => {
    const at = lo + ((hi - lo) * i) / (TICKS - 1);
    const raw = round(at);
    const [x1, y1, x2, y2] = spoke(angle(at, lo, hi), 86, 98);
    return {
      raw, x1, y1, x2, y2, class: tickClass(raw, level, volume),
    };
  });
}

/**
 * The dB labels: the candidate `volume.marks` in order, one within LABEL_GAP degrees of one kept
 * before it dropped, then sorted round the dial: `[{ text, at, x, y, red, tick }]`, red inside the
 * risk zone and `tick` its short line's ends.
 */
export function labels(volume) {
  const { lo, hi, risk } = volume;
  const kept = [];
  for (const mark of volume.marks) {
    const a = angle(mark.at, lo, hi);
    if (kept.every((k) => Math.abs(angle(k.at, lo, hi) - a) >= LABEL_GAP)) kept.push(mark);
  }
  kept.sort((x, y) => x.at - y.at);
  return kept.map(({ at, text }) => {
    const a = angle(at, lo, hi);
    const [x, y] = point(a, 112 + 7 * Math.abs(Math.sin((a * Math.PI) / 180)));
    return {
      text,
      at: at.toFixed(2),
      x,
      y,
      red: risk !== null && at >= risk,
      tick: spoke(a, 101.5, 105),
    };
  });
}

/**
 * The dial's arcs and marks, each `null` where it is not drawn: the track (the whole sweep), the
 * value (bottom to the level), the risk zone, a climb's arc and its target tick, the cap's tick
 * and the handle's centre.
 */
export function shapes(volume) {
  const {
    lo, hi, raw, risk, cap, target,
  } = volume;
  const at = (step) => angle(step, lo, hi);
  const top = SWEEP / 2;
  const level = levelOf(volume);
  const climbing = target !== null && raw !== null && target > raw;
  return {
    track: arc(-top, top, 80),
    value: level !== null && level > lo ? arc(-top, at(level), 80) : null,
    zone: risk !== null ? arc(at(risk), top, 80) : null,
    target: climbing ? arc(at(raw), at(target), 80) : null,
    targetTick: climbing ? spoke(at(target), 73, 87) : null,
    capTick: cap !== null ? spoke(at(cap), 86, 98) : null,
    handle: raw !== null ? point(at(raw), 80) : null,
  };
}
