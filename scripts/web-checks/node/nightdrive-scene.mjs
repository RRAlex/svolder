/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Night drive's scene maths (themes/nightdrive/): the camera that puts
// the floor's rows on the screen, the one frequency axis, the music's levels (the ridge's unit,
// the 24 columns, the bass and the highs, the row rate) and the floor's rows.
// Run: node nightdrive-scene.mjs
import assert from 'node:assert/strict';
import { CAMERA, horizonOf, project } from '../../../bins/svolderd/web/themes/nightdrive/camera.js';
import { BAND_COLUMN, COLUMNS, axisHz, axisX } from '../../../bins/svolderd/web/themes/nightdrive/axis.js';
import {
  bandUnits, bassOf, columnLevels, highsOf, ridgeUnit, rowRate, silent,
} from '../../../bins/svolderd/web/themes/nightdrive/music.js';
import { createFloor } from '../../../bins/svolderd/web/themes/nightdrive/floor.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const near = (a, b, eps = 1e-9) => assert.ok(Math.abs(a - b) <= eps, `${a} is not ${b}`);

const W = 1000;
const H = 800;
const HOR = 0.56 * H;

ok('the_nearest_row_is_at_the_bottom_and_the_far_row_near_the_horizon', () => {
  const [, near1] = project(W, H, HOR, 0, 1);
  near(near1, H);
  const [, far] = project(W, H, HOR, 0, 0);
  assert.ok(far > HOR && far - HOR < (H - HOR) / 8, far);
  near(far, HOR + (H - HOR) / 9);
  const [mid] = project(W, H, HOR, 0, 0.5);
  near(mid, W / 2);
  assert.deepEqual(CAMERA, { K: 8, spread: 0.24, rows: 22, half: 6, side: 30, sideStep: 0.5 });
  assert.equal(horizonOf(), 0.56);
});

ok('the_far_rows_are_wider_than_f6s', () => {
  const [x] = project(W, H, HOR, 6, 0);
  near(x - W / 2, 0.16 * W);
  // F6: K = 12, spread 0.15: 6 * 0.15 / 13 of the width
  const f6 = (6 * 0.15) / 13;
  assert.ok(0.16 > f6 * 2.2, f6);
});

ok('the_axis_puts_20_hz_left_and_20_khz_right_and_round_trips', () => {
  near(axisX(20, W), 0);
  near(axisX(20000, W), W);
  near(axisX(632.4555320336759, W), W / 2, 1e-6);
  for (const hz of [20, 55, 440, 1000, 4700, 20000]) near(axisHz(axisX(hz, W), W), hz, hz * 1e-9);
});

ok('every_band_lands_in_one_column_and_every_column_has_a_band', () => {
  assert.equal(COLUMNS, 24);
  assert.equal(BAND_COLUMN.length, 240);
  const seen = new Array(COLUMNS).fill(0);
  for (const c of BAND_COLUMN) {
    assert.ok(c >= 0 && c < COLUMNS, c);
    seen[c]++;
  }
  assert.ok(seen.every((count) => count > 0), seen.join());
  // the columns are in order and the top band, whose centre is past 20 kHz, is in the last
  for (let i = 1; i < 240; i++) assert.ok(BAND_COLUMN[i] >= BAND_COLUMN[i - 1]);
  assert.equal(BAND_COLUMN[0], 0);
  assert.equal(BAND_COLUMN[239], 23);
});

ok('a_lone_band_at_1_khz_lights_its_column_only', () => {
  const left = new Float32Array(240);
  const right = new Float32Array(240);
  // the band whose centre is nearest 1 kHz: 20 * 2^((i + .5) / 24)
  const i = Math.round(Math.log2(1000 / 20) * 24 - 0.5);
  right[i] = 1;
  const cols = columnLevels(left, right, new Float32Array(COLUMNS));
  const lit = [...cols].map((v, k) => (v > 0 ? k : -1)).filter((k) => k >= 0);
  assert.deepEqual(lit, [BAND_COLUMN[i]]);
  // the louder channel counts, and the column is the mean of its bands' units
  assert.ok(cols[BAND_COLUMN[i]] > 0 && cols[BAND_COLUMN[i]] < 1);
  const units = bandUnits(left, right, new Float32Array(240));
  near(units[i], 1);
  assert.equal(units.filter((u) => u > 0).length, 1);
});

ok('the_ridge_reads_minus_60_to_0_db', () => {
  near(ridgeUnit(12 / 72), 0);
  near(ridgeUnit(42 / 72), 0.5);
  near(ridgeUnit(1), 1);
  near(ridgeUnit(0), 0);
  near(ridgeUnit(2), 1);
  near(ridgeUnit(-1), 0);
});

ok('the_bass_and_the_highs_are_the_means_of_their_columns', () => {
  const cols = new Float32Array(COLUMNS);
  for (let k = 0; k < 6; k++) cols[k] = 0.6;
  for (let k = 19; k < 24; k++) cols[k] = 0.3;
  near(bassOf(cols), 0.6, 1e-6);
  near(highsOf(cols), 0.3, 1e-6);
  assert.equal(silent(new Float32Array(COLUMNS).fill(0.01)), true);
  assert.equal(silent(new Float32Array(COLUMNS).fill(0.03)), false);
  const one = new Float32Array(COLUMNS);
  one[7] = 0.5;
  assert.equal(silent(one), false);
});

ok('rows_come_twice_a_beat_or_steadily', () => {
  assert.equal(rowRate(120), 4);
  assert.equal(rowRate(null), 3.5);
  assert.equal(rowRate(0), 3.5);
  assert.equal(rowRate(300), 6);
  assert.equal(rowRate(30), 2);
});

ok('the_floor_keeps_22_rows_newest_at_the_horizon', () => {
  const floor = createFloor({});
  assert.equal(floor.row(0), null);
  for (let r = 1; r <= 40; r++) {
    floor.advance(0.25, 4, new Float32Array(COLUMNS).fill(r / 100), true);
  }
  assert.equal(floor.whole, 40);
  near(floor.row(0)[0], 0.40, 1e-6);
  near(floor.row(1)[0], 0.39, 1e-6);
  near(floor.row(21)[0], 0.19, 1e-6);
  assert.equal(floor.row(22), null);
  assert.equal(floor.row(-1), null);
  // a row is a copy: the columns handed in later do not change it
  const cols = new Float32Array(COLUMNS).fill(0.5);
  floor.advance(0.25, 4, cols, true);
  cols.fill(0.9);
  near(floor.row(0)[3], 0.5, 1e-6);
  near(floor.frac, 0, 1e-9);
  floor.advance(0.125, 4, cols, true);
  near(floor.frac, 0.5, 1e-9);
  floor.clear();
  assert.equal(floor.row(0), null);
  assert.equal(floor.whole, 0);
});

ok('a_still_floor_does_not_roll', () => {
  const floor = createFloor({});
  floor.advance(0.25, 4, new Float32Array(COLUMNS).fill(0.4), true);
  const { whole, frac } = floor;
  floor.advance(5, 4, new Float32Array(COLUMNS).fill(0.9), false);
  assert.equal(floor.whole, whole);
  assert.equal(floor.frac, frac);
  near(floor.row(0)[0], 0.4, 1e-6);
  // a long gap while rolling is the loop's held step: it births as many rows as it covers
  floor.advance(0.1, 6, new Float32Array(COLUMNS).fill(0.2), true);
  assert.equal(floor.whole, whole);
  assert.ok(floor.frac > 0.5);
});

console.log(`nightdrive-scene.mjs: ${n} passed`);
