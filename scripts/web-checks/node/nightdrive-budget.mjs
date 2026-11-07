/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Night drive's pixel budget and its step-down (themes/nightdrive/
// budget.js), on a fake clock and fake draw times: the density and megapixel caps, the steps down on
// slow draws, the 30 a second cap at the floor and the way back after 10 s under budget.
// Run: node nightdrive-budget.mjs
import assert from 'node:assert/strict';
import { BUDGET, baseScale, createBudget, paceUnder } from '../../../bins/svolderd/web/themes/nightdrive/budget.js';
import { keepFrame } from '../../../bins/svolderd/web/themes/nightdrive/motion.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const near = (a, b, eps = 1e-9) => assert.ok(Math.abs(a - b) <= eps, `${a} is not ${b}`);

/** Feed `count` draws of `ms` each, one every `gap` s from `from`; the number of changes of scale. */
function feed(budget, count, ms, from, gap = 1 / 60, full = false) {
  let changes = 0;
  let at = from;
  for (let i = 0; i < count; i++) {
    at += gap;
    if (budget.sample(ms, at, full)) changes++;
  }
  return { changes, at };
}

/** A budget whose wet road has been dropped already (by 60 slow draws long before the case starts), so the
 * case sees the scale and the rate go as they did before there was a wet road. */
function dry(options) {
  const b = createBudget(options);
  b.size(390, 844, 3);
  if (b.wet) {
    feed(b, 60, 30, -1000);
    assert.equal(b.wet, false);
  }
  return b;
}

ok('the_budget_caps_the_density_at_2_and_the_canvas_at_2_5_megapixels', () => {
  near(baseScale(1280, 800, 1), 1);
  near(baseScale(390, 844, 3), 2, 1e-9);
  near(baseScale(1280, 800, 2), Math.sqrt(2.5e6 / (1280 * 800)));
  const wide = baseScale(1920, 1200, 3);
  assert.ok(wide * wide * 1920 * 1200 <= 2.5e6 + 1);
  assert.equal(BUDGET.dpr, 2);
  const b = dry();
  b.size(390, 844, 3);
  near(b.scale, 2);
  assert.equal(b.capped, false);
});

ok('a_slow_mean_steps_the_scale_down_by_0_75_to_one_backing_px_and_then_caps_the_rate', () => {
  const b = dry();
  b.size(390, 844, 3);
  assert.equal(feed(b, 59, 30, 0).changes, 0, 'not before 60 draws');
  assert.equal(b.sample(30, 1), true, 'the 60th draw steps down');
  near(b.scale, 1.5);
  assert.equal(b.step, 1);
  let at = 1;
  at = feed(b, 59, 30, at).at;
  assert.equal(b.scale, 1.5, 'a fresh window of 60 is needed');
  assert.equal(b.sample(30, at + 0.02), true);
  near(b.scale, 1.125);
  at = feed(b, 60, 30, at + 0.02).at;
  near(b.scale, 1, 1e-9);
  assert.equal(b.capped, false, 'the floor is reached first, the rate after');
  feed(b, 60, 30, at);
  assert.equal(b.capped, true);
  near(b.scale, 1);
  assert.ok(1000 / BUDGET.fps > 33 && 1000 / BUDGET.fps < 34);
});

ok('one_slow_draw_among_fast_ones_changes_nothing', () => {
  const b = dry();
  b.size(390, 844, 3);
  for (let i = 0; i < 300; i++) b.sample(i % 60 === 0 ? 80 : 3, i / 60);
  assert.equal(b.step, 0);
  assert.equal(b.capped, false);
});

ok('the_scale_comes_back_only_after_10_s_comfortably_under_budget', () => {
  const b = dry();
  b.size(390, 844, 3);
  let at = feed(b, 60 * 4, 30, 0).at;
  assert.equal(b.capped, true);
  // 9 s of fast draws: nothing yet
  at = feed(b, 540, 3, at, 1 / 60).at;
  assert.equal(b.capped, true);
  assert.equal(b.step, 3);
  // past 10 s: the rate first, then the scale one step at a time, each after another 10 s
  at = feed(b, 120, 3, at, 1 / 60).at;
  assert.equal(b.capped, false);
  assert.equal(b.step, 3);
  at = feed(b, 60 * 12, 3, at, 1 / 60).at;
  assert.equal(b.step, 2);
  at = feed(b, 60 * 12, 3, at, 1 / 60).at;
  assert.equal(b.step, 1);
  feed(b, 60 * 12, 3, at, 1 / 60);
  assert.equal(b.step, 0);
  near(b.scale, 2);
});

ok('a_middling_mean_neither_steps_down_nor_counts_as_comfortable', () => {
  const b = dry();
  b.size(390, 844, 3);
  let at = feed(b, 60, 30, 0).at;
  assert.equal(b.step, 1);
  // 9 ms draws for 30 s: over the comfortable mark, under the slow one
  at = feed(b, 60 * 30, 9, at).at;
  assert.equal(b.step, 1);
  feed(b, 60 * 12, 3, at);
  assert.equal(b.step, 0);
});

ok('a_pause_between_draws_is_not_comfortable_time', () => {
  const b = dry();
  b.size(390, 844, 3);
  let at = feed(b, 60, 30, 0).at;
  assert.equal(b.step, 1);
  // 60 fast draws start the clock of the way back; 4 s later the tab is hidden for 30 s
  at = feed(b, 60 * 5, 3, at).at;
  assert.equal(b.step, 1);
  assert.equal(b.sample(3, at + 30), false, 'the first draw after the pause steps nothing up');
  assert.equal(b.step, 1);
  // the clock starts afresh: 10 s of fresh good draws still bring it back
  feed(b, 60 * 12, 3, at + 30);
  assert.equal(b.step, 0);
});

ok('a_phone_that_is_never_slow_stays_at_its_capped_density', () => {
  const b = dry();
  b.size(390, 844, 3);
  feed(b, 60 * 60, 2, 0);
  near(b.scale, 2);
  assert.equal(b.step, 0);
});

ok('too_few_draws_a_second_for_3_s_steps_down_while_every_frame_is_asked_for_though_each_draw_is_fast', () => {
  const b = dry();
  b.size(390, 844, 3);
  // 15 a second, 3 ms each: the mean is far under the slow mark
  const first = feed(b, 15 * 3, 3, 0, 1 / 15, true);
  assert.equal(first.changes, 0, 'not within 3 s: the first second does not count');
  assert.equal(b.step, 0);
  const more = feed(b, 15 * 2, 3, first.at, 1 / 15, true);
  assert.equal(more.changes, 1);
  assert.equal(b.step, 1);
  near(b.scale, 1.5);
  // and again 4 s later, as a slow mean does, down to the floor and then the rate
  let at = more.at;
  at = feed(b, 15 * 5, 3, at, 1 / 15, true).at;
  assert.equal(b.step, 2);
  at = feed(b, 15 * 5, 3, at, 1 / 15, true).at;
  at = feed(b, 15 * 5, 3, at, 1 / 15, true).at;
  near(b.scale, 1);
  assert.equal(b.capped, true);
});

ok('a_rate_under_24_means_nothing_when_the_scene_asks_for_less_than_every_frame_or_the_draws_are_comfortable', () => {
  const slowAsked = dry();
  slowAsked.size(390, 844, 3);
  assert.equal(feed(slowAsked, 15 * 10, 3, 0, 1 / 15, false).changes, 0, 'a paced scene draws seldom by choice');
  assert.equal(slowAsked.step, 0);
  const fine = dry();
  fine.size(390, 844, 3);
  assert.equal(feed(fine, 60 * 20, 3, 0, 1 / 60, true).changes, 0, '60 a second is no trouble');
  const cap = dry();
  cap.size(390, 844, 3);
  assert.equal(feed(cap, 30 * 20, 3, 0, 1 / 30, true).changes, 0, 'the 30 a second cap is drawn at 30, over 24');
});

ok('a_brief_dip_under_3_s_is_not_a_step_down', () => {
  const b = dry();
  b.size(390, 844, 3);
  let at = feed(b, 60 * 2, 3, 0, 1 / 60, true).at;
  at = feed(b, 15 * 2, 3, at, 1 / 15, true).at;
  at = feed(b, 60 * 4, 3, at, 1 / 60, true).at;
  assert.equal(b.step, 0);
  assert.equal(b.capped, false);
});

ok('after_a_step_down_for_the_rate_the_way_back_stays_shut_for_a_minute', () => {
  const b = dry();
  b.size(390, 844, 3);
  let at = feed(b, 15 * 5, 3, 0, 1 / 15, true).at;
  assert.equal(b.step, 1);
  // 30 s of 60 a second with fast draws would bring a slow mean's step back after 10 s; here not before 60 s
  at = feed(b, 60 * 40, 3, at, 1 / 60, true).at;
  assert.equal(b.step, 1, 'still down after 40 s of comfort');
  feed(b, 60 * 40, 3, at, 1 / 60, true);
  assert.equal(b.step, 0, 'back once the minute is over and 10 s of comfort have passed');
});

ok('a_pause_between_draws_restarts_the_rate_too', () => {
  const b = dry();
  b.size(390, 844, 3);
  let at = feed(b, 15 * 2, 3, 0, 1 / 15, true).at;
  // a hidden tab: 20 s later the draws begin again, and the first second of them is not counted
  at += 20;
  const after = feed(b, 15 * 3, 3, at, 1 / 15, true);
  assert.equal(after.changes, 0);
  assert.equal(b.step, 0);
});

ok('a_phone_starts_at_the_rate_s_cap_and_the_cap_lifts_after_10_s_under_budget', () => {
  const b = createBudget({ coarse: true });
  b.size(390, 844, 3);
  assert.equal(b.capped, true);
  near(b.scale, 2);
  assert.equal(b.step, 0);
  let at = feed(b, 30 * 9, 3, 0, 1 / 30, true).at;
  assert.equal(b.capped, true, 'not before 10 s');
  at = feed(b, 30 * 4, 3, at, 1 / 30, true).at;
  assert.equal(b.capped, false);
  assert.equal(dry().capped, false, 'a pointer that is not coarse starts uncapped');
});

ok('the_30_a_second_cap_keeps_its_cadence_at_any_screen_rate_and_never_trips_the_rate_trigger', () => {
  const pace = 1000 / BUDGET.fps;
  for (const hz of [50, 60, 72, 90, 120, 144]) {
    const b = createBudget({ coarse: true });
    b.size(390, 844, 3);
    let drawnAt = -Infinity;
    let drawn = 0;
    const frame = 1000 / hz;
    for (let t = 1000; t < 1000 + 20000; t += frame) {
      if (!keepFrame({ sinceMs: t - drawnAt, frameMs: frame, paceMs: pace })) continue;
      drawnAt = t;
      drawn++;
      b.sample(3, t / 1000, true);
    }
    const perSecond = drawn / 20;
    assert.ok(perSecond >= (hz <= 50 ? 23 : 28) && perSecond <= 1.3 * BUDGET.fps * (hz === 72 ? 1.3 : 1), `${hz} Hz draws ${perSecond.toFixed(1)} a second`);
    assert.equal(b.step, 0, `${hz} Hz: no step down`);
  }
  assert.equal(keepFrame({ sinceMs: 5, frameMs: 16, paceMs: 0 }), true, 'no pace: every frame');
});

ok('a_phone_that_has_come_up_from_the_start_cap_draws_at_most_60_a_second_at_any_screen_rate', () => {
  assert.equal(BUDGET.phoneFps, 60);
  assert.equal(paceUnder({ asked: 0, capped: true, coarse: true }), 1000 / 30, 'the start cap');
  assert.equal(paceUnder({ asked: 0, capped: false, coarse: true }), 1000 / 60, 'a phone, up from it');
  assert.equal(paceUnder({ asked: 0, capped: false, coarse: false }), 0, 'a fine pointer keeps the screen rate');
  assert.equal(paceUnder({ asked: 125, capped: false, coarse: true }), 125, 'a slower ask stays');
  const pace = paceUnder({ asked: 0, capped: false, coarse: true });
  for (const hz of [60, 90, 120, 144]) {
    const frame = 1000 / hz;
    for (const jitter of [0, 0.8, 1.4]) {
      let drawnAt = -Infinity;
      let drawn = 0;
      for (let k = 0, t = 1000; t < 21000; k++, t += frame + (k % 2 ? jitter : -jitter)) {
        if (!keepFrame({ sinceMs: t - drawnAt, frameMs: frame, paceMs: pace, atMost: true })) continue;
        drawnAt = t;
        drawn++;
      }
      const perSecond = drawn / 20;
      assert.ok(perSecond <= 66.8, `${hz} Hz (jitter ${jitter}) draws ${perSecond.toFixed(1)} a second`);
      assert.ok(perSecond >= (hz === 60 || hz === 120 ? 55 : 40), `${hz} Hz (jitter ${jitter}) draws ${perSecond.toFixed(1)} a second`);
    }
  }
  // the nearest-frame rule alone would give 72 on a 144 Hz screen
  assert.equal(keepFrame({ sinceMs: 13.9, frameMs: 6.94, paceMs: pace }), true);
  assert.equal(keepFrame({ sinceMs: 13.9, frameMs: 6.94, paceMs: pace, atMost: true }), false);
});

ok('the_wet_road_is_the_first_thing_a_slow_mean_drops_before_the_scale_and_then_the_rate', () => {
  const b = createBudget();
  b.size(390, 844, 3);
  assert.equal(b.wet, true, 'a desktop starts with it');
  const first = feed(b, 60, 30, 0);
  assert.equal(first.changes, 0, 'dropping it changes no scale: nothing to size again');
  assert.equal(b.wet, false);
  assert.equal(b.step, 0);
  near(b.scale, 2);
  assert.equal(b.capped, false);
  const second = feed(b, 60, 30, first.at);
  assert.equal(second.changes, 1);
  assert.equal(b.step, 1, 'the scale goes next');
  near(b.scale, 1.5);
  assert.equal(b.capped, false);
  let at = feed(b, 60, 30, second.at).at;
  at = feed(b, 60, 30, at).at;
  near(b.scale, 1);
  assert.equal(b.capped, false);
  feed(b, 60, 30, at);
  assert.equal(b.capped, true, 'the rate last');
});

ok('too_few_draws_a_second_drops_the_wet_road_first_too', () => {
  const b = createBudget();
  b.size(390, 844, 3);
  const first = feed(b, 15 * 5, 3, 0, 1 / 15, true);
  assert.equal(b.wet, false);
  assert.equal(b.step, 0, 'the scale is still whole');
  const more = feed(b, 15 * 5, 3, first.at, 1 / 15, true);
  assert.equal(b.step, 1);
  assert.equal(more.changes, 1);
});

ok('a_phone_starts_without_the_wet_road_and_it_comes_last_after_the_cap_and_the_scale', () => {
  const b = createBudget({ coarse: true });
  b.size(390, 844, 3);
  assert.equal(b.wet, false);
  assert.equal(b.capped, true);
  let at = feed(b, 30 * 13, 3, 0, 1 / 30, true).at;
  assert.equal(b.capped, false, 'the cap goes first');
  assert.equal(b.wet, false, 'not yet');
  at = feed(b, 30 * 13, 3, at, 1 / 30, true).at;
  assert.equal(b.wet, true, 'then the wet road, once the budget allows');
  // a phone that stepped down comes back the other way: the scale before the wet road
  const slow = createBudget({ coarse: true });
  slow.size(390, 844, 3);
  let t = feed(slow, 60 * 2, 30, 0).at;
  assert.ok(slow.step >= 1);
  const before = slow.step;
  t = feed(slow, 60 * 14 * (before + 2), 3, t, 1 / 60).at;
  assert.equal(slow.step, 0);
  assert.equal(slow.wet, true);
});

ok('while_idle_the_wet_road_returns_once_the_draws_measure_under_budget_and_a_step_down_still_drops_it', () => {
  const b = createBudget();
  b.size(390, 844, 3);
  b.setIdle(false);
  let at = feed(b, 60, 30, 0).at;
  assert.equal(b.wet, false, 'dropped while playing');
  // paused, the draws under budget: back after one window of them, with no 180 s hold
  b.setIdle(true);
  assert.equal(b.wet, false, 'not before the idle draws are measured');
  at = feed(b, 60, 3, at, 1 / 8).at;
  assert.equal(b.wet, true, 'paused under budget: the wet road is back after one window');
  // playing again keeps it while the draws are fine
  b.setIdle(false);
  at = feed(b, 60 * 5, 3, at, 1 / 60).at;
  assert.equal(b.wet, true, 'still there while playing under budget');
  // if it pushes the draws over budget, a step-down drops it and the hold applies as before
  at = feed(b, 60, 30, at, 1 / 60).at;
  assert.equal(b.wet, false, 'over budget: dropped');
  b.setIdle(true);
  at = feed(b, 60 * 2, 3, at, 1 / 8).at;
  assert.equal(b.wet, true, 'dropped while playing, so a pause under budget brings it back at once');
  // paused and over budget: dropped again, and the idle way back is shut (no coming and going every window)
  at = feed(b, 60, 30, at, 1 / 8).at;
  assert.equal(b.wet, false, 'paused over budget: dropped again');
  at = feed(b, 60 * 3, 3, at, 1 / 8).at;
  assert.equal(b.wet, false, 'and held, as after any step-down');
});

ok('a_wet_road_dropped_while_playing_is_held_for_wetHeldS_when_playing_goes_on', () => {
  const b = createBudget();
  b.size(390, 844, 3);
  b.setIdle(false);
  let at = feed(b, 60, 30, 0).at;
  at = feed(b, 60 * 60, 3, at, 1 / 60).at;
  assert.equal(b.wet, false, 'not at 60 s');
  at = feed(b, 60 * 140, 3, at, 1 / 60).at;
  assert.equal(b.wet, true, 'after wetHeldS');
});

ok('the_wet_road_stays_off_for_3_minutes_after_a_step_down_dropped_it', () => {
  const b = createBudget();
  b.size(390, 844, 3);
  let at = feed(b, 60, 30, 0).at;
  assert.equal(b.wet, false);
  // good draws for a minute and a half: still off (the way back is shut for 60 s, the wet road's own wait for 180)
  at = feed(b, 60 * 90, 3, at, 1 / 60).at;
  assert.equal(b.wet, false);
  at = feed(b, 60 * 60, 3, at, 1 / 60).at;
  assert.equal(b.wet, false, 'not before 180 s');
  at = feed(b, 60 * 60, 3, at, 1 / 60).at;
  assert.equal(b.wet, true);
  // and it is dropped again the moment draws are slow
  feed(b, 60, 30, at);
  assert.equal(b.wet, false);
  assert.equal(BUDGET.wetHeldS, 180);
});

ok('a_probe_lifts_the_cap_and_a_slow_scene_is_stepped_down_within_a_second_each_step', () => {
  const b = createBudget({ coarse: true });
  b.size(390, 844, 3);
  assert.equal(b.capped, true);
  b.probe();
  // draws of 16 ms at 60 a second under no cap: the cap is lifted and nothing is lowered
  let at = feed(b, 120, 3, 100, 1 / 60, true).at;
  assert.equal(b.capped, false);
  assert.equal(b.step, 0);
  // the scene turns slow (30 ms a draw, 20 a second): a step within 0.5 s of draws and then the next
  b.probe();
  const began = at;
  for (let i = 0; i < 400 && b.step < 2; i++) {
    at += 1 / 20;
    b.sample(30, at, true);
  }
  assert.ok(b.step >= 1, 'stepped');
  assert.ok(at - began < 3, `settled in ${(at - began).toFixed(2)} s`);
});

ok('a_probe_holds_the_limits_a_good_scene_stays_where_it_is_and_a_held_cap_stays', () => {
  const good = createBudget({ coarse: true });
  good.size(390, 844, 3);
  good.probe();
  feed(good, 240, 4, 0, 1 / 60, true);
  assert.deepEqual([good.capped, good.step, good.wet], [false, 0, false]);
  // a step-down on the rate holds the way back shut: a probe afterwards does not lift the cap
  const slow = createBudget({ coarse: true });
  slow.size(390, 844, 3);
  slow.probe();
  let at = 0;
  for (let i = 0; i < 400; i++) { at += 1 / 20; slow.sample(3, at, true); }
  assert.equal(slow.capped, true);
  slow.probe();
  feed(slow, 30, 3, at, 1 / 60, true);
  assert.equal(slow.capped, true, 'held');
});

ok('a_probe_steps_down_on_slow_draws_alone_and_on_few_draws_alone', () => {
  // 20 ms draws at 60 a second: slow, though often
  const slow = createBudget({ coarse: false });
  slow.size(390, 844, 3);
  slow.probe();
  let at = 0;
  for (let i = 0; i < 60 && slow.wet; i++) { at += 1 / 60; slow.sample(20, at, true); }
  assert.equal(slow.wet, false, 'the wet road is dropped within a second');
  assert.ok(at < 0.8, `${at}`);
  // 3 ms draws at 15 a second: quick, but seldom
  const seldom = createBudget({ coarse: false });
  seldom.size(390, 844, 3);
  seldom.probe();
  at = 0;
  for (let i = 0; i < 60 && seldom.wet; i++) { at += 1 / 15; seldom.sample(3, at, true); }
  assert.equal(seldom.wet, false, 'the wet road is dropped within a second or so');
  assert.ok(at < 1.5, `${at}`);
});

ok('a_start_from_a_kept_tier_begins_there_and_is_corrected_by_the_measuring', () => {
  const b = createBudget({ coarse: true, start: { capped: false, step: 1 } });
  b.size(390, 844, 3);
  assert.equal(b.capped, false);
  assert.equal(b.step, 1);
  assert.deepEqual(b.tier, { capped: false, step: 1 });
  let at = 0;
  for (let i = 0; i < 400 && b.step < 2; i++) { at += 1 / 20; b.sample(30, at, true); }
  assert.ok(b.step >= 2 || b.capped, 'corrected at once, not after a window');
  assert.ok(at < 4, `${at} s`);
});

console.log(`${n} budget checks passed`);
