/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The bubbles of the tone control on the mountains: one ring per band on the curve, larger and lit
// while it is held; and a diamond at each end of the stretch of the curve the lit band controls, on
// the curve (the pink on the curve itself is drawn with the ridge, draw/road.js). `geo` is
// `{ w, h, H, dpr }` in CSS pixels; `tune` is `{ m, bubbles, held, bell }` (the morph, the bubbles in
// CSS px, the band held or null, the lit band `{ diamonds, held }` or null).

import { PALETTE } from './palette.js';

/** The radius of a bubble and of a held one, CSS px. */
export const RADIUS = 7;
export const RADIUS_HELD = 9;

/** The morph above which the bubbles are drawn. */
const SHOWN = 0.02;

/** Draw the bubbles of `tune`, fading in with the morph; nothing while it is barely begun. */
export function drawBubbles(g, geo, tune) {
  if (!tune || tune.m <= SHOWN) return;
  g.save();
  g.globalAlpha = Math.min(1, tune.m * 1.4);
  const alpha = g.globalAlpha;
  for (const b of tune.bubbles) {
    const on = tune.held === b.band;
    g.beginPath();
    g.arc(b.x, b.y, on ? RADIUS_HELD : RADIUS, 0, Math.PI * 2);
    if (geo.glow !== false) {
      // the glow is the ring drawn wider and fainter under itself
      g.strokeStyle = PALETTE.bubbleGlow;
      g.globalAlpha = alpha * 0.15;
      g.lineWidth = 12;
      g.stroke();
      g.globalAlpha = alpha * 0.3;
      g.lineWidth = 6;
      g.stroke();
      g.globalAlpha = alpha;
    }
    g.lineWidth = 2;
    g.strokeStyle = PALETTE.bubbleRing;
    g.fillStyle = on ? PALETTE.bubbleHeld : PALETTE.bubble;
    g.fill();
    g.stroke();
  }
  g.restore();
}

/** The half side of a diamond, CSS px. */
export const DIAMOND = 5;

/** Draw the diamonds of the lit band of `tune`, the one held filled. */
export function drawDiamonds(g, geo, tune) {
  const bell = tune?.bell;
  if (!bell || tune.m <= SHOWN || bell.diamonds.length === 0) return;
  g.save();
  g.globalAlpha = Math.min(1, tune.m * 1.4);
  const alpha = g.globalAlpha;
  for (const d of bell.diamonds) {
    g.save();
    g.translate(d.x, d.y);
    g.rotate(Math.PI / 4);
    g.fillStyle = bell.held === d.side ? PALETTE.diamondHeld : PALETTE.diamond;
    g.strokeStyle = PALETTE.diamondEdge;
    if (geo.glow !== false) {
      g.strokeStyle = PALETTE.diamondGlow;
      g.globalAlpha = alpha * 0.2;
      g.lineWidth = 10;
      g.strokeRect(-DIAMOND, -DIAMOND, 2 * DIAMOND, 2 * DIAMOND);
      g.globalAlpha = alpha * 0.35;
      g.lineWidth = 5;
      g.strokeRect(-DIAMOND, -DIAMOND, 2 * DIAMOND, 2 * DIAMOND);
      g.globalAlpha = alpha;
      g.strokeStyle = PALETTE.diamondEdge;
    }
    g.lineWidth = 1.6;
    g.fillRect(-DIAMOND, -DIAMOND, 2 * DIAMOND, 2 * DIAMOND);
    g.strokeRect(-DIAMOND, -DIAMOND, 2 * DIAMOND, 2 * DIAMOND);
    g.restore();
  }
  g.restore();
}
