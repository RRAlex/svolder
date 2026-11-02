/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for themes/ember/level-write.js, a slider's writes: one in flight, the newest
// waiting, each for the device that was shown when the value was asked for. A value that waits
// behind the write in flight must never go to a device shown after it, nor from a slider that has
// left the page. Run: node level-write.mjs
import assert from 'node:assert/strict';
import { createLevelWriter } from '../../../bins/svolderd/web/themes/ember/level-write.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

// Actions whose writes answer when the test says.
function rig() {
  const calls = [];
  const gates = [];
  const state = { shown: 'A', connected: true, landed: [], refused: [] };
  const actions = {
    setControl(key, body, options) {
      calls.push([key, body, options]);
      return new Promise((resolve) => gates.push(resolve));
    },
  };
  const ask = createLevelWriter({
    key: 'balance',
    actions,
    shown: () => state.shown,
    connected: () => state.connected,
    landed: (answer) => state.landed.push(answer.raw),
    refused: (why) => state.refused.push(why),
  });
  const answer = async (value = { ok: true, status: 200 }) => {
    gates.shift()(value);
    await new Promise((resolve) => setImmediate(resolve));
  };
  return { calls, state, ask, answer };
}

await ok('a_write_names_the_device_that_was_shown_when_it_was_asked', async () => {
  const { calls, ask, answer } = rig();
  ask(10);
  assert.deepEqual(calls, [['balance', { raw: 10 }, { device: 'A' }]]);
  await answer();
});

await ok('a_value_waiting_for_the_write_in_flight_is_dropped_when_another_device_is_shown', async () => {
  const { calls, state, ask, answer } = rig();
  ask(10);
  ask(20);
  ask(30);
  state.shown = 'B';
  await answer();
  assert.equal(calls.length, 1, 'nothing more was sent, to A or to B');
  assert.deepEqual(calls[0][2], { device: 'A' });
});

await ok('an_answer_is_not_shown_by_a_slider_that_left_the_page_while_its_write_was_in_flight', async () => {
  const { state, ask, answer } = rig();
  ask(10);
  state.connected = false;
  await answer({ ok: true, status: 200, raw: 10 });
  assert.deepEqual(state.landed, [], 'a slider that is gone is not told');
});

await ok('a_value_waiting_is_dropped_when_its_slider_left_the_page', async () => {
  const { calls, state, ask, answer } = rig();
  ask(10);
  ask(20);
  state.connected = false;
  await answer();
  assert.equal(calls.length, 1);
});

await ok('a_value_waiting_goes_out_after_the_write_in_flight_when_nothing_changed', async () => {
  const { calls, ask, answer } = rig();
  ask(10);
  ask(20);
  ask(30);
  await answer();
  assert.deepEqual(calls.map((c) => c[1].raw), [10, 30], 'the newest waits, the middle is replaced');
  await answer();
});

await ok('an_answer_for_a_device_no_longer_shown_does_not_move_the_slider', async () => {
  const { state, ask, answer } = rig();
  ask(10);
  state.shown = 'B';
  await answer({ ok: true, status: 200, raw: 12, text: '12' });
  assert.deepEqual(state.landed, []);
});

await ok('a_refusal_is_heard_with_its_words_and_drops_what_waited', async () => {
  const { calls, state, ask, answer } = rig();
  ask(10);
  ask(20);
  await answer({ ok: false, status: 409, error: 'No.' });
  assert.deepEqual(state.refused, ['No.']);
  assert.equal(calls.length, 1);
});

console.log(`level-write.mjs: ${n} checks passed`);
