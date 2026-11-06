/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Night drive's tuning on the mountains (themes/nightdrive/tune.js):
// the map between pixels and frequency and gain, the bubbles, the held curve and the morph.
// Run: node nightdrive-tune.mjs
import assert from 'node:assert/strict';
import {
  GRACE_MS, bandsOf, bubblesOf, createMorph, curveAt, diamondsOf, gMaxOf, halfPoints, heldTone,
  inBand, labelOf, mountainMap, qAtDiamond, tunable,
} from '../../../bins/svolderd/web/themes/nightdrive/tune.js';
import { axisX } from '../../../bins/svolderd/web/themes/nightdrive/axis.js';
import { responseDb } from '../../../bins/svolderd/web/core/tone-curve.js';
import { createDrag, bandSteps } from '../../../bins/svolderd/web/core/tone-drag.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const near = (a, b, eps = 1e-9) => assert.ok(Math.abs(a - b) < eps, `${a} is not ${b}`);

const W = 1200;
const H = 900;
const HORIZON = 0.56 * H;
const STEPS = {
  gain: Array.from({ length: 25 }, (_, i) => [i - 12, `${i - 12 >= 0 ? '+' : ''}${i - 12}.0 dB`]),
  q: [[0.5, '0.50'], [1, '1.00'], [4.3, '4.30']],
  bands: [
    { f: [[40, '40 Hz'], [60, '60 Hz']] },
    { f: [[250, '250 Hz'], [400, '400 Hz']] },
  ],
};
const map = mountainMap({ w: W, h: H, H: HORIZON, gMax: 12 });
const toneOf = (more = {}) => ({
  mode: 'eq',
  markers: [{ band: 1, hz: 40, db: 3 }, { band: 2, hz: 250, db: -6 }],
  response: Array.from({ length: 181 }, () => 0),
  readings: { bands: [[0, 15, 2], [0, 6, 1]] },
  ...more,
});
const page = (more = {}, deviceMore = {}) => ({
  viewer: { guest: false, writes: true },
  device: { tone: toneOf(), tone_steps: STEPS, ...deviceMore },
  ...more,
});

ok('the_map_puts_0_db_at_the_baseline_and_the_range_inside_the_mountains', () => {
  near(map.yOfDb(0), HORIZON - 0.08 * H);
  near(map.yOfDb(12), HORIZON - 0.08 * H - 0.14 * H);
  near(map.yOfDb(-12), HORIZON - 0.08 * H + 0.14 * H);
  // the whole range is inside the band that shows the curve; the lowest gain dips onto the floor
  assert.ok(map.yOfDb(-12) > HORIZON && map.yOfDb(-12) < HORIZON + 0.1 * H);
  assert.ok(inBand({ h: H, H: HORIZON }, map.yOfDb(12)) && inBand({ h: H, H: HORIZON }, map.yOfDb(-12)));
  near(map.hzAt(0), 20);
  near(map.hzAt(W), 20000, 1e-6);
  near(map.hzAt(W / 2), 20 * 1000 ** 0.5, 1e-9);
});

ok('the_map_round_trips_hz_and_db', () => {
  for (const db of [-12, -3.5, 0, 0.5, 12]) near(map.dbAt(map.yOfDb(db)), db);
  for (const x of [0, 100, 600, 1199]) near(axisX(map.hzAt(x), W), x, 1e-6);
});

ok('gmax_is_the_largest_gain_of_the_steps', () => {
  assert.equal(gMaxOf(STEPS), 12);
  assert.equal(gMaxOf({ gain: [[-6, 'x'], [4, 'y']] }), 6);
  assert.equal(gMaxOf({ gain: [] }), 12);
  assert.equal(gMaxOf(null), 12);
});

ok('the_morph_rises_over_the_mountains_and_falls_1_5_s_after_the_last_touch', () => {
  const morph = createMorph();
  assert.equal(GRACE_MS, 1500);
  let now = 0;
  const run = (secs) => {
    for (let i = 0; i < Math.round(secs * 60); i++) { now += 1000 / 60; morph.step(1 / 60, now); }
  };
  run(0.5);
  assert.equal(morph.m, 0);
  morph.over(true, now);
  run(1 / 60);
  near(morph.m, 0.12, 1e-9);
  run(0.5);
  assert.ok(morph.m > 0.97, morph.m);
  morph.over(false);
  assert.equal(morph.target, 0);
  run(1.5);
  assert.equal(morph.m, 0);
  // a drag: the target holds while it is held and for the grace after, whatever the pointer does
  morph.held(true, now);
  run(0.3);
  assert.equal(morph.target, 1);
  morph.held(false, now);
  const released = now;
  run(1.2);
  assert.equal(morph.target, 1);
  assert.ok(morph.m > 0.99);
  morph.at(released + GRACE_MS - 1);
  assert.equal(morph.target, 1);
  morph.at(released + GRACE_MS);
  assert.equal(morph.target, 0);
  run(1.0);
  assert.ok(morph.m < 0.01, morph.m);
  // a lifted finger or a wheel notch is a touch: the same grace
  morph.touch(now);
  morph.at(now + 1499);
  assert.equal(morph.target, 1);
  morph.at(now + 1501);
  assert.equal(morph.target, 0);
  // the bubbles show above 0.02 and are grabbed from 0.5: the steps in between
  const slow = createMorph();
  slow.over(true, 0);
  const ms = [];
  for (let i = 1; i <= 60; i++) ms.push(slow.step(1 / 60, i * 16.7));
  assert.ok(ms.findIndex((m) => m >= 0.02) < ms.findIndex((m) => m >= 0.5));
  assert.ok(ms.every((m, i) => i === 0 || m >= ms[i - 1]));
});

ok('a_resting_pointer_lets_the_curve_go', () => {
  const morph = createMorph();
  morph.over(true, 1000);
  morph.at(1000 + GRACE_MS - 1);
  assert.equal(morph.target, 1, 'moved a moment ago');
  morph.at(1000 + GRACE_MS);
  assert.equal(morph.target, 0, 'resting for the grace');
  // a move brings it back, and leaving with the pointer is as before
  morph.over(true, 5000);
  assert.equal(morph.target, 1);
  morph.at(5000 + GRACE_MS + 1);
  assert.equal(morph.target, 0);
  // a held drag keeps it whatever the pointer does, and a touch has its own grace
  morph.held(true, 9000);
  morph.at(20000);
  assert.equal(morph.target, 1);
  morph.held(false, 20000);
  morph.at(20000 + GRACE_MS - 1);
  assert.equal(morph.target, 1);
  morph.at(20000 + GRACE_MS);
  assert.equal(morph.target, 0);
  // the pointer's own place is not enough: only a move at or after the clock counts
  const idle = createMorph();
  idle.over(false, 100);
  idle.at(200);
  assert.equal(idle.target, 0);
});

ok('the_pace_returns_to_the_levels_once_the_morph_has_settled', () => {
  const morph = createMorph();
  let now = 0;
  const run = (secs) => {
    for (let i = 0; i < Math.round(secs * 60); i++) { now += 1000 / 60; morph.step(1 / 60, now); }
  };
  assert.equal(morph.moving, false);
  morph.over(true, now);
  assert.equal(morph.moving, true);
  run(0.1);
  assert.equal(morph.moving, true);
  // a pointer that keeps moving on the mountains: the curve is at 1 and stands still
  run(0.5);
  morph.over(true, now);
  run(0.5);
  morph.over(true, now);
  run(0.5);
  assert.ok(morph.m > 0.99);
  assert.equal(morph.moving, false);
  morph.over(false);
  assert.equal(morph.moving, true);
  run(1.5);
  assert.equal(morph.moving, false);
  assert.equal(morph.m, 0);
  // the grace counts as the target: moving while it rises and falls, not while it holds
  morph.touch(now);
  assert.equal(morph.moving, true);
  run(1.0);
  assert.equal(morph.moving, false);
  run(0.6);
  assert.equal(morph.moving, true);
});

ok('a_guest_or_no_writes_never_morphs', () => {
  assert.equal(tunable(page()), true);
  assert.equal(tunable(page({ viewer: { guest: true } })), false);
  assert.equal(tunable(page({ viewer: { writes: false } })), false);
  assert.equal(tunable(page({}, { inert: true })), false);
  const morph = createMorph();
  morph.allow(false);
  morph.over(true);
  morph.touch(0);
  morph.held(true, 0);
  for (let i = 1; i <= 120; i++) morph.step(1 / 60, i * 16.7);
  assert.equal(morph.m, 0);
  assert.equal(morph.target, 0);
  morph.allow(true);
  assert.equal(morph.target, 1);
});

ok('no_bands_never_morphs', () => {
  // an amp has no tone control; Flat with no bands lists no markers
  assert.equal(tunable({ viewer: {}, device: {} }), false);
  assert.equal(tunable({ viewer: {}, device: { tone: null, tone_steps: STEPS } }), false);
  assert.equal(tunable(page({}, { tone: toneOf({ markers: [] }) })), false);
  assert.equal(tunable(page({}, { tone_steps: { gain: STEPS.gain, q: STEPS.q, bands: [] } })), false);
  assert.equal(tunable(page({}, { tone_steps: undefined })), false);
  assert.equal(tunable(null), false);
});

ok('loudness_shows_the_curve_without_bubbles', () => {
  const loud = toneOf({ mode: 'loudness', response: Array.from({ length: 181 }, (_, i) => 4 - i / 45) });
  assert.equal(tunable(page({}, { tone: loud })), true);
  assert.deepEqual(bubblesOf(loud, map, W), []);
  const y = curveAt(loud.response, map, W);
  near(y(0), map.yOfDb(4));
  near(y(W), map.yOfDb(4 - 180 / 45), 1e-9);
  assert.ok(y(W / 2) > y(0) && y(W / 2) < y(W));
});

ok('bubbles_sit_at_each_bands_own_gain', () => {
  const bubbles = bubblesOf(toneOf(), map, W);
  assert.deepEqual(bubbles.map((b) => b.band), [1, 2]);
  near(bubbles[0].x, axisX(40, W));
  near(bubbles[0].y, map.yOfDb(3));
  near(bubbles[1].x, axisX(250, W));
  near(bubbles[1].y, map.yOfDb(-6));
  // Flat is the band's own gain too
  const flat = bubblesOf(toneOf({ mode: 'flat' }), map, W);
  near(flat[1].y, map.yOfDb(-6));
});

ok('a_held_band_draws_the_curve_and_the_bubble_from_its_raw_steps', () => {
  const tone = toneOf();
  const held = new Map([[2, { f: 1, g: 18, q: 2 }]]);
  const now = heldTone(tone, STEPS, held);
  assert.deepEqual(now.markers[0], { band: 1, hz: 40, db: 3 });
  assert.deepEqual(now.markers[1], { band: 2, hz: 400, db: 6 });
  assert.equal(now.response.length, 181);
  assert.ok(now.response.some((db) => db > 1));
  // nothing held: the bands as read (band 1 +3 dB at 40 Hz), and the model's tone when unread
  const same = heldTone(tone, STEPS, new Map());
  assert.deepEqual(same.markers, tone.markers);
  assert.equal(heldTone(toneOf({ readings: { bands: [[0, 15, 2], ['x', 6, 1]] } }), STEPS, new Map()).markers
    .length, 2);
});

ok('the_label_reads_frequency_gain_and_q_from_the_steps_words', () => {
  assert.equal(labelOf(STEPS, 2, { f: 0, g: 15, q: 1 }), '250 Hz · +3.0 dB · Q 1.00');
  assert.equal(labelOf(STEPS, 1, { f: 1, g: 6, q: 2 }), '60 Hz · -6.0 dB · Q 4.30');
});

// ---- Q from a diamond ----
const QS = [0.5, 0.7, 1, 1.4, 2, 2.8, 4, 5.6, 8];
const one = (f0, gainDb, q, hz) => responseDb([{ f0, gainDb, q }], [hz])[0];

ok('the_diamonds_sit_where_the_bell_is_at_half_its_gain', () => {
  for (const f0 of [60, 1000, 4000, 12000]) {
    for (const q of [0.5, 1, 4]) {
      for (const gainDb of [-9, 6]) {
        const { lo, hi } = halfPoints({ f0, gainDb, q });
        assert.ok(lo <= f0 && hi >= f0, `${f0} ${q} ${gainDb}: ${lo} ${hi}`);
        // an end of the axis is where the bell never comes down to half
        if (lo > 20) near(one(f0, gainDb, q, lo), gainDb / 2, 0.05);
        if (hi < 20000) near(one(f0, gainDb, q, hi), gainDb / 2, 0.05);
        assert.ok(lo > 20 || one(f0, gainDb, q, 20) * Math.sign(gainDb) >= Math.abs(gainDb) / 2 - 0.05);
      }
    }
  }
});

ok('a_diamond_at_a_steps_half_point_picks_that_step', () => {
  let tried = 0;
  for (const band of [{ f0: 250, gainDb: 6 }, { f0: 1000, gainDb: -9 }, { f0: 4000, gainDb: 3 }]) {
    for (const side of ['lo', 'hi']) {
      const points = QS.map((q) => halfPoints({ ...band, q })[side]);
      QS.forEach((q, i) => {
        // a step whose half point another step shares (an axis end) is the lowest index's
        if (points.indexOf(points[i]) !== i) return;
        assert.equal(qAtDiamond(QS, band, side, points[i]), i, `${band.f0} ${side} step ${i}`);
        // and a little off it, on either side, still names it
        assert.equal(qAtDiamond(QS, band, side, points[i] * 1.002), i);
        assert.equal(qAtDiamond(QS, band, side, points[i] / 1.002), i);
        tried++;
      });
    }
  }
  assert.ok(tried >= 30, tried);
  // a tie goes to the lower index: two equal Q steps share their half points
  assert.equal(qAtDiamond([1, 1, 2], { f0: 1000, gainDb: 6 }, 'hi', halfPoints({ f0: 1000, gainDb: 6, q: 1 }).hi), 0);
});

ok('the_wider_the_diamonds_the_lower_the_q', () => {
  const band = { f0: 1000, gainDb: 6 };
  const widths = QS.map((q) => { const p = halfPoints({ ...band, q }); return Math.log(p.hi / p.lo); });
  for (let i = 1; i < widths.length; i++) assert.ok(widths[i] < widths[i - 1], `${i}: ${widths}`);
  // dragging the upper diamond out names a lower Q, in
  const inner = halfPoints({ ...band, q: 4 }).hi;
  const outer = halfPoints({ ...band, q: 1 }).hi;
  assert.ok(qAtDiamond(QS, band, 'hi', outer) < qAtDiamond(QS, band, 'hi', inner));
  // and the lower diamond the same way: out is to the left
  assert.ok(qAtDiamond(QS, band, 'lo', halfPoints({ ...band, q: 1 }).lo)
    < qAtDiamond(QS, band, 'lo', halfPoints({ ...band, q: 4 }).lo));
});

ok('a_flat_band_has_its_diamonds_at_the_analog_width', () => {
  for (const [f0, q] of [[250, 1], [1000, 0.5], [1000, 4]]) {
    const { lo, hi } = halfPoints({ f0, gainDb: 0, q });
    const octaves = Math.asinh(1 / (2 * q)) / Math.LN2;
    near(lo, f0 * 2 ** -octaves, 1e-9 * f0);
    near(hi, f0 * 2 ** octaves, 1e-9 * f0);
  }
  // under 0.05 dB the same, at a gain of 0.04
  near(halfPoints({ f0: 1000, gainDb: 0.04, q: 1 }).hi, halfPoints({ f0: 1000, gainDb: 0, q: 1 }).hi);
});

ok('the_upper_diamond_stops_at_20_khz', () => {
  const wide = halfPoints({ f0: 18000, gainDb: 6, q: 0.5 });
  assert.equal(wide.hi, 20000);
  assert.ok(wide.lo > 8000 && wide.lo < 18000);
  const low = halfPoints({ f0: 25, gainDb: 6, q: 0.5 });
  assert.equal(low.lo, 20);
  // dragging past the end names the widest step that reaches it (the lower index of those that do)
  assert.equal(qAtDiamond(QS, { f0: 18000, gainDb: 6 }, 'hi', 30000), 0);
  assert.equal(qAtDiamond(QS, { f0: 18000, gainDb: 6 }, 'hi', 19990), 0);
});

ok('the_diamonds_sit_on_the_curve_at_the_half_gain_points', () => {
  const band = { f0: 1000, gainDb: 6, q: 1 };
  // a curve that is not the band's own: the two diamonds follow it, whatever the half gain's height
  const curve = (x) => map.yOfDb(2) - x / 100;
  const [lo, hi] = diamondsOf(band, map, W, curve);
  assert.deepEqual([lo.side, hi.side], ['lo', 'hi']);
  const half = halfPoints(band);
  near(lo.x, axisX(half.lo, W));
  near(hi.x, axisX(half.hi, W));
  near(lo.y, curve(lo.x));
  near(hi.y, curve(hi.x));
  assert.ok(lo.x < axisX(1000, W) && hi.x > axisX(1000, W));
  // with no curve they fall back to the half gain's own height
  near(diamondsOf(band, map, W)[0].y, map.yOfDb(3));
});

ok('a_flat_band_s_highlight_spans_the_analog_width_around_its_bubble', () => {
  const band = { f0: 1000, gainDb: 0, q: 1 };
  const [lo, hi] = diamondsOf(band, map, W, () => 100);
  assert.ok(lo.x < axisX(1000, W) && hi.x > axisX(1000, W));
  near(lo.x, axisX(halfPoints(band).lo, W));
});

ok('the_bands_are_read_from_the_tone_and_the_steps', () => {
  const bands = bandsOf(toneOf(), STEPS, new Map());
  assert.deepEqual(bands, [{ f0: 40, gainDb: 3, q: 4.3 }, { f0: 250, gainDb: -6, q: 1 }]);
  assert.deepEqual(bandsOf(toneOf(), STEPS, new Map([[2, { f: 1, g: 18, q: 0 }]]))[1], { f0: 400, gainDb: 6, q: 0.5 });
  assert.equal(bandsOf(toneOf({ readings: { bands: [[0, 15, 2], ['x', 6, 1]] } }), STEPS, new Map()), null);
});

ok('a_diamond_drag_lands_on_the_step_it_names', () => {
  const steps = {
    gain: STEPS.gain,
    q: QS.map((q) => [q, String(q)]),
    bands: [{ f: [[1000, '1 kHz']] }],
  };
  const band = { f0: 1000, gainDb: 6, q: 2 };
  for (const side of ['lo', 'hi']) {
    for (const target of [0, 3, 5, 8]) {
      const writes = [];
      const drag = createDrag({
        band: 1,
        gesture: 'pinch',
        start: { f: 0, g: 18, q: 4 },
        press: { distance: 1 },
        plot: { width: W, height: H },
        steps: bandSteps(steps, 1),
        write: async (param, raw) => { writes.push([param, raw]); return {}; },
        on: { held() {}, answer() {}, toast() {}, ended() {} },
      });
      // the pointer is at the half point of step `target`: the Q asked is that step's, exactly
      const at = qAtDiamond(QS, band, side, halfPoints({ ...band, q: QS[target] })[side]);
      drag.spread(QS[4] / QS[at]);
      assert.deepEqual(drag.held, { f: 0, g: 18, q: at });
      assert.equal(at, target);
    }
  }
});

console.log(`nightdrive-tune.mjs: ${n} passed`);
