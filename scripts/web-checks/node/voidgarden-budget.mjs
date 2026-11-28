/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Void Garden's frame budget (themes/voidgarden/budget.js): the ladder's
// hierarchy (the backdrop first in two rounds of small steps: the sky's, the water's and the land's
// detail, the trees, then the backdrop's share of the resolution; then the particles, the post
// chain's cost, the effects, the hero's particles, and the whole frame's resolution last of all),
// the aim at 60 with the backdrop's steps only and the fall to 30 that goes back to the least step
// holding it, the way up
// (a calm spell, a step on trial, the way shut after a failed one), the same steps at 30, 60 and
// 144 draws a second, the GPU time and the draws-a-second rules, a phone's start, the kept tier,
// the held step, and the pure functions of the module's file. No clock, no random, no DOM: the
// budget is handed the draw time and the seconds. Run: node voidgarden-budget.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  BUDGET, GROUPS, baseScale, createBudget, groupOf, groupOfStep, ladderOf, paceUnder, tierKey,
} from '../../../bins/svolderd/web/themes/voidgarden/budget.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const near = (got, want, eps, what) =>
  assert.ok(Math.abs(got - want) <= eps, `${what}: ${got} is not within ${eps} of ${want}`);

const RATES = [30, 60, 144];

/** What the budget is at now, as one comparable object: every level, the target and the step. */
const view = (b) => {
  const out = {};
  for (const [k, v] of Object.entries(b.levels)) out[k] = typeof v === 'number' ? Number(v.toFixed(4)) : v;
  return { ...out, target: b.target, step: b.step };
};

/** The levels at the top of the ladder, for a base of 1. */
const TOP = {
  sky: 1, water: 1, det: 1, rings: 1, flora: 1, back: 1, q: 1, post: 1, bloom2: true, grit: true,
  hero: 1, scale: 1,
};

/** The steps spent chasing 60 on the ladder of `base`: the backdrop's. */
const quietOf = (base = 1) => ladderOf(base).findIndex((s) => groupOf(s.what) !== 'backdrop');

/** A budget at 1920x1080 on a density-1 screen (base scale 1.0). */
const desktop = (opts) => {
  const b = createBudget(opts);
  b.size(1920, 1080, 1);
  return b;
};

/**
 * Draws of `ms` at `rate` a second for `seconds` (from `from`), the way a loop hands them in.
 * Returns the views after each change of step or target, with the clock of the change.
 */
function run(b, { rate, seconds, ms = 20, from = 0, full = true, gpuMs = null }) {
  const out = [];
  const steps = Math.round(seconds * rate);
  for (let i = 1; i <= steps; i++) {
    const t = from + i / rate;
    const before = `${b.step}/${b.target}`;
    b.sample({ ms, gpuMs, seconds: t, full });
    if (`${b.step}/${b.target}` !== before) out.push({ at: t, view: view(b) });
  }
  return out;
}

/** The step of `what` that the ladder takes at step index `i` (1-based step number). */
const whatAt = (base, i) => ladderOf(base)[i - 1].what;

ok('the_ladder_drops_the_backdrop_first_and_the_sphere_and_effects_last_in_small_steps', () => {
  const ladder = ladderOf(1);
  const groups = ladder.map((s) => groupOf(s.what));
  const seq = groups.filter((g, i) => i === 0 || g !== groups[i - 1]);
  assert.deepEqual(seq, ['backdrop', 'particles', 'post', 'effects', 'hero'], 'the groups once each, in order');
  assert.deepEqual(GROUPS, seq);
  // the backdrop: two rounds of the sky, the water, the land (its detail normals and rings), the
  // trees, then the backdrop's share of the resolution
  const back = ladder.slice(0, quietOf(1)).map((s) => s.what);
  assert.deepEqual(back, [
    'sky', 'water', 'det', 'rings', 'flora', 'back', 'back', 'back',
    'sky', 'water', 'det', 'rings', 'flora', 'back', 'back', 'back', 'back', 'rings',
  ]);
  const shares = ladder.filter((s) => s.what === 'back').map((s) => s.value);
  assert.equal(shares.at(-1), 0.5, 'the backdrop share ends at 0.5');
  assert.ok(shares[2] === 0.75 && shares.indexOf(0.75) < ladder.filter((s) => s.what === 'back').length, 'a share of 0.75 in the first round');
  for (let i = 1; i < shares.length; i++) {
    assert.ok(shares[i] / shares[i - 1] >= 0.88, `each share step small (${shares[i - 1]} -> ${shares[i]})`);
  }
  // the sphere and the effects last: the bloom's second octave and the grit, the hero's particles,
  // then the whole frame's resolution, never before every other step
  const firstOf = (what) => ladder.findIndex((s) => s.what === what);
  assert.ok(firstOf('bloom2') > firstOf('post') && firstOf('post') > firstOf('q'), 'the effects after the post\'s cost after the particles');
  assert.ok(firstOf('hero') > firstOf('grit'), 'the hero\'s particles after the effects');
  assert.ok(ladder.slice(firstOf('scale')).every((s) => s.what === 'scale'), 'the frame\'s resolution is the last of all');
  const scales = ladder.filter((s) => s.what === 'scale').map((s) => s.value);
  for (let i = 1; i < scales.length - 1; i++) near(scales[i] / scales[i - 1], BUDGET.scaleStep, 0.01, 'the frame by 0.82 a step');
  assert.equal(scales.at(-1), BUDGET.scaleLast, 'down to 0.3');
  assert.deepEqual(ladder.filter((s) => s.what === 'q').map((s) => s.value), [0.7, 0.5, 0.3]);
  assert.deepEqual(ladderOf(0.668).filter((s) => s.what === 'scale').map((s) => Number(s.value.toFixed(3))), [0.548, 0.449, 0.368, 0.3], '4K: the frame from its base');
  assert.equal(ladderOf(0.3).filter((s) => s.what === 'scale').length, 0, 'a base at 0.3 keeps its frame');
  assert.deepEqual(ladderOf(0.668).slice(0, quietOf(0.668)), ladderOf(1).slice(0, quietOf(1)), 'the backdrop\'s steps are shares: the same at every base');
});

ok('a_frame_that_holds_30_but_not_60_ends_at_30_at_full_detail', () => {
  const b = desktop();
  b.forceDrawMs(20);
  const changes = run(b, { rate: 60, seconds: 30, ms: 1 });
  const quiet = quietOf(1);
  assert.equal(changes.at(-1).view.target, 30, 'the target falls to 30 once the quiet steps have not held 60');
  assert.equal(changes.at(-1).view.step, 0, 'and goes back to the least step that holds 30: the top');
  assert.equal(changes.filter((c) => c.view.target === 60).length, quiet, 'every quiet step was tried at 60 first');
  assert.deepEqual(view(b), { ...TOP, target: 30, step: 0 });
  assert.equal(b.capped, true, 'paced to 30');
  run(b, { rate: 30, seconds: 40, ms: 1, from: 30 });
  assert.equal(b.step, 0, 'and stays: 20 ms holds 30');
  assert.equal(b.target, 30);
});

ok('a_frame_that_holds_neither_steps_down_the_ladder_in_order_the_sphere_and_effects_last', () => {
  const b = desktop();
  b.forceDrawMs(28);
  const changes = run(b, { rate: 60, seconds: 90, ms: 1 });
  const total = b.steps;
  assert.equal(b.step, total, 'the whole ladder');
  assert.equal(b.target, 30);
  const seen = changes.map((c) => c.view);
  const quiet = quietOf(1);
  assert.ok(seen.slice(0, quiet).every((v) => v.target === 60 && v.grit && v.bloom2 && v.q === 1 && v.hero === 1 && v.scale === 1 && v.post === 1),
    'the backdrop\'s steps first, aiming at 60, the particles, the post, the effects and the hero untouched');
  assert.equal(seen[quiet].target, 30, 'then the target falls to 30');
  const order = [];
  for (let i = 1; i < seen.length; i++) {
    const was = seen[i - 1];
    const now = seen[i];
    // the level given up (the backdrop's share rises again as the frame goes under 1.0: not a step)
    const moved = Object.keys(TOP).find((k) => now[k] < was[k] || (was[k] === true && now[k] === false));
    if (moved) order.push(groupOf(moved));
  }
  const seq = order.filter((k, i) => i === 0 || k !== order[i - 1]);
  assert.deepEqual(seq, ['backdrop', 'particles', 'post', 'effects', 'hero'], 'the groups in order, the hero last');
  assert.deepEqual(view(b), {
    sky: 0, water: 0, det: 0, rings: 0.4, flora: 0.5, back: 1, q: 0.3, post: 0.5, bloom2: false,
    grit: false, hero: 0.35, scale: 0.3, target: 30, step: total,
  }, 'nothing below 0.3: the backdrop is drawn at the frame\'s 0.3, not under it');
  for (let i = 1; i < changes.length; i++) {
    const to30 = changes[i].view.target !== changes[i - 1].view.target;
    const group = groupOf(ladderOf(1)[changes[i - 1].view.step]?.what);
    const look = group === 'effects' || group === 'hero';
    near(changes[i].at - changes[i - 1].at, to30 ? 1 + BUDGET.lowBottomS : look ? 1 + BUDGET.lowLookS : 1, 0.05,
      to30 ? 'a lasting slow rate at the end of the quiet steps gives 60 up'
        : look ? 'the effects and the hero go only when the slowness lasts' : 'one slow second a step');
  }
});

ok('a_draw_time_forced_low_steps_back_up_by_a_trial_of_60_at_the_end_of_the_quiet_steps', () => {
  const b = desktop();
  b.forceDrawMs(28);
  run(b, { rate: 60, seconds: 90, ms: 1 });
  assert.equal(b.step, b.steps);
  b.forceDrawMs(2);
  const up = run(b, { rate: 30, seconds: 600, ms: 20, from: 90 });
  assert.equal(b.step, 0, 'every step comes back');
  assert.equal(b.target, 60, 'and the target 60');
  const quiet = quietOf(1);
  assert.deepEqual([up[0].view.target, up[0].view.step], [60, quiet], '30 is not for good: 60 is tried at the end of the quiet steps');
  near(up[0].at - 90, BUDGET.upAfterS, 0.2, 'after a calm spell');
  for (let i = 1; i < up.length; i++) {
    assert.ok(up[i].at - up[i - 1].at >= BUDGET.upAfterS - 0.1, 'each step back waits the calm spell');
    assert.equal(up[i].view.step, up[i - 1].view.step - 1, 'one step at a time, the render scale last');
  }
  assert.deepEqual(up.at(-1).view, { ...TOP, target: 60, step: 0 });
});

ok('the_same_steps_come_at_30_60_and_144_draws_a_second', () => {
  const seen = RATES.map((rate) => {
    const b = desktop();
    b.forceDrawMs(28);
    const down = run(b, { rate, seconds: 20, ms: 1 });
    b.forceDrawMs(2);
    const up = run(b, { rate, seconds: 70, ms: 20, from: 20 });
    return { down: down.map((c) => c.view), up: up.map((c) => c.view), downAt: down.map((c) => c.at), upAt: up.map((c) => c.at) };
  });
  assert.ok(seen[0].down.length >= 10, 'a step a second');
  assert.ok(seen[0].up.length >= 4);
  for (const s of seen.slice(1)) {
    assert.deepEqual(s.down, seen[0].down, 'the same steps down');
    assert.deepEqual(s.up, seen[0].up, 'the same steps up');
    s.downAt.forEach((at, i) => near(at, seen[0].downAt[i], 0.2, 'the same time down'));
    s.upAt.forEach((at, i) => near(at, seen[0].upAt[i], 0.2, 'the same time up'));
  }
});

ok('a_gpu_time_over_the_limit_steps_down_though_the_cpu_is_fast_and_none_is_not_slow', () => {
  const b = desktop();
  run(b, { rate: 60, seconds: 1.5, ms: 2, gpuMs: 30 });
  assert.equal(b.step, 1, 'a slow GPU second steps down');
  const c = desktop();
  run(c, { rate: 60, seconds: 3, ms: 2, gpuMs: null });
  assert.equal(c.step, 0, 'no GPU reading, a fast CPU: nothing');
  const d = desktop();
  run(d, { rate: 60, seconds: 3, ms: 11.9, gpuMs: 14.9 });
  assert.equal(d.step, 0, 'under 15 ms holds 60');
  assert.equal(d.target, 60);
  const e = desktop();
  run(e, { rate: 60, seconds: 1.5, ms: 15.5 });
  assert.equal(e.step, 1, 'a CPU mean over 15 ms steps down a second');
  const hitch = desktop();
  for (let i = 1; i <= 180; i++) hitch.sample({ ms: i % 20 === 0 ? 800 : 4, gpuMs: 4, seconds: i / 60, full: true });
  assert.equal(hitch.step, 0, 'a draw in twenty that hitches (a shader compiling) is not a slow second');
  const far = desktop();
  run(far, { rate: 60, seconds: 1.5, ms: 2, gpuMs: 60 });
  assert.equal(far.step, 2, 'a cost over twice the limit takes two steps');
});

ok('too_few_draws_a_second_steps_down_where_the_gpu_gives_no_time', () => {
  const b = desktop();
  const down = run(b, { rate: 40, seconds: 6, ms: 1 });
  assert.ok(down.length >= 1, 'forty draws a second under 52 at the 60 target');
  near(down[0].at, 2, 0.2, 'after the first second and 1 s of it');
  assert.equal(down[0].view.target, 60);
  const calm = desktop();
  assert.equal(run(calm, { rate: 40, seconds: 8, ms: 1, full: false }).length, 0, 'not while the scene asks for fewer');
});

ok('a_slow_rate_that_the_chases_steps_do_not_raise_is_the_screens_and_the_target_falls_to_30_at_the_top', () => {
  const b = desktop();
  const changes = run(b, { rate: 40, seconds: 20, ms: 1, gpuMs: 5 });
  const at60 = changes.filter((c) => c.view.target === 60);
  assert.equal(at60.length, BUDGET.chaseSteps, 'the quiet steps are tried while the rate is low: the chase\'s steps, then the verdict');
  assert.deepEqual([changes.at(-1).view.target, changes.at(-1).view.step], [30, 0], 'then the picture is whole again and the draws are paced');
  near(changes[0].at, 2, 0.2, 'after the first second and 1 s');
  // a rate that each step raises is worth chasing: the steps go on until it is 55 or more
  const gain = desktop();
  for (let t = 0; t < 12;) {
    t += 1 / (40 + 6 * gain.step);
    gain.sample({ ms: 1, gpuMs: 5, seconds: t, full: true });
  }
  assert.deepEqual([gain.target, gain.step], [60, 3], 'it stays at 60, three steps down');
  const phone = createBudget({ coarse: true });
  phone.size(390, 844, 3);
  const down = run(phone, { rate: 20, seconds: 6, ms: 1, gpuMs: 5 });
  assert.ok(down.length >= 1 && down.every((c) => c.view.target === 30), 'a phone is at 30 already: a rate under it steps down, whatever the GPU time reads');
  assert.equal(run(createBudget({ coarse: true }), { rate: 29, seconds: 6, ms: 1, gpuMs: 5 }).length, 0, 'and 29 draws a second hold 30');
});

ok('a_dip_at_the_end_of_the_quiet_steps_is_not_the_screens_limit_but_a_lasting_one_is_and_60_is_tried_again_later', () => {
  const quiet = quietOf(1);
  /** Draws whose rate each step down raises by 2.5 a second, to 57 at the end of the quiet steps;
   * `dip` seconds of 40 a second at `from`. */
  const play = (b, { seconds, from = 0, dip = null, flat = null }) => {
    const out = [];
    let t = from;
    while (t < from + seconds) {
      let rate = flat ?? 57 - 2.5 * (quiet - b.step);
      if (dip && t >= dip[0] && t < dip[1]) rate = 40;
      t += 1 / rate;
      const was = `${b.step}/${b.target}`;
      b.sample({ ms: 2, gpuMs: 10, seconds: t, full: true });
      if (`${b.step}/${b.target}` !== was) out.push({ at: t, step: b.step, target: b.target });
    }
    return out;
  };
  const b = desktop();
  play(b, { seconds: 40 });
  assert.deepEqual([b.step, b.target], [quiet, 60], 'the rate rises with each step: down to the end of the quiet steps, 60 held there');
  const held = desktop();
  play(held, { seconds: 40 });
  assert.equal(play(held, { seconds: 30, from: 40, dip: [42, 46] }).filter((c) => c.target === 30).length, 0, 'a low rate for 4 s is a hitch');
  const gone = play(b, { seconds: 80, from: 40, flat: 50 });
  const to30 = gone.find((c) => c.target === 30);
  assert.ok(to30, 'a lasting low rate at the end of the quiet steps gives 60 up');
  near(to30.at - 40, BUDGET.lowBottomS, 1.2, 'after 6 s of it');
  assert.equal(to30.step, 0, 'to the least step that holds 30 (the top)');
  const back = gone.find((c) => c.target === 60);
  assert.ok(back, '60 is tried again');
  near(back.at - to30.at, BUDGET.retry60S, 3, 'after a minute, from the end of the quiet steps');
  assert.equal(back.step, quiet);
  const second = gone.filter((c) => c.target === 30)[1];
  assert.ok(second && second.at - back.at < 8, 'and given up again when the rate is still low');
  const retry2 = play(b, { seconds: 200, from: 120, flat: 50 }).find((c) => c.target === 60);
  assert.ok(retry2.at - (gone.filter((c) => c.target === 30)[1].at) >= 2 * BUDGET.retry60S - 3, 'the second wait is twice as long');
});

ok('the_look_goes_only_when_a_slow_rate_lasts_not_for_a_hitch', () => {
  const ladder = ladderOf(1);
  const at = ladder.findIndex((x) => x.what === 'bloom2');
  const make = () => { const b = createBudget({ start: { v: BUDGET.version, key: tierKey('', 1), step: at, fps: 30 } }); b.size(1920, 1080, 1); return b; };
  const play = (b, dips) => {
    let t = 0;
    let most = b.step;
    while (t < 20) {
      const rate = dips.some(([a, z]) => t >= a && t < z) ? 22 : 30;
      t += 1 / rate;
      b.sample({ ms: 2, gpuMs: 10, seconds: t, full: true });
      most = Math.max(most, b.step);
    }
    return most;
  };
  const hitch = make();
  play(hitch, [[5, 8]]);
  assert.equal(hitch.bloom2, true, 'three slow seconds do not take the bloom');
  const lasting = make();
  play(lasting, [[5, 12]]);
  assert.equal(lasting.bloom2, false, 'seven do');
  const quiet = createBudget({ start: { v: BUDGET.version, key: tierKey('', 1), step: 1, fps: 30 } });
  quiet.size(1920, 1080, 1);
  assert.ok(play(quiet, [[5, 7.5]]) >= 2, 'a quiet step goes after one slow second');
});

ok('a_step_up_on_trial_that_is_too_much_is_undone_and_the_way_up_shuts_longer_each_time', () => {
  const b = desktop({ start: { v: BUDGET.version, key: tierKey('', 1), step: 3, fps: 60 } });
  // comfortable at the lower step; the next one up is too much
  b.forceDrawMs(2);
  const down = b.step;
  const first = run(b, { rate: 60, seconds: 14, ms: 1 });
  assert.equal(first.length, 1, 'a step up after the calm spell');
  assert.equal(b.step, down - 1);
  const at = first[0].at;
  b.forceDrawMs(28);
  const back = run(b, { rate: 60, seconds: 1.2, ms: 1, from: 14 });
  assert.equal(back[0].view.step, down, 'too much: undone at once');
  assert.ok(back[0].at - at < BUDGET.trialS);
  b.forceDrawMs(2);
  const shut = run(b, { rate: 60, seconds: 50, ms: 1, from: 15.2 });
  assert.equal(shut.filter((c) => c.view.step < down).length, 0, 'the way up is shut for 60 s');
  const open = run(b, { rate: 60, seconds: 40, ms: 1, from: 65.2 });
  assert.equal(open.length >= 1, true, 'and open after');
});

ok('a_step_given_up_is_not_tried_again_for_5_minutes', () => {
  const b = desktop({ start: { v: BUDGET.version, key: tierKey('', 1), step: 3, fps: 60 } });
  run(b, { rate: 60, seconds: 1.5, ms: 1, gpuMs: 28 });
  const lowered = b.step;
  assert.ok(lowered > 3, 'a slow second takes a step');
  const calm = run(b, { rate: 60, seconds: 200, ms: 2, gpuMs: 2, from: 1.5 });
  assert.equal(calm.filter((c) => c.view.step < lowered).length, 0, 'comfortable for 200 s, and the step it left stays given up');
  const later = run(b, { rate: 60, seconds: 120, ms: 2, gpuMs: 2, from: 201.5 });
  assert.ok(later.some((c) => c.view.step < lowered), 'and is tried after the 5 minutes');
});

ok('under_the_30_target_the_limit_is_27_ms_and_the_pace_is_the_caps', () => {
  const b = desktop();
  b.forceDrawMs(20);
  run(b, { rate: 60, seconds: 30, ms: 1 });
  assert.equal(b.target, 30);
  b.forceDrawMs(null);
  const before = b.step;
  run(b, { rate: 30, seconds: 6, ms: 1, from: 30 });
  assert.equal(b.step, before, '30 draws a second paced is the target, not slow');
  assert.equal(paceUnder({ asked: 0, capped: true }), 1000 / 30);
  assert.equal(paceUnder({ asked: 100, capped: true }), 100);
  assert.equal(paceUnder({ asked: 0, capped: false }), 0);
  run(b, { rate: 30, seconds: 3, ms: 1, gpuMs: 30, from: 36 });
  assert.ok(b.step > before, '30 ms a frame cannot hold 30');
});

ok('a_phone_aims_at_30_from_one_step_down_and_never_at_60', () => {
  const b = createBudget({ coarse: true });
  b.size(390, 844, 3);
  assert.deepEqual(view(b), { ...TOP, sky: 0.5, target: 30, step: 1 });
  assert.equal(b.scale, 1, 'the first step is the sky\'s detail: a phone\'s frame stays at its base, 1.0');
  assert.equal(b.capped, true);
  run(b, { rate: 30, seconds: 120, ms: 2, gpuMs: 2, full: false });
  assert.equal(b.step, 0, 'comfortable: the step comes back');
  assert.equal(b.target, 30, 'and the target stays 30');
  assert.equal(b.scale, 1, 'a phone never draws denser than 1.0');
});

ok('a_probe_reads_in_half_the_window', () => {
  const b = desktop({ start: { v: BUDGET.version, key: tierKey('', 1), step: 0, fps: 60 } });
  b.probe();
  run(b, { rate: 60, seconds: 0.9, ms: 30 });
  assert.equal(b.step, 1, 'slow draws step down within the probe, in under a second');
});

ok('the_kept_tier_is_where_a_visit_starts_and_a_bad_one_is_ignored', () => {
  const kept = desktop({ start: { v: BUDGET.version, key: tierKey('', 1), step: 3, fps: 60 } });
  assert.equal(kept.step, 3);
  assert.equal(kept.target, 60);
  assert.deepEqual(kept.tier, { v: BUDGET.version, key: tierKey('', 1), step: 3, fps: 60 });
  const kept30 = desktop({ start: { v: BUDGET.version, key: tierKey('', 1), step: 2, fps: 30 } });
  assert.deepEqual([kept30.step, kept30.target], [2, 30], 'the kept 30 starts at 30');
  for (const bad of [{ v: 1, step: 3 }, { step: 3 }, { v: BUDGET.version, key: tierKey('', 1), step: -1 }, { v: BUDGET.version, key: tierKey('', 1), step: 1.5 },
    { v: BUDGET.version, key: tierKey('', 1), step: 'x' }, { v: BUDGET.version, key: tierKey('', 1), step: BUDGET.maxStep + 1 }, {}, null]) {
    assert.equal(desktop({ start: bad }).step, 0, JSON.stringify(bad));
  }
  const phone = createBudget({ coarse: true, start: { v: BUDGET.version, key: tierKey('', 1), step: 4, fps: 30 } });
  phone.size(390, 844, 3);
  assert.equal(phone.step, 4, 'a phone that has a kept step starts where it left off');
  const dense = desktop({ start: { v: BUDGET.version, key: tierKey('', 1), step: 99, fps: 60 } });
  assert.equal(dense.step, 0);
  const big = createBudget({ start: { v: BUDGET.version, key: tierKey('', 1), step: BUDGET.maxStep, fps: 60 } });
  big.size(1920, 1080, 1);
  assert.equal(big.step, big.steps, 'a step past the ladder is its end');
});

ok('a_pause_is_no_evidence_and_draws_far_apart_in_a_row_are', () => {
  const b = desktop();
  run(b, { rate: 60, seconds: 0.6, ms: 30 });
  b.pause();
  b.sample({ ms: 30, seconds: 100, full: true });
  assert.equal(b.step, 0, 'the first draw after a hidden page starts the reading over');
  const slow = desktop();
  slow.forceDrawMs(28);
  for (let i = 0; i < 6; i++) slow.sample({ ms: 2, seconds: i * 3.3, full: true });
  assert.equal(slow.step, 5, 'a forced slow draw steps down at each draw, however far apart');
  const rate = desktop();
  for (let i = 0; i < 6; i++) rate.sample({ ms: 2, seconds: i * 3.3, full: true });
  assert.ok(rate.step >= 1, 'under a draw a second for 2 s steps down as well');
});

ok('the_base_density_is_at_most_1_75_and_3_7_megapixels_and_4k_keeps_its_frame_while_the_backdrop_steps', () => {
  near(baseScale(1280, 800, 2), 1.75, 1e-9, 'a laptop at density 2');
  near(baseScale(1280, 800, 1), 1, 1e-9, 'density 1 is 1');
  near(baseScale(2560, 1440, 2), 1.0016, 0.001, '1440p at density 2 lands on 3.7 MP, about 1.0');
  near(baseScale(3840, 2160, 2), Math.sqrt(3.7e6 / (3840 * 2160)), 1e-9, '4K is held to the megapixels');
  const b = createBudget();
  b.size(3840, 2160, 1);
  near(b.scale, 0.668, 0.001, '4K starts at 0.668');
  b.forceDrawMs(28);
  run(b, { rate: 60, seconds: 30, ms: 1 });
  assert.equal(b.back, 0.5, 'the backdrop down to half the frame\'s resolution');
  near(b.scale, 0.668, 0.001, 'the frame (the sphere) still at its base');
  assert.equal(whatAt(0.668, 1), 'sky');
  assert.equal(whatAt(0.668, quietOf(0.668) + 1), 'q', 'and the particles come after the backdrop');
});

ok('the_detail_pref_pins_the_top_or_the_whole_ladder_and_auto_is_the_budget', () => {
  const high = desktop({ detail: 'high', start: { v: BUDGET.version, key: tierKey('', 1), step: 5, fps: 30 } });
  assert.equal(high.step, 0, 'High ignores the kept tier');
  assert.equal(high.target, 60);
  assert.equal(high.detail, 'high');
  run(high, { rate: 60, seconds: 6, ms: 40 });
  assert.equal(high.step, 0, 'High does not step down');
  const phone = createBudget({ coarse: true, detail: 'high' });
  phone.size(400, 800, 3);
  assert.equal(phone.capped, false, 'High lifts a phone\'s cap');
  const low = desktop({ detail: 'low' });
  assert.equal(low.step, low.steps, 'Low starts at the end of the ladder');
  assert.equal(low.target, 30);
  assert.equal(low.q, BUDGET.q.at(-1));
  assert.equal(low.scale, BUDGET.scaleLast);
  run(low, { rate: 60, seconds: 30, ms: 1 });
  assert.equal(low.step, low.steps, 'Low does not step up');
  // the pref changes at run time: the way back to the budget
  assert.equal(low.setDetail('auto'), false, 'nothing moves at once');
  assert.equal(low.step, low.steps, 'Auto goes on from where it stood');
  run(low, { rate: 30, seconds: 600, ms: 1, from: 30, gpuMs: 2 });
  assert.equal(low.step, 0, 'and steps up on its own');
  assert.equal(low.setDetail('nonsense'), false, 'a value the budget does not know is Auto');
  const b = desktop();
  assert.equal(b.setDetail('high'), false, 'no scale to change at the top');
  assert.equal(b.setDetail('low'), true);
  assert.equal(b.step, b.steps);
});

ok('a_held_step_stays_until_the_pref_changes_and_the_pref_read_again_does_not_end_it', () => {
  const b = desktop();
  assert.equal(b.hold(4, 30), false, 'a backdrop step: the frame keeps its size');
  assert.deepEqual([b.step, b.target], [4, 30]);
  assert.equal(desktop().hold(desktop().steps, 30), true, 'the frame\'s last resort: the render scale changed');
  b.forceDrawMs(2);
  run(b, { rate: 60, seconds: 30, ms: 1 });
  assert.equal(b.step, 4, 'no step on its own');
  assert.equal(b.setDetail('auto'), false, 'the pref read again is the same pref');
  assert.equal(b.step, 4);
  assert.equal(b.detail, 'auto');
  b.setDetail('high');
  assert.equal(b.step, 0, 'a changed pref ends the hold');
  assert.equal(desktop().hold(999, 60) && 1, 1);
});

ok('the_budget_is_pure_no_clock_no_random_no_dom', () => {
  const src = readFileSync(new URL('../../../bins/svolderd/web/themes/voidgarden/budget.js', import.meta.url), 'utf8')
    .replace(/\/\/.*$/gm, '').replace(/\/\*[\s\S]*?\*\//g, '');
  for (const bad of ['Math\\.random', 'Date\\.now', 'performance\\.', '\\bdocument\\b', '\\bwindow\\b', 'setTimeout']) {
    assert.ok(!new RegExp(bad).test(src), `no ${bad}`);
  }
});

// ---- 3.6 fix 1: a vsync simulation (the loop's keepFrame pacing on a display of `hz`), the costs of
// a 1080p ladder (GPU ms at each step: the old ladder's measured top and bottom on radeonsi, the
// backdrop's steps between them alike, then the small steps of the particles, the post, the effects
// and the hero, and the frame's resolution), a GPU that is busy until its last frame is done. The
// same lcg each run: no random.
const Q = quietOf(1);
const HD = [
  ...Array.from({ length: Q + 1 }, (_, i) => Number((43.3 * (12.7 / 43.3) ** (i / Q)).toFixed(1))),
  12.5, 12.3, 12.1, 11.9, 11.8, 11.7, 11.6, 11.5, 9.3, 7.9, 6.5, 6.4, 6.3, 6.2,
];
assert.equal(HD.length, ladderOf(1).length + 1, 'a cost for every step');
const keepIt = ({ sinceMs, frameMs, paceMs }) => !(paceMs > 0) || sinceMs + frameMs / 2 >= paceMs;

/** Plays `secs` seconds on a `hz` display: the budget `b` (1080p) is offered every vsync, draws at the
 * pace it asks for, and is handed `costs[step]` ms (x `spike[2]` between `spike[0]` and `spike[1]`).
 * Returns the draws a second over the last 60 s and the final step/target. */
function play(b, { hz, costs, secs, spike = null, jitter = 0.3 }) {
  const vs = 1000 / hz;
  let t = 0;
  let drawnAt = -1e9;
  let offeredAt = 0;
  let busyUntil = 0;
  let rnd = 12345;
  const r = () => { rnd = (rnd * 1103515245 + 12345) % 2147483648; return rnd / 2147483648; };
  let late = 0;
  const events = [];
  let key = `${b.step}@${b.target}`;
  while (t < secs * 1000) {
    t += vs;
    const ts = t + (r() - 0.5) * jitter;
    if (ts < busyUntil) continue;
    const frameMs = Math.min(50, ts - offeredAt);
    offeredAt = ts;
    b.offer?.(frameMs);
    if (!keepIt({ sinceMs: ts - drawnAt, frameMs, paceMs: paceUnder({ asked: 0, capped: b.capped }) })) continue;
    drawnAt = ts;
    let gpu = costs[Math.min(b.step, costs.length - 1)];
    if (spike && ts / 1000 >= spike[0] && ts / 1000 < spike[1]) gpu *= spike[2];
    busyUntil = ts + Math.ceil(gpu / vs - 1e-9) * vs - vs / 2;
    b.sample({ ms: 2, gpuMs: gpu, seconds: ts / 1000, full: true });
    if (ts / 1000 > secs - 60) late++;
    const now = `${b.step}@${b.target}`;
    if (now !== key) { key = now; events.push({ at: ts / 1000, step: b.step, target: b.target }); }
  }
  return { fps: late / 60, step: b.step, target: b.target, events };
}

/** The kept tier of a budget that settled on `step` at `fps`, for the device and size class. */
const keptTier = (step, fps, device = '', base = 1) =>
  ({ v: BUDGET.version, key: tierKey(device, base), step, fps });

ok('sim_a_load_spike_that_gave_60_up_does_not_keep_30_for_good', () => {
  const b = desktop();
  const r = play(b, { hz: 60, costs: HD, secs: 900, spike: [120, 140, 1.4] });
  assert.equal(r.target, 60, 'a GPU that held 60 holds it again after a 20 s load');
  assert.ok(r.fps > 58, `at ${r.fps} draws a second`);
  const next = createBudget({ start: b.tier });
  next.size(1920, 1080, 1);
  const again = play(next, { hz: 60, costs: HD, secs: 200 });
  assert.equal(again.target, 60, 'and the next visit from the kept tier too');
});

ok('sim_a_kept_30_on_a_gpu_whose_bottom_holds_60_reaches_60', () => {
  const hd13 = HD.map((c, i) => (i === Q ? 13.4 : c));
  const b = desktop({ start: keptTier(Q, 30) });
  const r = play(b, { hz: 60, costs: hd13, secs: 300 });
  assert.equal(r.target, 60);
  assert.ok(r.fps > 58, `at ${r.fps} draws a second`);
  const third = desktop({ start: keptTier(3, 30) });
  assert.equal(play(third, { hz: 60, costs: HD, secs: 300 }).target, 60, 'from a kept {3, 30} too');
});

ok('sim_a_failed_trial_of_60_goes_back_to_30_at_once_and_waits_longer', () => {
  const slow = HD.map((c) => c * 1.6);
  const b = desktop({ start: keptTier(Q, 30) });
  const r = play(b, { hz: 60, costs: slow, secs: 700 });
  const ups = r.events.filter((e) => e.target === 60);
  assert.ok(ups.length >= 2, 'a trial of 60 now and then');
  for (const up of ups) {
    const back = r.events.find((e) => e.at > up.at && e.target === 30);
    assert.ok(back && back.at - up.at < 6, `back at 30 within 6 s (${back && back.at - up.at})`);
  }
  assert.equal(r.fps > 28 && r.fps < 31, true, `30 held (${r.fps})`);
  assert.ok(ups[1].at - ups[0].at >= BUDGET.retry60S, 'the wait grows');
});

ok('sim_after_two_failed_trials_60_is_not_tried_again_until_a_reload', () => {
  // the user, 11-27: "only happen twice before giving up and requiring a reload"
  const slow = HD.map((c) => c * 1.6);
  const b = desktop({ start: keptTier(Q, 30) });
  const r = play(b, { hz: 60, costs: slow, secs: 3600 });
  const ups = r.events.filter((e) => e.target === 60);
  assert.equal(ups.length, BUDGET.retry60Tries, `${ups.length} trials of 60 in an hour`);
  assert.equal(r.target, 30);
  const reload = desktop({ start: b.tier });
  const again = play(reload, { hz: 60, costs: slow, secs: 700 });
  assert.ok(again.events.some((e) => e.target === 60), 'a reload tries 60 again');
});

ok('sim_a_fast_gpu_on_a_50_or_24_hz_display_keeps_the_look_and_the_top', () => {
  const strong = HD.map((c) => c / 4);
  for (const hz of [50, 48, 25, 24]) {
    const b = desktop();
    const r = play(b, { hz, costs: strong, secs: 300 });
    assert.equal(r.step, 0, `${hz} Hz: the top step (${r.step})`);
    assert.ok(b.bloom2 && b.grit, `${hz} Hz: the look kept`);
  }
  const fast = desktop();
  const r = play(fast, { hz: 144, costs: strong, secs: 120 });
  assert.equal(r.step, 0, '144 Hz, a strong GPU: the top');
  assert.ok(r.fps > 65, `and the GPU's own pace is drawn (${r.fps})`);
});

ok('sim_a_bottom_that_cost_far_over_60_is_not_tried_again_a_blip_for_nothing', () => {
  const far = HD.map((c) => c * 2.6);
  const b = desktop();
  const r = play(b, { hz: 60, costs: far, secs: 500 });
  const tries = r.events.filter((e) => e.target === 60 && e.at > 30);
  assert.equal(tries.length, 0, `no trial of 60 on a GPU that read ${(12.7 * 2.6).toFixed(0)} ms at the bottom (${tries.length})`);
  assert.equal(r.target, 30);
});

ok('sim_the_30_cap_holds_at_75_120_144_165_hz_and_a_slow_gpu_is_not_walked_down_by_the_rate', () => {
  const slow = HD.map((c) => c * 1.6);
  for (const hz of [60, 75, 120, 144, 165, 50]) {
    const b = desktop();
    const r = play(b, { hz, costs: slow, secs: 400 });
    assert.equal(r.target, 30, `${hz} Hz: 30`);
    assert.ok(r.step <= Q, `${hz} Hz: the least step that holds 30 with room (${r.step})`);
    assert.ok(b.bloom2 && b.grit, `${hz} Hz: the look kept`);
  }
});

ok('at_30_the_chases_steps_that_do_not_raise_the_rate_stop_the_descent_and_go_back', () => {
  const b = desktop({ start: keptTier(2, 30) });
  let t = 0;
  // 26 draws a second whatever the step (a display that paces 25): the rate verdict at 30 is
  // not the GPU's, and steps do not raise it
  const seen = [];
  while (t < 16) {
    t += 1 / 26;
    const was = b.step;
    b.sample({ ms: 1, gpuMs: 4, seconds: t, full: true });
    if (b.step !== was) seen.push(b.step);
  }
  assert.deepEqual(seen, [3, 4, 5, 6, 7, 2], 'the descent stops after the chase\'s steps and goes back to where it began');
  assert.equal(BUDGET.chaseSteps, 5);
});

ok('the_kept_tier_is_keyed_by_the_tier_gpu_and_size_class', () => {
  const tier = keptTier(10, 30, 'webgl2|amd radeonsi', 0.668);
  const same = createBudget({ device: 'webgl2|amd radeonsi', start: tier });
  same.size(3840, 2160, 1);
  assert.deepEqual([same.step, same.target], [10, 30], 'the same GPU and size class starts where it left off');
  assert.equal(same.tier.key, tier.key);
  const small = createBudget({ device: 'webgl2|amd radeonsi', start: tier });
  small.size(1920, 1080, 1);
  assert.deepEqual([small.step, small.target], [0, 60], 'a 4K tier opened at 1080p starts fresh');
  const other = createBudget({ device: 'webgl2|nvidia', start: tier });
  other.size(3840, 2160, 1);
  assert.deepEqual([other.step, other.target], [0, 60], 'another GPU starts fresh');
  const late = createBudget({ device: null, start: tier });
  late.size(3840, 2160, 1);
  assert.equal(late.step, 0, 'before the GPU is known nothing is taken');
  late.device('webgl2|amd radeonsi');
  assert.deepEqual([late.step, late.target], [10, 30], 'the kept tier applies once the device is known');
});

ok('a_resize_maps_the_levels_by_their_meaning_not_by_step_number', () => {
  // the backdrop's steps are shares, the same on every ladder: a step there keeps its levels
  const b = desktop({ start: keptTier(Q, 30) });
  const before = view(b);
  assert.equal(b.back, 0.5);
  b.size(3840, 2160, 1);
  assert.deepEqual({ ...view(b), scale: 1 }, { ...before, scale: 1 }, '4K: the same levels');
  near(b.scale, 0.668, 0.001, 'at the 4K frame\'s base');
  b.size(1920, 1080, 1);
  assert.deepEqual(view(b), before, 'and back');
  assert.equal(b.target, 30, 'the target stays');
  // the effects given up stay given up
  const look = desktop({ start: keptTier(ladderOf(1).findIndex((s) => s.what === 'grit') + 1, 30) });
  assert.equal(look.grit, false);
  look.size(3840, 2160, 1);
  assert.deepEqual([look.grit, look.bloom2], [false, false], 'the effects stay given up');
  // the frame's last resort stays about as small
  const last = desktop({ start: keptTier(ladderOf(1).findIndex((s) => s.what === 'scale' && s.value < 0.4) + 1, 30) });
  near(last.scale, 0.371, 0.001, 'the frame at 0.37');
  last.size(3840, 2160, 1);
  near(last.scale, 0.368, 0.001, 'the nearest of the 4K frame\'s steps');
  assert.equal(last.rings, 0.4);
  assert.equal(last.hero, 0.35, 'the hero\'s particles given up as before');
});

ok('a_resize_at_the_top_stays_at_the_top_and_a_backdrop_step_keeps_its_share', () => {
  const top = desktop();
  top.size(3840, 2160, 1);
  assert.equal(top.step, 0);
  near(top.scale, 0.668, 0.001, 'the new base');
  const shared = desktop({ start: keptTier(7, 60) });
  assert.equal(shared.back, 0.82);
  shared.size(3840, 2160, 1);
  assert.deepEqual([shared.step, shared.back, shared.q], [7, 0.82, 1], 'the same backdrop share on the 4K ladder');
});

ok('a_phones_backdrop_share_stops_at_0_6_and_its_frame_is_not_below_0_75', () => {
  const lad = ladderOf(1, true);
  assert.deepEqual(lad.filter((s) => s.what === 'scale').map((s) => s.value), [0.75], 'the frame\'s last resort 0.75');
  assert.equal(Math.min(...lad.filter((s) => s.what === 'back').map((s) => s.value)), 0.6, 'the backdrop at 0.6 at least');
  const phone = createBudget({ coarse: true });
  phone.size(390, 844, 3);
  phone.forceDrawMs(40);
  run(phone, { rate: 30, seconds: 90, ms: 1 });
  assert.ok(phone.scale >= 0.75, `the phone never draws under 0.75 (${phone.scale})`);
  assert.equal(phone.step, phone.steps);
});

ok('the_backdrop_is_never_drawn_under_0_3_px_and_a_hidpi_frame_gives_its_density_above_1_with_the_backdrop', () => {
  for (const [w, h, d] of [[1920, 1080, 1], [3840, 2160, 1], [1280, 800, 2]]) {
    const b = createBudget();
    b.size(w, h, d);
    for (let step = 0; step <= b.steps; step++) {
      b.hold(step, 30);
      // (a frame of 1.0 or more keeps its own floor: the backdrop's share of 0.5 there)
      if (b.scale < 1) {
        assert.ok(b.back * b.scale >= BUDGET.backLeast - 1e-9,
          `${w}x${h}@${d} step ${step}: backdrop ${b.back} x frame ${b.scale}`);
      }
    }
  }
  const lad = ladderOf(1.75);
  const dense = lad.filter((s) => s.what === 'scale' && s.value >= 1);
  assert.deepEqual(dense.map((s) => Number(s.value.toFixed(2))), [1.43, 1.18, 1], 'a 2x screen: 1.75 to 1.0 by 0.82');
  assert.ok(dense.every((s) => groupOfStep(s) === 'backdrop'), 'with the backdrop\'s steps');
  const firstOf = (pred) => lad.findIndex(pred);
  assert.ok(firstOf((s) => s.what === 'scale' && s.value === 1) < firstOf((s) => s.what === 'q'), 'before the particles, the effects and the hero');
  assert.ok(lad.filter((s) => s.what === 'scale' && s.value < 1).every((s) => groupOfStep(s) === 'hero'), 'under 1.0 the frame is the last resort');
});

ok('the_samples_make_no_per_frame_arrays', () => {
  const b = desktop();
  run(b, { rate: 60, seconds: 20, ms: 2, gpuMs: 8 });
  const counts = { sort: 0, map: 0, filter: 0, slice: 0 };
  const real = {};
  for (const k of Object.keys(counts)) {
    real[k] = Array.prototype[k];
    Array.prototype[k] = function counted(...args) { counts[k]++; return real[k].apply(this, args); };
  }
  const stringify = JSON.stringify;
  let strings = 0;
  JSON.stringify = (...args) => { strings++; return stringify(...args); };
  try {
    for (let i = 0; i < 600; i++) {
      b.sample({ ms: 2, gpuMs: 8, seconds: 20 + i / 60, full: true });
      void [b.grit, b.bloom2, b.q, b.rings, b.scale, b.back, b.sky, b.hero, b.capped, b.target, b.step];
    }
  } finally {
    for (const k of Object.keys(counts)) Array.prototype[k] = real[k];
    JSON.stringify = stringify;
  }
  assert.deepEqual(counts, { sort: 0, map: 0, filter: 0, slice: 0 }, 'ten seconds of steady draws');
  assert.equal(strings, 0);
});

ok('a_held_step_is_told_apart_from_auto', () => {
  const b = desktop();
  b.hold(3, 60);
  assert.equal(b.held, true);
  b.setDetail('high');
  assert.equal(b.held, false);
});

console.log(`voidgarden-budget: ${n} ok`);
process.exit(0);
