/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Ember's order of sections (themes/ember/order.js): the default
// order, a kept order that drops what the layout lacks and gives a new section its default zone,
// the arrow keys' moves between and within zones, a section placed by index, and one order per
// layout. Run: node ember-order.mjs
import assert from 'node:assert/strict';
import {
  DEFAULT_ORDER, MOVABLE, ZONES, isDefault, keptOrder, moveBy, moveTo, orderFor, resolveOrder,
} from '../../../bins/svolderd/web/themes/ember/order.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

ok('the_default_order_is_todays_layout', () => {
  assert.deepEqual(ZONES, ['band', 'left', 'right']);
  assert.deepEqual(MOVABLE, ['playing', 'visualizer', 'volume', 'controls', 'power', 'tone',
    'presets', 'device-presets', 'meters']);
  assert.deepEqual(resolveOrder(null, 'dac'), {
    band: ['playing', 'visualizer'],
    left: ['volume', 'controls', 'power'],
    right: ['tone', 'presets', 'device-presets'],
  });
  assert.deepEqual(resolveOrder(null, 'amp'), DEFAULT_ORDER.amp);
  assert.equal(isDefault(resolveOrder(null, 'dac'), 'dac'), true);
  assert.equal(isDefault(moveBy(resolveOrder(null, 'dac'), 'volume', 'ArrowDown'), 'dac'), false);
  // a section the screen does not draw may stand anywhere without the order looking arranged
  const apart = { band: ['playing', 'visualizer'], left: ['volume', 'controls'],
    right: ['tone', 'presets', 'device-presets', 'power'] };
  assert.equal(isDefault(apart, 'dac'), false);
  assert.equal(isDefault(apart, 'dac', (id) => id !== 'power'), true);
  for (const id of MOVABLE) {
    assert.equal(Object.values(DEFAULT_ORDER).some((layout) => Object.values(layout)
      .some((ids) => ids.includes(id))), true, `${id} is in a default order`);
  }
});

ok('a_kept_order_drops_unknown_ids_and_appends_new_ones_to_their_default_zone', () => {
  const kept = { band: ['tone', 'nothing'], left: ['power'], right: ['volume'] };
  assert.deepEqual(resolveOrder(kept, 'dac'), {
    band: ['tone', 'playing', 'visualizer'],
    left: ['power', 'controls'],
    right: ['volume', 'presets', 'device-presets'],
  });
  // the same id twice stays where it came first; zones that are not lists are no zones
  assert.deepEqual(resolveOrder({ band: ['tone'], left: ['tone'], right: 'x' }, 'dac').left,
    ['volume', 'controls', 'power']);
  // a section of the layout that the kept order lacks goes at the end of its default zone
  assert.deepEqual(resolveOrder({ band: [], left: [], right: [] }, 'amp'), DEFAULT_ORDER.amp);
  assert.deepEqual(resolveOrder({ left: ['controls', 'power'] }, 'amp'), {
    band: ['playing', 'visualizer'], left: ['controls', 'power', 'meters'], right: [],
  });
});

ok('arrows_move_within_and_between_zones', () => {
  const start = resolveOrder(null, 'dac');
  const right = moveBy(start, 'volume', 'ArrowRight');
  assert.equal(right.right[0], 'volume', 'Volume, Right, goes to the top of the right column');
  assert.deepEqual(right.left, ['controls', 'power']);
  assert.deepEqual(right.right, ['volume', 'tone', 'presets', 'device-presets']);
  assert.deepEqual(moveBy(start, 'volume', 'ArrowUp'), start, 'Up at index 0 stays');
  assert.deepEqual(moveBy(start, 'playing', 'ArrowLeft'), start, 'Left from the band stays');
  assert.deepEqual(moveBy(start, 'tone', 'ArrowRight'), start, 'Right from the right stays');
  assert.deepEqual(moveBy(start, 'volume', 'ArrowDown').left, ['controls', 'volume', 'power']);
  assert.deepEqual(moveBy(start, 'power', 'ArrowDown'), start, 'Down at the end stays');
  assert.deepEqual(moveBy(start, 'power', 'ArrowUp').left, ['volume', 'power', 'controls']);
  // between zones at the same index, clamped to the end of the other zone
  assert.deepEqual(moveBy(start, 'power', 'ArrowLeft').band, ['playing', 'visualizer', 'power']);
  assert.deepEqual(moveBy(start, 'controls', 'ArrowLeft').band, ['playing', 'controls', 'visualizer']);
  assert.deepEqual(moveBy(start, 'device-presets', 'ArrowLeft').left,
    ['volume', 'controls', 'device-presets', 'power']);
  assert.deepEqual(moveBy(start, 'visualizer', 'ArrowRight').left,
    ['volume', 'visualizer', 'controls', 'power']);
  // a key that is not an arrow, and an id the order lacks, change nothing
  assert.deepEqual(moveBy(start, 'volume', 'Enter'), start);
  assert.deepEqual(moveBy(start, 'meters', 'ArrowDown'), start);
  // sections the screen does not draw are stepped over
  const seen = (id) => id !== 'controls';
  assert.deepEqual(moveBy(start, 'volume', 'ArrowDown', seen).left, ['controls', 'power', 'volume']);
  assert.deepEqual(moveBy(start, 'power', 'ArrowUp', seen).left, ['power', 'volume', 'controls']);
  // the order given is not changed
  assert.deepEqual(start, resolveOrder(null, 'dac'));
});

ok('move_to_inserts_at_the_index', () => {
  const start = resolveOrder(null, 'dac');
  assert.deepEqual(moveTo(start, 'tone', 'band', 1).band, ['playing', 'tone', 'visualizer']);
  assert.deepEqual(moveTo(start, 'tone', 'band', 0).band, ['tone', 'playing', 'visualizer']);
  assert.deepEqual(moveTo(start, 'tone', 'band', 9).band, ['playing', 'visualizer', 'tone']);
  assert.deepEqual(moveTo(start, 'tone', 'band', -3).band, ['tone', 'playing', 'visualizer']);
  assert.deepEqual(moveTo(start, 'tone', 'band', 1).right, ['presets', 'device-presets']);
  // within its own zone the index counts the others
  assert.deepEqual(moveTo(start, 'volume', 'left', 1).left, ['controls', 'volume', 'power']);
  assert.deepEqual(moveTo(start, 'volume', 'left', 2).left, ['controls', 'power', 'volume']);
  // into an emptied zone
  const bare = moveTo(moveTo(moveTo(start, 'tone', 'left', 0), 'presets', 'left', 0),
    'device-presets', 'left', 0);
  assert.deepEqual(bare.right, []);
  assert.deepEqual(moveTo(bare, 'volume', 'right', 0).right, ['volume']);
  // an index counts the sections that are drawn; the others keep their places around them
  const seen = (id) => id !== 'controls';
  assert.deepEqual(moveTo(start, 'tone', 'left', 1, seen).left,
    ['volume', 'controls', 'tone', 'power']);
  assert.deepEqual(moveTo(start, 'tone', 'left', 2, seen).left,
    ['volume', 'controls', 'power', 'tone']);
  assert.equal(moveTo(start, 'tone', 'nowhere', 0), start);
  assert.equal(moveTo(start, 'meters', 'band', 0), start);
});

ok('an_order_kept_for_a_dac_is_not_an_amps', () => {
  const stored = { dac: { band: ['tone'], left: [], right: [] } };
  assert.equal(keptOrder(stored, 'amp'), null);
  assert.deepEqual(resolveOrder(keptOrder(stored, 'amp'), 'amp'), DEFAULT_ORDER.amp);
  assert.deepEqual(keptOrder(stored, 'dac'), stored.dac);
  for (const bad of [null, undefined, 'x', 3, [], { dac: 'x' }, { dac: [] }]) {
    assert.equal(keptOrder(bad, 'dac'), null, JSON.stringify(bad));
  }
  const prefs = { get: (name, fallback) => (name === 'arrange' ? stored : fallback) };
  assert.equal(orderFor(prefs, 'dac').band[0], 'tone');
  assert.deepEqual(orderFor(prefs, 'amp'), DEFAULT_ORDER.amp);
  assert.deepEqual(orderFor(null, 'dac'), DEFAULT_ORDER.dac);
});

ok('any_run_of_moves_keeps_every_section_exactly_once', () => {
  // a seeded walk of arrow moves and placements, with some sections not drawn
  let seed = 7;
  const next = (n) => {
    seed = (seed * 1103515245 + 12345) % 2147483648;
    return seed % n;
  };
  const keys = ['ArrowUp', 'ArrowDown', 'ArrowLeft', 'ArrowRight'];
  for (const layout of ['dac', 'amp']) {
    const ids = Object.values(DEFAULT_ORDER[layout]).flat();
    const hidden = new Set(ids.filter(() => next(3) === 0));
    const drawn = (id) => !hidden.has(id);
    let order = resolveOrder(null, layout);
    for (let i = 0; i < 400; i += 1) {
      const id = ids[next(ids.length)];
      order = next(2) === 0
        ? moveBy(order, id, keys[next(4)], drawn)
        : moveTo(order, id, ZONES[next(3)], next(5) - 1, drawn);
      const flat = ZONES.flatMap((zone) => order[zone]);
      assert.deepEqual([...flat].sort(), [...ids].sort(), `${layout} after ${i} moves`);
      assert.deepEqual(resolveOrder(order, layout), order, 'a kept order resolves to itself');
    }
  }
});

console.log(`ember-order: ${n} checks passed`);
