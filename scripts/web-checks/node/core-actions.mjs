/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/actions.js, what a theme does to the device: each action posts today's
// request, a guest's and a read-only page's actions send nothing and say why, a refusal is toasted
// and the model read again, an answer's toast is said and its parts applied, the volume's cap and
// the raise that waits for a live stream, and a device switch with its address. The post is a fake
// that answers when the test says. Run: node core-actions.mjs
import assert from 'node:assert/strict';
import { createActions } from '../../../bins/svolderd/web/core/actions.js';
import { createStore } from '../../../bins/svolderd/web/core/store.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
const flush = () => new Promise((resolve) => setImmediate(resolve));

const volume = (over = {}) => ({
  key: 'volume', lo: 0, hi: 100, raw: 30, text: '30', target: null, cap: null, ...over,
});
const dac = (over = {}) => ({
  name: 'Gungnir 2', url: 'Gungnir%202', volume: volume(), controls: [], tone: null,
  facts: { identify_gap_secs: 3 }, ...over,
});
const amp = () => ({ name: 'Amp', url: 'Amp', volume: null, controls: [], tone: null });
const pageOf = (viewer, over = {}) => ({
  v: 1, viewer, fleet: { devices: [], hops: [{ id: 'HD 650' }] }, shown: 'Gungnir%202',
  device: dac(), followed: { Amp: amp() }, card: null, host: null, strip: null, ...over,
});
const SIGNED = { signed_in: true, guest: false, writes: true, read_only: null };

// Everything the actions touch, faked. `reply(path, body)` answers a post: a status and JSON.
function rig({ viewer = SIGNED, page, reply = () => ({ status: 200 }), model } = {}) {
  const posts = [];
  const gets = [];
  const toasts = [];
  const pushed = [];
  const assigned = [];
  const store = createStore(page ?? pageOf(viewer), {
    signedOut: () => assigned.push('./'),
    fetch: async (path) => {
      gets.push(path);
      const answer = model ? model(path) : { status: 200, body: { device: dac(), strip: null } };
      return { status: answer.status, json: async () => answer.body ?? {} };
    },
  });
  const actions = createActions({
    store,
    post: async (path, body) => { posts.push([path, body]); return reply(path, body); },
    toast: (text, action) => toasts.push(action ? [text, action] : [text]),
    history: { pushState: (...args) => pushed.push(args) },
    location: { assign: (to) => assigned.push(to) },
  });
  return { store, actions, posts, gets, toasts, pushed, assigned };
}

await ok('each_action_posts_todays_request', async () => {
  const table = [
    [(a) => a.setVolume(30), 'api/v1/devices/Gungnir%202/volume', { raw: 30 }],
    [(a) => a.setControl('input', { value: 'USB' }), 'api/v1/devices/Gungnir%202/controls/input',
      { value: 'USB' }],
    [(a) => a.setMode('eq'), 'api/v1/devices/Gungnir%202/eq/mode', { mode: 'eq' }],
    [(a) => a.setBand(2, 'g', { step: 1 }), 'api/v1/devices/Gungnir%202/eq/bands/2',
      { param: 'g', step: 1 }],
    [(a) => a.setLoudness({ step: -1 }), 'api/v1/devices/Gungnir%202/eq/loudness', { step: -1 }],
    [(a) => a.recallPreset('Warm'), 'api/v1/devices/Gungnir%202/presets/recall', { name: 'Warm' }],
    [(a) => a.savePreset('Mine'), 'api/v1/devices/Gungnir%202/presets', { name: 'Mine' }],
    [(a) => a.renamePreset('Mine', 'Yours'), 'api/v1/devices/Gungnir%202/presets/rename',
      { from: 'Mine', to: 'Yours' }],
    [(a) => a.deletePreset('Mine'), 'api/v1/devices/Gungnir%202/presets/delete', { name: 'Mine' }],
    [(a) => a.recallSlot(2), 'api/v1/devices/Gungnir%202/device-presets/recall', { slot: 2 }],
    [(a) => a.saveSlot(2, true), 'api/v1/devices/Gungnir%202/device-presets/save',
      { slot: 2, confirm: true }],
    [(a) => a.saveSlot(2), 'api/v1/devices/Gungnir%202/device-presets/save',
      { slot: 2, confirm: false }],
    [(a) => a.applySetting('Warm'), 'api/v1/devices/Gungnir%202/device-presets/apply',
      { name: 'Warm' }],
    [(a) => a.press('next'), 'api/v1/host/press', { button: 'next' }],
    [(a) => a.connect(), 'api/v1/devices/Gungnir%202/connect', {}],
    [(a) => a.identify(), 'api/v1/devices/Gungnir%202/identify', {}],
    [(a) => a.stopClimb(), 'api/v1/devices/Gungnir%202/volume/stop', {}],
    [(a) => a.setControl('mute', { value: true }, { device: 'Amp' }),
      'api/v1/devices/Amp/controls/mute', { value: true }],
    [(a) => a.hops.add({ name: 'HD 650', kind: 'headphones' }), 'api/v1/hops',
      { name: 'HD 650', kind: 'headphones' }],
    [(a) => a.hops.edit('HD 650', { note: null }), 'api/v1/hops/edit', { id: 'HD 650', note: null }],
    [(a) => a.hops.remove('HD 650'), 'api/v1/hops/remove', { id: 'HD 650' }],
    [(a) => a.hops.move('HD 650', 0), 'api/v1/hops/move', { id: 'HD 650', to: 0 }],
  ];
  for (const [run, path, body] of table) {
    const { actions, posts } = rig();
    const answer = await run(actions);
    assert.deepEqual(posts, [[path, body]], path);
    assert.equal(answer.ok, true, path);
    assert.equal(JSON.stringify(posts[0][1]), JSON.stringify(body), `${path}: the body's order`);
  }
});

await ok('a_guest_action_sends_nothing_and_offers_sign_in', async () => {
  const guest = { signed_in: false, guest: true, writes: false, read_only: null };
  const { actions, posts, gets, toasts } = rig({ viewer: guest });
  const answers = [
    await actions.setVolume(31),
    await actions.setControl('input', { value: 'USB' }),
    await actions.press('next'),
    await actions.hops.add({ name: 'x', kind: 'amp' }),
    await actions.saveSlot(1, true),
  ];
  assert.deepEqual(posts, []);
  assert.deepEqual(gets, []);
  assert.equal(toasts.length, 5);
  assert.deepEqual(toasts[0], ['Sign in to change this', { label: 'Sign in', href: './?sign-in' }]);
  assert.ok(answers.every((a) => a.ok === false && a.error === 'Sign in to change this'));
  actions.signInPrompt();
  assert.deepEqual(toasts.at(-1), ['Sign in to change this', { label: 'Sign in', href: './?sign-in' }]);
});

await ok('a_read_only_action_sends_nothing_and_says_why', async () => {
  const viewer = { signed_in: true, guest: false, writes: false, read_only: 'Read-only: [web] writes is off' };
  const { actions, posts, toasts } = rig({ viewer });
  const answer = await actions.setMode('eq');
  assert.deepEqual(posts, []);
  assert.deepEqual(toasts, [['Read-only: [web] writes is off']]);
  assert.equal(answer.ok, false);
});

await ok('a_refusal_toasts_its_words_and_reads_the_model_again', async () => {
  const { actions, toasts, gets } = rig({
    reply: () => ({ status: 409, error: 'Refused: test' }),
  });
  const answer = await actions.setControl('phase', { value: true });
  assert.deepEqual(toasts, [['Refused: test']]);
  assert.deepEqual(gets, ['api/v1/devices/Gungnir%202/model']);
  assert.equal(answer.ok, false);
  assert.equal(answer.status, 409);
  assert.equal(answer.error, 'Refused: test');
  // No answer at all is a refusal too.
  const silent = rig({ reply: () => ({ status: 0, error: 'svolder did not answer.' }) });
  await silent.actions.setMode('flat');
  assert.deepEqual(silent.toasts, [['svolder did not answer.']]);
});

await ok('the_answer_toast_is_said_and_its_parts_applied', async () => {
  const { actions, toasts, store } = rig({
    reply: () => ({
      status: 200, toast: 'Recalled Preset 2', parts: { tone: { mode: 'eq', note: null } },
    }),
  });
  const parts = [];
  store.on('part', (p) => parts.push(p.name));
  const answer = await actions.recallSlot(2);
  assert.deepEqual(toasts, [['Recalled Preset 2']]);
  assert.deepEqual(parts, ['tone']);
  assert.equal(store.part('tone').mode, 'eq');
  assert.equal(answer.toast, 'Recalled Preset 2');
  assert.equal(answer.ok, true);
});

await ok('identify_says_its_words_with_the_gap_the_facts_carry', async () => {
  const sent = rig({ reply: () => ({ status: 200, sent: false }) });
  await sent.actions.identify();
  assert.deepEqual(sent.toasts, [['Just sent — wait 3 s']]);
  const refused = rig({ reply: () => ({ status: 409, error: 'device not connected' }) });
  await refused.actions.identify();
  assert.deepEqual(refused.toasts, [['device not connected']]);
  const dead = rig({ reply: () => ({ status: 0 }) });
  await dead.actions.connect();
  assert.deepEqual(dead.toasts, [['The device did not answer.']]);
});

await ok('the_cap_clamps_and_a_raise_waits_for_a_live_stream', async () => {
  const capped = rig({ page: pageOf(SIGNED, { device: dac({ volume: volume({ cap: 40 }) }) }) });
  await capped.actions.setVolume(45);
  assert.deepEqual(capped.posts, [['api/v1/devices/Gungnir%202/volume', { raw: 40 }]]);
  await capped.actions.setVolume(150);
  assert.equal(capped.posts.at(-1)[1].raw, 40);
  const live = rig({ page: pageOf(SIGNED, { device: dac({ volume: volume({ raw: 40, cap: 50 }) }) }) });
  live.store.setState('lost');
  const raised = await live.actions.setVolume(41);
  assert.deepEqual(live.posts, [], 'a raise is not sent while the link is lost');
  assert.equal(raised.ok, false);
  assert.equal(raised.dropped, true, 'the answer says it was not sent');
  await live.actions.setVolume(39);
  assert.deepEqual(live.posts, [['api/v1/devices/Gungnir%202/volume', { raw: 39 }]], 'a lowering goes');
  live.store.setState('waiting');
  await live.actions.setVolume(45);
  assert.equal(live.posts.length, 1, 'nor while the page waits on waking');
  live.store.setState('open');
  await live.actions.setVolume(45);
  assert.equal(live.posts.at(-1)[1].raw, 45);
});

await ok('a_raise_queued_for_a_device_no_longer_shown_is_not_sent', async () => {
  const gates = [];
  const { actions, posts, store } = rig({
    // The first write waits for the test; any later one is answered at once.
    reply: () => (gates.length === 0
      ? new Promise((resolve) => gates.push(resolve)) : Promise.resolve({ status: 200 })),
    model: () => ({ status: 200, body: { device: amp(), strip: null } }),
  });
  const first = actions.setVolume(25);
  actions.setVolume(35);
  await flush();
  assert.deepEqual(posts.map(([, b]) => b.raw), [25], 'a lowering is in flight, a raise waits');
  await actions.showDevice('Amp');
  assert.equal(store.device('Gungnir%202'), null);
  gates[0]({ status: 200 });
  const answer = await first;
  assert.deepEqual(posts.map(([, b]) => b.raw), [25], 'the raise has no gesture behind it now');
  assert.equal(answer.dropped, true);
});

await ok('a_refusal_with_no_words_says_the_words_it_always_did', async () => {
  const bare = () => ({ status: 409 });
  const said = async (run) => {
    const { actions, toasts } = rig({ reply: bare });
    await run(actions);
    return toasts[0][0];
  };
  assert.equal(await said((a) => a.setMode('eq')), 'The change was not made.');
  assert.equal(await said((a) => a.setControl('phase', { value: true })), 'The change was not made.');
  assert.equal(await said((a) => a.recallSlot(1)), 'The change was not made.');
  assert.equal(await said((a) => a.setVolume(31)), 'Refused.');
  assert.equal(await said((a) => a.setBand(1, 'g', { step: 1 })), 'Refused.');
  assert.equal(await said((a) => a.press('next')), 'Refused.');
});

await ok('one_volume_write_is_in_flight_and_the_newest_waits', async () => {
  const gates = [];
  const { actions, posts } = rig({
    reply: (path, body) => new Promise((resolve) => {
      gates.push(() => resolve({ status: 200, landed: { raw: body.raw } }));
    }),
  });
  const first = actions.setVolume(31);
  const second = actions.setVolume(32);
  const third = actions.setVolume(33);
  await flush();
  assert.deepEqual(posts.map(([, b]) => b.raw), [31], 'one in flight');
  gates.shift()();
  await flush();
  assert.deepEqual(posts.map(([, b]) => b.raw), [31, 33], 'the newest waited, 32 was replaced');
  gates.shift()();
  const answer = await first;
  assert.equal(answer.ok, true);
  assert.equal(answer.landed.raw, 33, 'the run that began it answers with the last write\'s answer');
  assert.equal((await second).queued, true);
  assert.equal((await third).queued, true);
});

await ok('a_refused_volume_write_drops_what_waited_and_reads_the_model', async () => {
  const gates = [];
  const { actions, posts, toasts, gets } = rig({
    reply: () => new Promise((resolve) => gates.push(resolve)),
  });
  const first = actions.setVolume(31);
  actions.setVolume(32);
  await flush();
  gates[0]({ status: 409, error: 'Refused: loud' });
  await first;
  assert.equal(posts.length, 1, 'the waiting write goes with the refusal');
  assert.deepEqual(toasts, [['Refused: loud']]);
  assert.equal(gets.length, 1);
});

await ok('a_device_that_cannot_be_shown_stays_and_says_so', async () => {
  const gone = rig({ model: () => ({ status: 404, body: {} }) });
  const answer = await gone.actions.showDevice('Amp');
  assert.deepEqual(gone.toasts, [['That device could not be shown.']]);
  assert.equal(gone.store.shown(), 'Gungnir%202');
  assert.deepEqual(gone.pushed, []);
  assert.equal(answer.ok, false);
  const down = createStore(pageOf(SIGNED), { fetch: async () => { throw new Error('down'); } });
  const toasts = [];
  const actions = createActions({
    store: down, post: async () => ({ status: 200 }), toast: (t) => toasts.push(t),
    history: { pushState() {} }, location: { assign() {} },
  });
  await actions.showDevice('Amp');
  assert.deepEqual(toasts, ['svolder did not answer.']);
  assert.equal(down.shown(), 'Gungnir%202');
  const said = rig({ model: () => ({ status: 404, body: { error: 'No such device' } }) });
  await said.actions.showDevice('Amp');
  assert.deepEqual(said.toasts, [['No such device']]);
  const out = rig({ model: () => ({ status: 401, body: {} }) });
  await out.actions.showDevice('Amp');
  assert.deepEqual(out.assigned, ['./']);
});

await ok('show_device_pushes_the_address', async () => {
  const { actions, store, pushed } = rig({
    model: () => ({ status: 200, body: { device: { ...amp(), card: { kind: 'warn' } }, strip: { bars: [] } } }),
  });
  const models = [];
  store.on('model', (p) => models.push(p.shown));
  const answer = await actions.showDevice('Amp');
  assert.equal(answer.ok, true);
  assert.equal(pushed.length, 1);
  assert.equal(pushed[0][2], './?device=Amp');
  assert.equal(store.shown(), 'Amp');
  assert.equal(store.device().name, 'Amp');
  assert.deepEqual(models, ['Amp']);
  assert.equal(store.page().card, null, 'the model has a card only when no device is shown');
  assert.equal(store.page().followed.Amp, undefined, 'a shown device is not also followed');
  await actions.showDevice('Gungnir%202', { push: false });
  assert.equal(pushed.length, 1, 'a history step pushes nothing');
  assert.equal(store.shown(), 'Gungnir%202');
});

await ok('the_newest_switch_wins', async () => {
  const gates = [];
  const store = createStore(pageOf(SIGNED), {
    fetch: (path) => new Promise((resolve) => gates.push([path, resolve])),
  });
  const pushed = [];
  const actions = createActions({
    store, post: async () => ({ status: 200 }), toast() {}, history: { pushState: (...a) => pushed.push(a) },
    location: { assign() {} },
  });
  const first = actions.showDevice('Amp');
  const second = actions.showDevice('Gungnir%202');
  const reply = (url, name) => ({
    status: 200, json: async () => ({ device: { ...amp(), name, url } }),
  });
  gates[1][1](reply('Gungnir%202', 'second'));
  gates[0][1](reply('Amp', 'first'));
  await Promise.all([first, second]);
  assert.equal(store.device().name, 'second');
  assert.equal(pushed.length, 1);
});

await ok('sign_out_ends_the_session_or_says_why', async () => {
  const done = rig();
  await done.actions.signOut();
  assert.deepEqual(done.posts, [['logout', {}]]);
  assert.deepEqual(done.assigned, ['./']);
  const refused = rig({ reply: () => ({ status: 500, error: 'No' }) });
  await refused.actions.signOut();
  assert.deepEqual(refused.toasts, [['No']]);
  assert.deepEqual(refused.assigned, []);
});

await ok('a_hop_answer_updates_the_fleet_and_a_refusal_only_says_why', async () => {
  const hops = [{ id: 'A' }, { id: 'B' }];
  const { actions, store } = rig({ reply: () => ({ status: 200, hops }) });
  await actions.hops.move('B', 0);
  assert.deepEqual(store.page().fleet.hops, hops);
  const refused = rig({ reply: () => ({ status: 422, error: 'Saving hops is off: read-only' }) });
  await refused.actions.hops.remove('A');
  assert.deepEqual(refused.toasts, [['Saving hops is off: read-only']]);
  assert.equal(refused.gets.length, 0, 'a hop write has no device to read again');
});

console.log(`${n} core-actions checks passed`);
