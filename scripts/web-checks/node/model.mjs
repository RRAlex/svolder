/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for smoke-model.js: the rules without a GPU. Run: node model.mjs
import assert from 'node:assert/strict';
import {
  Haze, HazeClock, TUNING, drawShade, ease, frameDt, gridSize, hazeTarget, pointerSplat, restSplat,
  rhythm, scrollStir, seeded, shadePick, shiftUv, sidePick, sourceStrength, splatBox, wander,
} from '../../../bins/svolderd/web/themes/ember/smoke-model.js';
import { lookTests, retuned } from './look-values.mjs';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const okLook = lookTests(TUNING, ok);
const near = (a, b, e = 1e-9) => assert.ok(Math.abs(a - b) <= e, `${a} vs ${b}`);

// The one test that holds the approved look: every other test written for a look number is
// skipped by run-all.sh while this one fails, so a retuned number fails exactly one check.
ok('the approved look: TUNING holds the numbers of node/look-values.mjs', () => {
  assert.deepEqual(retuned(TUNING), [], 'TUNING and the approved look differ: if the retune is '
    + 'meant, change the number in node/look-values.mjs and in TUNING together, then rerun the '
    + 'node harnesses to see which rules the new number breaks');
});
ok('TUNING: frozen, every value a finite number', () => {
  assert.ok(Object.isFrozen(TUNING));
  for (const [k, v] of Object.entries(TUNING)) assert.ok(Number.isFinite(v), k);
  assert.equal(TUNING.maxVelocity, undefined); // no clamp that is not in the method
});
ok('sourceStrength: 0.22 + 0.78 * level, clamped', () => {
  near(sourceStrength(0), 0.22);
  near(sourceStrength(0.06), 0.22 + 0.78 * 0.06);
  near(sourceStrength(1), 1);
  near(sourceStrength(-3), 0.22);
  near(sourceStrength(7), 1);
  near(sourceStrength(NaN), 0.22);
});
ok('ease: converges, never overshoots, monotone', () => {
  let v = 0;
  let last = 0;
  for (let i = 0; i < 60; i++) {
    v = ease(v, 1, 1 / 60, 1);
    assert.ok(v >= last && v <= 1);
    last = v;
  }
  assert.ok(v > 0.97 && v < 1, `after 1 s: ${v}`);
  let w = 1;
  for (let i = 0; i < 600; i++) { w = ease(w, 0.2, 1 / 60, 1); assert.ok(w >= 0.2); }
  near(w, 0.2, 1e-6);
  // one huge step lands on the target, not past it
  assert.ok(ease(0, 1, 100, 1) <= 1);
  assert.equal(ease(0.3, 0.8, 0, 1), 0.3);
  assert.equal(ease(0.3, 0.8, 0.1, 0), 0.8);
  // frame-rate independent: two half steps = one whole step
  near(ease(ease(0, 1, 0.05, 1), 1, 0.05, 1), ease(0, 1, 0.1, 1), 1e-12);
});
ok('frameDt: clamps to maxDt, never below 0', () => {
  assert.equal(frameDt(3600), TUNING.maxDt);
  assert.equal(frameDt(0.01), 0.01);
  assert.equal(frameDt(-5), 0);
  assert.equal(frameDt(NaN), 0);
});
ok('gridSize: short side fixed, long side in the aspect', () => {
  assert.deepEqual(gridSize(1280, 900, 128), { w: 182, h: 128 });
  assert.deepEqual(gridSize(390, 844, 128), { w: 128, h: 277 });
  assert.deepEqual(gridSize(500, 500, 128), { w: 128, h: 128 });
  assert.deepEqual(gridSize(2560, 1440, 512), { w: 910, h: 512 });
  assert.deepEqual(gridSize(1000, 300, 128), { w: 427, h: 128 });
  assert.deepEqual(gridSize(0, 0, 128), { w: 1, h: 1 });
});
const base = { now: 100, lastStir: -Infinity, lastFrame: 99, away: false, fadedAt: null,
  hidden: false };
okLook('rhythm: calm = one frame per 1/calmFps, else skip', () => {
  assert.equal(rhythm({ ...base, lastFrame: 100 - 0.01 }), 'skip');
  assert.equal(rhythm({ ...base, lastFrame: 100 - 0.03 }), 'skip');
  assert.equal(rhythm({ ...base, lastFrame: 100 - 0.05 }), 'frame');
  assert.equal(rhythm({ ...base, lastFrame: 100 - 0.2 }), 'frame');
  // a 60 Hz callback that lands a hair early still counts
  assert.equal(rhythm({ ...base, lastFrame: 100 - 0.047 }), 'frame');
});
okLook('rhythm: stirred = every frame for stirredSecs', () => {
  assert.equal(rhythm({ ...base, lastStir: 99.5, lastFrame: 100 - 0.001 }), 'frame');
  assert.equal(rhythm({ ...base, lastStir: 100 - 1.99, lastFrame: 100 - 0.001 }), 'frame');
  assert.equal(rhythm({ ...base, lastStir: 100 - 2.01, lastFrame: 100 - 0.001 }), 'skip');
});
ok('rhythm: away stops once its fade is over, not before', () => {
  const a = { ...base, away: true, fadedAt: 100.5 };
  assert.equal(rhythm({ ...a, lastFrame: 99.99 }), 'skip');
  assert.equal(rhythm({ ...a, now: 100.4, lastFrame: 100.39, lastStir: 100.39 }), 'frame');
  assert.equal(rhythm({ ...a, now: 100.5 }), 'stop');
  assert.equal(rhythm({ ...a, now: 130, lastStir: 129.9 }), 'stop');
  assert.equal(rhythm({ ...base, away: false, fadedAt: 90 }), 'frame');
});
ok('rhythm: hidden stops, whatever else', () => {
  assert.equal(rhythm({ ...base, hidden: true }), 'stop');
  assert.equal(rhythm({ ...base, hidden: true, lastStir: 99.9 }), 'stop');
  assert.equal(rhythm({ ...base, hidden: true, away: true, fadedAt: 1e9 }), 'stop');
});
ok('restSplat: uv position (y up), radius in canvas heights, pull rate, dye amount', () => {
  const sp = restSplat({ x: 100, y: 300 }, [400, 800], 0.5, 0.05, [1, 0.5, 0.25], Math.PI / 2);
  near(sp.x, 0.25);
  near(sp.y, 1 - 300 / 800);
  near(sp.radius, (TUNING.restRadius * 400) / 800); // portrait: the short side is the width
  const land = restSplat({ x: 0, y: 0 }, [1600, 800], 1, 0.05, [1, 1, 1], 0);
  near(land.radius, TUNING.restRadius); // landscape: the short side is the height
  near(land.x, 0);
  near(land.y, 1);
  near(sp.rate, 1 - Math.exp(-TUNING.restPull * 0.05));
  near(sp.pull[1], TUNING.restDrift * 0.5); // along the angle (90 degrees: up)
  near(sp.pull[0], 0);
  near(land.pull[0], TUNING.restDrift); // along 0 degrees: to the right
  near(land.pull[1], 0);
  assert.deepEqual([sp.pull[2], sp.pull[3]], [0, 0]);
  const amount = TUNING.restDye * 0.5 * 0.05;
  near(sp.dye[3], amount);
  near(sp.dye[0], amount);
  near(sp.dye[1], amount * 0.5);
  near(sp.dye[2], amount * 0.25);
  assert.equal(sp.rest, true);
  assert.equal(restSplat({ x: 1, y: 1 }, [400, 800], 1, 0, [1, 1, 1], 0).dye[3], 0);
});
ok('shiftUv: the content goes against the scroll, y up; nothing for a bad size or delta', () => {
  assert.deepEqual(shiftUv(0, 300, [390, 844]), [-0, 300 / 844]);
  near(shiftUv(39, 0, [390, 844])[0], -0.1);
  assert.deepEqual(shiftUv(0, -100, [400, 800]), [-0, -0.125]);
  for (const bad of [[0, 0], [400, 0], [0, 800], [NaN, 800]]) {
    assert.deepEqual(shiftUv(5, 5, bad), [0, 0]);
  }
  assert.deepEqual(shiftUv(NaN, 5, [400, 800]), [0, 0]);
  assert.deepEqual(shiftUv(5, Infinity, [400, 800]), [0, 0]);
});
ok('splatBox: every texel outside the box has the Gaussian under 1.3e-4; the box is on the grid',
  () => {
    const grid = [128, 96];
    const aspect = 1.4;
    for (const [x, y, r] of [[0.5, 0.5, 0.04], [0.1, 0.9, 0.16], [0.97, 0.02, 0.05],
      [0.5, 0.5, 0.01], [0.3, 0.6, 0.3]]) {
      const box = splatBox({ x, y }, r, aspect, grid);
      assert.ok(box.x >= 0 && box.y >= 0 && box.x + box.w <= grid[0] && box.y + box.h <= grid[1]);
      let worst = 0;
      for (let j = 0; j < grid[1]; j++) {
        for (let i = 0; i < grid[0]; i++) {
          if (i >= box.x && i < box.x + box.w && j >= box.y && j < box.y + box.h) continue;
          const dx = ((i + 0.5) / grid[0] - x) * aspect;
          const dy = (j + 0.5) / grid[1] - y;
          worst = Math.max(worst, Math.exp(-(dx * dx + dy * dy) / (r * r)));
        }
      }
      assert.ok(worst < 1.3e-4, `${x} ${y} ${r}: ${worst}`);
    }
    // a splat well inside is a small box, not the whole grid
    const small = splatBox({ x: 0.5, y: 0.5 }, 0.04, aspect, grid);
    assert.ok(small.w * small.h < 0.2 * grid[0] * grid[1], JSON.stringify(small));
  });
ok('splatBox: clipped at the edges; nothing for a splat off the grid or a bad number', () => {
  const corner = splatBox({ x: 0, y: 0 }, 0.1, 1, [100, 100]);
  assert.deepEqual([corner.x, corner.y], [0, 0]);
  assert.ok(corner.w >= 30 && corner.w <= 31 && corner.h >= 30 && corner.h <= 31);
  assert.equal(splatBox({ x: 2, y: 0.5 }, 0.1, 1, [100, 100]), null);
  assert.equal(splatBox({ x: 0.5, y: -1 }, 0.1, 1, [100, 100]), null);
  for (const bad of [NaN, Infinity]) {
    assert.equal(splatBox({ x: bad, y: 0.5 }, 0.1, 1, [100, 100]), null);
    assert.equal(splatBox({ x: 0.5, y: 0.5 }, bad, 1, [100, 100]), null);
    assert.equal(splatBox({ x: 0.5, y: 0.5 }, 0.1, 1, [bad, 100]), null);
  }
  assert.equal(splatBox({ x: 0.5, y: 0.5 }, 0.1, 0, [100, 100]), null);
});

ok('seeded: deterministic in [0, 1), two seeds differ', () => {
  const a = seeded(1);
  const b = seeded(1);
  const c = seeded(2);
  const xs = Array.from({ length: 50 }, () => a());
  assert.deepEqual(xs, Array.from({ length: 50 }, () => b()));
  assert.notDeepEqual(xs, Array.from({ length: 50 }, () => c()));
  assert.ok(xs.every((x) => x >= 0 && x < 1));
  assert.equal(new Set(xs).size, 50);
});

/** The hue, saturation and lightness (percent) of an sRGB triple 0 to 1. */
function hslOf([r, g, b]) {
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  if (d === 0) return { h: 0, s: 0, l: l * 100 };
  const s = d / (1 - Math.abs(2 * l - 1));
  let h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  h = (h * 60 + 360) % 360;
  return { h, s: s * 100, l: l * 100 };
}
/** The hue distance, round the circle, in degrees. */
const hueGap = (a, b) => Math.min(Math.abs(a - b), 360 - Math.abs(a - b));

okLook('drawShade: every range of the palette over 10 000 draws, means near the centres', () => {
  const random = seeded(7);
  const keys = ['h', 's', 'l', 'g', 'step'];
  const stat = Object.fromEntries(keys.map((k) => [k, [Infinity, -Infinity, 0]]));
  const add = (key, x) => {
    const t = stat[key];
    t[0] = Math.min(t[0], x);
    t[1] = Math.max(t[1], x);
    t[2] += x / 10000;
  };
  for (let i = 0; i < 10000; i++) {
    const sh = drawShade(random);
    add('h', sh.hsl.h);
    add('s', sh.hsl.s);
    add('l', sh.hsl.l);
    add('g', sh.greyShare);
    const base = hslOf(sh.base);
    near(base.h, sh.hsl.h, 0.5);
    near(base.s, sh.hsl.s, 0.5);
    near(base.l, sh.hsl.l, 0.5);
    const grey = hslOf(sh.grey);
    assert.ok(hueGap(grey.h, sh.hsl.h) <= 25.5 || grey.s < 1, `grey hue ${grey.h} vs ${sh.hsl.h}`);
    near(grey.s, sh.hsl.s / 3, 0.5);
    near(grey.l, sh.hsl.l - 4, 0.5);
    const light = hslOf(sh.light);
    near(light.l, Math.min(82, sh.hsl.l + 10), 0.5);
    near(light.s, sh.hsl.s, 0.8);
    assert.ok(hueGap(light.h, sh.hsl.h) < 1.5 || sh.hsl.s < 10, `light hue ${light.h}`);
    for (const c of [sh.base, sh.grey, sh.light, sh.dial, ...sh.sides]) {
      assert.ok(c.length === 3 && c.every((x) => x >= 0 && x <= 1));
    }
    add('step', hueGap(hslOf(sh.sides[0]).h, hslOf(sh.sides[1]).h) / 2);
  }
  const span = (key, a, b) => {
    const [mn, mx, mean] = stat[key];
    assert.ok(mn >= a && mx <= b, `${key} range ${mn}..${mx}`);
    assert.ok(mn < a + 0.02 * (b - a) && mx > b - 0.02 * (b - a), `${key} reaches its ends`);
    near(mean, (a + b) / 2, 0.1 * (a + b) / 2);
  };
  span('h', 205, 275);
  span('s', 54, 60);
  span('l', 55, 74);
  span('g', 0.3, 0.7);
  span('step', 22, 30);
});
ok('drawShade: the palette is three tones, the sides 20 degrees or more apart, the dial distinct',
  () => {
    const random = seeded(17);
    for (let i = 0; i < 10000; i++) {
      const sh = drawShade(random);
      assert.equal(sh.sides.length, 2);
      assert.equal(sh.dial.length, 3);
      const [a, b] = sh.sides.map(hslOf);
      const d = hslOf(sh.dial);
      assert.ok(hueGap(a.h, b.h) >= TUNING.shadeToneMargin, `sides ${a.h} ${b.h}`);
      assert.ok(hueGap(d.h, a.h) >= TUNING.shadeToneMargin, `dial ${d.h} vs side ${a.h}`);
      assert.ok(hueGap(d.h, b.h) >= TUNING.shadeToneMargin, `dial ${d.h} vs side ${b.h}`);
      // the three share the saturation; the side tones the base's lightness, the dial's lighter
      for (const t of [a, b, d]) near(t.s, sh.hsl.s, 0.8);
      near(a.l, sh.hsl.l, 0.5);
      near(b.l, sh.hsl.l, 0.5);
      assert.ok(d.l > a.l + 5, `dial lightness ${d.l} vs ${a.l}`);
    }
  });
okLook('drawShade: the palette\'s saturation lies in the user\'s band, half way to 2/3 of the way '
  + 'from the flat grey shade\'s toward the violet glow palette\'s, from both sets of values',
  () => {
  // the violet glow palette: three colours weighted 0.5, 0.3 and 0.2
  const hex = (h) => [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16) / 255);
  const glow = ['#8b5cf6', '#6d3fd1', '#b9a0ff'].map((c) => hslOf(hex(c)).s);
  const before = [0.5, 0.3, 0.2].reduce((t, w, i) => t + w * glow[i], 0);
  // the flat grey shade: base saturation 8 to 38, the haze's emissions grey (a third of it) 0.3 to
  // 0.7 of the time; the dial takes the base
  const mid = (8 + 38) / 2;
  const hazeNow = mid * (0.5 / 3 + 0.5);
  const dialNow = mid;
  near(before, 83.17, 0.01);
  near(hazeNow, 15.33, 0.01);
  const band = (now) => [now + 0.5 * (before - now), now + (2 / 3) * (before - now)];
  const [hLo, hHi] = band(hazeNow);
  const [dLo, dHi] = band(dialNow);
  const lo = Math.max(hLo, dLo);
  const hi = Math.min(hHi, dHi);
  assert.ok(lo < hi, 'the haze\'s and the dial\'s bands overlap');
  assert.ok(TUNING.shadeSatLo >= lo && TUNING.shadeSatHi <= hi,
    `${TUNING.shadeSatLo}..${TUNING.shadeSatHi} outside ${lo}..${hi}`);
  assert.ok(TUNING.shadeSatHi - TUNING.shadeSatLo >= 4, 'a load still varies in saturation');
  const random = seeded(23);
  for (let i = 0; i < 10000; i++) {
    const s = hslOf(drawShade(random).sides[i % 2]).s;
    assert.ok(s >= lo && s <= hi, `side saturation ${s}`);
    assert.ok(hslOf(drawShade(random).dial).s >= lo);
  }
});
ok('drawShade: the grey hue wraps into [0, 360) at the circle\'s ends', () => {
  const sh = drawShade(() => 0.9999);
  assert.ok(sh.hsl.h < TUNING.shadeHueHi + 0.01);
  for (let i = 0; i < 2000; i++) {
    const g = hslOf(drawShade(seeded(i)).grey);
    assert.ok(g.h >= 0 && g.h < 360);
  }
});
ok('shadePick: grey with probability greyShare, within 2 %, else the base', () => {
  const sh = drawShade(seeded(5));
  const random = seeded(11);
  let grey = 0;
  for (let i = 0; i < 10000; i++) {
    const c = shadePick(sh, random);
    if (c === sh.grey) grey++;
    else assert.equal(c, sh.base);
  }
  near(grey / 10000, sh.greyShare, 0.02);
  assert.equal(shadePick(sh, () => sh.greyShare - 1e-9), sh.grey);
  assert.equal(shadePick(sh, () => sh.greyShare), sh.base);
});

/** The dye density (amount per px2) of `splats` at the points of a lattice, summed over the
 * lattice's points inside each of the `boxes` ([x0, y0, x1, y1]) and averaged per point. */
function densities(frames, boxes, size) {
  const step = 40;
  const pts = [];
  for (let y = step / 2; y < size[1]; y += step) {
    for (let x = step / 2; x < size[0]; x += step) pts.push([x, y]);
  }
  const sum = boxes.map(() => 0);
  const count = boxes.map(() => 0);
  const grid = pts.map(() => 0);
  for (const list of frames) {
    for (const s of list) {
      pts.forEach(([x, y], i) => {
        const d2 = ((x - s.x) ** 2 + (y - s.y) ** 2) / (s.radius * s.radius);
        if (d2 < 9) grid[i] += s.dye[3] * Math.exp(-d2);
      });
    }
  }
  pts.forEach(([x, y], i) => {
    boxes.forEach(([x0, y0, x1, y1], k) => {
      if (x >= x0 && x < x1 && y >= y0 && y < y1) {
        sum[k] += grid[i];
        count[k]++;
      }
    });
  });
  return sum.map((v, k) => v / Math.max(1, count[k]) / frames.length);
}
/** `secs` seconds of the haze's splats at `dt` steps. */
function run(haze, size, column, secs, dt, from = 0) {
  const frames = [];
  for (let t = from; t < from + secs; t += dt) {
    frames.push(haze.splats({ size, column, now: t, dt }));
  }
  return frames;
}

ok('Haze: at 2560x1440 with a 1100 px column the side bars hold more dye than the column', () => {
  const size = [2560, 1440];
  const column = { left: 730, right: 1830 };
  const frames = run(new Haze(seeded(3)), size, column, 120, 0.05);
  const [side, mid] = densities(frames, [[0, 0, 730, 1440], [730, 0, 1830, 1440]], size);
  assert.ok(mid > 0, `column ${mid}`);
  assert.ok(side > 2 * mid, `side ${side} vs column ${mid}`);
  const [edge, inner] = densities(frames, [[0, 0, 200, 1440], [530, 0, 730, 1440]], size);
  assert.ok(edge > inner, `densest toward the window's edge: ${edge} vs ${inner}`);
});
ok('Haze: with no column at 390x844 the outer edges hold more dye than the middle', () => {
  const size = [390, 844];
  const frames = run(new Haze(seeded(4)), size, null, 120, 0.05);
  const [left, right, middle] = densities(frames,
    [[0, 0, 60, 844], [330, 0, 390, 844], [120, 0, 270, 844]], size);
  assert.ok(left > middle && right > middle, `${left} ${right} vs ${middle}`);
  assert.ok(middle > 0);
  // margins narrower than the edge share fall back to the same edge bands
  const narrow = run(new Haze(seeded(4)), size, { left: 12, right: 378 }, 120, 0.05);
  const [l2, r2, m2] = densities(narrow, [[0, 0, 60, 844], [330, 0, 390, 844], [120, 0, 270, 844]],
    size);
  assert.ok(l2 > m2 && r2 > m2);
});
ok('Haze: deterministic for a seed, and the fallback bands are lighter than margins', () => {
  const size = [2560, 1440];
  const column = { left: 730, right: 1830 };
  const a = run(new Haze(seeded(9)), size, column, 5, 0.05);
  const b = run(new Haze(seeded(9)), size, column, 5, 0.05);
  assert.deepEqual(a, b);
  const total = (frames) => frames.slice(1).flat().reduce((t, s) => t + s.dye[3], 0);
  const light = run(new Haze(seeded(9)), size, null, 5, 0.05);
  assert.ok(total(light) < 0.9 * total(a), `${total(light)} vs ${total(a)}`);
  assert.ok(a.flat().every((s) => s.rest === true));
  // margins narrower than the edge share are no margins
  const tight = run(new Haze(seeded(9)), size, { left: 100, right: 2460 }, 5, 0.05);
  assert.deepEqual(tight, light);
});
okLook('Haze: at 390x844, at sampled instants, the edges beat the middle in every third', () => {
  const size = [390, 844];
  const column = { left: 16, right: 374 };
  const alpha = (a) => 255 * TUNING.peakAlpha * (1 - Math.exp(-a * TUNING.drawGain));
  /** The mean alpha of a static field of `sp` over the box, sampled every 10 px. */
  const mean = (sp, [x0, y0, x1, y1]) => {
    let t = 0;
    let k = 0;
    for (let y = y0 + 5; y < y1; y += 10) {
      for (let x = x0 + 5; x < x1; x += 10) {
        let a = 0;
        for (const s of sp) {
          a += s.dens * Math.exp(-((x - s.x) ** 2 + (y - s.y) ** 2) / s.radius ** 2);
        }
        t += alpha(a);
        k++;
      }
    }
    return t / k;
  };
  let worst = Infinity;
  let bad = 0;
  let tries = 0;
  for (let seed = 1; seed <= 60; seed++) {
    const haze = new Haze(seeded(seed));
    haze.splats({ size, column, now: 0, dt: 0 });
    for (let t = 30 + seed; t < 30 + seed + 600; t += 47) {
      const sp = haze.splats({ size, column, now: t, dt: 1 })
        .map((s) => ({ ...s, dens: s.dye[3] / TUNING.dyeDissipation }));
      for (const [y0, y1] of [[0, 281], [281, 563], [563, 844]]) {
        const edges = (mean(sp, [0, y0, 60, y1]) + mean(sp, [330, y0, 390, y1])) / 2;
        const mid = mean(sp, [100, y0, 290, y1]);
        worst = Math.min(worst, edges / mid);
        tries++;
        if (edges <= mid) bad++;
      }
    }
  }
  assert.equal(bad, 0, `${bad} of ${tries} thirds had edges <= middle (worst ratio ${worst})`);
});
ok('HazeClock: the haze is fed every simulated frame, in proportion to its step, whatever the '
  + 'frame rate', () => {
  // frames of the wall time `wall` (s), each capped by frameDt as the page does
  const fedAt = (wall) => {
    const clock = new HazeClock();
    let fed = 0;
    let sim = 0;
    let feedings = 0;
    let frames = 0;
    for (let t = 0; t < 200; t += wall) {
      const dt = frameDt(wall);
      sim += dt;
      frames++;
      const due = clock.step(dt);
      if (due !== null) {
        fed += due;
        feedings++;
        assert.equal(due, dt, 'a feeding is the frame\'s own step');
      }
    }
    return { fed, sim, feedings, frames, perWall: fed / 200 };
  };
  for (const wall of [0.05, 1 / 15, 0.2, 0.5, 2]) {
    const { fed, sim, feedings, frames } = fedAt(wall);
    near(fed, sim, 1e-9);
    assert.equal(feedings, frames, `${wall}: every frame feeds, not a pulse`);
  }
  // at 5 frames a second a wall-clock feed would be 4 times the dye's loss; this one is not
  assert.ok(fedAt(0.2).perWall <= TUNING.maxDt / 0.2 + 0.01);
  const clock = new HazeClock();
  assert.ok(clock.step(0.016) > 0); // the first frame feeds at once
  assert.equal(clock.step(0.016), 0.016); // and so does the next
  assert.equal(clock.step(NaN), null);
  assert.equal(clock.step(-1), null);
  assert.equal(clock.step(0), null);
  assert.equal(new HazeClock().step(NaN), 0); // the first frame lays the density down
});
okLook('Haze: a side source is stronger nearer the window\'s edge, and no source leaves the canvas',
  () => {
    const size = [2560, 1440];
    const column = { left: 730, right: 1830 };
    const frames = run(new Haze(seeded(13)), size, column, 400, 0.05).slice(1);
    const near = [];
    const far = [];
    for (const list of frames) {
      for (const sp of list) {
        assert.ok(sp.x >= 0 && sp.x <= size[0] && sp.y >= 0 && sp.y <= size[1]);
        const edge = Math.min(sp.x, size[0] - sp.x);
        if (sp.x < 730 && sp.radius < 250) {
          (edge < 150 ? near : edge > 330 ? far : []).push(sp.dye[3]);
        }
        if (sp.radius > 250) { // a spread source's centre keeps its margin from the sides
          const margin = Math.min(sp.radius, TUNING.hazeSpreadMargin * size[0]);
          assert.ok(sp.x >= margin - 1e-6 && sp.x <= size[0] - margin + 1e-6);
          assert.ok(sp.y >= Math.min(sp.radius, size[1] / 2) - 1e-6);
        }
      }
    }
    const mean = (v) => v.reduce((t, x) => t + x, 0) / v.length;
    assert.ok(near.length > 50 && far.length > 50, `${near.length} ${far.length}`);
    assert.ok(mean(near) > 1.2 * mean(far), `${mean(near)} vs ${mean(far)}`);
  });
ok('Haze: a long dt is the longest feeding, and every feeding adds dye', () => {
  const at = (dt) => new Haze(seeded(2)).splats({ size: [800, 600], column: null, now: 5, dt })
    .map((sp) => sp.dye[3]);
  const quiet = new Haze(seeded(2));
  quiet.splats({ size: [800, 600], column: null, now: 0, dt: 0.05 });
  const long = quiet.splats({ size: [800, 600], column: null, now: 1, dt: 1e9 });
  const full = quiet.splats({ size: [800, 600], column: null, now: 1, dt: TUNING.hazeMaxStep });
  assert.deepEqual(long.map((sp) => sp.dye[3]), full.map((sp) => sp.dye[3]));
  assert.ok(at(0.05).every((v) => v > 0));
});
ok('Haze: every emission takes one of the two side tones, each half the time, and the three '
  + 'tones are never mixed in', () => {
  const shade = drawShade(seeded(30));
  const haze = new Haze(seeded(31), shade);
  const seen = [0, 0];
  const same = (rgb, c) => rgb.every((v, i) => Math.abs(v - c[i]) < 1e-9);
  for (const list of run(haze, [1280, 900], null, 1500, 0.05)) {
    for (const sp of list) {
      const a = sp.dye[3];
      if (!(a > 0)) continue;
      const rgb = [sp.dye[0] / a, sp.dye[1] / a, sp.dye[2] / a];
      const k = shade.sides.findIndex((c) => same(rgb, c));
      assert.ok(k >= 0, `an emission of colour ${rgb} is not a side tone`);
      seen[k]++;
    }
  }
  near(seen[0] / (seen[0] + seen[1]), 0.5, 0.08);
  const pick = seeded(4);
  let first = 0;
  for (let i = 0; i < 10000; i++) if (sidePick(shade, pick) === shade.sides[0]) first++;
  near(first / 10000, 0.5, 0.02);
  assert.equal(sidePick(shade, () => 0.4999), shade.sides[0]);
  assert.equal(sidePick(shade, () => 0.5), shade.sides[1]);
});
okLook('Haze: each source pushes weakly, its direction turning round all four quadrants', () => {
  const haze = new Haze(seeded(41));
  const seen = Array.from({ length: 9 }, () => new Set());
  let sx = 0;
  let sy = 0;
  let n = 0;
  for (const list of run(haze, [1280, 900], null, 600, 0.05)) {
    list.forEach((sp, i) => {
      near(Math.hypot(sp.add[0], sp.add[1]), TUNING.hazePush * 0.05, 1e-9);
      seen[i].add((sp.add[0] >= 0 ? 1 : 0) + (sp.add[1] >= 0 ? 2 : 0));
      sx += sp.add[0];
      sy += sp.add[1];
      n++;
    });
  }
  assert.ok(seen.every((q) => q.size === 4), seen.map((q) => q.size).join());
  // no direction is preferred (in particular, not up)
  assert.ok(Math.abs(sx / n) < 0.1 * TUNING.hazePush * 0.05, `${sx / n}`);
  assert.ok(Math.abs(sy / n) < 0.1 * TUNING.hazePush * 0.05, `${sy / n}`);
});
ok('Haze: the dye per second is dyeDissipation x the target over 300 s, whatever the step', () => {
  for (const [size, column] of [[[2560, 1440], { left: 730, right: 1830 }], [[390, 844], null],
    [[1280, 900], { left: 40, right: 1240 }]]) {
    const target = TUNING.dyeDissipation * hazeTarget(size, column);
    assert.ok(target > 0);
    for (const dt of [0.05, 0.016, 0.0333]) {
      const frames = run(new Haze(seeded(21)), size, column, 300, dt);
      const sum = (list) => list.reduce((t, s) => t + s.dye[3] * Math.PI * s.radius ** 2, 0);
      // the first frame lays down the steady density at once (a primed start), the rest feed it
      near(sum(frames[0]) / hazeTarget(size, column), 1 + TUNING.dyeDissipation * dt, 0.1);
      let total = 0;
      for (const list of frames.slice(1)) total += sum(list);
      near(total / (300 - dt) / target, 1, 0.05);
    }
  }
});
ok('Haze: nothing is NaN for a zero, negative, huge or missing dt or a size of 0', () => {
  const finite = (list) => list.every((s) => [s.x, s.y, s.radius, ...s.dye, ...s.add]
    .every(Number.isFinite));
  for (const dt of [0, -1, 1e9, NaN, Infinity, undefined]) {
    const list = new Haze(seeded(1)).splats({ size: [1280, 900], column: null, now: 5, dt });
    assert.ok(finite(list), `dt ${dt}`);
    assert.ok(list.every((s) => s.dye[3] >= 0));
  }
  for (const size of [[0, 0], [0, 500], [500, 0], [NaN, 5], [1e9, 1e9]]) {
    const haze = new Haze(seeded(1));
    assert.ok(finite(haze.splats({ size, column: { left: 5, right: 10 }, now: 1, dt: 0.05 })));
    assert.ok(finite(haze.splats({ size, column: null, now: NaN, dt: 0.05 })));
  }
  assert.ok(finite(new Haze(seeded(1)).splats({ size: [800, 600], column: { left: NaN, right: 5 },
    now: 1, dt: 0.05 })));
  assert.equal(hazeTarget([0, 0], null), 0);
});
okLook(
  'scrollStir: the push grows with the scroll speed, is capped and keeps the direction', () => {
  const at = (dy, dt = 0.05) => scrollStir({ dy, dt, shortSide: 800 });
  const slow = at(20);
  const fast = at(60);
  assert.ok(slow.push > 0 && fast.push > slow.push, `${slow.push} ${fast.push}`);
  assert.equal(slow.agit, 0, 'under the speed floor nothing is added');
  assert.ok(fast.agit > slow.agit, `${slow.agit} ${fast.agit}`);
  near(at(-60).push, -fast.push);
  near(scrollStir({ dy: 30, dt: 0.025, shortSide: 800 }).push, fast.push); // the speed, not dy
  const wild = at(1e6);
  near(wild.push, TUNING.maxScrollPush);
  near(at(-1e6).push, -TUNING.maxScrollPush);
  assert.ok(Number.isFinite(wild.agit));
  assert.ok(TUNING.maxScrollPush < TUNING.maxPush, 'gentler than a finger flick');
  // a finger flick of the same speed pushes harder than the scroll does
  const flick = pointerSplat({ dx: 0, dy: 60, dt: 0.05, shortSide: 800, pressed: false });
  assert.ok(fast.push < Math.hypot(flick.fx, flick.fy), `${fast.push} vs the flick`);
});
ok('scrollStir: nothing for dt 0, NaN, Infinity or a bad short side', () => {
  const zero = { push: 0, agit: 0 };
  for (const dt of [0, -1, NaN, Infinity, undefined]) {
    assert.deepEqual(scrollStir({ dy: 50, dt, shortSide: 800 }), zero, `dt ${dt}`);
  }
  for (const dy of [NaN, Infinity, -Infinity, undefined, 0]) {
    assert.deepEqual(scrollStir({ dy, dt: 0.05, shortSide: 800 }), zero, `dy ${dy}`);
  }
  for (const shortSide of [0, -5, NaN, Infinity, undefined]) {
    assert.deepEqual(scrollStir({ dy: 50, dt: 0.05, shortSide }), zero, `side ${shortSide}`);
  }
});
ok('wander: finite, in [0, 2 pi), and its mean direction has no bias over 10 000 steps', () => {
  const random = seeded(11);
  let angle = 1;
  let sin = 0;
  let cos = 0;
  for (let i = 0; i < 10000; i++) {
    angle = wander(angle, 4, random); // a step of about a radian
    assert.ok(Number.isFinite(angle) && angle >= 0 && angle < 2 * Math.PI, `${angle}`);
    sin += Math.sin(angle);
    cos += Math.cos(angle);
  }
  assert.ok(Math.abs(sin / 10000) < 0.05, `mean sin ${sin / 10000}`);
  assert.ok(Math.abs(cos / 10000) < 0.05, `mean cos ${cos / 10000}`);
  // it walks: a step is small at the frame's length and larger over a long time
  const step = Math.abs(wander(3, 0.05, seeded(5)) - 3);
  assert.ok(step > 0 && step < 0.5, `${step}`);
  // a random walk: the typical step grows with the square root of dt, and has no sign bias
  const spread = (dt) => {
    const rnd = seeded(9);
    let abs = 0;
    let sum = 0;
    for (let i = 0; i < 4000; i++) {
      const d = wander(3, dt, rnd) - 3;
      abs += Math.abs(d);
      sum += d;
    }
    return { abs: abs / 4000, mean: sum / 4000 };
  };
  const a = spread(0.04);
  const b = spread(0.16);
  assert.ok(b.abs / a.abs > 1.8 && b.abs / a.abs < 2.2, `ratio ${b.abs / a.abs}`);
  assert.ok(Math.abs(a.mean) < 0.1 * a.abs && Math.abs(b.mean) < 0.1 * b.abs, `${a.mean}`);
  for (const bad of [[NaN, 0.05], [1, NaN], [1, -1], [1, Infinity]]) {
    assert.ok(Number.isFinite(wander(...bad, seeded(1))), `${bad}`);
  }
});
ok('the resting source has no upward mean velocity over 600 s (40 seeded runs)', () => {
  let up = 0;
  let right = 0;
  const runs = 40;
  const steps = 12000;
  for (let seed = 1; seed <= runs; seed++) {
    const random = seeded(seed);
    let angle = 2 * Math.PI * random();
    for (let i = 0; i < steps; i++) {
      angle = wander(angle, 0.05, random);
      const { pull } = restSplat({ x: 10, y: 10 }, [400, 800], 1, 0.05, [1, 1, 1], angle);
      up += pull[1];
      right += pull[0];
    }
  }
  const mean = (sum) => sum / (runs * steps) / TUNING.restDrift; // as a share of the drift
  assert.ok(Math.abs(mean(up)) < 0.1, `up ${mean(up)}`);
  assert.ok(Math.abs(mean(right)) < 0.1, `right ${mean(right)}`);
});
console.log(`${n} model checks passed`);
