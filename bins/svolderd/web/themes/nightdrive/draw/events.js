/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The sky's rare events on the sky canvas: wedges of light from the sun, turning, and shooting stars.
// What shows, and when, is the scheduler's (sky.js); this only draws it. `geo` is `{ w, h, H }` in
// CSS pixels, `sun` the sun's `{ cx, cy, r }`; `frame` is `{ t, bass, rays, stars }`.

import { SKY } from '../sky.js';
import { PALETTE, unitOf } from './palette.js';

/** How many wedges the rays have: the circle round the sun, nine of them over the sky above the horizon at rest. */
const WEDGES = 18;

/** Draw the rays (`rays` is their alpha, 0 for none) over the sky above the horizon. */
export function drawRays(g, geo, sun, frame) {
  if (!(frame.rays > 0.001)) return;
  const { w, H } = geo;
  const { cx, cy, r } = sun;
  g.save();
  g.globalAlpha = frame.rays;
  g.beginPath();
  g.rect(0, 0, w, H);
  g.clip();
  g.globalCompositeOperation = 'lighter';
  for (let k = 0; k < WEDGES; k++) {
    const ang = -Math.PI + (k + 0.5) * (2 * Math.PI / WEDGES) + (frame.raysAngle ?? 0) + Math.sin(frame.t * 0.07 + k) * 0.03;
    // a wedge pointing down into the ground is behind the horizon
    if (Math.sin(ang) > 0.2) continue;
    const len = r * (2.4 + frame.bass * 1.6 + (k % 3) * 0.4);
    const half = 0.05 + (k % 2) * 0.03;
    const fade = g.createRadialGradient(cx, cy, r * 0.6, cx, cy, len);
    fade.addColorStop(0, PALETTE.rayNear);
    fade.addColorStop(1, PALETTE.rayFar);
    g.fillStyle = fade;
    g.beginPath();
    g.moveTo(cx, cy);
    g.lineTo(cx + Math.cos(ang - half) * len, cy + Math.sin(ang - half) * len);
    g.lineTo(cx + Math.cos(ang + half) * len, cy + Math.sin(ang + half) * len);
    g.closePath();
    g.fill();
  }
  g.restore();
}

/** Draw the shooting stars `frame.stars` (the scheduler's, `t0` in the frame's seconds). */
export function drawStars(g, geo, frame) {
  const { w, h, H } = geo;
  const px = unitOf(w);
  for (const star of frame.stars) {
    const p = (frame.t - star.t0) / SKY.starLife;
    if (p < 0 || p >= 1) continue;
    const dx = star.dx * w;
    const x = star.x * w + dx * p;
    const y = star.y * H + h * 0.12 * p;
    const tail = g.createLinearGradient(x, y, x - dx * 0.18, y - h * 0.03);
    tail.addColorStop(0, `rgba(255,255,255,${1 - p})`);
    tail.addColorStop(1, PALETTE.starTail);
    g.strokeStyle = tail;
    g.lineWidth = 2 * px;
    g.beginPath();
    g.moveTo(x, y);
    g.lineTo(x - dx * 0.18, y - h * 0.03);
    g.stroke();
  }
}
