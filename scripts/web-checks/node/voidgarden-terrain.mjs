/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for Void Garden's land (themes/voidgarden/layers/land.js and its neighbours): the
// terrain's JS mirror equals the reference terrain's heights, river x and branch half-width at
// fixed points for three seeds (numbers recorded from the reference in fixtures/voidgarden-
// terrain.json), the seed's first world is the reference's, the chain of worlds is a pure function
// of the seed and flips without a seam; the trees and spires stand on the mirror, keep clear of the
// water, steep ground and the snow, thin evenly with the budget and follow the world's tree pair;
// the waterfalls fall off banks the mirror gives; the mood eases by time constants and the palette's
// colours are in range. Run: node voidgarden-terrain.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createTerrain, latticeHash, hash2, noise2, SEGMENT, WATER_LEVEL } from '../../../bins/svolderd/web/themes/voidgarden/layers/land.js';
import {
  GRIDS, LATHE, SINK, lathe, ribbons, spireShape, treesAround,
} from '../../../bins/svolderd/web/themes/voidgarden/layers/flora.js';
import { FALL_CELL, waterfallsOf, waterReach } from '../../../bins/svolderd/web/themes/voidgarden/layers/river.js';
import {
  ARCHETYPES, FLORA, landLayer, fragmentNormal, fragmentOct, fragmentStep, fragmentSteps,
} from '../../../bins/svolderd/web/themes/voidgarden/layers/land.js';
import {
  MOODS, MOOD_FOR, PROGRAMS, colourSpec, createMood, paletteProgram, sunDirection,
} from '../../../bins/svolderd/web/themes/voidgarden/layers/sky.js';
import { hashStr, mulberry } from '../../../bins/svolderd/web/themes/voidgarden/rng.js';
import {
  CACHE, FILL_PER_FRAME, RES, cacheBytes, cacheTexel, createLandCache, levelOct, levelStep, levelTexel,
  levelTile, slotOf, slotRects, tileWorld, windowOf,
} from '../../../bins/svolderd/web/themes/voidgarden/layers/land-cache.js';

const ref = JSON.parse(readFileSync(new URL('./fixtures/voidgarden-terrain.json', import.meta.url)));

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const near = (a, b, eps, what) => assert.ok(Math.abs(a - b) <= eps, `${what}: ${a} vs ${b}`);

ok('the rng matches the reference stream', () => {
  for (const { seed, first } of ref.mulberry) {
    const r = mulberry(seed);
    first.forEach((v, i) => assert.equal(r(), v, `seed ${seed} draw ${i}`));
  }
  assert.equal(hashStr('rv7'), hashStr('rv7'));
});

ok('the first world of a seed is the reference\'s: parameters, archetype, coast sides', () => {
  for (const c of ref.cases) {
    const t = createTerrain(c.seed, { hash: hash2 });
    c.riv.forEach((v, i) => near(t.params.riv[i], v, 1e-12, `riv ${i}`));
    c.ter.forEach((v, i) => near(t.params.ter[i], v, 1e-12, `ter ${i}`));
    c.ter2.forEach((v, i) => near(t.params.ter2[i], v, 1e-12, `ter2 ${i}`));
    assert.equal(t.entry(0).arch, c.arch, `seed ${c.seed} archetype`);
    assert.equal(t.entry(1).arch, c.arch);
    assert.equal(t.entry(0).b[1], c.sideA, `seed ${c.seed} side behind`);
    assert.equal(t.entry(1).b[1], c.sideB, `seed ${c.seed} side ahead`);
    assert.equal(t.entry(0).flora, c.flora, `seed ${c.seed} flora`);
  }
});

ok('the mirror is the reference terrain: river x, branch half-width and heights at 3 x 49 points', () => {
  let worst = 0;
  for (const c of ref.cases) {
    const t = createTerrain(c.seed, { hash: hash2 });
    for (const p of c.points) {
      near(t.riverX(p.z), p.riverX, 1e-9, `seed ${c.seed} riverX ${p.z}`);
      near(t.riverS(p.z), p.riverS, 1e-9, `seed ${c.seed} riverS ${p.z}`);
      near(t.height(p.x, p.z, 5), p.h5, 1e-9, `seed ${c.seed} h5 ${p.x},${p.z}`);
      near(t.height(p.x, p.z, 7), p.h7, 1e-9, `seed ${c.seed} h7 ${p.x},${p.z}`);
      worst = Math.max(worst, Math.abs(t.height(p.x, p.z, 5) - p.h5));
    }
  }
  assert.ok(worst < 1e-9);
});

ok('the seam height is the five-octave height and the river is the same function', () => {
  const t = createTerrain(7);
  assert.equal(t.height(12, -80), t.height(12, -80, 5));
  assert.equal(typeof t.riverX(-80), 'number');
  assert.equal(WATER_LEVEL, 0);
});

ok('the lattice hash is 24 integer bits, even, uncorrelated with its neighbours and pinned', () => {
  let sum = 0;
  let sq = 0;
  let near1 = 0;
  const N = 40000;
  for (let i = 0; i < N; i++) {
    const x = (i % 200) - 100;
    const y = Math.floor(i / 200) - 100;
    const h = latticeHash(x, y);
    assert.ok(h >= 0 && h < 1 && Number.isInteger(h * 16777216));
    sum += h;
    sq += h * h;
    near1 += h * latticeHash(x + 1, y);
  }
  near(sum / N, 0.5, 0.01, 'mean');
  near(Math.sqrt(sq / N - (sum / N) ** 2), Math.sqrt(1 / 12), 0.01, 'spread');
  near(near1 / N, 0.25, 0.01, 'neighbours');
  assert.equal(latticeHash(5, -9), latticeHash(5, -9));
  assert.notEqual(latticeHash(0, 1), latticeHash(1, 0));
  // the bits a GPU must reproduce
  const pinned = [[0, 0], [1, 2], [-3, 7], [12345, -6789], [-100, 100]].map(([x, y]) => latticeHash(x, y));
  assert.deepEqual(pinned, [0.9464108347892761, 0.8234747052192688, 0.888770580291748, 0.23886680603027344,
    0.1470433473587036]);
});

ok('the default terrain is the reference land in kind: the same spread of heights', () => {
  for (const c of ref.cases) {
    const mine = createTerrain(c.seed);
    const theirs = createTerrain(c.seed, { hash: hash2 });
    const stat = (t) => {
      let sum = 0;
      let sq = 0;
      let k = 0;
      for (let z = -1200; z < 0; z += 60) {
        for (let dx = -150; dx <= 150; dx += 15) {
          const h = t.height(t.riverX(z) + dx, z, 5);
          sum += h;
          sq += h * h;
          k++;
        }
      }
      return { mean: sum / k, sd: Math.sqrt(sq / k - (sum / k) ** 2) };
    };
    const a = stat(mine);
    const b = stat(theirs);
    assert.ok(Math.abs(a.mean - b.mean) < 4 && a.sd / b.sd > 0.6 && a.sd / b.sd < 1.7,
      `seed ${c.seed}: ${JSON.stringify(a)} against ${JSON.stringify(b)}`);
  }
  assert.equal(typeof noise2(0.25, 0.75), 'number');
});

ok('the chain of worlds is a pure function of the seed and differs between seeds', () => {
  const a = createTerrain(31);
  const b = createTerrain(31);
  const c = createTerrain(32);
  const names = (t) => [...Array(12).keys()].map((m) => t.entry(m).arch).join();
  assert.equal(names(a), names(b));
  assert.notEqual(names(a), names(c));
  for (let m = 2; m < 12; m++) assert.notEqual(a.entry(m).arch, a.entry(m - 1).arch, `world ${m}`);
  assert.equal(a.entry(5000).arch, a.entry(5000).arch);
});

ok('either boundary around a point gives the same land away from the flip', () => {
  const t = createTerrain(4242);
  for (const z of [-300, -900, -1700, -2500, -4000]) {
    const m = Math.floor(-z / SEGMENT);
    for (const dx of [-30, 0, 11, 40]) {
      const x = t.riverX(z) + dx;
      const behind = t.heightWith(t.refAt(z, m), x, z, 5);
      const ahead = t.heightWith(t.refAt(z, m + 1), x, z, 5);
      near(behind, ahead, 1e-9, `z ${z}`);
      near(behind, t.height(x, z, 5), 1e-9, `z ${z} nearest`);
    }
  }
});

ok('the land is continuous across a world flip: no step beyond the land\'s own', () => {
  const t = createTerrain(90125);
  const steps = (z0) => {
    let worst = 0;
    let last = null;
    for (let k = -60; k <= 60; k += 2) {
      const z = z0 + k;
      const h = t.height(t.riverX(z) + 20, z, 5);
      assert.ok(Number.isFinite(h));
      if (last !== null) worst = Math.max(worst, Math.abs(h - last));
      last = h;
    }
    return worst;
  };
  const flip = Math.max(...[1, 2, 3].map((m) => steps(-m * SEGMENT)));
  const plain = Math.max(...[1, 2, 3].map((m) => steps(-m * SEGMENT - 600)));
  assert.ok(flip <= 2.5 * plain + 1, `flip ${flip} against ${plain}`);
});


// ---------------------------------------------------------------- the seam
/** A stand-in for the vendored namespaces: every property is the stand-in, every call returns it,
 * so a layer's factory runs and builds with no GPU. */
const stand = () => new Proxy(function standIn() {}, {
  get: (_, key) => (key === 'then' ? undefined : stand()),
  apply: () => stand(),
  construct: () => stand(),
});

ok('a terrain a holder put on the context is the land layer\'s to replace at each build', () => {
  const ctx = { T: stand(), S: stand(), lib: stand(), u: stand(), sprites: stand() };
  const layer = landLayer(ctx);
  const mine = { height: () => 3, riverX: () => 0 };
  ctx.terrain = mine;
  layer.build(7);
  assert.notEqual(ctx.terrain, mine, 'a stand-in set before the build is not the ground after it');
  assert.equal(ctx.terrain.height(12, -80), createTerrain(7).height(12, -80, 5), 'the ground is the land\'s');
});

ok('ctx.terrain is the mirror of the seed built last: it follows build and survives a reseed', () => {
  const ctx = { T: stand(), S: stand(), lib: stand(), u: stand(), sprites: stand() };
  const layer = landLayer(ctx);
  layer.build(4242);
  const held = ctx.terrain;
  assert.ok(held && typeof held.height === 'function' && typeof held.riverX === 'function');
  const points = [[0, -50], [30, -400], [-60, -900], [12, -1500], [5, 40], [80, -2500]];
  const same = (seed) => {
    const mirror = createTerrain(seed);
    return points.every(([x, z]) => held.height(x, z) === mirror.height(x, z, 5)
      && held.riverX(z) === mirror.riverX(z));
  };
  assert.ok(same(4242));
  assert.ok(!same(7));
  layer.build(7);
  assert.equal(ctx.terrain, held, 'the seam object is the one holders keep');
  assert.ok(same(7) && !same(4242), 'the seam follows a new seed');
  assert.equal(held.height(12, -80), held.height(12, -80, 5));
});

// ---------------------------------------------------------------- the trees
const AHEAD = { x: 0, z: -1 };
const camAt = (t, z) => ({ x: t.riverX(z), z });

ok('the trees stand on the mirror: a foot at the ground less its sink, spires two below', () => {
  for (const seed of [7, 4242, 11]) {
    const t = createTerrain(seed);
    const found = treesAround(t, seed, camAt(t, -420), AHEAD);
    assert.ok(found.near.length + found.far.length > 20, `seed ${seed}: ${found.near.length} near trees`);
    for (const tree of found.near) near(tree.y, t.height(tree.x, tree.z, 4) - SINK, 1e-9, 'near foot');
    for (const tree of found.far) near(tree.y, t.height(tree.x, tree.z, 4) - 0.2, 1e-9, 'far foot');
    for (const tree of found.spires) near(tree.y, t.height(tree.x, tree.z, 4) - 2, 1e-9, 'spire foot');
  }
});

ok('no tree in the water or on its bank, on steep ground or over the snow line', () => {
  for (const seed of [7, 4242, 90125]) {
    const t = createTerrain(seed);
    const found = treesAround(t, seed, camAt(t, -900), AHEAD);
    const w = t.params.ter[3];
    for (const tree of [...found.near, ...found.far]) {
      const h = t.height(tree.x, tree.z, 4);
      assert.ok(h >= WATER_LEVEL + 0.3, `seed ${seed}: a tree at height ${h}`);
      assert.ok(h <= t.params.ter2[1] - 2, 'a tree over the snow line');
      const c = t.riverX(tree.z);
      const s = t.riverS(tree.z);
      const dw = Math.min(Math.abs(tree.x - c - s), Math.abs(tree.x - c + s));
      assert.ok(dw >= w * 1.45 - 1e-9, `seed ${seed}: a tree ${dw} from the channel`);
      const hx = t.height(tree.x + 1.5, tree.z, 3);
      const hz = t.height(tree.x, tree.z + 1.5, 3);
      assert.ok(1.5 / Math.hypot(h - hx, 1.5, h - hz) >= 0.8 - 1e-9, 'a tree on a steep slope');
      assert.ok(tree.kind >= 0 && tree.kind <= 6 && Number.isInteger(tree.kind));
    }
    for (const tree of found.spires) {
      assert.ok(t.height(tree.x, tree.z, 4) >= 0.3);
    }
  }
});

ok('the same seed and camera give the same trees, another seed other ones, another place shifts them', () => {
  const t = createTerrain(11);
  const a = treesAround(t, 11, camAt(t, -200), AHEAD);
  assert.ok(a.near.length > 50);
  assert.deepEqual(a, treesAround(createTerrain(11), 11, camAt(t, -200), AHEAD));
  const other = createTerrain(12);
  const c = treesAround(other, 12, camAt(other, -200), AHEAD);
  assert.notDeepEqual(a.near.slice(0, 5), c.near.slice(0, 5));
  const moved = treesAround(t, 11, { x: t.riverX(-240), z: -240 }, AHEAD);
  assert.notDeepEqual(a.near.slice(0, 5), moved.near.slice(0, 5));
});

ok('a smaller budget share keeps a subset of the trees, evenly', () => {
  const t = createTerrain(11);
  const full = treesAround(t, 11, camAt(t, -300), AHEAD, 1);
  const half = treesAround(t, 11, camAt(t, -300), AHEAD, 0.5);
  assert.ok(half.near.length < full.near.length && half.near.length > full.near.length * 0.3,
    `${half.near.length} of ${full.near.length}`);
  const keys = new Set(full.near.map((x) => `${x.x},${x.z}`));
  for (const x of half.near) assert.ok(keys.has(`${x.x},${x.z}`), 'a tree not in the full set');
  const cells = GRIDS.near.n * GRIDS.near.n;
  assert.ok(full.near.length <= cells && full.far.length <= GRIDS.far.n * GRIDS.far.n);
  assert.ok(full.spires.length <= GRIDS.spire.n * GRIDS.spire.n);
});

ok('the kinds follow the world\'s tree pair, the shore kind taking over near the water', () => {
  let checked = 0;
  for (const seed of [11, 7, 4242, 90125, 5, 6, 8, 9]) {
    const t = createTerrain(seed);
    const found = treesAround(t, seed, camAt(t, -200), AHEAD);
    const pair = FLORA[t.entry(1).flora].f;
    const kinds = new Set(found.near.map((x) => x.kind));
    for (const k of kinds) assert.ok(k === pair[1] || k === pair[2], `seed ${seed}: kind ${k} of ${pair}`);
    if (pair[0] < 0.9 && pair[1] !== pair[2] && found.near.length > 100) {
      const w = t.params.ter[3];
      const share = (lo, hi) => {
        let a = 0;
        let n = 0;
        for (const x of found.near) {
          const dw = Math.min(Math.abs(x.x - t.riverX(x.z) - t.riverS(x.z)), Math.abs(x.x - t.riverX(x.z) + t.riverS(x.z)));
          if (dw >= lo * w && dw < hi * w) { n++; if (x.kind === pair[1]) a++; }
        }
        return n > 10 ? a / n : null;
      };
      const shore = share(1.45, 2.5);
      const inland = share(5, 99);
      if (shore !== null && inland !== null) {
        assert.ok(shore <= inland + 1e-9, `seed ${seed}: kind A ${shore} at the shore, ${inland} inland`);
        checked++;
      }
    }
  }
  assert.ok(checked > 0, 'no seed had both a shore and inland sample');
});

ok('every shape is finite, closed and indexed in range', () => {
  for (const kind of [0, 1, 2, 5, 6]) {
    const s = lathe(kind);
    assert.equal(s.position.length, LATHE.rings * LATHE.around * 3);
    assert.ok(s.position.every(Number.isFinite));
    assert.equal(s.bark.length, LATHE.rings * LATHE.around);
  }
  for (const kind of [3, 4, 6]) {
    const s = ribbons(kind);
    assert.ok(s.position.length > 60 && s.position.every(Number.isFinite), `ribbons ${kind}`);
    assert.ok(Math.max(...s.index) < s.position.length / 3 && s.bark.length === s.position.length / 3);
    assert.ok(Math.max(...s.position.filter((_, i) => i % 3 === 1)) < 1.4, 'a tree taller than its unit');
  }
  assert.ok(spireShape().position.every(Number.isFinite));
});

// ---------------------------------------------------------------- the waterfalls
ok('a waterfall falls from a rim of the mirror\'s into the channel, the same ones every time', () => {
  let seen = 0;
  for (const seed of [7, 4242, 90125, 11, 3, 5]) {
    const t = createTerrain(seed);
    const falls = waterfallsOf(t, seed, 30, -2400);
    assert.deepEqual(falls, waterfallsOf(createTerrain(seed), seed, 30, -2400));
    for (const f of falls) {
      seen++;
      assert.ok(f.z0 <= 30 && f.z0 >= -2400 && f.w >= 0.9 && f.w <= 2.3);
      assert.equal(f.y1, 0.05);
      const rim = [-1, 1].map((side) => t.height(f.x0 + side * 0.6, f.z0, 5) - 0.4);
      assert.ok(rim.some((y) => Math.abs(y - f.y0) < 1e-9) && f.y0 > 5.5, 'the rim is on the mirror');
      assert.ok([-1, 1].some((side) => t.height(f.x1 - side * 0.6, f.z0, 5) <= 1.2), 'the foot is at the channel');
    }
    const some = waterfallsOf(t, seed, 30, -2400);
    for (let i = 1; i < some.length; i++) assert.ok(some[i - 1].z0 >= some[i].z0, 'nearest first');
  }
  assert.ok(seen > 0, 'no seed had a fall');
  // a delta never has one
  for (let seed = 1; seed < 400; seed++) {
    const t = createTerrain(seed);
    if (t.entry(0).wf === 0 && t.entry(1).wf === 0) {
      assert.deepEqual(waterfallsOf(t, seed, 30, -1000).filter((f) => f.z0 > -1000 + 0), []);
      return;
    }
  }
  assert.fail('no seed had two worlds without waterfalls');
});

ok('the water reaches the branch, the basin and a coast out to the horizon', () => {
  const t = createTerrain(11);
  const r = waterReach(t, -300);
  assert.ok(r.left > 8 && r.right > 8 && Number.isFinite(r.left + r.right));
  for (let seed = 1; seed < 200; seed++) {
    const c = createTerrain(seed);
    if (ARCHETYPES[c.entry(0).arch].b[1] !== 0 && c.entry(0).b[1] === 1 && c.entry(1).b[1] === 1) {
      const reach = waterReach(c, -300);
      assert.equal(reach.right, 420);
      assert.ok(reach.left < 420);
      return;
    }
  }
  assert.fail('no seed had a coast on the right');
});

// ---------------------------------------------------------------- the mood and the palette
ok('the mood eases by time constants: the same seconds at 30, 60 and 144 steps give one mood', () => {
  const run = (rate) => {
    const m = createMood({ seed: 5 });
    m.step(0, 'intro', 0.3);
    for (let i = 0; i < 20 * rate; i++) m.step(1 / rate, i < 8 * rate ? 'intro' : 'drop', 0.7);
    return { ...m.M };
  };
  const a = run(30);
  for (const rate of [60, 144]) {
    const b = run(rate);
    for (const k of Object.keys(a)) if (typeof a[k] === 'number') near(a[k], b[k], 1e-9, `${k} at ${rate}`);
  }
});

ok('a section change brings the next mood of its bag, never the same twice', () => {
  const m = createMood({ seed: 9 });
  m.step(0, 'intro', 0.3);
  const seen = [m.index];
  for (const section of ['drop', 'break', 'build', 'drop', 'intro', 'break']) {
    m.step(1, section, 0.5);
    assert.ok(MOOD_FOR[section].includes(m.index), `${section} gave mood ${m.index}`);
    seen.push(m.index);
  }
  for (let i = 1; i < seen.length; i++) assert.notEqual(seen[i], seen[i - 1]);
  // the same seed gives the same moods
  const again = createMood({ seed: 9 });
  again.step(0, 'intro', 0.3);
  assert.equal(again.index, seen[0]);
});

ok('a loud burst under 3.5 s moves no mood; a held one does', () => {
  const quiet = () => {
    const m = createMood({ seed: 3 });
    for (let i = 0; i < 30; i++) m.step(1, null, 0.3);
    return m;
  };
  const burst = quiet();
  const was = burst.index;
  for (let i = 0; i < 3; i++) burst.step(1, null, 0.8);
  burst.step(1, null, 0.3);
  assert.equal(burst.index, was);
  assert.equal(burst.section, 'build');
  const held = quiet();
  for (let i = 0; i < 6; i++) held.step(1, null, 0.8);
  assert.equal(held.section, 'drop');
  assert.ok(MOOD_FOR.drop.includes(held.index));
});

ok('every mood and program gives finite colours in range; the sun is a unit vector', () => {
  for (const name of PROGRAMS) {
    for (const [a, b] of [[0.55, 0.04], [0.7, 0.2], [0.02, 0.5]]) {
      const p = paletteProgram(name, a, b, 100);
      assert.equal(p.band.length, 3);
      for (const M of MOODS) {
        const spec = colourSpec(M, p);
        for (const [key, [h, sat, light, k]] of Object.entries(spec)) {
          assert.ok([h, sat, light, k].every(Number.isFinite), `${name} ${M.name} ${key}`);
          assert.ok(sat >= 0 && sat <= 1 && light >= 0 && light <= 1, `${name} ${M.name} ${key} ${sat} ${light}`);
        }
      }
    }
  }
  for (const yaw of [-1, 0, 0.7]) {
    for (const M of MOODS) near(Math.hypot(...sunDirection(yaw, M.az, M.el)), 1, 1e-12, 'sun');
  }
  assert.equal(FALL_CELL, 90);
});

const angleOf = (a, b) => (Math.acos(Math.max(-1, Math.min(1, a[0] * b[0] + a[1] * b[1] + a[2] * b[2]))) * 180) / Math.PI;

ok('a fragment takes the mockup\'s octaves by distance, never by ring, and the budget\'s top by its step', () => {
  assert.deepEqual([1, 0.75, 0.55, 0.4].map((s) => fragmentSteps(s).top), [8, 8, 7, 6]);
  assert.deepEqual([1, 0.75, 0.55, 0.4].map((s) => fragmentSteps(s).det), [1, 0.7, 0.45, 0]);
  for (const top of [8, 7, 6]) {
    let last = top;
    for (let d = 0; d < 3000; d += 0.5) {
      const o = fragmentOct(top, d);
      assert.equal(o, Math.min(top, Math.max(3, top - Math.floor(Math.log2(1 + 0.02 * d)))), `top ${top} d ${d}`);
      assert.ok(o <= last && o >= 3, 'never more with the distance, never under 3');
      last = o;
    }
    assert.equal(fragmentOct(top, 0), top);
  }
  assert.equal(fragmentOct(8, 49), 8);
  assert.equal(fragmentOct(8, 51), 7);
  assert.equal(fragmentOct(8, 1e6), 3);
});

ok('the fragment normal is continuous across a ring\'s edge (it reads the distance, not the ring)', () => {
  // the edges of the three rings' squares for a camera at the origin, 3 seeds x 309 land points; the
  // pair sits on either side of the edge, 0.02 apart. The octave count steps at its own shells
  // (distance 50, 150, 350 ...), which no ring edge is: pairs on a shell are left out.
  const shells = [0, 1, 2, 3, 4, 5].map((k) => (2 ** k - 1) / 0.02);
  const worst = [];
  for (const seed of [7, 4242, 90125]) {
    const t = createTerrain(seed);
    const cz = -300;
    const cam = [t.riverX(cz), 9, cz];
    let count = 0;
    for (const half of [72, 180, 420]) {
      for (let i = 0; i < 103; i++) {
        const along = -half + (2 * half * (i + 0.5)) / 103;
        const edges = [[along, -half], [half, along], [along, half]];
        for (const [ex, ez] of edges) {
          const a = [cam[0] + ex - 0.01 * Math.sign(ex || 1), cz + ez];
          const b = [cam[0] + ex + 0.01 * Math.sign(ex || 1), cz + ez];
          const d = (p) => Math.hypot(p[0] - cam[0], t.height(p[0], p[1], 5) - cam[1], p[1] - cam[2]);
          if (shells.some((r) => Math.abs(d(a) - r) < 0.5 || Math.abs(d(b) - r) < 0.5)) continue;
          assert.equal(fragmentOct(8, d(a)), fragmentOct(8, d(b)), 'one octave count on both sides');
          const na = fragmentNormal(t, a[0], a[1], d(a), 8);
          const nb = fragmentNormal(t, b[0], b[1], d(b), 8);
          worst.push(angleOf(na, nb));
          count++;
        }
      }
    }
    assert.ok(count > 300, `seed ${seed}: ${count} pairs`);
  }
  worst.sort((x, y) => x - y);
  const p90 = worst[Math.floor(worst.length * 0.9)];
  // the ring-count version measured p90 8.6 deg across such an edge; two points 0.02 apart on the
  // one function differ by the land's own slope (a cliff in the river's wall is the long tail)
  assert.ok(worst[Math.floor(worst.length / 2)] < 0.3 && p90 < 1.5, `median ${worst[Math.floor(worst.length / 2)]}, p90 ${p90}`);
});

ok('the normal step never falls under 16 ulps of z, so a long flight stays clean (z -400000)', () => {
  for (const z of [-20, -36000, -150000, -400000]) {
    for (const dist of [5, 20, 80]) {
      const e = fragmentStep(dist, z);
      assert.ok(e >= 0.04 + 0.003 * dist - 1e-12 && e >= Math.abs(z) * 2e-6 - 1e-12);
      const f32 = Math.fround(z + e) - Math.fround(z);
      assert.ok(Math.abs(f32 - e) / e < 0.08, `z ${z} d ${dist}: step ${e} lands as ${f32}`);
    }
  }
  // the normal at 400 km on float coordinates is the normal on exact ones; with the old step
  // (.04 + .003 d) it was not
  const t = createTerrain(7);
  const z0 = -400000;
  let worstNew = 0;
  let worstOld = 0;
  for (let i = 0; i < 60; i++) {
    const x = -30 + i;
    const z = z0 - i * 0.37;
    const dist = 12;
    const zq = Math.fround(z);
    const sample = (e, rounded) => {
      const h = t.height(x, zq, 5);
      const nx = h - t.height(x + e, zq, 5);
      const nz = h - t.height(x, rounded ? Math.fround(zq + e) : zq + e, 5);
      const len = Math.hypot(nx, e, nz);
      return [nx / len, e / len, nz / len];
    };
    const exact = (e) => sample(e, false);
    const real = (e) => sample(e, true);
    const eNew = fragmentStep(dist, z);
    const eOld = 0.04 + 0.003 * dist;
    worstNew = Math.max(worstNew, angleOf(exact(eNew), real(eNew)));
    worstOld = Math.max(worstOld, angleOf(exact(eOld), real(eOld)));
  }
  assert.ok(worstNew < 5, `float coordinates move the normal ${worstNew} deg`);
  assert.ok(worstNew <= worstOld, `floored ${worstNew} deg, fixed .04 step ${worstOld} deg`);
});

// ---------------------------------------------------------------- the terrain cache
ok('the cache\'s textures stay under 32 MB, float or packed, five levels of 512 x 512 texels', () => {
  assert.equal(RES, 512);
  assert.equal(CACHE.levels, 5);
  assert.equal(cacheBytes(false), 5 * 512 * 512 * 16);
  assert.ok(cacheBytes(false) <= 32 * 2 ** 20, `${cacheBytes(false)} bytes`);
  assert.ok(cacheBytes(true) < cacheBytes(false));
  for (let k = 1; k < CACHE.levels; k++) assert.equal(levelTexel(k), 2 * levelTexel(k - 1));
  assert.equal(levelTile(0), 32 * 0.125);
});

ok('a level\'s octaves follow the budget\'s most and the mockup\'s by distance where the level serves', () => {
  assert.deepEqual([0, 1, 2, 3, 4].map((k) => levelOct(8, k)), [8, 8, 7, 7, 6]);
  assert.deepEqual([0, 1, 2, 3, 4].map((k) => levelOct(7, k)), [7, 7, 6, 6, 5]);
  assert.deepEqual([0, 1, 2, 3, 4].map((k) => levelOct(6, k)), [6, 6, 5, 5, 4]);
  for (const top of [8, 7, 6, 3]) {
    for (let k = 0; k < CACHE.levels; k++) {
      assert.ok(levelOct(top, k) <= top && levelOct(top, k) >= 3);
      if (k) assert.ok(levelOct(top, k) <= levelOct(top, k - 1), 'never more octaves farther out');
    }
  }
  // where a level begins to serve beside the camera (its finer neighbour's shown half), the mockup's count
  // is the level's or one fewer
  for (let k = 1; k < CACHE.levels; k++) {
    const from = (CACHE.shown / 2) * levelTile(k - 1);
    const r9 = fragmentOct(8, from);
    assert.ok(levelOct(8, k) === r9 || levelOct(8, k) === r9 + 1, `level ${k} from ${from}: ${levelOct(8, k)} vs ${r9}`);
  }
});

ok('a level\'s normal step is half its texel, never under 16 ulps of z (z -400000)', () => {
  for (let k = 0; k < CACHE.levels; k++) {
    for (const z of [-20, -36000, -150000, -400000]) {
      const e = levelStep(k, z);
      assert.ok(e >= levelTexel(k) / 2 && e >= Math.abs(z) * 2e-6 - 1e-12);
      const f32 = Math.fround(z + e) - Math.fround(z);
      assert.ok(Math.abs(f32 - e) / e < 0.08, `level ${k} z ${z}: step ${e} lands as ${f32}`);
    }
  }
});

/** The corners of a camera's view on the ground out to its far plane (400), as xz points. */
const viewFan = (x, z, yaw, hfov) => {
  const out = [];
  for (let i = 0; i <= 16; i++) {
    const a = yaw - hfov / 2 + (hfov * i) / 16;
    for (const d of [5, 50, 150, 300, 400]) out.push([x + Math.sin(a) * d, z - Math.cos(a) * d]);
  }
  return out;
};
const shownOf = (k, w) => [(w.cx - CACHE.shown / 2) * levelTile(k), (w.cx + CACHE.shown / 2) * levelTile(k),
  (w.cz - CACHE.shown / 2) * levelTile(k), (w.cz + CACHE.shown / 2) * levelTile(k)];

ok('the windows nest, sit ahead of the camera and the coarsest holds all it sees to the far plane', () => {
  const r = mulberry(17);
  for (let i = 0; i < 400; i++) {
    const x = (r() - 0.5) * 400;
    const z = -r() * 400000;
    const yaw = (r() - 0.5) * 1.6;
    const fx = Math.sin(yaw);
    const fz = -Math.cos(yaw);
    const rects = [];
    for (let k = 0; k < CACHE.levels; k++) rects.push(shownOf(k, windowOf(k, x, z, fx, fz)));
    for (let k = 0; k < CACHE.levels; k++) {
      const [x0, x1, z0, z1] = rects[k];
      assert.ok(x0 < x && x < x1 && z0 < z && z < z1, `level ${k} holds the camera`);
      if (k) {
        const [a0, a1, b0, b1] = rects[k - 1];
        assert.ok(a0 >= x0 && a1 <= x1 && b0 >= z0 && b1 <= z1, `level ${k - 1} inside level ${k}`);
      }
    }
    // 88 degrees across: a 16:9 screen at the scene's 55 degree height
    const [x0, x1, z0, z1] = rects[CACHE.levels - 1];
    for (const [px, pz] of viewFan(x, z, yaw, 1.54)) {
      assert.ok(px > x0 && px < x1 && pz > z0 && pz < z1, `(${px}, ${pz}) seen from (${x}, ${z})`);
    }
  }
});

ok('at flight speed a window moves one tile a frame at most: the ring drawn ahead is in time', () => {
  // 16 units a second at 30 frames, turning by .05 a frame: the finest window moves the most
  let last = null;
  let yaw = 0;
  for (let f = 0; f < 3000; f++) {
    yaw += 0.05 * Math.sin(f / 40);
    const x = 30 * Math.sin(f / 90);
    const z = -f * (16 / 30);
    const w = windowOf(0, x, z, Math.sin(yaw), -Math.cos(yaw));
    if (last) assert.ok(Math.abs(w.cx - last.cx) <= 1 && Math.abs(w.cz - last.cz) <= 1, `frame ${f}`);
    last = w;
  }
});

ok('a tile is drawn in the world nearest its centre, which is the mirror\'s world at every texel', () => {
  for (const seed of [7, 4242, 90125]) {
    const t = createTerrain(seed);
    for (let k = 0; k < CACHE.levels; k++) {
      // the tile rows around the first three flips and their midpoints
      for (const z of [-50, -900, -1800, -2700, -3600, -5400]) {
        const tz = Math.floor(z / levelTile(k));
        const ref = t.refAt(0, tileWorld(k, tz));
        for (let j = 0; j < CACHE.tile; j += 7) {
          const gz = tz * CACHE.tile + j;
          const zz = (gz + 0.5) * levelTexel(k);
          for (const xx of [t.riverX(zz) + 7, t.riverX(zz) - 23]) {
            assert.equal(t.heightWith(ref, xx, zz, 6), t.height(xx, zz, 6), `seed ${seed} level ${k} z ${zz}`);
          }
        }
      }
    }
  }
});

ok('a texel is a function of its place: its height is the mirror\'s and its normal the mockup\'s three samples', () => {
  const t = createTerrain(7);
  for (let k = 0; k < CACHE.levels; k++) {
    for (const [gx, gz] of [[3, -1400], [-40, -9000], [17, 3]]) {
      const a = cacheTexel(t, k, gx, gz, 8);
      const x = (gx + 0.5) * levelTexel(k);
      const z = (gz + 0.5) * levelTexel(k);
      assert.equal(a.h, t.height(x, z, levelOct(8, k)));
      assert.ok(Math.abs(Math.hypot(...a.n) - 1) < 1e-12 && a.n[1] > 0);
      assert.deepEqual(cacheTexel(t, k, gx, gz, 8), a, 'the same texel twice');
    }
  }
});

ok('the tiles to draw become slot rectangles that cover them exactly, a shown square in four at most', () => {
  const cover = (list) => {
    const rects = slotRects(list);
    const seen = new Set();
    for (const r of rects) {
      for (let y = r.y0; y < r.y1; y++) {
        for (let x = r.x0; x < r.x1; x++) {
          const key = `${x},${y}`;
          assert.ok(!seen.has(key), 'no slot twice');
          seen.add(key);
        }
      }
    }
    assert.deepEqual([...seen].sort(), list.map((t) => `${slotOf(t.tx)},${slotOf(t.tz)}`).sort());
    return rects;
  };
  for (const [cx, cz] of [[0, 0], [5, -3], [-77, -1201], [8, 8]]) {
    const square = [];
    for (let tz = cz - 7; tz < cz + 7; tz++) for (let tx = cx - 7; tx < cx + 7; tx++) square.push({ tx, tz });
    assert.ok(cover(square).length <= 4);
    const row = square.filter((t) => t.tz === cz - 7);
    assert.ok(cover(row).length <= 2);
  }
  cover([{ tx: 3, tz: 4 }, { tx: 9, tz: -40 }, { tx: 4, tz: 4 }]);
});

// the cache's bookkeeping on a renderer that records what it is asked: real three nodes, no GPU
const T = await import('../../../bins/svolderd/web/themes/voidgarden/vendor/three.webgpu.js');
const S = await import('../../../bins/svolderd/web/themes/voidgarden/vendor/three.tsl.js');
const { terrainNodes } = await import('../../../bins/svolderd/web/themes/voidgarden/layers/land.js');
const { shading } = await import('../../../bins/svolderd/web/themes/voidgarden/layers/sky.js');

/** A cache on a recording renderer of `tier`: each pass is `pass <width>x<height>`, each flush
 * `flush`. */
function recordedCache(tier) {
  const order = [];
  let target = null;
  const renderer = {
    autoClear: true,
    backend: tier === 'webgl2' ? { gl: { flush: () => order.push('flush') } } : {},
    getRenderTarget: () => target,
    setRenderTarget: (rt) => { target = rt; },
    render: () => {
      assert.equal(renderer.autoClear, false, 'a tile never clears its target');
      order.push(`pass ${target.viewport.z}x${target.viewport.w}`);
    },
  };
  const ctx = { T, S, gpu: { renderer }, caps: { tier, floatTargets: true } };
  ctx.lib = { uniformBlock: () => ({}) };
  ctx.u = {};
  const sh = shading(ctx);
  const nodes = terrainNodes(ctx);
  return { cache: createLandCache(ctx, nodes, sh), order, renderer };
}
const flight = (t, z, yaw = 0) => ({
  cam: { x: t.riverX(z), y: 9, z, lookX: t.riverX(z) + Math.sin(yaw) * 34, lookY: 4, lookZ: z - Math.cos(yaw) * 34 },
});

ok('a seek or a new seed draws what is shown at once; the flight then draws a few tiles a frame', () => {
  for (const tier of ['webgl2', 'webgpu']) {
    const { cache, order } = recordedCache(tier);
    const t = createTerrain(7);
    cache.update(flight(t, -100), t);
    let s = cache.stats();
    assert.equal(s.sync, CACHE.levels, 'every level drawn before the first frame');
    const shownTiles = CACHE.levels * CACHE.shown * CACHE.shown;
    assert.equal(s.tiles, shownTiles + FILL_PER_FRAME, 'then the budget\'s few of the ring');
    for (const L of cache.levels) {
      assert.deepEqual(L.shown, { x0: L.cx - 7, x1: L.cx + 7, z0: L.cz - 7, z1: L.cz + 7 });
    }
    // flying on: a few a frame, never a level drawn at once
    for (let f = 1; f <= 600; f++) {
      const before = cache.stats().tiles;
      cache.update(flight(t, -100 - f * (16 / 30), 0.3 * Math.sin(f / 50)), t);
      assert.ok(cache.stats().tiles - before <= FILL_PER_FRAME, `frame ${f}`);
    }
    s = cache.stats();
    assert.equal(s.sync, CACHE.levels, 'the flight never stalls a frame');
    // standing still: the windows are full and nothing is drawn
    const still = flight(t, -100 - 600 * (16 / 30));
    for (let f = 0; f < 80; f++) cache.update(still, t);
    const idle = cache.stats().tiles;
    cache.update(still, t);
    assert.equal(cache.stats().tiles, idle, 'a still camera draws nothing');
    // a seek far ahead: drawn at once again
    cache.update(flight(t, -40000), t);
    assert.equal(cache.stats().sync, 2 * CACHE.levels);
    // another seed: no tile of the last is kept
    const u = createTerrain(8);
    cache.update(flight(u, -40000), u);
    assert.equal(cache.stats().sync, 3 * CACHE.levels);
    // the cache is keyed on the seed: a new terrain object of the same seed keeps every tile
    const syncs = cache.stats().sync;
    const before = cache.stats().tiles;
    cache.update(flight(u, -40000), createTerrain(8));
    assert.equal(cache.stats().sync, syncs, 'a rebuilt terrain of the seed keeps the live cache');
    assert.ok(cache.stats().tiles - before <= FILL_PER_FRAME);
    // reset drops the tiles (a lost context): the next update draws what is shown at once
    cache.reset();
    cache.update(flight(u, -40000), u);
    assert.equal(cache.stats().sync, syncs + CACHE.levels, 'a reset cache is drawn again at once');
    // the GL is flushed after every pass on WebGL2, never on WebGPU
    const passes = order.filter((o) => o.startsWith('pass')).length;
    assert.ok(passes > 0);
    if (tier === 'webgl2') {
      order.forEach((o, i) => { if (o.startsWith('pass')) assert.equal(order[i + 1], 'flush', `pass ${i}`); });
    } else {
      assert.ok(!order.includes('flush'));
    }
  }
});

console.log(`voidgarden-terrain.mjs: ${n} passed`);
