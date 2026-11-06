/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Night drive's readout and bisect switches (themes/nightdrive/debug.js,
// stats.js and debug-wire.js's lines): what an address asks for, the draws' rate and percentile, and
// the box's words. Run: node nightdrive-debug.mjs
import assert from 'node:assert/strict';
import { SWITCHES, debugOf } from '../../../bins/svolderd/web/themes/nightdrive/debug.js';
import { readoutLines } from '../../../bins/svolderd/web/themes/nightdrive/debug-wire.js';
import { createStats } from '../../../bins/svolderd/web/themes/nightdrive/stats.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

ok('an_address_without_the_parameter_asks_for_nothing', () => {
  assert.equal(debugOf(''), null);
  assert.equal(debugOf('?theme=nightdrive'), null);
  assert.equal(debugOf('?nd-debugger'), null);
});

ok('the_bare_parameter_shows_the_readout_with_every_layer_on', () => {
  for (const search of ['?nd-debug', '?theme=nightdrive&nd-debug', '?nd-debug=']) {
    const got = debugOf(search);
    assert.ok(got, search);
    assert.equal(got.off.size, 0);
  }
});

ok('the_switches_are_comma_separated_and_a_wrong_word_is_named_not_obeyed', () => {
  const got = debugOf('?theme=nightdrive&nd-debug=noglow,NoFloor, dpr1,fast');
  assert.deepEqual([...got.off].sort(), ['dpr1', 'nofloor', 'noglow']);
  assert.deepEqual(got.unknown, ['fast']);
  assert.deepEqual(Object.keys(SWITCHES).sort(), ['dpr1', 'noembers', 'nodash', 'nofloor', 'noglow', 'nohaze', 'nohud', 'nomood', 'noridge', 'nosky', 'nowet'].sort());
});

ok('the_rate_is_the_draws_of_the_last_second_not_the_one_asked_for', () => {
  const s = createStats();
  for (let i = 0; i < 90; i++) s.add(5, i * (1000 / 30));
  assert.ok(Math.abs(s.read(89 * (1000 / 30)).fps - 30) <= 1, 'a second of 30 a second');
  const idle = createStats();
  for (let i = 0; i < 5; i++) idle.add(3, i * 125);
  assert.equal(idle.read(500).fps, 5);
  assert.equal(idle.read(5000).fps, 0, 'a second with no draw is 0');
});

ok('the_mean_and_the_95th_percentile_are_of_the_last_60_draws', () => {
  const s = createStats();
  for (let i = 0; i < 100; i++) s.add(1, i * 16);
  for (let i = 0; i < 56; i++) s.add(2, 2000 + i * 16);
  for (let i = 0; i < 4; i++) s.add(40, 3000 + i * 16);
  const read = s.read(3048);
  assert.ok(Math.abs(read.mean - (56 * 2 + 4 * 40) / 60) < 1e-9, read.mean);
  assert.equal(read.p95, 40);
  const flat = createStats();
  for (let i = 0; i < 60; i++) flat.add(i + 1, i);
  assert.equal(flat.read(60).p95, 57);
});

ok('the_new_switches_are_named_and_obeyed', () => {
  const got = debugOf('?nd-debug=nomood,nowet,noembers,nohaze');
  assert.deepEqual([...got.off].sort(), ['noembers', 'nohaze', 'nomood', 'nowet']);
  assert.deepEqual(got.unknown, []);
  for (const name of ['nomood', 'nowet', 'noembers', 'nohaze']) assert.ok(SWITCHES[name].length > 10, name);
});

ok('the_box_says_each_number_and_n_a_where_the_browser_has_none', () => {
  const lines = readoutLines({
    perf: { fps: 58, pace: 0, drawMs: 12.34, p95: 20.06, scale: 1.5, capped: true, step: 1, wet: false },
    sky: { levels: { bass: 0.5, highs: 0.25, highsJump: 0.1, ear: { med: 0.42, long: 0.66, balance: 0.38, embers: 0.9 } } },
    view: { motion: 'full' },
    canvas: [618, 1372],
    dpr: '3.00 -> 1.50',
    longTasks: null,
    off: ['noglow'],
  }).join('\n');
  for (const want of ['58 draws/s', 'every frame', '12.3 ms mean', '20.1 p95', 'canvas 618x1372', 'dpr 3.00 -> 1.50',
    'scale 1.50', 'capped yes', 'step 1', 'wet dropped',
    'ear med 0.42 long 0.66 balance 0.38 embers 0.90', 'motion full', 'long tasks 10 s: n/a', 'bass 0.50 highs 0.25', 'off: noglow']) {
    assert.ok(lines.includes(want), `${want} in\n${lines}`);
  }
});

ok('the_box_without_an_ear_says_so', () => {
  const lines = readoutLines({
    perf: { fps: 30, pace: 0, drawMs: 1, p95: 2, scale: 1, capped: false, step: 0, wet: true },
    sky: null,
    view: { motion: 'calm' },
    canvas: [10, 10],
    dpr: '1.00 -> 1.00',
    longTasks: 0,
    off: [],
  }).join('\n');
  assert.ok(lines.includes('no ear') && lines.includes('wet ok') && lines.includes('no analysis'), lines);
});

console.log(`${n} readout checks passed`);
