/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for keep.js's swap hold of the tone part while a point on the curve is dragged: the
// part waits whatever the browser (with or without Element.moveBefore), its swap is read again
// once the hold goes, and a drag the device did not take puts the part among the swaps that
// wait, and two drags that overlap (a new one begun while the one before waits for its answers)
// each hold once and let go once; the part that waited is drawn once from the newest copy the
// model holds, not from the swap that was held. Run: node keep.mjs
import assert from 'node:assert/strict';
import {
  clear, drawNewest, holdTone, holds, modelName, moves, release,
} from '../../../bins/svolderd/web/themes/ember/keep.js';
import { createStore } from '../../../bins/svolderd/web/core/store.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

// No part is in use (no slider, no stepper): only the tone hold keeps a swap waiting.
const find = () => null;
const reloaded = () => {
  const ids = [];
  release((id) => ids.push(id), find);
  return ids;
};
const reset = () => {
  holdTone(false);
  clear();
  delete globalThis.Element;
};

ok('the_tone_part_waits_while_held_in_a_browser_that_cannot_move_a_kept_element', () => {
  reset();
  assert.equal(moves(), false);
  assert.equal(holds('part-tone', find), false, 'no hold, no wait');
  holdTone(true);
  assert.equal(holds('part-tone', find), true);
  assert.equal(holds('part-card', find), false, 'only the tone part');
  assert.equal(holds('part-bands', find), false);
  reset();
});

ok('the_tone_part_waits_while_held_even_where_a_kept_element_moves', () => {
  reset();
  globalThis.Element = { prototype: { moveBefore() {} } };
  assert.equal(moves(), true);
  assert.equal(holds('part-tone', find), false, 'moves(): no wait without the hold');
  holdTone(true);
  assert.equal(holds('part-tone', find), true, 'the hold wins over moves()');
  assert.equal(holds('part-controls', find), false);
  reset();
});

ok('the_release_keeps_the_tone_part_waiting_while_held_and_reads_it_once_after', () => {
  reset();
  holdTone(true);
  assert.equal(holds('part-tone', find), true);
  assert.deepEqual(reloaded(), [], 'held: nothing read');
  assert.deepEqual(reloaded(), [], 'held, again: still nothing');
  holdTone(false);
  assert.deepEqual(reloaded(), ['part-tone'], 'let go: the waiting swap is read');
  assert.deepEqual(reloaded(), [], 'once');
  reset();
});

ok('letting_go_with_again_reads_the_part_even_with_nothing_waiting', () => {
  reset();
  holdTone(true);
  holdTone(false, true);
  assert.deepEqual(reloaded(), ['part-tone'], 'a drag the device did not take is read back');
  assert.deepEqual(reloaded(), []);
  holdTone(true);
  holdTone(false);
  assert.deepEqual(reloaded(), [], 'without again and no swap that waited: nothing to read');
  reset();
});

ok('a_whole_screen_clears_the_waiting_swaps', () => {
  reset();
  holdTone(true);
  holds('part-tone', find);
  clear();
  holdTone(false);
  assert.deepEqual(reloaded(), []);
  reset();
});

ok('two_overlapping_holds_keep_the_tone_part_waiting_until_both_let_go', () => {
  reset();
  holdTone(true);                          // a drag, then its release: it waits for its answers
  assert.equal(holds('part-tone', find), true);
  holdTone(true);                          // a second drag begins meanwhile
  holdTone(false);                         // the first one's last answer
  assert.equal(holds('part-tone', find), true, 'the second still holds');
  assert.deepEqual(reloaded(), [], 'held by the second: nothing read');
  holdTone(false);                         // the second one's
  assert.deepEqual(reloaded(), ['part-tone'], 'both let go: the waiting swap is read');
  assert.equal(holds('part-tone', find), false);
  // a refused first drag still reads the part back, once the second has gone too
  holdTone(true);
  holdTone(true);
  holdTone(false, true);
  assert.deepEqual(reloaded(), [], 'the second holds');
  holdTone(false);
  assert.deepEqual(reloaded(), ['part-tone'], 'read back from the server');
  // a let go with nothing held does not go below none
  holdTone(false);
  holdTone(true);
  assert.equal(holds('part-tone', find), true);
  reset();
});

ok('the_held_part_is_drawn_from_the_newest_model_when_the_hold_ends', () => {
  reset();
  const store = createStore({ shown: 'A', device: { tone: { n: 0 } }, followed: {} },
    { fetch: () => { throw new Error('no read'); } });
  holdTone(true);
  for (const n of [1, 2]) {
    store.applyPart('A', 'tone', { n });
    assert.equal(holds('part-tone', find), true, `swap ${n} waits`);
  }
  const drawn = [];
  const draw = drawNewest(store, (id, data) => drawn.push([id, data]));
  release(draw, find);
  assert.deepEqual(drawn, [], 'held: nothing drawn');
  holdTone(false);
  release(draw, find);
  assert.deepEqual(drawn, [['part-tone', { n: 2 }]], 'once, from the second event');
  release(draw, find);
  assert.equal(drawn.length, 1, 'and not again');
  reset();
});

ok('a_use_of_the_pages_own_holds_a_swap_the_same_way_and_the_part_names_map_to_the_model', () => {
  reset();
  assert.equal(modelName('part-device-presets'), 'device_presets');
  assert.equal(modelName('part-tone'), 'tone');
  let open = true;
  const also = (id) => open && id === 'part-presets';
  assert.equal(holds('part-presets', find, also), true);
  assert.equal(holds('part-tone', find, also), false);
  const ids = [];
  release((id) => ids.push(id), find, also);
  assert.deepEqual(ids, [], 'the form is open');
  open = false;
  release((id) => ids.push(id), find, also);
  assert.deepEqual(ids, ['part-presets'], 'closed: it is drawn');
  reset();
});

console.log(`${n} keep checks passed`);
