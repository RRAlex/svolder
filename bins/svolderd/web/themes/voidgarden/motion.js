/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The motion levels: Full, Calm and Still, and the pace each draws at; and the rig that turns the
// feed into what the scene draws from: the scene's clock (the draws' seconds, scaled by the level
// and by the cues), the uniform values the layers read (features, cues eased by time constants,
// the track change's fade) and the camera's flight. Pure and deterministic: the seconds, the
// feed and the seed are handed in, so a frame is the same for the same (seed, t, feed); the only
// randomness is the seeded hash of (seed, kind, time cell): the flicker of the accents is a
// function of time, the same at 30 or 144 steps a second.
//
//   motionOf(prefs, reduced)       the level in use: the kept one, else Calm under reduced motion
//   paceOf({ motion, silent, playing, coarse })   the least ms between two draws
//   keepFrame({ sinceMs, frameMs, paceMs, atMost })   whether a frame the screen offers is drawn
//   createRig({ seed })            `step(dt, input)` -> `{ u, cam }`; `reseed(seed)`; `seed`

import { hashStr, mulberry } from './rng.js';

/** The levels, in the menu's order. */
export const MOTIONS = ['full', 'calm', 'still'];

/** The least ms between draws at each level (0 is every frame): Calm ten a second, Still one. */
const BASE_PACE = { full: 0, calm: 100, still: 1000 };

/** The least ms between draws while the music is silent or the player is not playing, on a coarse
 * pointer (a phone): a desktop keeps its level's own pace. */
export const IDLE_PACE = 1000 / 30;

/** The scene's time scale at each level (Calm and Still run at half time; the accents are
 * Full's). */
const TIME_SCALE = { full: 1, calm: 0.5, still: 0.5 };

/** The level in use: the kept one, else Calm under reduced motion and Full otherwise. */
export function motionOf(prefs, reduced) {
  const kept = prefs?.get?.('motion', null);
  if (MOTIONS.includes(kept)) return kept;
  return reduced ? 'calm' : 'full';
}

/** The least ms between two draws at `motion`; on a `coarse` pointer a silent or paused scene is
 * drawn at 30 a second at most. */
export function paceOf({
  motion, silent = false, playing = true, coarse = false,
}) {
  const base = BASE_PACE[motion] ?? 0;
  return coarse && (silent || !playing) ? Math.max(base, IDLE_PACE) : base;
}

/** `atMost` keeps a pace as a ceiling: a draw once 90 % of the pace has passed. */
export const AT_MOST = 0.9;

/** Whether a frame the screen offers is drawn under a pace of `paceMs` (a pace the loop does not
 * time itself: under 100 ms): the one whose time is nearest the pace's, so a screen at 72 Hz draws
 * every second frame and not every third. `sinceMs` is the time since the last draw, `frameMs` the
 * time since the last frame was offered. */
export const keepFrame = ({
  sinceMs, frameMs, paceMs, atMost = false,
}) => !(paceMs > 0) || (atMost ? sinceMs >= paceMs * AT_MOST : sinceMs + frameMs / 2 >= paceMs);

const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);
const lerp = (a, b, t) => a + (b - a) * t;
/** `1 - exp(-dt * rate)`: the share of the way to a target a time constant covers in `dt`
 * seconds. */
const ease = (dt, rate) => 1 - Math.exp(-dt * rate);

/** The flight by section: altitude, speed factor, field of view. */
export const FLY = Object.freeze({
  intro: { alt: 6, v: 1, fov: 55 },
  build: { alt: 10, v: 0.95, fov: 54 },
  drop: { alt: 2.6, v: 1.3, fov: 60 },
  break: { alt: 16, v: 0.6, fov: 50 },
});

/** How long a tear lasts after the link drops, seconds. */
const TEAR_S = 1;

/** The accents' flicker is drawn on a time grid, cells a second: the static's level, the tear's
 * on/off, and the tear's row (a new one every 0.09 s). */
const GRID = { static: 30, tear: 24, tearY: 1 / 0.09 };

/** A sample rate above this reads as high-resolution audio: the scene's grain shows. */
const HIGH_RATE_HZ = 48000;

/** The ground the camera flies over: `riverX(z)` the river's x at depth z, `height(x, z)` the
 * ground's height. The land layer replaces this; the default is flat. */
const FLAT = Object.freeze({ riverX: () => 0, height: () => 0 });

/**
 * The rig: one per scene. `step(dt, input)` moves everything by `dt` seconds of the page's clock
 * and returns the live `{ u, cam }` (the same objects each time, never copied). `input`: `motion`,
 * `features` (the smoothed values), `cues`, `director` (its `state`), `events` (this step's),
 * `aspect`, `mouse` `{ x, y }`, `terrain`.
 */
export function createRig({ seed = 0 } = {}) {
  let seeded = seed;
  const cells = { static: { n: NaN, v: 0 }, tear: { n: NaN, v: 0 }, tearY: { n: NaN, v: 0 } };
  /** The number in [0, 1) of `kind`'s cell at the rig's clock: one per cell, however often
   * asked. */
  const flicker = (kind) => {
    const cell = cells[kind];
    const n = Math.floor(u.gt * GRID[kind] + 1e-9);
    if (cell.n !== n) {
      cell.n = n;
      cell.v = mulberry(hashStr(`rig:${seeded}:${kind}:${n}`))();
    }
    return cell.v;
  };
  const u = {
    time: 0, gt: 0, bass: 0.3, mid: 0.3, high: 0.2, l: 0.35, r: 0.35, width: 0.6, loud: 0.35,
    flux: 0,
    cent: 0.4, flat: 0.2, peak: 0, shock: 1, morph: 0, frost: 0, haze: 0, cross: 0, grain: 0,
    static: 0, tear: 0, tearY: 0, fade: 1, seed: 0, calm: 0, colRate: 0, volume: 0.5,
  };
  const cam = {
    x: 0, y: 6, z: 0, lookX: 0, lookY: 3.6, lookZ: -34, fov: 55, roll: 0, aspect: 1.6, dist: 1,
  };
  let ts = 1;
  let shockAge = 99;
  let staticLevel = 0;
  let tearT = 0;
  const flight = { s: 0, v: 8, alt: 6, cy: 6, mx: 0, my: 0, roll: 0, t: 0 };
  let distF = 1;

  function cues(dt, input) {
    const { cues: c = {}, motion = 'full', features: f = {}, director: d = null } = input;
    const calm = motion !== 'full';
    const tsT = (c.mute ? 0 : c.pause ? 0.12 : 1) * TIME_SCALE[motion];
    ts += (tsT - ts) * ease(dt, 3);
    const sdt = dt * ts;
    u.time += sdt;
    u.gt += dt;
    u.calm = calm ? 1 : 0;
    if (!c.mute) {
      for (const k of ['bass', 'mid', 'high', 'l', 'r', 'width', 'loud', 'flux', 'cent', 'flat']) {
        if (Number.isFinite(f[k])) u[k] = f[k];
      }
    }
    for (const e of input.events ?? []) {
      if (e.type === 'peak') shockAge = 0;
    }
    u.peak = d ? clamp(d.peak * (calm ? 0.3 : 1)) : 0;
    shockAge += sdt;
    u.shock = Math.min(1, shockAge / 1.8);
    u.morph += ((d?.morph ?? 0) - u.morph) * ease(sdt, 0.9);
    const k2 = ease(dt, 2);
    u.frost += ((c.mute ? 1 : 0) - u.frost) * k2;
    u.haze += ((c.changed ? 1 : 0) - u.haze) * k2;
    u.cross += ((c.cross ? 1 : 0) - u.cross) * k2;
    u.grain = (c.rateHz ?? c.inputRateHz ?? 0) > HIGH_RATE_HZ ? 1 : 0;
    // the accents (static, the tear) are Full's; the stream of the seed keeps them reproducible
    staticLevel += ((c.weak && !calm ? 1 : 0) - staticLevel) * k2;
    u.static = staticLevel * (0.45 + 0.55 * flicker('static'));
    if (c.linkDrop && !calm) tearT = TEAR_S;
    if (tearT > 0) {
      tearT -= dt;
      u.tearY = flicker('tearY') * 1.6 - 0.8;
      u.tear = flicker('tear') < 0.7 ? 1 : 0.2;
    } else {
      u.tear = 0;
    }
    if (d) {
      u.fade = d.fade * d.fade * (3 - 2 * d.fade);
      u.seed = d.seed;
    }
    u.volume = Number.isFinite(c.volume) ? c.volume : 0.5;
    u.colRate = ease(dt, 1.5);
    return { sdt, calm };
  }

  function flightStep(dt, sdt, calm, input) {
    const {
      features: f = {}, director: d = null, aspect = 1.6, mouse = { x: 0, y: 0 }, terrain = FLAT,
    } = input;
    const section = d?.section ?? 'intro';
    const fly = FLY[section] ?? FLY.build;
    const tall = aspect < 0.9;
    const vol = Number.isFinite(u.volume) ? u.volume : 0.5;
    distF += (lerp(1.32, 0.8, vol) * (tall ? Math.sqrt(0.9 / aspect) : 1) - distF) * ease(dt, 2);
    const fl = flight;
    fl.t += sdt;
    const loud = f.loud ?? 0.35;
    const vt = (5 + 11 * loud) * fly.v * (calm ? 0.5 : 1);
    fl.v += (vt - fl.v) * ease(dt, 0.6);
    fl.s += fl.v * sdt;
    const z = -fl.s;
    const pr = calm ? 0.4 : 1;
    fl.mx += (mouse.x * pr - fl.mx) * ease(dt, 2.5);
    fl.my += (mouse.y * pr - fl.my) * ease(dt, 2.5);
    fl.alt += (fly.alt * distF * (1 + 0.25 * Math.sin(fl.t * 0.07)) - fl.alt) * ease(sdt, 0.35);
    const sway = Math.sin(fl.t * 0.11) * 0.5;
    const rx = terrain.riverX(z);
    const cx = rx + sway + fl.mx * 3 * distF;
    // clearance: a look-ahead along the flight line and both sides; land wants 3.4 above it,
    // water 2.7
    const g0 = Math.max(terrain.height(cx, z), 0);
    let gm = 0;
    for (const k of [0, 4, 9, 15, 23, 33, 45]) {
      const zz = z - k;
      const xx = terrain.riverX(zz) + sway + fl.mx * 3 * distF;
      for (const o of [-3, 0, 3]) {
        const hh = terrain.height(xx + o, zz);
        gm = Math.max(gm, (hh > 0.25 ? hh + 3.4 : 2.7) - k * 0.05);
      }
    }
    const cyT = Math.max(fl.alt + fl.my * 1.5, gm);
    const floor = Math.max(g0 + 2.4, 2.2);
    fl.cy += (cyT - fl.cy) * ease(dt, cyT > fl.cy ? 2.4 : 1.1);
    if (fl.cy < floor) fl.cy += (floor - fl.cy) * ease(dt, 9);
    fl.cy = Math.max(fl.cy, g0 + 1.3);
    // the groove: a bob on the bar, a dip on the kick, a sway over two bars (Full only)
    const g = d?.groove;
    const gr = !calm && g ? g.energy : 0;
    const bob = gr * (0.3 * Math.sin(2 * Math.PI * (g?.bar ?? 0)) - 0.18 * (g?.kick ?? 0));
    const beat = d?.beat ?? 0;
    const sw2 = gr * 0.8 * Math.sin((2 * Math.PI * beat) / 8);
    const lz = z - 34;
    cam.x = cx + sw2;
    cam.y = fl.cy + bob;
    cam.z = z;
    cam.lookX = terrain.riverX(lz) + fl.mx * 4 + sw2 * 0.4;
    cam.lookY = fl.cy * 0.45 + 0.6 + fl.my * 2.5 + bob * 0.5 + gr * 0.12 * (g?.kick ?? 0);
    cam.lookZ = lz;
    const kc = terrain.riverX(z - 30) - 2 * terrain.riverX(z - 15) + rx;
    fl.roll += (clamp(-kc * 0.02, -0.16, 0.16) - fl.mx * 0.04 * pr - fl.roll) * ease(dt, 1.2);
    cam.roll = fl.roll + gr * 0.016 * Math.sin((2 * Math.PI * beat) / 8 + 1);
    const fov = fly.fov * (tall ? 1.25 : 1);
    cam.fov += (fov - cam.fov) * ease(dt, 0.8);
    cam.aspect = aspect;
    cam.dist = distF;
  }

  return {
    u,
    cam,
    /** Move the scene `dt` seconds. */
    step(dt, input = {}) {
      const { sdt, calm } = cues(dt, input);
      flightStep(dt, sdt, calm, input);
      return { u, cam };
    },
    /** The seed the flicker is drawn from. */
    get seed() { return seeded; },
    /** Draw the flicker from another seed. */
    reseed(next) {
      seeded = next;
      for (const cell of Object.values(cells)) cell.n = NaN;
    },
  };
}
