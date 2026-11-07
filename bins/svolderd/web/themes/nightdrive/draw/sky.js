/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The sky: the dusk's gradient, the stars that twinkle faintly high up and the striped sun that
// pulses on each kick, drawn down to the horizon with its foot solid (the mountains and the floor
// hide it). It is drawn on the canvas under the road and the ridge, and it paints the whole canvas,
// the ground below the horizon too. `geo` is `{ w, h, H, dpr }` in CSS pixels; `frame` is
// `{ t, kick, dull, bass, rays, stars }`: seconds, the kick's 0 to 1, whether the scene is dull,
// the bass and the rare events' state (the scheduler's, see sky.js), and `look` (look.js): the mood
// that deepens or warms the sky, the sun and the stars, and the flare of a moment.

import { drawRays, drawStars } from './events.js';
import { LOOK, NEUTRAL } from '../look.js';
import { PALETTE, mixRgb, rgbOf, stripesOf, unitOf } from './palette.js';

/** How many stars there are. */
export const STARS = 60;

/** How far the sun's glow reaches beyond its edge, CSS px at a scene 1176 wide. */
const HALO = 40;

/** The colours the mood gives the sky and the sun, made again only when the mood or the flare has moved by a
 * thousandth or a hundredth. */
let hued = { key: NaN };
function huesOf(mood, flare) {
  const key = Math.round(mood * 1000) * 1000 + Math.round(flare * 100);
  if (hued.key !== key) {
    const horizon = mixRgb(PALETTE.skyDim[2], PALETTE.skyWarm[2], Math.min(1, mood + flare * 0.5));
    const warmth = Math.min(1, mood / LOOK.neutral + flare);
    hued = {
      key,
      top: rgbOf(mixRgb(PALETTE.skyDim[0], PALETTE.skyWarm[0], mood)),
      mid: rgbOf(mixRgb(PALETTE.skyDim[1], PALETTE.skyWarm[1], mood)),
      horizon: rgbOf(horizon),
      sun: [0, 1, 2].map((n) => rgbOf(mixRgb(PALETTE.sunDim[n], PALETTE.sunWarm[n], warmth))),
      stripe: rgbOf(mixRgb(PALETTE.skyDim[2], PALETTE.sunStripeWarm, Math.min(1, mood))),
    };
  }
  return hued;
}

/** Draw the sky and the sun of `frame` on `g`. */
export function drawSky(g, geo, frame) {
  const { w, h, H } = geo;
  const v = geo.v ?? h;
  const { t, kick, dull } = frame;
  const { mood, flare } = frame.look ?? NEUTRAL;
  const px = unitOf(w);
  g.fillStyle = dull ? PALETTE.dullNight : PALETTE.night;
  g.fillRect(0, H, w, h - H);
  const sky = g.createLinearGradient(0, 0, 0, H);
  const hue = huesOf(mood, flare);
  if (dull) {
    sky.addColorStop(0, PALETTE.dullNight);
    sky.addColorStop(1, PALETTE.dullHorizon);
  } else {
    sky.addColorStop(0, hue.top);
    sky.addColorStop(0.6, hue.mid);
    sky.addColorStop(1, hue.horizon);
  }
  g.fillStyle = sky;
  g.fillRect(0, 0, w, H);
  // the stars show more in a quiet night
  const shine = (1.15 - 0.4 * mood) / (1.15 - 0.4 * LOOK.neutral);
  for (let i = 0; i < STARS; i++) {
    const x = ((i * 97.31) % 1) * w;
    const y = (((i * 57.13) % 1) * H * 0.55);
    const twinkle = 0.5 + 0.5 * Math.sin(t * 1.7 + i);
    const size = Math.max(1, px * (i % 4 === 0 ? 2 : 1.2));
    g.fillStyle = `rgba(${PALETTE.star}, ${(dull ? 0.18 : (0.25 + 0.35 * twinkle) * shine) * (1 - y / (H * 0.55))})`;
    g.fillRect(x, y, size, size);
  }
  const r = v * 0.22 * (1 + kick * 0.04 + flare * 0.06);
  const cx = w / 2;
  const cy = H - v * 0.04;
  g.save();
  g.beginPath();
  g.rect(0, 0, w, H);
  g.clip();
  if (geo.glow !== false) {
    // the glow is drawn, not blurred: a ring of gradient round the sun (a blurred shadow is slow
    // where the canvas is drawn in software)
    const reach = r + HALO * px;
    const halo = g.createRadialGradient(cx, cy, r, cx, cy, reach);
    for (const [at, colour] of dull ? PALETTE.dullSunHalo : PALETTE.sunHalo) halo.addColorStop(at, colour);
    g.fillStyle = halo;
    // the sun stands on the horizon: nothing of the glow is wanted below it
    g.fillRect(cx - reach, cy - reach, 2 * reach, Math.min(reach, H - cy + 1) + reach);
  }
  if (!dull && flare > 0.01 && geo.glow !== false) {
    // a moment's flare: a warm halo, fading with the flare
    const warm = g.createRadialGradient(cx, cy, r * 0.8, cx, cy, r * 1.9);
    warm.addColorStop(0, `${PALETTE.sunFlare}${0.45 * flare})`);
    warm.addColorStop(1, PALETTE.sunFlareClear);
    g.fillStyle = warm;
    g.fillRect(0, 0, w, H);
  }
  const sun = g.createLinearGradient(0, cy - r, 0, cy + r);
  if (dull) {
    sun.addColorStop(0, PALETTE.dullSun[0]);
    sun.addColorStop(1, PALETTE.dullSun[1]);
  } else {
    // the sun is brighter the warmer the night, as it was drawn before there was a mood at 0.7
    for (let n = 0; n < 3; n++) sun.addColorStop(n / 2, hue.sun[n]);
  }
  g.fillStyle = sun;
  g.beginPath();
  g.arc(cx, cy, r, 0, Math.PI * 2);
  g.fill();
  // the stripes stay inside the sun, and clear of its foot at the horizon
  g.beginPath();
  g.arc(cx, cy, r, 0, Math.PI * 2);
  g.clip();
  g.fillStyle = dull ? PALETTE.dullSunStripe : hue.stripe;
  for (const [above, thick] of stripesOf(v)) g.fillRect(cx - r, H - above - thick, 2 * r, thick);
  g.restore();
  if (!dull) {
    drawRays(g, geo, { cx, cy, r }, frame);
    drawStars(g, geo, frame);
  }
}
