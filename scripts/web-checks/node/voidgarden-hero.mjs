/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Void Garden's hero and particles layers (layers/hero.js and
// layers/particles.js): the morph schedule from the director's own morph events (and from the
// eased `u.morph` number when a frame carries no events), the form changes and the sculpture's
// morph, the same weights at 30, 60 and 144 steps a second, the particle counts under the budget's
// steps and the cap, the placement above the land of a handed-in terrain, the shapes of the EQ
// forms against their formulas, the flock's CPU reference and its sample, the trail clock at any
// step rate, the burst only in Full, and the layers' graphs built on both tiers with the vendored
// three (the neighbour pass only on WebGPU). No clock, no
// random, no GPU: a fake renderer takes the compute calls. Run: node voidgarden-hero.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';

const WEB = '../../../bins/svolderd/web/themes/voidgarden/';
const hero = await import(`${WEB}layers/hero.js`);
const swarm = await import(`${WEB}layers/particles.js`);
const { createDirector } = await import(`${WEB}director.js`);
const { createRig } = await import(`${WEB}motion.js`);
const { BUDGET, createBudget } = await import(`${WEB}budget.js`);

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
const near = (got, want, eps, what) =>
  assert.ok(Math.abs(got - want) <= eps, `${what}: ${got} is not within ${eps} of ${want}`);

const RATES = [30, 60, 144];
const CAM = {
  x: 0, y: 6, z: -4, fov: 55, aspect: 1.6, right: [1, 0, 0], up: [0, 1, 0], forward: [0, 0, -1],
};
const LOUD = { bass: 0.5, mid: 0.4, high: 0.3, loud: 0.5, l: 0.4, r: 0.4, flux: 0.2, cent: 0.45,
  flat: 0.2, calm: 0, morph: 0, time: 1 };
/** The director run over `seconds` of a loud, busy piece at `rate` steps a second; returns the
 * morph and section events with their times. */
function directed(rate, seconds, flux = 0.7) {
  const d = createDirector();
  const events = [];
  const values = { bass: 0.5, mid: 0.4, high: 0.3, loud: 0.55, flux, cent: 0.4, flat: 0.2 };
  const raw = { bass: 0.5, mid: 0.4, high: 0.3, peak: false };
  for (let i = 1; i <= Math.round(seconds * rate); i++) {
    const t = i / rate;
    const got = d.step(1 / rate, { frame: null, raw, values, now: t * 1000, offsetMs: 0 });
    for (const e of got) events.push({ ...e, at: t });
  }
  return { d, events };
}

await ok('a_morph_trigger_of_the_director_changes_the_hero_every_time', () => {
  for (const rate of RATES) {
    const { events } = directed(rate, 30);
    const morphs = events.filter((e) => e.type === 'morph');
    assert.ok(morphs.length >= 6, `${rate}: the busy piece morphs ${morphs.length} times`);
    const state = hero.createHeroState(7);
    let changed = 0;
    for (const e of morphs) {
      const was = [state.plan.state.form, state.plan.state.shA, state.plan.state.scM,
        state.plan.state.morphs];
      state.step({ dt: 1 / rate, sdt: 1 / rate, u: LOUD, cam: CAM, events: [e], snap: false });
      const now = [state.plan.state.form, state.plan.state.shA, state.plan.state.scM,
        state.plan.state.morphs];
      assert.equal(now[3], was[3] + 1, 'the trigger is counted');
      if (now[0] !== was[0] || now[2] !== was[2] || now[1] !== was[1]) changed++;
    }
    assert.equal(changed, morphs.length,
      `${rate}: each of ${morphs.length} triggers changes the hero`);
  }
});

await ok('the_morph_schedule_is_the_same_at_30_60_and_144_steps_a_second', () => {
  const seen = RATES.map((rate) => {
    const { events } = directed(rate, 30);
    return events.filter((e) => e.type === 'morph').map((e) => Math.round(e.at));
  });
  assert.deepEqual(seen[0], seen[1]);
  assert.deepEqual(seen[0], seen[2]);
  assert.ok(seen[0].every((t, i) => i === 0 || t - seen[0][i - 1] >= 3), 'at least 3.5 s apart');
});

await ok('the_forms_follow_the_same_schedule_for_the_same_seed_and_differ_for_another', () => {
  const trace = (seed) => {
    const plan = hero.createHeroPlan(seed);
    const out = [plan.state.form];
    for (let i = 0; i < 12; i++) {
      plan.onMorph();
      out.push(plan.state.form * 10 + plan.state.shA);
    }
    return out;
  };
  assert.deepEqual(trace(3), trace(3));
  assert.notDeepEqual(trace(3), trace(4));
});

await ok('a_morph_trigger_read_off_the_eased_number_fires_once_per_toggle_at_any_rate', () => {
  for (const rate of RATES) {
    const d = createDirector();
    const rig = createRig({ seed: 1 });
    const watch = hero.createMorphWatch();
    let fired = 0;
    for (let i = 0; i < rate * 26; i++) {
      // the director's morph toggles every 4 s of the clock
      d.state.morph = Math.floor(i / rate / 4) % 2;
      rig.step(1 / rate, { motion: 'full', features: {}, cues: {}, director: d.state, events: [] });
      fired += watch.sample(rig.u.morph);
    }
    assert.equal(fired, 6, `${rate}: six toggles in 26 s, ${fired} seen`);
  }
});

await ok('the_weights_ease_to_the_same_numbers_at_any_step_rate', () => {
  const at = RATES.map((rate) => {
    const plan = hero.createHeroPlan(5);
    plan.to(plan.state.form === 8 ? 0 : 8);
    for (let i = 0; i < rate * 1.5; i++) plan.step(1 / rate);
    return plan.state.w.map((v) => Number(v.toFixed(4)));
  });
  assert.deepEqual(at[0], at[1]);
  assert.deepEqual(at[0], at[2]);
});

await ok('a_sculpture_morph_takes_4_5_s_and_ends_on_the_new_form', () => {
  const plan = hero.createHeroPlan(9);
  plan.to(8);
  plan.settle();
  let guard = 0;
  while (plan.state.scM < 0 && guard++ < 50) plan.onMorph();
  assert.ok(plan.state.scM >= 0, 'a morph on the sculpture starts the sculpture morph');
  const to = plan.state.shB;
  const from = plan.state.shA;
  assert.notEqual(to, from);
  const rate = 60;
  let t = 0;
  while (plan.state.scM >= 0 && t < 10) {
    plan.step(1 / rate);
    t += 1 / rate;
  }
  near(t, 4.5, 0.05, 'the sculpture morph runs about 4.5 s');
  assert.equal(plan.state.shA, to, 'it ends on the form it went to');
  assert.equal(plan.state.mt, 0);
});

await ok('every_sculpture_form_comes_round_in_a_bag_with_no_repeat_in_a_row', () => {
  const plan = hero.createHeroPlan(2);
  plan.to(8);
  const seen = new Set([plan.state.shA]);
  let prev = plan.state.shA;
  for (let i = 0; i < 9; i++) {
    plan.state.scM = -1;
    plan.state.mt = 0;
    // the schedule's own pick, through a morph
    while (plan.state.scM < 0) plan.onMorph();
    const next = plan.state.shB;
    assert.notEqual(next, prev, 'never the same form twice in a row');
    seen.add(next);
    prev = next;
    plan.state.shA = plan.state.shB;
    plan.state.scM = -1;
    if (plan.state.form !== 8) plan.to(8);
  }
  assert.ok(seen.size >= 8, `the bag gives ${seen.size} of ten forms in ten picks`);
  assert.equal(hero.SCULPTS.length, 10);
  assert.equal(hero.FORMS.length, 9);
});

await ok('the_particle_counts_follow_the_budgets_steps_and_never_pass_the_cap', () => {
  const tiers = [
    { tier: 'webgl2', maxParticles: 40000 },
    { tier: 'webgpu', maxParticles: 160000 },
    { tier: 'webgl2', maxParticles: 1000 },
  ];
  for (const caps of tiers) {
    const b = createBudget();
    b.size(1280, 800, 2);
    b.forceDrawMs(28);
    let last = Infinity;
    const seenQ = new Set();
    for (let i = 1; i <= 90 * 60; i++) {
      b.sample({ ms: 1, gpuMs: null, seconds: i / 60, full: true });
      const c = hero.heroCounts({ q: b.hero, caps });
      const s = swarm.particleCounts({ base: swarm.SWARM.base, q: b.q, caps,
        factor: caps.tier === 'webgpu' ? 4 : 1 });
      assert.ok(c.eq.n <= caps.maxParticles && c.sculpt.n <= caps.maxParticles, 'under the cap');
      assert.ok(s.n <= caps.maxParticles, 'the swarm under the cap');
      assert.ok(c.sculpt.n <= c.sculpt.max && c.eq.n <= c.eq.max, 'under what is held');
      assert.ok(c.sculpt.n <= last, 'never more as the budget steps down');
      last = c.sculpt.n;
      seenQ.add(Number(b.hero.toFixed(3)));
    }
    assert.ok(seenQ.has(BUDGET.hero.at(-1)), 'the ladder reached the hero\'s floor');
    const full = hero.heroCounts({ q: 1, caps });
    const floor = hero.heroCounts({ q: BUDGET.hero.at(-1), caps });
    if (caps.maxParticles >= 40000) {
      assert.equal(floor.sculpt.n, Math.floor(hero.HERO.sculpt * (caps.tier === 'webgpu' ? 2 : 1) * BUDGET.hero.at(-1)),
        'the floor is the hero\'s last share of its base count');
    }
  }
  const gl2 = hero.heroCounts({ q: 1, caps: { tier: 'webgl2', maxParticles: 40000 } });
  const gpu = hero.heroCounts({ q: 1, caps: { tier: 'webgpu', maxParticles: 160000 } });
  assert.equal(gl2.eq.n, 22000, 'WebGL2 keeps the original count of the particle forms');
  assert.equal(gl2.sculpt.n, 40000, 'and the sculpture is held to the cap');
  assert.equal(gpu.eq.n, 44000, 'WebGPU holds twice the particle forms');
  assert.equal(swarm.particleCounts({ base: 6000, caps: { tier: 'webgpu', maxParticles: 160000 },
    factor: 4 }).n, 24000, 'and four times the swarm');
});

/** A rolling land of the kind the land layer will give: height and river by a simple function. */
const terrain = {
  height: (x, z) => 4 + 3 * Math.sin(x * 0.21) * Math.cos(z * 0.13) + 0.05 * Math.abs(z),
  riverX: (z) => 5 * Math.sin(z * 0.04),
};

await ok('the_hero_hangs_above_the_land_ahead_of_the_camera_for_three_seeds', () => {
  for (const seed of [1, 2, 3]) {
    for (const z of [-4, -80, -300, -900]) {
      const state = hero.createHeroState(seed);
      const cam = { ...CAM, z, y: 9 };
      const out = state.step({ dt: 1 / 30, sdt: 1 / 30, u: LOUD, cam, terrain, snap: true });
      const ground = Math.max(terrain.height(out.pos[0], out.pos[2]), 0);
      assert.ok(out.pos[1] >= ground + 1.6 + out.hs * 0.9, `seed ${seed} z ${z}: above the land`);
      assert.ok(out.pos[2] < cam.z - 15, 'ahead of the camera');
    }
  }
  // the flat fallback when there is no terrain
  const flat = hero.createHeroState(1)
    .step({ dt: 1 / 30, sdt: 1 / 30, u: LOUD, cam: CAM, snap: true });
  assert.ok(Number.isFinite(flat.pos[1]) && flat.pos[1] > 0);
});

await ok('the_size_follows_the_section_and_the_loudness_without_a_director', () => {
  const step = (loud) => hero.createHeroState(1).step({
    dt: 1 / 30, sdt: 1 / 30, u: { ...LOUD, loud }, cam: CAM, snap: true });
  const drop = step(0.8).hs;
  const brk = step(0.1).hs;
  assert.ok(drop > brk * 1.6, `a drop (${drop}) is bigger than a break (${brk})`);
});

await ok('the_burst_comes_with_a_section_in_full_and_never_in_calm', () => {
  const full = hero.createHeroState(4);
  full.step({ dt: 1 / 30, sdt: 1 / 30, u: LOUD, cam: CAM, events: [], snap: true });
  const a = full.step({ dt: 1 / 30, sdt: 1 / 30, u: LOUD, cam: CAM,
    events: [{ type: 'section', section: 'drop' }] });
  assert.equal(a.burst, 1);
  assert.equal(a.bursts, 1);
  const b = full.step({ dt: 1 / 30, sdt: 1 / 30, u: LOUD, cam: CAM, events: [] });
  assert.equal(b.burst, 0, 'one step only: an impulse');
  assert.ok(b.ex > 0, 'the analytic burst of the wireframes goes on');
  const calm = hero.createHeroState(4);
  calm.step({ dt: 1 / 30, sdt: 1 / 30, u: { ...LOUD, calm: 1 }, cam: CAM, events: [], snap: true });
  const c = calm.step({ dt: 1 / 30, sdt: 1 / 30, u: { ...LOUD, calm: 1 }, cam: CAM,
    events: [{ type: 'section', section: 'drop' }, { type: 'morph' }] });
  assert.equal(c.burst, 0);
  assert.equal(c.ex, 0);
  assert.equal(c.bd, 0, 'no bend in Calm');
});

await ok('the_deform_follows_the_groove_the_director_hands_in', () => {
  const state = hero.createHeroState(2);
  const director = { groove: { kick: 0.9, snare: 0.4, hat: 0.2, energy: 0.8 }, hits: [3, 7, 11],
    beat: 6.5, section: 'drop' };
  const out = state.step({ dt: 1 / 30, sdt: 1 / 30, u: LOUD, cam: CAM, director, events: [],
    snap: true });
  near(out.kk, 0.9, 1e-9, 'the kick squash');
  near(out.sn, 0.4, 1e-9, 'the snare fold');
  assert.equal(out.hsd, 7, 'the snare counts the fold pattern');
  assert.equal(out.section, 'drop');
  assert.ok(Math.abs(out.tw) > 0.1, 'the twist sways with the beat');
});

await ok('the_band_numbers_give_the_envelopes_when_there_is_no_director_at_any_rate', () => {
  const hits = RATES.map((rate) => {
    const g = hero.createGrooveWatch();
    let kicks = 0;
    for (let i = 0; i < rate * 4; i++) {
      const t = i / rate;
      // a kick every 0.5 s: the bass jumps from 0.1 to 0.9 for 0.1 s
      const bass = (t % 0.5) < 0.1 ? 0.9 : 0.1;
      const out = g.step(1 / rate, { bass, mid: 0.2, high: 0.1 });
      kicks = out.hits[0];
    }
    return kicks;
  });
  for (const k of hits) assert.ok(k >= 7 && k <= 8, `eight kicks in 4 s, ${k} seen`);
});

await ok('the_eq_shapes_are_on_their_formulas_and_the_first_particles_thin_the_whole', () => {
  const N = 4000;
  const { q } = hero.eqTables(N, 3);
  assert.equal(q.length, N * 4);
  let mean = 0;
  for (let i = 0; i < 400; i++) mean += q[i * 4];
  near(mean / 400, 0.5, 0.06, 'the first 400 are spread over the whole form');
  for (let i = 0; i < N; i += 37) {
    const k = [q[i * 4], q[i * 4 + 1], q[i * 4 + 2], q[i * 4 + 3]];
    const s = hero.eqShape(0, k);
    near(Math.hypot(...s), 1.25, 1e-3, 'the sphere has radius 1.25');
    const t = hero.eqShape(1, k);
    const major = Math.hypot(t[0], t[2]);
    assert.ok(major >= 0.68 - 1e-6 && major <= 1.52 + 1e-6 && Math.abs(t[1]) <= 0.42 + 1e-6,
      'the torus');
    const c = hero.eqShape(2, k);
    near(Math.max(...c.map(Math.abs)), 0.95, 1e-3, 'the cube is a box of 0.95');
    const h = hero.eqShape(3, k);
    near(Math.hypot(h[0], h[2]), 0.6, 0.06, 'the helix has radius 0.6');
    const sp = hero.eqShape(4, k);
    assert.ok(Math.hypot(sp[0], sp[2]) <= 1.8 + 1e-6 && Math.abs(sp[1]) <= 0.1,
      'the spiral is flat');
  }
  const keys64 = (seed) => Array.from(hero.eqTables(64, seed).q);
  assert.deepEqual(keys64(3), keys64(3), 'seeded');
  assert.notDeepEqual(Array.from(hero.eqTables(64, 3).q), Array.from(hero.eqTables(64, 4).q));
});

await ok('the_flock_samples_never_pick_the_mote_itself_and_the_reference_obeys_its_rules', () => {
  const N = 500;
  for (let i = 0; i < N; i += 7) {
    for (let s = 0; s < swarm.SWARM.sample; s++) {
      for (const shift of [0, 1, 513]) {
        const j = swarm.flockSample(i, s, N, shift);
        assert.ok(j >= 0 && j < N && j !== i);
      }
    }
  }
  // two motes closer than the separation push apart; a far one is not seen
  const pos = new Float32Array(N * 4);
  const vel = new Float32Array(N * 4);
  for (let i = 0; i < N; i++) pos.set([i * 10, 0, 0, 0], i * 4);
  pos.set([0.1, 0, 0, 0], 3 * 4);
  const state = { pos, vel, n: 4, shift: 0 };
  const a0 = swarm.flockAcc(0, state);
  assert.ok(a0[0] < 0 || a0.every((v) => v === 0), 'pushed away from the near one or alone');
  // cohesion: two near motes (within the neighbourhood, over the separation) pull together
  const p2 = new Float32Array(8 * 4);
  const v2 = new Float32Array(8 * 4);
  for (let i = 0; i < 8; i++) p2.set([0.5, 0, 0, 0], i * 4);
  p2.set([0, 0, 0, 0], 0);
  v2.set([0, 0, 0, 0], 0);
  for (let i = 1; i < 8; i++) v2.set([1, 0, 0, 0], i * 4);
  const a = swarm.flockAcc(0, { pos: p2, vel: v2, n: 8, shift: 0 });
  assert.ok(a[0] > 0, 'cohesion and alignment pull it toward the others');
  assert.deepEqual(a, swarm.flockAcc(0, { pos: p2, vel: v2, n: 8, shift: 0 }), 'deterministic');
});

await ok('the_trails_move_by_the_clock_not_by_the_frame', () => {
  const moves = RATES.map((rate) => {
    const clock = { t: 0 };
    let due = 0;
    for (let i = 0; i < rate * 10; i++) due += swarm.trailDue(clock, 1 / rate);
    return due;
  });
  const want = Math.floor(10 / swarm.SWARM.trailGap);
  for (const m of moves) assert.ok(Math.abs(m - want) <= 1, `${m} of ${want} in 10 s`);
});

await ok('the_swarm_starts_on_a_shell_and_is_the_same_for_a_seed', () => {
  const a = swarm.swarmStart(300, 5);
  const b = swarm.swarmStart(300, 5);
  assert.deepEqual(Array.from(a.pos), Array.from(b.pos));
  for (let i = 0; i < 300; i++) {
    const r = Math.hypot(a.pos[i * 4], a.pos[i * 4 + 1], a.pos[i * 4 + 2]);
    assert.ok(r > 0.9 && r < 2.1, `on the shell (${r})`);
  }
  assert.notDeepEqual(Array.from(a.pos), Array.from(swarm.swarmStart(300, 6).pos));
});

// ---------------------------------------------------------------------- the graphs on both tiers
const T = await import(`${WEB}vendor/three.webgpu.js`);
const S = await import(`${WEB}vendor/three.tsl.js`);
const { createLayers } = await import(`${WEB}layers.js`);

/** A fake gpu of `tier` that records the compute passes it is handed. */
function fakeGpu(tier) {
  const calls = [];
  const caps = { tier, compute: true, neighbours: tier === 'webgpu', timestamps: false,
    maxParticles: tier === 'webgpu' ? 160000 : 40000 };
  return {
    calls,
    T, S, tier, caps,
    renderer: { compute: (node) => calls.push(node),
      getDrawingBufferSize: (v) => v.set(1280, 800) },
    render() {},
  };
}

for (const tier of ['webgl2', 'webgpu']) {
  await ok(`the_layers_build_on_${tier}_and_step_their_passes_with_no_error`, () => {
    const gpu = fakeGpu(tier);
    const layers = createLayers(gpu, { seed: 3 });
    const rig = createRig({ seed: 3 });
    rig.step(1 / 30, {});
    const frame = (extra = {}) => ({ t: 1, dt: 1 / 30, cam: rig.cam, u: rig.u, budget: { q: 1 },
      palette: null, seed: 3, ...extra });
    // the camera needs its matrices: layers.update aims it
    layers.update(frame());
    layers.update(frame());
    const heroLayer = layers.list.find((l) => l.name === 'hero');
    const swarmLayer = layers.list.find((l) => l.name === 'particles');
    const s = swarmLayer.state();
    assert.equal(s.flock, tier === 'webgpu', 'the flock only on WebGPU');
    const probe = swarmLayer.probe();
    assert.equal(Boolean(probe.passes.flock), tier === 'webgpu', 'no neighbour pass on WebGL2');
    assert.ok(gpu.calls.length >= 2, 'the passes were handed to the renderer');
    for (const p of [heroLayer.probe().eqPass, heroLayer.probe().scPass, probe.passes.step,
      probe.passes.trail, ...(probe.passes.flock ? [probe.passes.flock] : [])]) {
      assert.ok(p.isComputeNode);
    }
    assert.ok(heroLayer.probe().NSC <= gpu.caps.maxParticles);
    assert.ok(probe.MAX <= gpu.caps.maxParticles);
  });
}

/** A layer stack on a fake gpu of `tier` with a rig, and `frame(over)` stepping the rig by `dt`. */
function stack(tier, { seed = 5, drawingHeight = 800 } = {}) {
  const gpu = fakeGpu(tier);
  gpu.renderer.getDrawingBufferSize = (v) => v.set(drawingHeight * 1.6, drawingHeight);
  const layers = createLayers(gpu, { seed });
  const rig = createRig({ seed });
  const director = createDirector();
  const api = {
    gpu, layers, rig, director, t: 0,
    heroLayer: layers.list.find((l) => l.name === 'hero'),
    swarmLayer: layers.list.find((l) => l.name === 'particles'),
    step(over = {}) {
      const dt = over.dt ?? 1 / 30;
      api.t += dt;
      rig.step(dt, { motion: over.motion ?? 'full', features: {}, cues: {},
        director: director.state,
        events: [], aspect: 1.6 });
      layers.update({ t: api.t, dt, cam: rig.cam, u: { ...rig.u, ...(over.u ?? {}) },
        budget: { q: over.q ?? 1 }, palette: null, seed, director: director.state,
        events: over.events ?? [], viewH: over.viewH, flash: over.flash });
    },
  };
  return api;
}

/** The hero's uniforms as one comparable object. */
function snapshot(layer) {
  const { h } = layer.probe();
  const num = (u) => (typeof u.value === 'number' ? u.value
    : Array.from(u.value.elements ?? u.value.toArray?.() ?? [u.value.x, u.value.y, u.value.z]));
  return JSON.stringify(Object.fromEntries(Object.entries(h).map(([k, v]) =>
    [k, Array.isArray(v) ? v.map(num) : num(v)])));
}

for (const tier of ['webgl2', 'webgpu']) {
  await ok(`still_draws_a_second_apart_keep_the_triggers_and_the_swarm_on_${tier}`, () => {
    const s = stack(tier);
    s.step({ dt: 0 });
    const was = s.heroLayer.state().form;
    for (let i = 0; i < 12; i++) {
      s.step({ dt: 1, events: [{ type: 'morph' }, { type: 'section', section: 'drop' }] });
    }
    const st = s.heroLayer.state();
    assert.equal(st.morphs, 12, 'every morph trigger is counted');
    assert.notEqual(st.form, was, 'the form moved');
    assert.equal(s.swarmLayer.state().snap, false, 'the swarm is not reset by a long step');
    assert.ok(s.heroLayer.probe().h.snap.value === 0);
  });

  await ok(`the_hero_publishes_at_for_the_beacons_on_${tier}`, () => {
    const s = stack(tier);
    s.step({ dt: 0 });
    s.step();
    const { at } = s.layers.ctx.hero;
    assert.deepEqual(Object.keys(at).sort(), ['size', 'x', 'y', 'z']);
    assert.ok(Object.values(at).every(Number.isFinite));
    assert.equal(at.x, s.layers.ctx.hero.pos[0]);
    assert.equal(at.size, s.layers.ctx.hero.hs);
  });

  await ok(`the_sprites_are_sized_by_the_frames_view_not_the_canvas_on_${tier}`, () => {
    const px = (drawing, viewH) => {
      const s = stack(tier, { drawingHeight: drawing });
      s.step({ dt: 0, viewH });
      const { h } = s.heroLayer.probe();
      return [h.px.value, h.foc.value];
    };
    assert.deepEqual(px(800, 400), px(300, 400), 'the canvas size does not matter with a viewH');
    assert.notDeepEqual(px(800, 400), px(800, 800), 'the viewH does');
    assert.deepEqual(px(500, undefined), px(500, 500), 'without one the canvas is read');
  });

  await ok(`a_probe_frame_after_live_draws_equals_a_fresh_layers_on_${tier}`, () => {
    const fresh = stack(tier);
    fresh.step({ dt: 0 });
    const live = stack(tier);
    live.step({ dt: 0 });
    for (let i = 0; i < 300; i++) {
      const events = i % 40 === 5 ? [{ type: 'morph' }] : i % 90 === 7 ? [{ type: 'section' }] : [];
      live.step({ events, q: i > 150 ? 0.6 : 1 });
    }
    live.layers.build(5);
    live.step({ dt: 0 });
    const flat = (st) => JSON.stringify(JSON.parse(snapshot(st)), (k, v) =>
      (k === 'time' ? undefined : v));
    // the rig differs (time, camera); the hero's own state must not
    const keys = ['pqA', 'pqB', 'pqT', 'shA', 'shB', 'mt', 'kA', 'kB', 'w', 'gly', 'bd', 'ex',
      'burst'];
    const a = JSON.parse(flat(fresh.heroLayer));
    const b = JSON.parse(flat(live.heroLayer));
    for (const k of keys) assert.deepEqual(b[k], a[k], `${k} is the same after a build`);
  });

  await ok(`a_crossfade_shares_the_cap_on_${tier}`, () => {
    const s = stack(tier);
    s.step({ dt: 0 });
    const cap = s.gpu.caps.maxParticles;
    const plan = s.heroLayer.probe().state.plan;
    plan.to(2);
    s.step({ dt: 0 });
    plan.to(8);
    let both = 0;
    for (let i = 0; i < 200; i++) {
      s.step();
      const { counts } = s.heroLayer.state();
      assert.ok(counts.eq + counts.sculpt <= cap, `${counts.eq} + ${counts.sculpt} over ${cap}`);
      const w = plan.state.w;
      if (w[2] > 0.01 && w[3] > 0.01) both++;
    }
    assert.ok(both > 20, 'the crossfade was seen');
    // and the pure rule
    const c = hero.crossfadeCounts({ eq: { n: 22000 }, sculpt: { n: 40000 } },
      [0, 0, 0.5, 0.5], 40000);
    assert.ok(c.eq + c.sculpt <= 40000 && c.eq > 0 && c.sculpt > 0);
  });
}

await ok('the_hero_starts_on_the_lattice_planet_for_every_seed', () => {
  for (const seed of [1, 2, 3, 7, 11, 99]) {
    assert.equal(hero.createHeroPlan(seed).state.form, 0, `seed ${seed}`);
    assert.deepEqual(hero.createHeroPlan(seed).state.w, [1, 0, 0, 0]);
  }
});

await ok('the_instrument_is_128_bars_three_curves_a_disc_and_a_scope_trace', () => {
  const d = hero.ringData();
  const quads = 128 + 3 * 180 + 1;
  assert.equal(d.aR.length, quads * 4 * 4, 'four corners of four numbers a quad');
  assert.equal(d.index.length, quads * 6);
  const types = [0, 0, 0];
  for (let i = 0; i < d.aR.length; i += 4) types[d.aR[i]]++;
  assert.deepEqual(types, [128 * 4, 3 * 180 * 4, 4]);
  assert.equal(d.aW.length / 2, 2 * 257);
  assert.equal(d.wIndex.length, 256 * 6);
  const view = { right: [1, 0, 0], up: [0, 1, 0], forward: [0, 0, -1], fov: 55, aspect: 1.6 };
  const ahead = hero.heroScreen([0, 0, -20], view);
  assert.deepEqual([ahead.x, ahead.y, ahead.on], [0, 0, 1]);
  const right = hero.heroScreen([5, 0, -20], view);
  near(right.x, 5 / (20 * Math.tan((55 * Math.PI) / 360) * 1.6), 1e-9, 'the right of the centre');
  assert.equal(hero.heroScreen([0, 0, 20], view).on, 0, 'behind the camera it is off');
});

for (const tier of ['webgl2', 'webgpu']) {
  await ok(`the_hero_and_the_beacons_blend_after_the_river_and_the_hero_has_its_instrument_on_${tier}`, () => {
    const s = stack(tier);
    s.step({ dt: 0 });
    s.step();
    const order = (layer) => {
      const out = [];
      layer.object.traverse((o) => { if (o.isMesh || o.isSprite) out.push(o.renderOrder); });
      return out;
    };
    const river = Math.max(...order(s.layers.list.find((l) => l.name === 'river')));
    const heroOrders = s.heroLayer.object.children.map((o) => o.renderOrder);
    assert.equal(heroOrders.length, 7, 'lattice, icosphere, halo, two sprites, the ring, the scope');
    assert.ok(heroOrders.every((o) => o > river), `the hero's ${heroOrders} come after the river's ${river}`);
    const beacons = s.layers.list.find((l) => l.name === 'beacons');
    const lights = beacons.object.children.filter((o) => o.material.transparent
      || o.material.blending === T.AdditiveBlending);
    assert.ok(lights.length >= 4 && lights.every((o) => o.renderOrder > river));
  });
}

await ok('the_sections_stage_the_ring_the_curves_and_the_scope_and_they_ease_in_and_out', () => {
  // an intro that draws the ring off (the ring is on unless an intro's coin says no)
  let seed = 0;
  for (let k = 1; k < 200; k++) {
    const p = hero.createHeroPlan(k);
    p.onSection('intro');
    if (!p.state.ring) { seed = k; break; }
  }
  assert.ok(seed > 0, 'some seed draws the ring off at an intro');
  const plan = hero.createHeroPlan(seed);
  plan.onSection('intro');
  assert.equal(plan.state.ring, false);
  assert.equal(plan.state.rw, 1, 'the weight has not jumped');
  const seen = [];
  for (let i = 0; i < 240; i++) {
    plan.step(1 / 30);
    seen.push(plan.state.rw);
  }
  for (let i = 1; i < seen.length; i++) assert.ok(seen[i] <= seen[i - 1], 'it only falls');
  assert.ok(seen.some((v) => v > 0.1 && v < 0.9), 'it eases through the middle (no jump)');
  assert.ok(seen[0] > 0.9, 'the first step is small');
  assert.ok(seen.at(-1) < 0.01, 'and it ends off');
  // a drop always has the curves and the scope; a break rarely the scope
  for (let k = 1; k < 30; k++) {
    const p = hero.createHeroPlan(k);
    p.onSection('drop');
    assert.ok(p.state.ring && p.state.curves && p.state.scope, `seed ${k}: a drop shows all`);
  }
  // on the layer: the weights reach the uniforms and the meshes hide below 0.01
  const s = stack('webgl2', { seed });
  s.step({ dt: 0 });
  s.step();
  const [ring, scope] = s.heroLayer.object.children.slice(-2);
  s.step({ events: [{ type: 'section', section: 'intro' }] });
  const { h, state } = s.heroLayer.probe();
  assert.equal(state.plan.state.ring, false);
  assert.ok(h.rw.value > 0.9 && ring.visible, 'one frame later the ring is still on');
  for (let i = 0; i < 300; i++) s.step();
  assert.ok(h.rw.value < 0.01, 'the uniform eased to 0');
  if (!state.plan.state.curves) assert.ok(h.rcw.value < 0.01, 'curves off: the curve weight eased to 0');
  else assert.ok(h.rcw.value > 0.99, 'curves on: the curve weight is up');
  assert.equal(ring.visible, h.rw.value + h.rcw.value > 0.01, 'the ring shows while a weight does');
  assert.equal(scope.visible, h.sw.value > 0.01);
});

await ok('a_morph_may_toggle_the_parts_and_the_parts_use_a_stream_of_their_own', () => {
  const a = hero.createHeroPlan(9);
  const b = hero.createHeroPlan(9);
  for (let i = 0; i < 12; i++) a.onMorph();
  for (let i = 0; i < 12; i++) { b.onMorph(); b.state.ring = b.state.curves = b.state.scope = true; }
  assert.equal(a.state.form, b.state.form, 'the forms do not depend on the parts');
  const t = hero.createHeroPlan(9);
  let toggled = false;
  for (let i = 0; i < 40 && !toggled; i++) {
    const was = [t.state.ring, t.state.curves, t.state.scope].join();
    t.onMorph();
    toggled = was !== [t.state.ring, t.state.curves, t.state.scope].join();
  }
  assert.ok(toggled, 'some morph toggles a part');
});

await ok('the_scope_reads_the_heros_place_into_a_reused_object', () => {
  const view = { right: [1, 0, 0], up: [0, 1, 0], forward: [0, 0, -1], fov: 55, aspect: 1.6 };
  const out = { x: 9, y: 9, on: 9 };
  assert.equal(hero.heroScreen([0, 0, -20], view, out), out);
  assert.deepEqual(out, { x: 0, y: 0, on: 1 });
});

for (const tier of ['webgl2', 'webgpu']) {
  await ok(`the_beacon_bodies_are_the_10_by_12_grid_not_a_capsule_on_${tier}`, () => {
    const s = stack(tier);
    const g = s.layers.list.find((l) => l.name === 'beacons').bodyGeometry;
    const pos = g.getAttribute('position');
    assert.equal(pos.count, 120, '10 rows of 12 sides');
    assert.equal(g.index.count, 9 * 12 * 2 * 3, '9 bands of 12 quads, two triangles each');
    const rows = new Set();
    for (let i = 0; i < pos.count; i++) rows.add(pos.getX(i));
    assert.deepEqual([...rows].sort((p, q) => p - q), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
  });
  await ok(`the_first_world_draws_obelisks_stone_rings_pillars_and_the_lighthouse_with_a_cluster_near_the_start_on_${tier}`, () => {
    const s = stack(tier, { seed: 5 });
    const b = s.layers.list.find((l) => l.name === 'beacons');
    assert.deepEqual(b.state.kinds, [3, 5, 0, 2]);
    s.step({ dt: 0 });
    const kinds = new Set(b.state.members.map((m) => m.kind));
    for (const k of kinds) assert.ok([3, 5, 0, 2].includes(k), `kind ${k} is one of the four`);
    const near = b.state.members.filter((m) => Math.abs(m.cz - (-105)) < 15);
    assert.ok(near.length > 0 && near.every((m) => m.kind === 3), 'an obelisk cluster stands ahead');
  });
}

await ok('the_peaks_pulse_is_a_grant_of_the_limiter_and_never_in_calm', () => {
  const run = (flash, calm = 0) => {
    const state = hero.createHeroState(3);
    state.step({ dt: 1 / 30, sdt: 1 / 30, u: { ...LOUD, calm }, cam: CAM, snap: true });
    state.step({ dt: 1 / 30, sdt: 1 / 30, u: { ...LOUD, calm, peak: 0 }, cam: CAM, flash });
    return state.step({ dt: 1 / 30, sdt: 1 / 30, u: { ...LOUD, calm, peak: 1 }, cam: CAM, flash })
      .pulse;
  };
  assert.equal(run(undefined), 0, 'no limiter on the frame: no pulse');
  assert.equal(run(() => false), 0, 'refused: no pulse');
  assert.ok(run(() => true) > 0.9, 'granted: a pulse');
  assert.equal(run(() => true, 1), 0, 'none in Calm');
});

await ok('with_the_directors_events_the_pulse_rides_the_peaks_own_grant', () => {
  // one grant a peak: the director's peak event carries it (`flash`), as the beacons and the
  // post chain read it, so the hero never asks the frame's limiter itself then
  const run = (events, calm = 0) => {
    let asked = 0;
    const flash = () => { asked++; return true; };
    const state = hero.createHeroState(3);
    const at = (peak, evs) => ({ dt: 1 / 30, sdt: 1 / 30, u: { ...LOUD, calm, peak }, cam: CAM,
      director: {}, events: evs, flash });
    state.step({ ...at(0, []), snap: true });
    state.step(at(0, []));
    return { pulse: state.step(at(1, events)).pulse, asked };
  };
  const granted = run([{ type: 'peak', flash: true }]);
  assert.ok(granted.pulse > 0.9, 'a granted peak event: a pulse');
  assert.equal(granted.asked, 0, 'the hero never asks the limiter itself');
  assert.equal(run([{ type: 'peak', flash: false }]).pulse, 0, 'a refused peak: no pulse');
  assert.equal(run([]).pulse, 0, 'no peak event: no pulse, whatever u.peak says');
  assert.equal(run([{ type: 'peak', flash: true }], 1).pulse, 0, 'none in Calm');
});

await ok('the_spring_and_the_swarm_step_on_the_rigs_time_not_the_pages', () => {
  const s = stack('webgpu');
  s.step({ dt: 0 });
  s.step({ dt: 1 / 30 });
  const rate = s.heroLayer.probe().h.dt.value;
  assert.ok(rate > 0);
  // a muted scene's clock stands still: the integrators get no time
  const mute = { motion: 'full', features: {}, cues: { mute: true }, director: s.director.state };
  s.rig.step(1 / 30, mute);
  for (let i = 0; i < 60; i++) {
    s.rig.step(1 / 30, mute);
  }
  s.layers.update({ t: 9, dt: 1 / 30, cam: s.rig.cam, u: s.rig.u, budget: { q: 1 }, palette: null,
    seed: 5, director: s.director.state, events: [] });
  s.layers.update({ t: 9.03, dt: 1 / 30, cam: s.rig.cam, u: s.rig.u, budget: { q: 1 },
    palette: null, seed: 5, director: s.director.state, events: [] });
  assert.ok(s.heroLayer.probe().h.dt.value < rate * 0.2, 'the spring slows with the clock');
});

await ok('the_layers_use_no_clock_no_random_and_no_points_object', () => {
  for (const file of ['layers/hero.js', 'layers/particles.js']) {
    const text = readFileSync(new URL(`${WEB}${file}`, import.meta.url), 'utf8')
      .replace(/\/\/.*$/gm, '');
    for (const bad of [/Math\.random/, /Date\.now/, /performance\.now/, /new T\.Points\b/,
      /THREE\.Points/]) {
      assert.ok(!bad.test(text), `${file}: ${bad}`);
    }
  }
});

await ok('the_hero_files_keep_to_100_columns', () => {
  for (const file of ['layers/hero.js', 'layers/particles.js']) {
    const text = readFileSync(new URL(`${WEB}${file}`, import.meta.url), 'utf8');
    for (const [i, line] of text.split('\n').entries()) {
      assert.ok(line.length <= 100, `${file}:${i + 1} is ${line.length} columns`);
    }
  }
});

console.log(`voidgarden-hero: ${n} passed`);
