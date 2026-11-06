/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Night drive's ridge as heights (themes/nightdrive/ridge.js): no
// sawtooth in silence, a bass band as a rounded hill, the swell's drift and its freeze in Still, and
// the swell giving way to the music.
// Run: node nightdrive-ridge.mjs
import assert from 'node:assert/strict';
import { BANDS } from '../../../bins/svolderd/web/core/viz-model.js';
import { drawRidge } from '../../../bins/svolderd/web/themes/nightdrive/draw/road.js';
import {
  RIDGE, driftSwell, ridgeHeights, smoothUnits,
} from '../../../bins/svolderd/web/themes/nightdrive/ridge.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const W = 1280;
const H = 800;
const heights = (units, phase = 0) => {
  const soft = smoothUnits(units, new Float32Array(BANDS));
  const out = new Float32Array(Math.ceil(W / RIDGE.step) + 1);
  const count = ridgeHeights({ soft, w: W, h: H, phase }, out);
  return out.slice(0, count);
};
const second = (a) => Array.from({ length: a.length - 2 }, (_, i) => a[i] - 2 * a[i + 1] + a[i + 2]);

ok('the_ridge_has_no_sawtooth_in_silence', () => {
  for (const phase of [0, 0.13, 0.5]) {
    const a = heights(new Float32Array(BANDS), phase);
    const d = second(a);
    const signs = d.filter((v) => Math.abs(v) > 1e-4).map(Math.sign);
    const flips = signs.filter((s, i) => i > 0 && s !== signs[i - 1]).length;
    assert.ok(flips <= 6, `phase ${phase}: ${flips} changes of the second difference's sign`);
    assert.ok(Math.max(...d.map(Math.abs)) < 0.1, 'no spike');
    assert.ok(Math.max(...a) <= RIDGE.swell.amp * H + 1e-6 && Math.min(...a) >= 0, 'the swell stays under 0.012h');
  }
});

ok('a_notch_a_quarter_octave_wide_is_still_there_after_smoothing', () => {
  // the bands are 1/24 octave: 6 of them are a quarter octave, 2 of them a twelfth
  for (const [first, width] of [[100, 6], [150, 2]]) {
    const units = new Float32Array(BANDS).fill(0.8);
    for (let i = 0; i < width; i++) units[first + i] = 0;
    const a = heights(units);
    const plateau = 0.8 * RIDGE.scale * H;
    const seen = plateau - Math.min(...a);
    assert.ok(seen >= 0.4 * plateau, `a notch of ${width} bands shows ${(100 * seen / plateau).toFixed(0)} % of its depth`);
  }
});

ok('a_steady_spectrum_with_a_ripple_between_bands_shows_no_alternating_step', () => {
  // every other band 0.03 higher, as a steady analysis ripples: no tooth of more than half a px survives the smoothing.
  // The tooth is the second difference of the smoothed bands (in px of height), which does not depend on how often the
  // ridge is sampled; the samples' own second difference would shrink with a finer step and let a rougher kernel pass.
  const units = Float32Array.from({ length: BANDS }, (_, i) => 0.5 + (i % 2 ? 0.015 : -0.015));
  const a = heights(units);
  const soft = smoothUnits(units, new Float32Array(BANDS));
  let tooth = 0;
  for (let i = 10; i < BANDS - 10; i++) tooth = Math.max(tooth, Math.abs(soft[i - 1] - 2 * soft[i] + soft[i + 1]) * RIDGE.scale * H);
  assert.ok(tooth < 0.5, `a tooth of ${tooth.toFixed(2)} px (the bands differ by ${(0.03 * RIDGE.scale * H).toFixed(1)} px)`);
  assert.ok(Math.max(...second(a).map(Math.abs)) < 0.5, 'and the samples show no step either');
  // and without the smoothing it would be several px
  const bare = Float32Array.from(units);
  const out = new Float32Array(Math.ceil(W / RIDGE.step) + 1);
  const count = ridgeHeights({ soft: bare, w: W, h: H, phase: 0 }, out);
  assert.ok(Math.max(...second(out.slice(0, count)).map(Math.abs)) > 2, 'the ripple is there to be smoothed');
});

ok('a_bass_band_makes_a_rounded_hill', () => {
  const units = new Float32Array(BANDS);
  units[20] = 1;
  const a = heights(units);
  const peak = Math.max(...a);
  assert.ok(peak > 0.03 * H, `a hill ${peak.toFixed(1)} px tall`);
  const d = second(a);
  // a lone band is a hill of a width, bracketed from both sides round the kernel in use (sigma 0.87, radius 2; the second
  // difference is taken per 5 px of axis): twice as wide (sigma 1.74, radius 4) gives a ratio of 0.28 and a peak of 36 px,
  // half as wide (0.435) 2.12 and 109 px (an unsmoothed band, about 1.8 at a 5 px step); this one 0.97 and 65 px
  const ratio = (Math.max(...d.map(Math.abs)) / peak) * (5 / RIDGE.step) ** 2;
  assert.ok(ratio >= 0.5 && ratio <= 1.2, `the second difference is ${ratio.toFixed(2)} of the peak`);
  assert.ok(peak >= 45 && peak <= 85, `the lone band's hill peaks at ${peak.toFixed(1)} px`);
  // no straight run of more than about 20 px (4 steps) near the peak
  const near = Array.from(a, (v, i) => [v, i]).filter(([v]) => v >= 0.8 * peak).map(([, i]) => i);
  let run = 0;
  let longest = 0;
  for (const i of near) {
    run = i >= 1 && i < d.length + 1 && Math.abs(d[i - 1]) < 1e-3 * peak ? run + 1 : 0;
    longest = Math.max(longest, run);
  }
  assert.ok(longest * RIDGE.step <= 20, `a straight run of ${longest * RIDGE.step} px`);
});

ok('the_swell_drifts_slowly_and_freezes_in_still', () => {
  let full = 0;
  let calm = 0;
  let still = 0;
  // 10 s in 600 frames, and in 5 long ones: the same
  for (let i = 0; i < 600; i++) {
    full = driftSwell(full, 1 / 60, 'full');
    calm = driftSwell(calm, 1 / 60, 'calm');
    still = driftSwell(still, 1 / 60, 'still');
  }
  assert.ok(Math.abs(full - 1) < 1e-9, 'one wavelength in 10 s');
  assert.ok(Math.abs(calm - 0.5) < 1e-9, 'Calm drifts half as fast');
  assert.equal(still, 0, 'Still freezes it');
  assert.equal(driftSwell(0, 10, 'full'), 1, 'by time, not by frame');
  const silent = new Float32Array(BANDS);
  const at0 = heights(silent, 0);
  const at25 = heights(silent, 0.25);
  assert.ok(at0.some((v, i) => Math.abs(v - at25[i]) > 0.003 * H), 'the wave has moved sideways');
  assert.deepEqual([...heights(silent, 1)].map((v) => v.toFixed(4)), [...at0].map((v) => v.toFixed(4)), 'a wavelength on, the same');
});

ok('the_swell_gives_way_to_the_music', () => {
  const x = Math.round(0.4 * W / RIDGE.step);
  let before = -Infinity;
  const steps = [];
  for (let level = 0; level <= 1.0001; level += 0.01) {
    const a = heights(new Float32Array(BANDS).fill(level), 0.3);
    steps.push(a[x]);
    assert.ok(a[x] >= before - 1e-9, 'the ridge never falls as the music rises');
    if (steps.length > 1) assert.ok(a[x] - before < 0.2 * H * 0.011, 'no jump');
    before = a[x];
  }
  const loud = heights(new Float32Array(BANDS).fill(0.5), 0.3);
  const loud2 = heights(new Float32Array(BANDS).fill(0.5), 0.8);
  assert.ok(Math.abs(loud[x] - 0.5 * RIDGE.scale * H) < 1e-3 && loud[x] === loud2[x], 'above the threshold only the music shows');
});

ok('the_ridge_is_drawn_as_curves_with_no_run_of_straight_segments', () => {
  const calls = [];
  const g = new Proxy({}, {
    get: (_, name) => (typeof name === 'string' && /^(moveTo|lineTo|quadraticCurveTo|closePath|beginPath)$/.test(name)
      ? (...a) => calls.push([name, ...a]) : () => {}),
    set: () => true,
  });
  const units = new Float32Array(BANDS);
  units[30] = 1;
  drawRidge(g, { w: W, h: H, H: 0.56 * H, dpr: 1 }, units, { dull: false, swell: 0.2 });
  const kinds = calls.map((c) => c[0]);
  assert.ok(kinds.filter((k) => k === 'quadraticCurveTo').length >= W / RIDGE.step - 2, 'a curve segment per sample');
  assert.ok(kinds.filter((k) => k === 'lineTo').length <= 3, 'lineTo only at the two ends and the foot');
});

ok('a_very_wide_scene_is_drawn_whole', () => {
  const ys = [];
  const g = new Proxy({}, {
    get: (_, name) => (name === 'quadraticCurveTo' ? (a, b) => { ys.push(b); } : () => {}),
    set: () => true,
  });
  const wide = 20010;
  drawRidge(g, { w: wide, h: H, H: 0.56 * H, dpr: 1 }, new Float32Array(BANDS), { dull: false, swell: 0.1 });
  assert.equal(ys.length, wide / RIDGE.step, 'a segment for every sample');
  assert.ok(ys.every(Number.isFinite), 'no NaN past the old fixed size');
});

console.log(`${n} ridge checks passed`);
