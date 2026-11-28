/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The scene's particles layer: the swarm that flies around the hero, and the stateful-particle
// helpers the hero shares. Its factory is handed the scene's context (`ctx`: the vendored
// namespaces `T` and `S`, the shared `lib` and `sprites`, the capability bag `caps`, the uniform
// block `u`, the audio buffers, the `scene` and `camera`, and the hero's placement `ctx.hero`, set
// by the hero layer in the same frame) and returns `{ name, group, object, build(seed),
// update(frame), dispose() }`.
//
// The swarm is a flock of motes, each with a position and a velocity that live in compute
// buffers and are stepped by a compute pass: a pull toward a shell around the hero, a swirl about
// its axis, a push on the kick and the music's colour. On the WebGPU tier a second pass first
// lets every mote look at a sample of the others (cohesion, separation, alignment) through
// `lib.neighbours` on the compute-written buffers; on WebGL2 that pass does not exist and the swarm
// flies without it, no error. Each mote also keeps a short history of places (its trail), drawn as
// dimmer, smaller sprites; the history moves by the clock, not by the frame.
//
//   particlesLayer(ctx)       the layer
//   particleCounts(opts)      `{ n, max }`: a layer's count under the budget and the cap
//   SWARM                     the swarm's numbers
//   flockSample(i, s, n, shift)   the index of the s-th mote that mote i looks at
//   flockAcc(i, state, p)     the CPU reference of the neighbour pass for mote i
//   trailDue(clock, dt, gap)  the trail clock: whether the histories move this step

import { hashStr, mulberry } from '../rng.js';

/** The swarm's numbers: counts (the WebGPU tier's are four times the floor's), the flock's sample
 * and radii, the weights of its three rules, the trail's length and gap in seconds. */
export const SWARM = Object.freeze({
  base: 6000,
  webgpuFactor: 4,
  minimum: 400,
  sample: 6,
  separation: 0.22,
  neighbourhood: 0.9,
  wSeparate: 2.4,
  wCohere: 0.9,
  wAlign: 0.7,
  shell: 1.5,
  spread: 0.9,
  trail: 3,
  trailGap: 0.07,
  speedMax: 3.2,
  stepMax: 1 / 20,
});

/**
 * How many of a layer's `base` particles to draw: the budget's factor `q` (0.78 steps, floor 0.3)
 * of the tier's count, never under `minimum` and never over `caps.maxParticles`. `max` is what
 * the buffers hold (the count at `q` of 1).
 */
export function particleCounts({ base, q = 1, caps = null, factor = 1, minimum = SWARM.minimum }) {
  const cap = Number.isFinite(caps?.maxParticles) ? caps.maxParticles : 40000;
  const max = Math.max(1, Math.min(cap, Math.floor(base * factor)));
  const n = Math.max(Math.min(minimum, max), Math.min(max, Math.floor(base * factor * q)));
  return { n, max };
}

/** The index of the `s`-th mote that mote `i` of `n` looks at on a step whose clock is `shift`:
 * never `i` itself. */
export function flockSample(i, s, n, shift) {
  const at = (i * 7919 + (s + 1) * 104729 + shift * 1009) % n;
  return at === i ? (at + 1) % n : at;
}

const len = (v) => Math.hypot(v[0], v[1], v[2]);

/**
 * The acceleration the neighbour pass gives mote `i`: the sum of separation (away from a mote
 * nearer than `separation`), cohesion (toward the mean place of those within `neighbourhood`) and
 * alignment (toward their mean velocity), over `sample` others. `state`: `{ pos, vel, n, shift }`,
 * `pos` and `vel` flat xyz arrays of stride 4.
 */
export function flockAcc(i, { pos, vel, n, shift }, p = SWARM) {
  const at = (a, k) => [a[k * 4], a[k * 4 + 1], a[k * 4 + 2]];
  const me = at(pos, i);
  const sep = [0, 0, 0];
  const mean = [0, 0, 0];
  const dir = [0, 0, 0];
  let seen = 0;
  for (let s = 0; s < p.sample; s++) {
    const j = flockSample(i, s, n, shift);
    const other = at(pos, j);
    const d = [other[0] - me[0], other[1] - me[1], other[2] - me[2]];
    const r = len(d);
    if (r < p.separation) {
      const k = (p.separation - r) / Math.max(r, 1e-4);
      for (let c = 0; c < 3; c++) sep[c] -= d[c] * k;
    }
    if (r < p.neighbourhood) {
      const v = at(vel, j);
      for (let c = 0; c < 3; c++) {
        mean[c] += other[c];
        dir[c] += v[c];
      }
      seen++;
    }
  }
  const out = [0, 0, 0];
  if (seen === 0) return sep.map((v) => v * p.wSeparate);
  const mine = at(vel, i);
  for (let c = 0; c < 3; c++) {
    out[c] = sep[c] * p.wSeparate
      + (mean[c] / seen - me[c]) * p.wCohere
      + (dir[c] / seen - mine[c]) * p.wAlign;
  }
  return out;
}

/** The trail clock: `clock.t` gains `dt`; when it passes `gap` the histories move this step (and
 * it keeps the remainder), so the trail's length in seconds does not depend on the frame rate.
 * Returns 1 or 0. */
export function trailDue(clock, dt, gap = SWARM.trailGap) {
  clock.t += dt;
  if (clock.t < gap) return 0;
  clock.t -= gap * Math.floor(clock.t / gap);
  return 1;
}

/** The swarm's start: places on a shell about the origin and a velocity along the swirl. */
export function swarmStart(n, seed) {
  const rng = mulberry(hashStr(`swarm:${seed}`));
  const pos = new Float32Array(n * 4);
  const vel = new Float32Array(n * 4);
  for (let i = 0; i < n; i++) {
    const a = rng() * Math.PI * 2;
    const y = rng() * 2 - 1;
    const r = Math.sqrt(1 - y * y);
    const shell = SWARM.shell + (rng() - 0.5) * SWARM.spread;
    const at = [Math.cos(a) * r * shell, y * shell, Math.sin(a) * r * shell];
    pos.set([...at, rng()], i * 4);
    vel.set([-at[2] * 0.6, 0, at[0] * 0.6, 0], i * 4);
  }
  return { pos, vel };
}

/** The factory of the layer. */
export const particlesLayer = (ctx) => {
  const { T, S, lib, sprites, caps, u } = ctx;
  const object = new T.Group();
  const flock = Boolean(caps?.neighbours);
  const sized = particleCounts({
    base: SWARM.base, caps, factor: caps?.tier === 'webgpu' ? SWARM.webgpuFactor : 1,
  });
  const MAX = sized.max;
  const own = {
    start: lib.cpuBuffer(MAX, 'vec4'),
    pos: lib.computeBuffer(MAX, 'vec4'),
    vel: lib.computeBuffer(MAX, 'vec4'),
    acc: lib.computeBuffer(MAX, 'vec4'),
    trail: Array.from({ length: SWARM.trail }, () => lib.computeBuffer(MAX, 'vec4')),
  };
  const k = {
    active: S.uniform(0), dt: S.uniform(0), snap: S.uniform(1), shift: S.uniform(0),
    trailMove: S.uniform(0), weight: S.uniform(0), kick: S.uniform(0), lift: S.uniform(0),
    hero: S.uniform(new T.Vector3()), hs: S.uniform(1),
    px: S.uniform(1),
  };
  let started = null;
  let state = null;
  let first = true;
  let last = null;
  const clock = { t: 0 };

  /** The flock's acceleration on `acc`, from a sample of the others' places and velocities (the
   * compute-written buffers read through the neighbour path). */
  const flockPass = () => {
    const pos = lib.neighbours(own.pos);
    const vel = lib.neighbours(own.vel);
    return S.Fn(() => {
      const i = S.instanceIndex;
      S.If(i.toFloat().lessThan(k.active), () => {
        const me = pos.element(i).xyz;
        const mine = vel.element(i).xyz;
        const sep = S.vec3(0).toVar();
        const mean = S.vec3(0).toVar();
        const dir = S.vec3(0).toVar();
        const seen = S.float(0).toVar();
        const n = S.int(k.active);
        for (let s = 0; s < SWARM.sample; s++) {
          const raw = S.int(i).mul(7919).add((s + 1) * 104729).add(S.int(k.shift).mul(1009)).mod(n);
          const j = S.select(raw.equal(S.int(i)), raw.add(1).mod(n), raw);
          const other = pos.element(j).xyz;
          const d = other.sub(me);
          const r = S.length(d);
          S.If(r.lessThan(SWARM.separation), () => {
            sep.subAssign(d.mul(S.float(SWARM.separation).sub(r).div(S.max(r, 1e-4))));
          });
          S.If(r.lessThan(SWARM.neighbourhood), () => {
            mean.addAssign(other);
            dir.addAssign(vel.element(j).xyz);
            seen.addAssign(1);
          });
        }
        const rest = S.max(seen, 1);
        const pull = mean.div(rest).sub(me).mul(SWARM.wCohere)
          .add(dir.div(rest).sub(mine).mul(SWARM.wAlign));
        const out = sep.mul(SWARM.wSeparate).add(S.select(seen.greaterThan(0), pull, S.vec3(0)));
        own.acc.element(i).assign(S.vec4(out, 0));
      });
    })().compute(MAX);
  };

  /** The step: steering to the shell and the swirl, the flock's acceleration, the integration,
   * the trails; or the start over when `snap` is set. */
  const stepPass = () => S.Fn(() => {
    const i = S.instanceIndex;
    S.If(i.toFloat().lessThan(k.active), () => {
      const home = own.start.element(i);
      const p = own.pos.element(i);
      const v = own.vel.element(i);
      const prev = p.xyz.toVar();
      const dt = k.dt;
      const radius = S.length(prev).max(0.05);
      const out = prev.div(radius);
      const shell = S.float(SWARM.shell).add(home.w.sub(0.5).mul(SWARM.spread))
        .add(u.bass.mul(0.5)).add(k.kick.mul(0.8));
      const swirl = S.vec3(out.z.negate(), 0, out.x).mul(u.mid.mul(0.6).add(0.7));
      const lift = S.vec3(0, S.sin(u.time.mul(0.7).add(home.w.mul(40))).mul(0.25), 0);
      const noise = S.vec3(lib.vn(prev.mul(1.6).add(u.time.mul(0.4))),
        lib.vn(prev.mul(1.6).add(7.3)),
        lib.vn(prev.mul(1.6).add(13.1).sub(u.time.mul(0.3)))).sub(0.5);
      const steer = out.mul(shell.sub(radius).mul(4)).add(swirl).add(lift.mul(k.lift))
        .add(noise.mul(u.flat.mul(2).add(0.8)));
      const accel = flock ? steer.add(own.acc.element(i).xyz) : steer;
      const drag = S.exp(dt.mul(-1.8));
      const vel = v.xyz.add(accel.mul(dt)).mul(drag).toVar();
      const speed = S.length(vel);
      const capped = vel.mul(S.min(1, S.float(SWARM.speedMax).div(S.max(speed, 1e-4)))).toVar();
      const next = prev.add(capped.mul(dt)).toVar();
      S.If(k.snap.greaterThan(0.5), () => {
        p.assign(S.vec4(home.xyz, home.w));
        v.assign(S.vec4(S.vec3(home.z.negate(), 0, home.x).mul(0.6), 0));
      }).Else(() => {
        p.assign(S.vec4(next, home.w));
        v.assign(S.vec4(capped, 0));
      });
    });
  })().compute(MAX);

  /** The trails: the head's place is pushed onto the history when the trail clock is due, and every
   * history is the head's place on a start. A pass of its own, to name at most four buffers. */
  const trailPass = () => S.Fn(() => {
    const i = S.instanceIndex;
    S.If(i.toFloat().lessThan(k.active), () => {
      const head = own.pos.element(i);
      S.If(k.snap.greaterThan(0.5), () => {
        for (const t of own.trail) t.element(i).assign(head);
      }).ElseIf(k.trailMove.greaterThan(0.5), () => {
        own.trail[2].element(i).assign(own.trail[1].element(i));
        own.trail[1].element(i).assign(own.trail[0].element(i));
        own.trail[0].element(i).assign(head);
      });
    });
  })().compute(MAX);

  /** The world place of a history or position buffer: the hero's place plus its local place. */
  const world = (buffer, scale = 1) => ({
    element: (i) => S.vec4(k.hero.add(buffer.element(i).xyz.mul(k.hs).mul(scale)), 0),
  });
  const swarmSprite = (buffer, base, gain) => {
    const i = S.instanceIndex;
    const place = world(buffer);
    const z = S.varying(S.cameraViewMatrix.mul(S.vec4(place.element(i).xyz, 1)).z.negate());
    const hue = S.varying(buffer.element(i).w);
    const level = S.varying(ctx.audio.spectrum(S.int(hue.mul(63))));
    const colour = S.mix(u.colA, u.colB, hue).mul(level.mul(1.4).add(0.35)).mul(gain)
      .mul(k.weight);
    const size = {
      element: () => S.clamp(S.float(base).mul(k.px).mul(k.hs).div(S.max(z, 0.5)).mul(2.2), 1.2, 9),
    };
    return sprites.make({
      count: 1, position: place, size, mode: 'px', colour: lib.finish(colour, u), round: true,
    });
  };

  const passes = { flock: flock ? flockPass() : null, step: stepPass(), trail: trailPass() };
  const heads = swarmSprite(own.pos, 1.0, 1.0);
  const trails = own.trail.map((buffer, n) => swarmSprite(buffer, 0.8 - n * 0.15, 0.55 - n * 0.14));
  for (const s of [heads, ...trails]) object.add(s);
  object.visible = true;

  const setCount = (n) => {
    for (const s of [heads, ...trails]) s.count = n;
  };
  setCount(0);

  /** The state for a test: what a readback of the buffers needs. */
  const probe = () => ({ MAX, own, k, flock, passes });

  return {
    name: 'particles',
    group: 'base',
    object,
    probe,
    build(seed) {
      started = swarmStart(MAX, seed);
      lib.upload(own.start, started.pos);
      state = { seed, shift: 0, due: 0 };
      first = true;
      last = null;
      clock.t = 0;
    },
    update(frame) {
      const hero = ctx.hero?.t === frame.t ? ctx.hero : null;
      const q = frame.budget?.q ?? 1;
      const { n } = particleCounts({
        base: SWARM.base, q, caps: frame.caps ?? caps,
        factor: caps?.tier === 'webgpu' ? SWARM.webgpuFactor : 1,
      });
      const weight = hero ? hero.swarm : 0;
      // the rig's step (Calm runs at half time, a mute stands still), clamped; a reset is the first
      // frame after a build or the probe's (dt exactly 0), never a long step
      const dt = Math.min(SWARM.stepMax, Math.max(0, hero ? hero.sdt : 0));
      const snap = first || frame.dt === 0;
      k.weight.value = weight;
      k.active.value = n;
      k.dt.value = dt;
      k.snap.value = snap ? 1 : 0;
      k.kick.value = hero?.kick ?? 0;
      k.lift.value = hero?.lift ?? 0;
      if (hero) {
        k.hero.value.set(hero.pos[0], hero.pos[1], hero.pos[2]);
        k.hs.value = hero.hs;
      }
      if (hero) {
        k.px.value = hero.px;
      }
      k.trailMove.value = snap ? 0 : trailDue(clock, dt);
      // the flock's sample moves with the trail clock, not with the draws
      if (k.trailMove.value) state.shift = (state.shift + 1) % 1024;
      k.shift.value = state.shift;
      last = { n, weight, snap };
      setCount(weight > 0.01 ? n : 0);
      first = false;
      if (weight <= 0.01 && !snap) return;
      const r = ctx.gpu?.renderer;
      if (!r) return;
      if (passes.flock && !snap) r.compute(passes.flock);
      r.compute(passes.step);
      r.compute(passes.trail);
    },
    state: () => ({ ...last, flock, max: MAX }),
    dispose() {
      for (const s of [heads, ...trails]) s.material.dispose();
    },
  };
};
