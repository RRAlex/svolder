/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The pictures of the Virtual speakers tab as strings (data in, SVG out; no DOM, no state): the
// three views of the chosen set (Arrivals, Directions, Decay) and the response curve. Every
// number is the daemon's: the view's `curve` (four magnitude series), its `angles` and the
// set's `timing` (`SpeakersView.timing`: the arrivals at each ear, the tail of the room, the decay
// by band). Nothing is worked out of an impulse response here. A picture with no data to draw
// answers `''` and its panel says so in one line. Classes are frame.css's `fps-*`, which read the
// `--frame-*` tokens alone.

import { attr, esc } from '../../html.js';
import { fixedEven } from '../../words.js';

/** The words of the pictures. */
export const PICTURE_WORDS = {
  arrivals: 'Arrivals',
  directions: 'Directions',
  decay: 'Decay',
  left: 'Left ear',
  right: 'Right ear',
  near: 'near, direct',
  far: (ms) => `far, +${msText(ms)} ms`,
  reflections: 'first reflections and tail',
  axisArrivals: 'ms after the first arrival',
  axisDecay: 'time in ms, log scale',
  bands: { low: 'low', mid: 'mid', high: 'high' },
  response: 'Level at each ear by frequency, from 20 Hz to 20 kHz',
  sameEar: 'Same ear',
  otherEar: 'Other ear',
  cutTo: (secs) => `cut to ${secs} s`,
  endsAt: (ms) => `ends at ${msText(ms)} ms`,
};

/** A time in ms for a label: up to two decimals, no trailing zero. */
export function msText(ms) {
  if (!Number.isFinite(ms)) return '';
  return String(Number(ms >= 10 ? ms.toFixed(0) : ms >= 1 ? ms.toFixed(1) : ms.toFixed(2)));
}

const num = (value) => fixedEven(value, 1);
const text = (x, y, words, extra = '', cls = 'fps-t') => `<text class="${cls}"${attr('x', num(x))}`
  + `${attr('y', num(y))}${extra}>${esc(words)}</text>`;
const finite = (list) => Array.isArray(list) && list.every((one) => Number.isFinite(one));

/** The set's measured angles and distance, `{ left_deg, right_deg, distance_m }`, from what a
 * catalogue entry carries (`angles`) or the view's sentence ("left 30.0 deg, right 330.0 deg,
 * 1.5 m"); `null` when neither says. */
export function anglesOf(structured, sentence) {
  if (structured && Number.isFinite(structured.left_deg) && Number.isFinite(structured.right_deg)) {
    return {
      left_deg: structured.left_deg,
      right_deg: structured.right_deg,
      distance_m: Number.isFinite(structured.distance_m) ? structured.distance_m : null,
    };
  }
  const found = /left\s+(-?[\d.]+)\s*(?:deg|°)[^\d-]*right\s+(-?[\d.]+)\s*(?:deg|°)(?:[^\d-]*([\d.]+)\s*m)?/i
    .exec(sentence ?? '');
  if (!found) return null;
  const [left, right, distance] = [Number(found[1]), Number(found[2]), Number(found[3])];
  if (!Number.isFinite(left) || !Number.isFinite(right)) return null;
  return {
    left_deg: left, right_deg: right, distance_m: Number.isFinite(distance) ? distance : null,
  };
}

/** The level difference between the near and the far ear for each speaker, in dB, from the four
 * curves (the mean over 200 Hz to 5 kHz of near minus far); `null` without a curve. */
export function earDifference(curve) {
  const { hz, ll_db: ll, lr_db: lr, rl_db: rl, rr_db: rr } = curve ?? {};
  if (!finite(hz) || hz.length < 2 || ![ll, lr, rl, rr].every((one) => finite(one) && one.length === hz.length)) {
    return null;
  }
  const take = (near, far) => {
    let sum = 0;
    let count = 0;
    hz.forEach((f, i) => {
      if (f >= 200 && f <= 5000) {
        sum += near[i] - far[i];
        count++;
      }
    });
    return count === 0 ? 0 : sum / count;
  };
  return { left: take(ll, lr), right: take(rr, rl) };
}

const niceMax = (value) => {
  const steps = [0.1, 0.2, 0.25, 0.5, 1, 2, 2.5, 5, 10, 20, 25, 50, 100];
  return steps.find((one) => one >= value) ?? Math.ceil(value);
};

/** Arrivals: for each ear a lane of ticks on a ms axis, the tick's height its level (0 dB tallest,
 * -30 dB shortest); the first two are the direct sound (the near speaker, then the far one) and
 * the rest reflections; With room the ear's tail under them. `''` with no arrivals. */
export function arrivalsSvg(arrivals) {
  const sides = [['left', PICTURE_WORDS.left, 92], ['right', PICTURE_WORDS.right, 184]];
  if (!arrivals || sides.some(([key]) => !Array.isArray(arrivals[key]) || arrivals[key].length === 0)) {
    return '';
  }
  const W = 330;
  const L = 68;
  const R = 8;
  const pw = W - L - R;
  const tail = arrivals.tail && finite(arrivals.tail.times_ms) && arrivals.tail.times_ms.length > 1
    ? arrivals.tail : null;
  const last = Math.max(...sides.flatMap(([key]) => arrivals[key].map((one) => one.ms)));
  const max = tail ? niceMax(Math.max(tail.times_ms.at(-1), last))
    : niceMax(Math.max(last * 1.3, 0.1));
  const fx = (t) => L + (pw * Math.min(Math.max(t, 0), max)) / max;
  // one dB scale for the lane, the ticks and the tail alike: 0 dB is 56 px, -60 dB (the floor of
  // the contract) none; a tick is never less than 2 px
  const height = (db) => 56 * (1 - Math.min(Math.max(-db, 0), 60) / 60);
  const tall = (db) => Math.max(height(db), 2);
  let s = '';
  for (const [key, label, base] of sides) {
    s += text(6, base - 30, label, '', 'fps-t fps-t-strong');
    s += `<path class="fps-axis" d="M${L} ${base}H${W - R}"/>`;
    if (tail && Array.isArray(tail[key]) && tail[key].length === tail.times_ms.length && finite(tail[key])) {
      let d = `M${num(fx(tail.times_ms[0]))} ${base}`;
      tail.times_ms.forEach((t, i) => { d += `L${num(fx(t))} ${num(base - height(tail[key][i]))}`; });
      s += `<path class="fps-tail" d="${d}L${num(fx(tail.times_ms.at(-1)))} ${base}Z"/>`;
    }
    arrivals[key].forEach((one, i) => {
      if (!Number.isFinite(one.ms) || !Number.isFinite(one.db)) return;
      const x = num(fx(one.ms));
      s += `<path class="${i < 2 ? 'fps-direct' : 'fps-reflect'}" d="M${x} ${base}V${num(base - tall(one.db))}"/>`;
    });
    const first = arrivals[key][0];
    const second = arrivals[key][1];
    s += text(fx(first.ms) + 8, base - 62, PICTURE_WORDS.near);
    if (second && second.ms > first.ms) {
      s += text(fx(second.ms) + 8, base - 50, PICTURE_WORDS.far(second.ms - first.ms));
    }
  }
  const ticks = [0, 1, 2, 3, 4].map((i) => (max * i) / 4);
  for (const t of ticks) {
    s += `<path class="fps-axis" d="M${num(fx(t))} 206v5"/>`
      + text(fx(t), 223, msText(t), ' text-anchor="middle"');
  }
  s += `<path class="fps-axis" d="M${L} 206H${W - R}"/>`
    + text(W - R, 236, PICTURE_WORDS.axisArrivals, ' text-anchor="end"');
  if (tail) s += text(W - R, 40, PICTURE_WORDS.reflections, ' text-anchor="end"', 'fps-t fps-t-accent');
  return `<svg class="fps-svg" viewBox="0 0 ${W} 240" role="img"${attr('aria-label', arrivalsLabel(arrivals))}>${s}</svg>`;
}

/** The arrivals as one sentence for a screen reader. */
function arrivalsLabel(arrivals) {
  const count = (key) => arrivals[key].length;
  return `Arrival times at the left and right ear on a millisecond axis: ${count('left')} at the left, ${count('right')} at the right`;
}

/** Directions: the two speakers' measured angles on a circle whose dotted outline is the measured
 * distance, the bar at each angle the level difference between the near and far ear (0 to 12 dB),
 * a dashed line from the bar to its dot. `''` with no angles. */
export function directionsSvg(angles, difference) {
  if (!angles) return '';
  const W = 330;
  const cx = 165;
  const cy = 132;
  const r0 = 30;
  const rd = 84;
  const at = (a, r) => [cx - r * Math.sin((a * Math.PI) / 180), cy - r * Math.cos((a * Math.PI) / 180)];
  const dist = Number.isFinite(angles.distance_m) ? `${fixedEven(angles.distance_m, 1)} m` : '';
  let s = `<circle class="fps-ring" cx="${cx}" cy="${cy}" r="${r0}"/>`
    + `<circle class="fps-reach" cx="${cx}" cy="${cy}" r="${rd}"/>`;
  const both = [['left', angles.left_deg, 'end'], ['right', angles.right_deg, 'start']];
  for (const [side, deg, anchor] of both) {
    const diff = difference ? difference[side] : null;
    const reach = r0 + (diff === null ? 0 : Math.min(Math.max(diff, 0), 12)) * 3 + 1.5;
    const [x1, y1] = at(deg, r0);
    const [x2, y2] = at(deg, reach);
    const [dx, dy] = at(deg, rd);
    s += diff === null ? '' : `<path class="fps-bar" d="M${num(x1)} ${num(y1)}L${num(x2)} ${num(y2)}"/>`;
    s += `<path class="fps-link" d="M${num(x2)} ${num(y2)}L${num(dx)} ${num(dy)}"/>`
      + `<circle class="fps-dot" cx="${num(dx)}" cy="${num(dy)}" r="5.5"/>`;
    const [lx, ly] = at(deg, 100);
    // the side is where the dot stands; the words are the angle and the ear difference
    const words = `${fixedEven(deg, 0)}°${diff === null ? '' : ` · ${diff >= 0 ? '+' : '−'}${fixedEven(Math.abs(diff), 0)} dB`}`;
    s += text(lx, ly - 2, words, ` text-anchor="${anchor}"`, 'fps-t fps-t-strong');
  }
  for (const [a, label] of [[0, '0°'], [90, '90°'], [180, '180°'], [270, '270°']]) {
    const [x, y] = at(a, 121);
    s += text(x, y + 3.5, label, ' text-anchor="middle"');
  }
  s += text(cx, cy + 3.5, '+', ' text-anchor="middle"', 'fps-t fps-t-strong');
  if (dist) s += text(cx, cy + rd + 14, `dotted circle: ${dist}`, ' text-anchor="middle"');
  s += text(cx, 271, difference ? 'bar: level difference between the ears, 0 to 12 dB' : 'the measured angles', ' text-anchor="middle"');
  const label = `The two measured source angles, left ${fixedEven(angles.left_deg, 0)} and right ${fixedEven(angles.right_deg, 0)} degrees${dist ? `, ${dist} away` : ''}`;
  return `<svg class="fps-svg" viewBox="0 0 ${W} 277" role="img"${attr('aria-label', label)}>${s}</svg>`;
}

/** Decay: the level of a low, a mid and a high band against time (log, 0.1 ms to the response's
 * end, 0 to -60 dB). `''` with no decay. */
export function decaySvg(decay, cut) {
  const t = decay?.times_ms;
  const bands = ['low', 'mid', 'high'];
  if (!finite(t) || t.length < 2 || !bands.every((b) => finite(decay[b]) && decay[b].length === t.length)) {
    return '';
  }
  const W = 330;
  const H = 240;
  const L = 40;
  const R = 8;
  const T = 10;
  const B = 48;
  const pw = W - L - R;
  const ph = H - T - B;
  const lo = t[0];
  const hi = t.at(-1);
  const fx = (ms) => L + (pw * Math.log10(ms / lo)) / Math.log10(hi / lo);
  const dy = (db) => T + (ph * -Math.min(Math.max(db, -60), 0)) / 60;
  let s = '';
  for (const d of [0, -20, -40, -60]) {
    s += `<path class="fps-grid" d="M${L} ${num(dy(d))}H${W - R}"/>`
      + text(L - 5, dy(d) + 3.5, d === 0 ? '0 dB' : String(d), ' text-anchor="end"');
  }
  for (const f of [0.1, 1, 10, 100, 1000].filter((one) => one >= lo * 0.999 && one <= hi)) {
    s += `<path class="fps-grid" d="M${num(fx(f))} ${T}V${T + ph}"/>`
      + text(fx(f), T + ph + 13, String(f), ' text-anchor="middle"');
  }
  bands.forEach((band) => {
    const d = t.map((ms, i) => `${i === 0 ? 'M' : 'L'}${num(fx(ms))} ${num(dy(decay[band][i]))}`).join('');
    s += `<path class="fps-band fps-band-${band}" d="${d}"/>`;
  });
  const end = Number.isFinite(decay.end_ms) ? decay.end_ms : hi;
  s += `<path class="fps-end" d="M${num(fx(Math.min(end, hi)))} ${T}V${T + ph}"/>`;
  const endWords = cut ? PICTURE_WORDS.cutTo(fixedEven(end / 1000, 1)) : PICTURE_WORDS.endsAt(end);
  s += text(fx(Math.min(end, hi)) - 4, T + 10, endWords, ' text-anchor="end"', 'fps-t fps-t-strong');
  s += text(L + pw / 2, T + ph + 27, PICTURE_WORDS.axisDecay, ' text-anchor="middle"');
  bands.forEach((band, i) => {
    const x = L + 10 + i * 58;
    s += `<path class="fps-band fps-band-${band}" d="M${x} ${H - 8}h16"/>${text(x + 20, H - 4.5, PICTURE_WORDS.bands[band])}`;
  });
  return `<svg class="fps-svg" viewBox="0 0 ${W} ${H}" role="img" aria-label="Level against time for a low, a mid and a high band of the chosen set">${s}</svg>`;
}

const RX0 = 34;
const RW = 420;
const RH = 190;
/** The response: the four paths' level by frequency (left speaker solid, right speaker lighter;
 * the ear it plays into on the near side in ink, the far in the accent). `''` with no curve. */
export function responseSvg(curve) {
  const { hz, ll_db: ll, lr_db: lr, rl_db: rl, rr_db: rr } = curve ?? {};
  if (!finite(hz) || hz.length < 2 || ![ll, lr, rl, rr].every((one) => finite(one) && one.length === hz.length)) {
    return '';
  }
  const T = 8;
  const B = 22;
  const pw = RW - RX0 - 8;
  const ph = RH - T - B;
  const lo = hz[0];
  const top = hz.at(-1);
  const fx = (f) => RX0 + (pw * Math.log(f / lo)) / Math.log(top / lo);
  const dy = (db) => T + (ph * (6 - Math.min(Math.max(db, -30), 6))) / 36;
  const line = (db) => hz.map((f, i) => `${i === 0 ? 'M' : 'L'}${num(fx(f))} ${num(dy(db[i]))}`).join('');
  let s = '';
  for (const d of [6, 0, -10, -20, -30]) {
    s += `<path class="fps-grid" d="M${RX0} ${num(dy(d))}H${RW - 8}"/>`
      + text(RX0 - 5, dy(d) + 3.5, String(d), ' text-anchor="end"');
  }
  for (const [f, label] of [[100, '100'], [1000, '1k'], [10000, '10k']].filter(([f]) => f > lo && f < top)) {
    s += `<path class="fps-grid" d="M${num(fx(f))} ${T}V${T + ph}"/>`
      + text(fx(f), RH - 7, label, ' text-anchor="middle"');
  }
  s += `<path class="fps-other fps-second" d="${line(rl)}"/><path class="fps-same fps-second" d="${line(rr)}"/>`
    + `<path class="fps-other" d="${line(lr)}"/><path class="fps-same" d="${line(ll)}"/>`;
  return `<svg class="fps-svg" viewBox="0 0 ${RW} ${RH}" role="img"${attr('aria-label', PICTURE_WORDS.response)}>${s}</svg>`;
}

/** The level in words: `about 7 dB quieter than Off`. */
export function levelWords(db) {
  if (!Number.isFinite(db)) return '';
  const whole = Math.round(Math.abs(db));
  if (whole === 0) return 'about the same level as Off';
  return `about ${whole} dB ${db < 0 ? 'quieter' : 'louder'} than Off`;
}

