/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What the mood draws beyond the sky and the floor: the wet road (the sky, the sun and the mountains
// mirrored in the floor as thin horizontal slices that sway), the haze that lies over the far floor
// and the foot of the mountains, and the embers. `geo` is `{ w, h, H, dpr }` in CSS pixels.

import { mixRgb, rgbOf } from './palette.js';

/** The wet road's slices, and how much of the way down the floor mirrors how much of the way up. */
export const WET = Object.freeze({ slices: 32, reach: 0.85 });

/** Mirror `source` (a canvas holding the scene above the horizon at the density `geo.dpr`) in the floor
 * of `g`, as thin slices that sway slowly together as one wave, the more as they are near; `wet` (0 to 1) is how strong, `t`
 * the clock in seconds. */
export function drawWet(g, source, geo, wet, t) {
  const { w, h, H, dpr } = geo;
  const depth = h - H;
  const sway = Math.sin(t * 1.6);
  for (let k = 0; k < WET.slices; k++) {
    const f0 = k / WET.slices;
    const dy = H + depth * f0;
    const dh = depth / WET.slices + 1;
    const sh = dh * WET.reach;
    const sy = H - (dy - H) * WET.reach - sh;
    if (sy < 0) break;
    const wob = sway * w * 0.004 * (1 + f0 * 4);
    g.globalAlpha = wet * (1 - f0 * 0.7);
    g.drawImage(source, 0, sy * dpr, source.width, Math.min(sh * dpr, source.height - sy * dpr), wob, dy, w, dh);
  }
  g.globalAlpha = 1;
}

/** The haze of thickness `haze` (0 to 1), tinted with the `mood`. */
export function drawHaze(g, geo, haze, mood) {
  const { w, h, H } = geo;
  const top = H - h * 0.09;
  const bottom = H + (h - H) * 0.45;
  const colour = mixRgb([120, 60, 150], [200, 90, 160], mood);
  const fog = g.createLinearGradient(0, top, 0, bottom);
  const at = (alpha) => `rgba(${colour[0]},${colour[1]},${colour[2]},${alpha})`;
  fog.addColorStop(0, at(0));
  fog.addColorStop(0.35, at(haze * 0.8));
  fog.addColorStop(0.55, at(haze * 0.55));
  fog.addColorStop(1, at(0));
  g.fillStyle = fog;
  g.fillRect(0, top, w, bottom - top);
}

/** The sparks' colours from new to dying, cut once. */
const SPARK = Array.from({ length: 16 }, (_, n) => rgbOf(mixRgb([255, 120, 60], [255, 235, 140], (n + 0.5) / 16)));

/** The sparks `list` (embers.js), added to what is under them. */
export function drawEmbers(g, list) {
  if (!list.length) return;
  g.save();
  g.globalCompositeOperation = 'lighter';
  for (const p of list) {
    g.globalAlpha = p.life * 0.85;
    g.fillStyle = SPARK[Math.min(15, Math.floor(p.life * 16))];
    g.beginPath();
    g.arc(p.x, p.y, p.size, 0, Math.PI * 2);
    g.fill();
  }
  g.restore();
}
