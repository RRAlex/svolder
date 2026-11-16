/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the VU meter's page model (core/vu-model.js) and the levels stream
// (core.levels): the needle follows the engine's golden path (scripts/web-checks/fixtures/
// vu-ballistics.json, made by `make vu-golden`) to 1e-4 at every sample, so the two engines
// cannot drift; the 2000 ms snap and the lamp's 1200 ms hold are held at their exact edges; a
// levels-only or a full frame decodes; the reference clamps and resets; a stall or a clear rests
// the needles. The real core over a jsdom page (apt `node-jsdom`) opens the levels stream on
// `api/v1/host/frames?only=levels`, apart from the full one.
// Run: node vu.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { page } from './dom.mjs';
import { createCore } from '../../../bins/svolderd/web/core/core.js';
import {
  HOLD_MS, Lamp, MAX_CATCH_UP_MS, Needle, OMEGA, OVER, REF_DEFAULT, REF_MAX, REF_MIN, STOP_RATIO, Scale,
  VuModel, ZETA, byteAmplitude, decodeLevels, peakAmplitude,
} from '../../../bins/svolderd/web/core/vu-model.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const okAsync = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const golden = JSON.parse(readFileSync(new URL('../fixtures/vu-ballistics.json', import.meta.url), 'utf8'));

/** The byte a level of `db` dBFS is sent as (-72 .. 0 over 0 .. 255). */
const byteOf = (db) => Math.round(((db + 72) / 72) * 255);

// --- the golden path ----------------------------------------------------------------------------

ok('the_needle_follows_the_engines_golden_path_to_1e_4_at_every_sample', () => {
  assert.equal(golden.samples.length, 801);
  const needle = new Needle(0);
  let next = 0;
  let worst = 0;
  for (const [t, x, f] of golden.samples) {
    while (next < golden.events.length && golden.events[next][0] <= t) {
      needle.advanceTo(golden.events[next][0]);
      needle.set(golden.events[next][1]);
      next++;
    }
    needle.advanceTo(t);
    worst = Math.max(worst, Math.abs(needle.x - x), Math.abs(needle.deflection - f));
  }
  assert.ok(worst <= 1e-4, `the worst gap is ${worst}`);
});

ok('the_constants_are_the_golden_files', () => {
  assert.equal(MAX_CATCH_UP_MS, golden.max_catch_up_ms);
  assert.ok(Math.abs(STOP_RATIO - golden.stop_ratio) < 1e-12);
  assert.equal(OMEGA, golden.omega);
  assert.equal(ZETA, golden.zeta);
  assert.equal(HOLD_MS, 1200);
  assert.equal(OVER, 32767 / 32768);
});

ok('a_model_fed_the_golden_events_as_bytes_follows_the_path_a_byte_apart', () => {
  // The model is fed bytes, so its ratios are quantised to 0.28 dB; it must still end within a
  // step of the golden path (a gap of 0.28 dB at a ratio near 1 is 3 %).
  const model = new VuModel();
  let next = 0;
  let worst = 0;
  for (const [t, , f] of golden.samples) {
    while (next < golden.events.length && golden.events[next][0] <= t) {
      const [et, u] = golden.events[next++];
      const lv = u > 0 ? byteOf(20 * Math.log10(u) - 18) : 0;
      model.push([lv, lv, 0, 0], et);
    }
    const got = model.step(t).L.f;
    worst = Math.max(worst, Math.abs(got - f));
  }
  assert.ok(worst < 0.06, `the worst gap is ${worst}`);
});

// --- the scale and the reference ----------------------------------------------------------------

ok('the_scale_runs_from_minus_20_to_plus_3_with_a_stop_a_little_past', () => {
  assert.equal(Scale.deflection(0.1), 0);
  assert.ok(Math.abs(Scale.deflection(10 ** (3 / 20)) - 1) < 1e-12);
  assert.equal(Scale.deflection(100), Scale.STOP);
  assert.equal(Scale.deflection(-1), 0);
  assert.equal(Scale.deflection(NaN), 0);
  assert.ok(Math.abs(Scale.deflection(1) - 0.6857) < 1e-3, '0 VU is at about 0.69');
  assert.equal(Scale.readingDb(1), 0);
  assert.ok(Math.abs(Scale.readingDb(0.5) + 6.0206) < 1e-3);
  for (const bad of [0, -0, -0.001, NaN, undefined]) {
    assert.equal(Scale.readingDb(bad), -Infinity, `${bad}`);
  }
});

ok('the_reference_clamps_to_minus_30_and_minus_6_and_resets_to_minus_18', () => {
  assert.deepEqual([REF_MIN, REF_MAX, REF_DEFAULT], [-30, -6, -18]);
  const model = new VuModel();
  assert.equal(model.ref, -18);
  for (const [given, want] of [[-31, -30], [-5, -6], [200, -6], [-200, -30], [-17, -17],
    [-17.4, -17], [NaN, -18], ['x', -18], [undefined, -18]]) {
    model.setRef(given);
    assert.equal(model.ref, want, `${given}`);
  }
  assert.equal(new VuModel({ ref: -25 }).ref, -25);
  assert.equal(new VuModel({ ref: -99 }).ref, -30);
  assert.equal(new VuModel({ ref: 'x' }).ref, -18);
});

ok('a_level_at_the_reference_reads_zero_vu_after_a_second_and_the_reference_moves_it', () => {
  const lv = [byteOf(-18), byteOf(-18), 0, 0];
  const model = new VuModel();
  model.push(lv, 0);
  const at = model.step(1000);
  assert.ok(Math.abs(at.L.db) < 0.3, `${at.L.db} (a byte is 0.28 dB)`);
  assert.ok(Math.abs(at.R.db) < 0.3);
  // the same level against -30 pins at the stop; against -6 it reads about 12 dB down
  model.setRef(-30);
  assert.equal(model.step(2000).L.f, Scale.STOP);
  model.setRef(-6);
  const low = model.step(3000);
  assert.ok(Math.abs(low.L.db + 12) < 0.3, `${low.L.db}`);
});

ok('a_reference_change_acts_on_the_level_held_not_on_the_next_frame', () => {
  const model = new VuModel();
  model.push([byteOf(-18), 0, 0, 0], 0);
  model.step(1000);
  model.setRef(-20);
  const after = model.step(2000);
  assert.ok(Math.abs(after.L.db - 2) < 0.3, `${after.L.db}`);
  assert.equal(after.R.db, -Infinity);
});

// --- bytes --------------------------------------------------------------------------------------

ok('a_byte_is_an_amplitude_and_255_in_the_peak_is_exactly_an_over', () => {
  assert.equal(byteAmplitude(0), 0);
  assert.ok(Math.abs(20 * Math.log10(byteAmplitude(255))) < 1e-9);
  assert.ok(Math.abs(20 * Math.log10(byteAmplitude(1)) + 72 * (254 / 255)) < 1e-9);
  assert.equal(peakAmplitude(255), OVER);
  assert.equal(peakAmplitude(254), byteAmplitude(254));
  assert.ok(peakAmplitude(254) < OVER);
});

// --- the wire -----------------------------------------------------------------------------------

ok('a_levels_only_event_and_a_full_event_decode_alike', () => {
  const light = decodeLevels('{"t":1700000000123,"lv":[10,20,30,255]}');
  assert.equal(light.t, 1700000000123);
  assert.deepEqual([...light.levels], [10, 20, 30, 255]);
  const full = decodeLevels({
    t: 5, l: 'AAAA', r: 'AAAA', lv: [1, 2, 3, 4], beat: 0, bpm: null, phase: 0,
  });
  assert.deepEqual([full.t, [...full.levels]], [5, [1, 2, 3, 4]]);
  assert.ok(full.levels instanceof Uint8Array);
});

ok('anything_that_is_not_a_levels_event_decodes_to_null', () => {
  for (const bad of [null, undefined, 3, '', 'not json', '[]', '{}', '{"t":1}', '{"lv":[1,2,3,4]}',
    '{"t":"x","lv":[1,2,3,4]}', '{"t":1,"lv":[1,2,3]}', '{"t":1,"lv":[1,2,3,4,5]}',
    '{"t":1,"lv":[1,2,3,"4"]}', '{"t":1,"lv":[1,2,3,256]}', '{"t":1,"lv":[1,2,3,-1]}',
    '{"t":1,"lv":[1,2,3,1.5]}', '{"t":1,"lv":[1,2,3,null]}', { t: 1, lv: 'abcd' }]) {
    assert.equal(decodeLevels(bad), null, JSON.stringify(bad));
  }
});

ok('a_push_of_something_that_is_not_four_bytes_changes_nothing', () => {
  const model = new VuModel();
  for (const bad of [null, [1, 2, 3], [1, 2, 3, 'x'], [1, 2, 3, NaN], 'abcd']) {
    assert.equal(model.push(bad, 0), false, JSON.stringify(bad));
  }
  assert.equal(model.push([1, 2, 3, 4], 0), true);
  assert.equal(model.push(new Uint8Array([1, 2, 3, 4]), 1), true);
});

// --- the 2000 ms snap ---------------------------------------------------------------------------

/** The needle of `Needle.at`'s kind, at rest at 0 ms, fed 1. */
const fed = () => {
  const needle = new Needle(0);
  needle.set(1);
  return needle;
};

ok('a_gap_of_2000_ms_still_integrates_and_one_of_2001_ms_snaps', () => {
  const walked = fed();
  walked.advanceTo(2000);
  assert.notEqual(walked.x, 1, 'integrated: a hair short of the input, not on it');
  assert.ok(Math.abs(walked.x - 1) < 1e-6);
  const snapped = fed();
  snapped.advanceTo(2001);
  assert.equal(snapped.x, 1, 'snapped to the input');
  // what counts is the gap of one call: two calls of 1000 and 1001 integrate through
  const split = fed();
  split.advanceTo(1000);
  split.advanceTo(2001);
  assert.notEqual(split.x, 1);
  assert.equal(split.v !== 0, true, 'a needle that integrated still has some speed');
  assert.equal(snapped.v, 0);
});

ok('a_snap_resets_the_speed_of_a_needle_stalled_mid_swing', () => {
  const needle = fed();
  needle.advanceTo(100);
  assert.notEqual(needle.v, 0, 'moving at 100 ms');
  needle.advanceTo(100 + 2001);
  assert.equal(needle.v, 0);
  assert.equal(needle.x, 1);
});

ok('a_snap_holds_the_input_inside_the_stops_and_a_page_clock_far_from_zero_snaps', () => {
  const high = new Needle(0);
  high.set(10);
  high.advanceTo(2001);
  assert.equal(high.x, STOP_RATIO);
  const clock = new Needle(0);
  clock.set(0.5);
  clock.advanceTo(1.7e12);
  assert.equal(clock.x, 0.5, 'a unix-sized first time does not loop');
});

ok('the_first_sight_of_the_model_starts_the_clock_at_rest_whatever_its_origin', () => {
  const model = new VuModel();
  model.push([byteOf(-18), 0, 0, 0], 1.7e12);
  assert.equal(model.step(1.7e12).L.f, 0, 'at rest at the first sight');
  assert.ok(model.step(1.7e12 + 400).L.f > 0.5, 'then it rises on its own ballistics');
});

ok('a_stalled_stream_snaps_the_needle_to_the_last_level_held', () => {
  const model = new VuModel();
  model.push([byteOf(-18), byteOf(-18), 0, 0], 0);
  model.step(500);
  assert.ok(Math.abs(model.step(10_000).L.db) < 0.3, 'settled long after');
});

ok('a_time_that_goes_back_changes_nothing', () => {
  const model = new VuModel();
  model.push([byteOf(-18), 0, 0, 0], 1000);
  const a = model.step(1300).L.f;
  assert.equal(model.step(1200).L.f, a);
  assert.equal(model.step(1300).L.f, a);
});

// --- the needle's own timing --------------------------------------------------------------------

ok('a_step_to_the_reference_reaches_99_percent_at_300_ms_and_a_step_down_falls_by_300_ms', () => {
  const up = fed();
  let first = null;
  let top = 0;
  for (let ms = 1; ms < 2000; ms++) {
    up.advanceTo(ms);
    if (first === null && up.x >= 0.99) first = ms;
    top = Math.max(top, up.x);
  }
  assert.ok(first >= 285 && first <= 315, `${first}`);
  assert.ok(top >= 1.005 && top <= 1.015, `${top}`);
  const down = new Needle(1);
  down.set(0);
  let last = null;
  for (let ms = 1; ms < 2000; ms++) {
    down.advanceTo(ms);
    if (last === null && down.x <= 0.01) last = ms;
    assert.ok(down.x >= 0, 'never below the left stop');
  }
  assert.ok(last >= 285 && last <= 315, `${last}`);
});

ok('the_same_input_gives_the_same_path_at_30_47_and_irregular_frame_times', () => {
  const run = (gaps) => {
    const model = new VuModel();
    let t = 0;
    let i = 0;
    while (t <= 1000) {
      model.push([byteOf(-12), byteOf(-12), 0, 0], t);
      t += gaps[i++ % gaps.length];
    }
    return model.step(1000).L.f;
  };
  // each push holds the same level, so only the time of the pushes differs
  assert.ok(Math.abs(run([33, 33, 34]) - run([21, 21, 22])) < 1e-9);
  assert.ok(Math.abs(run([33, 33, 34]) - run([5, 40, 17, 63, 9, 28])) < 1e-9);
});

ok('a_needle_pushed_past_the_right_stop_leaves_it_at_once', () => {
  const model = new VuModel({ ref: -30 });
  model.push([byteOf(-3), 0, 0, 0], 0);
  assert.equal(model.step(2000).L.f, Scale.STOP, 'pinned');
  model.push([0, 0, 0, 0], 2000);
  assert.ok(model.step(2010).L.f < Scale.STOP, 'off the stop within 10 ms');
});

// --- the lamp -----------------------------------------------------------------------------------

ok('the_lamp_is_lit_from_an_over_for_1200_ms_and_dark_at_1201_on_the_page_clock', () => {
  const model = new VuModel();
  model.push([0, 0, 255, 0], 5000);
  assert.equal(model.step(5000).L.lamp, true);
  assert.equal(model.step(5000 + 1199).L.lamp, true);
  assert.equal(model.step(5000 + 1200).L.lamp, true, 'still lit at exactly 1200 ms');
  assert.equal(model.step(5000 + 1201).L.lamp, false, 'dark at 1201 ms');
  assert.equal(model.step(5000 + 1201).R.lamp, false, 'the right channel was never over');
});

ok('a_peak_of_254_or_less_is_not_an_over_and_each_channel_has_its_own_lamp', () => {
  const model = new VuModel();
  model.push([0, 0, 254, 255], 0);
  const now = model.step(10);
  assert.equal(now.L.lamp, false);
  assert.equal(now.R.lamp, true);
});

ok('a_new_over_re_arms_the_hold_and_a_quiet_frame_does_not_shorten_it', () => {
  const model = new VuModel();
  model.push([0, 0, 255, 255], 0);
  model.push([0, 0, 0, 0], 600);
  assert.equal(model.step(1200).L.lamp, true, 'a quiet frame keeps the hold');
  model.push([0, 0, 255, 0], 1000);
  assert.equal(model.step(2200).L.lamp, true, '1200 ms after the second over');
  assert.equal(model.step(2201).L.lamp, false);
  assert.equal(model.step(2201).R.lamp, false, 'the right one ran out at 1200 ms');
});

ok('the_lamp_alone_holds_at_the_same_edges', () => {
  const lamp = new Lamp();
  assert.equal(lamp.lit(0), false);
  lamp.push(OVER, 100);
  assert.deepEqual([lamp.lit(99), lamp.lit(100), lamp.lit(1300), lamp.lit(1301)],
    [false, true, true, false]);
  lamp.push(0.9999, 2000);
  assert.equal(lamp.lit(2000), false, 'a peak just under OVER is no over');
});

ok('a_fractional_page_clock_is_read_by_the_whole_millisecond', () => {
  const model = new VuModel();
  model.push([0, 0, 255, 0], 100.9);
  assert.equal(model.step(1300.9).L.lamp, true);
  assert.equal(model.step(1301.0).L.lamp, false);
});

// --- clear --------------------------------------------------------------------------------------

ok('clear_rests_the_needles_and_puts_the_lamps_out_and_keeps_the_reference', () => {
  const model = new VuModel({ ref: -24 });
  model.push([byteOf(-12), byteOf(-12), 255, 255], 0);
  assert.ok(model.step(1000).L.f > 0.5);
  model.clear();
  const after = model.step(1001);
  assert.deepEqual([after.L.f, after.R.f, after.L.lamp, after.R.lamp], [0, 0, false, false]);
  assert.equal(after.L.db, -Infinity);
  assert.equal(model.ref, -24);
  // and it starts again on any clock
  model.push([byteOf(-12), 0, 0, 0], 50);
  assert.ok(model.step(2000).L.f > 0.5);
});

ok('a_model_that_was_never_fed_reads_rest', () => {
  const now = new VuModel().step(123);
  assert.deepEqual([now.L.f, now.R.f, now.L.lamp, now.R.lamp], [0, 0, false, false]);
  assert.equal(now.L.db, -Infinity);
});

// --- rest ---------------------------------------------------------------------------------------

ok('resting_is_true_before_any_frame_and_after_a_clear_and_false_while_a_needle_moves', () => {
  const model = new VuModel();
  assert.equal(model.resting(0), true);
  model.push([byteOf(-12), 0, 0, 0], 0);
  model.step(10);
  assert.equal(model.resting(10), false, 'a needle on its way up');
  model.clear();
  assert.equal(model.resting(20), true);
});

ok('resting_is_true_once_a_constant_level_has_settled_and_false_just_before', () => {
  const model = new VuModel();
  model.push([byteOf(-12), byteOf(-30), 0, 0], 0);
  model.step(300);
  assert.equal(model.resting(300), false, 'still swinging at 300 ms');
  model.step(3000);
  assert.equal(model.resting(3000), true, 'settled at 3 s');
});

ok('resting_is_true_in_silence_and_false_while_a_needle_falls_back_to_it', () => {
  const model = new VuModel();
  model.push([byteOf(-6), 0, 0, 0], 0);
  model.step(1000);
  model.push([0, 0, 0, 0], 1000);
  model.step(1050);
  assert.equal(model.resting(1050), false);
  model.step(4000);
  assert.equal(model.resting(4000), true);
});

ok('resting_is_false_while_a_lamp_is_lit_and_true_after_its_hold', () => {
  const model = new VuModel();
  model.push([0, 0, 255, 0], 0);
  model.step(500);
  assert.equal(model.resting(500), false);
  assert.equal(model.resting(1200), false);
  model.step(1201);
  assert.equal(model.resting(1201), true);
});

ok('a_still_model_rests_whatever_the_level_but_not_with_a_lamp_lit', () => {
  const model = new VuModel();
  model.push([byteOf(-12), byteOf(-12), 0, 0], 0);
  assert.equal(model.resting(10, true), true);
  model.push([byteOf(-12), byteOf(-12), 255, 0], 20);
  assert.equal(model.resting(30, true), false);
  assert.equal(model.resting(1300, true), true);
});

// --- reduced motion -----------------------------------------------------------------------------

ok('settled_gives_the_value_the_needle_would_reach_so_reduced_motion_can_jump_to_it', () => {
  const model = new VuModel();
  model.push([byteOf(-18), byteOf(-24), 0, 0], 0);
  const jump = model.settled();
  assert.ok(Math.abs(jump.L.db) < 0.3);
  assert.ok(Math.abs(jump.R.db + 6) < 0.3);
  assert.equal(jump.L.f, Scale.deflection(jump.L.x));
  assert.equal(model.step(0).L.f, 0, 'the model itself was not moved');
});

ok('settled_gives_the_lamp_as_step_does_lit_at_1200_ms_and_dark_at_1201', () => {
  const model = new VuModel();
  model.push([0, 0, 255, 0], 5000);
  assert.equal(model.settled(5000).L.lamp, true);
  assert.equal(model.settled(5000 + 1200).L.lamp, true);
  assert.equal(model.settled(5000 + 1201).L.lamp, false);
  assert.equal(model.settled(5000 + 1200).R.lamp, false);
  assert.notEqual(model.settled(5000), model.step(5000), 'settled and step fill objects of their own');
  assert.equal(model.settled(5000 + 1200).L.lamp, model.step(5000 + 1200).L.lamp);
  assert.equal(new VuModel().settled(0).L.lamp, false, 'never fed: dark');
});

ok('settled_pins_at_the_right_stop_like_the_needle_does', () => {
  const model = new VuModel({ ref: -30 });
  model.push([byteOf(-6), 0, 0, 0], 0);
  const jump = model.settled(0);
  assert.equal(jump.L.x, STOP_RATIO);
  assert.equal(jump.L.f, Scale.STOP);
  const { db } = jump.L;
  assert.equal(db, model.step(5000).L.db, 'the same reading as the needle pinned');
});

// --- core.levels --------------------------------------------------------------------------------

await okAsync('core_levels_is_a_second_stream_on_the_levels_only_url_apart_from_the_full_one',
  async () => {
    const p = page('<div id="theme"></div><div id="toasts"></div>');
    globalThis.window = p.window;
    globalThis.document = p.document;
    const urls = [];
    p.window.EventSource = class {
      static CLOSED = 2;
      constructor(url) { this.url = url; this.readyState = 1; urls.push(url); this.on = {}; }
      addEventListener(name, fn) { this.on[name] = fn; }
      close() { this.closed = true; }
    };
    p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
    p.window.fetch = async () => ({ status: 200, ok: true, json: async () => ({}) });
    const core = createCore({
      page: {
        v: 1, viewer: { guest: false, writes: true }, fleet: { devices: [], hops: [] }, shown: 'A',
        device: null, followed: {}, card: null, host: null, strip: null,
      },
      themes: [],
      window: p.window,
    });
    assert.deepEqual(urls, [], 'nothing opens before a holder asks');
    assert.notEqual(core.levels, core.analysis);
    const heard = [];
    const hold = core.levels.open({ frame: (text) => heard.push(text) });
    assert.deepEqual(urls, ['api/v1/host/frames?only=levels']);
    assert.equal(core.analysis.holders, 0, 'the full stream stays shut');
    assert.equal(core.levels.holders, 1);
    hold.close();
    assert.equal(core.levels.holders, 0);
    const full = core.analysis.open({});
    assert.deepEqual(urls, ['api/v1/host/frames?only=levels', 'api/v1/host/frames']);
    full.close();
  });

console.log(`vu: ${n} passed`);
