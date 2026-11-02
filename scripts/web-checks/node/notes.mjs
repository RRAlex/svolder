/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the smoke's notes: the agitation measure, the note gate, the
// placement ranges, the three glyphs against a recording context, the sources' weights.
// Run: node notes.mjs
import assert from 'node:assert/strict';
import {
  Agitation, NoteGate, SettleWatch, TUNING, VolumeBurst, bigRise, clickPuff, lighten,
  meterAgitation, noteCell, notePlacement, notePosition, peakOf, pointerAgitation, seeded,
  stepAgitation, stepStir,
} from '../../../bins/svolderd/web/themes/ember/smoke-model.js';
import { drawAtlas } from '../../../bins/svolderd/web/themes/ember/smoke-notes.js';
import { lookTests } from './look-values.mjs';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const okLook = lookTests(TUNING, ok);
const near = (a, b, e = 1e-9) => assert.ok(Math.abs(a - b) <= e, `${a} vs ${b}`);

ok('TUNING: the note numbers exist, with no dye of the note\'s own', () => {
  assert.equal(TUNING.noteLift, undefined);
  for (const k of ['settleFrom', 'agitPointer', 'agitSpeedFloor', 'agitSpeedMax', 'agitStep',
    'agitMeter', 'noteInk', 'noteLight', 'noteCarve', 'noteHalo', 'noteHaloBlur', 'noteHaloWidth',
    'noteGlyphHeight', 'noteCell']) {
    assert.ok(Number.isFinite(TUNING[k]) && TUNING[k] > 0, k);
  }
  assert.ok(TUNING.noteHalo < 1 && TUNING.noteInk <= 1 && TUNING.noteCarve > 0.5
    && TUNING.noteCarve < 1);
});

okLook('Agitation: starts at zero, adds, decays as exp(-dt / 0.5 s)', () => {
  const a = new Agitation();
  assert.equal(a.value(0), 0);
  a.add(2, 10, 20, 1);
  near(a.value(1), 2);
  near(a.value(1.5), 2 * Math.exp(-1));
  near(a.value(2), 2 * Math.exp(-2));
  near(a.value(11), 2 * Math.exp(-20));
});

okLook('Agitation: contributions sum, each decaying from its own time', () => {
  const a = new Agitation();
  a.add(1, 0, 0, 0);
  a.add(1, 0, 0, 0.5);
  near(a.value(0.5), 1 * Math.exp(-1) + 1);
  near(a.value(1), 1 * Math.exp(-2) + 1 * Math.exp(-1));
});

ok('Agitation: a read at an earlier time than the last add neither grows nor breaks it', () => {
  const a = new Agitation();
  a.add(1, 0, 0, 5);
  near(a.value(4.9), 1);
  near(a.value(5), 1);
  a.add(1, 0, 0, 4.95);
  assert.ok(a.value(5) <= 2 + 1e-9 && a.value(5) >= 1.9);
});

ok('Agitation: where() is the largest recent contribution\'s place', () => {
  const a = new Agitation();
  a.add(1, 10, 10, 0);
  a.add(3, 50, 60, 0.1);
  a.add(2, 90, 90, 0.2);
  assert.deepEqual(a.where(), { x: 50, y: 60 });
  // the big one fades: a later, smaller one is larger by then
  const b = new Agitation();
  b.add(3, 1, 1, 0);
  b.add(1, 2, 2, 2);
  assert.deepEqual(b.where(), { x: 2, y: 2 });
});

ok('Agitation: where() with nothing added is null; reset forgets all', () => {
  const a = new Agitation();
  assert.equal(a.where(), null);
  a.add(5, 1, 2, 0);
  a.reset();
  assert.equal(a.value(0), 0);
  assert.equal(a.where(), null);
});

ok('Agitation: ignores non-finite or non-positive amounts and places', () => {
  const a = new Agitation();
  a.add(NaN, 1, 1, 0);
  a.add(-3, 1, 1, 0);
  a.add(Infinity, 1, 1, 0);
  a.add(1, NaN, 1, 0);
  a.add(1, 1, 1, NaN);
  assert.equal(a.value(0), 0);
  assert.equal(a.where(), null);
});

/** One try at the gate: whether a note may form at `t`; it is then recorded. */
const take = (g, t) => {
  if (!g.allows(t)) return false;
  g.record(t);
  return true;
};

okLook('NoteGate: the cooldown holds for 4 s after a note', () => {
  const g = new NoteGate();
  assert.equal(take(g, 10), true);
  assert.equal(take(g, 10.1), false);
  assert.equal(take(g, 13.99), false);
  assert.equal(take(g, 14), true);
  assert.equal(take(g, 14.5), false);
});

ok('NoteGate: the first note needs no wait (time zero is fine)', () => {
  assert.equal(take(new NoteGate(), 0), true);
});

ok('NoteGate: the cap holds with a short cooldown injected, and the life expires', () => {
  const g = new NoteGate({ cooldown: 0.5, life: 3, cap: 2 });
  assert.equal(take(g, 0), true);
  assert.equal(take(g, 0.5), true);
  assert.equal(g.live(0.6), 2);
  assert.equal(take(g, 1.0), false);
  assert.equal(take(g, 2.9), false);
  assert.equal(take(g, 3.0), true); // the first one's life is over
  assert.equal(g.live(3.0), 2);
  assert.equal(take(g, 3.5), true);
  assert.equal(take(g, 3.6), false);
});

okLook('NoteGate: defaults are 4 s, 3 s, 2; a cap of 1 and a zero cooldown', () => {
  const g = new NoteGate({ cooldown: 0, life: 1, cap: 1 });
  assert.equal(take(g, 0), true);
  assert.equal(take(g, 0.5), false);
  assert.equal(take(g, 1), true);
  const d = new NoteGate();
  assert.equal(d.cooldown, 4);
  assert.equal(d.life, 3);
  assert.equal(d.cap, 2);
});

ok('NoteGate: allows() alone records nothing; a non-finite time is refused', () => {
  const g = new NoteGate();
  assert.equal(g.allows(NaN), false);
  assert.equal(g.allows(0), true);
  assert.equal(g.allows(0), true);
  assert.equal(g.live(0), 0);
  g.record(NaN);
  assert.equal(g.live(0), 0);
  g.record(0);
  assert.equal(g.live(0), 1);
});

ok('NoteGate: twenty settles inside one second give at most one note, never over two alive', () => {
  const g = new NoteGate();
  let formed = 0;
  for (let i = 0; i < 20; i++) formed += take(g, i * 0.05) ? 1 : 0;
  assert.equal(formed, 1);
  // twenty quick clicks spread over 12 s: the gap and the cap hold at every moment
  const h = new NoteGate();
  const times = [];
  for (let i = 0; i < 20; i++) {
    const t = i * 0.6;
    if (take(h, t)) times.push(t);
    assert.ok(h.live(t) <= 2, `${h.live(t)} at ${t}`);
  }
  for (let i = 1; i < times.length; i++) assert.ok(times[i] - times[i - 1] >= 4 - 1e-9);
});

okLook('TUNING: the settle numbers are consistent', () => {
  assert.ok(TUNING.settleBelow < TUNING.settleFrom && TUNING.settleSecs > 0);
  assert.ok(TUNING.clickAgit > TUNING.settleFrom && TUNING.agitStepMin > TUNING.settleFrom);
});

ok('VolumeBurst: steps summing to 12 dB within 1 s give a rise of 12', () => {
  const b = new VolumeBurst();
  let r;
  for (let i = 0; i < 4; i++) r = b.add(-30 + i * 3, -27 + i * 3, 0.02, i * 0.25);
  near(r.rise, 12);
  near(r.riseShare, 0); // a dial with dB counts the dB rise only
});

okLook('VolumeBurst: a 6.8 dB rise over 0.134 of a dial with dB is not big', () => {
  const b = new VolumeBurst();
  let r;
  for (let i = 0; i < 17; i++) r = b.add(-30 + i * 0.4, -29.6 + i * 0.4, 0.134 / 17, i * 0.05);
  near(r.rise, 6.8, 1e-6);
  assert.equal(bigRise(r), false);
  // without dB the share rule decides
  const c = new VolumeBurst();
  for (let i = 0; i < 17; i++) r = c.add(null, null, 0.134 / 17, i * 0.05);
  assert.equal(bigRise(r), true);
});

okLook('VolumeBurst: a gap of 1.5 s splits the burst; 1 s does not', () => {
  const b = new VolumeBurst();
  b.add(-30, -24, 0.05, 0);
  near(b.add(-24, -18, 0.05, 1.5).rise, 6);
  const c = new VolumeBurst();
  c.add(-30, -24, 0.05, 0);
  near(c.add(-24, -18, 0.05, 1).rise, 12);
});

ok('VolumeBurst: a step down ends the burst; the next rise starts afresh', () => {
  const b = new VolumeBurst();
  b.add(-30, -20, 0.1, 0);
  assert.deepEqual(b.add(-20, -22, -0.01, 0.2), { rise: 0, riseShare: 0 });
  near(b.add(-22, -19, 0.02, 0.4).rise, 3);
});

ok('VolumeBurst: a dial without dB gives rise 0 and counts its share', () => {
  const b = new VolumeBurst();
  b.add(null, null, 0.05, 0);
  const r = b.add(null, null, 0.05, 0.3);
  assert.equal(r.rise, 0);
  near(r.riseShare, 0.1);
  assert.deepEqual(b.add(null, null, -0.05, 0.4), { rise: 0, riseShare: 0 });
});

ok('VolumeBurst: bad input adds nothing', () => {
  const b = new VolumeBurst();
  assert.deepEqual(b.add(NaN, -3, 0.1, 0), { rise: 0, riseShare: 0.1 });
  assert.deepEqual(new VolumeBurst().add(-3, -2, NaN, 0), { rise: 1, riseShare: 0 });
});

ok('VolumeBurst: a step with no change keeps the burst; the dB decides the direction', () => {
  const b = new VolumeBurst();
  b.add(-30, -24, 0.05, 0);
  near(b.add(-24, -24, 0, 0.2).rise, 6);
  assert.deepEqual(b.add(-24, -26, 0.01, 0.3), { rise: 0, riseShare: 0 });
});

ok('VolumeBurst: one step without dB in the burst takes the rise back to 0', () => {
  const b = new VolumeBurst();
  b.add(-30, -24, 0.05, 0);
  const r = b.add(null, -24, 0.05, 0.1);
  assert.equal(r.rise, 0);
  near(r.riseShare, 0.1);
});

okLook('bigRise: exactly 0.125 of the range is not big', () => {
  assert.equal(bigRise({ rise: 0, riseShare: 0.125, enteredRed: false }), false);
});

ok('NoteGate: a non-finite note is not recorded', () => {
  const g = new NoteGate();
  g.record(NaN);
  assert.equal(g.stamps.length, 0);
});

ok('SettleWatch: a roll equal to the odds loses; reset also empties the agitation', () => {
  const w = new SettleWatch();
  w.stir(3, 1, 1, 0, 0.5);
  let got = null;
  for (let t = 0; t < 5; t += 0.05) got ||= w.poll(t, () => 0.5);
  assert.equal(got, null);
  const v = new SettleWatch();
  v.stir(5, 1, 1, 0, 1);
  v.reset();
  v.stir(0.5, 2, 2, 0, 1);
  for (let t = 0; t < 8; t += 0.05) assert.equal(v.poll(t, () => 0), null);
});

okLook('bigRise: over 10 dB, over an eighth of a dial without dB, or into the red', () => {
  assert.equal(bigRise({ rise: 9.9, riseShare: 0, enteredRed: false }), false);
  assert.equal(bigRise({ rise: 10, riseShare: 0, enteredRed: false }), false);
  assert.equal(bigRise({ rise: 10.1, riseShare: 0, enteredRed: false }), true);
  assert.equal(bigRise({ rise: 0, riseShare: 0.12, enteredRed: false }), false);
  assert.equal(bigRise({ rise: 0, riseShare: 0.13, enteredRed: false }), true);
  assert.equal(bigRise({ rise: 0, riseShare: 0, enteredRed: true }), true);
  assert.equal(bigRise({}), false);
});

okLook('stepStir: a single step still stirs enough to arm a settle', () => {
  assert.ok(stepStir(0.008) > TUNING.settleFrom);
  assert.equal(stepStir(0.5), stepAgitation(0.5));
  assert.equal(stepStir(0), 0);
  assert.equal(stepStir(NaN), 0);
});

ok('clickPuff: small, at the click, along the angle, and bounded', () => {
  const p = clickPuff({ x: 5, y: 7 }, 0);
  assert.equal(p.x, 5);
  assert.equal(p.y, 7);
  near(p.fx, TUNING.clickPuffPush);
  near(p.fy, 0);
  assert.equal(p.dye, TUNING.clickPuffDye);
  assert.ok(TUNING.clickPuffDye < TUNING.puffDye && TUNING.clickPuffPush < TUNING.puffPush);
});

ok('SettleWatch: no settle while the agitation is above settleBelow', () => {
  const w = new SettleWatch();
  w.stir(3, 10, 20, 0, 1);
  for (let t = 0; t < 0.5; t += 0.05) assert.equal(w.poll(t, () => 0), null);
  // kept stirring: the agitation never falls
  for (let t = 0.5; t < 10; t += 0.1) {
    w.stir(1, 10, 20, t, 1);
    assert.equal(w.poll(t, () => 0), null);
  }
});

ok('SettleWatch: a stir that never rises above settleFrom never settles', () => {
  const w = new SettleWatch();
  w.stir(TUNING.settleFrom * 0.9, 1, 1, 0, 1);
  for (let t = 0; t < 10; t += 0.1) assert.equal(w.poll(t, () => 0), null);
});

ok('SettleWatch: one place per episode, at the largest stir\'s place, once it dies down', () => {
  const w = new SettleWatch();
  w.stir(2, 10, 20, 0, 1);
  w.stir(4, 50, 60, 0.1, 1);
  let got = null;
  let count = 0;
  for (let t = 0; t < 10; t += 0.05) {
    const p = w.poll(t, () => 0);
    if (p) { got = { ...p, t }; count++; }
  }
  assert.equal(count, 1);
  assert.deepEqual({ x: got.x, y: got.y }, { x: 50, y: 60 });
  assert.ok(got.t >= TUNING.settleSecs, `${got.t}`);
});

ok('SettleWatch: a failed roll spends the episode; the next stir starts another', () => {
  const w = new SettleWatch();
  w.stir(3, 1, 1, 0, 0.5);
  let any = null;
  for (let t = 0; t < 5; t += 0.05) any ||= w.poll(t, () => 0.99);
  assert.equal(any, null);
  for (let t = 5; t < 8; t += 0.05) assert.equal(w.poll(t, () => 0), null);
  w.stir(3, 2, 2, 8, 0.5);
  let got = null;
  for (let t = 8; t < 13; t += 0.05) got ||= w.poll(t, () => 0);
  assert.deepEqual(got, { x: 2, y: 2 });
});

ok('SettleWatch: the settle waits settleSecs after the rise, whatever the size', () => {
  const w = new SettleWatch();
  w.stir(TUNING.settleFrom * 1.01, 1, 1, 0, 1);
  // falls below settleBelow in under settleSecs, yet nothing forms before settleSecs
  let first = null;
  for (let t = 0; t < 3 && first === null; t += 0.01) if (w.poll(t, () => 0)) first = t;
  assert.ok(first >= TUNING.settleSecs - 1e-9, `${first}`);
});

ok('SettleWatch: the episode\'s odds are the highest of its contributions', () => {
  // a roll of 0.6 passes 2/3 but fails 1/3 and 1/2; the episode rolls once
  const place = (odds) => {
    const w = new SettleWatch();
    odds.forEach((o, i) => w.stir(2, 1, 1, i * 0.1, o));
    let got = null;
    let rolls = 0;
    for (let t = 0.3; t < 6; t += 0.05) {
      got ||= w.poll(t, () => { rolls++; return 0.6; });
    }
    return { got, rolls };
  };
  assert.deepEqual(place([1 / 3, 1 / 3, 1 / 3]), { got: null, rolls: 1 });
  assert.equal(place([1 / 3, 1 / 2, 1 / 3]).got, null);
  assert.notEqual(place([1 / 3, 2 / 3, 1 / 3]).got, null);
  assert.notEqual(place([2 / 3, 1 / 3, 1 / 3]).got, null);
});

okLook('SettleWatch: seeded rates over 10 000 episodes are within 3 % of 1/3, 1/2 and 2/3', () => {
  const odds3 = [TUNING.settleOdds, TUNING.clickOdds, TUNING.bigRiseOdds];
  assert.deepEqual(odds3, [1 / 3, 1 / 2, 2 / 3]);
  for (const odds of odds3) {
    const random = seeded(7);
    const w = new SettleWatch();
    let formed = 0;
    let t = 0;
    for (let i = 0; i < 10000; i++) {
      w.stir(3, 1, 1, t, odds);
      for (let end = t + 4; t < end; t += 0.05) if (w.poll(t, random)) formed++;
    }
    near(formed / 10000, odds, 0.03);
  }
});

ok('SettleWatch: reset drops a pending settle', () => {
  const w = new SettleWatch();
  w.stir(5, 1, 1, 0, 1);
  w.poll(0.1, () => 0);
  w.reset();
  for (let t = 0.1; t < 10; t += 0.05) assert.equal(w.poll(t, () => 0), null);
  // and the watch works again afterwards
  w.stir(5, 3, 3, 10, 1);
  let got = null;
  for (let t = 10; t < 15; t += 0.05) got ||= w.poll(t, () => 0);
  assert.deepEqual(got, { x: 3, y: 3 });
});

ok('SettleWatch: bad input is ignored', () => {
  const w = new SettleWatch();
  w.stir(NaN, 1, 1, 0, 1);
  w.stir(3, 1, 1, 0, NaN);
  w.stir(3, NaN, 1, 0, 1);
  for (let t = 0; t < 5; t += 0.1) assert.equal(w.poll(t, () => 0), null);
  assert.equal(w.poll(NaN, () => 0), null);
});

okLook(
  'notePlacement: 10 000 draws, glyph 0-2 all used, tilt within 25 deg, size 6-9 % of the side',
  () => {
    const seen = [0, 0, 0];
    let tmin = 1e9, tmax = -1e9, smin = 1e9, smax = -1e9;
    for (let i = 0; i < 10000; i++) {
      const p = notePlacement(Math.random, 800);
      assert.ok([0, 1, 2].includes(p.glyph));
      seen[p.glyph]++;
      tmin = Math.min(tmin, p.tilt); tmax = Math.max(tmax, p.tilt);
      smin = Math.min(smin, p.size); smax = Math.max(smax, p.size);
    }
    assert.ok(seen.every((c) => c > 2500), seen.join());
    assert.ok(tmin >= -25 && tmax <= 25 && tmin < -23 && tmax > 23, `${tmin} ${tmax}`);
    assert.ok(smin >= 48 - 1e-9 && smax <= 72 + 1e-9 && smin < 49 && smax > 71, `${smin} ${smax}`);
  });

okLook('notePlacement: the extremes of random() stay in range (0 and just under 1)', () => {
  for (const r of [0, 0.999999999]) {
    const p = notePlacement(() => r, 1000);
    assert.ok([0, 1, 2].includes(p.glyph));
    assert.ok(Math.abs(p.tilt) <= 25 && p.size >= 60 && p.size <= 90);
  }
  assert.equal(notePlacement(() => 0, 1000).glyph, 0);
  assert.equal(notePlacement(() => 0.5, 1000).glyph, 1);
  assert.equal(notePlacement(() => 0.999999, 1000).glyph, 2);
  near(notePlacement(() => 0.5, 1000).tilt, 0);
});

/** The peak of the agitation for a pointer moving at `speed` short sides a second for `secs`,
 * events every 1/120 s, through the model's own functions. */
function flick(speed, secs, side = 900) {
  const a = new Agitation();
  const dt = 1 / 120;
  let best = 0;
  for (let t = dt; t <= secs + 1e-9; t += dt) {
    a.add(pointerAgitation({ dist: speed * side * dt, dt, shortSide: side }), 0, 0, t);
    best = Math.max(best, a.value(t));
  }
  return best;
}

okLook(
  'the pointer: an ordinary move (2 short sides a second for 0.5 s) reaches the threshold with '
  + 'room to spare', () => {
  assert.ok(flick(2, 0.5) >= 1.25 * TUNING.settleFrom, `${flick(2, 0.5)}`);
  assert.ok(flick(2.5, 0.4) >= TUNING.settleFrom, `${flick(2.5, 0.4)}`);
});

ok('the pointer: a real short flick (0.2-0.3 s at 5 short sides a second or more) reaches the '
  + 'threshold', () => {
  for (const [s, d] of [[5, 0.2], [5, 0.3], [6, 0.2], [8, 0.2], [8, 0.15], [20, 0.15],
    [100, 0.2]]) {
    assert.ok(flick(s, d) >= TUNING.settleFrom, `${s} for ${d}: ${flick(s, d)}`);
  }
});

okLook(
  'the pointer: a 600 px/s wave, a slow drift (1 short side a second), a moderate 0.3 s sweep and '
  + 'anything at or under the floor never reach it, however long', () => {
  for (const [s, d] of [[0.67, 60], [1, 60], [1.5, 0.3], [TUNING.agitSpeedFloor, 60],
    [0.2, 60]]) {
    assert.ok(flick(s, d) < TUNING.settleFrom, `${s} for ${d}: ${flick(s, d)}`);
  }
  assert.ok(flick(TUNING.agitSpeedFloor + 0.3, 0.3) < TUNING.settleFrom);
});

okLook('the pointer: the speed is clamped, so a wild flick adds no more than the clamp', () => {
  near(pointerAgitation({ dist: 1e6, dt: 0.01, shortSide: 900 }),
    (TUNING.agitSpeedMax - TUNING.agitSpeedFloor) * 0.01 * TUNING.agitPointer);
  near(pointerAgitation({ dist: 900 * 8 * 0.01, dt: 0.01, shortSide: 900 }),
    pointerAgitation({ dist: 900 * 80 * 0.01, dt: 0.01, shortSide: 900 }));
});

ok('the pointer: the event rate does not change it (the same flick at 60, 120 and 500 Hz)', () => {
  const at = (hz) => {
    const a = new Agitation();
    let best = 0;
    for (let i = 1; i <= Math.round(0.25 * hz); i++) {
      a.add(pointerAgitation({ dist: 6 * 900 / hz, dt: 1 / hz, shortSide: 900 }), 0, 0, i / hz);
      best = Math.max(best, a.value(i / hz));
    }
    return best;
  };
  near(at(60), at(500), 0.12 * at(500));
  near(at(120), at(500), 0.08 * at(500));
});

ok('the pointer: nothing for bad input', () => {
  for (const bad of [{ dist: 3, dt: 0, shortSide: 900 }, { dist: 3, dt: 0.01, shortSide: 0 },
    { dist: NaN, dt: 0.01, shortSide: 900 }, { dist: -5, dt: 0.01, shortSide: 900 },
    { dist: 3, dt: -1, shortSide: 900 }]) {
    assert.equal(pointerAgitation(bad), 0, JSON.stringify(bad));
  }
});

okLook('the volume step: the step\'s share of the range', () => {
  near(stepAgitation(0.2), 0.2 * TUNING.agitStep);
  assert.ok(stepAgitation(0.2) >= TUNING.settleFrom);
  assert.ok(stepAgitation(0.01) < TUNING.settleFrom);
  assert.equal(stepAgitation(NaN), 0);
  assert.equal(stepAgitation(-1), 0);
});

okLook('a meter: the excess over red per second, scaled to 0-1; nothing at or under red', () => {
  assert.equal(meterAgitation(0.5, TUNING.meterRed, 1), 0);
  assert.equal(meterAgitation(TUNING.meterRed, TUNING.meterRed, 1), 0);
  near(meterAgitation(1, TUNING.meterRed, 1), TUNING.agitMeter);
  near(meterAgitation(1, TUNING.meterRed, 0.5), TUNING.agitMeter / 2);
  near(meterAgitation(0.965, TUNING.meterRed, 1), TUNING.agitMeter * 0.5, 1e-9);
  assert.equal(meterAgitation(NaN, TUNING.meterRed, 1), 0);
  assert.equal(meterAgitation(2, TUNING.meterRed, 0), 0);
});

ok('peakOf: the largest reading among the first `limit` (the anchor\'s meters), with its index',
  () => {
    assert.deepEqual(peakOf([0.2, 0.9, 0.5], 3), { i: 1, fraction: 0.9 });
    assert.deepEqual(peakOf([0.2, 0.9, 0.5], 1), { i: 0, fraction: 0.2 });
    assert.deepEqual(peakOf([0.4, 0.4], 2), { i: 0, fraction: 0.4 });
    assert.deepEqual(peakOf([NaN, 0.3, undefined], 3), { i: 1, fraction: 0.3 });
    assert.equal(peakOf([], 3), null);
    assert.equal(peakOf([0.5], 0), null);
    assert.equal(peakOf(null, 3), null);
  });

/** The peak agitation of music peaking into red: `n` meters, one of them (the loudest) at `peak`
 * for `secs` every report, the others at `rest`, fed as the page does, one peak reading per frame
 * at 60 Hz. */
function music(n, peak, secs, rest = 0.6) {
  const a = new Agitation();
  const dt = 1 / 60;
  let best = 0;
  for (let t = dt; t <= secs + 1e-9; t += dt) {
    const readings = Array.from({ length: n }, (_, i) => (i === n - 1 ? peak : rest));
    const top = peakOf(readings, n);
    a.add(meterAgitation(top.fraction, TUNING.meterRed, dt), top.i, 0, t);
    best = Math.max(best, a.value(t));
  }
  return best;
}

okLook('meters: a music peak into red (0.99 for 0.15 s) reaches the threshold alike on a mono amp '
  + 'and on a five-meter amp; a 0.6 reading never does', () => {
  const one = music(1, 0.99, 0.15);
  const five = music(5, 0.99, 0.15);
  assert.ok(one >= TUNING.settleFrom, `mono ${one}`);
  near(one, five, 1e-9);
  assert.ok(music(1, 0.6, 30) === 0 && music(5, 0.6, 30) === 0);
});

okLook('meters: a reading just into red for a moment, or held at 0.95 for half a second, does not',
  () => {
    assert.ok(music(1, 0.95, 0.5) < TUNING.settleFrom, `${music(1, 0.95, 0.5)}`);
    assert.ok(music(5, 0.94, 0.5) < TUNING.settleFrom);
    assert.ok(music(1, 0.99, 0.03) < TUNING.settleFrom);
  });

okLook('meters: held for 5 s, 0.95 (near the top of the red zone) reaches the threshold; 0.94 and '
  + 'the edge of red (0.93) do not', () => {
  assert.ok(music(1, 0.95, 5) >= TUNING.settleFrom, `${music(1, 0.95, 5)}`);
  assert.ok(music(5, 0.95, 5) >= TUNING.settleFrom);
  assert.ok(music(1, 0.94, 5) < TUNING.settleFrom, `${music(1, 0.94, 5)}`);
  assert.equal(music(1, TUNING.meterRed, 30), 0);
});

ok('notePosition: kept `reach` inside the canvas, and at `reach` on a canvas smaller than it',
  () => {
    assert.deepEqual(notePosition({ x: 500, y: 400 }, 40, [1000, 800]), { x: 500, y: 400 });
    assert.deepEqual(notePosition({ x: 5, y: 790 }, 40, [1000, 800]), { x: 40, y: 760 });
    assert.deepEqual(notePosition({ x: 2000, y: -9 }, 40, [1000, 800]), { x: 960, y: 40 });
    assert.deepEqual(notePosition({ x: 10, y: 10 }, 40, [60, 60]), { x: 40, y: 40 });
  });

ok('lighten: a share of the way to white, each channel', () => {
  assert.deepEqual(lighten([0, 0.5, 1], 0), [0, 0.5, 1]);
  assert.deepEqual(lighten([0, 0.5, 1], 1), [1, 1, 1]);
  const t = lighten([0.2, 0.4, 0.8], 0.5);
  near(t[0], 0.6); near(t[1], 0.7); near(t[2], 0.9);
});

okLook('noteCell: the cell is the glyph\'s visible height over its share of the cell', () => {
  near(noteCell(54), 54 / TUNING.noteGlyphHeight);
  assert.ok(TUNING.noteGlyphHeight > 0.4 && TUNING.noteGlyphHeight < 0.8);
});

// ---- the atlas against a recording context
function recorder() {
  const log = [];
  const state = {};
  const ctx = new Proxy({}, {
    get(_, name) {
      if (name === '__log') return log;
      if (name in state) return state[name];
      return (...args) => { log.push({ op: name, args }); };
    },
    set(_, name, value) {
      state[name] = value;
      log.push({ op: `=${String(name)}`, args: [value] });
      return true;
    },
  });
  return ctx;
}

const CELL = 128;
const ctx = recorder();
drawAtlas(ctx, CELL, { blur: 0.11, width: 0.06 });
const log = ctx.__log;

/** The glyph path of each cell: the events between the cell's clip and its restore, after the
 * clip's own path. */
function cells() {
  const out = [];
  let cur = null;
  for (const e of log) {
    if (e.op === 'save') cur = { events: [] };
    else if (e.op === 'restore') { out.push(cur); cur = null; }
    else if (cur) cur.events.push(e);
  }
  return out;
}
const parts = cells();

ok('drawAtlas: three cells, each in its own save / clip / restore', () => {
  assert.equal(parts.length, 3);
  parts.forEach((p, i) => {
    const clip = p.events.findIndex((e) => e.op === 'clip');
    assert.ok(clip >= 0, `cell ${i} has no clip`);
    const rect = p.events.slice(0, clip).find((e) => e.op === 'rect');
    assert.deepEqual(rect.args, [i * CELL, 0, CELL, CELL]);
  });
});

/** The path's pieces of a cell (after the clip): ellipses, rects, curves, lines, fills. */
function pieces(p) {
  const clip = p.events.findIndex((e) => e.op === 'clip');
  const after = p.events.slice(clip + 1);
  const begin = after.findIndex((e) => e.op === 'beginPath');
  assert.ok(begin >= 0);
  const path = [];
  for (let i = begin + 1; i < after.length; i++) {
    if (['fill', 'stroke'].includes(after[i].op)) break;
    path.push(after[i]);
  }
  const count = (op) => path.filter((e) => e.op === op).length;
  return {
    path, heads: count('ellipse'), rects: count('rect'), curves: count('bezierCurveTo'),
    lines: count('lineTo'), fills: after.filter((e) => e.op === 'fill').length,
    strokes: after.filter((e) => e.op === 'stroke').length, after,
  };
}

ok('drawAtlas: the quaver has one tilted head, one stem and a flag; the pair two heads, two stems '
  + 'and a beam; the crotchet one head and a stem, no flag, no beam', () => {
  const [q, pair, c] = parts.map(pieces);
  assert.equal(q.heads, 1); assert.equal(q.rects, 1); assert.ok(q.curves >= 2);
  assert.equal(pair.heads, 2); assert.equal(pair.rects, 2); assert.equal(pair.curves, 0);
  assert.ok(pair.lines >= 3, 'the beam is a polygon');
  assert.equal(c.heads, 1); assert.equal(c.rects, 1); assert.equal(c.curves, 0);
  assert.equal(c.lines, 0);
  for (const g of [q, pair, c]) {
    for (const e of g.path.filter((x) => x.op === 'ellipse')) {
      assert.notEqual(e.args[4], 0, 'a head is tilted');
      assert.ok(e.args[2] > e.args[3], 'a head is wider than it is tall');
    }
  }
});

ok('drawAtlas: nothing is drawn outside its cell (every point of every path)', () => {
  parts.forEach((p, i) => {
    const x0 = i * CELL, x1 = x0 + CELL;
    const inside = (x, y) => x >= x0 && x <= x1 && y >= 0 && y <= CELL;
    for (const e of pieces(p).path) {
      const a = e.args;
      if (e.op === 'ellipse') {
        const r = Math.max(a[2], a[3]);
        assert.ok(inside(a[0] - r, a[1] - r) && inside(a[0] + r, a[1] + r), `ellipse ${a}`);
      } else if (e.op === 'rect') {
        assert.ok(inside(a[0], a[1]) && inside(a[0] + a[2], a[1] + a[3]), `rect ${a}`);
      } else if (['moveTo', 'lineTo'].includes(e.op)) {
        assert.ok(inside(a[0], a[1]), `${e.op} ${a}`);
      } else if (e.op === 'bezierCurveTo') {
        assert.ok(inside(a[0], a[1]) && inside(a[2], a[3]) && inside(a[4], a[5]), `curve ${a}`);
      }
    }
  });
});

ok('drawAtlas: the glyph sits inside a margin that the halo can use (>= 12 % of the cell)', () => {
  parts.forEach((p, i) => {
    const x0 = i * CELL;
    let lo = [1e9, 1e9], hi = [-1e9, -1e9];
    const grow = (x, y) => { lo = [Math.min(lo[0], x - x0), Math.min(lo[1], y)];
      hi = [Math.max(hi[0], x - x0), Math.max(hi[1], y)]; };
    for (const e of pieces(p).path) {
      const a = e.args;
      if (e.op === 'ellipse') {
        const r = Math.max(a[2], a[3]);
        grow(a[0] - r, a[1] - r);
        grow(a[0] + r, a[1] + r);
      }
      else if (e.op === 'rect') { grow(a[0], a[1]); grow(a[0] + a[2], a[1] + a[3]); }
      else if (['moveTo', 'lineTo'].includes(e.op)) grow(a[0], a[1]);
      else if (e.op === 'bezierCurveTo') { grow(a[0], a[1]); grow(a[2], a[3]); grow(a[4], a[5]); }
    }
    for (const v of [lo[0], lo[1], CELL - hi[0], CELL - hi[1]]) {
      assert.ok(v >= 0.12 * CELL, `cell ${i}: margin ${v}`);
    }
  });
});

ok('drawAtlas: the strip is cleared to opaque black over exactly its own size', () => {
  const fill = log.find((e) => e.op === 'fillRect');
  assert.deepEqual(fill.args, [0, 0, 3 * CELL, CELL]);
  assert.equal(log.find((e) => e.op === '=fillStyle').args[0], '#000');
});

ok('drawAtlas: each glyph is filled red for the mask and gets a green halo (a blurred shadow)',
  () => {
    for (const p of parts) {
      const g = pieces(p);
      assert.ok(g.fills >= 2 && g.strokes >= 1);
      const styles = g.after.filter((e) => e.op === '=fillStyle').map((e) => e.args[0]);
      assert.ok(styles.some((s) => /rgb\(255, ?0, ?0\)|#f00|#ff0000/i.test(s)), styles.join());
      const shadows = g.after.filter((e) => e.op === '=shadowColor').map((e) => e.args[0]);
      assert.ok(shadows.some((s) => /rgb\(0, ?255, ?0\)|#0f0|#00ff00/i.test(s)), shadows.join());
      assert.ok(g.after.some((e) => e.op === '=shadowBlur' && e.args[0] === 0.11 * CELL));
      assert.ok(g.after.some((e) => e.op === '=lineWidth' && e.args[0] === 0.06 * CELL));
    }
  });

console.log(`${n} node checks passed`);
