/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's two colours. From a cover: a 32x32 downsample's two dominant hues, found by a k-means
// of 2 over the pixels' hue (each weighted by how saturated and how far from black or white it
// is), lifted into the lightness and saturation range of the scene's own eight pairs. Without a
// cover, or from one that is grey: one of the eight pairs, chosen by the track's title and artist.
// Pure: no canvas (the page hands in the downsample's RGBA bytes), no randomness.
//
//   palette(pixels, { title, artist })   two hex colours `['#rrggbb', '#rrggbb']`
//   fromCover(pixels)                    the pair from a cover, or null for a grey or empty one
//   seeded(title, artist)                the pair the track's text picks
//   PALETTES                             the scene's eight pairs

import { hashStr } from './rng.js';

/** The scene's eight pairs. */
export const PALETTES = Object.freeze([
  ['#7fe0ff', '#ff7a4d'], ['#a58bff', '#d8ff6a'], ['#5ff0c8', '#ff5fa8'], ['#9db8ff', '#ffc35c'],
  ['#ff6464', '#62ffe9'], ['#c6f4ff', '#7a5cff'], ['#ffd1e8', '#3fd0ff'], ['#b6ff8a', '#ff7ae0'],
].map((pair) => Object.freeze(pair)));

/** The lightness and saturation the pairs span (HSL, 0 to 1): a cover's colours are lifted into it. */
export const LIGHTNESS = Object.freeze([0.62, 0.91]);
export const SATURATION_MIN = 0.83;

/** A cover whose mean pixel weight is under this is grey (no colour to take). */
const GREY_BELOW = 0.04;

/** A second hue closer than this (a share of the circle), or a cluster lighter than this share of
 * the weight, makes the pair a hue and its complement. */
const SAME_HUE = 0.06;
const MIN_SHARE = 0.08;

const TAU = Math.PI * 2;
const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);

/** RGB bytes to [hue 0-1, saturation, lightness] (HSL). */
function hsl(r, g, b) {
  const R = r / 255;
  const G = g / 255;
  const B = b / 255;
  const max = Math.max(R, G, B);
  const min = Math.min(R, G, B);
  const l = (max + min) / 2;
  const d = max - min;
  if (d === 0) return [0, 0, l];
  const s = d / (1 - Math.abs(2 * l - 1));
  let h;
  if (max === R) h = ((G - B) / d) % 6;
  else if (max === G) h = (B - R) / d + 2;
  else h = (R - G) / d + 4;
  return [(h / 6 + 1) % 1, s, l];
}

/** An HSL colour (all 0 to 1) as `#rrggbb`. */
export function hex(h, s, l) {
  const a = s * Math.min(l, 1 - l);
  const f = (n) => {
    const k = (n + h * 12) % 12;
    const v = l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(clamp(v) * 255).toString(16).padStart(2, '0');
  };
  return `#${f(0)}${f(8)}${f(4)}`;
}

/** A hue and its saturation and lightness, lifted into the scene's range, as hex. */
const lifted = (h, s, l) => hex(((h % 1) + 1) % 1, clamp(s, SATURATION_MIN, 1), clamp(l, LIGHTNESS[0], LIGHTNESS[1]));

/** The pair `title` and `artist` pick: `Math.floor(seed * 7.31) % 8` of a seed of 0 to 100. */
export function seeded(title, artist) {
  const seed = (hashStr(`${title ?? ''}\u0000${artist ?? ''}`) % 10000) / 100;
  return [...PALETTES[Math.floor(seed * 7.31) % PALETTES.length]];
}

/** The two dominant colours of `pixels` (RGBA bytes, 32x32 or any size), or null when it is grey. */
export function fromCover(pixels) {
  if (!pixels || pixels.length < 4) return null;
  const points = [];
  let total = 0;
  const n = Math.floor(pixels.length / 4);
  for (let i = 0; i < n; i++) {
    if (pixels[i * 4 + 3] < 128) continue;
    const [h, s, l] = hsl(pixels[i * 4], pixels[i * 4 + 1], pixels[i * 4 + 2]);
    const w = s * (1 - Math.abs(2 * l - 1));
    if (w <= 0) continue;
    points.push({ x: Math.cos(h * TAU), y: Math.sin(h * TAU), w, s, l });
    total += w;
  }
  if (points.length === 0 || total / n < GREY_BELOW) return null;

  // k = 2: start from the heaviest point and the point farthest from it (weight times distance),
  // so the result does not depend on a random start
  const a = points.reduce((best, p) => (p.w > best.w ? p : best), points[0]);
  const b = points.reduce((best, p) => {
    const far = (q) => q.w * ((q.x - a.x) ** 2 + (q.y - a.y) ** 2);
    return far(p) > far(best) ? p : best;
  }, points[0]);
  let ca = { x: a.x, y: a.y };
  let cb = { x: b.x, y: b.y };
  let groups = [[], []];
  for (let round = 0; round < 12; round++) {
    groups = [[], []];
    for (const p of points) {
      const da = (p.x - ca.x) ** 2 + (p.y - ca.y) ** 2;
      const db = (p.x - cb.x) ** 2 + (p.y - cb.y) ** 2;
      groups[da <= db ? 0 : 1].push(p);
    }
    const mean = (g, old) => {
      const w = g.reduce((s, p) => s + p.w, 0);
      return w > 0 ? { x: g.reduce((s, p) => s + p.x * p.w, 0) / w, y: g.reduce((s, p) => s + p.y * p.w, 0) / w } : old;
    };
    ca = mean(groups[0], ca);
    cb = mean(groups[1], cb);
  }
  const summary = (g, c) => {
    const w = g.reduce((s, p) => s + p.w, 0);
    return {
      w,
      h: (Math.atan2(c.y, c.x) / TAU + 1) % 1,
      s: w > 0 ? g.reduce((x, p) => x + p.s * p.w, 0) / w : 1,
      l: w > 0 ? g.reduce((x, p) => x + p.l * p.w, 0) / w : 0.7,
    };
  };
  const [first, second] = [summary(groups[0], ca), summary(groups[1], cb)].sort((p, q) => q.w - p.w);
  const gap = Math.abs(((second.h - first.h + 1.5) % 1) - 0.5);
  if (second.w / total < MIN_SHARE || gap < SAME_HUE) {
    return [lifted(first.h, first.s, first.l), lifted(first.h + 0.5, first.s, first.l)];
  }
  return [lifted(first.h, first.s, first.l), lifted(second.h, second.s, second.l)];
}

/** The pair for a track: its cover's when it has colour, else the one its title and artist pick. */
export function palette(pixels, { title = '', artist = '' } = {}) {
  return fromCover(pixels) ?? seeded(title, artist);
}
