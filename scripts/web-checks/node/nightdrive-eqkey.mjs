/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for Night drive's dash EQ key (themes/nightdrive/eq-key.js, draw/dash.js): what the key stands for on
// each side, the press that writes the other mode, a write that fails, and the stream's event
// coming before or after the write's answer, over the core's real store, holds and actions.
// Run: node nightdrive-eqkey.mjs
import assert from 'node:assert/strict';
import { eqKeyOf, pressEqKey } from '../../../bins/svolderd/web/themes/nightdrive/eq-key.js';
import { PARTS_OF } from '../../../bins/svolderd/web/themes/nightdrive/live.js';
import { drawDash } from '../../../bins/svolderd/web/themes/nightdrive/draw/dash.js';
import {
  DAC, GUEST, computerRig, modelOf,
} from './nightdrive-rig.mjs';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
const flush = () => new Promise((resolve) => setImmediate(resolve));

const modes = (mode, names = ['flat', 'eq', 'loudness']) => ({
  mode, modes: names.map((name) => ({ name, label: name, on: name === mode })),
});
const dac = (mode, names) => ({ ...DAC, tone: modes(mode, names) });
const act = (core) => ({ act: (name, value, fn) => fn(core.actions) });
const view = (r) => r.core.computer.view();
const press = (r) => pressEqKey({ core: r.core, act: act(r.core), inert: false });
const eqOn = (r, on) => r.store.emit('host', {
  ...r.store.page().host, computer: modelOf({ eq: { ...modelOf().eq, on } }),
});
const keyHtml = (r) => /<button[^>]*id="nd-eq-key"[^>]*>EQ<\/button>/.exec(
  drawDash(r.store.page(), {}, Date.now(), view(r)))?.[0] ?? null;

await ok('the_key_stands_for_the_eq_of_the_side_shown', () => {
  const on = computerRig({ side: 'computer' });
  assert.deepEqual(eqKeyOf(on.store.page(), view(on)), { side: 'computer', on: true, mode: 'off' });
  const off = computerRig({ side: 'computer', computer: modelOf({ eq: { ...modelOf().eq, on: false } }) });
  assert.deepEqual(eqKeyOf(off.store.page(), view(off)), { side: 'computer', on: false, mode: 'eq' });
  const none = computerRig({ side: 'computer', computer: modelOf({ eq: null }) });
  assert.equal(eqKeyOf(none.store.page(), view(none)), null, 'no EQ on the computer, no key');
  for (const [mode, want] of [['eq', ['device', true, 'flat']], ['flat', ['device', false, 'eq']], ['loudness', ['device', false, 'eq']]]) {
    const r = computerRig({ side: 'device', device: dac(mode) });
    const key = eqKeyOf(r.store.page(), view(r));
    assert.deepEqual([key.side, key.on, key.mode], want, mode);
  }
  const noFlat = computerRig({ side: 'device', device: dac('eq', ['eq', 'loudness']) });
  assert.equal(eqKeyOf(noFlat.store.page(), view(noFlat)), null, 'a tone with no Flat has no off to reach');
  assert.equal(eqKeyOf(computerRig({ side: 'device' }).store.page(), null), null, 'no tone, no key');
});

await ok('the_key_is_drawn_pressed_with_its_state_in_its_label', () => {
  const on = computerRig({ side: 'computer' });
  assert.match(keyHtml(on), /aria-pressed="true" aria-label="EQ on"/);
  const off = computerRig({ side: 'device', device: dac('flat') });
  assert.match(keyHtml(off), /aria-pressed="false" aria-label="EQ off"/);
  assert.equal(keyHtml(computerRig({ side: 'device' })), null, 'a device with no tone has no key');
});

await ok('the_dash_is_drawn_again_when_the_tone_changes', () => {
  assert.ok(PARTS_OF.dash.includes('tone'), 'the device key reads the tone mode');
});

await ok('a_press_on_the_computer_turns_the_eq_off_and_on_again', async () => {
  const r = computerRig({ side: 'computer' });
  await press(r);
  assert.deepEqual(r.posts, [['api/v1/host/eq', { on: false }]]);
  eqOn(r, false);
  assert.equal(view(r).eq.on, false);
  await press(r);
  assert.deepEqual(r.posts[1], ['api/v1/host/eq', { on: true }]);
});

await ok('the_stream_event_before_or_after_the_answer_leaves_the_key_right', async () => {
  // the write's answer first, then the stream's event
  const first = computerRig({ side: 'computer' });
  const sent = press(first);
  assert.equal(view(first).eq.on, false, 'drawn as asked at once');
  await sent;
  assert.equal(view(first).eq.on, false, 'and after the answer');
  eqOn(first, false);
  assert.equal(view(first).eq.on, false);
  // the stream's event first (the daemon already says off), then the answer
  let release;
  const second = computerRig({
    side: 'computer',
    reply: () => new Promise((resolve) => { release = () => resolve({ status: 200, ok: true }); }),
  });
  const waiting = press(second);
  await flush();
  eqOn(second, false);
  assert.equal(view(second).eq.on, false, 'the event with the answer still to come');
  release();
  await waiting;
  assert.equal(view(second).eq.on, false, 'the answer after the event');
});

await ok('a_write_that_fails_draws_the_eq_as_the_daemon_holds_it', async () => {
  const r = computerRig({ side: 'computer', reply: () => ({ status: 503, error: 'computer controls unavailable' }) });
  const answer = await press(r);
  assert.equal(answer.ok, false);
  assert.deepEqual(r.toasts, ['computer controls unavailable']);
  await flush();
  assert.equal(view(r).eq.on, true, 'the key goes back to what the daemon holds');
  assert.equal(r.pending.get('mode'), null);
});

await ok('a_press_on_the_device_asks_flat_or_eq', async () => {
  const eq = computerRig({ side: 'device', device: dac('eq') });
  await press(eq);
  assert.equal(eq.posts.length, 1);
  assert.match(eq.posts[0][0], /Gungnir%202/);
  assert.deepEqual(eq.posts[0][1], { mode: 'flat' });
  const flat = computerRig({ side: 'device', device: dac('flat') });
  await press(flat);
  assert.deepEqual(flat.posts[0][1], { mode: 'eq' });
  const loud = computerRig({ side: 'device', device: dac('loudness') });
  await press(loud);
  assert.deepEqual(loud.posts[0][1], { mode: 'eq' }, 'loudness reads as off: a press brings EQ');
});

await ok('a_device_that_fails_the_write_toasts_the_daemons_words', async () => {
  const r = computerRig({ side: 'device', device: dac('eq'), reply: () => ({ status: 502, error: 'device did not answer' }) });
  const answer = await press(r);
  assert.equal(answer.ok, false);
  assert.ok(r.toasts.includes('device did not answer'), JSON.stringify(r.toasts));
});

await ok('a_guest_is_turned_away_and_nothing_is_sent', async () => {
  for (const side of ['computer', 'device']) {
    const r = computerRig({ side, viewer: GUEST, device: dac('eq') });
    await pressEqKey({ core: r.core, act: act(r.core), inert: true });
    assert.deepEqual(r.posts, [], side);
  }
});

console.log(`${n} ok`);
