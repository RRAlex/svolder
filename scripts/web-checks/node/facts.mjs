/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the pure rules of core/words.js and facts.js: the ages as the daemon words them
// (the thresholds of `age_words` in crates/svolder-ipc/src/socket/signal.rs), the signal's words,
// the Identify toast and the read that a newer swap overtakes. Run: node facts.mjs
import assert from 'node:assert/strict';
import { ageWords, identifyWords, signalText } from '../../../bins/svolderd/web/core/words.js';
import { overtaken } from '../../../bins/svolderd/web/themes/ember/facts.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

ok('ageWords: seconds under 90 s, minutes from 90 s', () => {
  const cases = [[0, '0 s'], [1, '1 s'], [59, '59 s'], [89, '89 s'], [90, '1 min'], [119, '1 min'],
    [120, '2 min'], [3599, '59 min']];
  for (const [secs, want] of cases) assert.equal(ageWords(secs), want, `${secs} s`);
});

ok('ageWords: minutes under 90 min, hours from 90 min', () => {
  const cases = [[89 * 60, '89 min'], [90 * 60 - 1, '89 min'], [90 * 60, '1 h'],
    [2 * 3600, '2 h'], [36 * 3600 - 1, '35 h']];
  for (const [secs, want] of cases) assert.equal(ageWords(secs), want, `${secs} s`);
});

ok('ageWords: hours under 36 h, days from 36 h, one day singular', () => {
  const cases = [[36 * 3600, '1 day'], [47 * 3600, '1 day'], [48 * 3600, '2 days'],
    [10 * 86400, '10 days']];
  for (const [secs, want] of cases) assert.equal(ageWords(secs), want, `${secs} s`);
});

ok('ageWords: rounds down, and an age before the stamp reads as none', () => {
  assert.equal(ageWords(5.9), '5 s');
  assert.equal(ageWords(-3), '0 s');
});

ok('signalText: an advert always carries its age', () => {
  assert.equal(signalText('−62 dBm advert', false, 0), '−62 dBm advert, 0 s ago');
  assert.equal(signalText('−62 dBm advert', false, 180), '−62 dBm advert, 3 min ago');
});

ok('signalText: a live signal shows its age only from half a minute', () => {
  assert.equal(signalText('−58 dBm live', true, 0), '−58 dBm live');
  assert.equal(signalText('−58 dBm live', true, 29.9), '−58 dBm live');
  assert.equal(signalText('−58 dBm live', true, 30), '−58 dBm live, 30 s ago');
});

ok('identifyWords: sent, just sent with the daemon\'s gap, and the daemon\'s own words', () => {
  assert.equal(identifyWords({ status: 200, sent: true }, '2'), 'Identify sent');
  assert.equal(identifyWords({ status: 200, sent: false }, '2'), 'Just sent — wait 2 s');
  assert.equal(identifyWords({ status: 200, sent: false }, '5'), 'Just sent — wait 5 s');
  assert.equal(
    identifyWords({ status: 409, error: 'device not connected, next attempt in 12 s' }, '2'),
    'device not connected, next attempt in 12 s',
  );
  assert.equal(identifyWords({ status: 0 }, '2'), 'The device did not answer.');
});
ok('overtaken: a swap or another element in place since the read began drops the read', () => {
  const root = {};
  assert.equal(overtaken({ root, swaps: 2 }, { root, swaps: 2 }), false);
  assert.equal(overtaken({ root, swaps: 2 }, { root, swaps: 3 }), true);
  assert.equal(overtaken({ root, swaps: 2 }, { root: {}, swaps: 2 }), true);
  assert.equal(overtaken({ root, swaps: 2 }, { root: null, swaps: 2 }), true);
});
console.log(`facts.mjs: ${n} checks passed`);
