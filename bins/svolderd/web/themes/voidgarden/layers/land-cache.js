/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The land's terrain cache: what the land's shading reads of the terrain that does not depend on
// the light or the frame (the height, the normal from three samples, the material's noise),
// drawn into textures tile by tile as the flight reaches them, so a pixel reads a few texture taps
// instead of the terrain's noise. Five levels, nested around the camera like the land's rings:
// each a square of 16 x 16 tiles of 32 x 32 texels, its texel twice the finer one's, its window
// ahead of the camera; a tile lives in the slot of its index modulo 16 (a toroidal window, so the
// window moves by drawing only the tiles that come into it). A texel is a pure function of (seed,
// level, its place, the level's octaves): the same TSL terrain function the vertices take, at the
// level's octaves and normal step. The shading blends a level into the next coarser one over its
// outer tile, so no edge shows.
//
//   CACHE, RES                 the layout: levels, texels per tile, tiles per side, finest texel
//   FILL_PER_FRAME             the most tiles a frame draws while the land shows
//   levelTexel(k), levelTile(k)   a level's texel and tile, units
//   levelOct(top, k)           a level's octaves at the most `top` (the cache always draws at TOP)
//   levelStep(k, z)            a level's normal step at depth `z` (never under 16 ulps of z)
//   windowOf(k, x, z, fx, fz)  the tile at the centre of a level's window for a camera at (x, z)
//                              looking along (fx, fz)
//   slotOf(t), slotRects(list) a tile index's slot (0 .. 15); tiles grouped into slot rectangles
//   tileWorld(k, tz)           the world (flip) a tile row is drawn in
//   cacheTexel(terrain, k, gx, gz, top)   a texel's height and normal on the mirror
//   cacheBytes(packed)         the memory the cache's textures take
//   createLandCache(ctx, nodes, sh)   -> `{ update(frame, terrain), sample(rel, only), stats(),
//                              reset(), dispose() }`; `sample` gives the shading's nodes (`only`
//                              'h': the height alone)

/** The layout: levels, texels per tile side, tiles per window side, the tiles shown of them (the
 * outer ring is drawn ahead), the finest texel in units, and how far ahead of the camera a window's
 * centre sits (a share of its shown half). */
export const CACHE = Object.freeze({
  levels: 5, tile: 32, tiles: 16, shown: 14, texel: 0.125, ahead: 0.5,
});

/** Texels per window side. */
export const RES = CACHE.tile * CACHE.tiles;

/** The most tiles drawn in a frame while the land shows (a seek or a new seed draws what is in
 * view at once). */
export const FILL_PER_FRAME = 8;

/** The octaves the cache draws at, whatever the budget's step: the per-pixel cost does not depend on
 * them (a pixel reads texels), and a step change then never leaves a tile of the old count to redraw,
 * so no patchwork of two counts shows. The steps lighten the land through the detail normals, the
 * rings and the fill rate. */
export const TOP = 8;

/** The distance between two flips of the world (land.js SEGMENT). */
const SEGMENT = 1800;

const clamp = (v, a, b) => (v < a ? a : v > b ? b : v);
const sst = (a, b, x) => {
  const t = clamp((x - a) / (b - a), 0, 1);
  return t * t * (3 - 2 * t);
};

/** Level `k`'s texel, units. */
export const levelTexel = (k) => CACHE.texel * 2 ** k;

/** Level `k`'s tile, units. */
export const levelTile = (k) => CACHE.tile * levelTexel(k);

/** How many octaves under the budget's most a level takes: the finest two all, then one fewer per
 * two levels (the mockup's octaves by distance, one fewer per doubling of 1 + .02 d: the levels serve from
 * about 0, 30, 60, 120 and 240 units out). */
const OCT_DROP = Object.freeze([0, 0, 1, 1, 2]);

/** Level `k`'s octaves when the budget allows `top`, never under 3. */
export const levelOct = (top, k) => clamp(top - OCT_DROP[k], 3, top);

/** Level `k`'s normal step at depth `z`: half its texel (the mockup's step, .04 + .003 d, at the distance
 * the level begins to serve), and never under 16 ulps of a float at `z` (the flight does not rebase
 * z). */
export const levelStep = (k, z) => Math.max(levelTexel(k) / 2, Math.abs(z) * 2e-6);

/** The tile at the centre of level `k`'s window for a camera at (x, z) looking along (fx, fz)
 * (unit, in the xz plane): the window sits ahead, by `CACHE.ahead` of its shown half. */
export function windowOf(k, x, z, fx, fz) {
  const tau = levelTile(k);
  const half = (CACHE.shown / 2) * tau;
  return {
    cx: Math.round((x + fx * CACHE.ahead * half) / tau),
    cz: Math.round((z + fz * CACHE.ahead * half) / tau),
  };
}

/** The slot of tile index `t` along an axis. */
export const slotOf = (t) => ((t % CACHE.tiles) + CACHE.tiles) % CACHE.tiles;

/** The world nearest the tile row `tz` of level `k` (the flip its centre is nearest): the world a
 * texel of the tile is drawn in, so a tile is the same whichever draw makes it. */
export function tileWorld(k, tz) {
  const zc = (tz * CACHE.tile + CACHE.tile / 2) * levelTexel(k);
  return Math.max(0, Math.floor(-zc / SEGMENT + 0.5));
}

/** A texel's height and normal on the mirror (`terrain`, createTerrain's): texel (gx, gz) of level
 * `k`, its centre at ((g + .5) texel), the level's octaves at `top`, the normal from three samples
 * one step apart. */
export function cacheTexel(terrain, k, gx, gz, top) {
  const s = levelTexel(k);
  const x = (gx + 0.5) * s;
  const z = (gz + 0.5) * s;
  const oct = levelOct(top, k);
  const e = levelStep(k, z);
  const h = terrain.height(x, z, oct);
  const nx = h - terrain.height(x + e, z, oct);
  const nz = h - terrain.height(x, z + e, oct);
  const len = Math.hypot(nx, e, nz);
  return { h, n: [nx / len, e / len, nz / len] };
}

/** The bytes the cache's textures take: per level, the height and normal (four half floats, or
 * four bytes packed where float targets are absent) and the material's two RGBA8. */
export const cacheBytes = (packed) => CACHE.levels * RES * RES * ((packed ? 4 : 8) + 4 + 4);

/** The tiles of a window side around centre `c`: the shown ones [c - 7, c + 7) and the resident
 * ones [c - 8, c + 8). */
const shownRange = (c) => [c - CACHE.shown / 2, c + CACHE.shown / 2];
const residentRange = (c) => [c - CACHE.tiles / 2, c + CACHE.tiles / 2];

/** Whether the rectangle of tiles `r` ({x0, x1, z0, z1}) lies in the resident window of centre
 * (cx, cz). */
const inWindow = (r, cx, cz) => {
  const [ax, bx] = residentRange(cx);
  const [az, bz] = residentRange(cz);
  return r.x0 >= ax && r.x1 <= bx && r.z0 >= az && r.z1 <= bz;
};

/** Group the tiles `list` (each {tx, tz}) into slot rectangles: runs of neighbouring slots in a
 * row, merged down the rows where they span the same slots. */
export function slotRects(list) {
  const rows = new Map();
  for (const t of list) {
    const sy = slotOf(t.tz);
    if (!rows.has(sy)) rows.set(sy, []);
    rows.get(sy).push(slotOf(t.tx));
  }
  const runs = [];
  for (const [sy, xs] of [...rows].sort((a, b) => a[0] - b[0])) {
    xs.sort((a, b) => a - b);
    let start = xs[0];
    let prev = xs[0];
    for (let i = 1; i <= xs.length; i++) {
      if (i < xs.length && xs[i] === prev + 1) {
        prev = xs[i];
        continue;
      }
      runs.push({ x0: start, x1: prev + 1, y0: sy, y1: sy + 1 });
      if (i < xs.length) {
        start = xs[i];
        prev = xs[i];
      }
    }
  }
  const rects = [];
  for (const run of runs) {
    const above = rects.find((r) => r.x0 === run.x0 && r.x1 === run.x1 && r.y1 === run.y0);
    if (above) above.y1 = run.y1;
    else rects.push(run);
  }
  return rects;
}

/**
 * The cache on `ctx`'s renderer for the terrain `nodes` (terrainNodes) and the shading `sh`
 * (sky.js `shading`). Its textures are made at the first update (a hidden land takes no memory).
 * `update(frame, terrain)` moves the windows with the camera and draws the tiles they need:
 * a level whose shown tiles are not all drawn and whose last shown ones left the window (a seek, a
 * new seed) draws them at once; else a few a frame, the nearest first, then the ring ahead.
 * `sample(rel)` gives, for the camera-relative place `rel` (vec2 node), the shading's nodes
 * `{ h, n, r1, rs, st, snow, gc, cl }`.
 */
export function createLandCache(ctx, nodes, sh) {
  const { T, S } = ctx;
  const renderer = ctx.gpu?.renderer;
  const {
    Fn, If, float, int, vec2, vec3, vec4, ivec2, uniform, floor, mod, max, min, clamp: clampN,
    normalize, sin, select, abs, mix, screenCoordinate, texture, textureLoad,
  } = S;
  const { W } = nodes;
  const tile = CACHE.tile;

  /** Float targets need EXT_color_buffer_float on WebGL2 (phones may lack it): then the height and
   * normal are packed into RGBA8 (16-bit height, the normal's stereographic projection in 8-bit)
   * and read with four loads. The float texel is (height, normal x, z, y). */
  const packed = ctx.caps?.floatTargets === false || (ctx.caps?.tier === 'webgl2'
    && !renderer?.backend?.extensions?.has?.('EXT_color_buffer_float'));
  const H_MIN = -64;
  const H_SPAN = 256;

  // per level: the targets, the shading's uniforms and the drawing's
  const levels = [];
  for (let k = 0; k < CACHE.levels; k++) {
    levels.push({
      k,
      s: levelTexel(k),
      tau: levelTile(k),
      rts: null,
      // the shading: the camera's place in the window's texture (uv) and the shown square
      // relative to the camera (min x, min z, max x, max z; empty: nothing shown)
      off: uniform(new T.Vector2()),
      rect: uniform(new T.Vector4(1e9, 1e9, -1e9, -1e9)),
      // the drawing: the window's first texel, the level's octaves, the first world's index
      org: uniform(new T.Vector2()),
      oct: uniform(8),
      m0: uniform(0),
      worlds: Array.from({ length: 4 }, () => ({
        a: uniform(new T.Vector4()), b: uniform(new T.Vector4()), c: uniform(new T.Vector4()),
      })),
      // the tile each slot holds, the shown tiles, the window's centre, and the centre at which
      // the level last needed nothing
      keys: new Array(CACHE.tiles * CACHE.tiles).fill(null),
      shown: null,
      cx: 0,
      cz: 0,
      idle: null,
      quads: null,
    });
  }

  /** The drawing's nodes for level `L`: the texel's global index from its slot, its place, and the
   * world of its tile row (one of four entries around the window). */
  const texelPlace = (L) => {
    const sc = floor(screenCoordinate.xy);
    const g = L.org.add(mod(sc.sub(L.org), RES)).toVar();
    const p = g.add(0.5).mul(L.s).toVar();
    const tz = floor(g.y.div(tile));
    const zc = tz.mul(tile).add(tile / 2).mul(L.s);
    const m = max(floor(zc.negate().div(SEGMENT).add(0.5)), 0);
    const mi = clampN(m.sub(L.m0), 0, 2).toVar();
    const pick = (f, at) => select(at.equal(0), L.worlds[0][f],
      select(at.equal(1), L.worlds[1][f], select(at.equal(2), L.worlds[2][f], L.worlds[3][f])));
    const next = mi.add(1);
    const Wt = {
      riv: W.riv, ter: W.ter, ter2: W.ter2,
      arA: pick('a', mi), arB: pick('a', next), ar2A: pick('b', mi), ar2B: pick('b', next),
      ar3A: pick('c', mi), ar3B: pick('c', next),
      arZ: L.m0.add(mi).mul(-SEGMENT),
    };
    return { p, fns: nodes.forWorld(Wt) };
  };

  const encodeH = (h) => {
    const v = clampN(h.sub(H_MIN).div(H_SPAN), 0, 0.99999).mul(65535).toVar();
    const hi = floor(v.div(256));
    return vec2(hi.div(255), floor(v.sub(hi.mul(256))).div(255));
  };

  /** The three drawings of level `L`: (A) the height and the normal, (B) the rock's mix and band,
   * the strata, the snow's place, (C) the grass's shade, the strata's shade. */
  const drawings = (L) => {
    const nd2 = sh.nd2;
    const sstN = sh.sst;
    const a = Fn(() => {
      const { p, fns } = texelPlace(L);
      const e = max(float(L.s / 2), abs(p.y).mul(2e-6));
      const zt = fns.zTerms(p.y);
      const zt2 = fns.zTerms(p.y.add(e));
      const h = fns.heightFrom(p.x, p.y, zt, 8, L.oct).toVar();
      const hx = fns.heightFrom(p.x.add(e), p.y, zt, 8, L.oct);
      const hz = fns.heightFrom(p.x, p.y.add(e), zt2, 8, L.oct);
      const n = normalize(vec3(h.sub(hx), e, h.sub(hz))).toVar();
      // packed: the normal's stereographic projection (as fine on a cliff as on the flat)
      if (packed) return vec4(encodeH(h), n.xz.div(n.y.add(1)).mul(0.5).add(0.5));
      return vec4(h, n.x, n.z, n.y);
    })();
    const b = Fn(() => {
      const { p, fns } = texelPlace(L);
      const h = fns.heightFrom(p.x, p.y, fns.zTerms(p.y), 8, L.oct).toVar();
      const r1 = nd2(p.mul(0.45)).x.mul(0.6).add(nd2(p.mul(1.7)).x.mul(0.4));
      const rs = sin(h.mul(2.3).add(nd2(p.mul(0.04)).x.mul(7)));
      const st = nd2(vec2(p.x.mul(0.7).add(p.y.mul(0.5)), h.mul(2.6))).x.mul(0.6)
        .add(nd2(vec2(p.y.mul(0.9).sub(p.x.mul(0.3)), h.mul(5.1))).x.mul(0.4));
      const snow = sstN(W.ter2.y.sub(3), W.ter2.y.add(3),
        h.add(nd2(p.mul(0.15)).x.sub(0.5).mul(7)));
      return vec4(r1, rs.mul(0.5).add(0.5), st, snow);
    })();
    const c = Fn(() => {
      const { p } = texelPlace(L);
      const gc = nd2(p.mul(0.23).add(5)).x.mul(0.45).add(0.55)
        .mul(nd2(p.mul(1.9)).x.mul(0.4).add(0.8));
      return vec4(gc.div(1.2), nd2(p.mul(0.2)).x, 0, 1);
    })();
    return [a, b, c].map((node) => {
      const m = new T.NodeMaterial();
      m.fragmentNode = node;
      m.depthTest = false;
      m.depthWrite = false;
      return new T.QuadMesh(m);
    });
  };

  const makeTargets = (L) => {
    const target = (type, filter) => {
      const rt = new T.RenderTarget(RES, RES, {
        type, depthBuffer: false, stencilBuffer: false, generateMipmaps: false,
        minFilter: filter, magFilter: filter, wrapS: T.RepeatWrapping, wrapT: T.RepeatWrapping,
      });
      rt.texture.wrapS = T.RepeatWrapping;
      rt.texture.wrapT = T.RepeatWrapping;
      rt.texture.minFilter = filter;
      rt.texture.magFilter = filter;
      rt.texture.generateMipmaps = false;
      return rt;
    };
    L.rts = [
      packed ? target(T.UnsignedByteType, T.NearestFilter)
        : target(T.HalfFloatType, T.LinearFilter),
      target(T.UnsignedByteType, T.LinearFilter),
      target(T.UnsignedByteType, T.LinearFilter),
    ];
    L.quads = drawings(L);
  };

  let terrain = null;
  let seed = null;
  const counts = { tiles: 0, draws: 0, sync: 0, frames: 0 };

  /** Write level `L`'s drawing uniforms for the window of centre (cx, cz): its first texel, its
   * octaves and the four worlds from the one before its nearest tile row's. */
  const prepare = (L, cx, cz) => {
    const [x0] = residentRange(cx);
    const [z0, z1] = residentRange(cz);
    L.org.value.set(x0 * tile, z0 * tile);
    L.oct.value = levelOct(TOP, L.k);
    const m0 = Math.max(0, Math.min(tileWorld(L.k, z0), tileWorld(L.k, z1 - 1)) - 1);
    L.m0.value = m0;
    L.worlds.forEach((w, i) => {
      const e = terrain.entry(m0 + i);
      w.a.value.set(...e.a);
      w.b.value.set(...e.b);
      w.c.value.set(...e.c);
    });
  };

  /** Draw `list` (tiles of level `L`) into their slots, by slot rectangles. */
  const draw = (L, list) => {
    if (!list.length) return;
    for (const r of slotRects(list)) {
      for (let i = 0; i < 3; i++) {
        const rt = L.rts[i];
        rt.viewport.set(r.x0 * tile, r.y0 * tile, (r.x1 - r.x0) * tile, (r.y1 - r.y0) * tile);
        renderer.setRenderTarget(rt);
        L.quads[i].render(renderer);
        // Firefox on Mesa GL may drop a draw issued right after a pass of ours: flush after each
        // (the rule for a compute pass; the WebGPU tier has no GL)
        renderer.backend?.gl?.flush?.();
        counts.draws++;
      }
    }
    for (const t of list) {
      const slot = slotOf(t.tz) * CACHE.tiles + slotOf(t.tx);
      L.keys[slot] = t.key;
    }
    counts.tiles += list.length;
  };

  const keyOf = (tx, tz) => `${tx},${tz}`;

  /** The tiles of level `L` in the rectangle of tile ranges `xr`, `zr`: each with its key and
   * whether its slot holds it (`held`). */
  const tilesIn = (L, xr, zr) => {
    const out = [];
    for (let tz = zr[0]; tz < zr[1]; tz++) {
      for (let tx = xr[0]; tx < xr[1]; tx++) {
        const key = keyOf(tx, tz);
        const slot = slotOf(tz) * CACHE.tiles + slotOf(tx);
        const held = L.keys[slot] === key;
        out.push({ tx, tz, key, held });
      }
    }
    return out;
  };

  /** Level `L`'s window for the camera (x, z, looking along fx, fz): the tiles it still needs,
   * ranked (the shown ones missing, nearest first; then the ring ahead, the side the camera looks
   * first). A level with nothing left to show (a seek,
   * a new seed) draws its shown tiles at once. */
  const plan = (L, cam, fx, fz) => {
    const { cx, cz } = windowOf(L.k, cam.x, cam.z, fx, fz);
    L.cx = cx;
    L.cz = cz;
    prepare(L, cx, cz);
    const xs = shownRange(cx);
    const zs = shownRange(cz);
    const inner = tilesIn(L, xs, zs);
    const missing = inner.filter((t) => !t.held);
    const rect = { x0: xs[0], x1: xs[1], z0: zs[0], z1: zs[1] };
    if (!missing.length) {
      L.shown = rect;
    } else if (!(L.shown && inWindow(L.shown, cx, cz))) {
      draw(L, missing);
      counts.sync++;
      for (const t of missing) t.held = true;
      L.shown = rect;
    }
    const near = (t) => Math.hypot((t.tx + 0.5) * L.tau - cam.x, (t.tz + 0.5) * L.tau - cam.z);
    const want = [];
    for (const t of inner) {
      if (!t.held) want.push({ L, t, rank: near(t) });
    }
    for (const t of tilesIn(L, residentRange(cx), residentRange(cz))) {
      if (t.held || (t.tx >= xs[0] && t.tx < xs[1] && t.tz >= zs[0] && t.tz < zs[1])) continue;
      const ahead = ((t.tx + 0.5) * L.tau - cam.x) * fx + ((t.tz + 0.5) * L.tau - cam.z) * fz;
      want.push({ L, t, rank: 1e6 - ahead });
    }
    return want;
  };

  return {
    packed,
    levels,

    /** Move the windows to the camera of `frame` and draw what they need, for the mirror's
     * `terrain`. The cache is keyed on the terrain's seed: another object of the same seed (a
     * `build` of it) keeps the tiles. */
    update(frame, next) {
      // a renderer that cannot draw into a target (the node checks' stand-in) draws no tile
      if (!renderer?.setRenderTarget) return;
      terrain = next;
      if (next.seed !== seed) {
        // another seed: no tile of the last one is kept
        seed = next.seed;
        for (const L of levels) {
          L.keys.fill(null);
          L.shown = null;
          L.idle = null;
        }
      }
      const { cam } = frame;
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
      const wasTarget = renderer.getRenderTarget();
      const wasClear = renderer.autoClear;
      renderer.autoClear = false;
      const queue = [];
      for (const L of levels) {
        if (!L.rts) makeTargets(L);
        // a level whose window holds every tile it needs at these octaves has nothing to do
        // until the window moves
        const at = windowOf(L.k, cam.x, cam.z, fx, fz);
        if (L.idle && L.idle.cx === at.cx && L.idle.cz === at.cz) continue;
        const want = plan(L, cam, fx, fz);
        L.idle = want.length ? null : at;
        queue.push(...want);
      }
      queue.sort((p, q) => p.rank - q.rank);
      const now = new Map();
      for (const { L, t } of queue.slice(0, FILL_PER_FRAME)) {
        if (!now.has(L)) now.set(L, []);
        now.get(L).push(t);
      }
      for (const [L, list] of now) draw(L, list);
      renderer.setRenderTarget(wasTarget);
      renderer.autoClear = wasClear;
      // the shading's uniforms: the camera's place in each window's texture, the shown square
      const fr = (v) => v - Math.floor(v);
      for (const L of levels) {
        const span = L.s * RES;
        L.off.value.set(fr(cam.x / span), fr(cam.z / span));
        const r = L.shown;
        if (r && inWindow(r, L.cx, L.cz)) {
          L.rect.value.set(r.x0 * L.tau - cam.x, r.z0 * L.tau - cam.z, r.x1 * L.tau - cam.x,
            r.z1 * L.tau - cam.z);
        } else {
          L.rect.value.set(1e9, 1e9, -1e9, -1e9);
        }
      }
      counts.frames++;
    },

    /** The shading's nodes at the camera-relative place `rel`: the finest level whose shown square
     * holds it, blended into the next coarser one over its outer tile (less a texel). With `only`
     * 'h' just the height is read (one target: the water's depth). */
    sample(rel, only = null) {
      const all = only !== 'h';
      // a level's weight falls to 0 a texel inside its shown square, so the filter never reaches a
      // tile outside it (the ring drawn ahead, or a slot not yet drawn)
      const weights = levels.map((L) => {
        const r = L.rect;
        const m = min(min(rel.x.sub(r.x), r.z.sub(rel.x)), min(rel.y.sub(r.y), r.w.sub(rel.y)));
        return clampN(m.sub(L.s).div(L.tau), 0, 1);
      });
      const k0 = int(CACHE.levels - 1).toVar();
      const w0 = weights[CACHE.levels - 1].toVar();
      for (let k = CACHE.levels - 2; k >= 0; k--) {
        If(weights[k].greaterThan(0), () => {
          k0.assign(k);
          w0.assign(weights[k]);
        });
      }
      const read = (kNode) => {
        const A = vec4(0).toVar();
        const B = vec4(0).toVar();
        const C = vec4(0).toVar();
        const one = (L) => () => {
          const uv = L.off.add(rel.div(L.s * RES)).toVar();
          if (packed) {
            const t = uv.mul(RES).sub(0.5).toVar();
            const i0 = floor(t).toVar();
            const f = t.sub(i0).toVar();
            const at = (dx, dy) => {
              const v = textureLoad(L.rts[0].texture,
                ivec2(int(mod(i0.x.add(dx), RES)), int(mod(i0.y.add(dy), RES))));
              const h = v.x.mul(255 * 256).add(v.y.mul(255)).div(65535).mul(H_SPAN).add(H_MIN);
              const sp = v.zw.mul(2).sub(1).toVar();
              const d = sp.dot(sp).add(1);
              return vec4(h, sp.x.mul(2).div(d), sp.y.mul(2).div(d), float(1).sub(sp.dot(sp)).div(d));
            };
            A.assign(mix(mix(at(0, 0), at(1, 0), f.x), mix(at(0, 1), at(1, 1), f.x), f.y));
          } else {
            A.assign(texture(L.rts[0].texture, uv).level(0));
          }
          if (!all) return;
          B.assign(texture(L.rts[1].texture, uv).level(0));
          C.assign(texture(L.rts[2].texture, uv).level(0));
        };
        let chain = If(kNode.equal(0), one(levels[0]));
        for (let k = 1; k < CACHE.levels; k++) chain = chain.ElseIf(kNode.equal(k), one(levels[k]));
        return { A, B, C };
      };
      const fine = read(k0);
      const A = fine.A.toVar();
      const B = fine.B.toVar();
      const C = fine.C.toVar();
      If(w0.lessThan(1), () => {
        const coarse = read(min(k0.add(1), CACHE.levels - 1));
        A.assign(mix(coarse.A, A, w0));
        if (!all) return;
        B.assign(mix(coarse.B, B, w0));
        C.assign(mix(coarse.C, C, w0));
      });
      if (!all) return { h: A.x };
      const n = normalize(vec3(A.y, A.w, A.z));
      return {
        h: A.x, n, r1: B.x, rs: B.y.mul(0.5).add(0.5), st: B.z, snow: B.w, gc: C.x.mul(1.2),
        cl: C.y,
      };
    },

    /** Whether the targets exist (the first update made them): a shader may read the cache. */
    ready: () => levels[0].rts !== null,

    /** What the cache has done: tiles drawn, draws, levels drawn at once, frames; its memory. */
    stats: () => ({ ...counts, bytes: cacheBytes(packed), packed }),

    /** Drop every texture and every record of what they hold, for a lost context: the next
     * `update` makes the targets again and draws what is shown at once. */
    reset() {
      for (const L of levels) {
        for (const rt of L.rts ?? []) rt.dispose();
        for (const q of L.quads ?? []) q.material.dispose();
        L.rts = null;
        L.quads = null;
        L.keys.fill(null);
        L.shown = null;
        L.idle = null;
      }
      seed = null;
    },

    dispose() {
      this.reset();
    },
  };
}
