/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Void Garden's fx layers (themes/voidgarden/layers/beacons.js,
// layers/post.js, tsl/post.js), the parts that need no GPU: the accents' gate (never more than the
// flash limiter's count in any second, at 30, 60 and 144 steps a second, with the director's own
// flashes counted in the same window; none in Calm or while muted; 0.4 s apart), the strength of
// each post effect for a frame (the budget's grit step, then the bloom's second octave; Calm; the
// bisect switches; the haze that follows the changed-path cue), the director's mix eased by time
// constants, the groove a frame carries, the beacons' placement (deterministic, on the terrain,
// within the pool), the integers packed for the compute passes, the heights table an eruption
// lands on, and the particle counts under the budget and the caps. Run: node voidgarden-fx.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { page as domPage } from './dom.mjs';
import { wireScene } from '../../../bins/svolderd/web/themes/voidgarden/scene-wire.js';
import { FlashLimiter, MAX_FLASHES, WINDOW_S } from '../../../bins/svolderd/web/themes/voidgarden/director.js';
import { createBudget } from '../../../bins/svolderd/web/themes/voidgarden/budget.js';
import {
  ACCENT_GAP_S, FAMILIES, createAccents,
} from '../../../bins/svolderd/web/themes/voidgarden/layers/post.js';
import {
  BEACON, BITS, POOLS, cellsAt, classWidth, countsOf, erupts, heightsOf, packInts, packStatic,
  SHAPE_LISTS, UNFOLD, calmness, placeBeacons, unfold, widthClass, worldKinds,
} from '../../../bins/svolderd/web/themes/voidgarden/layers/beacons.js';
import {
  EFFECTS, GRIT, BLOOM2, createMix, createSurge, eventsOf, flashOf, grooveOf, levelsOf, mixOf,
} from '../../../bins/svolderd/web/themes/voidgarden/tsl/post.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const near = (got, want, eps, what) =>
  assert.ok(Math.abs(got - want) <= eps, `${what}: ${got} is not within ${eps} of ${want}`);

const RATES = [30, 60, 144];

/** The most grants in any window of `WINDOW_S` seconds of the times `ts`. */
function worstWindow(ts) {
  let worst = 0;
  for (let i = 0; i < ts.length; i++) {
    let count = 0;
    for (let j = i; j < ts.length && ts[j] - ts[i] < WINDOW_S - 1e-9; j++) count++;
    worst = Math.max(worst, count);
  }
  return worst;
}

/** The accents asked for on every step of `seconds` seconds at `rate` steps a second, beside a
 * director that asks for its own flash every step, all through one limiter. */
function spam(rate, seconds, options = {}) {
  const limiter = new FlashLimiter();
  const accents = createAccents();
  const dt = 1 / rate;
  const grants = [];
  const fired = [];
  let t = 0;
  const flash = (at) => {
    const granted = limiter.grant(at);
    if (granted) grants.push(at);
    return granted;
  };
  const kinds = ['barcode', 'mosh', 'vhs', 'sort', 'flare'];
  for (let i = 0; i < Math.round(seconds * rate); i++) {
    t += dt;
    // the director's flashes (a peak that is granted) compete for the same window
    if (i % 5 === 0) flash(t);
    for (const kind of kinds) {
      if (accents.fire(kind, { t, flash, ...options })) fired.push(t);
    }
    accents.step(dt);
  }
  return { grants, fired, accents };
}

ok('the accent gate never passes more than the limiter allows, at 30, 60 and 144 steps a second', () => {
  for (const rate of RATES) {
    const { grants, fired } = spam(rate, 12);
    assert.ok(fired.length >= 6, `${rate}/s: the case is vacuous (${fired.length} accents)`);
    assert.ok(worstWindow(grants) <= MAX_FLASHES, `${rate}/s: ${worstWindow(grants)} grants in a second`);
    for (const t of fired) assert.ok(grants.includes(t), `${rate}/s: an accent at ${t} the limiter did not grant`);
    for (let i = 1; i < fired.length; i++) {
      assert.ok(fired[i] - fired[i - 1] >= ACCENT_GAP_S - 1e-9, `${rate}/s: two accents ${fired[i] - fired[i - 1]} s apart`);
    }
  }
});

ok('an accent that the limiter refuses does not fire, and one that fires is counted', () => {
  const accents = createAccents();
  assert.equal(accents.fire('mosh', { t: 1, flash: () => false }), false);
  assert.equal(accents.levels().mosh, 0);
  assert.equal(accents.state.fires, 0);
  assert.equal(accents.fire('mosh', { t: 1, flash: () => true }), true);
  assert.equal(accents.state.fires, 1);
});

ok('no accent in Calm or while muted', () => {
  for (const options of [{ calm: true }, { mute: true }]) {
    const { fired } = spam(60, 5, options);
    assert.equal(fired.length, 0, JSON.stringify(options));
  }
});

ok('the accents come from the section\'s own family, in turn', () => {
  for (const [section, family] of Object.entries(FAMILIES)) {
    const accents = createAccents();
    let t = 0;
    const seen = [];
    for (let i = 0; i < family.length * 2; i++) {
      t += 1;
      accents.onEvents([{ type: 'section', section }], { section, t, bpm: 120, flash: () => true });
      accents.step(0.1);
      seen.push(...Object.entries(accents.levels()).filter(([k, v]) => k !== 'ca' && v > 0).map(([k]) => k));
    }
    assert.ok(accents.state.fires === family.length * 2, `${section}: ${accents.state.fires}`);
    assert.deepEqual([...new Set(seen)].filter((k) => !family.includes(k)), [], section);
  }
});

ok('an accent rises over 0.09 s, falls over the last of its length, and is gone after it', () => {
  const accents = createAccents();
  accents.fire('vhs', { t: 1, dur: 1, flash: () => true });
  near(accents.levels().vhs, 0, 1e-9, 'at the start');
  accents.step(0.09);
  assert.ok(accents.levels().vhs > 0.95);
  accents.step(0.4);
  near(accents.levels().vhs, 1, 1e-9, 'held');
  accents.step(0.35);
  assert.ok(accents.levels().vhs < 0.9 && accents.levels().vhs > 0);
  accents.step(0.3);
  assert.equal(accents.levels().vhs, 0);
});

ok('a peak that was granted a flash kicks the chromatic split, which decays by time', () => {
  for (const rate of RATES) {
    const accents = createAccents();
    accents.onEvents([{ type: 'peak', flash: true }], { section: 'drop', t: 1, flash: () => true });
    assert.equal(accents.levels().ca, 1);
    for (let i = 0; i < rate; i++) accents.step(1 / rate);
    near(accents.levels().ca, Math.exp(-3), 1e-6, `${rate}/s`);
  }
});

ok('the events of a frame: the director\'s, or the peaks the rig\'s number shows, granted by the gate', () => {
  const ctx = {};
  const state = { peak: 0 };
  assert.deepEqual(eventsOf(ctx, { events: [{ type: 'kick' }] }, state), [{ type: 'kick' }]);
  assert.equal(eventsOf(ctx, { u: { peak: 0.1 }, t: 1 }, state).length, 0);
  const [peak] = eventsOf(ctx, { u: { peak: 1 }, t: 2 }, state);
  assert.deepEqual(peak, { type: 'peak', flash: true });
  assert.equal(eventsOf(ctx, { u: { peak: 1 }, t: 3 }, state).length, 0, 'a held peak is one peak');
  const again = { peak: 0 };
  const same = eventsOf(ctx, { u: { peak: 1 }, t: 2 }, again);
  assert.equal(same[0], peak, 'the other layer of the same frame gets the same answer');
});

ok('the flash gate is the wire\'s when it hands one in, else one limiter the layers share', () => {
  const ctx = {};
  const mine = (t) => t < 1;
  assert.equal(flashOf(ctx, { flash: mine }), mine);
  const a = flashOf(ctx, {});
  const b = flashOf(ctx, {});
  assert.ok(a(0) && b(0.1) && a(0.2));
  assert.equal(b(0.3), false, 'the fourth grant of one window is refused across both');
});

ok('every flash of the fx layers (accents, the beacons\' surge, the chroma kick) passes the limiter', () => {
  for (const rate of RATES) {
    const limiter = new FlashLimiter();
    const accents = createAccents();
    const surge = createSurge();
    const dt = 1 / rate;
    const grants = [];
    const flash = (t) => {
      const ok2 = limiter.grant(t);
      if (ok2) grants.push(t);
      return ok2;
    };
    const lit = [];
    let t = 0;
    let next = 0.4;
    let refused = 0;
    for (let i = 0; i < Math.round(20 * rate); i++) {
      t += dt;
      const events = [];
      // a director with a peak every 0.4 s; it asks the limiter and says what it was granted
      if (t >= next - 1e-9) {
        next += 0.4;
        events.push({ type: 'peak', flash: flash(t) });
        if (!events[0].flash) refused++;
      }
      events.push({ type: 'section', section: 'drop' });
      accents.onEvents(events, { section: 'drop', t, bpm: 120, flash });
      const before = surge.state.level;
      const level = surge.step(dt, events, false);
      if (level > before) lit.push(t);
      if (accents.state.ca === 1) lit.push(t);
      accents.step(dt);
    }
    assert.ok(refused > 5, `${rate}/s: the limiter refused some peaks (${refused}), so the case bites`);
    assert.ok(worstWindow(grants) <= MAX_FLASHES, `${rate}/s: ${worstWindow(grants)} flashes in a second`);
    assert.ok(lit.length > 5, `${rate}/s: surges happened`);
    for (const at of lit) {
      assert.ok(grants.some((g) => Math.abs(g - at) < 1e-9), `${rate}/s: a surge or kick at ${at} with no grant`);
    }
  }
});

ok('a peak the limiter refused lights nothing; Calm lights nothing; the surge decays by time', () => {
  const surge = createSurge();
  surge.step(0.1, [{ type: 'peak', flash: false }], false);
  assert.equal(surge.state.level, 0);
  surge.step(0.1, [{ type: 'peak', flash: true }], true);
  assert.equal(surge.state.level, 0, 'Calm');
  const accents = createAccents();
  accents.onEvents([{ type: 'peak', flash: false }], { section: 'drop', t: 1, flash: () => true, calm: false });
  assert.equal(accents.state.ca, 0, 'a refused peak does not kick the split');
  for (const rate of RATES) {
    const s = createSurge();
    s.step(0, [{ type: 'peak', flash: true }], false);
    for (let i = 0; i < rate; i++) s.step(1 / rate, [], false);
    near(s.state.level, 0.8 * Math.exp(-1.3), 1e-9, `${rate}/s`);
  }
});

ok('a seed\'s start leaves nothing of the frames before it: the mix, the accents, the surge', () => {
  const mix = createMix();
  for (let i = 0; i < 100; i++) mix.step(0.1, 'drop');
  mix.reset();
  assert.deepEqual(mix.state, createMix().state);
  const accents = createAccents();
  accents.fire('mosh', { t: 1, flash: () => true });
  accents.state.ca = 1;
  accents.reset();
  assert.deepEqual(accents.levels(), { barcode: 0, mosh: 0, vhs: 0, sort: 0, flare: 0, ca: 0 });
  assert.equal(accents.fire('vhs', { t: 1.01, flash: () => true }), true, 'the gap is forgotten too');
});

// ------------------------------------------------------------------ the live path
await (async () => {
  const doc = JSON.parse(readFileSync(new URL('./fixtures/voidgarden-pages.json', import.meta.url)));
  const p = domPage('<div id="voidgarden"><canvas id="vg-scene"></canvas></div>');
  const control = new AbortController();
  let loop = null;
  const core = {
    prefs: { get: (k, d) => d, set() {} },
    model: { page: () => doc.pages.base },
    computer: { side: () => 'device' },
    lifecycle: {
      canvasLoop: (o) => { loop = o; return { start() {}, stop() {}, running: false, reduced: false }; },
      watchVisible: () => ({ stop() {} }),
    },
    analysis: { open: () => ({ close() {} }) },
  };
  const vg = {
    page: p.document.getElementById('voidgarden'), core, signal: control.signal, prefs: core.prefs,
    steps: new EventTarget(),
  };
  p.window.WebGL2RenderingContext = function WebGL2RenderingContext() {};
  const updates = [];
  const layers = {
    seed: 0, ctx: { audio: { set() {} } }, update: (f) => updates.push(f), draw() {}, build() {},
    readStats: async () => ({}), dispose() {},
  };
  const gpu = { tier: 'webgl2', caps: { tier: 'webgl2' }, size() {}, target: () => ({}), dispose() {} };
  wireScene(vg, { createGpu: async () => gpu, makeLayers: () => layers });
  await new Promise((resolve) => setTimeout(resolve, 0));
  ok('the wire hands the layers the director\'s events, state, section, cues and its own limiter', () => {
    loop.draw(0.1, 1);
    loop.draw(0.1, 2);
    const f = updates.at(-1);
    assert.ok(Array.isArray(f.events), 'events');
    assert.equal(typeof f.director.groove.kick, 'number', 'the director\'s state');
    assert.equal(f.section, f.director.section);
    assert.equal(typeof f.cues, 'object');
    assert.equal(typeof f.flash, 'function');
    // the limiter is the director\'s: three grants a second, the fourth refused
    const grants = [f.flash(), f.flash(), f.flash(), f.flash()];
    assert.deepEqual(grants, [true, true, true, false]);
  });
  control.abort();
})();

// ------------------------------------------------------------------ the strengths of the effects
const quiet = { energy: 0.3, kick: 0 };
const base = () => ({
  u: { loud: 0.5, bass: 0.4, frost: 0, haze: 0 },
  groove: quiet,
  mix: { bg: 1, exposure: 1, lift: 0 },
  accents: { barcode: 0, mosh: 0, vhs: 0, sort: 0, flare: 0, ca: 0 },
  signal: { vhs: 0, block: 0, sort: 0 },
  budget: { grit: true, bloom2: true },
});

ok('every effect has a strength and the lists name effects that exist', () => {
  const { strengths } = levelsOf(base());
  assert.deepEqual(Object.keys(strengths).sort(), [...EFFECTS].sort());
  for (const name of [...GRIT, ...BLOOM2]) assert.ok(EFFECTS.includes(name), name);
});

ok('the budget turns the bloom\'s second octave off first, then the grit effects: the look goes last', () => {
  const budget = createBudget();
  budget.size(1920, 1080, 1);
  budget.forceDrawMs(40);
  let seconds = 0;
  const seen = [];
  for (let i = 0; i < 900; i++) {
    seconds += 1 / 30;
    budget.sample({ ms: 20, seconds });
    const input = base();
    input.budget = { grit: budget.grit, bloom2: budget.bloom2 };
    input.accents = { barcode: 1, mosh: 1, vhs: 1, sort: 1, flare: 0, ca: 0 };
    input.u.frost = 1;
    const { strengths } = levelsOf(input);
    seen.push([budget.step, budget.grit, budget.bloom2,
      GRIT.every((k) => strengths[k] === 0) && strengths.grain === 1 && strengths.dither > 0,
      strengths.halation > 0]);
  }
  const first = seen.find((s) => !s[2]);
  assert.deepEqual(first.slice(1), [true, false, false, false], 'halation off first, the grit still drawn');
  assert.ok(first[0] > 1, 'after the quiet steps');
  const second = seen.find((s) => !s[1]);
  assert.deepEqual(second.slice(1), [false, false, true, false], 'then the grit off');
  assert.ok(second[0] > first[0]);
  assert.deepEqual(seen[0].slice(1), [true, true, false, true]);
});

ok('Calm draws no accent and no pumping; Full does', () => {
  const full = base();
  full.accents = { barcode: 1, mosh: 1, vhs: 1, sort: 1, flare: 1, ca: 1 };
  const calm = { ...full, calm: true };
  const f = levelsOf(full).strengths;
  const c = levelsOf(calm).strengths;
  for (const k of ['barcode', 'mosh', 'vhs', 'sort']) {
    assert.ok(f[k] > 0, k);
    assert.equal(c[k], 0, `${k} in Calm`);
  }
  assert.ok(c.anamorphic < f.anamorphic);
  assert.ok(c.chroma < f.chroma);
});

ok('the bisect switches zero one effect and leave the rest', () => {
  const input = base();
  const all = levelsOf(input).strengths;
  const some = levelsOf({ ...input, off: new Set(['bloom', 'chroma']) }).strengths;
  assert.equal(some.bloom, 0);
  assert.equal(some.chroma, 0);
  for (const k of EFFECTS.filter((e) => e !== 'bloom' && e !== 'chroma')) assert.equal(some[k], all[k], k);
});

ok('the heat haze follows the changed-path cue and the 1-bit grade the mute', () => {
  const input = base();
  assert.equal(levelsOf(input).strengths.haze, 0);
  input.u.haze = 1;
  assert.equal(levelsOf(input).strengths.haze, 1);
  input.u.haze = 0.4;
  near(levelsOf(input).strengths.haze, 0.4, 1e-9, 'half');
  assert.equal(levelsOf(base()).strengths.dither, 0);
  const muted = base();
  muted.u.frost = 1;
  near(levelsOf(muted).strengths.dither, 0.6, 1e-9, 'muted');
});

ok('the link\'s cues drive the VHS, the datamosh and the sort when no accent does', () => {
  const input = base();
  input.signal = { vhs: 1, block: 1, sort: 0.8 };
  const s = levelsOf(input).strengths;
  assert.equal(s.vhs, 1);
  assert.equal(s.mosh, 1);
  assert.equal(s.sort, 0.8);
});

// ------------------------------------------------------------------ the director's mix
ok('the director\'s mix: a section sets the lights\' gain and the exposure; unknown reads as a build', () => {
  assert.ok(mixOf('drop').bg > mixOf('build').bg && mixOf('build').bg > mixOf('break').bg);
  assert.ok(mixOf('break').bg > mixOf('intro').bg);
  assert.ok(mixOf('drop').exposure > mixOf('intro').exposure);
  assert.deepEqual(mixOf('nonsense'), mixOf('build'));
});

ok('the mix eases by a time constant: the same state after two seconds at every step rate', () => {
  const ends = RATES.map((rate) => {
    const mix = createMix();
    for (let i = 0; i < 2 * rate; i++) mix.step(1 / rate, 'drop');
    return { ...mix.state };
  });
  for (const e of ends) for (const k of Object.keys(e)) near(e[k], ends[0][k], 1e-9, k);
  assert.ok(ends[0].bg > 0.55 && ends[0].bg < 1, 'eased, not jumped');
});

ok('the groove of a frame: the director\'s when handed in, else one from the rig\'s numbers', () => {
  const groove = {
    kick: 0.7, snare: 0.2, hat: 0.1, energy: 0.8, bar: 0.25, down: 0.03,
  };
  const g = grooveOf({
    u: {}, director: { groove, beat: 9.5, bpm: 128, hits: [4, 2, 1], section: 'drop' }, section: 'drop',
  });
  assert.deepEqual(
    [g.kick, g.energy, g.beat, g.bpm, g.kn, g.sn, g.section],
    [0.7, 0.8, 9.5, 128, 4, 2, 'drop'],
  );
  const f = grooveOf({ u: { time: 8, bass: 0.9, mid: 0.2, high: 0.5, loud: 0.5 } });
  assert.equal(f.beat, 16);
  assert.equal(f.bpm, 120);
  near(f.kick, 0.78, 1e-9, 'kick');
  assert.equal(f.snare, 0);
  assert.equal(f.section, 'build');
});

// ------------------------------------------------------------------ the beacons' placement
const hills = {
  height: (x, z) => 3 + 0.08 * x + 1.5 * Math.sin(z / 6) + 0.8 * Math.cos(x / 5),
  riverX: (z) => 4 * Math.sin(z / 40),
};

ok('the placement is the seed\'s and the cell\'s: the same twice, another seed differs', () => {
  const cells = cellsAt(-200);
  const a = placeBeacons({ seed: 7, cells, terrain: hills });
  const b = placeBeacons({ seed: 7, cells, terrain: hills });
  const c = placeBeacons({ seed: 8, cells, terrain: hills });
  assert.ok(a.length > 5);
  assert.deepEqual(a, b);
  assert.notDeepEqual(a.map((m) => m.x), c.map((m) => m.x));
  // a cell keeps its cluster when the window moves on by one
  const next = placeBeacons({ seed: 7, cells: cells.slice(1), terrain: hills });
  const shared = a.filter((m) => m.z < -BEACON.cell * (cells[1] + 0.2) - 1e-9);
  for (const m of next.slice(0, 3)) assert.ok(shared.some((s) => s.x === m.x && s.z === m.z));
});

ok('every member stands on the terrain (the water level at least) and within the pool', () => {
  for (const seed of [1, 7, 23]) {
    const members = placeBeacons({ seed, cells: cellsAt(-300), terrain: hills });
    assert.ok(members.length <= BEACON.members);
    for (const m of members) {
      const want = Math.max(hills.height(m.x, m.z), 0) + (m.kind === 4 ? 0.25 : 0);
      near(m.y, want, 1e-9, `seed ${seed} kind ${m.kind}`);
    }
  }
});

ok('without a terrain the ground is flat and the river straight', () => {
  const members = placeBeacons({ seed: 7, cells: cellsAt(0) });
  assert.ok(members.length > 0);
  for (const m of members) assert.equal(m.kind === 4 ? m.y - 0.25 : m.y, 0);
});

ok('the window of cells follows the camera: one behind, six ahead', () => {
  assert.deepEqual(cellsAt(0), [0, 1, 2, 3, 4, 5, 6, 7]);
  assert.deepEqual(cellsAt(-95), [0, 1, 2, 3, 4, 5, 6, 7]);
  assert.equal(cellsAt(-300)[0], 2);
  assert.equal(cellsAt(-300).length, BEACON.behind + BEACON.ahead + 1);
});

ok('the packed integers read back: kind, light mode, wet, width, index, size, shapes', () => {
  const members = placeBeacons({ seed: 7, cells: cellsAt(-100), terrain: hills });
  for (const m of members) {
    const p = packInts(m);
    assert.ok(Number.isInteger(p) && p < 2 ** 24);
    const bits = (shift, width) => Math.floor(p / 2 ** shift) % 2 ** width;
    assert.equal(bits(BITS.kind, 3), m.kind);
    assert.equal(bits(BITS.lm, 2), m.lm);
    assert.equal(bits(BITS.wet, 1), m.wet);
    assert.equal(bits(BITS.j, 6), m.j);
    assert.equal(bits(BITS.n, 6), m.n);
    near(classWidth(bits(BITS.wq, 5)), m.w, 0.051, 'width');
    assert.ok(Number.isInteger(m.shape) && m.shape >= 0 && m.shape < SHAPE_LISTS[m.kind].length,
      `kind ${m.kind} starts at ${m.shape} of its ${SHAPE_LISTS[m.kind].length} shapes`);
  }
  assert.equal(classWidth(widthClass(-1)), -1);
  assert.equal(classWidth(widthClass(1)), 1);
});

ok('the bodies cycle the sculpted shapes of their kind: crystal, monolith, ring, lantern, obelisk, octahedron', () => {
  assert.deepEqual(JSON.parse(JSON.stringify(SHAPE_LISTS)),
    { 0: [0, 5], 1: [5], 2: [3], 3: [0, 1, 4, 2, 3, 5], 4: [0], 5: [1, 4, 0, 2] });
  const seen = new Set();
  for (const seed of [1, 2, 3, 4, 5, 6]) {
    for (const m of placeBeacons({ seed, cells: cellsAt(-600), terrain: hills })) {
      seen.add(`${m.kind}:${m.shape}`);
    }
  }
  assert.ok([...seen].some((k) => k.startsWith('3:') && k !== '3:0'), 'the obelisks start at other shapes');
});

ok('the stone rings lean out and back over 8 bars each way, continuous at every flip', () => {
  for (const calm of [false, true]) {
    let was = unfold(0, calm);
    assert.equal(was, 0, 'upright at the start');
    let top = 0;
    for (let beat = 0.01; beat <= 64 * 3; beat += 0.01) {
      const v = unfold(beat, calm);
      assert.ok(Math.abs(v - was) < 0.02, `a step of ${Math.abs(v - was)} at beat ${beat}`);
      assert.ok(v >= 0 && v <= 1);
      top = Math.max(top, v);
      was = v;
    }
    assert.equal(top, 1, 'it reaches out');
  }
  for (let k = 1; k < 6; k++) {
    near(unfold(32 * k - 1e-6), unfold(32 * k + 1e-6), 1e-4, `across beat ${32 * k}`);
  }
  assert.equal(unfold(31.99), 0);
  assert.equal(unfold(32 + UNFOLD.ease + 1), 1, 'out for the rest of the 2nd 8 bars');
  assert.equal(unfold(64 + UNFOLD.ease + 1), 0, 'and back for the next 8');
  assert.ok(unfold(33, true) < unfold(33, false), 'Calm eases slower');
});

ok('a world draws four of the six kinds, the first one the sculpted four, a cluster always near', () => {
  assert.deepEqual(worldKinds(5, 0), [3, 5, 0, 2]);
  const sets = new Set();
  for (const seed of [1, 2, 3, 4, 5, 6, 7, 8]) {
    const ks = worldKinds(seed, 1);
    assert.equal(new Set(ks).size, 4);
    assert.ok(ks.every((k) => k >= 0 && k <= 5));
    sets.add([...ks].sort().join());
    for (const m of placeBeacons({ seed, cells: cellsAt(0), terrain: hills, kinds: ks })) {
      assert.ok(ks.includes(m.kind), `seed ${seed}: kind ${m.kind} is not in ${ks}`);
    }
  }
  assert.ok(sets.size > 1, 'the worlds differ');
  for (const seed of [1, 2, 3, 4, 5, 6, 7, 8]) {
    const kinds = worldKinds(seed, 0);
    const members = placeBeacons({
      seed, cells: cellsAt(0), terrain: hills, kinds, near: { z: -105, kind: kinds[0] },
    });
    const there = members.filter((m) => m.kind === kinds[0] && Math.abs(m.cz + 105) < 15);
    assert.ok(there.length > 0, `seed ${seed}: a ${kinds[0]} cluster stands at -105`);
  }
});

ok('a world\'s kinds are pure in (seed, world): world 0 sculpted whatever was built before', () => {
  const first = new Map();
  for (const seed of [11, 12, 13, 14, 15]) first.set(seed, [worldKinds(seed, 0), worldKinds(seed, 3)]);
  for (const seed of [15, 14, 13, 12, 11]) {
    placeBeacons({ seed: seed + 100, cells: cellsAt(0), terrain: hills });
    assert.deepEqual(worldKinds(seed, 0), [3, 5, 0, 2], `seed ${seed}`);
    assert.deepEqual([worldKinds(seed, 0), worldKinds(seed, 3)], first.get(seed));
  }
  assert.ok([11, 12, 13, 14, 15].some((s) => worldKinds(s, 3).join() !== worldKinds(s, 4).join()));
});

ok('the TSL unfold formula agrees with the JS unfold at sample beats, Calm eased not switched', () => {
  const sm = (v) => { const t = Math.min(1, Math.max(0, v)); return t * t * (3 - 2 * t); };
  // the nodes of beacons.js unfoldNode, term for term, with the exported constants
  const tsl = (bt, calm) => {
    const k = Math.floor(bt / UNFOLD.half);
    const f = (bt - k * UNFOLD.half) / (UNFOLD.ease + (UNFOLD.easeCalm - UNFOLD.ease) * calm);
    const was = k > 0.5 ? (k + 1) % 2 : 0;
    return was + ((k % 2) - was) * sm(f);
  };
  for (const calm of [0, 0.3, 1]) {
    for (let b = 0; b < 140; b += 0.37) near(tsl(b, calm), unfold(b, calm), 1e-9, `beat ${b} calm ${calm}`);
  }
  near(calmness(0.2), 1, 1e-9); near(calmness(0.8), 0, 1e-9); near(calmness(0.5), 0.5, 1e-9);
  // am crossing .5 inside an ease: the unfold moves under .1 per .01 of am (a switch would jump ~.5)
  let was = unfold(34, calmness(0.7));
  for (let am = 0.7; am > 0.3; am -= 0.01) {
    const now = unfold(34, calmness(am));
    assert.ok(Math.abs(now - was) < 0.1, `am ${am.toFixed(2)} steps ${now - was}`);
    was = now;
  }
});

ok('the static arrays: a slot per member, kind 7 where nothing stands, in two arrays', () => {
  const members = placeBeacons({ seed: 7, cells: cellsAt(0), terrain: hills });
  const packed = packStatic(members, 4);
  assert.deepEqual(Object.keys(packed).sort(), ['a', 'b']);
  assert.equal(packed.a.length, BEACON.members * 4 * 4);
  const m = members[1];
  const o = (1 * 4 + 2) * 4;
  assert.deepEqual(Array.from(packed.a.slice(o, o + 3)).map((v) => Math.fround(v)),
    [m.x, m.y, m.z].map(Math.fround));
  assert.equal(packed.b[o], packInts(m));
  const empty = (members.length * 4) * 4;
  assert.equal(packed.b[empty], 7);
});

// ------------------------------------------------------------------ the particles
ok('the particle counts follow the budget\'s share and never pass the caps', () => {
  const caps = { maxParticles: 40000 };
  const full = countsOf(1, caps);
  assert.deepEqual(full, { eruptions: POOLS.eruptions, sparks: POOLS.sparks });
  let q = 1;
  for (let step = 0; step < 6; step++) {
    q *= 0.78;
    const c = countsOf(q, caps);
    assert.ok(c.eruptions < full.eruptions && c.sparks < full.sparks);
    near(c.eruptions, Math.ceil(POOLS.eruptions * q), 0, 'eruptions');
  }
  assert.ok(countsOf(0.3, caps).eruptions >= 1);
  assert.deepEqual(countsOf(1, { maxParticles: 100 }), { eruptions: 100, sparks: 100 });
  assert.ok(countsOf(5, caps).eruptions <= POOLS.eruptions);
});

ok('the heights table is the terrain\'s on its grid, row by row, around the eruption', () => {
  const at = { x: 12, z: -40 };
  const table = heightsOf(hills, at);
  const cells = BEACON.tableCells;
  assert.equal(table.length, cells * cells);
  const x0 = at.x - (cells * BEACON.tableStep) / 2;
  const z0 = at.z - (cells * BEACON.tableStep) / 2;
  for (const [ix, iz] of [[0, 0], [10, 3], [48, 48], [95, 95], [7, 90]]) {
    near(table[iz * cells + ix], hills.height(x0 + ix, z0 + iz), 1e-5, `cell ${ix},${iz}`);
  }
});

ok('the eruption\'s bilinear read of that table is within 0.05 of the terrain between cells', () => {
  const at = { x: 0, z: 0 };
  const table = heightsOf(hills, at);
  const cells = BEACON.tableCells;
  const read = (x, z) => {
    const gx = Math.min(cells - 1.001, Math.max(0, x / BEACON.tableStep + cells / 2));
    const gz = Math.min(cells - 1.001, Math.max(0, z / BEACON.tableStep + cells / 2));
    const ix = Math.floor(gx);
    const iz = Math.floor(gz);
    const fx = gx - ix;
    const fz = gz - iz;
    const h = (a, b) => table[(iz + b) * cells + ix + a];
    return (h(0, 0) * (1 - fx) + h(1, 0) * fx) * (1 - fz) + (h(0, 1) * (1 - fx) + h(1, 1) * fx) * fz;
  };
  let worst = 0;
  for (let x = -40; x < 40; x += 3.7) for (let z = -40; z < 40; z += 2.9) worst = Math.max(worst, Math.abs(read(x, z) - hills.height(x, z)));
  assert.ok(worst < 0.05, `worst ${worst}`);
});

ok('a drop replayed through the real limiter: every peak is granted, eruptions happen, grants stay within the allowance', () => {
  for (const rate of RATES) {
    const limiter = new FlashLimiter();
    const accents = createAccents();
    const dt = 1 / rate;
    const steps = 20 * rate;
    let peaks = 0, granted = 0, eruptions = 0, spawnAt = -Infinity;
    let nextBeat = 0, nextHat = 0, nextPeak = 0.9, beat = 0;
    for (let i = 0; i < steps; i++) {
      const t = i * dt;
      const events = [];
      if (t >= nextBeat) { events.push({ type: 'beat', beat: beat++ }, { type: 'kick' }); nextBeat += 0.5; }
      if (t >= nextHat) { events.push({ type: 'hat', amp: 0.8 }); nextHat += 0.25; }
      // the director's own peak: it grants (or refuses) at the emission, on the shared limiter
      if (t >= nextPeak) {
        peaks++;
        const flash = limiter.grant(t);
        if (flash) granted++;
        events.push({ type: 'peak', flash });
        nextPeak += 2;
      }
      const ask = (at) => limiter.grant(at);
      for (const e of events) {
        const gapOk = t - spawnAt > POOLS.eruptGapS;
        if (erupts(e, { section: 'drop', gapOk, ask: () => ask(t) })) { spawnAt = t; eruptions++; }
      }
      accents.onEvents(events, { section: 'drop', bpm: 120, t, flash: ask });
    }
    assert.equal(peaks, 10, `${rate}/s`);
    assert.equal(granted, 10, `${rate}/s: every peak is granted`);
    assert.ok(eruptions > 0, `${rate}/s: eruptions`);
  }
});

ok('the accents never ask the limiter on a peak or a drop entry the director already decided', () => {
  for (const flash of [true, false]) {
    for (const e of [{ type: 'peak', flash }, { type: 'section', section: 'drop', flash }]) {
      const accents = createAccents();
      let asks = 0;
      accents.onEvents([e], { section: 'drop', bpm: 120, t: 1, flash: () => { asks++; return true; } });
      assert.equal(asks, 0, `${e.type} flash ${flash}`);
    }
  }
});

// ------------------------------------------------------------------ the files
ok('the fx files hold no Math.random, Date.now or Points object, and the beacons ask the context for the ground', () => {
  for (const f of ['layers/beacons.js', 'layers/post.js', 'tsl/post.js']) {
    const code = readFileSync(new URL(`../../../bins/svolderd/web/themes/voidgarden/${f}`, import.meta.url), 'utf8')
      .replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
    assert.ok(!/Math\.random|Date\.now|\bperformance\./.test(code), f);
    if (f === 'tsl/post.js') assert.ok(!/\bu\.fade\b/.test(code), 'the chain applies no fade: each material does');
    assert.ok(!/\b(?:THREE|T)\.Points\b|new\s+Points\s*\(/.test(code), f);
  }
  const beacons = readFileSync(new URL('../../../bins/svolderd/web/themes/voidgarden/layers/beacons.js', import.meta.url), 'utf8');
  assert.ok(/ctx\.terrain\s*\?\?\s*FLAT/.test(beacons), 'the ground is ctx.terrain with the flat fallback');
});

console.log(`voidgarden-fx.mjs: ${n} passed`);
