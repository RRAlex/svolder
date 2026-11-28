/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's hero layer: the sculpture the music deforms and morphs, hung over the river. Its
// factory is handed the scene's context (`ctx`: the vendored namespaces `T` and `S`, the shared
// `lib` and `sprites`, the capability bag `caps`, the uniform block `u`, the audio buffers, the
// `scene`, `camera` and `terrain`) and returns `{ name, group, object, build(seed), update(frame),
// dispose() }`.
//
// Nine forms: a lattice planet and a noise icosphere (wireframes with a halo), five forms made of
// particles that follow the spectrum (sphere, torus, cube, helix, spiral), a glyph swarm, and the
// sculpture, ten parametric and implicit forms of 50000 points that morph into each other. All
// of them deform with the music: the kick squashes and pumps, a twist sways over two bars, each
// bar bends the other way, the snare folds spikes, the hats shimmer, and a burst scatters the
// particles and lets them reform. The particle forms are stateful: each particle's offset from
// its form and its velocity live in compute buffers (a damped spring, stepped by a compute pass
// on both tiers) and the burst is an impulse into that state. The hero's placement, its groove
// `ctx.hero` is published each frame: `{ t, sdt, at: { x, y, z, size }, pos, hs, form, swarm, kick,
// lift, px, foc }` (`at` is where the beacons' sparks leave from; `sdt` the rig's step in seconds).
// The hero's placement, its groove
// and its form schedule are pure (`createHeroState`, `createHeroPlan`), driven by the director's
// events when the frame carries them and by the cue numbers when it does not.
//
//   heroLayer(ctx)                the layer
//   FORMS, SCULPTS                the nine hero forms and the sculpture's ten
//   createHeroPlan(seed)          the form schedule: `state`, `onMorph()`, `onSection()`, `to(f)`,
//                                 `sculpt(k)`, `step(sdt)`, `settle()`
//   createHeroState(seed)         the placement and the deformation: `step(input)` -> `{ ... }`
//   createMorphWatch()            a morph trigger read off the eased `u.morph` number
//   createGrooveWatch()           kick, snare and hat envelopes from the band numbers
//   heroCounts(opts)              the particle counts under the budget and the cap
//   crossfadeCounts(n, w, cap)    what both drawings may show at once (a crossfade shares the cap)
//   eqTables(n, seed), eqShape(k, q)   the particles' keys, and a shape's place for a key
//   HERO                          the numbers
//   INSTRUMENT, ringData()        the instrument's counts and its meshes' per-vertex numbers
//   heroScreen(d, view)           the hero's place on screen, as the scope reads it
//   heroInstrument(env)           the spectrum ring and the scope: `{ ring, scope }`

import { hashStr, mulberry } from '../rng.js';
import { particleCounts } from './particles.js';

export const FORMS = Object.freeze([
  'lattice planet', 'noise icosphere', 'EQ sphere', 'EQ torus', 'EQ cube', 'EQ helix',
  'EQ spiral', 'glyph swarm', 'sculpture',
]);
export const SCULPTS = Object.freeze([
  'knot', 'gyroid', 'galaxy', 'lotus', 'lissajous', 'superformula', 'helix', 'moebius', 'nova',
  'swarm',
]);

/** The hero's numbers: particle counts, the size by section, the springs (per second), how long
 * the burst lasts in beats, and how long the forms take to change (seconds). */
export const HERO = Object.freeze({
  eq: 22000,
  sculpt: 50000,
  webgpuFactor: 2,
  minimum: 600,
  size: Object.freeze({ intro: 1.7, build: 2.7, drop: 4, break: 2.1 }),
  omega: 4.5,
  burstSpeed: 14,
  burstSwirl: 3,
  burstBeats: 3.2,
  stepMax: 1 / 20,
  weightRate: 1.6,
  eqMorphS: 2.5,
  sculptMorphS: 4.5,
  morphSide: 0.05,
  fogD: 0.015,
});

const TAU = Math.PI * 2;
const clamp = (v, a = 0, b = 1) => (v < a ? a : v > b ? b : v);
const ease = (dt, rate) => 1 - Math.exp(-dt * rate);
const smooth = (a, b, x) => {
  const t = clamp((x - a) / (b - a));
  return t * t * (3 - 2 * t);
};
const fr = (x) => x - Math.floor(x);

/** The particle counts under the budget's factor `q` and the cap: the EQ forms and the sculpture
 * (`n` drawn, `max` held). */
export function heroCounts({ q = 1, caps = null } = {}) {
  const factor = caps?.tier === 'webgpu' ? HERO.webgpuFactor : 1;
  const part = (base) => particleCounts({ base, q, caps, factor, minimum: HERO.minimum });
  return { eq: part(HERO.eq), sculpt: part(HERO.sculpt) };
}

/**
 * What the two particle drawings put on screen at once: each count held to its own `n`, and while
 * both show (a crossfade) to a share of `cap` by their weights `w[2]` (EQ) and `w[3]` (sculpture),
 * so their sum never passes the cap. Returns `{ eq, sculpt }`.
 */
export function crossfadeCounts(n, w, cap = Infinity) {
  const eqOn = w[2] > 0.01;
  const scOn = w[3] > 0.01;
  let eq = eqOn ? n.eq.n : 0;
  let sculpt = scOn ? n.sculpt.n : 0;
  if (eq + sculpt > cap) {
    const share = w[2] / (w[2] + w[3]);
    eq = Math.min(eq, Math.floor(cap * share));
    sculpt = Math.min(sculpt, cap - eq);
  }
  return { eq, sculpt };
}

/** Per-form parameters of the sculpture's form `k`: a few numbers picked from `rng`. */
export function kFor(k, rng) {
  const pick = (a) => a[Math.floor(rng() * a.length)];
  switch (k) {
    case 0: return [...pick([[2, 3], [3, 2], [2, 5], [3, 4], [3, 5], [4, 3]]), 0, 0];
    case 1: return [2.6 + rng() * 1.2, 0, 0, 0];
    case 2: return [2 + Math.floor(rng() * 4), 2.4 + rng() * 2, 0, 0];
    case 3: return [5 + Math.floor(rng() * 5), 0, 0, 0];
    case 4: return [...pick([[3, 2, 5], [1, 4, 3], [5, 4, 3], [3, 5, 7], [2, 3, 4]]), 0];
    case 5: return pick([[6, 1, 1, 1], [7, 0.2, 1.7, 1.7], [4, 0.6, 0.5, 4], [5, 0.3, 0.3, 0.3],
      [8, 0.6, 1, 8], [3, 0.5, 1, 1]]);
    case 6: return [2.2 + rng() * 1.4, 0, 0, 0];
    case 7: return [rng() < 0.6 ? 1 : 3, 0, 0, 0];
    default: return [0, 0, 0, 0];
  }
}

/** The next of `list` from a shuffled bag (refilled when empty), not `avoid` first. */
export function bagNext(bag, list, rng, avoid) {
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

const FORM_IDS = FORMS.map((_, i) => i);
const SCULPT_IDS = SCULPTS.map((_, i) => i);

/**
 * The form schedule of one seed. A morph trigger of the director changes the hero: on the
 * sculpture it starts the morph to the next sculpture form (half the time), otherwise it goes to
 * the next form of a shuffled bag; a section change always goes to the next form. `step(sdt)` moves
 * the weights of the four drawings (lattice, icosphere, EQ forms, sculpture), the glyph swarm's
 * weight, the EQ shapes' morph and the sculpture's morph (about 4.5 s).
 */
export function createHeroPlan(seed) {
  const rng = mulberry(hashStr(`hero:${seed}`));
  const bags = { form: [], sculpt: [] };
  const s = {
    form: 0, w: [1, 0, 0, 0], gly: 0, pqA: 0, pqB: 0, pqT: 1, shA: 0, shB: 0, mt: 0, scM: -1,
    kA: [0, 0, 0, 0], kB: [0, 0, 0, 0], changes: 0, morphs: 0,
    // the instrument's parts (the ring's bars, the band curves, the scope): which are on, and their
    // eased weights; a stream of their own, so the forms' picks do not move
    ring: true, curves: true, scope: true, rw: 1, rcw: 1, sw: 1,
  };
  const parts = mulberry(hashStr(`hero-parts:${seed}`));
  const targets = () => [
    s.form === 0 ? 1 : 0, s.form === 1 ? 1 : 0, s.form >= 2 && s.form <= 7 ? 1 : 0,
    s.form === 8 ? 1 : 0,
  ];
  function pickSculpt(avoid) {
    return bagNext(bags.sculpt, SCULPT_IDS, rng, avoid);
  }
  function heroTo(f) {
    if (f >= 2 && f <= 6) {
      const cur = s.pqT >= 1 ? s.pqB : s.pqA;
      if (s.w[2] > 0.3) {
        s.pqA = cur;
        s.pqB = f - 2;
        s.pqT = 0;
      } else {
        s.pqA = f - 2;
        s.pqB = f - 2;
        s.pqT = 1;
      }
    }
    if (f === 8 && s.form !== 8) {
      s.shA = pickSculpt(s.shA);
      s.kA = kFor(s.shA, rng);
      s.mt = 0;
      s.scM = -1;
    }
    if (f !== s.form) s.changes++;
    s.form = f;
  }
  function sculptMorph() {
    s.scM = 0;
    s.shB = pickSculpt(s.shA);
    s.kB = kFor(s.shB, rng);
  }
  heroTo(0);
  s.w = targets();
  s.gly = s.form === 7 ? 1 : 0;
  s.pqA = s.pqB = Math.max(0, Math.min(4, s.form - 2));
  s.pqT = 1;
  return {
    state: s,
    onMorph() {
      s.morphs++;
      if (s.form === 8 && s.scM < 0 && rng() < 0.5) sculptMorph();
      else heroTo(bagNext(bags.form, FORM_IDS, rng, s.form));
      // each 8-bar morph the parts may toggle
      if (parts() < 0.5) s.ring = !s.ring;
      if (parts() < 0.4) s.curves = !s.curves;
      if (parts() < 0.4) s.scope = !s.scope;
    },
    /** A section of `type` (intro, build, drop, break): the next form, and the parts it stages. */
    onSection(type = 'build') {
      heroTo(bagNext(bags.form, FORM_IDS, rng, s.form));
      s.ring = type !== 'intro' || parts() < 0.5;
      s.curves = parts() < 0.6 || type === 'drop';
      s.scope = type === 'drop' || (type === 'build' ? parts() < 0.7 : parts() < 0.3);
    },
    /** Go to form `f` at once (a test, the debug readout). */
    to(f) {
      heroTo(f);
    },
    /** The sculpture's form `k`, with parameters from the seed's stream, no morph running. */
    sculpt(k) {
      s.shA = k;
      s.kA = kFor(k, rng);
      s.mt = 0;
      s.scM = -1;
    },
    step(sdt) {
      const k = ease(sdt, HERO.weightRate);
      const t = targets();
      for (let i = 0; i < 4; i++) s.w[i] += (t[i] - s.w[i]) * k;
      s.gly += ((s.form === 7 ? 1 : 0) - s.gly) * k;
      s.rw += ((s.ring ? 1 : 0) - s.rw) * k;
      s.rcw += ((s.curves ? 1 : 0) - s.rcw) * k;
      s.sw += ((s.scope ? 1 : 0) - s.sw) * k;
      s.pqT = Math.min(1, s.pqT + sdt / HERO.eqMorphS);
      if (s.scM >= 0) {
        s.scM += sdt / HERO.sculptMorphS;
        s.mt = clamp(s.scM);
        if (s.scM >= 1) {
          s.shA = s.shB;
          s.kA = s.kB;
          s.mt = 0;
          s.scM = -1;
        }
      }
    },
    /** All the weights at their targets and no morph running (a frame read from rest). */
    settle() {
      s.w = targets();
      s.gly = s.form === 7 ? 1 : 0;
      s.rw = s.ring ? 1 : 0;
      s.rcw = s.curves ? 1 : 0;
      s.sw = s.scope ? 1 : 0;
      s.pqT = 1;
      s.pqA = s.pqB;
    },
  };
}

/** A morph trigger read off `u.morph` (the director's toggle, eased): it fires when the number
 * crosses the middle in either direction, with a little hysteresis. `sample(v)` returns 1 or 0. */
export function createMorphWatch() {
  let side = null;
  return {
    sample(v) {
      if (!Number.isFinite(v)) return 0;
      if (side === null) {
        side = v > 0.5 ? 1 : 0;
        return 0;
      }
      if (side === 0 && v > 0.5 + HERO.morphSide) {
        side = 1;
        return 1;
      }
      if (side === 1 && v < 0.5 - HERO.morphSide) {
        side = 0;
        return 1;
      }
      return 0;
    },
    reset() {
      side = null;
    },
  };
}

/** Kick, snare and hat envelopes from the band numbers `u` (bass, mid, high), for a frame that
 * carries no director: an onset over a slow mean starts an envelope, at most one a 0.2 s (hats
 * 0.07 s). `step(dt, u)` returns `{ kick, snare, hat, hits }`. */
export function createGrooveWatch() {
  const gap = [0.2, 0.2, 0.07];
  const decay = [6, 6, 16];
  const hits = [0, 0, 0];
  const h = [0, 1, 2].map(() => ({ since: 9, mean: 0, amp: 0 }));
  const out = { kick: 0, snare: 0, hat: 0, hits };
  return {
    step(dt, u, bpm = 120) {
      const src = [u.bass, u.mid, u.high];
      for (let i = 0; i < 3; i++) {
        const e = h[i];
        e.since += dt;
        const v = Number.isFinite(src[i]) ? src[i] : 0;
        const onset = v - e.mean;
        e.mean += (v - e.mean) * ease(dt, 3);
        if (onset > 0.09 && e.since > gap[i]) {
          e.since = 0;
          e.amp = clamp(onset * 3);
          hits[i]++;
        }
      }
      const env = (e, k) => e.amp * smooth(0, 0.035, e.since) * Math.exp(-e.since * k);
      out.kick = env(h[0], (3 * bpm) / 60);
      out.snare = env(h[1], decay[1]);
      out.hat = env(h[2], decay[2]);
      return out;
    },
  };
}

const FLAT = Object.freeze({ height: () => 0, riverX: () => 0 });

/**
 * The hero's placement and deformation numbers. `step(input)`: `dt` and `sdt` (the page's seconds
 * and the rig's scaled ones), `u` (the rig's numbers), `cam` `{ x, y, z, fov, aspect }` with the
 * camera's `right`, `up` and `forward` vectors, `terrain`, `director` (its state or null),
 * `events` (this step's, or null), `snap`. Returns the live object (the same one each time): `pos`,
 * `hs` (size), `rotX`, `rotY`, `tw`, `bd`, `ex`, `kk`, `sn`, `ht`, `hsd`, `burst`, `bursts`,
 * `swarmKick`.
 */
export function createHeroState(seed) {
  const rng = mulberry(hashStr(`hero-state:${seed}`));
  const plan = createHeroPlan(seed);
  const morphs = createMorphWatch();
  const groove = createGrooveWatch();
  const wander = rng() * TAU;
  const own = {
    bal: 0, hs: 2, hy: null, rotY: rng() * TAU, rotX: 0, rOff: 0, rTgt: 0, bdS: 1, bdA: 0,
    bar: -1, exB: -9, burst: 0, bursts: 0, beat: 0, lastTime: null, peakSeen: 0, pulse: 0,
  };
  const sectionTypes = [];
  const out = {
    pos: [0, 5, -20], hs: 2, rotX: 0, rotY: 0, tw: 0, bd: 0, ex: 0, kk: 0, sn: 0, ht: 0, hsd: 0,
    burst: 0, bursts: 0, swarmKick: 0, pulse: 0, lift: 0, section: 'build', energy: 0.3,
  };

  return {
    plan,
    own,
    out,
    step(input) {
      const { dt, sdt, u, cam, director = null, snap = false, flash = null } = input;
      // a frame with a director has its events (none this step: an empty list); only a frame
      // without one reads the triggers off the numbers
      const events = input.events ?? (director ? [] : null);
      const terrain = input.terrain ?? FLAT;
      const calm = u.calm > 0.5;
      const am = calm ? 0.3 : 1;
      const loud = Number.isFinite(u.loud) ? u.loud : 0.35;
      const bass = Number.isFinite(u.bass) ? u.bass : 0.3;

      // the director's numbers, or the band numbers' own
      let g;
      let beat;
      let section;
      if (director) {
        g = { ...director.groove };
        g.hits = director.hits ?? [0, 0, 0];
        beat = director.beat;
        section = director.section;
      } else {
        const w = groove.step(dt, u);
        g = {
          kick: w.kick, snare: w.snare, hat: w.hat, hits: w.hits,
          energy: clamp(loud * 1.6),
        };
        beat = own.beat + (dt * 120) / 60;
        section = loud > 0.5 ? 'drop' : loud < 0.22 ? 'break' : 'build';
      }
      own.beat = beat;

      // the triggers: the director's events, else the numbers' own
      let morphed = 0;
      let sectioned = 0;
      if (events) {
        for (const e of events) {
          if (e.type === 'morph') morphed++;
          if (e.type === 'section') sectionTypes[sectioned++] = e.section ?? section;
        }
      } else {
        morphed = morphs.sample(u.morph);
      }
      // the peak's pulse is a flash, never in Calm. With the director's events it rides the peak
      // event's own grant (`flash`, as the beacons and the post chain read it): one grant a peak.
      // Without them it asks the frame's limiter on the peak's rising edge.
      const peakNow = Number.isFinite(u.peak) ? u.peak : 0;
      if (!snap && !calm) {
        if (events) {
          if (events.some((e) => e.type === 'peak' && e.flash === true)) own.pulse = 1;
        } else if (own.peakSeen <= 0.9 && peakNow > 0.9 && flash && flash()) {
          own.pulse = 1;
        }
      }
      own.peakSeen = peakNow;
      own.pulse *= Math.exp(-sdt * 3);
      own.burst = 0;
      if (!snap) {
        for (let i = 0; i < morphed; i++) {
          const was = plan.state.changes;
          plan.onMorph();
          if (plan.state.changes !== was && !calm) own.burst = 1;
        }
        for (let i = 0; i < sectioned; i++) {
          plan.onSection(sectionTypes[i]);
          if (!calm) {
            own.burst = 1;
            own.exB = beat;
          }
        }
        if (own.burst) own.bursts++;
      }
      if (snap) {
        plan.settle();
      } else {
        plan.step(sdt);
      }

      // the size, the balance and the height
      const sizeBase = HERO.size[section] ?? HERO.size.build;
      const sizeT = calm ? clamp(sizeBase, 1.2, 2.4) : sizeBase;
      own.hs = snap || own.hy === null ? sizeT : own.hs + (sizeT - own.hs) * ease(sdt, 0.5);
      const hs = own.hs * (0.85 + 0.35 * loud) * (1 + 0.05 * bass);
      const wal = ((u.r ?? 0) - (u.l ?? 0)) / ((u.l ?? 0) + (u.r ?? 0) + 0.05);
      own.bal += (wal - own.bal) * (snap ? 1 : ease(dt, 2));
      const cz = cam.z;
      const az = cz - (15 + 4 * own.hs);
      const hx = terrain.riverX(az);
      const gy = Math.max(terrain.height(hx, az), 0);
      const hyT = Math.max(gy + 1.6 + hs * 1.25, cam.y * 0.35 + hs * 1.1 + 1.5)
        + ((u.cent ?? 0.4) - 0.45) * hs * 1.8 + hs * 0.3;
      own.hy = own.hy === null || snap ? hyT : own.hy + (hyT - own.hy) * ease(dt, 1.2);
      const lean = own.bal * hs * 0.6;
      const pos = [
        hx + cam.right[0] * lean, own.hy + cam.right[1] * lean, az + cam.right[2] * lean,
      ];

      // the dance: a wander in the centre third, a push toward the camera on each kick, a turn
      // on each bar
      const dx = pos[0] - cam.x;
      const dy = pos[1] - cam.y;
      const dz = pos[2] - cam.z;
      const dist = Math.max(8, Math.hypot(dx, dy, dz));
      const th = Math.tan((cam.fov * Math.PI) / 360);
      const hw = dist * th * cam.aspect;
      const hv = dist * th;
      const E = g.energy;
      const side = clamp(
        hw * 0.2 * (0.75 * Math.sin((TAU * beat) / 16 + wander)
          + 0.25 * Math.sin((TAU * beat) / 40 + 1.3)) * am * (0.5 + 0.5 * E),
        -hw * 0.3, hw * 0.3,
      );
      const rise = hv * 0.12 * Math.sin((TAU * beat) / 24 + wander * 1.7) * am;
      const push = Math.min(dist * 0.32, g.kick * (5 + 3.4 * own.hs) * (0.45 + 0.55 * E) * am);
      const eb = beat - own.exB;
      let ex = calm || eb < 0 || eb > HERO.burstBeats
        ? 0 : smooth(0, 0.4, eb) * (1 - smooth(1, 2.9, eb));
      ex *= clamp(3.2 / hs, 0.5, 1);
      const fwd = cam.forward;
      const pull = push * (1 - 0.85 * ex);
      pos[0] += cam.right[0] * side + cam.up[0] * rise - fwd[0] * pull;
      pos[1] += cam.right[1] * side + cam.up[1] * rise - fwd[1] * pull;
      pos[2] += cam.right[2] * side + cam.up[2] * rise - fwd[2] * pull;

      const bar = Math.floor(beat / 4);
      if (bar !== own.bar) {
        if (own.bar >= 0 && !calm && !snap) {
          own.rTgt += 0.3 + 0.45 * E;
          own.bdS = -own.bdS;
          own.bdA = 1;
        }
        own.bar = bar;
      }
      own.rOff += (own.rTgt - own.rOff) * ease(dt, 7);
      own.bdA *= Math.exp(-dt * 1.5);
      own.rotY += sdt * 0.2 * (1 + loud);

      out.pos = pos;
      out.hs = hs;
      out.rotY = own.rotY + own.rOff;
      out.rotX = 0.25 * Math.sin((u.time ?? 0) * 0.1);
      out.tw = (1.15 * E * Math.sin((TAU * beat) / 8)) * (calm ? 0.25 : 1);
      out.bd = calm ? 0 : own.bdS * own.bdA * (0.35 + 0.65 * E);
      out.ex = ex;
      out.kk = g.kick * am;
      out.sn = g.snare * am;
      out.ht = g.hat * am;
      out.hsd = (g.hits?.[1] ?? 0) % 1000;
      out.burst = own.burst;
      out.bursts = own.bursts;
      out.pulse = own.pulse;
      out.swarmKick = g.kick * am;
      out.lift = E * am;
      out.section = section;
      out.energy = E;
      return out;
    },
  };
}

/**
 * The keys of the EQ forms' `n` particles (four numbers each: `u` along the form, a random
 * number, the index's parity, the golden angle of the index), shuffled so that the first `k` of
 * them are an even thinning of the whole. Flat xyzw array.
 */
export function eqTables(n, seed) {
  const rng = mulberry(hashStr(`eq:${seed}`));
  const keys = new Float32Array(n * 4);
  for (let i = 0; i < n; i++) {
    keys.set([(i + 0.5) / n, rng(), i % 2, (i * 2.399963) % TAU], i * 4);
  }
  const order = Array.from({ length: n }, (_, i) => i);
  for (let i = n - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    [order[i], order[j]] = [order[j], order[i]];
  }
  const q = new Float32Array(n * 4);
  order.forEach((from, to) => q.set(keys.subarray(from * 4, from * 4 + 4), to * 4));
  return { q };
}

/** The place of an EQ particle with keys `q` (`[u, random, parity, angle]`) on shape `k` (0
 * sphere, 1 torus, 2 cube, 3 helix, 4 spiral): the CPU mirror of the compute pass's shapes. */
export function eqShape(k, q) {
  const [u, rn, parity, a] = q;
  const y = -1 + 2 * u;
  switch (k) {
    case 0: {
      const rr = Math.sqrt(Math.max(0, 1 - y * y));
      return [Math.cos(a) * rr * 1.25, y * 1.25, Math.sin(a) * rr * 1.25];
    }
    case 1: {
      const tb = fr(u * 233) * TAU;
      const r0 = 1.1 + 0.42 * Math.cos(tb);
      return [r0 * Math.cos(u * TAU), 0.42 * Math.sin(tb), r0 * Math.sin(u * TAU)];
    }
    case 2: {
      const pp = fr(u * 160) * 4;
      const side = Math.floor(pp);
      const f = fr(pp) * 2 - 1;
      return [[f, 1, -f, -1][side] * 0.95, y * 0.95, [-1, f, 1, -f][side] * 0.95];
    }
    case 3: {
      const ha = u * TAU * 4 + parity * Math.PI;
      const hr = 0.6 + (rn - 0.5) * 0.1;
      return [Math.cos(ha) * hr, y * 1.7, Math.sin(ha) * hr];
    }
    default: {
      const sr = Math.sqrt(u) * 1.8;
      const sa = u * TAU * 7 + (Math.floor(rn * 3) * TAU) / 3 + sr * 0.6;
      return [Math.cos(sa) * sr, (rn - 0.5) * 0.1 * (1.9 - sr), Math.sin(sa) * sr];
    }
  }
}

/** The sculpture's `n` random keys (four numbers each). */
export function sculptKeys(n, seed) {
  const rng = mulberry(hashStr(`sculpt:${seed}`));
  const a = new Float32Array(n * 4);
  for (let i = 0; i < a.length; i++) a[i] = rng();
  return a;
}

// ------------------------------------------------------------------------------------------------
// the shading: TSL graphs, built once per layer

/** The hero's uniforms, the same on every pass and material. */
function heroUniforms(T, S) {
  const f = (v = 0) => S.uniform(v);
  return {
    w: [f(1), f(0), f(0), f(0)],
    hs: f(2), lev: f(0), gly: f(0), pqA: f(0), pqB: f(0), pqT: f(1),
    tw: f(0), bd: f(0), ex: f(0), sn: f(0), ht: f(0), kk: f(0), hsd: f(0),
    shA: f(0), shB: f(0), mt: f(0), turb: f(0.8), open: f(0.5), bright: f(1.1),
    kA: S.uniform(new T.Vector4()), kB: S.uniform(new T.Vector4()),
    camR: S.uniform(new T.Vector3(1, 0, 0)), hero: S.uniform(new T.Vector3()),
    rot: S.uniform(new T.Matrix3()),
    px: f(1), foc: f(600), focusD: f(20),
    rw: f(1), rcw: f(1), sw: f(1), res: S.uniform(new T.Vector2(1280, 800)),
    scope: S.uniform(new T.Vector3()),
    fogD: f(HERO.fogD), peak: f(0), burst: f(0), dt: f(0), snap: f(1), active: [f(0), f(0)],
  };
}

/** The bars, the curve points and the scope's samples. */
export const INSTRUMENT = Object.freeze({ bars: 128, curves: 3, curvePoints: 180, scope: 256 });

/** The geometry data of both meshes, flat arrays. A quad is two corners across (`side` -1, +1) at
 * each of two places along; its four corners make two triangles. */
export function ringData() {
  const aR = [];
  const index = [];
  const quad = (a, b, c, e) => {
    const o = aR.length / 4;
    aR.push(...a, ...b, ...c, ...e);
    index.push(o, o + 1, o + 2, o + 1, o + 3, o + 2);
  };
  for (let i = 0; i < INSTRUMENT.bars; i++) {
    const u = (i + 0.5) / INSTRUMENT.bars;
    quad([0, u, -1, 0], [0, u, 1, 0], [0, u, -1, 1], [0, u, 1, 1]);
  }
  for (let b = 0; b < INSTRUMENT.curves; b++) {
    for (let j = 0; j < INSTRUMENT.curvePoints; j++) {
      const u0 = j / INSTRUMENT.curvePoints;
      const u1 = (j + 1) / INSTRUMENT.curvePoints;
      quad([1, u0, -1, b], [1, u0, 1, b], [1, u1, -1, b], [1, u1, 1, b]);
    }
  }
  quad([2, 0, -1, 0], [2, 1, -1, 0], [2, 0, 1, 0], [2, 1, 1, 0]);
  const aW = [];
  const wIndex = [];
  for (let i = 0; i <= INSTRUMENT.scope; i++) {
    aW.push(i / INSTRUMENT.scope, -1, i / INSTRUMENT.scope, 1);
    const o = i * 2;
    if (i < INSTRUMENT.scope) wIndex.push(o, o + 1, o + 2, o + 1, o + 3, o + 2);
  }
  return {
    aR: new Float32Array(aR), index: new Uint32Array(index), aW: new Float32Array(aW),
    wIndex: new Uint32Array(wIndex),
  };
}

/** The hero's place on screen as the scope reads it: `{ x, y, on }` in clip units from the
 * camera's basis (`right`, `up`, `forward`: the view's axes) and the hero's offset `d` from the
 * camera; `on` is 1 in front of the camera, else 0. It fills `out` (a new object by default). */
export function heroScreen(d, { right, up, forward, fov, aspect }, out = {}) {
  const depth = d[0] * forward[0] + d[1] * forward[1] + d[2] * forward[2];
  const th = Math.tan((fov * Math.PI) / 360);
  const w = Math.max(Math.abs(depth), 0.1);
  out.x = (d[0] * right[0] + d[1] * right[1] + d[2] * right[2]) / (w * th * aspect);
  out.y = (d[0] * up[0] + d[1] * up[1] + d[2] * up[2]) / (w * th);
  out.on = depth > 0 ? 1 : 0;
  return out;
}

/**
 * The hero's instrument, two meshes: a ring of 128 spectrum bars, three band curves and a faint
 * disc in the plane facing the camera round the hero (type 0, 1, 2 of the ring's `aR`), and an
 * oscilloscope ribbon on screen above it. `env`: `T`, `S`, `lib`, `u`, `h` the hero's uniforms,
 * `bandC`, `spec`, `fog`, `wave`. Returns `{ ring, scope }`.
 */
export function heroInstrument({ T, S, lib, u, h, bandC, spec, fog, wave }) {
  const {
    float, vec2, vec3, vec4, sin, cos, select, smoothstep, normalize, abs, floor, exp,
  } = S;
  const data = ringData();
  const geometry = (attribute, name, size, index, count) => {
    const g = new T.BufferGeometry();
    g.setAttribute('position', new T.BufferAttribute(new Float32Array(count * 3), 3));
    g.setAttribute(name, new T.BufferAttribute(attribute, size));
    g.setIndex(new T.BufferAttribute(index, 1));
    return g;
  };
  const additive = (m) => {
    m.transparent = true;
    m.depthWrite = false;
    m.blending = T.AdditiveBlending;
    m.side = T.DoubleSide;
    return m;
  };

  // ---- the ring: type 0 spectrum bars (low at the bottom, high at the top, mirrored), type 1
  // three band curves, type 2 a translucent disc
  const ringMaterial = additive(new T.MeshBasicNodeMaterial());
  {
    const r = S.attribute('aR', 'vec4');
    const ty = r.x;
    const along = r.y;
    const sd = r.z;
    const e = r.w;
    const mirror = float(1).sub(abs(along.mul(2).sub(1)));
    // the bars
    const sBar = spec(mirror);
    const aBar = along.mul(TAU).sub(Math.PI / 2);
    const dr = vec2(cos(aBar), sin(aBar));
    const pBar = dr.mul(e.mul(sBar.mul(1.25).add(0.05)).add(1.62))
      .add(vec2(dr.y.negate(), dr.x).mul(sd.mul(0.022)));
    const cBar = bandC(mirror).mul(h.rw).mul(sBar.mul(1.5).add(0.25));
    // the band curves
    const b = floor(e.add(0.5));
    const f = b.add(mirror).div(3);
    const a1 = along.mul(TAU).sub(Math.PI / 2);
    const a2 = a1.add(0.02);
    const r0 = b.mul(0.34).add(2);
    const k = b.mul(0.3).add(1).mul(0.5);
    const f2 = b.add(1).sub(abs(along.add(0.0031831).mul(2).sub(1))).div(3);
    const q1 = vec2(cos(a1), sin(a1)).mul(r0.add(spec(f).mul(k)));
    const q2 = vec2(cos(a2), sin(a2)).mul(r0.add(spec(f2).mul(k)));
    const t = normalize(q2.sub(q1));
    const pCur = q1.add(vec2(t.y.negate(), t.x).mul(sd.mul(0.016)));
    const cCur = bandC(b.add(0.5).div(3)).mul(h.rcw).mul(spec(f).mul(1.3).add(0.45));
    // the disc
    const pDisc = vec2(along.mul(2).sub(1), sd).mul(2.75);
    const cDisc = bandC(u.cent).mul(h.rcw);
    const p = select(ty.lessThan(0.5), pBar, select(ty.lessThan(1.5), pCur, pDisc));
    const c = select(ty.lessThan(0.5), cBar, select(ty.lessThan(1.5), cCur, cDisc));
    const mv = S.modelViewMatrix.mul(vec4(p, 0, 1));
    ringMaterial.positionNode = vec3(p, 0);
    const vC = S.varying(c.mul(fog(mv.z.negate())));
    const vP = S.varying(pDisc.div(2.75));
    const vT = S.varying(ty);
    const rr = S.length(vP);
    const disc = smoothstep(1, 0.93, rr).mul(smoothstep(0.55, 0.95, rr).mul(0.09).add(0.035));
    ringMaterial.colorNode = vec4(lib.finish(select(vT.greaterThan(1.5), vC.mul(disc), vC), u), 1);
  }
  const ring = new T.Mesh(
    geometry(data.aR, 'aR', 4, data.index, data.aR.length / 4), ringMaterial,
  );
  ring.frustumCulled = false;

  // ---- the scope: a ribbon in clip space above the hero, the waveform along it
  const scopeMaterial = additive(new T.MeshBasicNodeMaterial());
  scopeMaterial.depthTest = false;
  {
    const w = S.attribute('aW', 'vec2');
    const x = w.x;
    const wv = (v) => wave(S.int(S.clamp(v, 0, 1).mul(255).add(0.5)));
    const amp = u.loud.add(0.45).mul(0.05);
    const half = S.min(float(0.42), float(0.42).mul(h.res.y).div(h.res.x).mul(1.6));
    const base = vec2(
      S.clamp(h.scope.x, -0.45, 0.45), S.clamp(h.scope.y.add(0.32), 0.08, 0.52),
    );
    const p = base.add(vec2(x.sub(0.5).mul(2).mul(half), wv(x).mul(amp)));
    const p2 = base.add(vec2(x.add(0.004).sub(0.5).mul(2).mul(half), wv(x.add(0.004)).mul(amp)));
    const dir = normalize(p2.sub(p).mul(h.res).add(1e-5));
    const out = p.add(vec2(dir.y.negate(), dir.x).mul(w.y).mul(5).mul(h.px).div(h.res).mul(2));
    scopeMaterial.vertexNode = vec4(out, 0, 1);
    const level = u.loud.mul(0.8).add(0.45);
    const edge = smoothstep(0, 0.14, x).mul(smoothstep(1, 0.86, x));
    const c = bandC(S.clamp(wv(x).mul(0.5).add(0.5), 0, 1)).mul(h.sw).mul(edge).mul(level)
      .mul(h.scope.z);
    const vC = S.varying(c);
    const vX = S.varying(w.y);
    const ax = vX.mul(vX);
    const m = exp(ax.mul(-30)).mul(1.5).add(exp(ax.mul(-4)).mul(0.18));
    scopeMaterial.colorNode = vec4(lib.finish(vC.mul(m), u), 1);
  }
  const scope = new T.Mesh(
    geometry(data.aW, 'aW', 2, data.wIndex, data.aW.length / 2), scopeMaterial,
  );
  scope.frustumCulled = false;
  scope.renderOrder = 4;
  return { ring, scope };
}

export const heroLayer = (ctx) => {
  const { T, S, lib, sprites, caps, u } = ctx;
  const { float, vec2, vec3, vec4, sin, cos, mix, clamp: sclamp, select, length, normalize } = S;
  const object = new T.Group();
  const h = heroUniforms(T, S);
  const counts = heroCounts({ q: 1, caps });
  const NEQ = counts.eq.max;
  const NSC = counts.sculpt.max;

  // ---- shared pieces of the shading
  const rot2 = (a, x, y) => {
    const c = cos(a);
    const s = sin(a);
    return vec2(c.mul(x).add(s.mul(y)), s.negate().mul(x).add(c.mul(y)));
  };
  /** The body deform: the kick squashes and pumps, a twist about the vertical, a bend. */
  const deform = (p) => {
    const q = vec3(
      p.x.mul(h.kk.mul(0.34).add(1)), p.y.mul(float(1).sub(h.kk.mul(0.3))),
      p.z.mul(h.kk.mul(0.34).add(1)),
    );
    const a = rot2(q.y.mul(h.tw), q.x, q.z);
    const b = rot2(h.bd.mul(q.y).mul(0.75), a.x, q.y);
    return vec3(b.x, b.y, a.y);
  };
  const nz3 = (p) => vec3(
    lib.vn(p), lib.vn(p.add(vec3(31.7, 11.3, 0))), lib.vn(p.add(vec3(5.1, 47.3, 19.9))),
  ).sub(0.5);
  const spec = (x) => ctx.audio.spectrum(S.int(sclamp(x, 0, 1).mul(62.7).add(0.5)));
  const bandC = (x) => {
    const c = sclamp(x, 0, 1);
    const c0 = u.colA.mul(1.5);
    const c2 = u.colB.mul(1.5);
    const c1 = mix(u.colA, u.colB, 0.5).mul(1.5);
    return select(c.lessThan(0.5), mix(c0, c1, c.mul(2)), mix(c1, c2, c.mul(2).sub(1)));
  };
  const palette = (x) => {
    const t = S.fract(x).mul(4);
    const f = S.fract(t);
    const e = f.mul(f).mul(float(3).sub(f.mul(2)));
    const a = u.colA;
    const m = mix(u.colA, u.colB, 0.5);
    const b = u.colB;
    return select(t.lessThan(1), mix(a, m, e), select(t.lessThan(2), mix(m, b, e),
      select(t.lessThan(3), mix(b, m, e), mix(m, a, e))));
  };
  const fog = (z) => S.exp(z.mul(h.fogD).mul(-0.4));

  // ---- the sculpture's ten forms: each `(P, t, K) => vec4(place, colour parameter)`
  const sph = (a, b) => {
    const th = a.mul(TAU);
    const c = float(1).sub(b.mul(2));
    const s = S.sqrt(S.max(float(0), float(1).sub(c.mul(c))));
    return vec3(s.mul(cos(th)), c, s.mul(sin(th)));
  };
  const kc = (a, p, q) => {
    const r = cos(q.mul(a)).add(2);
    return vec3(
      r.mul(cos(p.mul(a))), sin(q.mul(a)).negate().mul(1.1), r.mul(sin(p.mul(a))),
    ).mul(0.5);
  };
  const knot = (P, t, K) => {
    const a = S.fract(P.x.add(t.mul(0.035).mul(P.z.add(0.4)))).mul(TAU);
    const c = kc(a, K.x, K.y);
    const T0 = normalize(kc(a.add(0.01), K.x, K.y).sub(c));
    const n1 = normalize(S.cross(T0, vec3(0.31, 0.83, 0.47)));
    const n2 = S.cross(T0, n1);
    const ang = P.y.mul(TAU).add(t.mul(0.3));
    const ct = P.x.add(sin(ang).mul(0.08));
    const p = c.add(n1.mul(cos(ang)).add(n2.mul(sin(ang))).mul(0.26).mul(P.z.mul(0.2).add(0.8)));
    return vec4(p, ct);
  };
  const gy = (p) => S.dot(sin(p), cos(p.yzx));
  const gyroid = (P, t, K) => {
    const k = K.x;
    const o = vec3(t.mul(0.12), t.mul(0.07), t.mul(-0.05));
    let p = sph(P.x, P.y).mul(1.6).mul(S.pow(P.z, 0.33)).mul(k).add(o);
    for (let n = 0; n < 4; n++) {
      const g = cos(p).mul(cos(p.yzx)).sub(sin(p.zxy).mul(sin(p)));
      p = p.sub(g.mul(gy(p)).div(S.max(S.dot(g, g), 0.1)));
    }
    const cell = S.floor(p.add(1.5708).div(3.1416));
    p = p.sub(o).div(k);
    const L = length(p);
    p = p.mul(S.min(float(1), float(1.65).div(L)));
    const ct = L.mul(0.28).add(P.w.mul(0.1)).add(S.mod(cell.x.add(cell.y).add(cell.z), 2).mul(0.5));
    return vec4(p, ct);
  };
  const galaxy = (P, t, K) => {
    const core = vec4(
      sph(P.x, P.y).mul(0.42).mul(S.pow(P.z, 0.6)).mul(vec3(1, 0.55, 1)), P.z.mul(0.1).add(0.04),
    );
    const arms = K.x;
    const arm = S.floor(S.fract(P.w.mul(7.31)).mul(arms));
    const r = S.pow(P.x, 0.8).mul(1.75).add(0.12);
    const a = arm.div(arms).mul(TAU).add(S.log(r.add(0.3)).mul(K.y).mul(1.6))
      .add(P.y.sub(0.5).mul(1.1).div(r.add(0.5))).sub(t.mul(0.3).div(r.add(0.4)));
    const ct = r.mul(0.35).add(arm.mul(0.13)).add(P.z.mul(0.05));
    const spiral = vec4(
      cos(a).mul(r), P.z.sub(0.5).mul(0.2).mul(S.exp(r.mul(-1.2))), sin(a).mul(r), ct,
    );
    return select(P.w.greaterThan(0.88), core, spiral);
  };
  const lotus = (P, t, K) => {
    const L = S.floor(P.w.mul(4));
    const th = P.x.mul(TAU).add(L.mul(0.37)).add(t.mul(0.05));
    const pr = S.pow(S.abs(cos(K.x.mul(th).mul(0.5))), L.mul(0.25).add(0.6));
    const v = S.sqrt(P.y);
    const op = sclamp(h.open.sub(L.mul(0.12)), 0, 1);
    const R = float(1.75).sub(L.mul(0.32)).mul(pr).mul(v).mul(op.mul(0.55).add(0.45));
    const ct = L.mul(0.22).add(v.mul(0.35));
    const y = v.mul(v).mul(float(1).sub(op)).mul(float(1.5).sub(L.mul(0.2))).add(L.mul(0.18))
      .sub(0.55).add(P.z.sub(0.5).mul(0.025)).add(pr.mul(v).mul(0.2).mul(op));
    return vec4(cos(th).mul(R), y, sin(th).mul(R), ct);
  };
  const lj = (a, d, t, K) => vec3(
    sin(K.x.mul(a).add(d).add(t.mul(0.1))), sin(K.y.mul(a).add(d.mul(0.5))),
    sin(K.z.mul(a).add(d.mul(1.3)).add(1.57)),
  ).mul(1.5);
  const lissajous = (P, t, K) => {
    const k = S.floor(P.w.mul(14));
    const a = S.fract(P.x.add(t.mul(0.025))).mul(TAU);
    const d = k.mul(0.17);
    const c = lj(a, d, t, K);
    const T0 = normalize(lj(a.add(0.01), d, t, K).sub(c));
    const n = normalize(S.cross(T0, vec3(0.2, 1, 0.1)));
    return vec4(c.add(n.mul(P.y.sub(0.5)).mul(0.14)), k.div(14).mul(0.6).add(P.x.mul(0.4)));
  };
  const sf = (a, K, n1) => S.pow(
    S.pow(S.abs(cos(K.x.mul(a).mul(0.25))), K.z).add(S.pow(S.abs(sin(K.x.mul(a).mul(0.25))), K.w)),
    float(-1).div(n1),
  );
  const superformula = (P, t, K) => {
    const th = P.x.mul(TAU).sub(Math.PI);
    const ph = P.y.mul(Math.PI).sub(Math.PI * 0.5);
    const n1 = K.y.mul(sin(t.mul(0.4)).mul(0.35).add(1));
    const r1 = sf(th, K, n1);
    const r2 = sf(ph, K, n1);
    const p = vec3(
      r1.mul(cos(th)).mul(r2).mul(cos(ph)), r2.mul(sin(ph)), r1.mul(sin(th)).mul(r2).mul(cos(ph)),
    ).mul(1.6);
    const L = length(p);
    const fit = S.min(float(1), float(1.9).div(S.max(L, 0.01)));
    return vec4(p.mul(fit), L.mul(0.35).add(P.y.mul(0.4)));
  };
  const helix = (P, t, K) => {
    const st = S.step(0.4, P.w);
    const y1 = P.x.sub(0.5).mul(4.4);
    const a1 = y1.mul(K.x).add(t.mul(0.5)).add(st.mul(Math.PI));
    const strand = vec4(
      vec3(cos(a1).mul(0.75), y1, sin(a1).mul(0.75)).add(sph(P.y, P.z).mul(0.07)),
      st.mul(0.5).add(P.x.mul(0.25)),
    );
    const q = S.floor(P.x.mul(30));
    const y2 = q.add(0.5).div(30).sub(0.5).mul(4.4);
    const a2 = y2.mul(K.x).add(t.mul(0.5));
    const e = vec3(cos(a2).mul(0.75), y2, sin(a2).mul(0.75));
    const rung = vec4(
      mix(e, vec3(e.x.negate(), y2, e.z.negate()), P.y).add(P.z.sub(0.5).mul(0.03)),
      lib.h11(q.mul(7.31)).mul(0.5).add(0.25),
    );
    return select(P.w.lessThan(0.8), strand, rung);
  };
  const moebius = (P, t, K) => {
    const a = P.x.mul(TAU);
    const w = P.y.sub(0.5);
    const hh = a.mul(K.x).mul(0.5).add(t.mul(0.25));
    const c = cos(hh);
    const R = w.mul(c).add(1.35);
    return vec4(
      R.mul(cos(a)), w.mul(sin(hh)).add(P.z.sub(0.5).mul(0.03)), R.mul(sin(a)),
      P.x.add(P.y.mul(0.25)),
    );
  };
  const nova = (P, t) => {
    const dr = sph(S.floor(P.x.mul(24)).add(0.5).div(24), S.floor(P.y.mul(12)).add(0.5).div(12));
    const r = S.fract(P.z.add(t.mul(0.15))).mul(2).add(0.3);
    const rays = vec4(dr.mul(r), r.mul(0.3));
    const d = sph(P.x, P.y);
    const k = S.floor(P.w.sub(0.3).div(0.15));
    const ph = S.fract(t.mul(0.1).add(k.div(3)));
    const shell = vec4(
      d.mul(ph.mul(1.9).add(0.35).add(P.z.sub(0.5).mul(0.04))),
      k.mul(0.2).add(0.2).add(ph.mul(0.3)),
    );
    const heart = vec4(d.mul(0.38).mul(S.pow(P.z, 0.5)), P.z.mul(0.08).add(0.02));
    return select(P.w.lessThan(0.3), rays, select(P.w.lessThan(0.75), shell, heart));
  };
  const fc = (g, t) => vec3(
    sin(t.mul(1.1).add(g.mul(1.3))).mul(1.3), sin(t.mul(0.8).add(g.mul(2.1))).mul(0.75),
    cos(t.mul(0.9).add(g.mul(0.7))).mul(1.3),
  );
  const swarmForm = (P, t) => {
    const g = S.floor(P.w.mul(7));
    const tt = t.mul(0.45).add(g.mul(1.7));
    const c = fc(g, tt);
    const hd = normalize(c.sub(fc(g, tt.sub(0.15))).add(1e-4));
    let o = nz3(P.xyz.mul(4).add(vec3(0, t.mul(0.25), g))).mul(0.9)
      .add(sph(P.x, P.y).mul(0.18).mul(P.z));
    o = o.add(hd.mul(S.dot(o, hd)).mul(1.8));
    return vec4(c.add(o.mul(0.55)), g.div(7).add(P.z.mul(0.1)));
  };
  const forms = [knot, gyroid, galaxy, lotus, lissajous, superformula, helix, moebius, nova,
    swarmForm];
  /** The form numbered `k` (a node): an `If` chain over the ten. */
  const shape = (k, P, t, K) => {
    const out = vec4(0).toVar();
    let chain = S.If(k.lessThan(0.5), () => {
      out.assign(forms[0](P, t, K));
    });
    for (let n = 1; n < 9; n++) {
      chain = chain.ElseIf(k.lessThan(n + 0.5), () => {
        out.assign(forms[n](P, t, K));
      });
    }
    chain.Else(() => {
      out.assign(forms[9](P, t, K));
    });
    return out;
  };
  /** The sculpture's place and colour parameter for particle key `P`: form A, and while a morph
   * runs form B with a turbulence in between. */
  const sculpt = (P, t) => {
    const a = shape(h.shA, P, t, h.kA);
    const res = vec4(a).toVar();
    S.If(h.mt.greaterThan(0), () => {
      const b = shape(h.shB, P, t, h.kB);
      const m0 = sclamp(h.mt.mul(1.5).sub(P.z.mul(0.5)), 0, 1);
      const m = m0.mul(m0).mul(float(3).sub(m0.mul(2)));
      const w = sin(m.mul(Math.PI));
      let p = mix(a.xyz, b.xyz, m);
      p = p.add(nz3(p.mul(1.2).add(vec3(t.mul(0.3), P.w.mul(4), 0))).mul(w).mul(h.turb).mul(2.2));
      const xz = rot2(w.mul(P.w.sub(0.5)).mul(3), p.x, p.z);
      res.assign(vec4(xz.x, p.y, xz.y, mix(a.w, b.w, m)));
    });
    const p = res.xyz.add(
      nz3(res.xyz.mul(1.8).add(vec3(0, 0, t.mul(0.4)))).mul(u.mid.mul(0.2).add(0.04)),
    );
    const swell = u.bass.mul(0.07).add(1).add(h.peak.mul(0.22).mul(P.z.add(0.3)));
    return vec4(p.mul(swell), res.w);
  };

  // ---- the EQ forms' place: five shapes morphing, the spectrum pushing, the deform
  // a pass names at most four buffers: on WebGL2 every storage buffer a pass touches is a
  // transform feedback output, and four is the most a context gives
  const eqBufs = {
    q: lib.cpuBuffer(NEQ, 'vec4'),
    pos: lib.computeBuffer(NEQ, 'vec4'),
    off: lib.computeBuffer(NEQ, 'vec4'),
    vel: lib.computeBuffer(NEQ, 'vec4'),
  };
  const scBufs = {
    keys: lib.cpuBuffer(NSC, 'vec4'),
    pos: lib.computeBuffer(NSC, 'vec4'),
    off: lib.computeBuffer(NSC, 'vec4'),
    vel: lib.computeBuffer(NSC, 'vec4'),
  };
  const eqTarget = (i) => {
    const q = eqBufs.q.element(i);
    const shapeOf = (k) => {
      const [u0, rn, par, ang] = [q.x, q.y, q.z, q.w];
      const y = u0.mul(2).sub(1);
      const rr = S.sqrt(S.max(float(0), float(1).sub(y.mul(y))));
      const sphere = vec3(cos(ang).mul(rr).mul(1.25), y.mul(1.25), sin(ang).mul(rr).mul(1.25));
      const tb = S.fract(u0.mul(233)).mul(TAU);
      const r0 = cos(tb).mul(0.42).add(1.1);
      const torus = vec3(r0.mul(cos(u0.mul(TAU))), sin(tb).mul(0.42), r0.mul(sin(u0.mul(TAU))));
      const pp = S.fract(u0.mul(160)).mul(4);
      const side = S.floor(pp);
      const f = S.fract(pp).mul(2).sub(1);
      const pick4 = (a, b2, c, d) => select(side.lessThan(0.5), a, select(side.lessThan(1.5), b2,
        select(side.lessThan(2.5), c, d)));
      const cube = vec3(
        pick4(f, float(1), f.negate(), float(-1)).mul(0.95), y.mul(0.95),
        pick4(float(-1), f, float(1), f.negate()).mul(0.95),
      );
      const ha = u0.mul(TAU * 4).add(par.mul(Math.PI));
      const hr = q.y.sub(0.5).mul(0.1).add(0.6);
      const helixPos = vec3(cos(ha).mul(hr), y.mul(1.7), sin(ha).mul(hr));
      const sr = S.sqrt(u0).mul(1.8);
      const sa = u0.mul(TAU * 7).add(S.floor(rn.mul(3)).mul(TAU / 3)).add(sr.mul(0.6));
      const spiral = vec3(
        cos(sa).mul(sr), rn.sub(0.5).mul(0.1).mul(float(1.9).sub(sr)), sin(sa).mul(sr),
      );
      return select(k.lessThan(0.5), sphere, select(k.lessThan(1.5), torus,
        select(k.lessThan(2.5), cube, select(k.lessThan(3.5), helixPos, spiral))));
    };
    const pick = shapeOf;
    const t0 = sclamp(h.pqT.mul(1.7).sub(q.y.mul(0.7)), 0, 1);
    const t = t0.mul(t0).mul(float(3).sub(t0.mul(2)));
    let p = mix(pick(h.pqA), pick(h.pqB), t);
    const w = sin(t.mul(Math.PI));
    const time = u.time;
    const flow = (a, b, c, s) => vec3(
      lib.vn(p.mul(s).add(a)), lib.vn(p.mul(s).add(b)), lib.vn(p.mul(s).add(c)),
    ).sub(0.5);
    p = p.add(flow(time.mul(0.4), 7.1, float(13.7).sub(time.mul(0.3)), 2.5)
      .mul(w.mul(1.1).add(0.05).add(u.mid.mul(0.15)).add(u.flat.mul(u.high).mul(0.25))));
    const s = spec(q.x);
    const L = length(p);
    p = p.add(p.div(S.max(L, 0.05)).mul(s.mul(0.6).add(h.peak.mul(0.2))));
    p = p.mul(h.sn.mul(0.45).mul(S.step(0.72, lib.h11(q.y.mul(31).add(h.hsd)))).add(1)
      .add(sin(time.mul(41).add(q.y.mul(90))).mul(h.ht).mul(0.03)).sub(h.kk.mul(0.06)));
    p = p.add(flow(time.mul(0.6), 5.3, float(9.9).sub(time.mul(0.5)), 1.7)
      .mul(u.mid.mul(0.5).add(0.25).add(u.flat.mul(u.high).mul(0.6))))
      .add(p.div(S.max(L, 0.05)).mul(s).mul(0.45));
    p = p.mul(h.sn.mul(0.6).mul(S.step(0.8, lib.h11(q.y.mul(17).add(h.hsd.mul(1.3))))).add(1));
    return vec4(deform(p), q.x);
  };
  const scTarget = (i) => {
    const P = scBufs.keys.element(i);
    const r = sculpt(P, u.time);
    // the width spreads the form along the camera's right
    const dx = S.dot(r.xyz, h.camR);
    const widened = r.xyz.add(h.camR.mul(dx).mul(u.width.mul(0.25).sub(0.12)));
    const flowed = widened.add(nz3(widened.mul(2.3).add(vec3(u.time.mul(0.5), 0, P.w.mul(3))))
      .mul(u.mid.mul(0.35).add(0.12).add(h.kk.mul(0.3))));
    return vec4(deform(flowed), r.w);
  };

  /** One compute pass: the target of particle `i`, a damped spring on the offset, the burst's
   * impulse, written to `bufs.pos` (the hero's local place and the colour parameter). */
  const spring = (target, bufs, count, which) => S.Fn(() => {
    const i = S.instanceIndex;
    S.If(i.toFloat().lessThan(h.active[which]), () => {
      const tgt = target(i);
      const off = bufs.off.element(i);
      const vel = bufs.vel.element(i);
      const id = i.toFloat();
      const dt = h.dt;
      const accel = off.xyz.mul(-HERO.omega * HERO.omega).sub(vel.xyz.mul(2 * HERO.omega));
      const rd = normalize(
        vec3(lib.h11(id.mul(13.1)), lib.h11(id.mul(7.7)), lib.h11(id.mul(3.3))).sub(0.5)
          .add(tgt.xyz.mul(0.25)).add(1e-3),
      );
      const speed = h.burst.mul(HERO.burstSpeed).mul(lib.h11(id.mul(5.3)).mul(1.8).add(1));
      const swirl = vec3(tgt.z.negate(), 0, tgt.x).mul(h.burst).mul(HERO.burstSwirl)
        .mul(lib.h11(id.mul(2.9)).sub(0.5));
      const v1 = vel.xyz.add(accel.mul(dt)).add(rd.mul(speed)).add(swirl).toVar();
      const o1 = off.xyz.add(v1.mul(dt)).toVar();
      S.If(h.snap.greaterThan(0.5), () => {
        off.assign(vec4(0));
        vel.assign(vec4(0));
      }).Else(() => {
        off.assign(vec4(o1, 0));
        vel.assign(vec4(v1, 0));
      });
      const o = S.select(h.snap.greaterThan(0.5), vec3(0), o1);
      bufs.pos.element(i).assign(vec4(tgt.xyz.add(o), tgt.w));
    });
  })().compute(count);
  const eqPass = spring(eqTarget, eqBufs, NEQ, 0);
  const scPass = spring(scTarget, scBufs, NSC, 1);

  // ---- the drawing of the particles: one sprite per particle, in pixels
  /** The world place of a particle's local place: the hero's rotation, size and position (and the
   * heat haze of a changed path, for the sculpture). */
  const worldOf = (buffer, hazed = false) => ({
    element: (i) => {
      const at = h.hero.add(h.rot.mul(buffer.element(i).xyz.mul(h.hs)));
      return vec4(hazed ? lib.haze(at, 0.08, u.haze, u.time) : at, 0);
    },
  });
  const viewZ = (place, i) => S.varying(
    S.cameraViewMatrix.mul(vec4(place.element(i).xyz, 1)).z.negate(),
  );

  const glyph = (p, g) => {
    const a = S.abs(p);
    const plus = S.min(S.max(a.x.sub(0.12), a.y.sub(0.75)), S.max(a.y.sub(0.12), a.x.sub(0.75)));
    const ring = S.abs(length(p).sub(0.55)).sub(0.12);
    const diamond = S.abs(a.x.add(a.y).sub(0.62)).sub(0.12);
    const dash = S.max(a.x.sub(0.8), a.y.sub(0.12));
    const b = S.abs(vec2(p.x.add(p.y), p.x.sub(p.y))).mul(0.7071);
    const cross = S.min(S.max(b.x.sub(0.12), b.y.sub(0.75)), S.max(b.y.sub(0.12), b.x.sub(0.75)));
    return select(g.lessThan(0.5), plus, select(g.lessThan(1.5), ring,
      select(g.lessThan(2.5), diamond, select(g.lessThan(3.5), dash, cross))));
  };

  const eqPlace = worldOf(eqBufs.pos);
  const eqSprite = (() => {
    const i = S.instanceIndex;
    const q = eqBufs.q.element(i);
    const z = viewZ(eqPlace, i);
    const s = S.varying(spec(q.x));
    const isGlyph = S.varying(select(
      h.gly.greaterThan(0.5).and(S.fract(q.y.mul(7)).lessThan(0.35)),
      S.floor(S.fract(q.y.mul(13)).mul(5)).add(1), float(0),
    ));
    const speck = S.step(0.93, lib.h11(q.y.mul(977).add(S.floor(u.time.mul(14)))));
    const sk = u.flat.mul(2).mul(speck).mul(u.high).add(1);
    const colour = S.varying(
      bandC(q.x).mul(h.w[2]).mul(s.mul(1.3).add(0.25))
        .mul(select(isGlyph.greaterThan(0), float(0.4), float(1))).mul(sk).mul(fog(z)),
    );
    const size = {
      element: () => sclamp(
        select(isGlyph.greaterThan(0), float(0.07), float(0.024)).mul(h.hs)
          .mul(s.mul(0.8).add(1)).mul(h.foc).div(S.max(z, 0.5)),
        1, select(isGlyph.greaterThan(0), float(26), float(9)),
      ),
    };
    const p = S.uv().sub(0.5).mul(2);
    const disc = S.max(float(1).sub(S.dot(p, p)), 0);
    const m = select(isGlyph.greaterThan(0.5),
      float(1).sub(S.smoothstep(0, 0.2, glyph(p.mul(1.1), isGlyph.sub(1)))), disc.mul(disc));
    return sprites.make({
      count: 1, position: eqPlace, size, mode: 'px', colour: lib.finish(colour, u), opacity: m,
    });
  })();

  const scPlace = worldOf(scBufs.pos, true);
  const scSprite = (() => {
    const i = S.instanceIndex;
    const P = scBufs.keys.element(i);
    const z = viewZ(scPlace, i);
    const view = { maxC: h.px.mul(5), aper: float(0.4), focusD: h.focusD, px: h.px };
    const base = lib.psz(float(5), z, h.px, u.grain);
    const c = lib.coc(z, view);
    const total = S.varying(base.add(c));
    const blur = S.varying(sclamp(c.div(total).mul(1.4), 0, 1));
    const alpha = S.varying(S.pow(base.div(total), 1.1).mul(S.smoothstep(0.25, 1.2, z)));
    const col = S.varying(
      palette(scBufs.pos.element(i).w.add(P.w.mul(0.08)).add(u.time.mul(0.01)))
        .mul(u.mid.mul(0.3).add(0.14).add(h.peak.mul(0.4))).mul(h.w[3]).mul(h.bright),
    );
    const d = S.uv().sub(0.5);
    const r = S.dot(d, d).mul(4);
    const soft = S.max(float(1).sub(r), 0).mul(S.exp(r.mul(-6)).mul(0.75).add(0.25));
    const ring = S.smoothstep(1, 0.82, r).mul(S.smoothstep(0.35, 0.95, r).mul(0.55).add(0.45));
    const m = mix(soft, ring, blur);
    return sprites.make({
      count: 1, position: scPlace, size: { element: () => total }, mode: 'px',
      colour: lib.finish(col, u), opacity: alpha.mul(m),
    });
  })();

  // ---- the wireframes: the lattice planet, the noise icosphere, the halo
  const rimOf = (n, mv, fr2) => {
    const view = S.modelViewMatrix.mul(vec4(n, 0)).xyz;
    const f = float(1).sub(S.abs(S.dot(normalize(view), normalize(mv.xyz.negate()))));
    return mix(float(1), S.pow(f, h.lev.mul(2).add(1.4)).mul(1.7).add(0.07), fr2);
  };
  /** The wire vertex's deformed place and colour: `fr2` is 1 for the lattice, 0 for the
   * icosphere, `weight` its drawing weight. */
  const wire = (fr2, weight) => {
    const n = S.positionGeometry;
    const la = n.y.mul(0.5).add(0.5);
    const bd = mix(mix(u.bass, u.mid, S.smoothstep(0.28, 0.45, la)), u.high,
      S.smoothstep(0.55, 0.72, la));
    const time = u.time;
    const nzv = lib.vn(n.mul(float(2.4).sub(fr2.mul(1.1)))
      .add(vec3(time.mul(0.5), time.mul(0.31), time.mul(-0.2)))).mul(2).sub(1);
    const sp = spec(la.mul(0.9).add(0.05));
    const fo = lib.vn(n.mul(4.1).add(vec3(3, h.hsd.mul(7.31), h.hsd.mul(3.7)))).mul(2).sub(1);
    const shimmer = sin(time.mul(47).add(S.dot(n, vec3(91, 57, 33))));
    let r = nzv.mul(bd.mul(0.45).add(0.08)).mul(float(1).sub(fr2.mul(0.35)))
      .add(sp.mul(0.3).mul(float(1).sub(fr2.mul(0.5)))).add(1).add(h.peak.mul(0.25));
    r = r.add(S.pow(S.max(fo, 0), 1.5).mul(h.sn).mul(0.8)).sub(h.kk.mul(0.07))
      .add(shimmer.mul(h.ht).mul(0.035))
      .add(lib.vn(n.mul(11).add(time.mul(0.7))).sub(0.5).mul(u.flat).mul(0.16));
    r = r.add(nzv.mul(bd.mul(0.36).add(0.07)).mul(float(1).sub(fr2.mul(0.3))))
      .add(S.pow(S.max(fo, 0), 1.3).mul(h.sn).mul(0.85)).add(S.pow(sp, 2).mul(0.4))
      .add(shimmer.mul(h.ht).mul(0.03));
    const sc = S.floor(n.mul(2.4).add(0.5));
    const sh = lib.h13(sc.add(7.3));
    const place = deform(n.mul(r)).add(normalize(sc.add(vec3(0.013, 0.021, 0.017))).mul(h.ex)
      .mul(sh.mul(1.2).add(0.7))).add(vec3(0, h.ex.mul(h.ex).mul(sh.sub(0.3)).mul(0.8), 0));
    const mv = S.modelViewMatrix.mul(vec4(place, 1));
    const rim = rimOf(n, mv, fr2);
    const colour = mix(bandC(la), bandC(u.cent), 0.3).mul(weight)
      .mul(bd.mul(1.2).add(0.35).add(sp.mul(0.6))).mul(rim).mul(fog(mv.z.negate()))
      .mul(fr2.mul(0.8).add(1)).mul(h.sn.mul(0.3).add(h.ht.mul(0.2)).add(1))
      .mul(h.sn.mul(0.3).add(h.ex.mul(0.5)).add(1));
    return { place, colour };
  };
  const wireMaterial = (fr2, weight) => {
    const m = new T.LineBasicNodeMaterial();
    const w = wire(float(fr2), weight);
    m.positionNode = w.place;
    m.colorNode = lib.finish(S.varying(w.colour), u);
    m.transparent = true;
    m.depthWrite = false;
    m.blending = T.AdditiveBlending;
    return m;
  };
  const latticeMat = wireMaterial(1, h.w[0]);
  const icoMat = wireMaterial(0, h.w[1]);
  const lattice = new T.LineSegments(
    new T.WireframeGeometry(new T.IcosahedronGeometry(1, 4)), latticeMat,
  );
  const ico = new T.LineSegments(
    new T.WireframeGeometry(new T.IcosahedronGeometry(1, 16)), icoMat,
  );
  const haloMat = new T.MeshBasicNodeMaterial();
  {
    const n = S.positionGeometry;
    const scale = h.lev.mul(0.22).add(1.06).add(u.bass.mul(0.1));
    const place = deform(n.mul(scale)).mul(h.ex.mul(0.25).add(1));
    const mv = S.modelViewMatrix.mul(vec4(place, 1));
    const face = S.abs(S.dot(normalize(S.modelViewMatrix.mul(vec4(S.normalGeometry, 0)).xyz),
      normalize(mv.xyz.negate())));
    const glow = S.pow(float(1).sub(face), 2.2).mul(0.7)
      .add(S.pow(face, 3).mul(h.lev.mul(0.14).add(0.05)));
    const base = bandC(u.cent).mul(h.w[0].mul(0.8).add(h.w[1].mul(0.3))).mul(fog(mv.z.negate()))
      .mul(float(1).sub(h.ex.mul(0.75)));
    haloMat.positionNode = place;
    haloMat.colorNode = lib.finish(S.varying(base.mul(glow)), u);
    haloMat.transparent = true;
    haloMat.depthWrite = false;
    haloMat.blending = T.AdditiveBlending;
  }
  const halo = new T.Mesh(new T.SphereGeometry(1, 48, 32), haloMat);
  for (const o of [lattice, ico, halo]) {
    o.frustumCulled = false;
    object.add(o);
  }
  for (const s of [eqSprite, scSprite]) object.add(s);
  const instrument = heroInstrument({
    T, S, lib, u, h, bandC, spec, fog, wave: ctx.audio.wave,
  });
  object.add(instrument.ring, instrument.scope);
  // the hero blends over the river (drawn at order 1, writing no depth): it comes after it, or the
  // water paints over its lower half; the scope's trace is on top of all
  for (const o of object.children) o.renderOrder = 2;
  instrument.scope.renderOrder = 4;

  const euler = new T.Euler();
  const matrix = new T.Matrix4();
  const right = new T.Vector3();
  const up = new T.Vector3();
  const forward = new T.Vector3();
  const sizeOf = new T.Vector2();
  // the scope's inputs, reused each frame
  const offset = [0, 0, 0];
  const basis = { right: [0, 0, 0], up: [0, 0, 0], forward: [0, 0, 0], fov: 55, aspect: 1 };
  const screen = { x: 0, y: 0, on: 0 };
  let state = null;
  let seeded = null;
  let counts0 = null;
  let first = true;
  let lastTime = null;
  const info = {
    form: 0, weights: [1, 0, 0, 0], morphs: 0, bursts: 0, counts: { eq: 0, sculpt: 0 },
  };

  return {
    name: 'hero',
    group: 'base',
    object,
    /** What a test reads: the live state and the buffers. */
    probe: () => ({
      h, eqBufs, scBufs, eqPass, scPass, NEQ, NSC, get state() { return state; },
    }),
    build(seed) {
      seeded = seed;
      state = createHeroState(seed);
      const eq = eqTables(NEQ, seed);
      lib.upload(eqBufs.q, eq.q);
      lib.upload(scBufs.keys, sculptKeys(NSC, seed));
      first = true;
      lastTime = null;
      counts0 = null;
    },
    update(frame) {
      const { cam, camera } = frame;
      const uu = frame.u;
      const dt = Math.max(0, frame.dt || 0);
      const time = Number.isFinite(uu.time) ? uu.time : 0;
      const sdt = lastTime === null ? 0 : clamp(time - lastTime, 0, 0.25);
      lastTime = time;
      // a reset is the first frame after a build or the probe's frame (dt exactly 0); a long step
      // (Still draws a second apart) is clamped, never a reset
      const snap = first || frame.dt === 0;
      first = false;
      camera.matrixWorld.extractBasis(right, up, forward);
      forward.negate();
      const live = state.step({
        dt: Math.min(dt, HERO.stepMax * 5), sdt, u: uu,
        cam: {
          x: cam.x, y: cam.y, z: cam.z, fov: cam.fov, aspect: cam.aspect,
          right: [right.x, right.y, right.z], up: [up.x, up.y, up.z],
          forward: [forward.x, forward.y, forward.z],
        },
        terrain: ctx.terrain ?? undefined,
        director: frame.director ?? null,
        events: frame.events ?? null,
        flash: frame.flash ?? null,
        snap,
      });
      const plan = state.plan.state;
      const q = frame.budget?.hero ?? 1;
      const n = heroCounts({ q, caps: frame.caps ?? caps });
      const w = plan.w;

      // the uniforms
      for (let k = 0; k < 4; k++) h.w[k].value = w[k];
      h.hs.value = live.hs;
      h.lev.value = clamp((uu.loud ?? 0) * 0.8 + (uu.bass ?? 0) * 0.5);
      h.gly.value = plan.gly;
      h.pqA.value = plan.pqA;
      h.pqB.value = plan.pqB;
      h.pqT.value = plan.pqT;
      h.tw.value = live.tw;
      h.bd.value = live.bd;
      h.ex.value = live.ex;
      h.sn.value = live.sn;
      h.ht.value = live.ht;
      h.kk.value = live.kk;
      h.hsd.value = live.hsd;
      h.shA.value = plan.shA;
      h.shB.value = plan.shB;
      h.mt.value = plan.mt;
      h.kA.value.fromArray(plan.kA);
      h.kB.value.fromArray(plan.kB);
      h.camR.value.copy(right);
      h.hero.value.set(live.pos[0], live.pos[1], live.pos[2]);
      euler.set(live.rotX, live.rotY, 0);
      matrix.makeRotationFromEuler(euler);
      h.rot.value.setFromMatrix4(matrix);
      ctx.gpu?.renderer?.getDrawingBufferSize?.(sizeOf);
      // the size the sprites are drawn for: the frame's, else the canvas's
      const height = Number.isFinite(frame.viewH) && frame.viewH > 1 ? frame.viewH
        : sizeOf.y > 1 ? sizeOf.y : 800;
      h.px.value = Math.max(0.7, height / 800);
      h.foc.value = height / (2 * Math.tan((cam.fov * Math.PI) / 360));
      h.focusD.value = Math.hypot(live.pos[0] - cam.x, live.pos[1] - cam.y, live.pos[2] - cam.z);
      h.dt.value = Math.min(sdt, HERO.stepMax);
      h.peak.value = live.pulse;
      h.snap.value = snap ? 1 : 0;
      h.burst.value = live.burst;
      // the land's fog density when the land layer has set it (a number or a uniform), else ours
      const env = ctx.env?.fogD;
      const shared = typeof env === 'number' ? env : env?.value;
      h.fogD.value = Number.isFinite(shared) ? shared : HERO.fogD;
      const drawn = crossfadeCounts(n, w, (frame.caps ?? caps)?.maxParticles);
      h.active[0].value = drawn.eq;
      h.active[1].value = drawn.sculpt;

      // the objects
      for (const o of [lattice, ico, halo]) {
        o.position.set(live.pos[0], live.pos[1], live.pos[2]);
        o.scale.setScalar(live.hs);
        o.rotation.set(live.rotX, live.rotY, 0);
      }
      const { ring, scope } = instrument;
      ring.position.set(live.pos[0], live.pos[1], live.pos[2]);
      ring.quaternion.setFromRotationMatrix(camera.matrixWorld);
      ring.scale.setScalar(live.hs * 0.7);
      offset[0] = live.pos[0] - cam.x;
      offset[1] = live.pos[1] - cam.y;
      offset[2] = live.pos[2] - cam.z;
      right.toArray(basis.right);
      up.toArray(basis.up);
      forward.toArray(basis.forward);
      basis.fov = cam.fov;
      basis.aspect = cam.aspect;
      const view = heroScreen(offset, basis, screen);
      h.scope.value.set(view.x, view.y, view.on);
      h.rw.value = plan.rw;
      h.rcw.value = plan.rcw;
      h.sw.value = plan.sw;
      ring.visible = plan.rw + plan.rcw > 0.01;
      scope.visible = plan.sw > 0.01;
      h.res.value.set(height * cam.aspect, height);
      lattice.visible = w[0] > 0.01;
      halo.visible = w[0] + w[1] > 0.01;
      ico.visible = w[1] > 0.01;
      eqSprite.visible = w[2] > 0.01;
      scSprite.visible = w[3] > 0.01;
      eqSprite.count = drawn.eq;
      scSprite.count = drawn.sculpt;
      const r = ctx.gpu?.renderer;
      if (r && w[2] > 0.01) r.compute(eqPass);
      if (r && w[3] > 0.01) r.compute(scPass);
      counts0 = n;
      info.form = plan.form;
      info.weights = w.slice();
      info.morphs = plan.morphs;
      info.bursts = live.bursts;
      info.counts = { eq: drawn.eq, sculpt: drawn.sculpt };
      ctx.hero = {
        t: frame.t, sdt, at: { x: live.pos[0], y: live.pos[1], z: live.pos[2], size: live.hs },
        pos: live.pos, hs: live.hs, form: plan.form, swarm: plan.gly,
        kick: live.swarmKick, lift: live.lift, px: h.px.value, foc: h.foc.value,
      };
    },
    state: () => ({ ...info, seed: seeded, counts: counts0 ? info.counts : null }),
    dispose() {
      const { ring, scope } = instrument;
      for (const o of [lattice, ico, halo, eqSprite, scSprite, ring, scope]) o.material.dispose();
      for (const o of [lattice, ico, halo, ring, scope]) o.geometry.dispose();
    },
  };
};
