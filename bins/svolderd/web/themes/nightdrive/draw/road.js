/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The road and the mountains, on the canvas over the sky: the plain grid that runs on to both
// edges, the floor (a row of the music at each step, bass left, highs right, rolling toward the
// viewer: a grid whose crossings the music lifts and whose cells it heats) and the ridge the live
// spectrum draws along the horizon. `geo` is `{ w, h, H, dpr }` in CSS pixels; `frame` is
// `{ dull, look }`.

import { BANDS } from '../../../core/viz-model.js';
import { COLUMNS } from '../axis.js';
import { CAMERA } from '../camera.js';
import { PALETTE, cellColour, unitOf } from './palette.js';
import { heatColours, heatIndex } from './heat.js';
import { NEUTRAL } from '../look.js';
import { RIDGE, ridgeHeights, smoothUnits } from '../ridge.js';

/** The level of a cell in a row nobody has born yet. */
const QUIET = 0.05;

/** The floor's heat grid. `height`: the lift of a crossing of level 1 at the nearest row, as a share of
 * the scene's height (the farther rows scale as the row spacing does). `heatFloor`: the least peak the
 * heat is read against, so near silence is not stretched to the hot end; a cell's heat is its level over
 * the floor's own recent peak (floor.js), or over this where the peak is lower, and the colour and the
 * alpha follow that to the power `heatCurve`, which spreads the low levels over more colours. `lumaStep`:
 * the least difference of luminance (0 to 255) between the colours of two levels a listener should be
 * able to tell apart (the checks' yardstick). `extra`: how many rows the floor keeps
 * beyond the bottom of the scene, so a lifted near edge still has strips to the bottom. `waveSpeed`,
 * `waveWidth`, `waveAlpha`: the wave a moment sends down the floor, in rows a second, rows and the strongest glow. */
export const FLOOR = Object.freeze({
  height: 0.25, heatFloor: 0.15, heatCurve: 0.6, lumaStep: 20, extra: 2, waveSpeed: 20, waveWidth: 1.6, waveAlpha: 0.7,
});

/** The level (0 to 1) the heat colour and alpha are read at: the cell's level `v` over the floor's `peak` (not
 * under `FLOOR.heatFloor`), raised to `FLOOR.heatCurve`. */
export const heatLevel = (v, peak) => Math.min(1, Math.max(0, v / Math.max(FLOOR.heatFloor, peak))) ** FLOOR.heatCurve;

/** The row edges the floor is drawn on: those of the scene's rows, the last at the bottom when the floor is
 * at rest, and one past it. */
const EDGES = CAMERA.rows + FLOOR.extra;

/** The projected corners of the floor's grid, worked out once a frame: the screen's x of each
 * column edge at each row edge, and the screen's y of each row edge. */
const GX = new Float32Array(EDGES * (COLUMNS + 1));
const GY = new Float32Array(EDGES);

/** The depth divisor of the row edge at `z`, as `project` has it. */
const depth = (z) => 1 + (1 - z) * CAMERA.K;

/** Work out `GX` and `GY` for the floor as it has rolled. */
function corners(geo, frac) {
  const { w, h, H } = geo;
  const { half, rows, spread } = CAMERA;
  for (let k = 0; k < EDGES; k++) {
    const d = depth((k + frac) / rows);
    GY[k] = H + (h - H) / d;
    for (let i = 0; i <= COLUMNS; i++) {
      GX[k * (COLUMNS + 1) + i] = w / 2 + ((-half + (2 * half * i) / COLUMNS) * w * spread) / d;
    }
  }
}

/** The faint grid beside the spectrum's columns, out to both edges. */
function drawSide(g, geo, frac, dull) {
  const { w, h, H } = geo;
  const { half, side, sideStep, rows, spread } = CAMERA;
  const far = depth(0);
  const yFar = H + (h - H) / far;
  g.strokeStyle = dull ? PALETTE.dullGrid : PALETTE.grid;
  g.lineWidth = unitOf(w);
  for (let x = -side; x <= side; x += sideStep) {
    if (Math.abs(x) < half) continue;
    g.beginPath();
    g.moveTo(w / 2 + (x * w * spread) / far, yFar);
    g.lineTo(w / 2 + x * w * spread, h);
    g.stroke();
  }
  for (let k = 0; k <= rows; k++) {
    const d = depth(Math.min(1, (k + frac) / rows));
    const y = H + (h - H) / d;
    const k0 = (w * spread) / d;
    g.beginPath();
    g.moveTo(w / 2 - side * k0, y);
    g.lineTo(w / 2 - half * k0, y);
    g.moveTo(w / 2 + half * k0, y);
    g.lineTo(w / 2 + side * k0, y);
    g.stroke();
  }
}

/** The wave's glow (0 to 1) at row `row` (a row's middle, the horizon 0) `since` seconds after a
 * moment, of the strength `wave`. */
export const waveAt = (row, since, wave) => (wave > 0.01
  ? FLOOR.waveAlpha * wave * Math.exp(-(((row - since * FLOOR.waveSpeed) / FLOOR.waveWidth) ** 2))
  : 0);

/** The level of the crossings of each row edge, and their y with the lift: worked out once a frame. */
const LV = new Float32Array(EDGES * (COLUMNS + 1));
const GL = new Float32Array(EDGES * (COLUMNS + 1));

/** The grid's corners as the last `drawRoad` left them (`x`, `y` lifted, `v`, row edge by row edge,
 * `stride` to a row edge), for where the embers rise from. */
export const crossings = { x: GX, y: GL, v: LV, rows: EDGES - 1, stride: COLUMNS + 1 };

/** The step an alpha is rounded to where cells are filled together. */
const ALPHA_STEP = 0.02;

/** The grid lines' colours along the road, from the first column to the last. */
const STOPS = Array.from({ length: 7 }, (_, n) => cellColour(n / 6));

/** Work out `LV` and `GL`: a crossing's level is the mean of the two columns it lies between (half of
 * the one at the road's edge), and it is lifted by that level as high as `FLOOR.height` says. */
function lifts(geo, floor) {
  const { rows, K } = CAMERA;
  const span = ((geo.h - geo.H) * K) / rows;
  const stride = COLUMNS + 1;
  for (let k = 0; k < EDGES; k++) {
    const row = floor.row(k);
    const scale = (FLOOR.height * geo.h) / depth((k + floor.frac) / rows);
    // an edge below the bottom of the scene gives up its lift over one row spacing (at the bottom), so the
    // floor reaches the bottom whatever the level and no edge jumps as the rows roll
    const fade = GY[k] <= geo.h ? 1 : Math.max(0, 1 - (GY[k] - geo.h) / span);
    for (let i = 0; i <= COLUMNS; i++) {
      const left = i > 0 ? (row ? row[i - 1] : QUIET) : 0;
      const right = i < COLUMNS ? (row ? row[i] : QUIET) : 0;
      const v = (left + right) / 2;
      LV[k * stride + i] = v;
      GL[k * stride + i] = GY[k] - v * scale * fade;
    }
  }
}

/** Draw the plain grid and the floor of `floor`: a grid whose crossings are lifted by the level of
 * their row, the far strips first so the near ones cover them, each strip filled (the ground, then each
 * cell's heat) and then its lines. `frame.look` is the mood's look (look.js). */
export function drawRoad(g, geo, floor, frame) {
  const { w } = geo;
  const { rows } = CAMERA;
  const look = frame.look ?? NEUTRAL;
  const dull = Boolean(frame.dull);
  const dim = dull ? 0.5 : 1;
  const lines = (0.55 + 0.45 * look.mood) * dim;
  const heat = (0.45 + 0.55 * look.mood) * dim;
  const ground = dull ? 1 : 1 - 0.85 * look.wet;
  const colours = heatColours(look.x);
  const { peak } = floor;
  const stride = COLUMNS + 1;
  const line = Math.max(1, unitOf(w) * 1.1);
  corners(geo, floor.frac);
  lifts(geo, floor);
  g.globalAlpha = lines;
  drawSide(g, geo, floor.frac, dull);
  g.lineJoin = 'round';
  for (let k = 0; k < EDGES - 1; k++) {
    const a = k * stride;
    const b = (k + 1) * stride;
    const glow = waveAt(k + 0.5, look.since, look.wave);
    if (ground > 0.01) {
      g.beginPath();
      g.moveTo(GX[a], GL[a]);
      for (let i = 1; i <= COLUMNS; i++) g.lineTo(GX[a + i], GL[a + i]);
      for (let i = COLUMNS; i >= 0; i--) g.lineTo(GX[b + i], GL[b + i]);
      g.closePath();
      g.globalAlpha = ground;
      g.fillStyle = PALETTE.ground;
      g.fill();
    }
    // neighbouring cells of one colour and nearly one alpha are filled as one path: fewer fills, and no seam
    let was = -1;
    let level = -1;
    for (let i = 0; i < COLUMNS; i++) {
      const v = (LV[a + i] + LV[a + i + 1] + LV[b + i] + LV[b + i + 1]) / 4;
      const lit = heatLevel(v, peak);
      const at = dull ? 0 : heatIndex(Math.min(1, lit + glow * 0.5));
      const alpha = Math.round((heat * (0.15 + 0.75 * Math.min(1, lit * 1.4 + glow))) / ALPHA_STEP);
      if (at !== was || alpha !== level) {
        if (was >= 0) g.fill();
        g.fillStyle = dull ? PALETTE.dullCell : colours[at];
        g.globalAlpha = alpha * ALPHA_STEP;
        g.beginPath();
        was = at;
        level = alpha;
      }
      g.moveTo(GX[a + i], GL[a + i]);
      g.lineTo(GX[a + i + 1], GL[a + i + 1]);
      g.lineTo(GX[b + i + 1], GL[b + i + 1]);
      g.lineTo(GX[b + i], GL[b + i]);
      g.closePath();
    }
    g.fill();
    const near = Math.min(1, (k + 1 + floor.frac) / rows);
    const alpha = Math.min(1, 0.3 + 0.6 * near + glow) * lines;
    if (dull) g.strokeStyle = PALETTE.dullLine;
    else {
      const left = Math.min(GX[a], GX[b]);
      const spread = Math.max(1, Math.max(GX[a + COLUMNS], GX[b + COLUMNS]) - left);
      const across = g.createLinearGradient(left, 0, left + spread, 0);
      for (let n = 0; n < STOPS.length; n++) across.addColorStop(n / (STOPS.length - 1), STOPS[n]);
      g.strokeStyle = across;
    }
    g.lineWidth = line;
    g.globalAlpha = alpha;
    g.beginPath();
    g.moveTo(GX[b], GL[b]);
    for (let i = 1; i <= COLUMNS; i++) g.lineTo(GX[b + i], GL[b + i]);
    g.stroke();
    g.globalAlpha = alpha * 0.85;
    g.beginPath();
    for (let i = 0; i <= COLUMNS; i++) {
      g.moveTo(GX[a + i], GL[a + i]);
      g.lineTo(GX[b + i], GL[b + i]);
    }
    g.stroke();
    if (glow > 0.05) {
      // the wave's front: the strip's lines go pale and thick
      g.strokeStyle = PALETTE.wave;
      g.lineWidth = line * (1 + glow * 2.5);
      g.globalAlpha = Math.min(1, glow);
      g.beginPath();
      g.moveTo(GX[b], GL[b]);
      for (let i = 1; i <= COLUMNS; i++) g.lineTo(GX[b + i], GL[b + i]);
      g.stroke();
    }
  }
  g.globalAlpha = 1;
}

/** The ridge's levels smoothed across frequency, and its heights at each sample, worked out once a
 * frame. */
const SOFT = new Float32Array(BANDS);
let HEIGHTS = new Float32Array(512);

/** The mountains of a page with no music analysis: they take the tone curve's shape and breathe, `rate` rad/s and
 * `amp` of the scene's height. */
export const NOCAP = Object.freeze({ rate: 0.8, amp: 0.006 });

/** Draw the ridge of `ridge` (the ridge unit of each of the analysis's bands, 0 to 1): the levels
 * smoothed across frequency, a smooth curve through the heights every 5 px along the axis, filled
 * and edged in cyan, with the slow swell (`frame.swell`, its phase in wavelengths) under it while
 * the music is low; a flat low line when `frame.flat`. While the scene is tuned (`frame.tune`,
 * `{ m, curve }`) the edge blends from the music to the tone curve by `m`: `curve(x)` is the
 * curve's y at x. */
export function drawRidge(g, geo, ridge, frame) {
  const { w, h, H } = geo;
  const dull = Boolean(frame.dull);
  const px = unitOf(w);
  const tune = frame.tune?.curve && !frame.flat ? frame.tune : null;
  const nocap = frame.nocap ?? null;
  const breath = nocap ? Math.sin(frame.t * NOCAP.rate) * h * NOCAP.amp * nocap.breath : 0;
  smoothUnits(ridge, SOFT);
  const need = Math.ceil(w / RIDGE.step) + 1;
  if (HEIGHTS.length < need) HEIGHTS = new Float32Array(need);
  const n = ridgeHeights({ soft: SOFT, w, h, phase: frame.swell ?? 0 }, HEIGHTS);
  const edge = (i) => {
    const x = Math.min(w, i * RIDGE.step);
    if (frame.flat) return H - h * 0.03 - Math.sin((x / w) * 9) * h * 0.008;
    if (nocap) return (nocap.curve?.(x) ?? H - h * 0.08) + breath;
    const music = H - HEIGHTS[i];
    return tune ? music * (1 - tune.m) + tune.curve(x) * tune.m : music;
  };
  g.beginPath();
  g.moveTo(0, H);
  g.lineTo(0, edge(0));
  // quadratic segments through the midpoints of the samples: no straight run between them
  for (let i = 1; i < n; i++) {
    const x = Math.min(w, i * RIDGE.step);
    const xPrev = Math.min(w, (i - 1) * RIDGE.step);
    const y0 = edge(i - 1);
    const y1 = edge(i);
    g.quadraticCurveTo(xPrev, y0, (xPrev + x) / 2, (y0 + y1) / 2);
  }
  g.lineTo(w, edge(n - 1));
  g.lineTo(w, H);
  g.closePath();
  g.fillStyle = dull ? PALETTE.dullRidgeFill : PALETTE.ridgeFill;
  g.fill();
  g.strokeStyle = dull ? PALETTE.dullRidgeEdge : PALETTE.ridgeEdge;
  if (!dull && geo.glow !== false) {
    // the glow is the edge drawn wider and fainter under itself, more of it while the mountains are tuned
    const spread = 8 * px * Math.max(tune?.m ?? 0, 0.35);
    g.save();
    g.globalAlpha = 0.1;
    g.lineWidth = 2 * px + spread * 1.6;
    g.stroke();
    g.globalAlpha = 0.2;
    g.lineWidth = 2 * px + spread * 0.8;
    g.stroke();
    g.restore();
  }
  g.lineWidth = 2 * px;
  g.stroke();
  if (tune && !dull) highlight(g, geo, tune, edge, n);
}

/** How far the highlight fades out at each end, CSS px. */
const FADE_PX = 12;

/** The curve itself in pink between the lit band's two diamonds (`tune.bell.diamonds`), fading out over
 * `FADE_PX` at each end; the rest of the edge keeps its colour. `edge(i)` is the edge's y at sample
 * `i`, of `n`. */
function highlight(g, geo, tune, edge, n) {
  const diamonds = tune.bell?.diamonds;
  if (!diamonds || diamonds.length < 2 || tune.m <= 0.02) return;
  const { w } = geo;
  const lo = Math.max(0, Math.min(diamonds[0].x, diamonds[1].x));
  const hi = Math.min(w, Math.max(diamonds[0].x, diamonds[1].x));
  if (hi - lo < 1) return;
  const fade = Math.min(FADE_PX / (hi - lo), 0.5);
  const fill = g.createLinearGradient(lo, 0, hi, 0);
  fill.addColorStop(0, PALETTE.highlightClear);
  fill.addColorStop(fade, PALETTE.highlight);
  fill.addColorStop(1 - fade, PALETTE.highlight);
  fill.addColorStop(1, PALETTE.highlightClear);
  g.save();
  g.globalAlpha = Math.min(1, tune.m * 1.4);
  g.beginPath();
  const first = Math.max(0, Math.floor(lo / RIDGE.step) - 1);
  const last = Math.min(n - 1, Math.ceil(hi / RIDGE.step) + 1);
  g.moveTo(first * RIDGE.step, edge(first));
  for (let i = first + 1; i <= last; i++) g.lineTo(Math.min(w, i * RIDGE.step), edge(i));
  g.strokeStyle = fill;
  const line = 2.6 * unitOf(w);
  if (geo.glow !== false) {
    const alpha = g.globalAlpha;
    g.globalAlpha = alpha * 0.2;
    g.lineWidth = line + 9 * unitOf(w);
    g.stroke();
    g.globalAlpha = alpha;
  }
  g.lineWidth = line;
  g.stroke();
  g.restore();
}
