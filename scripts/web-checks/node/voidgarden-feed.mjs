/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for Void Garden's feed (themes/voidgarden/): the seeded randomness, each audio
// feature's range on the recorded frames and its response to a synthetic kick, hat and pad (bands
// set by hand), the smoothed values at any step rate, the 18 s history at any step rate and the
// waveform as a pure function of (bands, seed). Run: node voidgarden-feed.mjs
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import { Features, KEYS, isSilent } from '../../../bins/svolderd/web/themes/voidgarden/features.js';
import { BUCKETS, History, SECONDS } from '../../../bins/svolderd/web/themes/voidgarden/history.js';
import { hashStr, mulberry } from '../../../bins/svolderd/web/themes/voidgarden/rng.js';
import { SAMPLES, waveOf } from '../../../bins/svolderd/web/themes/voidgarden/wave.js';
import { frame, recorded, recordedFrames, rmsByte, within } from './voidgarden-rig.mjs';
import { decode } from '../../../bins/svolderd/web/core/viz-model.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const near = (got, want, eps, what) =>
  assert.ok(Math.abs(got - want) <= eps, `${what}: ${got} is not within ${eps} of ${want}`);

/** The raw features of one frame, from a fresh Features. */
const rawOf = (f) => ({ ...new Features().observe(f) });
const settle = (f, seconds = 3, rate = 60) => {
  const feat = new Features();
  feat.observe(f);
  for (let i = 0; i < seconds * rate; i++) feat.step(1 / rate);
  return { ...feat.values };
};

// --- the randomness ----------------------------------------------------------------------------

ok('hashStr_is_fnv1a_and_mulberry_is_a_seeded_stream', () => {
  assert.equal(hashStr(''), 2166136261);
  assert.equal(hashStr('a'), 0xe40c292c);
  assert.equal(hashStr('foobar'), 0xbf9cf968);
  const a = mulberry(7);
  const b = mulberry(7);
  const run = Array.from({ length: 50 }, () => a());
  assert.deepEqual(run, Array.from({ length: 50 }, () => b()));
  assert.ok(run.every((v) => v >= 0 && v < 1));
  assert.notDeepEqual(run, Array.from({ length: 50 }, ((c) => () => c())(mulberry(8))));
  assert.equal(new Set(run).size, 50);
});

// --- each feature on the recorded frames -------------------------------------------------------

ok('the_recorded_frames_decode_and_every_feature_stays_in_range_and_moves', () => {
  const frames = recordedFrames();
  assert.ok(frames.length > 500, 'a recording of ~20 s');
  assert.equal(frames.length, recorded().length, 'every recorded frame decodes');
  const feat = new Features();
  const rows = frames.map((f) => ({ ...feat.observe(f) }));
  for (const key of [...KEYS, 'peak']) {
    const v = rows.map((r) => r[key]);
    assert.ok(v.every((x) => x >= 0 && x <= 1), `${key} in 0..1`);
    const lo = Math.min(...v);
    const hi = Math.max(...v);
    if (key === 'peak') continue;
    assert.ok(hi - lo > 0.12, `${key} moves (${lo.toFixed(3)}..${hi.toFixed(3)})`);
    // no feature sits at full scale for a third of the piece
    assert.ok(v.filter((x) => x >= 0.999).length < v.length / 3, `${key} does not saturate`);
  }
  // the synthetic piece has a kick every 0.5 s: bass swings with it, its mean is not the floor
  const bass = rows.map((r) => r.bass);
  assert.ok(Math.max(...bass) > 0.7 && bass.filter((x) => x > 0.5).length > frames.length / 12);
});

ok('a_kick_hat_and_pad_each_light_their_own_band', () => {
  const kick = rawOf(frame({ left: within(25, 150, -18) }));
  assert.ok(kick.bass > 0.85, `kick bass ${kick.bass}`);
  assert.ok(kick.mid < 0.1 && kick.high < 0.1, 'the kick is not heard in mids or highs');
  const hat = rawOf(frame({ left: within(2000, 12000, -22) }));
  assert.ok(hat.high > 0.85, `hat high ${hat.high}`);
  assert.ok(hat.bass < 0.1 && hat.mid < 0.1);
  const pad = rawOf(frame({ left: within(150, 2000, -20) }));
  assert.ok(pad.mid > 0.85, `pad mid ${pad.mid}`);
  assert.ok(pad.bass < 0.1 && pad.high < 0.1);
  const silence = rawOf(frame({}));
  assert.equal(silence.bass + silence.mid + silence.high, 0);
});

ok('l_and_r_read_the_rms_bytes_and_loud_is_their_mean', () => {
  const r = rawOf(frame({ levels: [rmsByte(0.1), rmsByte(0.2), 0, 0] }));
  near(r.l, 0.32, 0.02, 'l = 3.2 x 0.1');
  near(r.r, 0.64, 0.02, 'r = 3.2 x 0.2');
  near(r.loud, (r.l + r.r) / 2, 1e-9, 'loud');
  assert.equal(rawOf(frame({ levels: [rmsByte(0.9), rmsByte(0.9), 0, 0] })).l, 1, 'clamped');
});

ok('width_is_zero_for_mono_and_one_for_one_side', () => {
  const mono = rawOf(frame({ left: within(100, 8000, -30), right: within(100, 8000, -30) }));
  assert.equal(mono.width, 0);
  const one = rawOf(frame({ left: within(100, 8000, -30), right: -200 }));
  assert.equal(one.width, 1);
  const some = rawOf(frame({ left: within(100, 8000, -30), right: within(100, 8000, -36) }));
  assert.ok(some.width > 0.05 && some.width < 0.9, `some ${some.width}`);
  assert.equal(rawOf(frame({})).width, 0, 'silence is not wide');
});

ok('peak_is_a_peak_byte_of_255_on_either_side_and_nothing_below', () => {
  assert.equal(rawOf(frame({ levels: [100, 100, 255, 10] })).peak, 1);
  assert.equal(rawOf(frame({ levels: [100, 100, 10, 255] })).peak, 1);
  assert.equal(rawOf(frame({ levels: [100, 100, 254, 254] })).peak, 0);
});

ok('centroid_follows_the_spectrum_and_flatness_tells_noise_from_a_tone', () => {
  const low = rawOf(frame({ left: within(30, 120, -30) }));
  const high = rawOf(frame({ left: within(4000, 14000, -30) }));
  assert.ok(low.cent < 0.25 && high.cent > 0.7, `cent ${low.cent} ${high.cent}`);
  assert.equal(rawOf(frame({})).cent, 0.4, 'no sound: the resting centroid');
  const noise = rawOf(frame({ left: within(30, 14000, -30) }));
  const tone = rawOf(frame({ left: within(990, 1010, -6) }));
  assert.ok(noise.flat > 0.9, `noise ${noise.flat}`);
  assert.ok(tone.flat < 0.45 && tone.flat < noise.flat - 0.5, `tone ${tone.flat}`);
});

ok('flux_rises_on_an_onset_and_is_zero_for_a_steady_sound', () => {
  const feat = new Features();
  let t = 0;
  const quiet = () => frame({ t: (t += 33), left: within(100, 6000, -50) });
  for (let i = 0; i < 90; i++) feat.observe(quiet());
  assert.equal(feat.raw.flux, 0, 'steady');
  feat.observe(frame({ t: (t += 33), left: within(100, 6000, -20) }));
  assert.ok(feat.raw.flux > 0.5, `onset ${feat.raw.flux}`);
  for (let i = 0; i < 90; i++) feat.observe(frame({ t: (t += 33), left: within(100, 6000, -20) }));
  assert.ok(feat.raw.flux < 0.05, 'a held loud sound is steady again');
});

ok('bands_at_mid_scale_map_to_exact_values_so_an_offset_or_a_divisor_moving_fails', () => {
  // each band mean is a dB level: the feature is (mean dB + offset) / span, so these sit at 0.5
  const f = frame({
    left: (hz) => (hz >= 25 && hz <= 150 ? -47 : hz > 150 && hz <= 2000 ? -57 : hz > 2000 && hz <= 12000 ? -66 : -200),
    levels: [100, 100, 0, 0],
  });
  const raw = rawOf(f);
  near(raw.bass, 0.5, 0.01, 'bass at -47 dB');
  near(raw.mid, 0.5, 0.01, 'mid at -57 dB');
  near(raw.high, 0.5, 0.01, 'high at -66 dB');
  // 10 dB louder is 0.2 (bass, mid) or 10/52 (high) more
  const louder = rawOf(frame({
    left: (hz) => (hz >= 25 && hz <= 150 ? -37 : hz > 150 && hz <= 2000 ? -47 : hz > 2000 && hz <= 12000 ? -56 : -200),
    levels: [100, 100, 0, 0],
  }));
  near(louder.bass - raw.bass, 0.2, 0.01, 'bass spans 50 dB');
  near(louder.mid - raw.mid, 0.2, 0.01, 'mid spans 50 dB');
  near(louder.high - raw.high, 10 / 52, 0.01, 'high spans 52 dB');
});

ok('flatness_of_a_known_spectrum_and_flux_of_a_known_ratio_are_exact', () => {
  // half the bands at -30 dB, the other half at the analysis floor (-72): the geometric mean power
  // sits 18 dB under the arithmetic one (2 sqrt(P1 P2) / (P1 + P2) at 42 dB apart), and
  // (-18 + 36) / 26 is 0.69
  const probe = frame({ left: -200, levels: [100, 100, 0, 0] });
  const half = probe.left.length >> 1;
  const bytes = Uint8Array.from(probe.left, (_, i) => (i < half ? 149 : 1));
  const spectrum = { ...probe, left: bytes, right: bytes };
  near(rawOf(spectrum).flat, 0.69, 0.03, 'flatness of half at -30 dB, half at the floor');
  const uniform = frame({ left: -30, levels: [100, 100, 0, 0] });
  assert.equal(rawOf(uniform).flat, 1, 'a flat spectrum is the top');
  // flux: the fast mean 0.5 and the slow 0.2, one steady frame 1/30 s on (the two means ease by
  // their time constants: 0.75^2 and 0.994^2 at 60 a second): the ratio is 1.4233 over 1.25
  const steady = frame({ t: 0, left: within(100, 6000, -30), levels: [100, 100, 0, 0] });
  const feat = new Features();
  feat.observe(steady);
  feat.fluxFast = 0.5;
  feat.fluxSlow = 0.2;
  const raw = feat.observe({ ...steady, t: 1000 / 30 });
  const fast = 0.5 * 0.75 ** 2;
  const slow = 0.2 * 0.994 ** 2;
  near(feat.fluxFast, fast, 0.002, 'the fast mean after 1/30 s');
  near(feat.fluxSlow, slow, 0.001, 'the slow mean after 1/30 s');
  near(raw.flux, (fast / (slow + 1e-4) - 1.25) * 0.9, 0.01, 'flux over the 1.25 threshold');
});

ok('the_flux_means_fall_by_the_same_factor_per_second_at_30_and_60_frames_a_second', () => {
  const quiet = frame({ left: -200, levels: [100, 100, 0, 0] });
  const loud = (t) => frame({ t, left: within(100, 6000, -30), levels: [100, 100, 0, 0] });
  const fall = (rate, seconds) => {
    const feat = new Features();
    feat.observe({ ...quiet, t: 0 });
    feat.observe(loud(1000 / rate));
    const f0 = [feat.fluxFast, feat.fluxSlow];
    for (let k = 2; k <= 1 + Math.round(seconds * rate); k++) feat.observe(loud((k * 1000) / rate));
    return [feat.fluxFast / f0[0], feat.fluxSlow / f0[1]];
  };
  const tauFast = -1 / 60 / Math.log(1 - 0.25);
  const tauSlow = -1 / 60 / Math.log(1 - 0.006);
  for (const [rate, other] of [[30, 60], [60, 120]]) {
    const a = fall(rate, 0.1);
    const b = fall(other, 0.1);
    near(a[0] / b[0], 1, 0.01, `the fast mean after 0.1 s at ${rate} and ${other}`);
    near(a[0], Math.exp(-0.1 / tauFast), 0.01, `the fast mean's fall at ${rate}`);
    const c = fall(rate, 0.5);
    const d = fall(other, 0.5);
    near(c[1] / d[1], 1, 0.01, `the slow mean after 0.5 s at ${rate} and ${other}`);
    near(c[1], Math.exp(-0.5 / tauSlow), 0.01, `the slow mean's fall at ${rate}`);
  }
});

ok('silent_means_both_rms_bytes_zero', () => {
  assert.equal(isSilent(frame({})), true);
  assert.equal(isSilent(frame({ levels: [0, 1, 0, 0] })), false);
});

// --- the smoothed values -----------------------------------------------------------------------

ok('smoothed_values_are_the_same_at_30_60_and_144_steps_a_second', () => {
  const hot = frame({ left: within(25, 150, -18), levels: [rmsByte(0.2), rmsByte(0.3), 0, 0] });
  const at = (rate) => {
    const feat = new Features();
    feat.observe(hot);
    for (let i = 0; i < rate * 0.5; i++) feat.step(1 / rate);
    return feat.values;
  };
  const ref = at(30);
  for (const rate of [60, 144]) {
    const got = at(rate);
    for (const key of KEYS) near(got[key], ref[key], 1e-9, `${key} at ${rate}`);
  }
});

ok('the_envelopes_attack_fast_release_slowly_and_gate_and_mute_act', () => {
  const feat = new Features();
  feat.observe(frame({ left: within(25, 150, -18), levels: [rmsByte(0.3), rmsByte(0.3), 0, 0] }));
  for (let i = 0; i < 12; i++) feat.step(1 / 60);
  assert.ok(feat.values.bass > 0.85, `bass attacks in 0.2 s: ${feat.values.bass}`);
  assert.ok(feat.values.loud < 0.5, `loud attacks slowly: ${feat.values.loud}`);
  const frozen = { ...feat.values };
  feat.step(1, { mute: true });
  assert.deepEqual(feat.values, frozen, 'mute freezes');
  feat.observe(frame({}));
  for (let i = 0; i < 60; i++) feat.step(1 / 60);
  assert.ok(feat.values.bass < 0.01 && feat.values.loud > 0.05, 'bass lets go fast, loud slowly');
  feat.observe(frame({ left: within(25, 150, -18) }));
  for (let i = 0; i < 120; i++) feat.step(1 / 60, { gate: 0 });
  assert.ok(feat.values.bass < 0.001, 'a closed gate pulls to rest');
  near(feat.values.width, 0.5, 0.01, 'and width to the middle');
});

// --- the history -------------------------------------------------------------------------------

ok('history_keeps_18_seconds_at_any_step_rate', () => {
  assert.equal(SECONDS, 18);
  assert.equal(BUCKETS, 144);
  for (const rate of [30, 60, 144]) {
    const h = new History();
    const total = 40;
    for (let i = 0; i < total * rate; i++) {
      const t = (i + 1) / rate;
      h.push(Object.fromEntries(KEYS.map((k) => [k, t / total])), 1 / rate);
    }
    assert.equal(h.filled, BUCKETS, `full at ${rate}`);
    const s = h.series('bass');
    // the ring is a ramp, oldest first: the newest bucket is the mean of the last 1/8 s, the oldest
    // the mean of the bucket 18 s before
    near(s[BUCKETS - 1], (total - 1 / 16) / total, 0.002, `newest at ${rate}`);
    near(s[0], (total - 18 + 1 / 16) / total, 0.004, `oldest at ${rate}`);
    for (let i = 1; i < BUCKETS; i++) assert.ok(s[i] > s[i - 1], `rising at ${rate} bucket ${i}`);
  }
});

ok('history_fills_in_time_not_in_steps_and_holds_peaks', () => {
  const h = new History();
  for (let i = 0; i < 9 * 144; i++) h.push(Object.fromEntries(KEYS.map((k) => [k, 0.5])), 1 / 144);
  assert.equal(h.filled, 72, 'nine seconds fill half the ring');
  const g = new History();
  const v = (x) => Object.fromEntries(KEYS.map((k) => [k, x]));
  g.push(v(0.9), 0.05);
  for (let i = 0; i < 20; i++) g.push(v(0.1), 0.05);
  assert.equal(g.hold('mid'), 0.9, 'held for 1.2 s');
  for (let i = 0; i < 20; i++) g.push(v(0.1), 0.05);
  assert.ok(g.hold('mid') < 0.9 && g.hold('mid') > 0.1, 'then falling');
  for (let i = 0; i < 100; i++) g.push(v(0.1), 0.05);
  near(g.hold('mid'), 0.1, 1e-9, 'meets the value');
});

// --- the waveform ------------------------------------------------------------------------------

ok('the_waveform_is_a_pure_function_of_bands_and_seed', () => {
  const bands = Array.from({ length: 64 }, (_, i) => 0.5 + 0.4 * Math.sin(i));
  const a = waveOf(bands, 12.5);
  assert.equal(a.length, SAMPLES);
  assert.deepEqual(a, waveOf([...bands], 12.5));
  assert.notDeepEqual(a, waveOf(bands, 12.6));
  const louder = waveOf(bands.map((x) => x * 1.2), 12.5);
  assert.notDeepEqual(a, louder);
  assert.ok(a.every((x) => x >= -1 && x <= 1));
  const into = new Float32Array(SAMPLES);
  assert.equal(waveOf(bands, 12.5, into), into);
  assert.deepEqual(into, a);
});

ok('a_silent_spectrum_is_a_flat_line_and_one_band_is_one_sine', () => {
  assert.ok(waveOf(new Array(64).fill(0), 3).every((x) => x === 0));
  const one = new Array(64).fill(0);
  one[0] = 1;
  const w = waveOf(one, 3);
  // band 0 is harmonic 1: one cycle across the window, so two zero crossings
  let crossings = 0;
  for (let i = 1; i < w.length; i++) if (Math.sign(w[i]) !== Math.sign(w[i - 1])) crossings++;
  assert.ok(crossings >= 1 && crossings <= 3, `crossings ${crossings}`);
  const top = new Array(64).fill(0);
  top[63] = 1;
  const t = waveOf(top, 3);
  let c2 = 0;
  for (let i = 1; i < t.length; i++) if (Math.sign(t[i]) !== Math.sign(t[i - 1])) c2++;
  assert.ok(c2 > 100, `the top band is fast: ${c2}`);
  const quiet = waveOf(new Array(64).fill(0.1), 3);
  const loud = waveOf(new Array(64).fill(0.5), 3);
  const rms = (x) => Math.sqrt(x.reduce((s, v) => s + v * v, 0) / x.length);
  assert.ok(rms(loud) > 3 * rms(quiet));
});

ok('recorded_wire_frames_are_the_shape_decode_reads', () => {
  const wire = recorded()[0];
  assert.deepEqual(Object.keys(wire).sort(), ['beat', 'bpm', 'l', 'lv', 'phase', 'r', 't']);
  assert.ok(decode(wire));
});

// --- the feed's modules are pure -----------------------------------------------------------------

ok('the_feed_modules_use_no_clock_no_random_no_dom_and_import_only_the_two_core_models', () => {
  const dir = new URL('../../../bins/svolderd/web/themes/voidgarden/', import.meta.url);
  const names = ['rng', 'features', 'director', 'cues', 'history', 'wave', 'palette'];
  for (const name of names) {
    assert.ok(readdirSync(dir).includes(`${name}.js`), name);
    const source = readFileSync(new URL(`${name}.js`, dir), 'utf8');
    const code = source.replace(/\/\*[\s\S]*?\*\//g, '').replace(/\/\/.*$/gm, '');
    for (const banned of ['Math.random', 'Date.now', 'performance.now', 'document', 'window', 'canvas', 'setTimeout', 'setInterval', 'requestAnimationFrame']) {
      assert.ok(!code.includes(banned), `${name}.js uses ${banned}`);
    }
    for (const m of code.matchAll(/from '([^']+)'/g)) {
      assert.ok(
        m[1].startsWith('./') || m[1] === '../../core/viz-model.js' || m[1] === '../../core/vu-model.js',
        `${name}.js imports ${m[1]}`,
      );
    }
  }
});

console.log(`${n} ok`);
