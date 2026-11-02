/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for smoke-director.js: the smoke's rules without a GPU, a DOM or a clock. A fake
// clock and a seeded random drive the director; the plans it answers are what is checked.
// Run: node director.mjs
import assert from 'node:assert/strict';
import { Director } from '../../../bins/svolderd/web/themes/ember/smoke-director.js';
import {
  TUNING, VolumeBurst, bigRise, seeded, dialDot, uvOf,
} from '../../../bins/svolderd/web/themes/ember/smoke-model.js';
import { lookTests } from './look-values.mjs';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const okLook = lookTests(TUNING, ok);
const SIZE = [1280, 900];
const FRAME = 1 / 60;
const ANCHOR = {
  x: 640, y: 450, radius: 60, meters: [{ x: 400, y: 600 }, { x: 880, y: 600 }],
  column: { left: 300, right: 980 },
};

/** A director on a seeded random at time 0, with a clock that `tick` advances by one 60 Hz frame
 * and a log of every plan; `stir` makes a click, which arms a settle. */
const world = (seed = 1, options = {}) => {
  const w = {
    t: 0, plans: [], d: new Director({ random: seeded(seed), now: 0, ...options }),
  };
  w.tick = (events) => {
    w.t += FRAME;
    if (events) events(w.d, w.t);
    const plan = w.d.frame(w.t, SIZE);
    w.plans.push(plan);
    return plan;
  };
  w.run = (secs, events) => {
    const out = [];
    for (let i = 0; i < Math.round(secs / FRAME); i++) out.push(w.tick(events));
    return out;
  };
  return w;
};
const count = (plans, action) => plans.filter((p) => p.action === action).length;
const finite = (v, path = 'plan') => {
  if (typeof v === 'number') assert.ok(Number.isFinite(v), `${path} is ${v}`);
  else if (v && typeof v === 'object') {
    for (const [k, x] of Object.entries(v)) finite(x, `${path}.${k}`);
  }
};

okLook('rhythm: calm at 20 fps untouched, stirred after a pointer, stop when away has faded, '
  + 'nothing while hidden', () => {
  const w = world();
  const calm = w.run(60);
  const runs = count(calm, 'run');
  assert.ok(runs >= 1190 && runs <= 1210, `calm runs ${runs}`);
  assert.ok(calm.every((p) => p.dt <= TUNING.maxDt + 1e-12 && p.dt >= 0));
  assert.equal(count(calm, 'stop'), 0);
  // a pointer stroke: every frame simulates for stirredSecs after the last splat
  w.d.pointer(1, { x: 100, y: 100, t: w.t }, true, SIZE, w.t);
  w.t += FRAME;
  assert.equal(w.d.pointer(1, { x: 160, y: 120, t: w.t }, true, SIZE, w.t), true);
  w.plans.length = 0;
  const stirred = w.run(1.9);
  assert.equal(count(stirred, 'run'), stirred.length);
  assert.ok(stirred[0].splats.some((s) => s.kind === 'pointer'));
  const calmAgain = w.run(6).slice(-120);
  assert.ok(count(calmAgain, 'run') < 60, 'back to calm');
  // each kind of event wakes every frame at once, whatever the calm rhythm's phase
  const events = {
    pointer: (d, t) => { d.pointer(2, { x: 10, y: 10, t: t - FRAME }, true, SIZE, t);
      d.pointer(2, { x: 80, y: 40, t }, true, SIZE, t); },
    scroll: (d, t) => d.scroll(0, 40, t),
    click: (d, t) => d.click(300, 300, t),
    step: (d, t) => d.step(1, 0.1, 0.3, false, t),
  };
  for (const [name, fire] of Object.entries(events)) {
    for (let phase = 0; phase < 3; phase++) {
      const v = world(20 + phase);
      v.d.anchor(ANCHOR);
      v.run(1 + phase * FRAME);
      v.t += FRAME;
      fire(v.d, v.t);
      assert.equal(v.d.frame(v.t, SIZE).action, 'run', `${name} wakes the next frame (${phase})`);
    }
  }
  // a meter in its red zone stirs; one below it is calm
  const red = world(23);
  red.d.anchor(ANCHOR);
  red.run(1);
  red.d.meters([1, 1]);
  red.run(2);
  assert.equal(count(red.run(2), 'run'), 120, 'a red meter runs every frame');
  const green = world(24);
  green.d.anchor(ANCHOR);
  green.d.meters([0.1, 0.1]);
  assert.ok(count(green.run(5).slice(-120), 'run') < 60, 'a quiet meter stays calm');
  // away: simulates through the fade, then stops, drawing once
  w.d.away(true, w.t);
  const faded = w.run(TUNING.fadeSecs + 2);
  const firstStop = faded.findIndex((p) => p.action === 'stop');
  assert.ok(firstStop > 0);
  assert.ok(faded.slice(0, firstStop).every((p) => p.action !== 'stop'));
  assert.ok(faded.slice(0, firstStop).filter((p) => p.action === 'run').every((p) => p.away));
  assert.ok(faded.slice(firstStop).every((p) => p.action === 'stop'));
  assert.equal(faded[firstStop].redraw, true);
  assert.equal(faded[firstStop].draw.sat, 0);
  assert.equal(faded[firstStop].draw.bright, TUNING.awayOpacity);
  assert.ok(faded.slice(firstStop + 1).every((p) => !p.redraw));
  assert.equal(w.d.wakeAt(), null);
  // back: runs again
  w.d.away(false, w.t);
  w.d.wake(w.t);
  assert.equal(w.tick().action, 'run');
  // hidden: nothing, whatever the events
  w.d.visible(false, w.t);
  w.d.level(1);
  const hidden = w.run(5);
  assert.ok(hidden.every((p) => p.action === 'stop' && !p.redraw && p.splats.length === 0));
  assert.equal(w.d.idle(), true);
  assert.equal(w.d.wakeAt(), null);
});

okLook('haze: fed every simulated frame in proportion to dt; a 3600 s hidden gap owes nothing on '
  + 'return', () => {
  const w = world();
  const first = w.tick();
  assert.ok(first.splats.filter((s) => s.rest && !s.kind).length > 0, 'the first frame feeds');
  // every simulated frame feeds the haze, its dye in proportion to the frame's own step: no
  // pulse every 0.25 s
  // (a click in the middle makes frames of two lengths: 1/60 s stirred, 1/20 s calm)
  w.run(3);
  w.d.click(300, 300, w.t);
  const ran = w.run(7).filter((p) => p.action === 'run');
  assert.ok(new Set(ran.map((p) => p.dt.toFixed(3))).size >= 2, 'frames of two lengths');
  assert.ok(ran.length > 100, `${ran.length} frames`);
  const sources = first.splats.filter((s) => s.rest && !s.kind).length;
  for (const p of ran) {
    const fed = p.splats.filter((s) => s.rest && !s.kind && s.dye && s.dye[3] > 0);
    assert.equal(fed.length, sources, `a frame of dt ${p.dt} fed ${fed.length} sources`);
  }
  const amountPerSecond = (p) => p.splats.filter((s) => s.rest && !s.kind && s.dye)
    .reduce((t, s) => t + s.dye[3], 0) / p.dt;
  const rates = ran.map(amountPerSecond);
  assert.ok(Math.max(...rates) < 1.1 * Math.min(...rates), 'the same dye per second each frame');
  // an hour hidden: the first frame back simulates at most maxDt and owes nothing
  w.d.visible(false, w.t);
  w.t += 3600;
  w.d.visible(true, w.t);
  w.d.wake(w.t);
  const back = w.tick();
  assert.equal(back.action, 'run');
  assert.ok(back.dt <= TUNING.maxDt && back.dt > 0, `dt ${back.dt}`);
  assert.ok(back.splats.length > 0, 'the haze feeds the frame back, one step only');
  // the same without the page's help: a frame an hour late is capped
  const late = world();
  late.run(1);
  late.t += 3600;
  const plan = late.tick();
  assert.ok(plan.dt <= TUNING.maxDt, `late dt ${plan.dt}`);
  // a settle pending when the page hid survives it: it rolls on return, once
  const n2 = world(2, { roll: 0 });
  n2.run(0.2);
  assert.equal(n2.d.click(300, 300, n2.t), true);
  n2.tick(); // the stir is seen and the settle armed
  n2.d.visible(false, n2.t);
  n2.t += 3600;
  n2.d.visible(true, n2.t);
  n2.d.wake(n2.t);
  const after = n2.run(2);
  assert.equal(n2.d.stats(n2.t).notes.rolls, 1);
  assert.equal(after.filter((p) => p.note).length, 1);
  // haze off: no feeding at all
  const off = world();
  off.d.haze(false);
  assert.equal(off.run(5).filter((p) => p.splats.length > 0).length, 0);
  assert.equal(off.d.haze(true), false);
});

ok('scroll: a burst stirs, then nothing after it falls under scrollFloor', () => {
  const w = world();
  w.run(1);
  const burst = w.run(1, (d, t) => d.scroll(0, 50, t));
  const per = TUNING.scrollSplatRows * TUNING.scrollSplatCols;
  const pushes = burst.filter((p) => p.splats.some((s) => s.kind === 'scroll'));
  assert.ok(pushes.length > 30, `scroll frames ${pushes.length}`);
  assert.ok(pushes.every((p) => p.splats.filter((s) => s.kind === 'scroll').length === per));
  assert.ok(pushes.every((p) => p.splats.filter((s) => s.kind === 'scroll')
    .every((s) => s.pull[1] > 0)));
  assert.ok(burst.filter((p) => p.shift).length > 30, 'the grids move with the page');
  const sum = burst.reduce((a, p) => a + (p.shift ? p.shift[1] : 0), 0);
  assert.ok(Math.abs(sum - (50 * burst.length) / SIZE[1]) < 1e-6 * burst.length);
  assert.ok(w.d.stats().splats.scroll === pushes.length * per);
  const after = w.run(3);
  const last = after.map((p) => p.splats.some((s) => s.kind === 'scroll'));
  const lastOn = last.lastIndexOf(true);
  assert.ok(lastOn < after.length / 2, 'the push ends soon after the burst');
  assert.ok(after.slice(lastOn + 1).every((p) => p.splats.every((s) => s.kind !== 'scroll')));
  assert.equal(w.d.scrollV, 0);
});

okLook(
  'settle: one place per episode; highest odds win; reset on away and lost; kept on hide', () => {
  // two clicks of one episode: one roll, at the place of the larger stir (the later one)
  const w = world(3, { roll: 0 });
  w.run(0.3);
  w.d.click(100, 100, w.t);
  w.run(0.2);
  w.d.click(900, 600, w.t);
  const notes = w.run(8).filter((p) => p.note);
  assert.equal(notes.length, 1);
  assert.equal(w.d.stats(w.t).notes.rolls, 1);
  assert.ok(Math.abs(notes[0].note.uv.x - 900 / SIZE[0]) < 0.3, 'near the second click');
  assert.equal(w.d.stats(w.t).notes.stamped, 1);
  // odds: a roll of 0.5 loses at the click's 1/2 and a step's 1/3, wins at a big rise's 2/3
  const odds = (events) => {
    const o = world(4, { roll: 0.5 });
    o.d.anchor(ANCHOR);
    o.run(0.3);
    events(o.d, o.t);
    o.run(8);
    return o.d.stats(o.t).notes;
  };
  const click = odds((d, t) => d.click(300, 300, t));
  assert.deepEqual([click.rolls, click.wins], [1, 0]);
  const small = odds((d, t) => d.step(1, 0.2, 0.5, false, t));
  assert.deepEqual([small.rolls, small.wins], [1, 0]);
  const big = odds((d, t) => d.step(1, 0.2, 0.5, true, t));
  assert.deepEqual([big.rolls, big.wins], [1, 1]);
  const both = odds((d, t) => { d.click(300, 300, t); d.step(1, 0.2, 0.5, true, t); });
  assert.deepEqual([both.rolls, both.wins], [1, 1]);
  // reset: away and lost forget the pending episode; hiding and haze off keep it
  const forgets = (events) => {
    const o = world(5, { roll: 0 });
    o.run(0.3);
    o.d.click(300, 300, o.t);
    events(o);
    o.d.wake(o.t);
    o.run(10);
    return o.d.stats(o.t).notes.rolls;
  };
  assert.equal(forgets((o) => { o.d.away(true, o.t); o.d.away(false, o.t); }), 0);
  assert.equal(forgets((o) => { o.d.lost(true); o.d.lost(false); }), 0);
  assert.equal(forgets((o) => { o.d.visible(false, o.t); o.d.visible(true, o.t); }), 1);
  assert.equal(forgets((o) => { o.d.haze(false); o.d.haze(true); }), 1);
  // no note while away, nor without an atlas
  const aw = world(6, { roll: 0 });
  aw.d.away(true, aw.t);
  aw.d.click(1, 1, aw.t);
  assert.equal(aw.d.click(1, 1, aw.t), false);
  const plain = world(7, { roll: 0, notes: false });
  plain.run(0.3);
  plain.d.click(300, 300, plain.t);
  assert.equal(plain.run(8).filter((p) => p.note).length, 0);
  assert.equal(plain.d.stats(plain.t).notes.rolls, 0);
});

okLook('gate: 4 s gap and a cap of 2 across settle and click notes', () => {
  const w = world(8, { roll: 0 });
  w.d.anchor(ANCHOR);
  let k = 0;
  const plans = w.run(240, (d, t) => {
    // an episode every 2.5 s, alternating a click and a step
    if (Math.floor(t / 2.5) !== Math.floor((t - FRAME) / 2.5)) {
      if (k++ % 2) d.click(200 + 40 * k, 300, t);
      else d.step(1, 0.2, 0.5, false, t);
    }
  });
  const times = plans.map((p, i) => (p.note ? (i + 1) * FRAME : null)).filter((x) => x !== null);
  const stats = w.d.stats(w.t).notes;
  assert.ok(times.length >= 10, `notes ${times.length}`);
  assert.ok(stats.rolls > times.length, 'some settles were refused by the gate');
  for (let i = 1; i < times.length; i++) {
    const gap = times[i] - times[i - 1];
    assert.ok(gap >= TUNING.noteCooldown - 1e-9, `gap ${gap}`);
  }
  for (let i = 0; i + TUNING.noteCap < times.length; i++) {
    assert.ok(times[i + TUNING.noteCap] - times[i] >= TUNING.noteLife - 1e-9, 'cap in a life');
  }
  assert.equal(stats.stamped, times.length);
  assert.equal(stats.last.at > 0, true);
  assert.ok(stats.live <= TUNING.noteCap);
});

okLook('bands: a column change starts the band fade for hazeFadeSecs', () => {
  const w = world();
  w.d.anchor(ANCHOR);
  const calm = w.run(1);
  assert.ok(calm.every((p) => p.bandFade === null));
  const at = w.t;
  w.d.anchor({ ...ANCHOR, column: { left: 100, right: 1180 } });
  const after = w.run(2.5);
  const faded = after.map((p, i) => (p.bandFade ? i : -1)).filter((i) => i >= 0);
  assert.ok(faded.length > 0, 'a fade starts');
  const first = after[faded[0]];
  const { lo, hi } = first.bandFade;
  assert.ok(lo >= 0 && hi <= 1 && lo < hi);
  near(first.bandFade.keep, Math.exp(-TUNING.hazeFadeRate * first.dt));
  const span = (faded.length - 1) * FRAME * 3; // calm frames are every third callback
  assert.ok(span <= TUNING.hazeFadeSecs + 0.1 && span >= TUNING.hazeFadeSecs - 0.2,
    `fade lasts ${span} s from ${at}`);
  assert.ok(after.slice(faded.at(-1) + 1).every((p) => p.bandFade === null));
  // a change under hazeFadeShare starts nothing
  const quiet = world();
  quiet.d.anchor(ANCHOR);
  quiet.run(1);
  quiet.d.anchor({ ...ANCHOR, column: { left: 302, right: 978 } });
  assert.ok(quiet.run(2).every((p) => p.bandFade === null));
});

function near(a, b, e = 1e-9) { assert.ok(Math.abs(a - b) <= e, `${a} vs ${b}`); }

okLook('step: a burst over 10 dB is big; a dB dial ignores the share rule', () => {
  const dbs = (steps) => {
    const b = new VolumeBurst();
    let out = null;
    steps.forEach(([from, to, share], i) => { out = b.add(from, to, share, i * 0.1); });
    return bigRise(out);
  };
  assert.equal(dbs([[-30, -26, 0.05], [-26, -22, 0.05], [-22, -18.5, 0.05]]), true);
  assert.equal(dbs([[-30, -26, 0.05], [-26, -22, 0.05], [-22, -20, 0.05]]), false);
  // a dial with dB readings: a large share does not make a small dB rise big
  assert.equal(dbs([[-30, -29.5, 0.4], [-29.5, -29, 0.4]]), false);
  // without dB readings the share decides
  assert.equal(dbs([[null, null, 0.07], [null, null, 0.07]]), true);
  assert.equal(dbs([[null, null, 0.05], [null, null, 0.05]]), false);
  // a step down ends the burst
  assert.equal(dbs([[-30, -20, 0.2], [-20, -25, -0.1], [-25, -24, 0.01]]), false);
  assert.equal(bigRise({ enteredRed: true }), true);
  // the director: a big step has the big odds, a step has the step's
  const odds = (big) => {
    const w = world(9, { roll: 0.5 });
    w.d.anchor(ANCHOR);
    w.run(0.3);
    assert.equal(w.d.step(1, 0.1, 0.3, big, w.t), true);
    w.run(8);
    return w.d.stats(w.t).notes.wins;
  };
  assert.equal(odds(false), 0);
  assert.equal(odds(true), 1);
  // a step puffs at the dial's dot, at most maxPuffs wait, none while away or without a dial
  const p = world(10);
  assert.equal(p.d.step(1, 0.1, 0.3, false, 0), false, 'no anchor, no puff');
  p.d.anchor(ANCHOR);
  for (let i = 0; i < TUNING.maxPuffs + 5; i++) p.d.step(1, 0.1, 0.3, false, 0);
  const plan = p.tick();
  assert.equal(plan.splats.filter((s) => s.kind === 'puff').length, TUNING.maxPuffs);
  p.d.away(true, p.t);
  assert.equal(p.d.step(1, 0.1, 0.3, false, p.t), false);
});

ok('seed: the same seed gives identical plans for 600 frames of the same events', () => {
  const script = (d, t) => {
    const i = Math.round(t / FRAME);
    if (i === 1) { d.anchor(ANCHOR); d.level(0.6); }
    if (i % 90 === 10) d.click(200 + (i % 700), 300, t);
    if (i % 70 === 20) d.step(i % 2 ? 1 : -1, 0.15, 0.4, i % 3 === 0, t);
    if (i % 5 === 0) d.pointer(1, { x: 100 + i, y: 200 + (i % 50), t }, i % 2 === 0, SIZE, t);
    if (i > 200 && i < 260) d.scroll(0, 30, t);
    if (i % 30 === 0) d.meters([(i % 7) / 7, (i % 5) / 5]);
    if (i === 400) d.column({ left: 100, right: 1100 });
    if (i === 450) d.away(true, t);
    if (i === 520) d.away(false, t);
  };
  const play = (seed) => {
    const w = world(seed, { roll: undefined });
    w.run(10, script);
    return { plans: JSON.stringify(w.plans), stats: JSON.stringify(w.d.stats(w.t)) };
  };
  const a = play(11);
  const b = play(11);
  assert.equal(a.plans, b.plans);
  assert.equal(a.stats, b.stats);
  assert.equal(JSON.parse(a.plans).length, 600);
  assert.notEqual(a.plans, play(12).plans);
  // the draws are the director's alone: a shared random sequence is consumed identically
  const draws = (seed) => {
    let k = 0;
    const r = seeded(seed);
    const w = { t: 0, d: new Director({ random: () => { k++; return r(); }, now: 0 }) };
    for (let i = 1; i <= 120; i++) { w.t += FRAME; script(w.d, w.t); w.d.frame(w.t, SIZE); }
    return k;
  };
  assert.equal(draws(5), draws(5));
});

ok('bad input: NaN, Infinity, a zero or huge size or dt give finite plans', () => {
  const bad = [NaN, Infinity, -Infinity, 0, -1, 1e300, 1e-300, undefined, null, 'x'];
  const w = world(13);
  w.run(0.5);
  for (const v of bad) {
    w.d.level(v);
    w.d.step(v, v, v, false, v);
    w.d.click(v, v, v);
    w.d.pointer(1, { x: v, y: v, t: v }, true, SIZE, v);
    w.d.pointer(2, { x: 1, y: 1, t: 1 }, true, [v, v], v);
    w.d.touchStart(3, { x: v, y: v, t: v });
    w.d.scroll(v, v, v);
    w.d.meters([v, v]);
    w.d.meters(v);
    w.d.anchor({ x: v, y: v, radius: v, meters: [{ x: v, y: v }, null, v], column: v });
    w.d.column({ left: v, right: v });
    w.d.roll(v);
    w.d.visible(true, v);
    w.d.wake(v);
    for (const size of [[v, v], [v, 900], [1280, v], [0, 0], [1e300, 1e300], [1, 1], [1e7, 1e7]]) {
      for (const now of [w.t + 0.02, NaN, Infinity, 1e15]) {
        finite(w.d.frame(now, size), `frame(${now}, ${size})`);
      }
    }
    finite(w.d.stats(v));
    w.d.wakeAt();
  }
  // a valid canvas afterwards still plans
  w.d.anchor(ANCHOR);
  w.d.level(0.5);
  w.d.roll(null);
  w.d.visible(true, w.t);
  w.t = 1e15;
  const plan = w.d.frame(w.t, SIZE);
  finite(plan);
  assert.ok(plan.dt <= TUNING.maxDt);
  // tiny and huge dt between frames
  const t = world(14);
  t.t = 1e-12;
  finite(t.d.frame(t.t, SIZE));
  t.t = 1e6;
  const p = t.d.frame(t.t, SIZE);
  finite(p);
  assert.ok(p.dt <= TUNING.maxDt);
  // an unusable canvas never simulates
  assert.equal(t.d.frame(t.t + 1, [0, 900]).action, 'stop');
});


/** The pointer and scroll events the checks below replay: a 2 short sides a second pointer move
 * for half a second, and a fast scroll of 3000 px in half a second. */
const wave = (o) => {
  const t0 = o.t;
  o.run(0.5, (d, t) => d.pointer(3, { x: 100 + 1800 * (t - t0), y: 500, t }, false, SIZE, t));
};
const fastScroll = (o) => {
  let i = 0;
  o.run(0.5, (d, t) => { if (i++ % 3 === 0) d.scroll(0, 300, t); });
};
/** [rolls, wins] after `events` with every roll forced to `r`, 8 s on. */
const forced = (r, events, seed = 30) => {
  const o = world(seed, { roll: r });
  o.d.anchor(ANCHOR);
  o.run(0.3);
  events(o);
  o.run(8);
  const s = o.d.stats(o.t).notes;
  return [s.rolls, s.wins];
};

okLook(
  'odds: each source settles once, at its own odds (the roll that wins tells them apart)', () => {
  // a mouse stir 1/3, a click 1/2, a one-step change 1/3, a 20 dB rise 2/3, a fast scroll 1/3
  assert.deepEqual([forced(0.30, wave), forced(0.40, wave)], [[1, 1], [1, 0]], 'mouse');
  const click = (o) => o.d.click(600, 300, o.t);
  assert.deepEqual([forced(0.40, click), forced(0.55, click)], [[1, 1], [1, 0]], 'click');
  const step = (o) => o.d.step(1, 0.1, 0.3, false, o.t);
  assert.deepEqual([forced(0.30, step), forced(0.40, step)], [[1, 1], [1, 0]], 'step');
  const rise = (o) => o.d.step(1, 0.1, 0.3, true, o.t);
  assert.deepEqual([forced(0.55, rise), forced(0.70, rise)], [[1, 1], [1, 0]], 'rise');
  assert.deepEqual([forced(0.30, fastScroll), forced(0.40, fastScroll)], [[1, 1], [1, 0]],
    'scroll');
});

ok('idle: 600 s of an untouched page, the haze feeding, rolls no settle and forms no note', () => {
  const w = world(31, { roll: 0 });
  w.d.anchor(ANCHOR);
  w.run(600);
  const s = w.d.stats(w.t).notes;
  assert.deepEqual([s.rolls, s.stamped], [0, 0]);
  const quiet = world(32, { roll: 0 });
  quiet.d.anchor(ANCHOR);
  quiet.d.meters([0.5, 0.5]);
  quiet.run(30);
  assert.deepEqual([quiet.d.stats(quiet.t).notes.rolls, quiet.d.stats(quiet.t).notes.stamped],
    [0, 0], 'meters held at 0.5 for 30 s');
});

ok('away and lost: nothing that happens then rolls a settle or forms a note', () => {
  const after = (go, back) => {
    const o = world(33, { roll: 0 });
    o.d.anchor(ANCHOR);
    o.run(0.5);
    go(o);
    wave(o);
    o.run(0.2);
    o.d.step(1, 0.3, 0.7, true, o.t);
    o.d.click(300, 300, o.t);
    o.d.scroll(0, 400, o.t);
    o.run(2);
    back(o);
    o.run(8);
    const s = o.d.stats(o.t).notes;
    return [s.rolls, s.stamped];
  };
  assert.deepEqual(after((o) => o.d.away(true, o.t), (o) => o.d.away(false, o.t)), [0, 0]);
  assert.deepEqual(after((o) => o.d.lost(true), (o) => o.d.lost(false)), [0, 0], 'lost');
  // a puff queued just before the spell is not drawn when it ends
  const puffed = world(38);
  puffed.d.anchor(ANCHOR);
  puffed.run(0.3);
  puffed.d.click(300, 300, puffed.t);
  puffed.d.away(true, puffed.t);
  puffed.run(0.5);
  puffed.d.away(false, puffed.t);
  puffed.d.wake(puffed.t);
  assert.equal(puffed.run(1).filter((p) => p.splats.some((s) => s.kind === 'puff')).length, 0);
  // a stir pending before the spell is forgotten by it
  for (const spell of ['away', 'lost']) {
    const o = world(34, { roll: 0 });
    o.d.anchor(ANCHOR);
    o.run(0.5);
    o.d.step(1, 0.15, 0.6, false, o.t);
    if (spell === 'away') { o.d.away(true, o.t); o.d.away(false, o.t); } else {
      o.d.lost(true); o.d.lost(false);
    }
    o.run(8);
    assert.equal(o.d.stats(o.t).notes.rolls, 0, spell);
  }
});

okLook(
  'a glide of 40 small steps is one stir; twenty clicks one stir; one click every 1.3 s for 16 s '
  + 'keeps the 4 s gap and the cap', () => {
  const g = world(35, { roll: 0 });
  g.d.anchor(ANCHOR);
  g.run(0.3);
  let k = 0;
  g.run(2, (d, t) => {
    if (k < 40 && Math.round(t / FRAME) % 3 === 0) d.step(1, 0.01, 0.2 + 0.015 * k++, false, t);
  });
  g.run(7);
  const gs = g.d.stats(g.t).notes;
  assert.equal(gs.rolls, 1);
  assert.ok(gs.stamped <= 1);
  const c = world(36, { roll: 0 });
  c.d.anchor(ANCHOR);
  c.run(0.3);
  for (let i = 0; i < 20; i++) c.d.click(600, 300, c.t);
  c.run(8);
  const cs = c.d.stats(c.t).notes;
  assert.deepEqual([cs.rolls, cs.stamped], [1, 1]);
  assert.ok(cs.live <= 2);
  const s = world(37, { roll: 0 });
  s.d.anchor(ANCHOR);
  s.run(0.3);
  let worst = 0;
  s.run(16, (d, t) => {
    if (Math.round(t / FRAME) % 78 === 0) d.click(600, 300, t);
    worst = Math.max(worst, d.stats(t).notes.live);
  });
  const at = s.plans.map((p, i) => (p.note ? (i + 1) * FRAME : null)).filter((x) => x !== null);
  assert.ok(at.length >= 3 && at.length <= 4, `notes ${at.length}`);
  for (let i = 1; i < at.length; i++) {
    assert.ok(at[i] - at[i - 1] >= TUNING.noteCooldown - 1e-6, `gap ${at[i] - at[i - 1]}`);
  }
  assert.ok(worst <= TUNING.noteCap);
});

okLook(
  'meters: a music peak into red forms a note at a meter on one, two and five meters; a reading '
  + 'of 0.9 never does', () => {
  const layouts = {
    1: [{ x: 216, y: 298 }],
    2: [{ x: 441, y: 310 }, { x: 839, y: 310 }],
    5: [0, 1, 2, 3, 4].map((i) => ({ x: 200 + 200 * i, y: 310 })),
  };
  for (const [count, meters] of Object.entries(layouts)) {
    const peak = world(40 + Number(count), { roll: 0 });
    peak.d.anchor({ ...ANCHOR, meters });
    peak.run(0.5);
    peak.run(0.2, (d) => d.meters(meters.map(() => 0.99)));
    peak.d.meters(meters.map(() => 0));
    const notes = peak.run(8).filter((p) => p.note);
    assert.equal(notes.length, 1, `${count} meters: one note`);
    const n = notes[0].note;
    assert.ok(meters.some((m) => Math.hypot(n.x - m.x, n.y - m.y) < 120), `${count}: at a meter`);
    const held = world(50 + Number(count), { roll: 0 });
    held.d.anchor({ ...ANCHOR, meters });
    held.d.meters(meters.map(() => 0.9));
    held.run(3);
    held.d.meters(meters.map(() => 0));
    held.run(8);
    assert.deepEqual([held.d.stats(held.t).notes.rolls, held.d.stats(held.t).notes.stamped],
      [0, 0], `${count} meters held at 0.9`);
  }
});

okLook('phone width: a flick of 255 px in 0.10-0.15 s, or 312 px in 0.10-0.15 s, across 390 px '
  + 'settles into a note near it', () => {
  const PHONE = [390, 844];
  for (const [px, secs] of [[255, 0.10], [255, 0.15], [312, 0.10], [312, 0.15]]) {
    const o = world(60, { roll: 0 });
    o.d.anchor({ x: 195, y: 300, radius: 50, meters: [], column: null });
    const frame = () => { o.t += FRAME; return o.d.frame(o.t, PHONE); };
    for (let i = 0; i < 10; i++) frame();
    const t0 = o.t;
    o.d.pointer(1, { x: 20, y: 600, t: t0 }, false, PHONE, t0);
    const notes = [];
    for (let i = 0; i < 8 * 60; i++) {
      const rel = o.t + FRAME - t0;
      if (rel <= secs + 1e-9) {
        o.d.pointer(1, { x: 20 + px * Math.min(1, rel / secs), y: 600, t: o.t + FRAME }, false,
          PHONE, o.t + FRAME);
      }
      const p = frame();
      if (p.note) notes.push(p.note);
    }
    assert.equal(notes.length, 1, `${px} px in ${secs} s`);
    assert.ok(Math.abs(notes[0].y - 600) <= notes[0].cell, 'near the flick');
  }
});

ok('splats: a pointer move pushes along the motion, y up; a step puffs at the dial\'s dot', () => {
  const w = world(70);
  w.d.anchor(ANCHOR);
  w.run(0.3);
  w.d.pointer(1, { x: 300, y: 200, t: w.t }, false, SIZE, w.t);
  w.t += FRAME;
  w.d.pointer(1, { x: 400, y: 300, t: w.t }, false, SIZE, w.t);
  const push = w.tick().splats.find((s) => s.kind === 'pointer');
  assert.ok(push, 'a pointer splat');
  assert.ok(push.add[0] > 0 && push.add[1] < 0, `right and down on screen: ${push.add}`);
  for (const at of [0.15, 0.85]) {
    const v = world(71);
    v.d.anchor(ANCHOR);
    v.run(0.3);
    v.d.step(1, 0.1, at, false, v.t);
    const puff = v.tick().splats.find((x) => x.kind === 'puff');
    assert.ok(puff, 'a puff');
    const want = uvOf(dialDot(ANCHOR, at), SIZE);
    near(puff.x, want.x, 1e-6);
    near(puff.y, want.y, 1e-6);
  }
});

ok('palette: the dial\'s cloud takes the dial tone, the haze the two side tones, and a stir '
  + 'its own shade', () => {
  const w = world(5);
  w.d.anchor(ANCHOR);
  const { sides, dial } = w.d.shade;
  const same = (dye, c) => [0, 1, 2].every((i) => Math.abs(dye[i] / dye[3] - c[i]) < 1e-9);
  let restSeen = 0;
  const hazeSeen = [0, 0];
  for (const p of w.run(20)) {
    for (const sp of p.splats) {
      if (!sp.rest || sp.kind || !sp.dye || !(sp.dye[3] > 0)) continue;
      if (sp.pull) {
        assert.ok(same(sp.dye, dial), 'the resting source is the dial tone');
        restSeen++;
      } else {
        const k = sides.findIndex((c) => same(sp.dye, c));
        assert.ok(k >= 0, 'a haze emission is a side tone');
        hazeSeen[k]++;
      }
    }
  }
  assert.ok(restSeen > 100 && hazeSeen[0] > 100 && hazeSeen[1] > 100, `${restSeen} ${hazeSeen}`);
});

okLook(
  'note places: at the stir\'s own place when it is visible and dense, else the nearest visible, '
  + 'dense place on the whole canvas, else none (no gap spent); the dye is read once per note',
() => {
  const click = [300, 497];
  const noteFor = (options, seed = 80, at = click) => {
    const w = world(seed, { roll: 0, ...options });
    w.d.anchor(ANCHOR);
    w.run(0.3);
    w.d.click(at[0], at[1], w.t);
    return { w, note: w.run(3, null).map((p) => p.note).find(Boolean) };
  };
  assert.ok(Number.isFinite(TUNING.noteOpenHalf) && TUNING.noteDenseMin > 0
    && TUNING.noteFindStep > 0 && TUNING.noteFindStep <= 1, 'tuning');
  assert.equal(TUNING.noteFloor, undefined, 'no dye of its own');
  const dense = TUNING.noteDenseMin * 2;
  const thin = TUNING.noteDenseMin / 2;
  const { note: plain } = noteFor({});
  assert.ok(plain, 'a note with neither test');
  near(plain.x, click[0], plain.cell);
  // a control covers a 128 x 52 box centred on the click: every spot whose box touches it is shut
  const covered = (x, y, half) => Math.abs(x - click[0]) < 64 + half
    && Math.abs(y - click[1]) < 26 + half;
  const open = (x, y, half) => !covered(x, y, half);
  const lattice = (note) => {
    const step = note.cell * TUNING.noteFindStep;
    const out = [];
    for (let j = -40; j <= 40; j++) {
      for (let i = -60; i <= 60; i++) {
        const p = { x: plain.x + i * step, y: plain.y + j * step };
        const r = note.cell * 0.71;
        if (p.x >= r && p.x <= SIZE[0] - r && p.y >= r && p.y <= SIZE[1] - r) out.push(p);
      }
    }
    return out;
  };
  const dist = (p, q) => Math.hypot(p.x - q.x, p.y - q.y);
  // 1. visible and dense at the stir's own place: it stays, whatever is denser elsewhere
  let reads = 0;
  const askedDense = [];
  const askedOpen = [];
  const { note: here } = noteFor({
    open: (x, y, half) => { askedOpen.push(half); return true; },
    density: () => {
      reads++;
      return (x, y, half) => { askedDense.push(half); return x > 900 ? 10 * dense : dense; };
    },
  });
  assert.ok(here, 'a note');
  near(here.x, plain.x, 1e-6);
  near(here.y, plain.y, 1e-6);
  assert.equal(reads, 1, 'the dye is read once for the note');
  assert.ok(askedDense.every((h) => Math.abs(h - here.cell / 2) < 1e-9), 'the box of the cell');
  assert.ok(askedOpen.every((h) => Math.abs(h - here.cell * TUNING.noteOpenHalf) < 1e-9), 'half');
  // 2. covered at the stir's place, dense everywhere: the nearest open lattice point, anywhere
  const { note: moved } = noteFor({ open, density: () => () => dense });
  assert.ok(moved, 'a note beside the control');
  assert.ok(open(moved.x, moved.y, moved.cell * TUNING.noteOpenHalf), 'the note is open');
  const near2 = lattice(moved).filter((p) => open(p.x, p.y, moved.cell * TUNING.noteOpenHalf))
    .reduce((m, p) => Math.min(m, dist(p, plain)), Infinity);
  near(dist(moved, plain), near2, 1e-6);
  // 3. thin at the stir's place, dense only far away: the nearest dense point (not the 0.9 cell
  // reach of before), kept open
  const farDense = (x) => (x > 900 ? dense : thin);
  const { note: far } = noteFor({ open, density: () => (x) => farDense(x) });
  assert.ok(far, 'a note in the dense smoke');
  assert.ok(far.x > 900 - far.cell / 2 - 1e-6, `the note at ${far.x}`);
  assert.ok(dist(far, plain) > 0.9 * far.cell, 'further than the old reach');
  const nearest = lattice(far).filter((p) => farDense(p.x) >= TUNING.noteDenseMin
    && open(p.x, p.y, far.cell * TUNING.noteOpenHalf)).reduce((m, p) => Math.min(m, dist(p, plain)),
    Infinity);
  near(dist(far, plain), nearest, 1e-6);
  // the same with dense smoke in scattered patches, where the nearest point is along neither axis
  const rand = seeded(9);
  const patches = new Set();
  for (let k = 0; k < 130; k++) {
    patches.add(`${Math.floor(rand() * 26)},${Math.floor(rand() * 18)}`);
  }
  const patchy = (x, y) => (patches.has(`${Math.floor(x / 50)},${Math.floor(y / 50)}`)
    ? dense : thin);
  patches.delete(`${Math.floor(click[0] / 50)},${Math.floor(click[1] / 50)}`);
  const { note: diag } = noteFor({ open, density: () => (x, y) => patchy(x, y) });
  assert.ok(diag, 'a note in a patch of dense smoke');
  const nearestPatch = lattice(diag).filter((p) => patchy(p.x, p.y) >= TUNING.noteDenseMin
    && open(p.x, p.y, diag.cell * TUNING.noteOpenHalf))
    .reduce((m, p) => Math.min(m, dist(p, plain)), Infinity);
  near(dist(diag, plain), nearestPatch, 1e-6);
  // the nearest is the nearest in the straight line: of two dense points on the lattice, one a
  // step across and a step down, the other two steps across, the first is chosen
  const mid = [900, 450];
  const { note: centre } = noteFor({}, 80, mid);
  const unit = centre.cell * TUNING.noteFindStep;
  const at = (di, dj) => [centre.x + di * unit, centre.y + dj * unit];
  const picks = [at(1, 1), at(2, 0), at(-2, 0)];
  const { note: line } = noteFor({
    density: () => (x, y) => (picks.some((p) => Math.hypot(p[0] - x, p[1] - y) < 1e-6)
      ? dense : thin),
  }, 80, mid);
  assert.ok(line, 'a note at one of the dense points');
  near(line.x, picks[0][0], 1e-6);
  near(line.y, picks[0][1], 1e-6);
  // 4. nothing dense anywhere, or nothing open: no note, and the gap and the cap slot are not
  // spent, so the next settle, with room, makes one at once (a 4 s gap would hold it back)
  for (const shut of ['density', 'open']) {
    let room = false;
    const options = shut === 'density'
      ? { density: () => (() => (room ? dense : thin)) }
      : { open: () => room };
    const w = world(80, { roll: 0, ...options });
    w.d.anchor(ANCHOR);
    w.run(0.3);
    w.d.click(click[0], click[1], w.t);
    assert.equal(w.run(3, null).filter((p) => p.note).length, 0, `no note with no ${shut}`);
    assert.equal(w.d.stats(w.t).notes.stamped, 0);
    room = true;
    w.d.click(click[0], click[1], w.t);
    const later = w.run(3, null).filter((p) => p.note);
    assert.equal(later.length, 1, `a note once there is room, the gap unspent (${shut})`);
  }
  // 5. a dye that cannot be read (null) or a reader that throws counts as dense everywhere
  for (const density of [() => null, () => { throw new Error('no read'); }]) {
    const { note } = noteFor({ density });
    assert.ok(note, 'a note when the dye cannot be read');
    near(note.x, plain.x, 1e-6);
  }
  const { note: free } = noteFor({ open: () => true });
  near(free.x, plain.x, 1e-6);
  near(free.y, plain.y, 1e-6);
});

ok('a_beat_asks_the_smoke_for_a_puff: a beat puffs outward from the dial, stirs nothing, '
  + 'and asks for nothing while away or without a dial', () => {
  const w = world(80);
  assert.equal(w.d.beat(1, w.t), false, 'no anchor, no puff');
  w.d.anchor(ANCHOR);
  w.run(0.3);
  assert.equal(w.d.beat(1, w.t), true);
  const puff = w.tick().splats.find((s) => s.kind === 'puff');
  assert.ok(puff, 'a puff');
  // it leaves the dial's track at the glow's level (the target, 0 here), the push pointing away
  // from the dial's centre (the splat's y is up: `add` has the screen's y negated)
  const dot = dialDot(ANCHOR, 0);
  const want = uvOf(dot, SIZE);
  near(puff.x, want.x, 1e-6);
  near(puff.y, want.y, 1e-6);
  const out = [dot.x - ANCHOR.x, -(dot.y - ANCHOR.y)];
  assert.ok(puff.add[0] * out[0] + puff.add[1] * out[1] > 0, `outward: ${puff.add}`);
  assert.ok(puff.dye && puff.dye[3] > 0, 'it carries dye');
  // it leaves the knob where the caller says it is (the volume's fraction, as a step takes it),
  // not where the music has lifted the glow to
  const at = world(85);
  at.d.anchor(ANCHOR);
  at.d.level(0.9);
  at.run(0.3);
  at.d.beat(1, at.t, 0.25);
  const knob = at.tick().splats.find((s) => s.kind === 'puff');
  const knobAt = uvOf(dialDot(ANCHOR, 0.25), SIZE);
  near(knob.x, knobAt.x, 1e-6);
  near(knob.y, knobAt.y, 1e-6);
  // a beat is not a stir: no settle to make a note, no pointer splat, no fast rhythm of its own
  const stats = w.d.stats(w.t);
  assert.equal(stats.notes.rolls, 0);
  w.run(8);
  assert.equal(w.d.stats(w.t).notes.rolls, 0, 'a beat arms no note');
  // a stronger beat is a bigger puff; strength outside 0..1 is bounded; not a number is no beat
  const size = (strength) => {
    const v = world(81);
    v.d.anchor(ANCHOR);
    v.run(0.3);
    v.d.beat(strength, v.t);
    const s = v.tick().splats.find((x) => x.kind === 'puff');
    return Math.hypot(...s.add);
  };
  assert.ok(size(1) > size(0.3), 'stronger beat, stronger puff');
  near(size(7), size(1), 1e-9);
  assert.equal(world(82).d.beat(Number.NaN, 0), false);
  // at most maxPuffs wait, as for steps
  const q = world(83);
  q.d.anchor(ANCHOR);
  for (let i = 0; i < TUNING.maxPuffs + 5; i++) q.d.beat(1, 0);
  assert.equal(q.tick().splats.filter((s) => s.kind === 'puff').length, TUNING.maxPuffs);
  // none while away or while the context is lost
  q.d.away(true, q.t);
  assert.equal(q.d.beat(1, q.t), false);
  const lostWorld = world(84);
  lostWorld.d.anchor(ANCHOR);
  lostWorld.d.lost(true);
  assert.equal(lostWorld.d.beat(1, 0), false);
});

console.log(`${n} checks passed`);
