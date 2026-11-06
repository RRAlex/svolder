/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Night drive's sky events and motion (themes/nightdrive/): the rays
// after a held bass, the shooting star on a transient, both by the clock and never by the frame,
// and the motion levels with their pace.
// Run: node nightdrive-sky.mjs
import assert from 'node:assert/strict';
import { SKY, createSky, raysEnvelope } from '../../../bins/svolderd/web/themes/nightdrive/sky.js';
import { MOTIONS, motionOf, paceOf } from '../../../bins/svolderd/web/themes/nightdrive/motion.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const near = (a, b, eps = 1e-9) => assert.ok(Math.abs(a - b) <= eps, `${a} is not ${b}`);

/** Step `sky` from `from` to `to` seconds at `fps`, with `input(t)`; returns every result's firing times. */
function run(sky, from, to, fps, input) {
  const rays = [];
  const stars = [];
  let seenRays = false;
  const seenStars = new Set();
  const fired = [];
  const seenLog = new Set();
  const dt = 1 / fps;
  for (let t = from; t <= to + 1e-9; t += dt) {
    const out = sky.step(t, { on: true, ...input(t) });
    for (const e of sky.log) if (!seenLog.has(e)) { seenLog.add(e); fired.push(e); }
    if (out.rays > 0 && !seenRays) { rays.push(t); seenRays = true; }
    if (out.rays === 0) seenRays = false;
    for (const s of out.stars) if (!seenStars.has(s.t0)) { seenStars.add(s.t0); stars.push(s.t0); }
  }
  return { rays, stars, fired };
}

const zero = () => 0;

/** The shooting stars a scheduler's log holds that a transient brought (a shower's stars are not these). */
const transients = (sky) => sky.log.filter((e) => e.kind === 'star').length;

/** A scheduler stepped from 0 to `to` seconds at `fps` over `input`. */
function ran(to, fps, input, random = zero) {
  const sky = createSky({ random });
  run(sky, 0, to, fps, input);
  return sky;
}

ok('SKY_is_frozen_with_its_numbers', () => {
  assert.ok(Object.isFrozen(SKY));
  assert.equal(SKY.floor, 0.1);
  assert.equal(SKY.bassMin, 0.8);
  assert.equal(SKY.bassHold, 3);
  assert.equal(SKY.raysGap, 60);
  assert.equal(SKY.raysAlpha, 0.55);
  assert.equal(SKY.starJump, 0.45);
  assert.deepEqual([SKY.envTau, SKY.rangeTau, SKY.range, SKY.rangeShare, SKY.fill, SKY.drain], [0.4, 60, 0.01, 0.1, 1, 0.5]);
  assert.equal(SKY.starMin, 0.5);
  assert.equal(SKY.starGap, 5);
  assert.equal(SKY.starChance, 0.5);
  assert.equal(SKY.starLife, 0.8);
  assert.equal(SKY.showerMin, 0.8);
  assert.equal(SKY.showerHold, 2);
  assert.equal(SKY.showerGap, 40);
});

/** A bass that sits at 0.1 and is at 0.7 from `from` to `to` seconds: a build. */
const build = (from, to, high = 0.7) => (t) => ({ bass: t >= from && t < to ? high : 0.1, highs: 0 });

ok('rays_need_the_bass_held_high_in_its_range_for_3_s', () => {
  // a build of 2 s is not held for 3 s; one of 5 s is
  const short = run(createSky({ random: zero }), 0, 12, 60, build(5, 7));
  assert.equal(short.rays.length, 0);
  const sky = createSky({ random: zero });
  const held = run(sky, 0, 12, 60, build(5, 10));
  assert.equal(held.rays.length, 1);
  assert.equal(sky.log.length, 1);
  assert.equal(sky.log[0].kind, 'rays');
  assert.ok(held.rays[0] > 7.5 && held.rays[0] < 9.5, held.rays[0]);
  // a short dip drains the count but does not clear it: the kick-shaped bass dips between beats
  const dip = run(createSky({ random: zero }), 0, 12, 60, (t) => ({ bass: t >= 5 && t < 10 && !(t > 7 && t < 7.15) ? 0.7 : 0.1, highs: 0 }));
  assert.equal(dip.rays.length, 1, dip.rays);
  // a long dip does
  const gone = run(createSky({ random: zero }), 0, 14, 60, (t) => ({ bass: t >= 5 && t < 10 && !(t > 7 && t < 9) ? 0.7 : 0.1, highs: 0 }));
  assert.equal(gone.rays.length, 0);
  // the same height all the time is no build: a drone has no range to be high in
  assert.equal(run(createSky({ random: zero }), 0, 120, 60, () => ({ bass: 0.7, highs: 0.5 })).rays.length, 0);
});

ok('rays_come_at_most_once_a_minute', () => {
  // a build of 10 s every 20 s for 10 minutes
  const got = run(createSky({ random: zero }), 0, 600, 30, (t) => ({ bass: t % 20 >= 10 ? 0.8 : 0.1, highs: 0 }));
  assert.ok(got.rays.length <= 10 && got.rays.length >= 9, got.rays.length);
  for (let i = 1; i < got.rays.length; i++) assert.ok(got.rays[i] - got.rays[i - 1] >= 60 - 1e-6);
});

ok('the_rays_fade_in_2_s_hold_6_s_fade_out_4_s', () => {
  near(raysEnvelope(0), 0);
  near(raysEnvelope(1), 0.5);
  near(raysEnvelope(2), 1);
  near(raysEnvelope(5), 1);
  near(raysEnvelope(8), 1);
  near(raysEnvelope(10), 0.5);
  near(raysEnvelope(12), 0);
  near(raysEnvelope(20), 0);
  near(raysEnvelope(-1), 0);
  const sky = createSky({ random: zero });
  let peak = 0;
  for (let t = 0; t < 20; t += 0.05) peak = Math.max(peak, sky.step(t, { on: true, bass: t < 5 ? 0.1 : 0.9, highs: 0 }).rays);
  near(peak, 0.55, 1e-6);
});

ok('a_star_needs_a_strong_transient', () => {
  assert.equal(transients(ran(20, 60, () => ({ bass: 0, highs: 0.5 }))), 0, 'steady highs: no transient');
  const jump = ran(3, 60, (t) => ({ bass: 0, highs: t < 0.5 ? 0.2 : 0.5 }));
  assert.equal(transients(jump), 1);
  assert.equal(jump.log[0].kind, 'star');
  assert.equal(transients(ran(3, 60, (t) => ({ bass: 0, highs: t < 0.5 ? 0.35 : 0.5 }))), 0);
  // a rise that is a jump of the range but is under the floor of the range (near silence) is no star
  assert.equal(transients(ran(3, 60, (t) => ({ bass: 0, highs: t < 0.5 ? 0.0 : 0.04 }))), 0);
  // a jump to a level that is not near the minute's loudest is no star: the highs reached 0.9 a moment ago
  const low = createSky({ random: zero });
  low.step(0, { on: true, bass: 0, highs: 0.9 });
  low.step(0.2, { on: true, bass: 0, highs: 0.05 });
  run(low, 0.3, 3, 60, (t) => ({ bass: 0, highs: t < 0.6 ? 0.05 : 0.3 }));
  assert.equal(transients(low), 0);
  const star = createSky({ random: () => 0.5 });
  const lone = star.step(0, { on: true, bass: 0, highs: 0.1 });
  assert.deepEqual(lone.stars, []);
});

ok('a_star_has_its_place_and_lasts_0_8_s', () => {
  const seq = [0.25, 0.5, 0.0];
  let i = 0;
  const sky = createSky({ random: () => seq[i++ % seq.length] });
  sky.step(0, { on: true, bass: 0, highs: 0.1 });
  const out = sky.step(0.1, { on: true, bass: 0, highs: 0.8 });
  assert.equal(out.stars.length, 1);
  const s = out.stars[0];
  assert.ok(s.x >= 0 && s.x <= 1 && s.y >= 0 && s.y <= 0.4 && s.dx !== 0 && Math.abs(s.dx) <= 0.5, s);
  assert.equal(sky.step(0.85, { on: true, bass: 0, highs: 0.1 }).stars.length, 1);
  assert.equal(sky.step(1.0, { on: true, bass: 0, highs: 0.1 }).stars.length, 0);
});

ok('stars_come_at_most_one_every_5_s', () => {
  // a transient every 200 ms: the stars a transient brings (not a shower's) are 5 s apart or more
  const sky = ran(100, 30, (t) => ({ bass: 0, highs: Math.round(t * 10) % 2 ? 0.8 : 0.1 }));
  const at = sky.log.filter((e) => e.kind === 'star').map((e) => e.at);
  assert.ok(at.length >= 8 && at.length <= 20, at.length);
  for (let i = 1; i < at.length; i++) assert.ok(at[i] - at[i - 1] >= 5 - 1e-6);
});

ok('the_dice_is_rolled_for_a_transient_not_for_a_frame', () => {
  // random 0.9 always fails: nothing, however long the transient lasts or fast the frames come
  for (const fps of [30, 144]) {
    assert.equal(transients(ran(30, fps, (t) => ({ bass: 0, highs: t < 1 ? 0.1 : 0.5 }), () => 0.9)), 0);
  }
});

ok('the_limits_hold_at_any_frame_rate', () => {
  const input = (t) => {
    // 200 s: a drop (bass high) from 10 s to 150 s, a burst of highs for 0.5 s every 7.3 s
    const bass = t >= 10 && t < 150 ? 0.8 : 0.1;
    const highs = t % 7.3 < 0.5 ? 0.8 : 0.1;
    return { bass, highs };
  };
  const counts = [30, 60, 144].map((fps) => {
    const got = run(createSky({ random: zero }), 0, 200, fps, input);
    return [got.rays.length, got.stars.length];
  });
  for (const [r, s] of counts) {
    assert.ok(Math.abs(r - counts[0][0]) <= 1 && Math.abs(s - counts[0][1]) <= 1, JSON.stringify(counts));
  }
  assert.ok(counts[0][0] >= 2 && counts[0][1] >= 20, JSON.stringify(counts));
  // one 2 s gap in the stepping gives the same firings
  const sparse = createSky({ random: zero });
  let rays = 0;
  let stars = 0;
  let last = null;
  const seen = new Set();
  for (let t = 0; t <= 200 + 1e-9; t += 1 / 60) {
    if (t > 52 && t < 54) continue;
    const out = sparse.step(t, { on: true, ...input(t) });
    if (out.rays > 0 && last === 0) rays++;
    if (out.rays > 0 && last === null) rays++;
    last = out.rays;
    for (const s of out.stars) if (!seen.has(s.t0)) { seen.add(s.t0); stars++; }
  }
  assert.deepEqual([rays, stars], counts[1]);
  console.log('   firings at 30, 60, 144 steps a second (rays, stars):', JSON.stringify(counts), 'gap:', [rays, stars]);
});

ok('a_quick_rise_is_a_transient_at_any_frame_rate', () => {
  // the highs climb from 0.1 to 0.8 over 0.5 s: the average is a time constant of seconds, so it lags the same
  // distance behind at 30, 60 and 144 steps a second, and the rise is one transient at each
  const rise = (t) => ({ bass: 0, highs: t < 2 ? 0.1 : Math.min(0.8, 0.1 + (t - 2) * 1.4) });
  const counts = [30, 60, 144].map((fps) => transients(ran(5, fps, rise)));
  assert.deepEqual(counts, [1, 1, 1]);
});

ok('a_shower_comes_on_intense_highs_at_most_every_40_s', () => {
  // highs at 0.1 for 4 s of every 10 and at 0.7 for the 6 s after: a build every 10 s
  const intense = (t) => ({ bass: 0, highs: t % 10 >= 4 ? 0.7 : 0.1 });
  // under 2 s of intense highs: nothing
  const brief = createSky({ random: zero });
  run(brief, 0, 12, 60, (t) => ({ bass: 0, highs: t >= 4 && t < 5.5 ? 0.7 : 0.1 }));
  assert.equal(brief.log.filter((e) => e.kind === 'shower').length, 0);
  const showers = [];
  for (const fps of [30, 60, 144]) {
    const sky = createSky({ random: zero });
    const got = run(sky, 0, 200, fps, intense);
    const log = got.fired.filter((e) => e.kind === 'shower');
    showers.push([log.map((e) => Math.round(e.at)), got.stars.length]);
    assert.ok(log.length >= 4 && log.length <= 5, `${log.length} showers in 200 s`);
    for (let i = 1; i < log.length; i++) assert.ok(log[i].at - log[i - 1].at >= 40 - 1e-6);
    assert.ok(log[0].at >= 5.5 && log[0].at < 7.5, log[0].at);
    for (const e of log) assert.ok(e.count >= 3 && e.count <= 6, JSON.stringify(e));
    // a shower's stars ignore the 5 s gap: three of them come within 1.5 s
    const first = got.stars.filter((t) => t >= log[0].at && t < log[0].at + 1.6);
    assert.ok(first.length >= log[0].count, `${first.length} stars for ${log[0].count}`);
    assert.ok(first.at(-1) - first[0] <= 1.5 + 1e-6);
  }
  assert.deepEqual(showers[0], showers[2], 'the same at 30 and at 144 steps a second');
  // a random roll of the count gives 3 to 6
  const many = createSky({ random: () => 0.99 });
  run(many, 0, 9, 60, intense);
  assert.equal(many.log.find((e) => e.kind === 'shower').count, 6);
  // highs that are up for 1 s and down for 2 s never fill the count
  const dip = createSky({ random: zero });
  run(dip, 0, 30, 60, (t) => ({ bass: 0, highs: t % 3 < 1 ? 0.7 : 0.1 }));
  assert.equal(dip.log.filter((e) => e.kind === 'shower').length, 0);
});

ok('the_events_read_the_music_against_its_own_range_so_quiet_listening_fires_what_loud_does', () => {
  const fires = (scale) => {
    // 200 s: the bass held high from 10 to 150 s, a burst of highs for 0.5 s every 7.3 s
    const got = run(createSky({ random: zero }), 0, 200, 30, (t) => ({
      bass: (t >= 10 && t < 150 ? 0.8 : 0.1) * scale,
      highs: (t % 7.3 < 0.5 ? 0.8 : 0.1) * scale,
    }));
    return [got.rays.length, got.stars.length];
  };
  const loud = fires(1);
  assert.ok(loud[0] >= 2 && loud[1] >= 20, JSON.stringify(loud));
  // a quarter of the volume is the same music: the same firings
  assert.deepEqual(fires(0.25), loud);
  assert.deepEqual(fires(0.5), loud);
});

/** A kick-shaped signal (a rise in 0.15 of a period, a fall after it) and a hat-shaped one, at `f` a second. */
const kick = (t, f) => { const x = (t * f) % 1; return x < 0.15 ? x / 0.15 : Math.exp(-(x - 0.15) * 6); };
const hat = (t, f) => { const x = (t * f) % 1; return x < 0.1 ? 1 : Math.exp(-(x - 0.1) * 8); };

/** What a scheduler fires over `secs` s of `input`, counted by kind. */
function fires(input, secs = 300) {
  const got = run(createSky({ random: zero }), 0, secs, 60, input);
  const count = (kind) => got.fired.filter((e) => e.kind === kind).length;
  return { rays: count('rays'), shower: count('shower'), star: count('star') };
}

ok('music_with_dynamics_fires_rays_a_shower_and_stars_at_the_users_levels', () => {
  // the user's live run: the bass 0.16 to 0.54 as kicks at 2 a second, the highs 0.09 to 0.22 as hats at 4 a
  // second, a verse of 40 s at 60 % of the swing and a chorus of 30 s at the full swing, for 300 s
  const swing = (t) => (t % 70 < 40 ? 0.6 : 1);
  const got = fires((t) => ({
    bass: 0.16 + swing(t) * 0.38 * kick(t, 2),
    highs: 0.09 + swing(t) * 0.13 * hat(t, 4),
  }));
  console.log('   events in 300 s of the user-shaped signal (rays, shower, star):', JSON.stringify(got));
  assert.ok(got.rays >= 1 && got.rays <= 4, JSON.stringify(got));
  assert.ok(got.shower >= 1 && got.shower <= 4, JSON.stringify(got));
  assert.ok(got.star >= 3, JSON.stringify(got));
});

ok('the_user_shaped_signal_through_a_30_hz_smoothing_stand_in_fires_at_30_and_60_draws_a_second', () => {
  // the viz model hands out the bands at 30 a second, eased with a time constant of about 36 ms
  const swing = (t) => (t % 70 < 40 ? 0.6 : 1);
  const bass = (t) => 0.16 + swing(t) * 0.38 * kick(t, 2);
  const highs = (t) => 0.09 + swing(t) * 0.13 * hat(t, 4);
  for (const fps of [30, 60]) {
    const eased = { bass: bass(0), highs: highs(0), tick: 0 };
    const input = (t) => {
      for (; eased.tick <= Math.floor(t * 30); eased.tick++) {
        const at = eased.tick / 30;
        const k = 1 - Math.exp(-1 / 30 / 0.036);
        eased.bass += (bass(at) - eased.bass) * k;
        eased.highs += (highs(at) - eased.highs) * k;
      }
      return { bass: eased.bass, highs: eased.highs };
    };
    const sky = createSky({ random: zero });
    const got = run(sky, 0, 300, fps, input).fired;
    const count = (kind) => got.filter((e) => e.kind === kind).length;
    assert.ok(count('rays') >= 1 && count('shower') >= 1, `${fps} draws a second: ${count('rays')} rays, ${count('shower')} showers`);
  }
});

ok('the_rays_turn_clockwise_at_the_named_rate_and_calm_at_half_of_it', () => {
  assert.equal(SKY.raysSpin, 6);
  const turned = (spin, secs) => {
    const sky = createSky({ random: () => 0 });
    // a bass held high from the start brings the rays; the angle is read from then on
    let last = null;
    let fired = null;
    for (let t = 0; t <= 40 && !fired; t += 1 / 60) {
      const out = sky.step(t, { on: true, bass: t < 0.1 ? 0.1 : 0.7, highs: 0, spin });
      if (sky.log.some((e) => e.kind === 'rays')) { fired = t; last = out.raysAngle; }
    }
    assert.ok(fired !== null, 'the rays came');
    for (let t = fired + 1 / 60; t <= fired + secs + 1e-9; t += 1 / 60) last = sky.step(t, { on: true, bass: 0.7, highs: 0, spin }).raysAngle;
    return last;
  };
  const rad = (deg) => deg * Math.PI / 180;
  near(turned(1, 10), rad(60), 0.02);
  near(turned(0.5, 10), rad(30), 0.02);
  // clockwise on a screen whose y points down is a growing angle; the angle does not move while the rays are off
  const idle = createSky({ random: () => 0 });
  for (let t = 0; t < 5; t += 1 / 60) near(idle.step(t, { on: true, bass: 0.1, highs: 0 }).raysAngle, 0);
});

ok('each_showing_of_the_rays_starts_at_a_random_angle', () => {
  const start = (random) => {
    const sky = createSky({ random: () => random });
    for (let t = 0; t <= 40; t += 1 / 60) {
      const out = sky.step(t, { on: true, bass: t < 0.1 ? 0.1 : 0.7, highs: 0 });
      if (sky.log.some((e) => e.kind === 'rays')) return out.raysAngle;
    }
    return null;
  };
  near(start(0.25), Math.PI / 2, 0.02);
  assert.ok(Math.abs(start(0.75) - start(0.25)) > 1);
});

ok('a_flat_drone_and_near_silence_fire_nothing', () => {
  assert.deepEqual(fires(() => ({ bass: 0.1, highs: 0.05 })), { rays: 0, shower: 0, star: 0 });
  assert.deepEqual(fires(() => ({ bass: 0.01, highs: 0.005 })), { rays: 0, shower: 0, star: 0 });
  // a hum that wobbles a little round its level is under the range that counts
  assert.deepEqual(fires((t) => ({ bass: 0.1 + 0.02 * Math.sin(t * 5), highs: 0.05 + 0.01 * Math.sin(t * 9) })), { rays: 0, shower: 0, star: 0 });
  // and a kick-shaped signal whose whole swing is under the range that counts (0.01) is near silence too
  const hush = fires((t) => ({ bass: 0.004 + 0.004 * kick(t, 2), highs: 0.002 + 0.002 * hat(t, 4) }));
  assert.equal(hush.rays + hush.shower, 0, JSON.stringify(hush));
});

ok('a_smooth_bass_swell_from_0_30_to_0_54_over_20_s_fires_rays', () => {
  const got = fires((t) => ({ bass: 0.3 + 0.24 * Math.sin(Math.PI * Math.min(1, t / 20)), highs: 0 }), 40);
  assert.ok(got.rays >= 1, JSON.stringify(got));
});

ok('quiet_listening_fires_what_loud_does', () => {
  const at = (scale) => fires((t) => ({
    bass: (0.16 + (t % 70 < 40 ? 0.6 : 1) * 0.38 * kick(t, 2)) * scale,
    highs: (0.09 + (t % 70 < 40 ? 0.6 : 1) * 0.13 * hat(t, 4)) * scale,
  }));
  const loud = at(1);
  // half the volume is the same music
  const half = at(0.5);
  assert.ok(half.rays === loud.rays && half.shower >= 1 && half.star >= 3, JSON.stringify([loud, half]));
});

ok('the_levels_readout_says_what_the_music_reaches', () => {
  const sky = createSky({ random: zero });
  assert.deepEqual(sky.levels(), {
    bass: 0, highs: 0, bassHeldS: 0, highsJump: 0, bassRel: 0, highsRel: 0, jumpRel: 0, max60: { bass: 0, highs: 0, highsJump: 0 },
  });
  run(sky, 0, 5, 60, (t) => ({ bass: t < 0.5 ? 0.1 : 0.7, highs: t < 1 ? 0.2 : 0.5 }));
  const now = sky.levels();
  near(now.bass, 0.7);
  near(now.highs, 0.5);
  assert.ok(now.bassHeldS > 0 && now.bassHeldS < 3, `the leaky count since the rays: ${now.bassHeldS}`);
  assert.ok(now.max60.highsJump > 0.25 && now.max60.highsJump <= 0.3 + 1e-9, now.max60.highsJump);
  near(now.max60.bass, 0.7);
  near(now.max60.highs, 0.5);
  near(now.bassRel, 1);
  near(now.highsRel, 1);
  assert.ok(now.jumpRel >= 0 && now.jumpRel <= 1, now.jumpRel);
  // the maxima look back 60 s: a minute of quiet forgets them, the current values follow at once
  run(sky, 5.1, 70, 10, zero);
  const later = sky.levels();
  assert.deepEqual([later.bass, later.highs, later.bassHeldS], [0, 0, 0]);
  assert.deepEqual(later.max60, { bass: 0, highs: 0, highsJump: 0 });
  assert.deepEqual([later.bassRel, later.highsRel], [0, 0]);
  // off: the levels are still read, the jump is not
  const off = createSky({ random: zero });
  off.step(0, { on: false, bass: 0.4, highs: 0.6 });
  assert.deepEqual([off.levels().bass, off.levels().highs, off.levels().highsJump], [0.4, 0.6, 0]);
});

ok('a_gap_inside_the_hold_does_not_count_as_held', () => {
  const sky = createSky({ random: zero });
  const fired = [];
  const feed = (t) => {
    sky.step(t, { on: true, bass: t < 0.5 ? 0.1 : 0.7, highs: 0 });
    if (sky.log.length > fired.length) fired.push(t);
  };
  for (let t = 0; t <= 1.5 + 1e-9; t += 1 / 60) feed(t);
  assert.equal(fired.length, 0);
  // the stepping stops for 3 s (a hidden tab), then goes on
  for (let t = 4.5; t <= 10; t += 1 / 60) feed(t);
  assert.equal(fired.length, 1);
  assert.ok(fired[0] >= 4.5 + 3 - 1e-6 && fired[0] <= 4.5 + 3 + 0.05, fired[0]);
});

ok('nothing_fires_when_off', () => {
  const sky = createSky({ random: zero });
  let any = 0;
  for (let t = 0; t < 120; t += 1 / 30) {
    const out = sky.step(t, { on: false, bass: 0.9, highs: Math.floor(t * 4) % 2 ? 0.9 : 0.1 });
    any += out.rays + out.stars.length;
  }
  assert.equal(any, 0);
  assert.equal(sky.log.length, 0);
  // coming on starts the hold afresh
  const first = sky.step(120, { on: true, bass: 0.1, highs: 0 });
  assert.equal(first.rays, 0);
  // going off ends what is shown
  for (let t = 120; t < 124; t += 0.1) sky.step(t, { on: true, bass: 0.9, highs: 0 });
  assert.ok(sky.step(124.1, { on: true, bass: 0.9, highs: 0 }).rays > 0);
  assert.equal(sky.step(124.2, { on: false, bass: 0.9, highs: 0 }).rays, 0);
});

ok('the_log_keeps_the_last_20', () => {
  const sky = createSky({ random: zero });
  for (let t = 0; t < 2000; t += 0.5) sky.step(t, { on: true, bass: 0, highs: Math.floor(t * 2) % 2 ? 0.9 : 0.1 });
  assert.equal(sky.log.length, 20);
  const entry = sky.log[19];
  assert.equal(entry.kind, 'star');
  assert.ok(typeof entry.at === 'number' && typeof entry.highs === 'number' && typeof entry.avg === 'number');
  assert.ok(sky.log[19].at > sky.log[0].at);
  sky.clear();
  assert.equal(sky.step(5000, { on: true, bass: 0, highs: 0 }).stars.length, 0);
});

ok('reduced_motion_starts_on_calm_unless_a_level_is_kept', () => {
  assert.deepEqual(MOTIONS, ['full', 'calm', 'still']);
  const prefs = (v) => ({ get: (name, fallback) => (name === 'motion' && v !== undefined ? v : fallback) });
  assert.equal(motionOf(prefs(undefined), false), 'full');
  assert.equal(motionOf(prefs(undefined), true), 'calm');
  assert.equal(motionOf(prefs('full'), true), 'full');
  assert.equal(motionOf(prefs('still'), false), 'still');
  assert.equal(motionOf(prefs('bogus'), false), 'full');
  assert.equal(motionOf(prefs('bogus'), true), 'calm');
});

ok('the_pace_of_each_level_and_of_silence_and_the_idle_pace_is_the_phones_alone', () => {
  const base = { full: 0, calm: 100, still: 1000 };
  for (const coarse of [false, true]) {
    for (const motion of MOTIONS) {
      for (const [name, state] of [['playing', { silent: false, playing: true }], ['paused', { silent: false, playing: false }], ['silent', { silent: true, playing: true }]]) {
        const got = paceOf({ motion, coarse, ...state });
        // a phone draws a silent or paused scene at 125 ms at least; a desktop keeps the level's own pace
        const want = coarse && name !== 'playing' ? Math.max(base[motion], 125) : base[motion];
        assert.equal(got, want, `${coarse ? 'coarse' : 'fine'} ${motion} ${name}`);
      }
    }
  }
  assert.equal(paceOf({ motion: 'full', silent: true, playing: false }), 0, 'no pointer given: a desktop');
});

console.log(`nightdrive-sky.mjs: ${n} passed`);
