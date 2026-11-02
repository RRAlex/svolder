// SPDX-FileCopyrightText: 2026 the svolder authors
// SPDX-License-Identifier: GPL-3.0-or-later
// Node harness for the fold of Paper's `The device` (themes/paper/fold.js): folded to begin with,
// the choice stored, and held in memory when storage refuses or is absent. Run: node paper-fold.mjs
import assert from 'node:assert/strict';
import { createFold } from '../../../bins/svolderd/web/themes/paper/fold.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const memory = () => {
  const map = new Map();
  return { get: (k, d) => (map.has(k) ? map.get(k) : d), set: (k, v) => { map.set(k, v); return true; }, map };
};

ok('the_fold_starts_folded_and_stores_each_choice', () => {
  const prefs = memory();
  const fold = createFold(() => prefs);
  assert.equal(fold.open, false);
  fold.toggle();
  assert.equal(fold.open, true);
  assert.equal(prefs.map.get('deviceOpen'), true);
  fold.toggle();
  assert.equal(prefs.map.get('deviceOpen'), false);
});

ok('a_stored_choice_is_the_starting_point', () => {
  const prefs = memory();
  prefs.set('deviceOpen', true);
  assert.equal(createFold(() => prefs).open, true);
});

ok('a_store_that_refuses_still_unfolds_and_stays_unfolded', () => {
  const refusing = { get: (_k, d) => d, set: () => false };
  const throwing = { get: () => { throw new Error('no'); }, set: () => { throw new Error('no'); } };
  for (const prefs of [refusing, throwing, null]) {
    const fold = createFold(() => prefs);
    assert.equal(fold.open, false);
    fold.toggle();
    assert.equal(fold.open, true, 'open after the tap');
    assert.equal(fold.open, true, 'and again at the next redraw');
    fold.toggle();
    assert.equal(fold.open, false);
  }
});

console.log(`paper-fold: ${n} passed`);
