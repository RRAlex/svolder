/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The colours of the scene's canvases, each literal in one place. A dull scene (the device out of
// reach, the stream lost) wears the greys.

export const PALETTE = Object.freeze({
  night: '#0a0018',
  dusk: '#5b0f5e',
  dullNight: '#0c0a12',
  dullHorizon: '#2c2834',
  star: '255, 236, 255',
  skyDim: Object.freeze([[5, 2, 26], [26, 7, 54], [62, 17, 80]]),
  skyWarm: Object.freeze([[20, 4, 41], [76, 17, 89], [180, 48, 106]]),
  sunDim: Object.freeze([[205, 160, 90], [190, 90, 70], [150, 40, 80]]),
  sunWarm: Object.freeze([[255, 213, 74], [255, 138, 61], [255, 45, 120]]),
  sunFlare: 'rgba(255,170,90,',
  sunFlareClear: 'rgba(255,120,90,0)',
  sunHalo: Object.freeze([[0, 'rgba(255,60,140,.55)'], [0.3, 'rgba(255,60,140,.3)'], [0.65, 'rgba(255,60,140,.09)'], [1, 'rgba(255,60,140,0)']]),
  sunStripe: '#5b0f5e',
  sunStripeWarm: Object.freeze([74, 17, 80]),
  dullSun: Object.freeze(['#8a8592', '#4a4652']),
  dullSunHalo: Object.freeze([[0, 'rgba(160,150,170,.2)'], [0.4, 'rgba(160,150,170,.08)'], [1, 'rgba(160,150,170,0)']]),
  dullSunStripe: '#2c2834',
  rayNear: 'rgba(255,120,200,.22)',
  rayFar: 'rgba(255,60,200,0)',
  starTail: 'rgba(255,120,230,0)',
  ridgeFill: '#14002a',
  ridgeEdge: '#22d3ee',
  veil: 'rgba(5, 3, 10, .55)',
  dullRidgeFill: '#15131b',
  dullRidgeEdge: 'rgba(150,145,160,.6)',
  grid: 'rgba(255,60,200,.22)',
  dullGrid: 'rgba(150,145,160,.12)',
  dullCell: 'hsl(265, 10%, 58%)',
  dullLine: 'rgb(150, 145, 160)',
  ground: 'rgb(10, 3, 22)',
  wave: 'rgb(255, 230, 250)',
  bubble: '#0a0018',
  bubbleHeld: '#22d3ee',
  bubbleRing: '#b5f7ff',
  bubbleGlow: '#22d3ee',
  highlight: '#ff9ae6',
  highlightClear: 'rgba(255,154,230,0)',
  diamond: '#1a0030',
  diamondHeld: '#ff3cc8',
  diamondEdge: '#ff9ae6',
  diamondGlow: '#ff3cc8',
});

/** The `[r, g, b]` `f` of the way from `a` to `b`, and as a colour string. */
export const mixRgb = (a, b, f) => a.map((v, i) => Math.round(v + (b[i] - v) * f));
export const rgbOf = (c) => `rgb(${c[0]},${c[1]},${c[2]})`;

/** The colour of the column `f` (0 for the first, 1 for the last): violet for the bass to orange
 * for the highs. */
export const cellColour = (f) => `hsl(${300 - 120 * f}, 100%, ${55 + 10 * f}%)`;

/** A line's width scales with the scene's width, up from a hairline on a phone. */
export const unitOf = (w) => Math.min(1.4, Math.max(0.6, w / 1176));

/** The sun's dark stripes, as shares of the scene's height: they widen downward, five of them, and the
 * lowest ends `foot` above the horizon so the sun's foot meets the horizon solid, whatever dips there. */
export const SUN_STRIPES = Object.freeze({ count: 5, foot: 0.05, step: 0.035, thin: 0.004 });

/** The thumbnail's stripes, in the thumbnail's own units (it is 160 by 104 and the menu draws it 32 px wide):
 * two, each 7.5 units, which is 1.5 px at the menu's size, `gap` apart, the lowest `foot` above the horizon. */
export const THUMB_STRIPES = Object.freeze({ count: 2, thick: 7.5, gap: 4, foot: 5.2 });

/** The thumbnail's stripes as `[offset above the horizon, thickness]`, top one first. */
export const thumbStripes = () => Array.from({ length: THUMB_STRIPES.count }, (_, k) => [
  THUMB_STRIPES.foot + (THUMB_STRIPES.count - 1 - k) * (THUMB_STRIPES.thick + THUMB_STRIPES.gap),
  THUMB_STRIPES.thick,
]);

/** The stripes as `[offset above the horizon, thickness]` in px for a scene `h` tall, top one first. */
export const stripesOf = (h) => Array.from({ length: SUN_STRIPES.count }, (_, k) => [
  (SUN_STRIPES.foot + SUN_STRIPES.step * (SUN_STRIPES.count - 1 - k)) * h,
  SUN_STRIPES.thin * (k + 1) * h,
]);
