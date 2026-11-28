/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's beacons layer: the lights the music lifts, and the particles it throws. Six kinds of
// beacon stand in clusters on the ground ahead (pillars that fire on the kick, masts that run a
// 16th chaser on the hats, a lighthouse that sweeps once a bar, obelisks that swell with sustained
// mids, a runway of lamps along the river, a ring of stones that lights its next stone on each
// snare). Their instance data is written by compute passes each frame (a sculpted body, a lamp, a
// beam), from the groove the director hands in and the audio, and drawn as instanced bodies, soft
// lamps, beams and their reflections on the water. Eruptions and sparks are stateful particles in
// compute: an eruption rises from the ground ahead and lands on the terrain (a heights table the
// pass reads); sparks leave the hero's place on each hit. The ground is `ctx.terrain` (`height`,
// `riverX`), flat when the land layer has not set it. Optional seams the context may carry:
// `ctx.hero.at` (`{ x, y, z, size }`, where the sparks leave from; else ahead of the camera) and
// `ctx.fx` (the limiter the layers share when the frame hands no `flash`, made on first use).
// A frame carries the director's `events`, `director`, `section` and `flash` (its limiter).
//
//   BEACON                       the numbers of the placement
//   placeBeacons({ seed, cells, terrain, kinds, near })   the members of the clusters in `cells`
//   worldKinds(seed, world)      the four kinds a world of the seed's flight draws
//   unfold(beat, calm)           the stone rings' lean, continuous across the 8-bar flips
//   packStatic(members, slots)   the per-slot arrays the compute passes read
//   countsOf(q, caps)            how many eruption and spark particles are drawn
//   heightsOf(terrain, at)       the heights table an eruption lands on
//   beaconsLayer(ctx)            the layer factory

import { hashStr, mulberry } from '../rng.js';
import { shading } from './sky.js';
import {
  createMix, createSurge, eventsOf, flashOf, grooveOf,
} from '../tsl/post.js';

const TAU = Math.PI * 2;

/** A body's grid: profile rows and sides. */
const BODY = Object.freeze({ rows: 10, sides: 12 });

/** The placement's numbers: a cell's length along the flight, the cells kept ahead and behind, the
 * members the pool holds, the river's half width, the heights table's cells and size, and how far
 * from the start a cluster always stands (`near`). */
export const BEACON = Object.freeze({
  cell: 90, ahead: 6, behind: 1, members: 160, river: 9, tableCells: 96, tableStep: 1, near: 105,
});

/** The particle pools: 520 eruption particles and 260 sparks, their gravity and drag. */
export const POOLS = Object.freeze({
  eruptions: 520, sparks: 260, gravity: 9, drag: 0.8, landedS: 0.6, eruptGapS: 3,
});

const FLAT = Object.freeze({ height: () => 0, riverX: () => 0 });

/** The body shapes a kind cycles through, one step each 4-bar phrase from the member's start (0
 * crystal, 1 monolith, 2 ring, 3 lantern, 4 obelisk, 5 octahedron). */
export const SHAPE_LISTS = Object.freeze({
  0: [0, 5], 1: [5], 2: [3], 3: [0, 1, 4, 2, 3, 5], 4: [0], 5: [1, 4, 0, 2],
});
const KINDS = [0, 1, 2, 3, 4, 5];

/** The beats a stone ring takes to lean out or back (3, or 10 in Calm); it flips each 8 bars. */
export const UNFOLD = Object.freeze({ half: 32, ease: 3, easeCalm: 10 });

/** How Calm the groove reads, 0 to 1, from the motion amount `am`: eased across `am` .4 to .6, so
 * the unfold and the morph widths never step when `am` crosses the middle inside an ease. */
export function calmness(am) {
  const t = clamp((am - 0.4) / 0.2);
  return 1 - t * t * (3 - 2 * t);
}

/**
 * The stone rings' unfold, 0 upright to 1 leaning out, at beat `beat`: it flips each 8 bars (32
 * beats) and eases to its new side over a few beats, so it is continuous everywhere (out for 8
 * bars, back for 8, never a jump).
 */
export function unfold(beat, calm = 0) {
  const b = Math.max(0, Number.isFinite(beat) ? beat : 0);
  const k = Math.floor(b / UNFOLD.half);
  const f = (b - k * UNFOLD.half) / (UNFOLD.ease + (UNFOLD.easeCalm - UNFOLD.ease) * (+calm));
  const t = clamp(f);
  const e = t * t * (3 - 2 * t);
  const was = k > 0 ? (k - 1) % 2 : 0;
  return was + ((k % 2) - was) * e;
}

/** The kinds world `world` of a seed's flight draws, four of the six: a function of the seed and
 * the index alone. World 0, where every track opens, gets the sculpted four (obelisks, stone
 * rings, pillars and the lighthouse); later worlds take four of the six from the seed. */
export function worldKinds(seed, world = 0) {
  if (world === 0) return [3, 5, 0, 2];
  const r = mulberry(hashStr(`kinds:${seed}:${world}`));
  const ks = KINDS.slice();
  for (let i = ks.length - 1; i > 0; i--) {
    const j = Math.floor(r() * (i + 1));
    [ks[i], ks[j]] = [ks[j], ks[i]];
  }
  return ks.slice(0, 4);
}

const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);

/** Whether an event erupts: a drop's entry, or a peak in a drop, past the gap. The director's own
 * grant (`e.flash`, true or false) decides; only an eruption-to-be without the field asks the
 * frame's gate (`ask()`), so a beat, a kick or a hat never uses up a grant. */
export function erupts(e, { section, gapOk, ask }) {
  const entry = e.type === 'section' && e.section === 'drop';
  const peak = e.type === 'peak' && section === 'drop';
  return (entry || peak) && gapOk && (e.flash === true || (e.flash === undefined && ask()));
}

/** The most particles of the two pools drawn under the budget's share `q` and the caps. */
export function countsOf(q, caps) {
  const cap = caps?.maxParticles ?? 40000;
  const share = (n) => Math.max(1, Math.min(n, cap, Math.ceil(n * clamp(q, 0, 1))));
  return { eruptions: share(POOLS.eruptions), sparks: share(POOLS.sparks) };
}

/** A table of the terrain's heights on a square grid of `BEACON.tableCells` cells of
 * `BEACON.tableStep` around (`x`, `z`): row by row. */
export function heightsOf(terrain, { x, z }) {
  const n = BEACON.tableCells;
  const step = BEACON.tableStep;
  const out = new Float32Array(n * n);
  const x0 = x - (n * step) / 2;
  const z0 = z - (n * step) / 2;
  for (let iz = 0; iz < n; iz++) {
    for (let ix = 0; ix < n; ix++) {
      out[iz * n + ix] = terrain.height(x0 + ix * step, z0 + iz * step);
    }
  }
  return out;
}

/** The cells (indices along the flight) that hold beacons for a camera at depth `z`. */
export function cellsAt(z) {
  const first = Math.max(0, Math.floor(-z / BEACON.cell)) - BEACON.behind;
  const out = [];
  for (let ci = Math.max(0, first); ci <= Math.max(0, first) + BEACON.ahead + BEACON.behind; ci++) {
    out.push(ci);
  }
  return out;
}

/** How a cluster's light is made: the palette, an RGB cycle, or spectral. */
const lightMode = (r) => (r < 0.55 ? 0 : r < 0.8 ? 1 : 2);

/**
 * The members of the clusters in cells `cells`, placed from `seed` on `terrain`: each member is
 * `{ kind, j, n, x, y, z, h, w, ph, rot, cx, cy, cz, lm, wet, shape }`. A cell holds one cluster
 * (or none); the same seed and cell give the same cluster. At most `BEACON.members` members.
 * `kinds` (default the world's four, `worldKinds(seed)`) are the kinds a cell draws from; `near`
 * (`{ z, kind }`) makes the cell of depth `z` hold a cluster of `kind` at `z` always.
 */
export function placeBeacons({
  seed, cells, terrain = FLAT, kinds = worldKinds(seed), near = null,
}) {
  const out = [];
  const ground = (x, z) => Math.max(terrain.height(x, z), 0);
  const snap = (x, z, rad, r) => {
    let bx = x;
    let bz = z;
    let bh = terrain.height(x, z);
    for (let k = 0; k < 5; k++) {
      const xx = x + (r() - 0.5) * rad * 2;
      const zz = z + (r() - 0.5) * rad * 2;
      const hh = terrain.height(xx, zz);
      if (hh > bh) {
        bh = hh;
        bx = xx;
        bz = zz;
      }
    }
    return { x: bx, z: bz };
  };
  for (const ci of cells) {
    const r = mulberry(hashStr(`bc${seed}:${ci}`));
    let zc = -(ci * BEACON.cell + 20 + r() * 50);
    let kind = kinds[Math.floor(r() * kinds.length)];
    const here = near && ci === Math.floor(-near.z / BEACON.cell);
    if (here) {
      zc = near.z;
      kind = near.kind;
    }
    if (r() > 0.8 && !here) continue;
    const c0 = terrain.riverX(zc);
    const side = r() < 0.5 ? -1 : 1;
    const RV = BEACON.river;
    let ax = c0 + side * (RV + r() * (kind === 2 ? 6 : 34));
    if (kind !== 4 && kind !== 5) {
      let best = -1e9;
      for (let k = 0; k < 6; k++) {
        const x = c0 + side * (RV + r() * (kind === 2 ? 6 : 34));
        const h = terrain.height(x, zc);
        if (h > best) {
          best = h;
          ax = x;
        }
      }
    }
    const members = [];
    if (kind === 0) {
      for (let j = 0; j < 3; j++) {
        const s = snap(ax + (r() - 0.5) * 30, zc + (r() - 0.5) * 30, 5, r);
        members.push({ x: s.x, z: s.z, h: 0, w: 0 });
      }
    } else if (kind === 1) {
      for (let j = 0, n = 2 + Math.floor(r() * 2); j < n; j++) {
        const s = snap(ax + (r() - 0.5) * 40, zc + (r() - 0.5) * 40, 6, r);
        members.push({ x: s.x, z: s.z, h: 6 + r() * 5, w: 0 });
      }
    } else if (kind === 2) {
      members.push({ x: ax, z: zc, h: 5 + r() * 3, w: 0 });
    } else if (kind === 3) {
      for (let j = 0, n = 3 + Math.floor(r() * 2); j < n; j++) {
        const s = snap(ax + (r() - 0.5) * 22, zc + (r() - 0.5) * 22, 3, r);
        members.push({ x: s.x, z: s.z, h: 3.5 + r() * 4, w: 0.7 + r() * 0.6 });
      }
    } else if (kind === 4) {
      for (const sd of [-1, 1]) {
        for (let j = 0; j < 18; j++) {
          const z = zc - j * 4.5;
          members.push({ x: terrain.riverX(z) + sd * RV * 0.7, z, h: 0, w: sd, rw: true });
        }
      }
    } else {
      const cx = c0 + side * (RV + 2.6 + r() * 14);
      for (let j = 0; j < 9; j++) {
        const a = (j / 9) * TAU;
        members.push({
          x: cx + Math.cos(a) * 7.5, z: zc + Math.sin(a) * 7.5, h: 2.2 + r() * 1.6, w: 0,
        });
      }
    }
    if (out.length + members.length > BEACON.members) continue;
    const lm = lightMode(r());
    const ph = r() * TAU;
    const shape = Math.floor(r() * (SHAPE_LISTS[kind] ?? [0]).length);
    let sx = 0;
    let sz = 0;
    for (const m of members) {
      sx += m.x;
      sz += m.z;
    }
    const cx = sx / members.length;
    const cz = sz / members.length;
    const cy = ground(cx, cz);
    members.forEach((m, j) => {
      const y = ground(m.x, m.z) + (m.rw ? 0.25 : 0);
      out.push({
        kind,
        j,
        n: members.length,
        x: m.x,
        y,
        z: m.z,
        h: m.h,
        w: m.w,
        ph,
        rot: r() * TAU,
        cx,
        cy,
        cz,
        lm,
        wet: Math.abs(m.x - terrain.riverX(m.z)) < 18 ? 1 : 0,
        shape,
      });
    });
  }
  return out;
}

/** The bit layout of a member's integers packed in one float (exact below 2^24): the kind (3 bits),
 * the light mode (2), whether it stands by the water (1), the width class (5), the index in its
 * cluster (6) and the cluster's size (6). */
export const BITS = Object.freeze({ kind: 0, lm: 3, wet: 5, wq: 6, j: 11, n: 17 });

/** The width class of `w` (a width of 0.7 to 1.3, or a side, -1 or 1) and back. */
export const widthClass = (w) => Math.round((w + 1) * 10);
export const classWidth = (q) => q / 10 - 1;

/** The integers of a member as one float. */
export const packInts = (m) => m.kind + m.lm * 2 ** BITS.lm + m.wet * 2 ** BITS.wet
  + widthClass(m.w) * 2 ** BITS.wq + m.j * 2 ** BITS.j + m.n * 2 ** BITS.n;

/**
 * The arrays the compute passes read, `slots` copies of each member (a pass reads its own element's
 * index, so a member that several slots draw is written once per slot). `a` holds `x, y, z, rot`;
 * `b` holds `ints` (`packInts`), `ph`, `shape` (its start in the kind's list) and `h`. A slot with
 * no member has kind 7, which nothing draws. Two arrays only: the WebGL2 backend runs a pass over at most four
 * storage buffers in all, and a pass writes one or two.
 */
export function packStatic(members, slots) {
  const n = BEACON.members * slots;
  const arrays = { a: new Float32Array(n * 4), b: new Float32Array(n * 4) };
  for (let i = 0; i < BEACON.members; i++) {
    const m = members[i];
    for (let s = 0; s < slots; s++) {
      const o = (i * slots + s) * 4;
      if (!m) {
        arrays.b[o] = 7;
        continue;
      }
      arrays.a.set([m.x, m.y, m.z, m.rot], o);
      arrays.b.set([packInts(m), m.ph, m.shape, m.h], o);
    }
  }
  return arrays;
}

/** The terrain of the context (flat until the land layer sets one). */
const terrainOf = (ctx) => ctx.terrain ?? FLAT;

/** The factory of the layer. */
export const beaconsLayer = (ctx) => {
  const {
    T, S, lib, u, caps, gpu,
  } = ctx;
  const {
    float, vec2, vec3, vec4, uniform, mix, select, If, floor, fract, sin, cos, exp, pow, max, min,
    abs, length, reflect,
    smoothstep, normalize, cross, dot, mod, instanceIndex, positionGeometry,
  } = S;
  /** `unfold` as nodes: `bt` the beat, `calm` the 0 to 1 node of `calmness`. */
  const unfoldNode = (bt, calm) => {
    const k = floor(bt.div(UNFOLD.half));
    const f = bt.sub(k.mul(UNFOLD.half)).div(mix(float(UNFOLD.ease),
      float(UNFOLD.easeCalm), calm));
    const was = select(k.greaterThan(0.5), mod(k.add(1), 2), float(0));
    return mix(was, mod(k, 2), smoothstep(0, 1, S.clamp(f, 0, 1)));
  };
  /** `calmness(am)` as nodes. */
  const calmNode = () => {
    const t = smoothstep(0.4, 0.6, bu.am);
    return float(1).sub(t);
  };
  const M = BEACON.members;
  const object = new T.Group();
  const U = (v = 0) => uniform(v);
  /** The groove and the section as the passes read them, written each frame. */
  const bu = {
    kk: U(), sn: U(), ht: U(), dn: U(), E: U(0.3), bt: U(), bp: U(), kn: U(), sqn: U(), sus: U(),
    bg: U(0.55), am: U(1), drop: U(), F: U(), sK: U(), sN: U(), sH: U(), sD: U(), sU: U(),
  };
  const spec = ctx.audio;

  // ------------------------------------------------------------------ the beacons' passes
  // The WebGL2 backend runs a pass over at most four storage buffers in all (inputs and outputs),
  // so each pass reads the two static arrays of its slots and writes one or two buffers; the
  // nodes of a slot's draw are built once per pass from the same function.
  const statics = (slots) => ({
    a: lib.cpuBuffer(M * slots, 'vec4'), b: lib.cpuBuffer(M * slots, 'vec4'),
  });
  const st = { body: statics(2), lamp: statics(4), beam: statics(2) };
  const out = {
    bodyP: lib.computeBuffer(M * 2, 'vec4'),
    bodyQ: lib.computeBuffer(M * 2, 'vec4'),
    bodyC: lib.computeBuffer(M * 2, 'vec4'),
    bodyS: lib.computeBuffer(M * 2, 'vec4'),
    lampP: lib.computeBuffer(M * 4, 'vec4'),
    lampSz: lib.computeBuffer(M * 4, 'float'),
    lampC: lib.computeBuffer(M * 4, 'vec4'),
    lampR: lib.computeBuffer(M * 4, 'float'),
    beamS: lib.computeBuffer(M * 2, 'vec4'),
    beamE: lib.computeBuffer(M * 2, 'vec4'),
    beamC: lib.computeBuffer(M * 2, 'vec4'),
  };

  /** The colour of a hue, saturation .95 and lightness .6, through the sRGB curve. */
  const hue = (h) => {
    const a = 0.38;
    const f = (n) => {
      const k = mod(float(n).add(h.mul(12)), 12);
      return float(0.6).sub(float(a).mul(max(-1, min(min(k.sub(3), float(9).sub(k)), 1))));
    };
    return vec3(f(0), f(8), f(4)).pow(2.2);
  };
  /** The palette's six lights, lifted. */
  const kc = (i) => mix(u.colA, u.colB, i / 5).mul(1.5);

  /**
   * The light of member `j` of `n` in a cluster of mode `lm` (0 palette `pal`, 1 RGB cycle, 2
   * spectral) and its spectral position (-1 when the light is not spectral); `alt` shifts the hue.
   */
  const light = ({ lm, j, n, ph, pal, alt = 0 }) => {
    const f = select(n.greaterThan(1), j.div(max(n.sub(1), 1)), float(0.5));
    const shift = float(alt);
    const cyc = hue(fract(
      u.time.mul(0.09).mul(bu.am).add(j.mul(0.13)).add(ph).add(shift.mul(0.33)),
    ));
    const sp = hue(f.mul(0.78).add(shift.mul(0.04)));
    const rgb = select(lm.lessThan(0.5), pal, select(lm.lessThan(1.5), cyc, sp));
    return { rgb, spec: select(lm.greaterThan(1.5), f, float(-1)) };
  };

  /** What every pass reads of a slot's statics, and the drives of each kind. */
  const reads = (set, slots) => {
    const i = instanceIndex;
    const a = set.a.element(i);
    const b = set.b.element(i);
    const bits = (shift, width) => mod(floor(b.x.div(2 ** shift)), 2 ** width);
    const slot = float(i).mod(slots);
    const g = bu.bg.mul(bu.E.mul(0.75).add(0.45));
    const am = bu.am;
    const kind = bits(BITS.kind, 3);
    const j = bits(BITS.j, 6);
    const n = bits(BITS.n, 6);
    const w = bits(BITS.wq, 5).div(10).sub(1);
    const gateK = select(
      bu.drop.greaterThan(0.5).and(mod(bu.kn, max(n, 1)).notEqual(j)), bu.kk.mul(0.15), bu.kk,
    );
    const th = bu.bp.mul(TAU).add(b.y);
    const sw = bu.sus.mul(float(1).sub(u.flat.mul(0.5)));
    const sq = mod(bu.sqn, max(n, 1));
    const on5 = select(sq.equal(j), bu.sn, float(0));
    const run = max(n.div(2), 1);
    const jj = mod(j, run);
    const chase = fract(bu.bt).mul(run);
    const dd = mod(mod(chase.sub(jj), run).add(run), run);
    return {
      x: a.x, y: a.y, z: a.z, rot0: a.w, kind, j, n, ph: b.y, h: b.w, w,
      lm: bits(BITS.lm, 2), wet: bits(BITS.wet, 1), shape: b.z, slot, g, am, gateK, th, sw, sq,
      on5, run, jj, dd,
      k0: gateK.mul(1.6).add(0.18).mul(g).add(bu.F.mul(1.5)),
      k2: float(0.4).add(bu.E.mul(0.5)).add(bu.dn.mul(1.1)).mul(g).add(bu.F.mul(1.5)),
      k3: sw.mul(2).add(0.15).mul(g).add(bu.F.mul(1.5)).add(bu.kk.mul(g).mul(0.5)),
      k5: on5.mul(1.8).add(0.12).add(bu.dn.mul(0.6).mul(bu.E)).mul(g).add(bu.F.mul(1.2)),
      k4: exp(dd.mul(-0.8)).mul(bu.kk.mul(1.2).add(0.5).add(bu.E.mul(0.3))).add(0.1).mul(g)
        .add(bu.F),
    };
  };

  /** The nodes of a body slot, as variables set by the kind's block: `P` the place and size, `Q`
   * the turn, the height ratio, the tilt and the unfold, `C` the light, `Sx` the two shapes
   * (a * 8 + b), the morph between them, the twist and the spectral slice. */
  const bodyNodes = () => {
    const r = reads(st.body, 2);
    const phrase = floor(bu.bt.div(16));
    // linear: the shader eases it once (and takes sin(pi * m) of it); 3 beats in Calm
    const calmOf = calmNode();
    const mt = S.clamp(bu.bt.sub(phrase.mul(16)).div(mix(float(1.25), float(3),
      calmOf)), 0, 1);
    // the kind's list of shapes: the shape of the phrase before, the one now, and the morph
    const pick = (list, i) => {
      let node = float(list[list.length - 1]);
      for (let k = list.length - 2; k >= 0; k--) {
        node = select(i.lessThan(k + 0.5), float(list[k]), node);
      }
      return node;
    };
    const idAt = (i) => select(r.kind.equal(0), pick(SHAPE_LISTS[0], i),
      select(r.kind.equal(3), pick(SHAPE_LISTS[3], i), select(r.kind.equal(5),
        pick(SHAPE_LISTS[5], i), select(r.kind.equal(1), float(SHAPE_LISTS[1][0]),
          select(r.kind.equal(2), float(SHAPE_LISTS[2][0]), float(SHAPE_LISTS[4][0]))))));
    const len = select(r.kind.equal(0), float(2), select(r.kind.equal(3), float(6),
      select(r.kind.equal(5), float(4), float(1))));
    const now = r.shape.add(phrase);
    const shape = {
      a: idAt(mod(now.add(len).sub(1), len)), b: idAt(mod(now, len)),
      m: select(len.greaterThan(1.5), mt, float(0)),
    };
    const fixed = (id) => ({ a: float(id), b: float(id), m: float(0) });
    const P = vec4(0).toVar();
    const Q = vec4(0).toVar();
    const C = vec4(0).toVar();
    const Sx = vec4(0, 0, 0, -1).toVar();
    const gate = mix(float(1), float(1).div(max(r.n, 1)), bu.drop);
    const put = (size, lift, hR, tilt, twist, unfoldBy, rot, lit, glow, sh = shape) => {
      P.assign(vec4(r.x, r.y.add(lift), r.z, size));
      Q.assign(vec4(rot, hR, tilt, unfoldBy));
      C.assign(vec4(lit.rgb, glow));
      Sx.assign(vec4(sh.a.mul(8).add(sh.b), sh.m, twist, lit.spec));
    };
    const first = r.slot.lessThan(0.5);
    // pillars: crystals that spin and swell on their kick
    If(r.kind.equal(0).and(first), () => {
      const s0 = bu.F.mul(0.3).add(bu.am.mul(r.gateK).mul(0.55)).add(1).mul(2.1);
      put(s0, 0, float(1.5), float(0), r.gateK.mul(bu.am).mul(0.8), r.gateK.mul(bu.am).mul(0.25),
        r.rot0.add(bu.am.mul(u.time.mul(0.25).add(bu.sK.mul(7).mul(gate)))),
        light({ lm: r.lm, j: r.j, n: r.n, ph: r.ph, pal: kc(0) }), r.k0.mul(1.3).add(0.3));
    });
    // masts: a spinning octahedron on the hat chaser
    If(r.kind.equal(1).and(first), () => {
      const s1 = bu.ht.mul(bu.am).mul(0.35).add(1).mul(1.1);
      put(s1, r.h.add(0.2), float(1), float(0.3), float(0), float(0),
        r.rot0.add(bu.am.mul(u.time.mul(0.6).add(bu.sH.mul(9)))),
        light({ lm: r.lm, j: r.j, n: r.n, ph: r.ph, pal: kc(3), alt: 1 }),
        bu.ht.mul(2.2).mul(r.g).add(bu.F).mul(1.5).add(0.4), fixed(5));
    });
    // the lighthouse: a tower with a lantern that flashes on the downbeat
    If(r.kind.equal(2).and(first), () => {
      put(r.h.div(3.1), 0, float(3.1), float(0), float(0), float(0), r.ph,
        { rgb: kc(5), spec: float(-1) }, r.g.mul(0.2).add(0.12), fixed(1));
    });
    If(r.kind.equal(2).and(first.not()), () => {
      const s2 = bu.dn.mul(bu.am).mul(0.3).add(1).mul(1.5);
      put(s2, r.h.sub(0.35), float(1.05), float(0), float(0), bu.dn.mul(bu.am).mul(0.3),
        r.rot0.add(bu.am.mul(u.time.mul(0.3).add(bu.sD.mul(3)))),
        light({ lm: r.lm, j: float(0), n: float(1), ph: r.ph, pal: kc(2) }),
        r.k2.mul(1.6).add(0.5), fixed(3));
    });
    // obelisks: monuments that grow and turn with sustained mids
    If(r.kind.equal(3).and(first), () => {
      const s3 = bu.F.mul(0.15).add(bu.kk.mul(bu.am).mul(0.28)).add(1).mul(r.w).mul(2.5);
      const hR = r.h.mul(r.sw.mul(bu.am).mul(1.1).add(1)).div(s3);
      put(s3, 0, hR, float(0), r.sw.mul(bu.am).mul(0.35), float(0),
        r.rot0.add(bu.am.mul(u.time.mul(0.15).add(bu.sU.mul(1.4)).add(bu.sN.mul(4)))),
        light({ lm: r.lm, j: r.j, n: r.n, ph: r.ph, pal: kc(3) }), r.k3.mul(1.3).add(0.3));
    });
    // stones: lift and spin on their snare, lean out over eight bars
    If(r.kind.equal(5).and(first), () => {
      const s5 = r.on5.mul(bu.am).mul(0.2).add(1).mul(1.8);
      const un = unfoldNode(bu.bt, calmNode());
      const tilt = un.mul(0.5).mul(select(mod(r.j, 2).greaterThan(0.5), float(1), float(-1)));
      put(s5, r.on5.mul(bu.am).mul(1.1), r.h.mul(1.1).div(s5), tilt, r.on5.mul(bu.am).mul(0.2),
        un.mul(0.6),
        r.rot0.add(r.j.div(r.n).mul(TAU)).add(bu.am.mul(u.time.mul(0.08).add(bu.sN.mul(6)))),
        light({ lm: r.lm, j: r.j, n: r.n, ph: r.ph, pal: kc(5) }), r.k5.mul(1.4).add(0.2));
    });
    return { P, Q, C, Sx };
  };

  /** The nodes of a lamp slot: `P` the place, `Z` the size, `C` the light, `R` the reflection's
   * size (0 away from the water). */
  const lampNodes = () => {
    const r = reads(st.lamp, 4);
    const P = vec4(0).toVar();
    const Z = float(0).toVar();
    const C = vec4(0, 0, 0, -1).toVar();
    const lamp = (at, size, inten, lit) => {
      P.assign(vec4(r.x, at, r.z, 0));
      Z.assign(size);
      C.assign(vec4(lit.rgb.mul(inten), lit.spec));
    };
    const s0 = r.slot.lessThan(0.5);
    If(r.kind.equal(0).and(s0), () => {
      const s = bu.F.mul(0.3).add(bu.am.mul(r.gateK).mul(0.55)).add(1).mul(2.1);
      lamp(r.y.add(s.mul(1.5)), r.gateK.mul(bu.am).mul(3).add(2.6), r.k0.mul(3.5).add(0.6),
        light({ lm: r.lm, j: r.j, n: r.n, ph: r.ph, pal: kc(0) }));
    });
    If(r.kind.equal(1), () => {
      const on = select(mod(floor(bu.bt.mul(4)).add(r.j), 4).equal(r.slot), bu.ht, float(0));
      const top = r.slot.equal(3);
      lamp(r.y.add(r.h.mul(r.slot.add(1)).div(4)), on.mul(bu.am).add(1.6),
        on.mul(2.8).mul(r.g).add(bu.F).mul(2).add(0.25),
        light({
          lm: r.lm, j: r.j, n: r.n, ph: r.ph, pal: select(top, kc(3), kc(1)),
          alt: select(top, float(1), float(0)),
        }));
    });
    If(r.kind.equal(2).and(s0), () => {
      lamp(r.y.add(r.h).add(0.5), bu.dn.mul(bu.am).mul(2).add(3), r.k2.mul(2.8).add(1),
        light({ lm: r.lm, j: float(0), n: float(1), ph: r.ph, pal: kc(2) }));
    });
    If(r.kind.equal(3).and(s0), () => {
      lamp(r.y.add(0.4), r.sw.mul(3).add(3), r.k3.mul(1.6),
        light({ lm: r.lm, j: r.j, n: r.n, ph: r.ph, pal: kc(5), alt: 1 }));
    });
    If(r.kind.equal(4).and(s0), () => {
      lamp(r.y, float(0.9), r.k4.mul(2.2),
        light({
          lm: r.lm, j: r.jj, n: r.run, ph: r.ph, pal: select(r.w.lessThan(0), kc(4), kc(0)),
          alt: select(r.w.greaterThan(0), float(1), float(0)),
        }));
    });
    If(r.kind.equal(5).and(s0), () => {
      lamp(r.y.add(0.3), r.on5.mul(bu.am).mul(2).add(2.4), r.k5.mul(1.5),
        light({ lm: r.lm, j: r.j, n: r.n, ph: r.ph, pal: kc(1), alt: 1 }));
    });
    // the ring's own heart lamp, once per cluster: the ring's centre is its first stone's place
    // less the ring's radius
    If(r.kind.equal(5).and(r.slot.equal(1)).and(r.j.equal(0)), () => {
      const lit = bu.dn.greaterThan(0.05).or(bu.F.greaterThan(0.05));
      P.assign(vec4(r.x.sub(7.5), r.y.add(0.5), r.z, 0));
      Z.assign(select(lit, float(3.5), float(0)));
      C.assign(vec4(kc(5).mul(bu.dn.mul(bu.E).mul(r.g).add(bu.F).mul(2.8)), -1));
    });
    const wet = select(
      Z.greaterThan(0).and(r.wet.greaterThan(0.5)).and(P.y.greaterThan(0.3)), Z, float(0),
    );
    return { P, Z, C, R: wet };
  };

  /** The nodes of a beam slot: `A` its start and width, `B` its end and width, `C` its light. */
  const beamNodes = () => {
    const r = reads(st.beam, 2);
    const A = vec4(0).toVar();
    const B = vec4(0).toVar();
    const C = vec4(0).toVar();
    const first = r.slot.lessThan(0.5);
    const beam = (x0, y0, z0, w0, x1, y1, z1, w1, lit, k) => {
      A.assign(vec4(x0, y0, z0, w0));
      B.assign(vec4(x1, y1, z1, w1));
      C.assign(vec4(lit.rgb.mul(k), lit.spec));
    };
    If(r.kind.equal(0).and(first), () => {
      const s = bu.F.mul(0.3).add(bu.am.mul(r.gateK).mul(0.55)).add(1).mul(2.1);
      const top = r.y.add(s.mul(1.5));
      const on = r.gateK.mul(bu.am);
      beam(r.x, top, r.z, on.mul(0.9).add(0.6), r.x, top.add(on.mul(55).add(70)), r.z,
        on.mul(3).add(1.8), light({ lm: r.lm, j: r.j, n: r.n, ph: r.ph, pal: kc(0) }),
        r.k0.mul(2.6));
    });
    If(r.kind.equal(1).and(first), () => {
      beam(r.x, r.y, r.z, float(0.12), r.x, r.y.add(r.h), r.z, float(0.08),
        { rgb: vec3(0.9, 0.95, 1), spec: float(-1) }, bu.bg.mul(0.15).add(0.3));
    });
    If(r.kind.equal(2), () => {
      const top = r.y.add(r.h).add(0.5);
      const lit = light({ lm: r.lm, j: float(0), n: float(1), ph: r.ph, pal: kc(2) });
      If(first, () => {
        beam(r.x, top, r.z, float(0.4), r.x.add(cos(r.th).mul(90)), top.sub(3.5),
          r.z.add(sin(r.th).mul(90)), float(9), lit, r.k2.mul(1.6));
      });
      If(first.not(), () => {
        beam(r.x, top, r.z, float(0.3), r.x.sub(cos(r.th).mul(40)), top.sub(2.5),
          r.z.sub(sin(r.th).mul(40)), float(4), lit, r.k2.mul(0.4));
      });
    });
    If(r.kind.equal(3).and(first), () => {
      const s3 = r.w.mul(2.5);
      const top = r.y.add(s3.mul(1.2).mul(r.h.mul(r.sw.mul(1.1).add(1)).div(s3)));
      beam(r.x, top, r.z, r.sw.mul(0.6).add(0.2), r.x, top.add(r.sw.mul(30)).add(3), r.z,
        r.sw.mul(2.5).add(1), light({ lm: r.lm, j: r.j, n: r.n, ph: r.ph, pal: kc(5), alt: 1 }),
        r.k3.mul(r.sw).mul(1.1));
    });
    return { A, B, C };
  };

  /** A pass over `count` slots writing `buffers` (at most two) from the nodes `make` builds. */
  const pass = (count, make) => S.Fn(() => {
    const written = make();
    for (const [buffer, node] of written) buffer.element(instanceIndex).assign(node);
  })().compute(count);
  const bodyPasses = [
    pass(M * 2, () => { const n = bodyNodes(); return [[out.bodyP, n.P], [out.bodyQ, n.Q]]; }),
    pass(M * 2, () => { const n = bodyNodes(); return [[out.bodyC, n.C], [out.bodyS, n.Sx]]; }),
  ];
  const lampPasses = [
    pass(M * 4, () => { const n = lampNodes(); return [[out.lampP, n.P], [out.lampSz, n.Z]]; }),
    pass(M * 4, () => { const n = lampNodes(); return [[out.lampC, n.C], [out.lampR, n.R]]; }),
  ];
  const beamPasses = [
    pass(M * 2, () => { const n = beamNodes(); return [[out.beamS, n.A], [out.beamE, n.B]]; }),
    pass(M * 2, () => { const n = beamNodes(); return [[out.beamC, n.C]]; }),
  ];
  const { bodyP, bodyQ, bodyC, bodyS, lampP, lampSz, lampC, lampR, beamS, beamE, beamC } = out;

  // ------------------------------------------------------------------ what draws them
  const identity = (mesh, n) => {
    const m = new T.Matrix4();
    for (let i = 0; i < n; i++) mesh.setMatrixAt(i, m);
    mesh.instanceMatrix.needsUpdate = true;
    mesh.frustumCulled = false;
    return mesh;
  };
  const finish = (rgb) => lib.frost(rgb, u.frost).mul(u.fade);
  /** A colour's spectral brightness: the slice `f` of the spectrum (-1 for none). */
  const loudness = (f) => {
    const at = S.int(floor(f.mul(0.84).add(0.08).mul(63).add(0.5)));
    return select(f.lessThan(0), float(1), spec.spectrum(at).mul(1.7).add(0.3));
  };

  // the sculpted bodies: a grid of 10 profile rows by 12 sides, shaped as a crystal, a monolith, a
  // stone ring, a lantern, an obelisk or an octahedron, two of them blended while a phrase changes
  // the shape; flat facets lit by the sun and the sky, fogged. A body the camera is about to pass
  // shrinks and dims and is gone inside 7 units, so it never walls off the view.
  const bodyMaterial = new T.MeshBasicNodeMaterial();
  {
    const sh = shading(ctx);
    const i = instanceIndex;
    const P = bodyP.element(i);
    const Q = bodyQ.element(i);
    const Cc = bodyC.element(i);
    const Sx = bodyS.element(i);
    const rows = positionGeometry.x;
    const along = positionGeometry.y.div(BODY.sides);
    const ngon = (a, n) => {
      const step = TAU / n;
      return float(Math.cos(step / 2)).div(cos(mod(a, step).sub(step / 2)));
    };
    const table = (j, list) => {
      let node = float(list[list.length - 1]);
      for (let k = list.length - 2; k >= 0; k--) {
        node = select(j.lessThan(k + 0.5), float(list[k]), node);
      }
      return node;
    };
    const lerp = (a, b, t) => float(a).add(float(b - a).mul(t));
    /** The shape `k`'s point at row `j` and along `u`. */
    const bsh = (k, j, uu) => {
      const a = uu.mul(TAU);
      const v = j.div(9);
      const prof = select(v.lessThan(0.15), lerp(0.25, 0.95, v.div(0.15)),
        select(v.lessThan(0.7), lerp(0.95, 0.8, v.sub(0.15).div(0.55)),
          lerp(0.8, 0, v.sub(0.7).div(0.3))));
      const r0 = prof.mul(0.32).mul(ngon(a, 6));
      const y1 = min(j.div(8), 1);
      const r1 = select(j.greaterThan(8.5), float(0),
        lerp(1, 0.66, y1).mul(0.3).mul(ngon(a.add(Math.PI / 4), 4)));
      const y3 = table(j, [0, 0.08, 0.1, 0.3, 0.55, 0.6, 0.64, 0.76, 0.9, 1]);
      const r3 = table(j, [0.22, 0.22, 0.42, 0.5, 0.5, 0.32, 0.46, 0.2, 0.05, 0]).mul(0.5)
        .mul(ngon(a, 6));
      const y4 = select(j.greaterThan(8.5), float(1), j.div(8).mul(0.86));
      const r4 = select(j.greaterThan(8.5), float(0),
        lerp(0.2, 0.13, y4.div(0.86)).mul(ngon(a.add(Math.PI / 4), 4)));
      const y5 = v.add(0.35);
      const r5 = float(1).sub(abs(v.mul(2).sub(1))).mul(0.42).mul(ngon(a, 4));
      const rad = select(k.lessThan(0.5), r0, select(k.lessThan(1.5), r1,
        select(k.lessThan(3.5), r3, select(k.lessThan(4.5), r4, r5))));
      const yy = select(k.lessThan(0.5), v, select(k.lessThan(1.5), y1,
        select(k.lessThan(3.5), y3, select(k.lessThan(4.5), y4, y5))));
      const ph = v.mul(TAU);
      const q = ngon(ph.add(Math.PI / 4), 4).mul(0.075);
      const r2 = cos(ph).mul(q).add(0.42);
      const torus = vec3(cos(a).mul(r2), sin(a).mul(r2).add(0.56), sin(ph).mul(q));
      return select(k.greaterThan(1.5).and(k.lessThan(2.5)), torus,
        vec3(cos(a).mul(rad), yy, sin(a).mul(rad)));
    };
    const turn = (a, x, y) => vec2(
      cos(a).mul(x).add(sin(a).mul(y)), cos(a).mul(y).sub(sin(a).mul(x)),
    );
    const sa = floor(Sx.x.div(8));
    const sb = Sx.x.sub(sa.mul(8));
    const m = Sx.y;
    const sm = sin(m.mul(Math.PI));
    const blend = m.mul(m).mul(float(3).sub(m.mul(2)));
    const base = mix(bsh(sa, rows, along), bsh(sb, rows, along), blend);
    const unfold = max(sm, Q.w);
    const wide = unfold.mul(0.75).add(1);
    const tall = base.y.mul(float(1).sub(unfold.mul(0.32))).mul(Q.y);
    const spun = turn(Sx.z.mul(tall).add(Q.x).add(sm.mul(Math.PI / 2)), base.x.mul(wide),
      base.z.mul(wide));
    const tilted = turn(Q.z, spun.x, tall);
    const away = length(vec2(P.x.sub(S.cameraPosition.x), P.z.sub(S.cameraPosition.z)));
    const near = sh.sst(7, 26, away);
    const size = P.w.mul(near.mul(0.65).add(0.35));
    const world = vec3(P.x, P.y, P.z).add(vec3(tilted.x, tilted.y, spun.y).sub(vec3(0, 0.12, 0))
      .mul(size));
    bodyMaterial.positionNode = select(P.w.greaterThan(0).and(away.greaterThanEqual(7)), world,
      vec3(0, -1000, 0));
    // the fragment
    const vW = S.varying(world);
    const vC = S.varying(Cc.rgb.mul(loudness(Sx.w)));
    const vG = S.varying(Cc.w.mul(near.mul(0.7).add(0.3)));
    const vV = S.varying(rows.div(9));
    const vS = S.varying(select(m.lessThan(0.5), sa, sb));
    const V = vW.sub(S.cameraPosition);
    const dist = length(V);
    const vd = V.div(dist);
    const flat = normalize(cross(S.dFdx(vW), S.dFdy(vW)));
    const n = select(dot(flat, vd).greaterThan(0), flat.negate(), flat);
    const dif = max(dot(n, sh.env.sunDir), 0);
    const fr = pow(float(1).sub(max(dot(n, vd.negate()), 0)), 2);
    const alb = vec3(0.045, 0.05, 0.06).mul(sh.env.albT);
    let col = alb.mul(sh.env.sunC.mul(sh.env.sunI).mul(dif).mul(1.4)
      .add(sh.env.ambC.mul(n.y.mul(0.5).add(0.35)))
      .add(sh.env.zenC.mul(0.9).add(sh.env.horC.mul(0.5)).mul(n.y.mul(0.6).add(0.4))));
    const band = select(abs(vS.sub(3)).lessThan(0.5),
      sh.sst(0.1, 0.2, vV).mul(sh.sst(0.62, 0.5, vV)), float(0));
    const grid = select(abs(vS.sub(1)).lessThan(0.5).or(abs(vS.sub(4)).lessThan(0.5)),
      float(1).sub(sh.sst(0, 0.05, abs(fract(vV.mul(6).add(0.5)).sub(0.5)))), float(0));
    const glint = pow(max(dot(reflect(vd, n), sh.env.sunDir), 0), 20).mul(0.5);
    col = col.add(vC.mul(vG).mul(fr.mul(fr).mul(1.6).add(0.03).add(vV.mul(vV).mul(0.1))
      .add(band.mul(1.4)).add(grid.mul(0.9)).add(glint)));
    col = sh.fog(col, V, dist, vW.y);
    bodyMaterial.colorNode = vec4(finish(col), 1);
    bodyMaterial.side = T.DoubleSide;
  }
  const bodyGeometry = (() => {
    const g = new T.BufferGeometry();
    const at = [];
    const index = [];
    for (let r = 0; r < BODY.rows; r++) for (let a = 0; a < BODY.sides; a++) at.push(r, a, 0);
    for (let r = 0; r < BODY.rows - 1; r++) {
      for (let a = 0; a < BODY.sides; a++) {
        const i0 = r * BODY.sides + a;
        const i1 = r * BODY.sides + ((a + 1) % BODY.sides);
        index.push(i0, i0 + BODY.sides, i1, i1, i0 + BODY.sides, i1 + BODY.sides);
      }
    }
    g.setAttribute('position', new T.BufferAttribute(new Float32Array(at), 3));
    g.setIndex(index);
    return g;
  })();
  const bodies = identity(new T.InstancedMesh(bodyGeometry, bodyMaterial, M * 2), M * 2);

  // the beams: a quad stretched from one end to the other, turned toward the camera around its axis
  const beamMaterial = new T.MeshBasicNodeMaterial();
  {
    const i = instanceIndex;
    const s = beamS.element(i);
    const e = beamE.element(i);
    const c = beamC.element(i);
    const t = positionGeometry.y.add(0.5);
    const axis = e.xyz.sub(s.xyz);
    const along = s.xyz.add(axis.mul(t));
    const side = normalize(cross(axis, S.cameraPosition.sub(along)));
    const width = mix(s.w, e.w, t);
    beamMaterial.positionNode = along.add(side.mul(positionGeometry.x.mul(width)));
    const across = float(1).sub(abs(S.uv().x.mul(2).sub(1)));
    const fade = across.mul(across).mul(smoothstep(0, 0.08, S.uv().y))
      .mul(float(1).sub(S.uv().y.mul(0.6)));
    beamMaterial.colorNode = vec4(finish(c.rgb.mul(loudness(c.w))).mul(fade).mul(0.5), 1);
    beamMaterial.transparent = true;
    beamMaterial.depthWrite = false;
    beamMaterial.blending = T.AdditiveBlending;
    beamMaterial.side = T.DoubleSide;
  }
  const beams = identity(
    new T.InstancedMesh(new T.PlaneGeometry(1, 1), beamMaterial, M * 2), M * 2,
  );

  // the lamps, and their reflections on the water: the lamp sprites mirrored below the surface and
  // stretched into a streak
  const lampColour = lampC.element(instanceIndex);
  const lamps = ctx.sprites.make({
    count: M * 4,
    position: lampP,
    size: lampSz,
    mode: 'world',
    round: true,
    colour: finish(lampColour.rgb.mul(loudness(lampColour.w)).mul(0.5)),
  });
  const reflMaterial = new T.SpriteNodeMaterial();
  reflMaterial.positionNode = lampP.element(instanceIndex).xyz.mul(vec3(1, -1, 1));
  const streak = lampR.element(instanceIndex);
  reflMaterial.scaleNode = vec2(streak.mul(0.7), streak.mul(2.4));
  reflMaterial.colorNode = vec4(
    finish(lampColour.rgb.mul(loudness(lampColour.w)).mul(0.22)).mul(ctx.lib.spr()), 1,
  );
  reflMaterial.transparent = true;
  reflMaterial.depthWrite = false;
  reflMaterial.blending = T.AdditiveBlending;
  const reflections = new T.Sprite(reflMaterial);
  reflections.count = M * 4;
  reflections.frustumCulled = false;

  // ------------------------------------------------------------------ eruptions and sparks
  const grid = (() => {
    const n = BEACON.tableCells;
    if (gpu.tier === 'webgpu') {
      const buffer = lib.cpuBuffer(n * n);
      return {
        read: (ix, iz) => buffer.element(S.int(iz).mul(n).add(S.int(ix))),
        write: (values) => lib.upload(buffer, values),
      };
    }
    const data = new Float32Array(n * n);
    const texture = new T.DataTexture(data, n, n, T.RedFormat, T.FloatType);
    texture.minFilter = T.NearestFilter;
    texture.magFilter = T.NearestFilter;
    texture.generateMipmaps = false;
    texture.needsUpdate = true;
    return {
      read: (ix, iz) => S.textureLoad(texture, S.ivec2(S.int(ix), S.int(iz))).x,
      write(values) {
        data.set(values);
        texture.needsUpdate = true;
      },
    };
  })();
  const fx = {
    // the table's centre and the eruption's origin
    patch: U(new T.Vector3()),
    origin: U(new T.Vector3()),
    count: U(POOLS.eruptions), salt: U(0), dt: U(0),
    sparkAt: U(new T.Vector3()), sparkStart: U(0), sparkN: U(0), sparkSize: U(2), sparkHi: U(0),
    sparkCount: U(POOLS.sparks), sparkSalt: U(0), bal: U(0),
  };
  const ePos = lib.computeBuffer(POOLS.eruptions, 'vec4');
  const eVel = lib.computeBuffer(POOLS.eruptions, 'vec4');
  const eSize = lib.computeBuffer(POOLS.eruptions, 'float');
  const sPos = lib.computeBuffer(POOLS.sparks, 'vec4');
  const sVel = lib.computeBuffer(POOLS.sparks, 'vec4');
  const sSize = lib.computeBuffer(POOLS.sparks, 'float');

  /** The ground at (x, z) from the table, bilinear; the edge cells beyond it. */
  const heightAt = (x, z) => {
    const n = BEACON.tableCells;
    const gx = x.sub(fx.patch.x).div(BEACON.tableStep).add(n / 2);
    const gz = z.sub(fx.patch.z).div(BEACON.tableStep).add(n / 2);
    const cx = S.clamp(gx, 0, n - 1.001);
    const cz = S.clamp(gz, 0, n - 1.001);
    const ix = floor(cx);
    const iz = floor(cz);
    const fxr = cx.sub(ix);
    const fzr = cz.sub(iz);
    const h = (dx, dz) => grid.read(ix.add(dx), iz.add(dz));
    return mix(mix(h(0, 0), h(1, 0), fxr), mix(h(0, 1), h(1, 1), fxr), fzr);
  };

  const eruptSpawn = S.Fn(() => {
    const i = float(instanceIndex);
    const key = i.add(fx.salt.mul(57.3));
    const a = lib.h11(key.mul(1.37)).mul(TAU);
    const el = lib.h11(key.mul(2.11).add(3)).mul(1.1).add(0.35);
    const sp = lib.h11(key.mul(3.7).add(9)).mul(22).add(6);
    const life = lib.h11(key.mul(5.3).add(1)).mul(1.8).add(1.6);
    const live = i.lessThan(fx.count);
    ePos.element(instanceIndex).assign(vec4(fx.origin.x, fx.origin.y.add(0.5), fx.origin.z,
      select(live, life, float(0))));
    eVel.element(instanceIndex).assign(vec4(cos(a).mul(cos(el)).mul(sp).mul(0.55), sin(el).mul(sp),
      sin(a).mul(cos(el)).mul(sp).mul(0.55), 0));
  })().compute(POOLS.eruptions);

  const eruptStep = S.Fn(() => {
    const p = ePos.element(instanceIndex).toVar();
    const v = eVel.element(instanceIndex).toVar();
    If(p.w.greaterThan(0), () => {
      p.w.subAssign(fx.dt);
      If(v.w.lessThan(0.5), () => {
        v.y.subAssign(fx.dt.mul(POOLS.gravity));
        const drag = exp(fx.dt.mul(-POOLS.drag));
        v.x.mulAssign(drag);
        v.y.mulAssign(drag);
        v.z.mulAssign(drag);
        p.x.addAssign(v.x.mul(fx.dt));
        p.y.addAssign(v.y.mul(fx.dt));
        p.z.addAssign(v.z.mul(fx.dt));
        const ground = heightAt(p.x, p.z);
        If(p.y.lessThan(ground), () => {
          p.y.assign(ground);
          v.assign(vec4(0, 0, 0, 1));
          p.w.assign(min(p.w, POOLS.landedS));
        });
      });
    });
    ePos.element(instanceIndex).assign(p);
    eVel.element(instanceIndex).assign(v);
    eSize.element(instanceIndex).assign(select(p.w.greaterThan(0),
      min(p.w, 1).mul(0.38), float(0)));
  })().compute(POOLS.eruptions);

  const sparkSpawn = S.Fn(() => {
    const i = float(instanceIndex);
    const rel = mod(i.sub(fx.sparkStart).add(fx.sparkCount), fx.sparkCount);
    const key = i.add(fx.sparkSalt.mul(31.7));
    If(rel.lessThan(fx.sparkN), () => {
      const a = lib.h11(key.mul(1.91)).mul(TAU);
      const raw = lib.h11(key.mul(2.77).add(4));
      const y = select(fx.sparkHi.greaterThan(0.5), raw.mul(0.8).add(0.2), raw.mul(-0.8).sub(0.2));
      const rr = float(1).sub(y.mul(y)).sqrt();
      const dx = cos(a).mul(rr).add(fx.bal.mul(0.6));
      const dz = sin(a).mul(rr);
      const sp = lib.h11(key.mul(4.1).add(2)).mul(6).add(3).mul(fx.sparkSize).mul(0.5);
      const at = fx.sparkAt;
      const s = fx.sparkSize.mul(1.2);
      sPos.element(instanceIndex).assign(vec4(at.x.add(dx.mul(s)), at.y.add(y.mul(s)),
        at.z.add(dz.mul(s)), lib.h11(key.mul(6.3).add(8)).mul(0.45).add(0.45)));
      sVel.element(instanceIndex).assign(vec4(dx.mul(sp), y.mul(sp), dz.mul(sp), 0));
    });
  })().compute(POOLS.sparks);

  const sparkStep = S.Fn(() => {
    const p = sPos.element(instanceIndex).toVar();
    const v = sVel.element(instanceIndex).toVar();
    If(p.w.greaterThan(0), () => {
      p.w.subAssign(fx.dt);
      const drag = exp(fx.dt.mul(-POOLS.drag));
      v.x.mulAssign(drag);
      v.y.mulAssign(drag);
      v.z.mulAssign(drag);
      p.x.addAssign(v.x.mul(fx.dt));
      p.y.addAssign(v.y.mul(fx.dt));
      p.z.addAssign(v.z.mul(fx.dt));
    });
    sPos.element(instanceIndex).assign(p);
    sVel.element(instanceIndex).assign(v);
    sSize.element(instanceIndex).assign(select(p.w.greaterThan(0),
      min(p.w, 1).mul(fx.sparkSize).mul(0.08), float(0)));
  })().compute(POOLS.sparks);

  const eruptClear = S.Fn(() => {
    ePos.element(instanceIndex).assign(vec4(0));
    eVel.element(instanceIndex).assign(vec4(0));
  })().compute(POOLS.eruptions);
  const sparkClear = S.Fn(() => {
    sPos.element(instanceIndex).assign(vec4(0));
    sVel.element(instanceIndex).assign(vec4(0));
  })().compute(POOLS.sparks);

  const band = (pos) => mix(u.colA, u.colB, float(instanceIndex).mod(3).div(2)).mul(1.8)
    .mul(min(pos.element(instanceIndex).w, 1)).mul(0.85);
  const eruptions = ctx.sprites.make({
    count: POOLS.eruptions, position: ePos, size: eSize, mode: 'world', round: true,
    colour: finish(band(ePos)),
  });
  const sparks = ctx.sprites.make({
    count: POOLS.sparks, position: sPos, size: sSize, mode: 'world', round: true,
    colour: finish(band(sPos).mul(1.2)),
  });

  object.add(bodies, beams, lamps, reflections, eruptions, sparks);
  // the lights and streaks blend over the river (drawn at order 1, writing no depth): they come
  // after it, or the water paints over them
  for (const o of [beams, lamps, reflections, eruptions, sparks]) o.renderOrder = 2;

  // ------------------------------------------------------------------ the layer
  const mix6 = createMix();
  const surge = createSurge();
  const seenPeak = { peak: 0 };
  const state = {
    seed: null, first: null, terrain: null, spawnAt: -1e9, erupts: 0, sparksAt: 0, sparkSeq: 0,
    members: [], eruptions: 0, spinWas: 0, kinds: null,
  };

  /** Place the clusters of the window the camera is in and send them to the passes. */
  function place(seed, z) {
    const terrain = terrainOf(ctx);
    const cells = cellsAt(z);
    // the world's four kinds: a function of the seed and the world's index (0 here, the start of
    // the seed's flight), so a seed's frame never depends on what the page built before it
    state.kinds = worldKinds(seed, 0);
    state.seed = seed;
    state.first = cells[0];
    state.terrain = ctx.terrain;
    state.members = placeBeacons({
      seed, cells, terrain, kinds: state.kinds,
      near: { z: -BEACON.near, kind: state.kinds[0] },
    });
    for (const [set, slots] of [[st.body, 2], [st.lamp, 4], [st.beam, 2]]) {
      const packed = packStatic(state.members, slots);
      for (const key of Object.keys(packed)) lib.upload(set[key], packed[key]);
    }
  }

  /** The origin of an eruption: ahead of the camera on the ground, where the land is. */
  function eruptAt(frame, terrain) {
    const { cam } = frame;
    const fwd = normalize3(cam.lookX - cam.x, 0, cam.lookZ - cam.z);
    const x = cam.x + fwd[0] * 30;
    const z = cam.z + fwd[2] * 30;
    return { x, z, y: Math.max(terrain.height(x, z), 0) };
  }

  const api = {
    name: 'beacons',
    group: 'base',
    object,
    /** What a check reads: the placed members, the pass objects and the particle buffers. */
    state,
    fx,
    statics: st,
    bodyGeometry,
    buffers: { ePos, eVel, sPos, sVel, ...out },
    passes: {
      bodyPasses, lampPasses, beamPasses, eruptClear, sparkClear, eruptSpawn, eruptStep, sparkSpawn,
      sparkStep,
    },
    /** A seed's start: the beacons placed, the spins and the eased mix back at their start and
     * the particles gone, so a frame of the seed reads nothing the frames before it left. */
    build(seed) {
      for (const key of ['sK', 'sN', 'sH', 'sD', 'sU']) bu[key].value = 0;
      mix6.reset();
      surge.reset();
      seenPeak.peak = 0;
      Object.assign(state, { spawnAt: -1e9, sparksAt: 0, pendingSpawn: false });
      state.clear = true;
      place(seed, state.cam ?? 0);
    },

    /** Start an eruption at (`x`, `z`): the heights table from the terrain, and the spawn pass. */
    erupt(x, z, y = null) {
      const terrain = terrainOf(ctx);
      const ground = y ?? Math.max(terrain.height(x, z), 0);
      grid.write(heightsOf(terrain, { x, z }));
      fx.patch.value.set(x, 0, z);
      fx.origin.value.set(x, ground, z);
      fx.salt.value = ++state.erupts;
      state.pendingSpawn = true;
    },

    update(frame) {
      const { u: rig, cam, dt = 0 } = frame;
      state.cam = cam.z;
      const terrain = terrainOf(ctx);
      const groove = grooveOf(frame);
      const calm = rig.calm > 0.5;
      const mixed = mix6.step(dt, groove.section);
      const events = eventsOf(ctx, frame, seenPeak);
      // the window of cells follows the camera; a new seed or terrain places it again
      const first = cellsAt(cam.z)[0];
      if (frame.seed !== state.seed || first !== state.first || ctx.terrain !== state.terrain) {
        place(frame.seed ?? state.seed ?? 0, cam.z);
      }
      bu.kk.value = calm ? 0.3 : groove.kick;
      bu.sn.value = calm ? 0.3 : groove.snare;
      bu.ht.value = calm ? 0.3 : groove.hat;
      bu.dn.value = calm ? 0 : groove.down;
      bu.E.value = groove.energy;
      bu.bt.value = groove.beat;
      bu.bp.value = groove.bar;
      bu.kn.value = groove.kn;
      bu.sqn.value = groove.sn;
      bu.sus.value = clamp(rig.mid ?? 0.3);
      bu.bg.value = mixed.bg;
      bu.am.value = calm ? 0.2 : 1;
      bu.drop.value = groove.section === 'drop' ? 1 : 0;
      bu.F.value = surge.step(dt, events, calm);
      bu.sK.value += dt * bu.kk.value;
      bu.sN.value += dt * bu.sn.value;
      bu.sH.value += dt * bu.ht.value;
      bu.sD.value += dt * bu.dn.value;
      bu.sU.value += dt * bu.sus.value;
      const passes = [...bodyPasses, ...lampPasses, ...beamPasses];
      if (state.clear) {
        passes.push(eruptClear, sparkClear);
        state.clear = false;
      }

      // the particles: the events call for an eruption (a drop's entry, a peak in a drop, once
      // every few seconds) and for sparks (a hit); the limiter gates the eruptions
      const counts = countsOf(frame.budget?.q ?? 1, caps);
      fx.count.value = counts.eruptions;
      fx.sparkCount.value = counts.sparks;
      eruptions.count = counts.eruptions;
      sparks.count = counts.sparks;
      fx.dt.value = dt;
      const flash = flashOf(ctx, frame);
      const hero = ctx.hero?.at ?? null;
      let burst = 0;
      let hi = 0;
      for (const e of events) {
        if (calm) break;
        const gapOk = frame.t - state.spawnAt > POOLS.eruptGapS;
        if (erupts(e, { section: groove.section, gapOk, ask: () => flash(frame.t) })) {
          state.spawnAt = frame.t;
          const at = eruptAt(frame, terrain);
          api.erupt(at.x, at.z, at.y);
        }
        if (e.type === 'kick') burst += 12;
        else if (e.type === 'snare') {
          burst += 10;
          hi = 1;
        } else if (e.type === 'hat') {
          burst += 5;
          hi = 1;
        }
      }
      if (burst > 0) {
        const at = hero ?? {
          x: cam.x + (cam.lookX - cam.x) * 0.4, y: cam.y + 1, z: cam.z + (cam.lookZ - cam.z) * 0.4,
          size: 2,
        };
        fx.sparkAt.value.set(at.x, at.y, at.z);
        fx.sparkSize.value = at.size ?? 2;
        fx.sparkStart.value = state.sparksAt;
        fx.sparkN.value = Math.min(burst, counts.sparks);
        fx.sparkHi.value = hi;
        fx.sparkSalt.value = ++state.sparkSeq;
        state.sparksAt = (state.sparksAt + fx.sparkN.value) % counts.sparks;
        passes.push(sparkSpawn);
      }
      if (state.pendingSpawn) {
        passes.push(eruptSpawn);
        state.pendingSpawn = false;
      }
      if (dt > 0) passes.push(eruptStep, sparkStep);
      gpu.renderer.compute(passes);
    },
    dispose() {
      for (const mesh of [bodies, beams]) {
        mesh.geometry.dispose();
        mesh.material.dispose();
      }
      for (const sprite of [lamps, reflections, eruptions, sparks]) sprite.material.dispose();
    },
  };
  return api;
};

/** A direction `(x, y, z)` as an array of unit length (the first axis when it has none). */
function normalize3(x, y, z) {
  const len = Math.hypot(x, y, z);
  return len > 1e-6 ? [x / len, y / len, z / len] : [0, 0, -1];
}
