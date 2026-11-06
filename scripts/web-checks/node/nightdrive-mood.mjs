/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Night drive's mood (themes/nightdrive/mood.js, look.js, embers.js): the ear's
// levels read against the last minute's loudest, the medium and the long view, the balance, a moment at a
// drop and the embers of a peak; what the look makes of them at each Motion level; the sparks. All by the
// clock: the same music gives the same mood at 30, 60 and 144 steps a second. Run: node nightdrive-mood.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { BANDS } from '../../../bins/svolderd/web/core/viz-model.js';
import { EMBERS, capOf, createEmbers } from '../../../bins/svolderd/web/themes/nightdrive/embers.js';
import { LOOK, NEUTRAL, createLook, flareOf } from '../../../bins/svolderd/web/themes/nightdrive/look.js';
import { EAR, createEar } from '../../../bins/svolderd/web/themes/nightdrive/mood.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const near = (a, b, eps = 1e-9) => assert.ok(Math.abs(a - b) <= eps, `${a} is not ${b}`);

/** Bands at `level` everywhere, `bass` and `highs` over the bass and the highs when given. */
function bands(level, { bass = level, highs = level } = {}) {
  const out = new Float32Array(BANDS).fill(level);
  out.fill(bass, 0, EAR.bassBands);
  out.fill(highs, BANDS - EAR.highBands);
  return out;
}

/** Run `ear` for `secs` at `fps` over `level` (a number, or a function of the seconds into this run); the
 * times of its moments, relative to the start of the ear's life, and the last state. */
function run(ear, secs, fps, level, from = 0) {
  const moments = [];
  const dt = 1 / fps;
  let state = null;
  for (let t = dt; t <= secs + 1e-9; t += dt) {
    const at = typeof level === 'function' ? level(t) : level;
    state = ear.step(dt, at instanceof Float32Array ? at : bands(at));
    if (state.moment) moments.push(from + t);
  }
  return { moments, state };
}

/** An ear that has heard `secs` of silence: the medium and the long view at 0. */
function silent(secs = 120) {
  const ear = createEar();
  run(ear, secs, 10, 0);
  return ear;
}

ok('the_ears_numbers_are_the_briefs', () => {
  assert.ok(Object.isFrozen(EAR));
  assert.deepEqual([EAR.med, EAR.long, EAR.balance, EAR.momentRise, EAR.momentMin, EAR.momentGap, EAR.loud, EAR.emberAfter, EAR.emberTau, EAR.peakFloor],
    [1.2, 8, 6, 0.2, 0.6, 15, 0.78, 6, 1.5, 0.05]);
});

ok('a_level_is_read_against_the_loudest_of_the_last_minute', () => {
  const ear = silent();
  const loud = run(ear, 20, 30, 0.6).state;
  near(loud.level, 0.6, 1e-6);
  near(loud.rel, 1, 1e-6);
  const quiet = run(ear, 5, 30, 0.15).state;
  assert.ok(quiet.rel > 0.2 && quiet.rel < 0.3, `a quarter of the peak: ${quiet.rel}`);
  assert.ok(quiet.peak > 0.55, 'the peak is held');
  // a minute on, the peak has fallen away toward the music being played
  const later = run(ear, 300, 10, 0.15).state;
  assert.ok(later.peak < 0.2 && later.rel > 0.75, `${later.peak} ${later.rel}`);
  assert.ok(run(silent(), 60, 10, 0.01).state.peak >= EAR.peakFloor, 'never under the floor');
});

ok('the_medium_view_eases_over_1_2_s_and_the_long_over_8_s', () => {
  const ear = silent();
  const one = run(ear, 1.2, 60, 0.5).state;
  near(one.med, 1 - Math.exp(-1), 0.02);
  near(one.long, 1 - Math.exp(-1.2 / 8), 0.02);
  const more = run(ear, 6.8, 60, 0.5).state;
  near(more.long, 1 - Math.exp(-1), 0.02);
});

ok('quiet_music_at_a_low_volume_still_reaches_long_near_1_after_a_minute', () => {
  const state = run(silent(), 60, 30, 0.06).state;
  assert.ok(state.long > 0.99, `${state.long}`);
  const lower = run(silent(), 60, 30, 0.2).state;
  assert.ok(lower.long > 0.99, `${lower.long}`);
});

ok('a_drop_after_a_breakdown_fires_one_moment_and_a_steady_loud_song_fires_none_after_the_first', () => {
  const song = (t) => {
    if (t < 30) return 0.5;
    if (t < 50) return 0.12;
    return 0.5;
  };
  const got = run(silent(), 80, 30, song);
  assert.equal(got.moments.length, 2, `the song's start and the drop: ${got.moments}`);
  assert.ok(got.moments[0] < 5 && got.moments[1] > 50 && got.moments[1] < 53, `${got.moments}`);
  // a steady song after silence: the first, and none after it
  const steady = run(silent(), 300, 30, 0.5);
  assert.equal(steady.moments.length, 1, `${steady.moments}`);
  // the page opens on no drop at all: the ear starts at the look the scene always had
  assert.equal(run(createEar(), 120, 30, 0.5).moments.length, 0);
});

ok('a_moment_comes_at_most_once_every_15_s', () => {
  // a drop every 6 s: quiet 3 s, loud 3 s
  const got = run(silent(), 240, 30, (t) => (t % 6 < 3 ? 0.1 : 0.5));
  assert.ok(got.moments.length >= 5, `${got.moments.length}`);
  for (let i = 1; i < got.moments.length; i++) assert.ok(got.moments[i] - got.moments[i - 1] > 15, `${got.moments}`);
});

ok('the_balance_leans_to_the_highs_or_the_bass_and_silence_holds_it', () => {
  const bright = run(silent(), 40, 30, bands(0.3, { bass: 0.1, highs: 0.5 })).state;
  assert.ok(bright.balance > 0.65, `${bright.balance}`);
  const dark = run(silent(), 40, 30, bands(0.3, { bass: 0.6, highs: 0.1 })).state;
  assert.ok(dark.balance < 0.2, `${dark.balance}`);
  const ear = silent();
  run(ear, 40, 30, bands(0.3, { bass: 0.6, highs: 0.1 }));
  const held = ear.read().balance;
  near(run(ear, 120, 30, 0).state.balance, held, 1e-9);
  near(run(ear, 120, 30, 0.01).state.balance, held, 1e-9);
  // eased over 6 s, not a jump
  const step = run(silent(), 1, 30, bands(0.3, { bass: 0.1, highs: 0.5 })).state;
  assert.ok(step.balance < 0.6 && step.balance > 0.45, `${step.balance}`);
});

ok('a_peak_of_6_s_brings_the_embers_which_ease_in_and_out', () => {
  const ear = silent();
  const early = run(ear, 5, 30, 0.5).state;
  assert.ok(early.med > 0.78 && early.embers === 0, `${early.med} ${early.embers}`);
  const later = run(ear, 8, 30, 0.5).state;
  assert.ok(later.peakHeld > 6 && later.embers > 0.5, `${later.peakHeld} ${later.embers}`);
  const long = run(ear, 12, 30, 0.5).state;
  assert.ok(long.embers > 0.95, `${long.embers}`);
  const gone = run(ear, 25, 30, 0.05).state;
  assert.ok(gone.embers < 0.05, `${gone.embers}`);
  assert.ok(gone.peakHeld < 6, 'the count of seconds falls at twice the rate');
  assert.equal(run(silent(), 5, 30, 0.5).state.embers, 0, 'a peak of 5 s is not enough');
});

ok('the_ear_is_the_same_at_30_60_and_144_steps_a_second_and_after_a_gap', () => {
  const song = (t) => (t < 20 ? 0.5 : t < 35 ? 0.1 : 0.5);
  const seen = [30, 60, 144].map((fps) => {
    const ear = silent();
    const got = run(ear, 60, fps, song);
    return { moments: got.moments.length, med: got.state.med, long: got.state.long, embers: got.state.embers, balance: got.state.balance };
  });
  for (const s of seen) {
    assert.equal(s.moments, seen[0].moments);
    for (const k of ['med', 'long', 'embers', 'balance']) near(s[k], seen[0][k], 0.03);
  }
  // one 2 s gap in the stepping (a stall) gives the same
  const ear = silent();
  let moments = 0;
  for (let t = 1 / 60; t <= 60; t += 1 / 60) {
    if (t > 40 && t < 42) continue;
    const dt = t > 42 && t < 42.02 ? 2 + 1 / 60 : 1 / 60;
    if (ear.step(dt, bands(song(t))).moment) moments++;
  }
  assert.equal(moments, seen[0].moments);
});

ok('clear_starts_the_ear_again', () => {
  const ear = silent();
  run(ear, 20, 30, 0.5);
  ear.clear();
  near(ear.read().long, EAR.start);
  assert.equal(ear.read().embers, 0);
});

const heard = (over = {}) => ({
  med: 0.7, long: 0.7, balance: EAR.balanceStart, moment: false, embers: 0, ...over,
});

ok('the_mood_is_0_18_plus_0_82_of_the_long_view_and_0_7_looks_as_it_did', () => {
  const look = createLook();
  near(look.step(1, heard({ long: 0 })).mood, 0.18);
  near(createLook().step(1, heard({ long: 1 })).mood, 1);
  near(createLook().step(1, heard({ long: 0.5 })).mood, 0.59);
  assert.equal(LOOK.neutral, 0.7);
  near(NEUTRAL.mood, 0.7);
  const bare = createLook().step(1, heard({ long: 0 }), 'full', new Set(['nomood']));
  assert.equal(bare.mood, 0.7);
  assert.equal(bare.cruise, 1);
  assert.equal(bare.x, 0);
});

ok('the_road_is_wet_and_the_air_hazy_in_the_quiet_and_dry_and_clear_when_loud', () => {
  const at = (med, off) => createLook().step(1, heard({ med }), 'full', off);
  near(at(0, new Set()).wet, 0.6);
  near(at(0.2, new Set()).wet, 0.75 - 0.22);
  assert.equal(at(0.7, new Set()).wet, 0);
  assert.equal(at(1, new Set()).wet, 0);
  near(at(0, new Set()).haze, 0.75);
  near(at(0.3, new Set()).haze, 0.85 - 0.36);
  assert.equal(at(0.8, new Set()).haze, 0);
  assert.equal(at(0, new Set(['nowet'])).wet, 0);
  assert.equal(at(0, new Set(['nohaze'])).haze, 0);
  assert.ok(at(0, new Set(['nowet'])).haze > 0.7, 'one switch leaves the others');
});

ok('the_heat_leans_warm_for_bass_and_cool_for_bright_music', () => {
  const x = (balance) => createLook().step(1, heard({ balance })).x;
  near(x(0.45), 0);
  near(x(0.35), -0.4);
  assert.equal(x(0.1), -1);
  near(x(0.55), 0.4);
  assert.equal(x(0.9), 1);
});

ok('the_cruise_speed_is_0_6_to_1_4_times_the_tempo_s_and_eases_over_seconds', () => {
  const look = createLook();
  near(look.step(1, heard({ long: 0 })).cruise, 0.6);
  const slow = createLook();
  slow.step(1, heard({ long: 0 }));
  const after1 = slow.step(1, heard({ long: 1 })).cruise;
  assert.ok(after1 > 0.6 && after1 < 1.3, `${after1}`);
  let c = after1;
  for (let i = 0; i < 20; i++) c = slow.step(1, heard({ long: 1 })).cruise;
  near(c, 1.4, 0.01);
});

ok('a_moment_flares_the_sun_for_2_5_s_and_sends_a_wave_in_full_only', () => {
  near(flareOf(0), 0);
  assert.ok(flareOf(0.15) > 0.8 && flareOf(0.15) <= 1);
  assert.ok(flareOf(1) < flareOf(0.3));
  assert.equal(flareOf(2.5), 0);
  assert.equal(flareOf(-1), 0);
  assert.equal(flareOf(Infinity), 0);
  const run1 = (motion) => {
    const look = createLook();
    look.step(0.1, heard(), motion);
    look.step(0.05, heard({ moment: true }), motion);
    return look.step(0.2, heard(), motion);
  };
  const full = run1('full');
  const calm = run1('calm');
  const still = run1('still');
  assert.ok(full.flare > 0.4 && full.wave > 0.4, JSON.stringify(full));
  near(calm.flare, full.flare * LOOK.calmFlare, 1e-9);
  assert.equal(calm.wave, 0);
  assert.equal(still.flare, 0);
  assert.equal(still.wave, 0);
  assert.equal(run1('full').since, full.since);
  const bare = createLook();
  bare.step(0.1, heard({ moment: true }), 'full', new Set(['nomood']));
  assert.equal(bare.read().flare, 0);
});

ok('calm_halves_the_ears_rates_of_change_and_still_takes_the_colours_as_they_come', () => {
  // the same loud passage after silence: the long view after 8 s is 1 - e^-1 in Full and 1 - e^-0.5 in Calm
  const hear = (slow) => {
    const ear = silent();
    let state = null;
    for (let t = 1 / 60; t <= 8 + 1e-9; t += 1 / 60) state = ear.step(1 / 60, bands(0.5), slow);
    return state;
  };
  near(hear(1).long, 1 - Math.exp(-1), 0.02);
  near(hear(2).long, 1 - Math.exp(-0.5), 0.02);
  near(hear(2).med, 1 - Math.exp(-8 / 2.4), 0.02);
  // the look takes what the ear says, at each Motion level alike
  const take = (motion) => createLook().step(1, heard({ long: 0.5, med: 0.2 }), motion);
  near(take('calm').mood, take('full').mood);
  near(take('still').wet, take('full').wet);
});

ok('calm_halves_the_balances_rate_of_change_too_and_the_scene_hands_it_slow_2', () => {
  // all highs after the start: the balance climbs from 0.45 toward 1 with a time constant of 6 s (12 s in Calm)
  const highsOnly = Float32Array.from({ length: BANDS }, (_, i) => (i >= BANDS - 80 ? 0.5 : 0));
  const balanceAfter = (slow) => {
    const ear = silent();
    let state = null;
    for (let t = 1 / 60; t <= 6 + 1e-9; t += 1 / 60) state = ear.step(1 / 60, highsOnly, slow);
    return state.balance;
  };
  const full = balanceAfter(1);
  const calm = balanceAfter(2);
  assert.ok(full > 0.45 && calm > 0.45 && calm < full, `${calm} ${full}`);
  near((calm - EAR.balanceStart) / (1 - EAR.balanceStart), (1 - Math.exp(-0.5)), 0.05);
  // the scene passes 2 for Calm and 1 otherwise: a pass of 1 for Calm would leave it at the Full rate
  const wire = readFileSync(new URL('../../../bins/svolderd/web/themes/nightdrive/scene-wire.js', import.meta.url), 'utf8');
  assert.match(wire, /ear\.step\(dt, ridge, view\.motion === 'calm' \? 2 : 1, /);
  assert.doesNotMatch(wire, /ear\.step\(dt, ridge, 1, /);
});

ok('still_finds_no_moment_so_spends_none_and_a_flare_stops_on_a_switch_to_it', () => {
  const ear = silent();
  // a drop in Still: nothing found, no gap started
  let still = 0;
  for (let t = 0; t < 3; t += 1 / 30) if (ear.step(1 / 30, bands(0.5), 1, false).moment) still++;
  assert.equal(still, 0);
  // the same drop in Full soon after is found at once (the 15 s gap was not spent)
  const full = createEar();
  run(full, 120, 10, 0);
  let moments = 0;
  for (let t = 0; t < 2; t += 1 / 30) if (full.step(1 / 30, bands(0.5), 1, false).moment) moments++;
  for (let t = 0; t < 2; t += 1 / 30) if (full.step(1 / 30, bands(0.5), 1, true).moment) moments++;
  assert.equal(moments, 1);
  // a flare running when the level goes to Still is gone at once
  const look = createLook();
  look.step(0.1, heard(), 'full');
  look.step(0.05, heard({ moment: true }), 'full');
  assert.ok(look.step(0.2, heard(), 'full').flare > 0.4);
  const stilled = look.step(0.2, heard(), 'still');
  assert.equal(stilled.flare, 0);
  assert.equal(stilled.wave, 0);
});

ok('embers_only_at_a_peak_and_only_in_full', () => {
  const at = (motion, off) => createLook().step(1, heard({ embers: 0.8 }), motion, off).embers;
  near(at('full', new Set()), 0.8);
  assert.equal(at('calm', new Set()), 0);
  assert.equal(at('still', new Set()), 0);
  assert.equal(at('full', new Set(['noembers'])), 0);
});

/** A small deterministic dice. */
function dice(seed = 7) {
  let s = seed;
  return () => {
    s = (s * 1664525 + 1013904223) % 4294967296;
    return s / 4294967296;
  };
}

/** The floor's corners, `level` everywhere, for a scene `w` by `h` with the horizon at `H`. */
function grid(level, w = 1280, h = 800, H = 448) {
  const rows = 22;
  const stride = 25;
  const x = new Float32Array((rows + 1) * stride);
  const y = new Float32Array((rows + 1) * stride);
  const v = new Float32Array((rows + 1) * stride).fill(level);
  for (let k = 0; k <= rows; k++) {
    for (let i = 0; i < stride; i++) {
      x[k * stride + i] = (w * i) / 24;
      y[k * stride + i] = H + ((h - H) * k) / rows;
    }
  }
  return { x, y, v, rows, stride, w, h, H };
}

ok('embers_rise_from_hot_cells_only_while_the_peak_lasts_and_fade_and_die', () => {
  const g = grid(0.9);
  const ember = createEmbers({ random: dice() });
  const input = (strength, grid2 = g) => ({ strength, crossings: grid2, w: grid2.w, h: grid2.h, H: grid2.H });
  for (let i = 0; i < 120; i++) ember.step(1 / 60, input(0.04));
  assert.equal(ember.list.length, 0, 'nothing under the strength of 0.05');
  for (let i = 0; i < 240; i++) ember.step(1 / 60, input(1, grid(0.54)));
  assert.equal(ember.list.length, 0, 'cells under 0.55 bring none, at any strength');
  for (let i = 0; i < 60; i++) ember.step(1 / 60, input(1));
  assert.ok(ember.list.length > 10, `${ember.list.length}`);
  const p = ember.list[0];
  const y0 = p.y;
  const life0 = p.life;
  ember.step(0.1, input(0));
  assert.ok(p.y < y0 && p.life < life0, 'it rises and fades');
  assert.ok(ember.list.every((e) => e.y >= g.H * EMBERS.top), 'none above the top');
  // with the music gone the sparks die out within their life
  for (let i = 0; i < 60 * 2.2; i++) ember.step(1 / 60, input(0));
  assert.equal(ember.list.length, 0);
});

ok('sparks_rise_only_from_the_near_half_of_the_floor', () => {
  const rise = (hot) => {
    const g = grid(0);
    for (let k = 0; k <= g.rows; k++) if (hot(k)) g.v.fill(0.95, k * g.stride, (k + 1) * g.stride);
    const ember = createEmbers({ random: dice(3) });
    for (let i = 0; i < 240; i++) ember.step(1 / 60, { strength: 1, crossings: g, w: g.w, h: g.h, H: g.H });
    return ember.list.length;
  };
  const first = Math.floor(22 * EMBERS.from);
  assert.equal(rise((k) => k < first), 0, 'the far rows bring none');
  assert.ok(rise((k) => k >= first) > 5, 'the near rows do');
});

ok('the_number_alive_is_capped_by_the_canvas_s_area', () => {
  assert.equal(capOf(1280, 800), EMBERS.max);
  assert.equal(capOf(1920, 1200), EMBERS.max);
  const phone = capOf(412, 915);
  assert.ok(phone >= EMBERS.min && phone < 45, `${phone}`);
  assert.equal(capOf(50, 50), EMBERS.min);
  for (const [w, h] of [[1280, 800], [412, 915]]) {
    const g = grid(1, w, h, h * 0.56);
    const ember = createEmbers({ random: dice(11) });
    for (let i = 0; i < 600; i++) ember.step(1 / 60, { strength: 1, crossings: g, w, h, H: g.H });
    assert.ok(ember.list.length <= capOf(w, h) && ember.list.length >= capOf(w, h) - 3, `${w}: ${ember.list.length}`);
  }
});

ok('the_births_follow_the_strength_and_the_clock_not_the_frame', () => {
  const births = (fps, strength) => {
    const g = grid(0.8);
    const ember = createEmbers({ random: dice(5) });
    let total = 0;
    // every spark is born and none dies inside the count: count the new ones by identity
    const seen = new Set();
    for (let t = 0; t < 1; t += 1 / fps) {
      ember.step(1 / fps, { strength, crossings: g, w: g.w, h: g.h, H: g.H });
      for (const p of ember.list) if (!seen.has(p)) { seen.add(p); total++; }
    }
    return total;
  };
  const full = [30, 60, 144].map((fps) => births(fps, 0.5));
  for (const c of full) assert.ok(c > 0.6 * full[1] && c < 1.5 * full[1], JSON.stringify(full));
  const weak = births(60, 0.15);
  assert.ok(weak < 0.5 * full[1], `${weak} against ${full[1]}`);
});

console.log(`${n} mood checks passed`);
