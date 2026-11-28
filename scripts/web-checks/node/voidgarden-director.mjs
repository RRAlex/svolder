/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for Void Garden's conductor (themes/voidgarden/director.js): the beat from the
// frame's tempo and phase and its drop of a flag over 120 ms late, groove hits (at most one per
// 200 ms, hats per 70 ms), sections with hysteresis and the 3.5 s pending rule (a 3 s loud burst
// in a quiet piece makes no drop), peak and morph triggers, the track change's fade and seed, and
// the flash limiter (never over 3 a second at 30, 60 and 144 steps a second, the same firings at
// each). Run: node voidgarden-director.mjs
import assert from 'node:assert/strict';
import {
  FlashLimiter, MAX_FLASHES, SECTION, WORLDS, createDirector, seedOf,
} from '../../../bins/svolderd/web/themes/voidgarden/director.js';
import { Features } from '../../../bins/svolderd/web/themes/voidgarden/features.js';
import { frame, rmsByte, within } from './voidgarden-rig.mjs';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const near = (got, want, eps, what) =>
  assert.ok(Math.abs(got - want) <= eps, `${what}: ${got} is not within ${eps} of ${want}`);

const RATES = [30, 60, 144];

/**
 * Steps a conductor over `seconds` at `rate` steps a second. `frameAt(k)` is the k-th frame of a
 * 30 a second stream (or null): it is handed over at the first step at or after its time `k/30`.
 * Features run on the frames as the wire runs them. `input(t)` adds fields to the step's input
 * (`mute`, `pause`, `offsetMs`). Returns the events with the director's clock, and the director.
 */
function run({ rate, seconds, frameAt = () => null, input = () => ({}), director = createDirector(), features = new Features() }) {
  const events = [];
  const steps = Math.round(seconds * rate);
  let k = 0;
  for (let i = 1; i <= steps; i++) {
    const t = i / rate;
    let f = null;
    while (k / 30 <= t + 1e-9 && frameAt(k) !== undefined) {
      const next = frameAt(k);
      k++;
      if (next) f = next;
      else if (next === null) continue;
    }
    if (f) features.observe(f);
    features.step(1 / rate, { gate: input(t).pause ? 0 : 1, mute: input(t).mute });
    for (const e of director.step(1 / rate, {
      frame: f, raw: features.raw, values: features.values, now: t * 1000, offsetMs: 0, ...input(t),
    })) events.push({ ...e, at: director.state.time });
  }
  return { events, director, features };
}

/** The frames of a 120 bpm piece at 30 a second: `t` ms from 0, a beat every 500 ms, phase by it. */
const tempoFrame = (k, over = {}) => {
  const t = (k * 1000) / 30;
  return frame({ t, bpm: 120, phase: Math.floor(((t % 500) / 500) * 256), levels: [100, 100, 0, 0], ...over });
};

ok('the_beat_follows_bpm_and_phase_at_every_step_rate', () => {
  for (const rate of RATES) {
    const { events } = run({ rate, seconds: 10, frameAt: (k) => (k < 300 ? tempoFrame(k) : undefined) });
    const beats = events.filter((e) => e.type === 'beat');
    assert.ok(beats.length >= 19 && beats.length <= 21, `beats at ${rate}: ${beats.length}`);
    for (const b of beats) {
      const off = Math.abs(((b.at + 0.25) % 0.5) - 0.25);
      assert.ok(off <= 1 / 30 + 1 / rate, `beat at ${b.at} is ${off} s off the grid at ${rate}`);
      assert.equal(b.bpm, 120);
    }
    for (let i = 1; i < beats.length; i++) near(beats[i].at - beats[i - 1].at, 0.5, 1 / 30 + 1 / rate, 'a beat every 0.5 s');
  }
});

ok('the_beat_follows_another_tempo_a_phase_off_zero_and_a_daemon_clock_far_from_the_pages', () => {
  // 100 bpm (a beat every 600 ms), the grid 137 ms after the daemon clock's zero, and a daemon
  // clock an hour and more ahead of the page's: the frame carries the daemon's time
  const OFFSET = 4_000_000_137;
  const GRID = 137;
  const at100 = (k) => {
    const t = (k * 1000) / 30 + OFFSET;
    const phase = Math.floor((((((t - OFFSET - GRID) % 600) + 600) % 600) / 600) * 256);
    return frame({ t, bpm: 100, phase, levels: [100, 100, 0, 0] });
  };
  for (const rate of RATES) {
    const { events, director } = run({
      rate, seconds: 12, frameAt: (k) => (k < 360 ? at100(k) : undefined), input: () => ({ offsetMs: OFFSET }),
    });
    const beats = events.filter((e) => e.type === 'beat');
    assert.ok(beats.length >= 19 && beats.length <= 21, `beats at ${rate}: ${beats.length}`);
    for (const b of beats) {
      const off = Math.abs((((b.at - 0.137 + 0.3) % 0.6) + 0.6) % 0.6 - 0.3);
      assert.ok(off <= 1 / 30 + 1 / rate + 0.003, `beat at ${b.at} is ${off} s off the grid at ${rate}`);
      assert.equal(b.bpm, 100);
    }
    for (let i = 1; i < beats.length; i++) near(beats[i].at - beats[i - 1].at, 0.6, 1 / 30 + 1 / rate, 'a beat every 0.6 s');
    assert.equal(director.state.bpm, 100);
  }
  // the bar follows the beats: the beat counter, at each beat, stays the same distance from the
  // number of beats heard (a counter stepped at another tempo drifts away), and the bar is a
  // quarter further each time
  for (const rate of RATES) {
    const d = createDirector();
    const feat = new Features();
    const marks = [];
    for (let i = 1; i <= rate * 12; i++) {
      const t = i / rate;
      const k = Math.floor(t * 30 + 1e-9);
      const f = k >= 1 && Math.floor((i - 1) * 30 / rate + 1e-9) < k ? at100(k - 1) : null;
      if (f) feat.observe(f);
      for (const e of d.step(1 / rate, { frame: f, raw: feat.raw, values: feat.values, now: t * 1000, offsetMs: OFFSET })) {
        if (e.type === 'beat') marks.push({ beat: e.beat, count: d.state.beat - e.beat, bar: d.state.groove.bar });
      }
    }
    assert.ok(marks.length >= 19, `beats at ${rate}: ${marks.length}`);
    for (const m of marks) near(m.count, marks[0].count, 0.3, `the beat count at beat ${m.beat} at ${rate}`);
    for (let i = 1; i < marks.length; i++) {
      near(((marks[i].bar - marks[i - 1].bar + 1.25) % 1) - 0.5, 0, 0.1, `the bar moves a quarter at beat ${marks[i].beat}`);
    }
  }
});

ok('a_beat_flag_over_120_ms_late_is_dropped_and_one_in_time_is_not', () => {
  const flagged = (t) => frame({ t, beat: true, bpm: null, levels: [100, 100, 0, 0] });
  // handed over 300 ms after its time: dropped
  const late = createDirector();
  const quietFrame = frame({ t: 0, levels: [100, 100, 0, 0] });
  const seen = [];
  const feed = (d, f, now) => d.step(0.01, { frame: f, raw: new Features().raw, values: new Features().values, now, offsetMs: 0 });
  seen.push(...feed(late, flagged(1000), 1300));
  seen.push(...feed(late, quietFrame, 1310));
  assert.equal(seen.filter((e) => e.type === 'beat').length, 0, 'late flag');
  const prompt = createDirector();
  const got = feed(prompt, flagged(2000), 2050);
  assert.equal(got.filter((e) => e.type === 'beat').length, 1, 'flag in time');
  assert.equal(feed(prompt, quietFrame, 2060).filter((e) => e.type === 'beat').length, 0, 'once');
  // the 120 ms edge: 100 ms late counts, 150 ms late does not
  const edge = (lateMs) => feed(createDirector(), flagged(5000), 5000 + lateMs).some((e) => e.type === 'beat');
  assert.equal(edge(100), true);
  assert.equal(edge(150), false);
});

ok('groove_hits_come_from_band_onsets_at_most_one_per_200_ms_and_hats_per_70', () => {
  // a kick every 125 ms (on for one frame), a hat every 40 ms (on for one frame)
  const kicks = (k) => (k < 300 ? frame({ t: (k * 1000) / 30, left: k % 4 === 0 ? within(25, 150, -18) : -200, levels: [100, 100, 0, 0] }) : undefined);
  const hats = (k) => (k < 300 ? frame({ t: (k * 1000) / 30, left: k % 2 === 0 ? within(2000, 12000, -22) : -200, levels: [100, 100, 0, 0] }) : undefined);
  for (const rate of RATES) {
    const kick = run({ rate, seconds: 10, frameAt: kicks }).events.filter((e) => e.type === 'kick');
    assert.ok(kick.length >= 20, `kicks fire (${kick.length}) at ${rate}`);
    for (let i = 1; i < kick.length; i++) assert.ok(kick[i].at - kick[i - 1].at > 0.2 - 1e-9, `kick gap ${kick[i].at - kick[i - 1].at}`);
    const hat = run({ rate, seconds: 10, frameAt: hats }).events.filter((e) => e.type === 'hat');
    assert.ok(hat.length >= 40, `hats fire (${hat.length}) at ${rate}`);
    for (let i = 1; i < hat.length; i++) assert.ok(hat[i].at - hat[i - 1].at > 0.07 - 1e-9, `hat gap ${hat[i].at - hat[i - 1].at}`);
    assert.ok(hat.length > kick.length, 'hats are allowed more often');
    for (const e of [...kick, ...hat]) assert.ok(e.amp > 0 && e.amp <= 1);
  }
});

ok('a_steady_sound_stops_making_hits_and_a_still_scene_makes_none', () => {
  const steady = (k) => (k < 300 ? frame({ t: (k * 1000) / 30, left: within(25, 150, -18), levels: [100, 100, 0, 0] }) : undefined);
  const { events } = run({ rate: 60, seconds: 10, frameAt: steady });
  // the onset from silence retriggers while the slow mean (a third of a second) catches up, then stops
  const kick = events.filter((e) => e.type === 'kick');
  assert.ok(kick.length >= 1 && kick.length <= 5, `${kick.length} kicks`);
  assert.ok(kick.every((e) => e.at < 1.5), 'none once the mean has caught up');
  const kicks = (k) => (k < 300 ? frame({ t: (k * 1000) / 30, left: k % 4 === 0 ? within(25, 150, -18) : -200, levels: [100, 100, 0, 0] }) : undefined);
  for (const flag of ['pause', 'mute']) {
    const still = run({ rate: 60, seconds: 10, frameAt: kicks, input: () => ({ [flag]: true }) }).events;
    assert.equal(still.filter((e) => ['kick', 'snare', 'hat', 'beat', 'section'].includes(e.type)).length, 0, flag);
  }
});

ok('a_3_second_loud_burst_in_a_quiet_piece_makes_no_drop_and_6_seconds_does', () => {
  const quiet = (k) => frame({ t: (k * 1000) / 30, left: within(100, 6000, -50), levels: [rmsByte(0.01), rmsByte(0.01), 0, 0] });
  const loud = (k) => frame({ t: (k * 1000) / 30, left: within(100, 6000, -12), levels: [rmsByte(0.5), rmsByte(0.5), 0, 0] });
  const piece = (burstSeconds) => (k) => {
    const t = k / 30;
    if (t >= 40) return undefined;
    return t >= 10 && t < 10 + burstSeconds ? loud(k) : quiet(k);
  };
  for (const rate of RATES) {
    const short = run({ rate, seconds: 30, frameAt: piece(3) });
    assert.equal(short.events.filter((e) => e.type === 'section').length, 0, `no section change at ${rate}`);
    assert.equal(short.director.state.section, 'intro');
    const long = run({ rate, seconds: 30, frameAt: piece(6) });
    const sections = long.events.filter((e) => e.type === 'section');
    assert.equal(sections[0]?.section, 'drop', `a sustained loud part is a drop at ${rate}`);
    // it takes over only after the pending time past the moment loudness crossed the threshold
    assert.ok(sections[0].at > 10 + SECTION.PENDING_S, `drop at ${sections[0].at}`);
  }
});

ok('section_hysteresis_keeps_a_loudness_near_the_threshold_from_flipping', () => {
  const d = createDirector();
  const input = (loud) => ({ frame: null, raw: new Features().raw, values: { loud, flux: 0 }, now: 0 });
  // a clear drop first
  for (let i = 0; i < 600; i++) d.step(0.01, input(0.7));
  assert.equal(d.state.section, 'drop');
  // 0.46 and 0.54 alternating every 0.5 s: past the entry, never past the exit
  let changes = 0;
  for (let i = 0; i < 4000; i++) {
    const before = d.state.section;
    d.step(0.01, input(Math.floor(i / 50) % 2 ? 0.46 : 0.54));
    if (d.state.section !== before) changes++;
  }
  assert.equal(changes, 0, 'stays a drop');
  // below the exit it goes after the pending time, to a break once a drop was heard
  const d2 = createDirector();
  for (let i = 0; i < 600; i++) d2.step(0.01, input(0.7));
  for (let i = 0; i < 300; i++) d2.step(0.01, input(0.1));
  assert.equal(d2.state.section, 'drop', 'not yet (3 s)');
  assert.equal(d2.state.pending, 'break');
  for (let i = 0; i < 100; i++) d2.step(0.01, input(0.1));
  assert.equal(d2.state.section, 'break');
  // a break holds while loudness stays under the exit (0.28), and ends past it
  for (let i = 0; i < 800; i++) d2.step(0.01, input(0.25));
  assert.equal(d2.state.section, 'break', 'between the entry and the exit it stays');
  for (let i = 0; i < 400; i++) d2.step(0.01, input(0.35));
  assert.equal(d2.state.section, 'build');
});

ok('a_drop_holds_through_loudness_between_the_exit_and_the_entry_for_seconds', () => {
  const d = createDirector();
  const input = (loud) => ({ frame: null, raw: new Features().raw, values: { loud, flux: 0 }, now: 0 });
  for (let i = 0; i < 600; i++) d.step(0.01, input(0.7));
  assert.equal(d.state.section, 'drop');
  // 0.46 is under the entry (0.5) and over the exit (0.42): the drop stays, nothing pends
  const events = [];
  for (let i = 0; i < 500; i++) events.push(...d.step(0.01, input(0.46)));
  assert.equal(d.state.section, 'drop', 'five seconds at 0.46');
  assert.equal(d.state.pending, null);
  assert.equal(events.filter((e) => e.type === 'section').length, 0);
});

ok('a_candidate_that_changes_restarts_the_pending_time_and_one_that_returns_cancels_it', () => {
  const d = createDirector();
  const input = (loud, flux = 0) => ({ frame: null, raw: new Features().raw, values: { loud, flux }, now: 0 });
  for (let i = 0; i < 300; i++) d.step(0.01, input(0.35));
  assert.equal(d.state.pending, 'build');
  for (let i = 0; i < 200; i++) d.step(0.01, input(0.7));
  assert.equal(d.state.pending, 'drop', 'a different candidate replaces it');
  assert.ok(d.state.pendingFor <= 2.01, 'with its own time');
  for (let i = 0; i < 100; i++) d.step(0.01, input(0.1));
  assert.equal(d.state.pending, null, 'back to the section it is in');
  assert.equal(d.state.section, 'intro');
});

ok('a_hits_slow_mean_eases_by_time_so_a_hit_is_the_same_at_30_and_60_frames_a_second', () => {
  // a steady bass level from silence: each hit's strength is the onset over the slow mean, so
  // it falls with the time since the sound began, whatever the frames' spacing
  const amps = (rate) => {
    const d = createDirector();
    const raw = { ...new Features().raw, bass: 0.5 };
    const hits = [];
    for (let k = 1; k <= rate * 0.8; k++) {
      for (const e of d.step(1 / rate, {
        frame: frame({ t: (k * 1000) / rate, levels: [100, 100, 0, 0] }), raw, values: new Features().values, now: (k * 1000) / rate,
      })) if (e.type === 'kick') hits.push(e.amp);
    }
    return hits;
  };
  const a = amps(30);
  const b = amps(60);
  const c = amps(120);
  assert.equal(a.length, 3, 'a kick at the start, and two more while the mean catches up');
  assert.equal(b.length, a.length);
  assert.equal(c.length, a.length);
  a.forEach((v, i) => {
    near(b[i], v, 0.12, `hit ${i} at 60 against 30`);
    near(c[i], v, 0.12, `hit ${i} at 120 against 30`);
  });
  // the third hit sits near 0.5 e^{-3 t} * 3 at t about 0.4 s
  near(a[2], 1.5 * Math.exp(-1.2), 0.12, 'the third hit at the mean\'s own constant');
});

ok('peaks_come_at_most_every_400_ms_and_morphs_every_3_5_s', () => {
  const peaky = (k) => (k < 600 ? frame({ t: (k * 1000) / 30, levels: [100, 100, 255, 255] }) : undefined);
  for (const rate of RATES) {
    const peaks = run({ rate, seconds: 20, frameAt: peaky }).events.filter((e) => e.type === 'peak');
    assert.ok(peaks.length >= 45 && peaks.length <= 51, `peaks ${peaks.length} at ${rate}`);
    for (let i = 1; i < peaks.length; i++) assert.ok(peaks[i].at - peaks[i - 1].at >= 0.4 - 1e-9);
  }
  const d = createDirector();
  const events = [];
  const input = { frame: null, raw: new Features().raw, values: { loud: 0.3, flux: 0.8 }, now: 0 };
  for (let i = 0; i < 2000; i++) events.push(...d.step(0.01, input).map((e) => ({ ...e, at: d.state.time })));
  const morphs = events.filter((e) => e.type === 'morph');
  assert.equal(morphs.length, 6, 'one per 3.5 s over 20 s');
  assert.deepEqual(morphs.map((m) => m.morph), [1, 0, 1, 0, 1, 0], 'each flips the other way');
  for (let i = 1; i < morphs.length; i++) assert.ok(morphs[i].at - morphs[i - 1].at > 3.5);
  // a muted scene makes neither
  const m = createDirector();
  const muted = [];
  for (let i = 0; i < 1000; i++) muted.push(...m.step(0.01, { ...input, mute: true }));
  assert.equal(muted.length, 0);
});

ok('over_decays_in_time_the_same_at_every_step_rate', () => {
  const at = (rate) => {
    const d = createDirector();
    const raw = { ...new Features().raw, peak: 1 };
    d.step(1 / rate, { frame: frame({ t: 0, levels: [100, 100, 255, 255] }), raw, values: new Features().values, now: 0 });
    for (let i = 1; i < rate * 0.25; i++) d.step(1 / rate, { frame: null, raw, values: new Features().values, now: 0 });
    return [d.state.over, d.state.peak, d.state.groove.energy];
  };
  const ref = at(30);
  for (const rate of [60, 144]) {
    at(rate).forEach((v, i) => near(v, ref[i], 0.07, `state ${i} at ${rate}`));
  }
});

ok('a_new_track_fades_out_reseeds_at_black_and_fades_back_in', () => {
  assert.equal(seedOf('Song', 'Artist'), seedOf('Song', 'Artist'));
  assert.notEqual(seedOf('Song', 'Artist'), seedOf('Song', 'Other'));
  assert.notEqual(seedOf('Ab', 'c'), seedOf('A', 'bc'), 'the join is not ambiguous');
  const s = seedOf('Song', 'Artist');
  assert.ok(s >= 0 && s < 100);
  for (const rate of RATES) {
    const d = createDirector();
    const feat = new Features();
    const stepOnce = () => d.step(1 / rate, { frame: null, raw: feat.raw, values: feat.values, now: 0 });
    for (let i = 0; i < rate; i++) stepOnce();
    assert.equal(d.state.fade, 1);
    d.track('Song', 'Artist');
    const events = [];
    let lowest = 1;
    let seedAt = null;
    for (let i = 0; i < rate * 3; i++) {
      for (const e of stepOnce()) { events.push(e); seedAt = d.state.time; }
      lowest = Math.min(lowest, d.state.fade);
    }
    assert.equal(events.filter((e) => e.type === 'reseed').length, 1, `one reseed at ${rate}`);
    assert.equal(events.find((e) => e.type === 'reseed').seed, s);
    assert.equal(d.state.seed, s);
    assert.equal(lowest, 0, 'black');
    assert.equal(d.state.fade, 1, 'back in');
    near(seedAt - 1, 1 / 2.2, 2 / rate, `the new seed lands after the fade out at ${rate}`);
    assert.equal(d.state.section, 'intro');
  }
});

ok('the_flash_limiter_never_grants_over_3_in_any_second', () => {
  assert.equal(MAX_FLASHES, 3);
  const lim = new FlashLimiter();
  const granted = [];
  for (let i = 0; i < 2000; i++) if (lim.grant(i * 0.01)) granted.push(i * 0.01);
  for (const t of granted) {
    assert.ok(granted.filter((g) => g >= t && g < t + 1 - 1e-9).length <= 3, `window from ${t}`);
  }
  assert.ok(granted.length >= 55 && granted.length <= 60, `it still grants 3 a second: ${granted.length}`);
  assert.equal(lim.grant(20), true, 'after a quiet spell a flash is allowed at once');
});

ok('the_director_grants_the_same_flashes_at_30_60_and_144_steps_a_second', () => {
  // a request every 1/6 s (a time on every rate's grid), for 10 s, through the director's clock
  const firings = (rate) => {
    const d = createDirector();
    const raw = new Features().raw;
    const values = new Features().values;
    const out = [];
    for (let i = 1; i <= rate * 10; i++) {
      d.step(1 / rate, { frame: null, raw, values, now: 0 });
      if (i % (rate / 6) === 0 && d.flash()) out.push(Math.round((i * 6) / rate));
    }
    return out;
  };
  const ref = firings(30);
  assert.ok(ref.length >= 28 && ref.length <= 32, `3 of every 6 requests: ${ref.length}`);
  for (const rate of [60, 144]) assert.deepEqual(firings(rate), ref, `same firings at ${rate}`);
  for (const t of ref) assert.ok(ref.filter((g) => g >= t && g < t + 6).length <= 3, 'never over 3 in a second');
});

ok('peaks_and_drops_carry_the_limiters_answer', () => {
  // peaks every 0.4 s want 2.5 a second: all granted; then flash() asked for in between takes the
  // others' share, so a later peak is refused
  const d = createDirector();
  const raw = { ...new Features().raw, peak: 1 };
  const values = new Features().values;
  const peaks = [];
  for (let i = 0; i < 400; i++) {
    const f = i % 41 === 0 ? frame({ t: i * 10, levels: [100, 100, 255, 255] }) : null;
    const events = d.step(0.01, { frame: f, raw: f ? raw : { ...raw, peak: 0 }, values, now: i * 10 });
    if (i === 42) d.flash();
    if (i === 43) d.flash();
    peaks.push(...events.filter((e) => e.type === 'peak'));
  }
  assert.equal(peaks.length, 10);
  assert.ok(peaks.some((p) => !p.flash), 'one is refused for a flash granted just before');
  assert.equal(peaks[0].flash, true);
  const all = peaks.filter((p) => p.flash).length;
  assert.ok(all < 10);
});

// ---- the explore keys (0.40.0 task 3.10) ----
/** Step `d` for `seconds` at `rate`, quietly; returns the events. */
function quiet(d, seconds, rate = 60) {
  const feat = new Features();
  const events = [];
  for (let i = 0; i < Math.round(seconds * rate); i++) {
    events.push(...d.step(1 / rate, { frame: null, raw: feat.raw, values: feat.values, now: 0 }));
  }
  return events;
}

ok('a_new_seed_and_the_next_scene_reseed_through_the_fade_the_same_at_any_step_rate', () => {
  const landed = (rate, kind, times = 1) => {
    const d = createDirector();
    d.state.seed = 12.34;
    quiet(d, 1, rate);
    for (let i = 0; i < times; i++) {
      d.explore(kind);
      const ev = quiet(d, 4, rate);
      assert.equal(ev.filter((e) => e.type === 'reseed').length, 1, `one reseed at ${rate}`);
      assert.equal(d.state.fade, 1, 'back in after the fade');
    }
    return d.state.seed;
  };
  for (const kind of ['seed', 'scene']) {
    const seeds = RATES.map((r) => landed(r, kind));
    assert.equal(new Set(seeds).size, 1, `${kind}: the same world at 30, 60 and 144`);
    assert.notEqual(seeds[0], 12.34, `${kind}: another world`);
    assert.ok(seeds[0] >= 0 && seeds[0] < 100);
  }
  assert.notEqual(landed(60, 'seed'), landed(60, 'scene'), 'a new set is not the next world of the old');
  assert.notEqual(landed(60, 'scene', 2), landed(60, 'scene', 1), 'the next scene walks on');
  assert.equal(landed(60, 'scene', WORLDS), 12.34, 'after the set the track’s own world comes round');
});

ok('a_seed_depends_only_on_the_track_the_set_and_the_world', () => {
  const d = createDirector();
  d.track('Song', 'Artist');
  quiet(d, 4);
  assert.equal(d.state.seed, seedOf('Song', 'Artist'));
  d.explore('scene');
  quiet(d, 4);
  const second = d.state.seed;
  d.track('Song', 'Artist');
  quiet(d, 4);
  d.explore('scene');
  quiet(d, 4);
  assert.equal(d.state.seed, second, 'the same track and world, the same seed: frame(seed, t) is the seed’s alone');
});

ok('an_explore_effect_is_one_peak_or_morph_through_the_flash_limiter_and_none_flashes_when_flashing_is_off', () => {
  const d = createDirector();
  quiet(d, 1);
  d.explore('effect');
  const peak = quiet(d, 0.1).filter((e) => e.type === 'peak');
  assert.equal(peak.length, 1);
  assert.equal(peak[0].flash, true, 'the limiter granted it');
  assert.equal(peak[0].explore, true);
  d.explore('effect');
  assert.equal(quiet(d, 0.1).filter((e) => e.type === 'morph').length, 1, 'the next effect is the other one');
  // the limiter's three grants a second are shared with every other flash
  const l = createDirector();
  quiet(l, 1);
  for (let i = 0; i < MAX_FLASHES; i++) assert.equal(l.flash(), true);
  l.explore('effect');
  assert.equal(quiet(l, 0.05).find((e) => e.type === 'peak').flash, false, 'no fourth flash in the second');
  // reduced motion or Still: the page says flash false, and the limiter keeps its grants
  const r = createDirector();
  quiet(r, 1);
  r.explore('effect', { flash: false });
  assert.equal(quiet(r, 0.1).find((e) => e.type === 'peak').flash, false);
  assert.equal(r.flash(), true);
  const u = createDirector();
  u.explore('nonsense');
  assert.deepEqual(quiet(u, 1).filter((e) => e.type === 'peak' || e.type === 'reseed'), []);
});

console.log(`${n} ok`);
