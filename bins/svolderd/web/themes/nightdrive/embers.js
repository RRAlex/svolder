/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The embers: sparks that rise from the floor's hot near cells, only while the music has been at a
// peak for a while (the ear's `embers`). Born, moved and aged by the clock's seconds, never by the
// frame, and never more alive than a cap that follows the canvas's size. A pure module.

/** The numbers: `max` the most alive on a canvas of `area` CSS px squared or more, `min` the fewest
 * the cap falls to on a small one; `rate` the births a second of a cell of level 1 beyond `hot` at
 * full strength; `from` the share of the floor's rows, from the horizon, that sparks do not rise
 * from; `hold` the least strength that brings any; `life` the seconds a spark lives; `drag` how fast
 * a spark's climb slows (the rate of an exponential, a second); `top`, how high a spark may rise, as a share
 * of the horizon's height. */
export const EMBERS = Object.freeze({
  max: 90, min: 24, area: 1280 * 800, rate: 1.2, hot: 0.55, from: 0.45, hold: 0.05, life: 1.83, drag: 0.9, top: 0.4,
});

/** The most sparks alive at once on a canvas `w` by `h`. */
export const capOf = (w, h) => Math.round(Math.min(
  EMBERS.max,
  Math.max(EMBERS.min, (EMBERS.max * w * h) / EMBERS.area),
));

/**
 * The sparks. `step(dt, { strength, crossings, w, h, H })` moves and ages the ones alive and, while
 * `strength` is over 0.05, lets new ones rise from `crossings` (`{ x, y, v, rows, stride }`: the
 * floor's grid corners on the screen with their levels, row edge by row edge, `rows` the last edge; none
 * below the scene rise); the rate of births
 * is the strength's. `list` is the sparks alive, each `{ x, y, vx, vy, life, size }`, `life` from 1
 * (new) to 0.
 */
export function createEmbers({ random = Math.random } = {}) {
  const list = [];
  return {
    list,

    step(dt, {
      strength, crossings, w, h, H,
    }) {
      const slow = Math.exp(-dt * EMBERS.drag);
      for (let n = list.length - 1; n >= 0; n--) {
        const p = list[n];
        p.x += p.vx * dt;
        p.y += p.vy * dt;
        p.vy *= slow;
        p.life -= dt / EMBERS.life;
        if (p.life <= 0 || p.y < H * EMBERS.top) list.splice(n, 1);
      }
      if (!(strength > EMBERS.hold) || !crossings) return;
      const cap = capOf(w, h);
      const { x, y, v, rows, stride } = crossings;
      for (let k = Math.floor(rows * EMBERS.from); k <= rows && list.length < cap; k++) {
        for (let i = 0; i < stride && list.length < cap; i++) {
          const level = v[k * stride + i];
          if (level <= EMBERS.hot || y[k * stride + i] > h) continue;
          if (random() < Math.min(1, (level - 0.5) * EMBERS.rate * strength * dt)) {
            list.push({
              x: x[k * stride + i],
              y: y[k * stride + i],
              vx: (random() - 0.5) * w * 0.02,
              vy: -h * (0.1 + random() * 0.16),
              life: 1,
              size: (1.2 + random() * 2) * (w / 1000) + 1,
            });
          }
        }
      }
    },

    clear() {
      list.length = 0;
    },
  };
}
