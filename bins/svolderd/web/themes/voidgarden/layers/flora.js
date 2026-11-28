/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's flora layer: seven kinds of tree (pine, cypress, broadleaf, bare, palm, crystal,
// willow), rock spires, and the far trees as sprites. A world names two kinds (a share of one and
// the shore kind that takes over near the water) and a size; the cells around the camera are
// walked on the CPU, each cell's tree is kept or dropped by the forest's density, the distance to
// the water, the slope and the height, and stands on the terrain's mirror (so a base is where the
// land is, never a GPU readback). A kind is one instanced mesh read from two buffers (where, how
// big and which colour); the shapes are turned profiles and ribbons made once. The placement is a
// pure function of the seed and the camera's cell, so the same frame is the same trees.
//
//   treesAround(terrain, seed, cam, forward, share, only)
//                          -> `{ near, far, spires }`: the trees of the grids (all three, or the
//                          named ones), each `{ x, y, z, kind, size, turn, tint, snow }`
//   GRIDS                  the three grids: cells, spacing, how far ahead, reach, inner reach
//   SINK                   how far a trunk is sunk into the ground
//   lathe(kind), ribbons(kind), spireShape()   the shapes' vertex data
//   floraLayer(ctx)        the layer factory

import { hashStr, mulberry } from '../rng.js';
import { FLORA, WATER_LEVEL, hash2, noise2 } from './land.js';
import { shading } from './sky.js';

const TAU = Math.PI * 2;
const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);
const sst = (a, b, x) => {
  const t = clamp((x - a) / (b - a));
  return t * t * (3 - 2 * t);
};

/** How far a tree's foot is sunk into the ground, units. */
export const SINK = 0.25;

/** The three grids of cells: `n` by `n` cells of `sp` units around a centre `ahead` in front of
 * the camera, a tree kept within `reach` and (far trees) beyond `inner`. */
export const GRIDS = Object.freeze({
  near: { n: 72, sp: 2, ahead: 36, reach: 84, inner: 0, snap: 4 },
  far: { n: 96, sp: 3.6, ahead: 150, reach: 250, inner: 70, snap: 12 },
  spire: { n: 26, sp: 24, ahead: 170, reach: 330, inner: 0, snap: 24 },
});

/** The octaves of noise a tree's ground is read at, and the steps the slope is read over. */
const HEIGHT_OCT = 4;
const SLOPE_OCT = 3;
const SLOPE_STEP = 1.5;

/** The cell offsets of a grid in a fixed shuffled order: a smaller share of cells thins the
 * forest evenly. */
const orders = new Map();
function cellOrder(name) {
  if (orders.has(name)) return orders.get(name);
  const { n } = GRIDS[name];
  const rng = mulberry(hashStr(`flora:cells:${name}`));
  const list = [];
  for (let j = 0; j < n; j++) for (let i = 0; i < n; i++) list.push([i - n / 2, j - n / 2]);
  for (let i = list.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    [list[i], list[j]] = [list[j], list[i]];
  }
  orders.set(name, list);
  return list;
}

/** The hash of a cell with an offset (the same hashes the cells' trees are drawn from). */
const cellHash = (cx, cy, dx, dy = dx) => hash2(cx + dx, cy + dy);

/** A kind's size factor. */
const sizeOf = (kind) => (kind === 4 ? 1.3 : kind === 5 ? 0.85 : kind === 3 ? 1.15 : 1);

/**
 * The trees of the three grids around the camera at `cam` (x, z) looking along `forward` (unit x,
 * z): for each cell in a fixed shuffled order, the first `share` of them (0 to 1, the budget's
 * thinning), the tree the cell holds if any. Near trees stand to 84 units, far ones from 70 to
 * 250, spires out to 330. A tree is dropped in the water or on its bank (within 1.45 river widths
 * of a channel), on steep ground (slope under 0.8) or above the snow line, and by the forest's
 * density at the cell.
 */
export function treesAround(terrain, seed, cam, forward, share = 1, only = ['near', 'far',
  'spire']) {
  const { ter, ter2 } = terrain.params;
  const result = { near: [], far: [], spires: [] };
  for (const name of only) {
    const grid = GRIDS[name];
    const order = cellOrder(name);
    const count = Math.floor(order.length * clamp(name === 'spire' ? Math.max(share, 0.5) : share));
    const ox = Math.round((cam.x + forward.x * grid.ahead) / grid.snap) * grid.snap;
    const oz = Math.round((cam.z + forward.z * grid.ahead) / grid.snap) * grid.snap;
    for (let index = 0; index < count; index++) {
      const [ci, cj] = order[index];
      const cx = Math.floor(ox / grid.sp + 0.5) + ci;
      const cy = Math.floor(oz / grid.sp + 0.5) + cj;
      const qx = (cx + 0.1 + 0.8 * cellHash(cx, cy, 7)) * grid.sp;
      const qz = (cy + 0.1 + 0.8 * cellHash(cx, cy, 26)) * grid.sp;
      const hh = hash2(cx * 1.3 + 10, cy * 1.3 + 10);
      const dxc = qx - cam.x;
      const dzc = qz - cam.z;
      const dc = Math.hypot(dxc, dzc);
      if (dc > grid.reach || dxc * forward.x + dzc * forward.z < -14) continue;
      const edge = 16 * cellHash(cx, cy, 4);
      if (name === 'far' ? dc < grid.inner + edge : name === 'near' && dc > grid.reach - edge) {
        continue;
      }
      const blend = terrain.blendAt(qz);
      const forest = blend.c[3];
      const spires = blend.c[1];
      const c0 = terrain.riverX(qz);
      const s0 = terrain.riverS(qz);
      const dw = Math.min(Math.abs(qx - c0 - s0), Math.abs(qx - c0 + s0));
      if (name === 'spire') {
        if (hh > spires * 0.3 || dw < ter[3] * 2.6) continue;
        const h = terrain.height(qx, qz, HEIGHT_OCT);
        if (h < WATER_LEVEL + 0.3) continue;
        const size = (9 + cellHash(cx, cy, 5) * 20) * (0.6 + 0.6 * spires);
        result.spires.push({
          x: qx, y: h - 2, z: qz, kind: 7, size, turn: hh * TAU, tint: cellHash(cx, cy, 9), snow: 0,
        });
        continue;
      }
      const dens = forest * sst(0.28, 0.62, noise2(qx * 0.017 + ter2[2] + 3,
        qz * 0.017 + ter2[2] + 3)
        + (forest - 0.5) * 0.4);
      if (hh > dens) continue;
      if (dw < ter[3] * 1.45) continue;
      const pair = FLORA[cellHash(cx, cy, 13) < blend.w ? blend.B.flora : blend.A.flora].f;
      const kind = cellHash(cx, cy, 11) < pair[0] - 0.5 * sst(ter[3] * 5, ter[3] * 1.8, dw)
        ? pair[1] : pair[2];
      const h = terrain.height(qx, qz, HEIGHT_OCT);
      if (h < WATER_LEVEL + 0.3) continue;
      const hx = terrain.height(qx + SLOPE_STEP, qz, SLOPE_OCT);
      const hz = terrain.height(qx, qz + SLOPE_STEP, SLOPE_OCT);
      const ny = SLOPE_STEP / Math.hypot(h - hx, SLOPE_STEP, h - hz);
      if (ny < 0.8 || h > ter2[1] - 2) continue;
      const size = (1.8 + 2.6 * cellHash(cx, cy,
        3.1)) * (0.75 + 0.5 * dens) * pair[3] * sizeOf(kind);
      (name === 'far' ? result.far : result.near).push({
        x: qx, y: h - (name === 'far' ? 0.2 : SINK), z: qz, kind, size, turn: hh * TAU,
        tint: cellHash(cx, cy, 2), snow: sst(ter2[1] - 9, ter2[1] - 2, h),
      });
    }
  }
  return result;
}

/** The turned profiles of the kinds: (radius, height) per ring, rings from the foot up. */
const PROFILES = {
  0: [[0, 0.06], [0.16, 0.06], [0.14, 0.42], [0.42, 0.1], [0.36, 0.32], [0.66, 0.08], [0.6, 0.21],
    [1, 0]],
  1: [[0, 0.05], [0.1, 0.05], [0.1, 0.15], [0.3, 0.23], [0.55, 0.24], [0.76, 0.17], [0.92, 0.08],
    [1.04, 0]],
  2: [[0, 0.07], [0.3, 0.06], [0.28, 0.24], [0.4, 0.46], [0.58, 0.52], [0.76, 0.44], [0.9, 0.24],
    [0.96, 0]],
  5: [[0, 0], [0.03, 0.2], [0.14, 0.3], [0.45, 0.27], [0.72, 0.14], [1, 0], [1, 0], [1, 0]],
  6: [[0, 0.08], [0.42, 0.07], [0.4, 0.24], [0.6, 0.4], [0.82, 0.42], [0.96, 0.32], [1.04, 0.15],
    [1.08, 0]],
};

/** The rings, around, and the foot count of a turned shape. */
export const LATHE = Object.freeze({ rings: 8, around: 6 });

/**
 * The turned shape of tree `kind` (0 pine, 1 cypress, 2 broadleaf, 5 crystal, 6 willow's crown):
 * `{ position, bark }` as flat arrays, 8 rings of 6 corners, each ring's corners at 60 degree
 * steps, with a little roughness on the round crowns; `bark` is 1 on the trunk's two rings.
 */
export function lathe(kind) {
  const profile = PROFILES[kind] ?? PROFILES[0];
  const rng = mulberry(hashStr(`lathe:${kind}`));
  const rough = kind === 2 || kind === 6;
  const position = [];
  const bark = [];
  for (let r = 0; r < LATHE.rings; r++) {
    const [radius, y] = profile[r];
    for (let a = 0; a < LATHE.around; a++) {
      const lumpy = rough ? 0.8 + 0.45 * rng() : 1;
      const angle = (a * TAU) / LATHE.around;
      position.push(Math.cos(angle) * radius * lumpy, y, Math.sin(angle) * radius * lumpy);
      bark.push(r < 2 ? 1 : 0);
    }
  }
  return { position, bark };
}

/** A ribbon's path at `t` for branch `b` of a bare tree, a palm or a willow's drapes: its centre
 * and half-width. */
function ribbonPoint(kind, b, t, hh) {
  const a = b * 2.39996 + hh * TAU;
  const d = [Math.cos(a), Math.sin(a)];
  if (kind < 3.5) {
    if (b < 0.5) return { p: [d[0] * 0.05 * t * t, t * 0.66, d[1] * 0.05 * t * t],
      w: 0.065 * (1 - 0.5 * t) };
    const fr = (v) => v - Math.floor(v);
    const y0 = 0.24 + 0.46 * fr(b * 0.618 + hh);
    const el = 0.45 + 0.5 * fr(b * 0.37 + hh * 3);
    const len = 0.34 + 0.26 * fr(b * 0.71 + hh * 7);
    const sway = 0.06 * Math.sin(t * 5 + b);
    return {
      p: [
        d[0] * t * len * (1 - 0.35 * el) + d[1] * sway,
        y0 + t * len * (0.35 + 0.8 * el) - t * t * 0.1,
        d[1] * t * len * (1 - 0.35 * el) - d[0] * sway,
      ],
      w: 0.032 * (1 - 0.8 * t),
    };
  }
  if (kind < 4.5) {
    const lean = [Math.cos(hh * TAU) * 0.24, Math.sin(hh * TAU) * 0.24];
    if (b < 0.5) return { p: [lean[0] * t * t, t, lean[1] * t * t], w: 0.045 * (1 - 0.3 * t) };
    return {
      p: [lean[0] + d[0] * t * 0.58, 1 + 0.16 * t - 0.55 * t * t, lean[1] + d[1] * t * 0.58],
      w: 0.11 * Math.sin(Math.PI * Math.min(t * 1.12, 1)) + 0.006,
    };
  }
  if (b < 0.5) return { p: [0, 0, 0], w: 0 };
  return {
    p: [
      d[0] * (0.4 + 0.04 * Math.sin(b * 3.1) + 0.06 * Math.sin(t * Math.PI)), 0.86 - 0.5 * t,
      d[1] * (0.4 + 0.04 * Math.sin(b * 3.1) + 0.06 * Math.sin(t * Math.PI)),
    ],
    w: 0.026 * (1 - 0.5 * t),
  };
}

/** Branches a ribbon kind has and the steps along each. */
const RIBBONS = Object.freeze({ branches: 8, steps: 6 });

/**
 * The ribbons of kind 3 (bare), 4 (palm) or 6 (willow drapes): `{ position, index, bark }`, each
 * branch two crossed strips along its path, the width across a plane through the branch and the
 * vertical; `bark` is 1 on the trunk.
 */
export function ribbons(kind) {
  const hh = 0.37;
  const position = [];
  const bark = [];
  const index = [];
  const k = kind === 6 ? 5 : kind;
  for (let b = 0; b < RIBBONS.branches; b++) {
    for (let plane = 0; plane < 2; plane++) {
      const start = position.length / 3;
      let any = false;
      for (let j = 0; j <= RIBBONS.steps; j++) {
        const t = j / RIBBONS.steps;
        const { p, w } = ribbonPoint(k, b, t, hh);
        if (w <= 0) continue;
        any = true;
        const heading = Math.hypot(p[0], p[2]) > 1e-3 ? [p[0], p[2]] : [1, 0];
        const norm = Math.hypot(...heading);
        const side = plane === 0
          ? [-heading[1] / norm, 0, heading[0] / norm] : [0, 1, 0];
        const wide = w * (k === 3 && b > 0 ? 1.6 : 1);
        for (const e of [-1, 1]) {
          position.push(p[0] + side[0] * wide * e, p[1] + side[1] * wide * e,
            p[2] + side[2] * wide * e);
          bark.push(b === 0 ? 1 : 0);
        }
      }
      if (!any) continue;
      for (let j = 0; j < RIBBONS.steps; j++) {
        const a = start + j * 2;
        index.push(a, a + 1, a + 2, a + 1, a + 3, a + 2);
      }
    }
  }
  return { position, index, bark };
}

/** A rock spire: a tapering turned shape, 6 rings of 7 corners, height 1 and a base radius 0.15
 * (the layer scales it by the spire's size). */
export function spireShape() {
  const position = [];
  const rng = mulberry(hashStr('spire'));
  for (let r = 0; r < 6; r++) {
    const t = r / 5;
    for (let a = 0; a < 7; a++) {
      const angle = (a * TAU) / 7;
      const rad = 0.15 * Math.pow(1 - t, 1.2) * (0.7 + 0.6 * rng()) + 0.0005;
      position.push(
        Math.cos(angle) * rad + t * t * 0.18 * (rng() - 0.5), t - 0.07, Math.sin(angle) * rad,
      );
    }
  }
  return { position };
}

/** Index of a turned shape of `rings` by `around` corners (closed around). */
function turnedIndex(rings, around) {
  const index = [];
  for (let r = 0; r < rings - 1; r++) {
    for (let a = 0; a < around; a++) {
      const i0 = r * around + a;
      const i1 = r * around + ((a + 1) % around);
      index.push(i0, i0 + around, i1, i1, i0 + around, i1 + around);
    }
  }
  return index;
}

/** How many of a kind's trees the buffers hold. */
const NEAR_MAX = GRIDS.near.n * GRIDS.near.n;
const FAR_MAX = GRIDS.far.n * GRIDS.far.n;
const SPIRE_MAX = GRIDS.spire.n * GRIDS.spire.n;

/** The factory of the layer. */
export const floraLayer = (ctx) => {
  const { T, S, lib, u, sprites } = ctx;
  const sh = shading(ctx);
  const { env, hs2, fog, fogCol } = sh;
  const {
    float, vec2, vec3, vec4, mix, sin, cos, max, abs, normalize, dot, cross, length, pow,
    select, atan, fract, floor, Fn, Discard, uv, positionLocal, instanceIndex, varying,
    dFdx, dFdy, cameraPosition, attribute,
  } = S;

  const object = new T.Group();
  const BARK = vec3(0.05, 0.04, 0.033);
  const GREY = vec3(0.5, 0.55, 0.6);

  /** One kind's mesh: its geometry, two buffers for the instances and a material. `spire` shapes
   * take the colour of rock. */
  function treeMesh(kind, shape, max2) {
    const geometry = new T.BufferGeometry();
    geometry.setAttribute('position', new T.BufferAttribute(new Float32Array(shape.position), 3));
    const corners = shape.position.length / 3;
    geometry.setAttribute('bark', new T.BufferAttribute(
      new Float32Array(shape.bark ?? new Array(corners).fill(0)), 1,
    ));
    const around = kind === 7 ? 7 : LATHE.around;
    geometry.setIndex(shape.index ?? turnedIndex(corners / around, around));
    geometry.boundingSphere = new T.Sphere(new T.Vector3(), 1e5);
    const where = lib.cpuBuffer(max2, 'vec4');
    const what = lib.cpuBuffer(max2, 'vec4');
    const material = new T.MeshBasicNodeMaterial();
    material.side = T.DoubleSide;
    const a = where.element(instanceIndex);
    const b = what.element(instanceIndex);
    const turn = a.w;
    const size = b.x;
    const c = cos(turn);
    const s = sin(turn);
    const local = positionLocal.mul(size);
    const rotated = vec3(c.mul(local.x).add(s.mul(local.z)), local.y,
      c.mul(local.z).sub(s.mul(local.x)));
    const wind = sin(u.time.mul(1.1).add(a.x.mul(0.06)).add(a.z.mul(0.04)))
      .mul(u.bass.mul(0.1).add(0.05));
    const sway = vec3(wind.mul(0.8), 0, wind.mul(0.6)).mul(rotated.y).mul(rotated.y).mul(0.05);
    const world = vec3(a.x, a.y, a.z).add(rotated).add(sway);
    material.positionNode = world;
    const vW = varying(world, 'tW');
    // the height mist summed at the vertex (its noise is tens of units across, a tree a few)
    const toCam = world.sub(cameraPosition);
    const vMist = varying(sh.mistI(toCam, length(toCam)), 'tMist');
    const vBark = varying(attribute('bark', 'float'), 'tB');
    const hv = b.y;
    const snow = b.z;
    material.colorNode = Fn(() => {
      const V = vW.sub(cameraPosition);
      const dist = length(V);
      const vd = V.div(dist);
      let n = normalize(cross(dFdx(vW), dFdy(vW)));
      n = select(dot(n, vd).greaterThan(0), n.negate(), n);
      let albedo;
      if (kind === 7) {
        albedo = mix(vec3(0.05, 0.045, 0.04), vec3(0.15, 0.13, 0.115), hv).mul(env.albT);
      } else if (kind === 5) {
        albedo = mix(env.light2C.mul(0.3), BARK.mul(env.albT), vBark);
      } else if (kind === 3) {
        albedo = mix(BARK.mul(env.albT).mul(2.2).mul(hv.mul(0.4).add(0.8)), BARK.mul(env.albT),
          vBark);
      } else {
        const leaf = mix(env.folC.mul(hv.mul(0.6).add(0.55)), GREY, snow.mul(0.55));
        albedo = mix(leaf, BARK.mul(env.albT), vBark);
      }
      const dif = max(dot(n, env.sunDir), 0);
      const lit = env.sunC.mul(env.sunI);
      let col = albedo.mul(lit.mul(dif).mul(1.4).add(env.ambC.mul(n.y.mul(0.5).add(0.35)))
        .add(env.zenC.mul(0.9).add(env.horC.mul(0.5)).mul(n.y.mul(0.6).add(0.4))));
      col = col.add(fogCol(vd).mul(pow(float(1).sub(max(dot(n, vd.negate()), 0)), 3)).mul(0.25))
        .add(albedo.mul(lit).mul(pow(max(dot(vd, env.sunDir), 0), 4)).mul(0.8));
      if (kind === 5) {
        const rim = pow(float(1).sub(max(dot(n, vd.negate()), 0)), 2);
        const glow = albedo.mul(u.high.mul(1.6).add(u.peak).add(1.1)).mul(rim.mul(0.9).add(0.35));
        col = mix(col, col.mul(0.4).add(glow), 1);
      }
      return vec4(lib.frost(fog(col, V, dist, vW.y, vMist), u.frost).mul(u.fade), 1);
    })();
    const mesh = new T.Mesh(geometry, material);
    mesh.frustumCulled = false;
    mesh.count = 0;
    object.add(mesh);
    return {
      kind, mesh, where, what, geometry, material, max: max2, values: new Float32Array(max2 * 4),
      params: new Float32Array(max2 * 4),
    };
  }

  // the kinds: turned ones, ribbon ones (a willow has both)
  const meshes = [];
  for (const kind of [0, 1, 2, 5, 6]) meshes.push(treeMesh(kind, lathe(kind), NEAR_MAX));
  for (const kind of [3, 4, 6]) {
    const m = treeMesh(kind, ribbons(kind), NEAR_MAX);
    m.ribbon = true;
    meshes.push(m);
  }
  const spires = treeMesh(7, spireShape(), SPIRE_MAX);
  meshes.push(spires);

  // the far trees: sprites with a silhouette per kind
  const farWhere = lib.cpuBuffer(FAR_MAX, 'vec4');
  const farSize = lib.cpuBuffer(FAR_MAX, 'float');
  const farWhat = lib.cpuBuffer(FAR_MAX, 'vec4');
  const far = sprites.make({
    count: FAR_MAX, position: farWhere, size: farSize, mode: 'world', additive: false,
  });
  far.count = 0;
  {
    const kind = farWhere.element(instanceIndex).w;
    const what = farWhat.element(instanceIndex);
    const pc = vec2(uv().x.mul(2).sub(1), float(1).sub(uv().y.mul(2)));
    const k = (v) => abs(kind.sub(v)).lessThan(0.5);
    far.material.colorNode = Fn(() => {
      const pine = abs(pc.x).sub(pc.y.add(1).mul(0.42));
      const cypress = abs(pc.x).sub(pc.y.add(1).mul(0.24)).sub(0.04);
      const round = length(pc.mul(vec2(1, 1.15))).sub(0.85);
      const c3 = pc.add(vec2(0, 0.3));
      const bareBody = length(c3.mul(vec2(1, 1.2))).sub(0.75);
      const bare = S.min(
        select(hs2(floor(pc.mul(5))).lessThan(0.5), max(bareBody, 0.01), bareBody),
        max(abs(pc.x).sub(0.06), pc.y.negate()),
      );
      const c4 = pc.add(vec2(0, 0.5));
      const an = atan(c4.y, c4.x);
      const palm = S.min(
        length(c4).sub(pow(abs(cos(an.mul(3.5))), 6).mul(0.55).add(0.12)),
        max(abs(pc.x.add(pc.y.mul(pc.y).mul(0.1))).sub(0.05), c4.y.negate()),
      );
      const crystal = abs(pc.x).mul(1.7).add(abs(pc.y)).sub(0.95);
      const willowBody = length(pc.add(vec2(0, 0.15)).mul(vec2(1, 1.25))).sub(0.85);
      const willow = select(
        pc.y.greaterThan(0.1).and(fract(pc.x.mul(4)).lessThan(0.35)), float(1), willowBody,
      );
      let shape = willow;
      shape = select(k(5), crystal, shape);
      shape = select(k(4), palm, shape);
      shape = select(k(3), bare, shape);
      shape = select(k(2), round, shape);
      shape = select(k(1), cypress, shape);
      shape = select(k(0), pine, shape);
      Discard(shape.greaterThan(0));
      const albedo = select(
        k(5), env.light2C.mul(0.3),
        select(
          k(3), BARK.mul(env.albT).mul(2.2),
          mix(env.folC.mul(what.y.mul(0.6).add(0.55)), GREY, what.z.mul(0.55)),
        ),
      );
      const at = far.material.positionNode;
      const V = at.sub(cameraPosition);
      const dist = length(V);
      const lit = env.sunC.mul(env.sunI);
      const col = albedo.mul(lit.mul(0.4).add(env.ambC.mul(0.6))
        .add(env.zenC.mul(0.9).add(env.horC.mul(0.5)).mul(0.7)));
      return vec4(lib.frost(fog(col, V, dist, at.y), u.frost).mul(u.fade), 1);
    })();
    far.material.opacityNode = float(1);
    far.material.transparent = false;
    far.material.depthWrite = true;
  }
  object.add(far);

  let terrain = null;
  let seeded = 0;
  const keys = { near: '', far: '', spire: '' };
  // the far trees' arrays, made once (the uploads read past the count only what the count hides)
  const farValues = new Float32Array(FAR_MAX * 4);
  const farSizes = new Float32Array(FAR_MAX);
  const farParams = new Float32Array(FAR_MAX * 4);

  /** Fill an instance buffer pair with `trees` and set the mesh's count. */
  function fill(m, trees) {
    const n = Math.min(trees.length, m.max);
    const { values, params } = m;
    for (let i = 0, k = 0; i < n; i++, k += 4) {
      const t = trees[i];
      values[k] = t.x;
      values[k + 1] = t.y;
      values[k + 2] = t.z;
      values[k + 3] = t.turn;
      params[k] = t.size;
      params[k + 1] = t.tint;
      params[k + 2] = t.snow;
      params[k + 3] = t.kind;
    }
    lib.upload(m.where, m.values);
    lib.upload(m.what, m.params);
    m.mesh.count = n;
  }

  return {
    name: 'flora',
    group: 'base',
    object,
    build(seed) {
      seeded = seed;
      terrain = ctx.land?.mirror ?? null;
      keys.near = keys.far = keys.spire = '';
    },
    update(frame) {
      sh.sync(frame);
      terrain = ctx.land?.mirror ?? terrain;
      if (!terrain) return;
      const { cam, budget } = frame;
      let fx = cam.lookX - cam.x;
      let fz = cam.lookZ - cam.z;
      // the heading in 15 degree sectors: the cells ahead do not shift with every nudge
      const sector = Math.round(Math.atan2(fz, fx) / (Math.PI / 12));
      fx = Math.cos(sector * (Math.PI / 12));
      fz = Math.sin(sector * (Math.PI / 12));
      const share = clamp(budget?.flora ?? 1, 0.25, 1);
      const forward = { x: fx, z: fz };
      const changed = [];
      for (const name of ['near', 'far', 'spire']) {
        const snap = GRIDS[name].snap;
        const cell = [Math.round(cam.x / snap), Math.round(cam.z / snap), sector, share, seeded];
        const key = cell.join(',');
        if (key !== keys[name]) {
          keys[name] = key;
          changed.push(name);
        }
      }
      if (!changed.length) return;
      const found = treesAround(terrain, seeded, cam, forward, share, changed);
      if (changed.includes('near')) {
        const byKind = new Map();
        for (const t of found.near) {
          if (!byKind.has(t.kind)) byKind.set(t.kind, []);
          byKind.get(t.kind).push(t);
        }
        for (const m of meshes) if (m.kind !== 7) fill(m, byKind.get(m.kind) ?? []);
      }
      if (changed.includes('spire')) fill(spires, found.spires);
      if (changed.includes('far')) {
        const nFar = Math.min(found.far.length, FAR_MAX);
        for (let i = 0, k = 0; i < nFar; i++, k += 4) {
          const t = found.far[i];
          farValues[k] = t.x;
          farValues[k + 1] = t.y + t.size * 0.45;
          farValues[k + 2] = t.z;
          farValues[k + 3] = t.kind;
          farSizes[i] = t.size * 1.7;
          farParams[k] = 0;
          farParams[k + 1] = t.tint;
          farParams[k + 2] = t.snow;
          farParams[k + 3] = 0;
        }
        lib.upload(farWhere, farValues);
        lib.upload(farSize, farSizes);
        lib.upload(farWhat, farParams);
        far.count = nFar;
      }
    },
    dispose() {
      for (const m of meshes) {
        m.geometry.dispose();
        m.material.dispose();
      }
      far.material.dispose();
    },
  };
};
