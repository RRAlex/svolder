/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's land layer: the terrain, its shading, and the JS mirror the other layers place by.
// The terrain is one function of the world, `height(x, z)`: eroded noise with derivatives, carved
// by a winding river that may fork, shaped by the world's archetype (canyon, mesas, coast, ...).
// The same function runs here (pure, in doubles: the camera's clearance, where a tree stands, where
// a waterfall falls) and in the vertex stage (`terrainNodes`, in TSL, on nested world-snapped
// grids), so what is placed on the CPU sits on what is drawn. A seed fixes the river, the heights
// and a chain of worlds along the flight; each world is blended into the next over 100 units.
//
//   createTerrain(seed)    -> the mirror: `{ params, entry(m), refAt(z, m?), blendAt(z),
//                          height(x, z, oct = 5), heightWith(ref, x, z, oct), riverX(z),
//                          riverS(z) }`
//   SEGMENT, WATER_LEVEL   the distance between two worlds along z, the water's height (0)
//   ARCHETYPES, FLORA, WATER   the world tables
//   terrainNodes(ctx)      the same function in TSL (`riverX`, `riverS`, `height(oct)`) and its
//                          world uniforms
//   landLayer(ctx)         the layer factory: sets `ctx.terrain = { height, riverX }` and
//                          `ctx.land` (the mirror, the terrain's nodes, `sync(frame)` which
//                          writes the world's uniforms, the rings' placement)

import { hashStr, mulberry } from '../rng.js';
import { grooveOf } from '../tsl/post.js';
import { createLandCache } from './land-cache.js';
import { shading } from './sky.js';

/** The water's height: land under it is river bed. */
export const WATER_LEVEL = 0;

/** The distance between the flips of the world along the flight, units. */
export const SEGMENT = 1800;

/** The half-length of a flip: the old world is blended into the new over twice this. */
const FLIP = 50;

const TAU = Math.PI * 2;
const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);
const lerp = (a, b, t) => a + (b - a) * t;
const fr = (x) => x - Math.floor(x);
/** Smoothstep from `a` to `b` (either way round) at `x`. */
const sst = (a, b, x) => {
  const t = clamp((x - a) / (b - a));
  return t * t * (3 - 2 * t);
};

/** The worlds: `a` fork amount, basin width, mesa terraces, islets; `b` canyon steepness, coast
 * side (0 none, else set to -1 or 1 by the seed), plateau lift, fork frequency; `c` rift, spires,
 * cliff steps, forest density; `wf` the chance of a waterfall per cell. */
export const ARCHETYPES = Object.freeze([
  { name: 'valley', a: [0.5, 0, 0, 0.25], b: [0, 0, 0, 0.005], c: [0.3, 0.2, 0.3, 0.85], wf: 0.3 },
  { name: 'canyon', a: [0.3, 0, 0.3, 0], b: [1, 0, 3, 0.004], c: [0.6, 0.5, 0.7, 0.25], wf: 0.8 },
  { name: 'braided delta', a: [1, 0.55, 0, 0.35], b: [0, 0, 0, 0.008], c: [0, 0, 0, 0.6], wf: 0 },
  { name: 'archipelago', a: [0.7, 1.7, 0, 1], b: [0, 0, 0, 0.005], c: [0, 0.6, 0.2, 0.9], wf: 0.2 },
  { name: 'mesas', a: [0.4, 0.35, 1, 0], b: [0.35, 0, 3.5, 0.004], c: [0.3, 0.8, 0.5, 0.2],
    wf: 0.5 },
  { name: 'coast', a: [0.55, 0.5, 0, 0.7], b: [0, 1, 0, 0.005], c: [0.2, 0.4, 0.8, 0.5], wf: 0.5 },
  { name: 'gorge country', a: [0.35, 0, 0, 0.1], b: [0.6, 0, 1.5, 0.004], c: [1, 0.3, 0.6, 0.4],
    wf: 0.9 },
  { name: 'spire forest', a: [0.5, 0.3, 0, 0.3], b: [0.2, 0, 0, 0.005], c: [0.1, 1, 0.1, 1],
    wf: 0.2 },
  { name: 'highland woods', a: [0.45, 0.15, 0, 0.15], b: [0.3, 0, 2, 0.005], c: [0.2, 0.15, 1,
    0.95], wf: 0.6 },
  { name: 'fjord', a: [0.3, 0.2, 0, 0.4], b: [0.9, 1, 0, 0.004], c: [0.8, 0.3, 0.4, 0.55],
    wf: 0.7 },
]);

/** The tree pairs: share of kind A, kind A, kind B (the shore kind), size. Kinds: 0 pine,
 * 1 cypress, 2 broadleaf, 3 bare, 4 palm, 5 crystal, 6 willow. */
export const FLORA = Object.freeze([
  { name: 'pine forest', f: [0.85, 0, 6, 1] }, { name: 'cypress groves', f: [0.8, 1, 2, 1.05] },
  { name: 'broadleaf wood', f: [0.85, 2, 6, 0.95] }, { name: 'winter woods', f: [0.6, 3, 0, 1] },
  { name: 'palm shore', f: [0.55, 2, 4, 1] }, { name: 'crystal grove', f: [0.6, 5, 3, 1] },
  { name: 'willow banks', f: [0.7, 0, 6, 1] }, { name: 'mixed wood', f: [0.55, 0, 2, 1] },
  { name: 'dead pines', f: [0.65, 3, 1, 1.05] },
]);

/** The water styles: chop, flow, dark horizon, glow. */
export const WATER = Object.freeze([
  { name: 'mirror', w: [0.3, 0.4, 1, 0] }, { name: 'choppy', w: [1.4, 1.2, 0.6, 0] },
  { name: 'glass river', w: [0.6, 0.8, 0.9, 0] }, { name: 'glowing', w: [0.7, 0.6, 0.8, 1] },
]);

/** The next of `list` from a shuffled bag (`bag` is refilled when empty), never `avoid` right after
 * it. */
function bagNext(bag, list, rng, avoid) {
  if (!bag.length) {
    const a = list.slice();
    for (let i = a.length - 1; i > 0; i--) {
      const j = Math.floor(rng() * (i + 1));
      [a[i], a[j]] = [a[j], a[i]];
    }
    if (a[0] === avoid && a.length > 1) a.push(a.shift());
    bag.push(...a);
  }
  return bag.shift();
}

const indices = (n) => Array.from({ length: n }, (_, i) => i);

/** The float lattice hash of two numbers, in [0, 1): what places a tree in a cell (a hash of the
 * shape a GPU computes without an integer, whose last digits differ from one GPU to the next, so
 * only the CPU reads it). */
export function hash2(x, y) {
  let a = fr(x * 0.1031);
  let b = fr(y * 0.1031);
  const c = a;
  const d = a * (b + 33.33) + b * (c + 33.33) + c * (a + 33.33);
  a += d;
  b += d;
  return fr((a + b) * (c + d));
}

/** The integer lattice hash of two integers, in [0, 1) with 24 bits: all in 32-bit integer
 * arithmetic, so a GPU computes the same bits (`terrainNodes`), on every tier and device. The
 * terrain's noise reads it. */
export function latticeHash(ix, iy) {
  let h = (Math.imul(ix | 0, 0x1f3d5b79) + Math.imul(iy | 0, 0x2a6e4c0b) + 0x6b43a9b5) | 0;
  h ^= h >>> 15;
  h = Math.imul(h, 0x2c1b3c6d);
  h ^= h >>> 12;
  h = Math.imul(h, 0x297a2d39);
  h ^= h >>> 15;
  return (h >>> 8) / 16777216;
}

/** Derivative noise of (x, y) over the lattice `hash`: `out` gets [value, d/dx, d/dy]; returns the
 * value. */
export function noise2(x, y, out = [0, 0, 0], hash = latticeHash) {
  const ix = Math.floor(x);
  const iy = Math.floor(y);
  const fx = x - ix;
  const fy = y - iy;
  const ux = fx * fx * fx * (fx * (fx * 6 - 15) + 10);
  const uy = fy * fy * fy * (fy * (fy * 6 - 15) + 10);
  const a = hash(ix, iy);
  const b = hash(ix + 1, iy);
  const c = hash(ix, iy + 1);
  const d = hash(ix + 1, iy + 1);
  const k = a - b - c + d;
  out[1] = 30 * fx * fx * (fx * (fx - 2) + 1) * (b - a + k * uy);
  out[2] = 30 * fy * fy * (fy * (fy - 2) + 1) * (c - a + k * ux);
  out[0] = a + (b - a) * ux + (c - a) * uy + k * ux * uy;
  return out[0];
}

/** The river's lateral frequencies. */
const RIVER_F = Object.freeze([0.0105, 0.023, 0.0047]);

/**
 * The terrain of `seed`: its parameters (the river's phases and swing, the noise's scale, relief
 * and the valley's size, the snow line and the seed offset), the chain of worlds, and the
 * functions of (x, z) that read them. `entry(m)` is the m-th world of the chain: entries 0 and 1
 * are one archetype with the coast on a side of its own (a seed's first world flips at z = 0),
 * and from entry 2 on a world follows the one before it, never the same archetype twice. `hash` is
 * the noise's lattice hash: the integer one the GPU reproduces bit for bit, or a stand-in (the node
 * proof hands in the reference terrain's float hash to compare against its heights).
 */
export function createTerrain(seed, { hash = latticeHash } = {}) {
  const r = mulberry(hashStr(`rv${seed}`));
  const riv = [r() * TAU, r() * TAU, r() * TAU, 16 + r() * 12];
  const ter = [1 / (34 + r() * 10), 24 + r() * 10, 26 + r() * 12, 4.6 + r() * 2];
  const ter2 = [0, 12 + r() * 8, 3 + r() * 40, 0];
  const params = { riv, rivF: RIVER_F, ter, ter2 };
  const archBag = [];
  const archIds = indices(ARCHETYPES.length);
  const floraIds = indices(FLORA.length);
  const first = bagNext(archBag, archIds, r, -1);
  const make = (arch, rng, flora) => {
    const A = ARCHETYPES[arch];
    const b = A.b.slice();
    if (A.b[1]) b[1] = rng() < 0.5 ? -1 : 1;
    return { arch, name: A.name, a: A.a.slice(), b, c: A.c.slice(), wf: A.wf, flora };
  };
  // the first two entries draw as the first world does: the side behind, the tree pair, the side
  // ahead; both take the second tree pair of the bag
  const floraBag = [];
  const sideBehind = make(first, r, 0);
  const pair = bagNext(floraBag, floraIds, r, 0);
  const sideAhead = make(first, r, 0);
  sideBehind.flora = bagNext(floraBag, floraIds, r, pair);
  sideAhead.flora = sideBehind.flora;
  const chain = [sideBehind, sideAhead];
  const next = mulberry(hashStr(`rw${seed}`));
  const nextFlora = [];
  const LAP = 256;

  /** World `m` of the chain (it wraps after 256). */
  function entry(m) {
    const k = m < chain.length || m < 2 + LAP ? m : 2 + ((m - 2) % LAP);
    while (chain.length <= k) {
      const prev = chain[chain.length - 1];
      const arch = bagNext(archBag, archIds, next, prev.arch);
      chain.push(make(arch, next, bagNext(nextFlora, floraIds, next, prev.flora)));
    }
    return chain[k];
  }

  /** The blend between world `m` and `m + 1` around z = -m * SEGMENT (the nearest flip to `z`
   * unless `m` is given). */
  function refAt(z, m = Math.max(0, Math.round(-z / SEGMENT))) {
    return { m, arZ: -m * SEGMENT, A: entry(m), B: entry(m + 1) };
  }

  const ar = [0, 0, 0, 0];
  const ar2 = [0, 0, 0, 0];
  const ar3 = [0, 0, 0, 0];
  const nz = [0, 0, 0];
  const lo = [0, 0, 0];

  function blend(ref, z) {
    const w = sst(ref.arZ + FLIP, ref.arZ - FLIP, z);
    for (let i = 0; i < 4; i++) {
      ar[i] = lerp(ref.A.a[i], ref.B.a[i], w);
      ar2[i] = lerp(ref.A.b[i], ref.B.b[i], w);
      ar3[i] = lerp(ref.A.c[i], ref.B.c[i], w);
    }
  }

  const riverXAt = (z) => riv[3] * (
    Math.sin(z * RIVER_F[0] + riv[0]) + 0.5 * Math.sin(z * RIVER_F[1] + riv[1])
    + 0.25 * Math.sin(z * RIVER_F[2] + riv[2])
  );

  /** The half-separation of the fork's two channels at `z` (0 where the river is one). */
  function riverSAt(ref, z) {
    blend(ref, z);
    return ar[0] * ter[3] * 3.4 * sst(0.36, 0.68, noise2(z * ar2[3], ter2[2] * 1.7 + 3, lo, hash));
  }

  /** The terrain's height at (x, z) in `ref`'s blend with `oct` octaves of noise. */
  function heightWith(ref, x, z, oct) {
    const sz = ter2[2];
    const c = riverXAt(z);
    const s = riverSAt(ref, z);
    const dx = x - c;
    const dr = Math.min(Math.abs(dx - s), Math.abs(dx + s));
    const isl = sst(s - 0.5 * ter[3], s - 1.5 * ter[3], Math.abs(dx))
      * sst(ter[3] * 1.1, ter[3] * 2.2, s);
    const reg = 0.65 + 0.6 * noise2(z * 0.0023, sz, lo, hash);
    const amp = ter[1] * reg * (1 + 0.8 * ar2[0]);
    const bw = ter[3] * (1 + ar[1] * 3);
    let px = x * ter[0] + sz;
    let py = z * ter[0] + sz;
    let a = 0;
    let b = 1;
    let ddx = 0;
    let ddy = 0;
    for (let i = 0; i < oct; i++) {
      const n = noise2(px, py, nz, hash);
      ddx += nz[1];
      ddy += nz[2];
      a += (b * n) / (1 + ddx * ddx + ddy * ddy);
      b *= 0.5;
      const nx = 1.6 * px + 1.2 * py;
      py = -1.2 * px + 1.6 * py;
      px = nx;
    }
    const m = a * 0.62;
    const bank = sst(bw * 0.75, bw * 1.35, dr);
    const wall = Math.pow(sst(bw * 0.9, ter[2] * reg * (1 + ar[1] * 1.5), dr), lerp(1, 0.5, ar2[0]))
      * (1 - 0.62 * isl);
    const mm = m - 0.2;
    const base = -1.5 + 2 * bank + amp * 0.35 * Math.pow(wall, 1.5) + ar2[2] * 8 * wall;
    const il = ar[3] * Math.max(noise2(x * 0.045 + sz + 5, z * 0.045 + sz + 5, lo, hash) - 0.52, 0)
      * 30 * (1 - wall * 0.7);
    let h = base + amp * 0.5 * (mm + Math.sqrt(mm * mm + 0.003)) * 1.25 * wall
      + il * (0.8 + 0.4 * m);
    if (ar[2] > 0.01 && h > 1) {
      const hs = h / 6;
      h = lerp(h, 6 * (Math.floor(hs) + sst(0.55, 1, hs - Math.floor(hs))), ar[2]);
    }
    if (Math.abs(ar2[1]) > 0.01) {
      h = lerp(h, -2 + il * 1.2, sst(bw * 1.5, bw * 6, dx * Math.sign(ar2[1])) * Math.abs(ar2[1]));
    }
    if (ar3[2] > 0.01) {
      h += sst(0.47, 0.53, noise2(x * 0.011 + sz + 21, z * 0.011 + sz + 21, lo,
        hash)) * ar3[2] * 7 * wall;
    }
    if (ar3[0] > 0.01) {
      const rv = Math.abs(noise2(x * 0.0045 + z * 0.0012 + sz * 0.7 + 40,
        z * 0.0062 + sz * 0.7 + 40, lo, hash)
        - 0.5);
      const rk = ar3[0] * sst(0.05, 0.022, rv) * sst(0.15, 0.6, wall);
      h = lerp(h, -3.5 + rv * 20, rk);
    }
    return h;
  }

  /** The world around `z` as the blend of its two neighbours: the blend `w` (0 the world behind, 1
   * the one ahead), the two entries and the blended forest and spire numbers `c`. */
  function blendAt(z) {
    const ref = refAt(z);
    blend(ref, z);
    return {
      ref, w: sst(ref.arZ + FLIP, ref.arZ - FLIP, z), A: ref.A, B: ref.B, c: ar3.slice(),
    };
  }

  return {
    seed,
    params,
    entry,
    refAt,
    blendAt,
    heightWith,
    riverX: riverXAt,
    riverS: (z) => riverSAt(refAt(z), z),
    /** The height at (x, z) with `oct` octaves of noise (the seam's five by default). */
    height: (x, z, oct = 5) => heightWith(refAt(z), x, z, oct),
  };
}

/** The terrain's nested grids, nearest first: cells across, spacing, octaves of noise. Each is
 * cut away where the finer one lies; spacing times cells over two is how far it reaches. */
const RINGS = Object.freeze([
  { n: 360, sp: 0.4, oct: 7 }, { n: 300, sp: 1.2, oct: 6 }, { n: 210, sp: 4, oct: 5 },
]);

/** The most octaves a fragment may take at the ring share `share` (8, 8, 7, 6), and how much of
 * the detail normals it keeps (1, .7, .45, 0): the fragment's cost follows the budget's step. */
export const fragmentSteps = (share) => ({
  top: share >= 0.75 ? 8 : share >= 0.55 ? 7 : 6,
  det: share >= 1 ? 1 : share >= 0.75 ? 0.7 : share >= 0.55 ? 0.45 : 0,
});

/** The octaves a fragment `dist` from the camera takes: `top` near, one fewer per doubling of
 * (1 + .02 dist), never under 3. It reads the distance only, so a ring's edge is no seam. */
export const fragmentOct = (top, dist) => clamp(
  top - Math.floor(Math.log2(1 + 0.02 * dist)), 3, top,
);

/** The step of the fragment's normal samples at `dist` from the camera and depth `z`: it widens
 * with the distance and never falls under 16 ulps of a float at `z` (the flight does not rebase
 * z, so far out a fixed step would quantise to noise). */
export const fragmentStep = (dist, z) => Math.max(0.04 + 0.003 * dist, Math.abs(z) * 2e-6);

/** The fragment's terrain normal from three samples of `terrain` (the mirror) at (x, z) seen from
 * `dist` away, as the shader takes it, without the detail normals. */
export function fragmentNormal(terrain, x, z, dist, top) {
  const oct = fragmentOct(top, dist);
  const e = fragmentStep(dist, z);
  const h = terrain.height(x, z, oct);
  const nx = h - terrain.height(x + e, z, oct);
  const nz = h - terrain.height(x, z + e, oct);
  const len = Math.hypot(nx, e, nz);
  return [nx / len, e / len, nz / len];
}

/** A square grid of `n` cells of `sp` units, centred on the origin, in the xz plane. */
function gridGeometry(T, n, sp) {
  const positions = new Float32Array((n + 1) * (n + 1) * 3);
  const indices = new Uint32Array(n * n * 6);
  for (let j = 0, k = 0; j <= n; j++) {
    for (let i = 0; i <= n; i++, k += 3) {
      positions[k] = (i - n / 2) * sp;
      positions[k + 2] = (j - n / 2) * sp;
    }
  }
  // the rows nearest the camera first (it looks toward -z, the rows' high end), so the depth test
  // turns away the land behind a near bank before its fragments run
  for (let j = n - 1, t = 0; j >= 0; j--) {
    for (let i = 0; i < n; i++, t += 6) {
      const a = j * (n + 1) + i;
      const c = a + n + 1;
      indices.set([a, c, a + 1, a + 1, c, c + 1], t);
    }
  }
  const geometry = new T.BufferGeometry();
  geometry.setAttribute('position', new T.BufferAttribute(positions, 3));
  geometry.setIndex(new T.BufferAttribute(indices, 1));
  geometry.boundingSphere = new T.Sphere(new T.Vector3(), 1e5);
  return geometry;
}

/**
 * The terrain in TSL: the world's uniforms (`W`, written each frame from the camera's world) and
 * the functions of (x, z) the mirror has, so a vertex and the CPU agree: `riverX`, `riverS`,
 * `height(oct)` (a function of two float nodes); `forWorld(W)` gives the same functions on another
 * world's numbers (the cache draws a tile in the world nearest it).
 */
export function terrainNodes(ctx) {
  if (ctx.land?.nodes) return ctx.land.nodes;
  const { T, S } = ctx;
  const sh = shading(ctx);
  const { sst } = sh;
  const {
    Fn, float, vec2, vec3, floor, fract, uniform,
  } = S;
  const word = (n) => S.uint(n);
  /** `latticeHash` in TSL: the same 32-bit integer arithmetic, so the same bits. */
  const lattice = Fn(([ix, iy]) => {
    const mixed = S.uint(S.int(ix)).mul(word(0x1f3d5b79))
      .add(S.uint(S.int(iy)).mul(word(0x2a6e4c0b))).add(word(0x6b43a9b5)).toVar();
    mixed.assign(mixed.bitXor(mixed.shiftRight(word(15))));
    mixed.assign(mixed.mul(word(0x2c1b3c6d)));
    mixed.assign(mixed.bitXor(mixed.shiftRight(word(12))));
    mixed.assign(mixed.mul(word(0x297a2d39)));
    mixed.assign(mixed.bitXor(mixed.shiftRight(word(15))));
    return float(mixed.shiftRight(word(8))).div(16777216);
  });
  /** `noise2` in TSL: `vec3(value, d/dx, d/dy)` over the lattice. */
  const nd2 = Fn(([x]) => {
    const i = floor(x).toVar();
    const f = fract(x).toVar();
    const q = f.mul(f).mul(f).mul(f.mul(f.mul(6).sub(15)).add(10)).toVar();
    const dq = f.mul(f).mul(30).mul(f.mul(f.sub(2)).add(1)).toVar();
    const a = lattice(i.x, i.y);
    const b = lattice(i.x.add(1), i.y);
    const c = lattice(i.x, i.y.add(1));
    const d = lattice(i.x.add(1), i.y.add(1));
    const k = a.sub(b).sub(c).add(d);
    const value = a.add(b.sub(a).mul(q.x)).add(c.sub(a).mul(q.y)).add(k.mul(q.x).mul(q.y));
    const grad = dq.mul(vec2(b.sub(a), c.sub(a)).add(k.mul(q.yx)));
    return vec3(value, grad.x, grad.y);
  });
  const v4 = () => uniform(new T.Vector4());
  const W = {
    riv: v4(), ter: v4(), ter2: v4(), arA: v4(), arB: v4(), ar2A: v4(), ar2B: v4(), ar3A: v4(),
    ar3B: v4(), arZ: uniform(0),
  };
  const nodes = { W, lattice, nd2, forWorld: (world) => terrainFns(world, { S, sst, nd2 }) };
  Object.assign(nodes, nodes.forWorld(W));
  return nodes;
}

/** The terrain's functions of (x, z) on the world `W` (its numbers as nodes: the camera's
 * uniforms, or a tile's world picked per texel): `riverX`, `riverS`, `blend`, `zTerms`,
 * `heightFrom`, `height(oct)`. */
function terrainFns(W, { S, sst, nd2 }) {
  const {
    Fn, If, float, vec2, mix, sin, abs, max, min, sign, step, floor, fract, pow, sqrt,
  } = S;
  const riverX = (z) => W.riv.w.mul(
    sin(z.mul(RIVER_F[0]).add(W.riv.x))
      .add(sin(z.mul(RIVER_F[1]).add(W.riv.y)).mul(0.5))
      .add(sin(z.mul(RIVER_F[2]).add(W.riv.z)).mul(0.25)),
  );
  /** The world's blend at `z`: `{ ar, ar2, ar3 }` as vec4 nodes. */
  const blend = (z) => {
    const w = sst(W.arZ.add(FLIP), W.arZ.sub(FLIP), z);
    return { ar: mix(W.arA, W.arB, w), ar2: mix(W.ar2A, W.ar2B, w), ar3: mix(W.ar3A, W.ar3B, w) };
  };
  const riverSIn = (ar, ar2, z) => ar.x.mul(W.ter.w).mul(3.4)
    .mul(sst(0.36, 0.68, nd2(vec2(z.mul(ar2.w), W.ter2.z.mul(1.7).add(3))).x));
  const riverS = (z) => {
    const { ar, ar2 } = blend(z);
    return riverSIn(ar, ar2, z);
  };
  /** What the height reads of `z` alone: the blended world, the river's place and width, the
   * regional scale. A fragment's three samples share the one of their `z` (the step in x). */
  const zTerms = (z) => {
    const b = blend(z);
    const ar = b.ar.toVar();
    const ar2 = b.ar2.toVar();
    const ar3 = b.ar3.toVar();
    const c = riverX(z).toVar();
    const s = riverSIn(ar, ar2, z).toVar();
    const reg = nd2(vec2(z.mul(0.0023), W.ter2.z)).x.mul(0.6).add(0.65).toVar();
    const amp = W.ter.y.mul(reg).mul(ar2.x.mul(0.8).add(1)).toVar();
    const bw = W.ter.w.mul(ar.y.mul(3).add(1)).toVar();
    return { ar, ar2, ar3, c, s, reg, amp, bw };
  };
  /** The height at (x, z) given the terms of z: `top` octaves of noise, or (with `oct` a float
   * node) the first `oct` of them, the rest skipped, so a far pixel pays for fewer. */
  const heightFrom = (x, z, zt, top, oct = null) => {
    const sz = W.ter2.z;
    const { ar, ar2, ar3, c, s, reg, amp, bw } = zt;
    const dx = x.sub(c).toVar();
    const dr = min(abs(dx.sub(s)), abs(dx.add(s))).toVar();
    const isl = sst(s.sub(W.ter.w.mul(0.5)), s.sub(W.ter.w.mul(1.5)), abs(dx))
      .mul(sst(W.ter.w.mul(1.1), W.ter.w.mul(2.2), s));
    const p = vec2(x.mul(W.ter.x).add(sz), z.mul(W.ter.x).add(sz)).toVar();
    const a = float(0).toVar();
    const d = vec2(0, 0).toVar();
    for (let i = 0; i < top; i++) {
      const b = 0.5 ** i;
      const octave = () => {
        const n = nd2(p).toVar();
        d.addAssign(n.yz);
        a.addAssign(n.x.mul(b).div(d.dot(d).add(1)));
        p.assign(vec2(p.x.mul(1.6).add(p.y.mul(1.2)), p.x.mul(-1.2).add(p.y.mul(1.6))));
      };
      if (oct) If(oct.greaterThan(i), octave);
      else octave();
    }
    const m = a.mul(0.62).toVar();
    const bank = sst(bw.mul(0.75), bw.mul(1.35), dr);
    const wall = pow(
      sst(bw.mul(0.9), W.ter.z.mul(reg).mul(ar.y.mul(1.5).add(1)), dr),
      mix(float(1), float(0.5), ar2.x),
    ).mul(float(1).sub(isl.mul(0.62))).toVar();
    const mm = m.sub(0.2);
    const base = float(-1.5).add(bank.mul(2))
      .add(amp.mul(0.35).mul(pow(wall, 1.5))).add(ar2.z.mul(8).mul(wall));
    // the world's features that are not there (a weight of zero) cost no noise sample
    const il = float(0).toVar();
    If(ar.w.notEqual(0), () => {
      il.assign(ar.w
        .mul(max(nd2(vec2(x.mul(0.045).add(sz).add(5), z.mul(0.045).add(sz).add(5))).x.sub(0.52),
          0))
        .mul(30).mul(float(1).sub(wall.mul(0.7))));
    });
    const h = base.add(amp.mul(0.5).mul(mm.add(sqrt(mm.mul(mm).add(0.003)))).mul(1.25).mul(wall))
      .add(il.mul(m.mul(0.4).add(0.8))).toVar();
    // mesa terraces
    const hs = h.div(6);
    const terraced = float(6).mul(floor(hs).add(sst(0.55, 1, fract(hs))));
    h.assign(mix(h, terraced, ar.z.mul(step(0.01, ar.z)).mul(step(1, h))));
    // the coast: land falls to a sea on one side
    const coast = sst(bw.mul(1.5), bw.mul(6), dx.mul(sign(ar2.y))).mul(abs(ar2.y))
      .mul(step(0.01, abs(ar2.y)));
    h.assign(mix(h, il.mul(1.2).sub(2), coast));
    // cliff steps
    If(ar3.z.greaterThanEqual(0.01), () => {
      h.addAssign(sst(0.47, 0.53, nd2(vec2(x.mul(0.011).add(sz).add(21),
        z.mul(0.011).add(sz).add(21)))
        .x).mul(ar3.z).mul(7).mul(wall));
    });
    // the rift
    If(ar3.x.greaterThanEqual(0.01), () => {
      const rv = abs(nd2(vec2(
        x.mul(0.0045).add(z.mul(0.0012)).add(sz.mul(0.7)).add(40),
        z.mul(0.0062).add(sz.mul(0.7)).add(40),
      )).x.sub(0.5));
      const rk = ar3.x.mul(sst(0.05, 0.022, rv)).mul(sst(0.15, 0.6, wall));
      h.assign(mix(h, rv.mul(20).sub(3.5), rk));
    });
    return h;
  };
  const heights = new Map();
  /** The terrain's height as a function `(x, z) => node` with `oct` octaves of noise. */
  const height = (oct) => {
    if (heights.has(oct)) return heights.get(oct);
    const fn = Fn(([x, z]) => heightFrom(x, z, zTerms(z), oct));
    heights.set(oct, fn);
    return fn;
  };
  return { riverX, riverS, blend, height, heightFrom, zTerms };
}

/** The numbers of `terrain`'s world around depth `z`, written to the uniforms of `W`. */
function writeWorld(W, terrain, z) {
  const ref = terrain.refAt(z);
  const { params } = terrain;
  W.riv.value.set(...params.riv);
  W.ter.value.set(...params.ter);
  W.ter2.value.set(...params.ter2);
  W.arA.value.set(...ref.A.a);
  W.arB.value.set(...ref.B.a);
  W.ar2A.value.set(...ref.A.b);
  W.ar2B.value.set(...ref.B.b);
  W.ar3A.value.set(...ref.A.c);
  W.ar3B.value.set(...ref.B.c);
  W.arZ.value = ref.arZ;
  return ref;
}

/** The factory of the layer. */
export const landLayer = (ctx) => {
  const { T, S } = ctx;
  const sh = shading(ctx);
  const nodes = terrainNodes(ctx);
  const { W } = nodes;
  const {
    Fn, vec2, vec3, max, normalize, uniform, varying, positionLocal, positionGeometry,
  } = S;
  let terrain = createTerrain(0);
  // the terrain's height, normal and material noise, drawn into textures as the flight reaches them
  const cache = createLandCache(ctx, nodes, sh);
  // the seam the other layers place by: it delegates, so it survives a reseed
  const seam = {
    height: (x, z, oct) => terrain.height(x, z, oct),
    riverX: (z) => terrain.riverX(z),
  };
  ctx.terrain = seam;
  ctx.land = {
    nodes,
    get mirror() { return terrain; },
    /** For the checks: `{ top, det }` held in place of the budget's fragment step (null: the
     * budget's), so a check can thin the rings and keep the shading's octaves. */
    pin: null,
    get world() { return terrain.refAt(0); },
    /** The terrain cache (its levels and what it has drawn), for the checks. */
    cache,
    /** For the checks: true shades the land straight from the terrain per fragment (the cache's
     * reference, the mockup's per-fragment land) instead of from the cache. */
    direct: false,
    /** Write the world's uniforms for the camera's depth (every layer that reads the terrain's
     * nodes calls it, so a hidden land leaves them current). */
    sync: (frame) => writeWorld(W, terrain, frame.cam.z),
    /** Where the rings are and what each cuts away. */
    get rings() {
      return rings.map((r) => ({
        cx: r.cx, cz: r.cz, half: r.half, sp: r.sp, inner: r.uInner.value.toArray(),
      }));
    },
  };

  // the hero's light on the land: its place and strength (xyz, w) and colour, written each frame
  const hero = { at: uniform(new T.Vector4()), colour: uniform(new T.Color()) };
  // the fragment's budget step: its most octaves, the share of the detail normals
  const frag = { top: uniform(8), det: uniform(1) };
  // the groove's light: the last kick's ripple (x, z, age, strength), the energy and the bar phase
  // (y, z), and the light's colour
  const groove = {
    rip: uniform(new T.Vector4(0, 0, 9, 0)), gr: uniform(new T.Vector4()),
    colour: uniform(new T.Color()),
  };
  const ripple = { kn: null };
  const object = new T.Group();
  const rings = RINGS.map((spec, i) => {
    const uRing = uniform(new T.Vector2());
    // the ring's centre less the camera's, in doubles: the cache is read by the place relative to
    // the camera, small numbers whatever the depth of the flight
    const uRel = uniform(new T.Vector2());
    const uInner = uniform(new T.Vector4(0, 0, 0, i ? 1 : 0));
    const hOf = nodes.height(spec.oct);
    const hLow = nodes.height(3);
    const material = new T.MeshBasicNodeMaterial();
    material.side = T.DoubleSide;
    const q = vec2(positionLocal.x.add(uRing.x), positionLocal.z.add(uRing.y));
    // the vertex's height from the terrain cache (a few taps, where the terrain's octaves of noise
    // cost the most of the land's frame at full density), or from the terrain itself before the
    // cache has targets; built at the first draw, after the first update made them
    const rel = vec2(positionGeometry.x.add(uRel.x), positionGeometry.z.add(uRel.y));
    const world = Fn(() => {
      const h = cache.ready() ? cache.sample(rel, 'h').h : hOf(q.x, q.y);
      return vec3(q.x, max(h, WATER_LEVEL), q.y);
    })();
    material.positionNode = world;
    // the shading reads the terrain per fragment (`terrainColour`): only the position and the
    // smooth low-octave height the occlusion compares against ride the vertices
    const vW = varying(world, 'vW');
    // the height mist summed at the vertex (its noise is tens of units across, a cell a few tenths)
    const toCam = world.sub(S.cameraPosition);
    const vMist = varying(sh.mistI(toCam, S.length(toCam)), 'vMist');
    const vLo = varying(hLow(q.x, q.y), 'vLo');
    // (the geometry's own place: positionLocal is the moved one once the position node is set)
    const vRel = varying(vec2(positionGeometry.x.add(uRel.x), positionGeometry.z.add(uRel.y)),
      'vRel');
    const colour = (direct) => Fn(() => terrainColour({
      ctx, sh, nodes, cache, direct, frag, hero, groove, vW, vLo, vRel, uInner, vMist,
    }))();
    material.colorNode = colour(false);
    // the checks' reference: the same material reading the terrain per fragment, made when asked
    const reference = () => {
      const m = material.clone();
      m.colorNode = colour(true);
      return m;
    };
    const mesh = new T.Mesh(gridGeometry(T, 2, spec.sp), material);
    mesh.frustumCulled = false;
    object.add(mesh);
    return {
      spec, mesh, material, reference, direct: null, uRing, uRel, uInner, sp: spec.sp, half: 1,
      cx: 0, cz: 0, share: -1,
    };
  });
  /** The hero's light (the place it published last frame, the weight of the loudness and the bass,
   * the palette pair's colours mixed by the centroid) for the fragments. */
  const lightFromHero = ({ u: rig }) => {
    const at = ctx.hero?.pos;
    if (!at) {
      hero.at.value.w = 0;
      return;
    }
    hero.at.value.set(at[0], at[1], at[2], 0.3 + 0.7 * (rig.loud ?? 0) + 0.4 * (rig.bass ?? 0));
    hero.colour.value.copy(ctx.u.colA.value).lerp(ctx.u.colB.value, rig.cent ?? 0.4)
      .multiplyScalar(1.5);
  };
  /** The groove's light: a band that runs out from each kick (born on the kick counter's change,
   * `riverX` of a place ahead of the camera, as the mockup puts it), a swell on the bar. */
  const lightFromGroove = (frame) => {
    const { cam, dt = 0, u: rig } = frame;
    const g = grooveOf(frame);
    const am = rig.calm > 0.5 ? 0.2 : 1;
    const { rip, gr } = groove;
    if (g.kn !== ripple.kn) {
      if (ripple.kn !== null) {
        const z = cam.z - 16 - 10 * g.energy;
        rip.value.set(terrain.riverX(z), z, 0, (0.4 + 0.8 * g.energy) * Math.max(g.kick, 0.5) * am);
      }
      ripple.kn = g.kn;
    }
    rip.value.z += dt;
    gr.value.set(0, g.bar, g.energy * am, 0);
    groove.colour.value.copy(ctx.u.colA.value);
  };
  const fit = (share) => {
    for (const r of rings) {
      if (r.share === share) continue;
      r.share = share;
      const n = 2 * Math.round((r.spec.n * Math.sqrt(share)) / 2);
      r.sp = (r.spec.sp * r.spec.n) / n;
      r.half = (n * r.sp) / 2;
      r.mesh.geometry.dispose();
      r.mesh.geometry = gridGeometry(T, n, r.sp);
    }
  };

  return {
    name: 'land',
    group: 'base',
    object,
    build(seed) {
      // a new world lights nothing until the hero publishes its place again
      hero.at.value.w = 0;
      groove.rip.value.set(0, 0, 9, 0);
      ripple.kn = null;
      terrain = createTerrain(seed);
      ctx.terrain = seam;
      writeWorld(W, terrain, 0);
      fit(1);
    },
    update(frame) {
      const { cam, budget } = frame;
      sh.sync(frame);
      ctx.land.sync(frame);
      lightFromHero(frame);
      const share = budget?.rings ?? 1;
      fit(share);
      // the detail normals follow the budget's land detail (or, without one, the rings' share)
      const byRings = fragmentSteps(share);
      const steps = ctx.land.pin ?? { top: byRings.top, det: budget?.det ?? byRings.det };
      frag.top.value = steps.top;
      frag.det.value = steps.det;
      lightFromGroove(frame);
      cache.update(frame, terrain);
      for (const r of rings) {
        if (ctx.land.direct && !r.direct) r.direct = r.reference();
        r.mesh.material = ctx.land.direct ? r.direct : r.material;
      }
      let fx = cam.lookX - cam.x;
      let fz = cam.lookZ - cam.z;
      const len = Math.hypot(fx, fz);
      if (len < 1e-6) {
        fx = 0;
        fz = -1;
      } else {
        fx /= len;
        fz /= len;
      }
      rings.forEach((r, i) => {
        const step2 = r.sp * 2;
        r.cx = Math.round((cam.x + fx * r.half * 0.6) / step2) * step2;
        r.cz = Math.round((cam.z + fz * r.half * 0.6) / step2) * step2;
        r.uRing.value.x = r.cx;
        r.uRing.value.y = r.cz;
        r.uRel.value.set(r.cx - cam.x, r.cz - cam.z);
        if (i) {
          const inner = rings[i - 1];
          r.uInner.value.set(inner.cx, inner.cz, inner.half - 2 * r.sp, 1);
        }
      });
    },
    /** The fragment's step this frame (for the checks). */
    fragment: () => ({ top: frag.top.value, det: frag.det.value }),
    dispose() {
      for (const r of rings) {
        r.mesh.geometry.dispose();
        r.material.dispose();
        r.direct?.dispose();
      }
      cache.dispose();
    },
  };
};

/** What `terrainColour` reads of the terrain, straight from it per fragment (the cache's reference
 * for the checks): the height and the normal from three samples, the mockup's octaves by distance
 * (`fragmentOct`) and step (`fragmentStep`), and the material's noise at the fragment. */
function directSample({
  S, sh, nodes, frag, vW, dist,
}) {
  const { sst, nd2 } = sh;
  const { W } = nodes;
  const {
    vec2, vec3, max, normalize, sin, abs, clamp, floor, log2,
  } = S;
  const oct = clamp(frag.top.sub(floor(log2(dist.mul(0.02).add(1)))), 3, frag.top);
  const e = max(dist.mul(0.003).add(0.04), abs(vW.z).mul(2e-6));
  const ztH = nodes.zTerms(vW.z);
  const ztZ = nodes.zTerms(vW.z.add(e));
  const h = nodes.heightFrom(vW.x, vW.z, ztH, 8, oct).toVar();
  const hx = nodes.heightFrom(vW.x.add(e), vW.z, ztH, 8, oct);
  const hz = nodes.heightFrom(vW.x, vW.z.add(e), ztZ, 8, oct);
  const p = vW.xz;
  return {
    h,
    n: normalize(vec3(h.sub(hx), e, h.sub(hz))),
    r1: nd2(p.mul(0.45)).x.mul(0.6).add(nd2(p.mul(1.7)).x.mul(0.4)),
    rs: sin(h.mul(2.3).add(nd2(p.mul(0.04)).x.mul(7))).mul(0.25).add(0.75),
    st: nd2(vec2(p.x.mul(0.7).add(p.y.mul(0.5)), h.mul(2.6))).x.mul(0.6)
      .add(nd2(vec2(p.y.mul(0.9).sub(p.x.mul(0.3)), h.mul(5.1))).x.mul(0.4)),
    snow: sst(W.ter2.y.sub(3), W.ter2.y.add(3), h.add(nd2(p.mul(0.15)).x.sub(0.5).mul(7))),
    gc: nd2(p.mul(0.23).add(5)).x.mul(0.45).add(0.55).mul(nd2(p.mul(1.9)).x.mul(0.4).add(0.8)),
    cl: nd2(p.mul(0.2)).x,
  };
}

/** The terrain's colour at a point: rock, strata on steep faces, snow, grass on gentle ground, a
 * wet band at the shore, lit by the sun, the sky and the fill, then fogged. */
function terrainColour({
  ctx, sh, nodes, cache, direct, frag, hero, groove, vW, vLo, vRel, uInner, vMist,
}) {
  const { S, lib, u } = ctx;
  const { env, sst, nd2, fogCol, fog } = sh;
  const { W } = nodes;
  const {
    float, vec2, vec3, vec4, mix, cos, max, normalize, dot, length, clamp, step, pow, abs, exp,
    Discard, cameraPosition, sqrt, smoothstep, If,
  } = S;
  const dd = abs(vW.xz.sub(uInner.xy));
  Discard(uInner.w.greaterThan(0.5).and(max(dd.x, dd.y).lessThan(uInner.z)));
  const V = vW.sub(cameraPosition);
  const dist = length(V);
  const vd = V.div(dist);
  // the height, the normal and the material's noise: from the terrain cache (drawn per texel from
  // three samples of the terrain at the level's octaves, so the land is crisp whatever the
  // density of the rings), or for the checks straight from the terrain per fragment
  const c = direct ? directSample({
    S, sh, nodes, frag, vW, dist,
  }) : cache.sample(vRel);
  const h = c.h.toVar();
  const gn = c.n.toVar();
  // a detail normal near the camera, the less the lower the budget's step
  const near = exp(dist.mul(-0.028)).mul(frag.det);
  const dn = vec2(0, 0).toVar();
  If(near.greaterThan(0.002), () => {
    const a = nd2(vW.xz.mul(1.7).add(3));
    const b = nd2(vW.xz.mul(4.1).add(11));
    dn.assign(vec2(a.y.mul(0.16).add(b.y.mul(0.07)), a.z.mul(0.16).add(b.z.mul(0.07))));
  });
  const n = normalize(
    gn.add(vec3(dn.x, 0, dn.y).mul(near).mul(gn.y.mul(-0.5).add(1)).mul(step(WATER_LEVEL, h))),
  );
  const dif = max(dot(n, env.sunDir), 0);
  const fill = max(dot(n, normalize(vec3(env.sunDir.x.negate(), 0.6, env.sunDir.z.negate()))), 0);
  const ao = clamp(h.sub(vLo).mul(0.12).add(0.45), 0.1, 1);
  const rock = mix(vec3(0.045, 0.042, 0.04), vec3(0.11, 0.1, 0.092), c.r1).mul(c.rs);
  const snow = c.snow.mul(sst(0.6, 0.85, n.y));
  let alb = mix(rock, vec3(0.62, 0.66, 0.72), snow);
  alb = mix(alb, vec3(0.1, 0.095, 0.085), float(1).sub(sst(0.4, 2.5, h)).mul(0.6));
  // strata on steep faces, grass on gentle ground, a wet band at the shore
  const sl = float(1).sub(n.y);
  const cl = mix(vec3(0.035, 0.032, 0.03), vec3(0.15, 0.135, 0.12), c.st)
    .mul(c.cl.mul(0.4).add(0.8));
  const rk = mix(alb, cl, sst(0.3, 0.62, sl).mul(float(1).sub(snow)));
  const forest = nodes.blend(vW.z).ar3.w;
  const gr = sst(0.34, 0.16, sl).mul(sst(0.15, 1.3, h.sub(WATER_LEVEL)))
    .mul(sst(W.ter2.y.sub(1), W.ter2.y.sub(8), h)).mul(clamp(forest.mul(1.4).add(0.25), 0, 1));
  const gc = env.grassC.mul(c.gc);
  alb = mix(rk, gc, gr.mul(float(1).sub(snow)))
    .mul(float(1).sub(sst(0.8, 0, h.sub(WATER_LEVEL)).mul(0.5))).mul(env.albT);
  const sun = env.sunC.mul(env.sunI);
  const ambient = env.ambC.mul(n.y.mul(0.5).add(0.2).add(fill.mul(0.9))).mul(ao);
  const glare = pow(max(dot(vd, env.sunDir), 0), 5);
  let col = alb.mul(sun.mul(dif).mul(1.5).add(ambient))
    .add(alb.mul(sun).mul(glare).mul(float(1).sub(n.y)).mul(0.6));
  const sky = env.zenC.mul(0.9).add(env.horC.mul(0.5));
  const rim = pow(float(1).sub(max(dot(n, vd.negate()), 0)), 3);
  // the hero lights the land near it, by the cosine and 60 / (60 + d^2)
  const hv = hero.at.xyz.sub(vW);
  const hd2 = dot(hv, hv);
  const heroLit = alb.mul(hero.colour).mul(hero.at.w)
    .mul(max(dot(n, hv.div(sqrt(hd2.add(0.001)))), 0)).mul(float(60).div(hd2.add(60)));
  col = col.add(alb.mul(sky).mul(n.y.mul(0.65).add(0.35)).mul(ao)).add(heroLit)
    .add(fogCol(vd).mul(rim).mul(0.35));
  // the land breathes with the groove: a light band runs out from each kick, a swell on the bar
  const { rip, gr: gv } = groove;
  const kd = length(vW.xz.sub(rip.xy));
  const kr = rip.z.mul(48);
  const band = exp(kd.sub(kr).div(rip.z.mul(6).add(2.5)).pow(2).negate()).mul(rip.w)
    .mul(exp(rip.z.mul(-1.6))).mul(gv.z).mul(smoothstep(150, 40, dist));
  col = col.add(groove.colour.mul(0.04).add(alb.mul(groove.colour).mul(0.5)).mul(band));
  col = col.mul(gv.z.mul(0.07).mul(cos(gv.y.mul(6.2831853))).add(1));
  col = fog(col, V, dist, vW.y, direct ? null : vMist);
  return vec4(lib.frost(col, u.frost).mul(u.fade), 1);
}
