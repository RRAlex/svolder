/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/freeze.js, the section keeper the themes share: a section under the hand
// (a value typed, a list open, a question asked, a stepper held) is kept from a redraw and drawn
// again, from the newest model, when the last hold on it goes. `keeper` counts the holds of one
// section (a gesture's and an open list's apart), `createRelease` gives one back after the press
// that closed the list, `createLive({ sections })` draws the sections from a store's events and
// keeps the ones that are held. Fakes stand in for the store, the document and the timers' task.
// Run: node core-freeze.mjs
import assert from 'node:assert/strict';
import { createLive, createRelease, keeper } from '../../../bins/svolderd/web/core/freeze.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
const task = () => new Promise((resolve) => setTimeout(resolve, 0));

/** A store of one device that says its events to whoever listens. */
function fakeStore() {
  const listeners = {};
  let page = { n: 0 };
  let shown = 'dac';
  return {
    shown: () => shown,
    show(id) { shown = id; },
    state: () => 'open',
    page: () => page,
    set(next) { page = next; },
    on(event, fn) {
      (listeners[event] ??= new Set()).add(fn);
      return () => listeners[event].delete(fn);
    },
    say(event, data) { for (const fn of [...(listeners[event] ?? [])]) fn(data); },
    listening: () => Object.values(listeners).reduce((sum, set) => sum + set.size, 0),
  };
}

/** A live over `store` with two sections that log what they are drawn from. It follows the device
 * `device()` names: the store's shown one unless a case gives another. */
function rig({ device } = {}) {
  const store = fakeStore();
  const drawn = [];
  const abort = new AbortController();
  const view = {};
  /** The computer's side: it says a change to whoever listens, as the store does. */
  const computer = {
    heard: new Set(),
    on(fn) {
      computer.heard.add(fn);
      return () => computer.heard.delete(fn);
    },
    say() { for (const fn of [...computer.heard]) fn(); },
  };
  const section = (name, parts) => ({ parts, draw: (page) => drawn.push([name, page.n]) });
  const live = createLive({
    core: { model: store, computer },
    signal: abort.signal,
    view,
    device: device ?? (() => store.shown()),
    sections: { a: section('a', ['volume']), b: section('b', ['controls']) },
  });
  return { store, live, drawn, abort, computer, view };
}

/** A live that logs the holds a keeper puts on it. */
function logLive() {
  const calls = [];
  return { calls, freeze: (name, on, gesture) => calls.push([name, on, Boolean(gesture)]) };
}

await ok('a_section_under_the_hand_is_kept_from_a_redraw_and_drawn_on_the_release', () => {
  const { store, live, drawn } = rig();
  const keep = keeper(live, 'a');
  store.set({ n: 1 });
  store.say('part', { device: 'dac', name: 'volume' });
  assert.deepEqual(drawn, [['a', 1]], 'drawn while nobody holds it');
  keep.take();
  assert.equal(live.frozen('a'), true);
  store.set({ n: 2 });
  store.say('part', { device: 'dac', name: 'volume' });
  store.set({ n: 3 });
  store.say('part', { device: 'dac', name: 'volume' });
  assert.deepEqual(drawn, [['a', 1]], 'nothing drawn under the hand');
  keep.drop();
  assert.deepEqual(drawn, [['a', 1], ['a', 3]], 'drawn once from the newest model at the release');
  assert.equal(live.frozen('a'), false);
});

await ok('a_hold_on_a_section_that_was_not_redrawn_draws_nothing_on_the_release', () => {
  const { live, drawn } = rig();
  const keep = keeper(live, 'a');
  keep.take();
  keep.drop();
  assert.deepEqual(drawn, []);
});

await ok('holds_are_counted_and_the_last_one_releases', () => {
  const { store, live, drawn } = rig();
  const keep = keeper(live, 'a');
  keep.take();
  keep.take();
  store.say('part', { device: 'dac', name: 'volume' });
  keep.drop();
  assert.deepEqual(drawn, [], 'one hold left');
  keep.drop();
  assert.equal(drawn.length, 1);
  keep.drop();
  assert.equal(drawn.length, 1, 'a drop with no hold is no hold below zero and no draw');
  keep.take();
  assert.equal(live.frozen('a'), true, 'the count did not go below zero');
});

await ok('two_sections_are_kept_independently', () => {
  const { store, live, drawn } = rig();
  const a = keeper(live, 'a');
  const b = keeper(live, 'b');
  a.take();
  store.say('part', { device: 'dac', name: 'volume' });
  store.say('part', { device: 'dac', name: 'controls' });
  assert.deepEqual(drawn, [['b', 0]], 'b is drawn, a is kept');
  b.take();
  a.drop();
  assert.deepEqual(drawn, [['b', 0], ['a', 0]]);
  assert.equal(live.frozen('b'), true, 'b is still held');
});

await ok('around_takes_a_hold_and_gives_it_back_when_the_function_throws', () => {
  const { live } = rig();
  const keep = keeper(live, 'a');
  assert.equal(keep.around(() => 7), 7);
  assert.equal(live.frozen('a'), true, 'a list open holds the section');
  keep.release();
  assert.equal(live.frozen('a'), false);
  assert.throws(() => keep.around(() => { throw new Error('no'); }), /no/);
  assert.equal(live.frozen('a'), false, 'a throw gives it back');
});

await ok('a_gesture_and_a_list_are_counted_apart_and_the_live_is_told_which', () => {
  const log = logLive();
  const keep = keeper(log, 'a');
  keep.around(() => 0);
  keep.take();
  keep.release();
  keep.drop();
  assert.deepEqual(log.calls, [
    ['a', true, false], ['a', true, true], ['a', true, true], ['a', false, false],
  ]);
});

await ok('dropLater_gives_the_hold_back_on_the_next_task', async () => {
  const { live } = rig();
  const keep = keeper(live, 'a');
  keep.take();
  keep.dropLater();
  assert.equal(live.frozen('a'), true, 'not yet');
  await task();
  assert.equal(live.frozen('a'), false);
});

await ok('dropLater_gives_back_the_hold_of_an_open_list', async () => {
  const { store, live, drawn } = rig();
  const keep = keeper(live, 'a');
  keep.around(() => 0);
  store.say('part', { device: 'dac', name: 'volume' });
  keep.dropLater();
  assert.equal(live.frozen('a'), true, 'not yet');
  await task();
  assert.equal(live.frozen('a'), false, 'the list is closed: its hold is given back');
  assert.deepEqual(drawn, [['a', 0]], 'and what came meanwhile is drawn');
});

await ok('dropLater_gives_back_a_list_hold_before_a_gesture_hold', async () => {
  const log = logLive();
  const keep = keeper(log, 'a');
  keep.around(() => 0);
  keep.take();
  keep.dropLater();
  await task();
  assert.deepEqual(log.calls.at(-1), ['a', true, true], 'the gesture is still in hand');
  keep.dropLater();
  await task();
  assert.deepEqual(log.calls.at(-1), ['a', false, false], 'with no list open it gives back the gesture');
});

await ok('a_reset_lets_the_hold_of_an_open_list_go', () => {
  const { live } = rig();
  const keep = keeper(live, 'a');
  keep.around(() => 0);
  keep.reset();
  assert.equal(live.frozen('a'), false);
  keep.around(() => 0);
  keep.release();
  assert.equal(live.frozen('a'), false, 'one list after the reset is one hold');
});

await ok('the_reset_epoch_drops_a_give_back_planned_before_it', async () => {
  const { live } = rig();
  const keep = keeper(live, 'a');
  keep.take();
  keep.dropLater();
  keep.reset();
  assert.equal(live.frozen('a'), false, 'a reset lets every hold go');
  keep.take();
  await task();
  assert.equal(live.frozen('a'), true, 'the planned give-back is for a hold the reset let go');
});

await ok('a_hand_holds_while_gripped_and_lets_go_once_its_writes_are_answered', async () => {
  const { live } = rig();
  const keep = keeper(live, 'a');
  let answered;
  const pending = new Promise((resolve) => { answered = resolve; });
  const hand = keep.hand({ pending: () => pending, ms: 0 });
  hand.grip();
  hand.grip();
  assert.equal(live.frozen('a'), true);
  hand.letGo();
  await task();
  await task();
  assert.equal(live.frozen('a'), true, 'a write is not answered');
  answered();
  await task();
  await task();
  assert.equal(live.frozen('a'), false);
});

await ok('a_new_grip_keeps_the_hold_a_slow_answer_would_have_released', async () => {
  const { live } = rig();
  const keep = keeper(live, 'a');
  let answered;
  const hand = keep.hand({ pending: () => new Promise((resolve) => { answered = resolve; }), ms: 0 });
  hand.grip();
  hand.letGo();
  await task();
  hand.grip();
  answered();
  await task();
  await task();
  assert.equal(live.frozen('a'), true);
  hand.forget();
});

await ok('a_hand_let_go_after_a_reset_does_not_drop_the_hold_taken_since', async () => {
  const { live } = rig();
  const keep = keeper(live, 'a');
  const hand = keep.hand({ pending: () => Promise.resolve(), ms: 0 });
  hand.grip();
  hand.letGo();
  keep.reset();
  keep.take();
  await task();
  await task();
  assert.equal(live.frozen('a'), true, 'the hand held in an epoch that is over');
});

await ok('a_hand_gripped_again_after_a_reset_holds_again_without_forget', () => {
  const { live } = rig();
  const keep = keeper(live, 'a');
  const hand = keep.hand({ pending: () => Promise.resolve(), ms: 0 });
  hand.grip();
  keep.reset();
  assert.equal(live.frozen('a'), false);
  hand.grip();
  assert.equal(live.frozen('a'), true, 'a grip after a reset takes a new hold');
});

await ok('a_forgotten_hand_drops_nothing_and_holds_anew_at_its_next_grip', async () => {
  const log = logLive();
  const keep = keeper(log, 'a');
  const hand = keep.hand({ pending: () => Promise.resolve(), ms: 0 });
  hand.grip();
  hand.letGo();
  hand.forget();
  await task();
  await task();
  assert.deepEqual(log.calls, [['a', true, true]], 'the release on its way is forgotten');
  hand.grip();
  keep.drop();
  assert.deepEqual(log.calls.at(-1), ['a', true, true], 'the grip after it took a hold of its own');
});

/** A document that records its listeners and fires them. */
function fakeDoc() {
  const on = {};
  return {
    addEventListener(type, fn) { (on[type] ??= []).push(fn); },
    removeEventListener(type, fn) { on[type] = (on[type] ?? []).filter((one) => one !== fn); },
    fire(type) { for (const fn of [...(on[type] ?? [])]) fn(); },
    listening: (type) => (on[type] ?? []).length,
  };
}

await ok('a_release_with_no_press_going_gives_back_on_the_next_task', async () => {
  const { live } = rig();
  const keep = keeper(live, 'a');
  const doc = fakeDoc();
  const release = createRelease(doc, keep, new AbortController().signal);
  keep.take();
  release();
  assert.equal(live.frozen('a'), true);
  await task();
  assert.equal(live.frozen('a'), false);
});

await ok('a_release_in_a_press_waits_for_the_pointer_up', async () => {
  const { live } = rig();
  const keep = keeper(live, 'a');
  const doc = fakeDoc();
  const release = createRelease(doc, keep, new AbortController().signal);
  keep.take();
  doc.fire('pointerdown');
  release();
  await task();
  assert.equal(live.frozen('a'), true, 'the press is not over');
  doc.fire('pointerup');
  await task();
  assert.equal(live.frozen('a'), false);
  assert.equal(doc.listening('pointerup'), 1, 'only the permanent listener is left');
});

await ok('createLive_draws_each_section_for_the_parts_it_shows', () => {
  const { store, drawn } = rig();
  store.say('part', { device: 'dac', name: 'controls' });
  store.say('part', { device: 'other', name: 'volume' });
  store.say('volume', {});
  assert.deepEqual(drawn, [['b', 0], ['a', 0]]);
  drawn.length = 0;
  store.say('model');
  assert.deepEqual(drawn, [['a', 0], ['b', 0]], 'a new model draws every section');
});

await ok('createLive_tells_a_switch_to_another_device_once', () => {
  const { store, live } = rig();
  let told = 0;
  live.onSwitch(() => { told++; });
  store.say('model');
  store.say('device', { device: 'dac' });
  assert.equal(told, 0, 'the same device is no switch');
  store.show('amp');
  store.say('model');
  assert.equal(told, 1);
  store.say('device', { device: 'amp' });
  assert.equal(told, 1, 'told once');
  store.show('dac');
  store.say('device', { device: 'dac' });
  assert.equal(told, 2, 'a device event of the newly shown device is a switch too');
});

await ok('createLive_follows_the_device_its_caller_names_not_the_shown_one', () => {
  let follows = 'amp';
  const { store, live, drawn } = rig({ device: () => follows });
  assert.equal(store.shown(), 'dac', 'the page shows another device');
  let told = 0;
  live.onSwitch(() => { told++; });
  store.say('part', { device: 'dac', name: 'volume' });
  store.say('volume', { device: 'dac' });
  store.say('volume', {});
  store.say('device', { device: 'dac' });
  assert.deepEqual(drawn, [], "the shown device's events are not its own (a reading that names none is the shown one's)");
  store.say('part', { device: 'amp', name: 'volume' });
  store.say('volume', { device: 'amp' });
  assert.deepEqual(drawn, [['a', 0], ['a', 0]], 'the parts and readings of the device it follows are drawn');
  drawn.length = 0;
  store.say('device', { device: 'amp' });
  assert.deepEqual(drawn, [['a', 0], ['b', 0]], 'and its whole shape draws every section');
  store.show('amp');
  store.say('model');
  assert.equal(told, 0, 'another device shown on the page is no switch for it');
  follows = 'pre';
  store.say('model');
  assert.equal(told, 1, 'the caller naming another device is');
});

await ok('createLive_gives_each_draw_the_device_it_follows', () => {
  const store = fakeStore();
  const got = [];
  createLive({
    core: { model: store, computer: { on: () => () => {} } },
    signal: new AbortController().signal,
    view: {},
    device: () => 'amp',
    sections: { a: { parts: '*', draw: (page, device) => got.push([page.n, device]) } },
  });
  store.say('fleet');
  assert.deepEqual(got, [[0, 'amp']]);
});

await ok('createLive_refuses_a_caller_that_names_no_device', () => {
  const store = fakeStore();
  assert.throws(() => createLive({
    core: { model: store, computer: { on: () => () => {} } },
    signal: new AbortController().signal,
    view: {},
    sections: {},
  }), /device/);
});

await ok('createLive_draws_every_section_when_the_computer_side_changes', () => {
  const { computer, drawn, abort } = rig();
  assert.equal(computer.heard.size, 1, 'it listens to the computer');
  computer.say();
  assert.deepEqual(drawn, [['a', 0], ['b', 0]]);
  abort.abort();
  assert.equal(computer.heard.size, 0, 'and no longer after the page is gone');
});

await ok('createLive_keeps_the_stream_state_on_the_view', () => {
  const { store, view } = rig();
  assert.equal(view.state, undefined, 'the page sets its own start');
  store.say('state', 'lost');
  assert.equal(view.state, 'lost');
  store.say('model');
  assert.equal(view.state, 'open', "a model brings the store's state");
});

await ok('nothing_is_kept_or_drawn_after_the_page_is_unmounted', () => {
  const { store, live, drawn, abort } = rig();
  const keep = keeper(live, 'a');
  keep.take();
  abort.abort();
  assert.equal(store.listening(), 0, 'every listener of the store is gone');
  assert.equal(live.frozen('a'), false, 'the holds are let go');
  store.say('model');
  assert.deepEqual(drawn, []);
});

console.log(`core-freeze.mjs: ${n} cases`);
