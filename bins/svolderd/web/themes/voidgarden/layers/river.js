/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's river layer: the water over the land's bed and the waterfalls off steep banks. The
// water is a ribbon of flat quads that follows the flight along the river (and out to a coast
// where the world has one); its shading reads the bed's depth from the land's terrain cache (the
// height the land's shading reads), so it fades into the shallows with no seam: four octaves of
// ripple, the sky reflected by the Fresnel term, a body colour from the palette, foam at the shore,
// flow lines, and the bass's light on the channel. A waterfall is a sheet of falling light from a
// rim to the water, placed on the CPU from the terrain's mirror, one per cell of the flight at the
// world's chance, brighter with the mids and the loudness.
//
//   waterfallsOf(terrain, seed, zFrom, zTo)  the falls between two depths, nearest the camera
//                          first: `{ x0, y0, z0, x1, y1, z1, w }` (rim, foot, width)
//   FALL_CELL              the length of a cell of the flight a fall can be in
//   riverLayer(ctx)        the layer factory

import { hashStr, mulberry } from '../rng.js';
import { shading } from './sky.js';
import { WATER, WATER_LEVEL, terrainNodes } from './land.js';

/** How long a cell of the flight is, units (a fall at most one to a cell). */
export const FALL_CELL = 90;

/** The most waterfalls drawn at once. */
const MAX_FALLS = 24;

/**
 * The waterfalls between depths `zFrom` (nearer the camera, larger z) and `zTo`: in each cell of
 * `FALL_CELL`, with the chance of the world there, a fall off a bank that rises steeply out of
 * the channel (a rim over 6 units high within 17 units of the water). Pure: the same terrain and
 * seed give the same falls.
 */
export function waterfallsOf(terrain, seed, zFrom, zTo) {
  const out = [];
  const first = Math.floor(-zFrom / FALL_CELL) - 1;
  const last = Math.ceil(-zTo / FALL_CELL) + 1;
  for (let ci = Math.max(0, first); ci <= last; ci++) {
    const r = mulberry(hashStr(`wf:${seed}:${ci}`));
    const zc = -(ci * FALL_CELL + 20 + r() * 50);
    const ref = terrain.refAt(zc);
    const chance = zc < ref.arZ ? ref.B.wf : ref.A.wf;
    if (!(r() < chance)) continue;
    const z = zc + (r() - 0.5) * 60;
    const width = terrain.params.ter[3];
    const side = r() < 0.5 ? -1 : 1;
    const xb = terrain.riverX(z) + side * (terrain.riverS(z) + width * 1.05);
    if (terrain.height(xb, z, 5) > 1.2) continue;
    for (const o of [3, 5, 8, 12, 17]) {
      const x = xb + side * o;
      const h = terrain.height(x, z, 5);
      if (h > 6) {
        out.push({
          x0: x - side * 0.6, y0: h - 0.4, z0: z, x1: xb + side * 0.6, y1: 0.05, z1: z,
          w: 0.9 + r() * 1.4,
        });
        break;
      }
    }
  }
  return out.filter((f) => f.z0 <= zFrom && f.z0 >= zTo).sort((a, b) => b.z0 - a.z0);
}

/** The water ribbon: rows along the flight (spacing, count) and columns across it. */
const RIBBON = Object.freeze({ rows: 150, cols: 24, spacing: 3, ahead: 40 });

/** A ribbon grid: x is the position across (0 to 1), z the row index. */
function ribbonGeometry(T) {
  const { rows, cols } = RIBBON;
  const positions = new Float32Array((rows + 1) * (cols + 1) * 3);
  const indices = new Uint32Array(rows * cols * 6);
  for (let j = 0, k = 0; j <= rows; j++) {
    for (let i = 0; i <= cols; i++, k += 3) {
      positions[k] = i / cols;
      positions[k + 2] = j;
    }
  }
  for (let j = 0, t = 0; j < rows; j++) {
    for (let i = 0; i < cols; i++, t += 6) {
      const a = j * (cols + 1) + i;
      const c = a + cols + 1;
      indices.set([a, c, a + 1, a + 1, c, c + 1], t);
    }
  }
  const geometry = new T.BufferGeometry();
  geometry.setAttribute('position', new T.BufferAttribute(positions, 3));
  geometry.setIndex(new T.BufferAttribute(indices, 1));
  geometry.boundingSphere = new T.Sphere(new T.Vector3(), 1e5);
  return geometry;
}

/** A fall's sheet: columns across, rows down; x is the position across (0 to 1), z the row (0 to
 * 1, top to foot). */
function sheetGeometry(T) {
  const cols = 4;
  const rows = 12;
  const positions = new Float32Array((rows + 1) * (cols + 1) * 3);
  const indices = new Uint32Array(rows * cols * 6);
  for (let j = 0, k = 0; j <= rows; j++) {
    for (let i = 0; i <= cols; i++, k += 3) {
      positions[k] = i / cols;
      positions[k + 2] = j / rows;
    }
  }
  for (let j = 0, t = 0; j < rows; j++) {
    for (let i = 0; i < cols; i++, t += 6) {
      const a = j * (cols + 1) + i;
      const c = a + cols + 1;
      indices.set([a, c, a + 1, a + 1, c, c + 1], t);
    }
  }
  const geometry = new T.BufferGeometry();
  geometry.setAttribute('position', new T.BufferAttribute(positions, 3));
  geometry.setIndex(new T.BufferAttribute(indices, 1));
  geometry.boundingSphere = new T.Sphere(new T.Vector3(), 1e5);
  return geometry;
}

/** How far the water reaches from the river's centre to each side where the worlds around `z`
 * need it: the branch and the basin, and a coast out to the horizon. `{ left, right }` (positive
 * numbers). */
export function waterReach(terrain, z) {
  const ref = terrain.refAt(z);
  const width = terrain.params.ter[3];
  let left = 0;
  let right = 0;
  for (const e of [ref.A, ref.B]) {
    const basin = e.a[0] * width * 3.4 + width * (1 + e.a[1] * 3) * 1.45 + 8;
    left = Math.max(left, basin);
    right = Math.max(right, basin);
    if (Math.abs(e.b[1]) > 0.01) {
      if (e.b[1] < 0) left = Math.max(left, 420);
      else right = Math.max(right, 420);
    }
  }
  return { left, right };
}

/** The factory of the layer. */
export const riverLayer = (ctx) => {
  const { T, S, lib, u, sprites } = ctx;
  const sh = shading(ctx);
  const { env, sst, nd2, hs2, skyBase, fogCol, fog } = sh;
  const nodes = terrainNodes(ctx);
  const {
    Fn, If, float, vec2, vec3, vec4, mix, max, normalize, dot, length, pow, exp, abs, fract, floor,
    reflect, uniform, varying, positionLocal, positionGeometry, cameraPosition, fwidth,
    instanceIndex,
  } = S;

  const object = new T.Group();
  // zRel: the ribbon's first row less the camera's depth, in doubles (the cache is read by the
  // place relative to the camera, small numbers whatever the depth of the flight)
  const reach = { left: uniform(0), right: uniform(0), zBase: uniform(0), zRel: uniform(0) };
  const style = uniform(new T.Vector4(0.6, 0.8, 0.9, 0));
  const detail = uniform(1);

  // the water
  const material = new T.MeshBasicNodeMaterial();
  material.transparent = true;
  material.side = T.DoubleSide;
  material.depthWrite = false;
  material.polygonOffset = true;
  material.polygonOffsetFactor = -2;
  material.polygonOffsetUnits = -2;
  const z = reach.zBase.sub(positionLocal.z.mul(RIBBON.spacing));
  const rx = nodes.riverX(z);
  const x = rx.add(mix(reach.left.negate(), reach.right, positionLocal.x));
  const world = vec3(x, WATER_LEVEL, z);
  material.positionNode = world;
  const fp = varying(vec2(x, z), 'fp');
  const centre = varying(rx, 'centre');
  // (from the geometry's own place: positionLocal is the moved one once the position node is set)
  const gz = reach.zBase.sub(positionGeometry.z.mul(RIBBON.spacing));
  const gx = nodes.riverX(gz).add(mix(reach.left.negate(), reach.right, positionGeometry.x));
  const rel = varying(
    vec2(gx.sub(cameraPosition.x), reach.zRel.sub(positionGeometry.z.mul(RIBBON.spacing))), 'fpRel',
  );
  // the shading is built at the first draw (the land cache's targets exist by then)
  material.colorNode = Fn(() => {
    const V = vec3(fp.x, WATER_LEVEL, fp.y).sub(cameraPosition);
    const dist = length(V);
    const vd = V.div(dist);
    // the bed's depth: the land cache's height (the land's own shading reads it), or the terrain's
    // five octaves per fragment where the land layer is off (the bisect) and nothing is cached
    const cache = ctx.land?.cache ?? null;
    const depth = float(WATER_LEVEL).sub(cache?.ready()
      ? cache.sample(rel, 'h').h : nodes.height(5)(fp.x, fp.y)).toVar();
    const t = u.time.mul(style.y.mul(0.6).add(0.7));
    const r1 = nd2(vec2(fp.x.mul(0.5), fp.y.mul(0.18).add(t.mul(0.8))));
    const r2 = nd2(vec2(fp.x.mul(1.6).add(3), fp.y.mul(0.7).add(t.mul(1.6))));
    // the finer ripples follow the budget's water detail (1, 0.5, 0): the finest goes at 0.5, the
    // next at 0, each standing in at its mean (a value of 0.5, no slope)
    const ripple = (above, at) => {
      const r = vec3(0.5, 0, 0).toVar();
      If(detail.greaterThan(above), () => {
        r.assign(nd2(at));
      });
      return r;
    };
    const r3 = ripple(0.25, fp.mul(vec2(3.1, 2.3)).add(vec2(t.mul(1.1), t.mul(-0.7))));
    const r4 = ripple(0.75, fp.mul(vec2(7.3, 6.1)).add(vec2(t.mul(-1.9), t.mul(2.3))));
    const near = exp(dist.mul(-0.018));
    const slope = r1.yz.mul(vec2(0.04, 0.009)).add(r2.yz.mul(vec2(0.04, 0.018)))
      .add(r3.yz.mul(0.012).add(r4.yz.mul(0.006)).mul(near)).mul(style.x.add(0.5));
    const normal = normalize(vec3(slope.x.negate(), 1, slope.y.negate()));
    const mirrored = reflect(vd, normal);
    const bounce = vec3(mirrored.x, abs(mirrored.y), mirrored.z);
    const fresnel = pow(float(1).sub(max(dot(vd.negate(), normal), 0)), 5).mul(0.98).add(0.02);
    const mirror = mix(fogCol(bounce).mul(0.4), skyBase(bounce), sst(0.02, 0.22, bounce.y));
    const body = mix(
      env.wsC.mul(max(env.sunDir.y, 0).mul(0.65).add(0.35).add(env.sunI.mul(0.3))), env.wdC,
      float(1).sub(exp(depth.mul(-0.6))),
    );
    let water = mix(body, mirror.mul(0.85), fresnel.mul(0.8))
      .add(env.sunC.mul(env.sunI).mul(pow(max(dot(bounce, env.sunDir), 0), 220)).mul(2.5));
    // foam at the shore
    const foam = sst(0.5, 0, depth.add(r3.x.sub(0.5).mul(0.3)))
      .mul(sst(0.3, 0.8, nd2(fp.mul(2.3).add(vec2(t.mul(0.4), t.mul(-0.3)))).x.add(r4.x.mul(0.3))));
    water = water.add(env.ambC.mul(1.5).add(env.sunC.mul(env.sunI).mul(0.4)).add(env.horC.mul(0.6))
      .mul(foam).mul(0.3).mul(exp(dist.mul(-0.01))));
    // flow lines along the channel and the bass's light on it
    const lateral = fp.x.sub(centre).div(nodes.W.ter.w);
    const wq = lateral.mul(5).add(r1.x.mul(0.7)).add(nd2(fp.mul(0.04).add(vec2(0, t.mul(0.1)))).x);
    const line = float(1).sub(sst(0, fwidth(fp.x).div(nodes.W.ter.w).mul(6).add(0.05),
      abs(fract(wq).sub(0.5))));
    const dash = sst(0.35, 0.7,
      fract(fp.y.mul(0.035).add(t.mul(0.5)).add(hs2(vec2(floor(wq), 3)))));
    water = water.add(env.light2C.mul(line).mul(dash).mul(0.5).mul(u.mid.mul(0.6).add(0.25))
      .mul(sst(1.3, 0.6, abs(lateral))));
    const channel = exp(fp.x.sub(centre).div(2).pow(2).negate());
    water = water.add(env.lightC.mul(u.bass.mul(1.6).add(0.6)).mul(channel)
      .mul(r1.x.mul(r2.x).mul(0.7).add(0.3)).mul(0.16));
    const foggy = fog(water, V, dist, float(WATER_LEVEL));
    // shallows show the bed through; the edge is where the bed rises through the water
    const cover = sst(-0.02, 0.12, depth).mul(sst(0, 1.1, depth).mul(0.5).add(0.5))
      .max(foam.mul(0.6).mul(sst(-0.05, 0.05, depth)));
    return vec4(lib.frost(foggy, u.frost).mul(u.fade), cover);
  })();
  const geometry = ribbonGeometry(T);
  const mesh = new T.Mesh(geometry, material);
  mesh.frustumCulled = false;
  mesh.renderOrder = 1;
  object.add(mesh);

  // the waterfalls: one sheet per fall, two buffers of its two ends
  const top = lib.cpuBuffer(MAX_FALLS, 'vec4');
  const foot = lib.cpuBuffer(MAX_FALLS, 'vec4');
  const sheetMaterial = new T.MeshBasicNodeMaterial();
  sheetMaterial.transparent = true;
  sheetMaterial.depthWrite = false;
  sheetMaterial.side = T.DoubleSide;
  sheetMaterial.blending = T.AdditiveBlending;
  {
    const a = top.element(instanceIndex);
    const b = foot.element(instanceIndex);
    const along = positionLocal.z;
    const across = positionLocal.x.mul(2).sub(1);
    const spread = a.w.mul(mix(float(1), float(1.35), along));
    const p = mix(a.xyz, b.xyz, along).add(vec3(across.mul(spread), 0, 0));
    sheetMaterial.positionNode = p;
    const uvx = varying(positionLocal.x, 'sx');
    const uvy = varying(along, 'sy');
    const id = float(instanceIndex);
    const streak = fract(uvy.mul(5).sub(u.time.mul(1.7)).add(hs2(vec2(floor(uvx.mul(6)), id))));
    const body2 = sst(0, 0.15, streak).mul(sst(1, 0.4, streak)).mul(0.6).add(0.4);
    const edge = sst(0, 0.2, uvx).mul(sst(1, 0.8, uvx)).mul(sst(0, 0.08, uvy));
    const bright = u.mid.mul(0.5).add(u.loud.mul(0.3)).add(0.3);
    const tint = mix(env.horC.mul(5), vec3(1, 1, 1), 0.55);
    const wv = varying(p, 'wv');
    const fv = wv.sub(cameraPosition);
    sheetMaterial.colorNode = vec4(
      lib.frost(tint.mul(body2).mul(edge).mul(bright), u.frost)
        .mul(exp(length(fv).mul(env.fogD).mul(-1.4))).mul(u.fade),
      1,
    );
  }
  const sheets = new T.Mesh(sheetGeometry(T), sheetMaterial);
  sheets.frustumCulled = false;
  sheets.count = 0;
  object.add(sheets);
  // a glow at each foot
  const feet = lib.cpuBuffer(MAX_FALLS, 'vec4');
  const feetSize = lib.cpuBuffer(MAX_FALLS, 'float');
  const glow = sprites.make({
    count: MAX_FALLS,
    position: feet,
    size: feetSize,
    mode: 'px',
    round: true,
    colour: lib.frost(env.horC.mul(5), u.frost).mul(u.fade),
  });
  glow.count = 0;
  object.add(glow);

  let terrain = null;
  let seeded = 0;
  let styleIndex = 0;
  const topValues = new Float32Array(MAX_FALLS * 4);
  const footValues = new Float32Array(MAX_FALLS * 4);
  const feetValues = new Float32Array(MAX_FALLS * 4);
  const feetSizes = new Float32Array(MAX_FALLS);

  return {
    name: 'river',
    group: 'base',
    object,
    build(seed) {
      seeded = seed;
      terrain = ctx.land?.mirror ?? null;
      styleIndex = Math.floor(mulberry(hashStr(`water:${seed}`))() * WATER.length);
      style.value.set(...WATER[styleIndex].w);
    },
    update(frame) {
      const { cam } = frame;
      sh.sync(frame);
      ctx.land?.sync(frame);
      detail.value = frame.budget?.water ?? 1;
      terrain = ctx.land?.mirror ?? terrain;
      if (!terrain) return;
      reach.zBase.value = Math.round((cam.z + RIBBON.ahead) / RIBBON.spacing) * RIBBON.spacing;
      reach.zRel.value = reach.zBase.value - cam.z;
      const { left, right } = waterReach(terrain, cam.z - 150);
      const near = waterReach(terrain, cam.z);
      reach.left.value = Math.max(left, near.left);
      reach.right.value = Math.max(right, near.right);
      const falls = waterfallsOf(terrain, seeded, cam.z + 30, cam.z - 420).slice(0, MAX_FALLS);
      falls.forEach((f, i) => {
        topValues.set([f.x0, f.y0, f.z0, f.w], i * 4);
        footValues.set([f.x1, f.y1, f.z1, 0], i * 4);
        feetValues.set([f.x1, 0.7, f.z1, 0], i * 4);
        feetSizes[i] = 18;
      });
      lib.upload(top, topValues);
      lib.upload(foot, footValues);
      lib.upload(feet, feetValues);
      lib.upload(feetSize, feetSizes);
      sheets.count = falls.length;
      glow.count = falls.length;
    },
    dispose() {
      geometry.dispose();
      material.dispose();
      sheets.geometry.dispose();
      sheetMaterial.dispose();
      glow.material.dispose();
    },
  };
};
