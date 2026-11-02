/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the smoke's sources: pointer, dial dot, volume puff, meters, steps.
// Run: node sources.mjs
import assert from 'node:assert/strict';
import {
  TUNING, dialDot, meterEmission, pointerSplat, stepOf, stepPuff, uvOf,
} from '../../../bins/svolderd/web/themes/ember/smoke-model.js';
import { lookTests } from './look-values.mjs';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const okLook = lookTests(TUNING, ok);
const near = (a, b, e = 1e-9) => assert.ok(Math.abs(a - b) <= e, `${a} vs ${b}`);
const still = { fx: 0, fy: 0, dye: 0 };

ok('TUNING: the new bounds exist, are finite and the puff cannot beat the push cap', () => {
  for (const k of ['maxPush', 'pointerPush', 'pointerRadius', 'pointerAdd', 'pointerGapSecs',
    'pointerDyePerSide', 'pointerDyeMax', 'maxPointerSplats', 'puffPush', 'puffFullSize',
    'puffFloor', 'puffDye', 'puffRadius', 'puffAdd', 'maxPuffs', 'meterRed', 'meterDye',
    'meterDrift', 'meterRedKick', 'meterRadius', 'meterPull', 'meterEaseSecs', 'meterFloor']) {
    assert.ok(Number.isFinite(TUNING[k]) && TUNING[k] > 0, k);
  }
  assert.ok(TUNING.puffPush <= TUNING.maxPush);
  assert.ok(TUNING.meterDrift + TUNING.meterRedKick <= TUNING.maxPush);
});
ok('pointerSplat: the force follows the motion and scales with speed over the short side', () => {
  const a = pointerSplat({ dx: 10, dy: 0, dt: 0.1, shortSide: 1000, pressed: false });
  near(a.fx, (100 / 1000) * TUNING.pointerPush);
  near(a.fy, 0);
  const b = pointerSplat({ dx: 20, dy: 0, dt: 0.1, shortSide: 1000, pressed: false });
  near(b.fx, 2 * a.fx);
  const c = pointerSplat({ dx: 10, dy: 0, dt: 0.1, shortSide: 500, pressed: false });
  near(c.fx, 2 * a.fx);
  const d = pointerSplat({ dx: -3, dy: 4, dt: 0.1, shortSide: 1000, pressed: false });
  near(Math.hypot(d.fx, d.fy), (50 / 1000) * TUNING.pointerPush);
  assert.ok(d.fx < 0 && d.fy > 0);
  near(d.fx / d.fy, -3 / 4);
});
ok('pointerSplat: clamped to maxPush, direction kept', () => {
  const s = pointerSplat({ dx: 400, dy: 300, dt: 0.001, shortSide: 800, pressed: false });
  near(Math.hypot(s.fx, s.fy), TUNING.maxPush, 1e-9);
  near(s.fx / s.fy, 4 / 3);
});
ok('pointerSplat: dye only when pressed, by the distance moved, capped', () => {
  const free = pointerSplat({ dx: 10, dy: 0, dt: 0.016, shortSide: 800, pressed: false });
  assert.equal(free.dye, 0);
  const held = pointerSplat({ dx: 10, dy: 0, dt: 0.016, shortSide: 800, pressed: true });
  near(held.dye, (10 / 800) * TUNING.pointerDyePerSide);
  const same = pointerSplat({ dx: 10, dy: 0, dt: 0.0001, shortSide: 800, pressed: true });
  near(same.dye, held.dye); // the distance, not the speed, so the event rate does not matter
  const far = pointerSplat({ dx: 800, dy: 0, dt: 0.016, shortSide: 800, pressed: true });
  assert.equal(far.dye, TUNING.pointerDyeMax);
  assert.equal(pointerSplat({ dx: 0, dy: 0, dt: 0.016, shortSide: 800, pressed: true }).dye, 0);
});
ok('pointerSplat: nothing for dt <= 0, a bad size or a non-number', () => {
  for (const dt of [0, -1, NaN]) {
    assert.deepEqual(pointerSplat({ dx: 9, dy: 9, dt, shortSide: 800, pressed: true }), still);
  }
  for (const shortSide of [0, -5, NaN]) {
    assert.deepEqual(pointerSplat({ dx: 9, dy: 9, dt: 0.1, shortSide, pressed: true }), still);
  }
  assert.deepEqual(pointerSplat({ dx: NaN, dy: 9, dt: 0.1, shortSide: 800, pressed: true }), still);
  assert.deepEqual(pointerSplat({ dx: Infinity, dy: 9, dt: 0.1, shortSide: 800, pressed: true }),
    still);
});
ok('dialDot: 0 is bottom left, 0.5 the top, 1 bottom right of the arc (screen y down)', () => {
  const at = { x: 200, y: 300, radius: 80 };
  const s = Math.SQRT1_2;
  const lo = dialDot(at, 0);
  near(lo.x, 200 - 80 * s); near(lo.y, 300 + 80 * s);
  const mid = dialDot(at, 0.5);
  near(mid.x, 200); near(mid.y, 220);
  const hi = dialDot(at, 1);
  near(hi.x, 200 + 80 * s); near(hi.y, 300 + 80 * s);
});
ok('dialDot: the tangent is the clockwise unit vector', () => {
  const at = { x: 0, y: 0, radius: 10 };
  const top = dialDot(at, 0.5);
  near(top.tx, 1); near(top.ty, 0); // at the top, clockwise is to the right
  const lo = dialDot(at, 0); // -135 degrees: clockwise heads up and to the left
  near(lo.tx, -Math.SQRT1_2); near(lo.ty, -Math.SQRT1_2);
  const hi = dialDot(at, 1); // +135 degrees: clockwise heads down and to the left
  near(hi.tx, -Math.SQRT1_2); near(hi.ty, Math.SQRT1_2);
  for (const t of [0, 0.2, 0.5, 0.8, 1]) {
    const d = dialDot(at, t);
    near(Math.hypot(d.tx, d.ty), 1);
    near(d.tx * (d.x - 0) + d.ty * (d.y - 0), 0, 1e-9); // perpendicular to the radius
  }
});
ok('dialDot: a fraction outside 0 to 1 stays on the arc; a bad one is the start', () => {
  const at = { x: 0, y: 0, radius: 10 };
  assert.deepEqual(dialDot(at, -4), dialDot(at, 0));
  assert.deepEqual(dialDot(at, 9), dialDot(at, 1));
  assert.deepEqual(dialDot(at, NaN), dialDot(at, 0));
});
ok('stepPuff: at the dot, along the tangent times the direction', () => {
  const dot = dialDot({ x: 100, y: 100, radius: 50 }, 0.5);
  const up = stepPuff(dot, 1, 0.05, 0.5);
  near(up.x, dot.x); near(up.y, dot.y);
  assert.ok(up.fx > 0); near(up.fy, 0);
  const down = stepPuff(dot, -1, 0.05, 0.5);
  near(down.fx, -up.fx); near(down.dye, up.dye);
  assert.deepEqual(stepPuff(dot, 0, 0.05, 0.5).fx, 0);
});
ok('stepPuff: grows with size and with level, and is capped', () => {
  const dot = dialDot({ x: 0, y: 0, radius: 50 }, 0.5);
  const f = (size, level) => stepPuff(dot, 1, size, level);
  assert.ok(f(0.02, 0.5).fx < f(0.04, 0.5).fx && f(0.02, 0.5).dye < f(0.04, 0.5).dye);
  assert.ok(f(0.05, 0.1).fx < f(0.05, 0.9).fx && f(0.05, 0.1).dye < f(0.05, 0.9).dye);
  assert.ok(f(0.05, 0.06).fx > 0, 'a muted step still puffs a little');
  const big = f(5, 1);
  near(big.fx, TUNING.puffPush); near(big.dye, TUNING.puffDye);
  assert.deepEqual(f(50, 7), big);
  assert.ok(Math.hypot(big.fx, big.fy) <= TUNING.maxPush);
  assert.deepEqual(f(0, 1).dye, 0);
  assert.deepEqual(f(NaN, 1).dye, 0);
});
okLook('meterEmission: grows with the fraction; red adds a kick from meterRed up', () => {
  const lo = meterEmission(0.2, TUNING.meterRed);
  const mid = meterEmission(0.6, TUNING.meterRed);
  assert.ok(lo.dye > 0 && mid.dye > lo.dye && mid.drift > lo.drift);
  near(mid.dye, TUNING.meterDye * 0.6);
  near(mid.drift, TUNING.meterDrift * 0.6);
  const under = meterEmission(0.92, 0.93);
  near(under.drift, TUNING.meterDrift * 0.92);
  const red = meterEmission(0.93, 0.93);
  near(red.drift, TUNING.meterDrift * 0.93 + TUNING.meterRedKick);
  assert.ok(red.drift > under.drift + TUNING.meterRedKick * 0.9);
  const full = meterEmission(1, 0.93);
  near(full.dye, TUNING.meterDye);
  assert.deepEqual(meterEmission(0, 0.93), { dye: 0, drift: 0 });
  assert.deepEqual(meterEmission(NaN, 0.93), { dye: 0, drift: 0 });
  assert.deepEqual(meterEmission(null, 0.93), { dye: 0, drift: 0 });
  assert.deepEqual(meterEmission(3, 0.93), full);
});
ok('stepOf: no previous, unread on either side, unchanged: null', () => {
  const r = (raw) => ({ raw, lo: 0, hi: 100 });
  assert.equal(stepOf(null, r(10)), null);
  assert.equal(stepOf(undefined, r(10)), null);
  assert.equal(stepOf(r(null), r(10)), null);
  assert.equal(stepOf(r(10), r(null)), null);
  assert.equal(stepOf(r(10), r(10)), null);
  assert.equal(stepOf(r(10), null), null);
  assert.equal(stepOf({ raw: 1, lo: 5, hi: 5 }, { raw: 2, lo: 5, hi: 5 }), null);
});
ok('stepOf: up, down and muted both sides, size as a share of the range', () => {
  const r = (raw) => ({ raw, lo: 20, hi: 120 });
  assert.deepEqual(stepOf(r(50), r(53)), { direction: 1, size: 0.03 });
  const d = stepOf(r(53), r(50));
  assert.equal(d.direction, -1); near(d.size, 0.03);
  assert.equal(stepOf(r(20), r(120)).size, 1);
  // a change while muted is a step like any other (the page sends level 0.06 with it)
  assert.deepEqual(stepOf(r(60), r(61)), { direction: 1, size: 0.01 });
});
ok('uvOf: a viewport point to the grid (y up)', () => {
  assert.deepEqual(uvOf({ x: 100, y: 200 }, [400, 800]), { x: 0.25, y: 0.75 });
});
console.log(`${n} source checks passed`);
