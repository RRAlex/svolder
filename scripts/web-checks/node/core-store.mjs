/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/store.js, the page's model: a model replaces everything and says so once,
// a part event changes that device's part only, the stream's readings fold into the model, a
// write's answer applies its parts with the old copy beside the new, and the stream's model
// replaces a snapshot that went stale. Run: node core-store.mjs
import assert from 'node:assert/strict';
import { createStore } from '../../../bins/svolderd/web/core/store.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const volume = (raw) => ({ key: 'volume', lo: 0, hi: 100, raw, text: `${raw}`, target: null });
const tone = (mode) => ({ mode, note: null, bands: null });
const gungnir = () => ({
  name: 'Gungnir 2', url: 'Gungnir%202', layout: 'dac', volume: volume(40), meters: null,
  controls: [{ kind: 'slider', key: 'balance', raw: 3, text: '3' }],
  tone: tone('flat'), power: null, facts: {}, card: null,
});
const amp = () => ({
  name: 'Amp', url: 'Amp', layout: 'amp', volume: null, controls: [], tone: null,
  meters: { live: true, meters: [{ key: 'l', fraction: 0.2 }] },
});
const page = (over = {}) => ({
  v: 1, viewer: { signed_in: true, guest: false, writes: true },
  fleet: { devices: [], hops: [] }, shown: 'Gungnir%202', device: gungnir(),
  followed: { Amp: amp() }, card: null, host: null, strip: null, ...over,
});
const answers = (status, json) => async () => ({ status, json: async () => json });

await ok('a_model_replaces_everything_and_notifies_once', () => {
  const store = createStore(page(), { fetch: answers(200, {}) });
  const heard = [];
  store.on('model', (p) => heard.push(p));
  const next = page({ followed: {}, device: { ...gungnir(), volume: volume(7) } });
  store.replace(next);
  assert.equal(heard.length, 1, 'once');
  assert.equal(heard[0], next);
  assert.equal(store.page(), next);
  assert.equal(store.part('volume').raw, 7);
  assert.equal(store.device('Amp'), null, 'the followed device of the old page is gone');
  assert.equal(store.shown(), 'Gungnir%202');
});

await ok('a_part_event_updates_that_device_only', () => {
  const store = createStore(page(), { fetch: answers(200, {}) });
  const parts = [];
  store.on('part', (p) => parts.push(p));
  const before = store.part('meters', 'Amp');
  store.emit('meters', { device: 'Amp', live: true, meters: [{ key: 'l', fraction: 0.9 }] });
  assert.equal(store.part('meters', 'Amp').meters[0].fraction, 0.9);
  assert.equal(store.part('meters'), null, 'the shown device has no meters and gets none');
  assert.equal(store.part('volume').raw, 40, 'the shown device keeps its own volume');
  assert.notEqual(store.part('meters', 'Amp'), before);
  store.applyPart('Amp', 'power', { on: true });
  assert.equal(store.part('power', 'Amp').on, true);
  assert.equal(store.part('power'), null);
  assert.deepEqual(parts.map((p) => [p.device, p.name]), [['Amp', 'power']]);
  store.applyPart('Nobody', 'power', { on: true });
  assert.equal(parts.length, 1, 'a device the page does not hold changes nothing');
});

await ok('a_volume_event_folds_into_the_model', () => {
  const store = createStore(page(), { fetch: answers(200, {}) });
  const heard = [];
  store.on('volume', (d) => heard.push(d));
  store.emit('volume', { device: 'Gungnir%202', raw: 55, text: '55' });
  assert.equal(store.part('volume').raw, 55);
  assert.equal(store.part('volume').hi, 100, 'the fields the event lacks stay');
  assert.equal(store.part('volume').device, undefined, 'the tag is not part of the volume');
  assert.deepEqual(heard, [{ device: 'Gungnir%202', raw: 55, text: '55' }]);
  store.emit('level', { device: 'Gungnir%202', key: 'balance', raw: 5, text: '5' });
  assert.equal(store.part('controls')[0].raw, 5);
  store.emit('level', { device: 'Gungnir%202', key: 'nothing', raw: 5 });
  assert.equal(store.part('controls').length, 1);
});

await ok('a_write_answer_applies_its_parts', () => {
  const store = createStore(page(), { fetch: answers(200, {}) });
  const parts = [];
  store.on('part', (p) => parts.push(p));
  const old = store.part('tone');
  store.applyParts('Gungnir%202', { tone: tone('eq'), controls: [] });
  assert.equal(store.part('tone').mode, 'eq');
  assert.equal(parts.length, 2);
  const [first] = parts;
  assert.equal(first.name, 'tone');
  assert.equal(first.device, 'Gungnir%202');
  assert.equal(first.before, old, 'the old tone rides beside the new');
  assert.equal(first.data.mode, 'eq');
});

await ok('a_stale_first_snapshot_is_replaced_by_the_streams_model', () => {
  const store = createStore(page(), { fetch: answers(200, {}) });
  assert.equal(store.part('volume').raw, 40);
  store.replace(page({ device: { ...gungnir(), volume: volume(42) } }));
  assert.equal(store.part('volume').raw, 42);
});

await ok('a_replacing_model_with_no_strip_keeps_the_strip_of_the_same_device', () => {
  const bars = { bars: [1, 2, 3] };
  const store = createStore(page({ strip: bars }), { fetch: answers(200, {}) });
  store.replace(page({ strip: null }));
  assert.equal(store.page().strip, bars, 'the stream\'s model carries none');
  const own = { bars: [9] };
  store.replace(page({ strip: own }));
  assert.equal(store.page().strip, own, 'a strip the model carries replaces it');
  store.replace(page({ strip: null, shown: 'Amp', device: amp(), followed: {} }));
  assert.equal(store.page().strip, null, 'another device shown has not its history');
});

await ok('a_read_that_the_sign_in_has_ended_for_loads_the_sign_in_page', async () => {
  const out = [];
  const store = createStore(page(), {
    fetch: answers(401, { error: 'no' }),
    signedOut: () => out.push('out'),
  });
  assert.equal((await store.refresh()).ok, false);
  assert.deepEqual(out, ['out']);
});

await ok('the_state_is_said_once_and_a_fleet_and_a_host_are_kept', () => {
  const store = createStore(page(), { fetch: answers(200, {}) });
  const states = [];
  store.on('state', (s) => states.push(s));
  assert.equal(store.state(), 'open');
  store.setState('lost');
  store.setState('lost');
  store.setState('open');
  assert.deepEqual(states, ['lost', 'open']);
  store.emit('fleet', { devices: [{ url: 'Amp' }], hops: [] });
  assert.equal(store.page().fleet.devices[0].url, 'Amp');
  store.emit('host', { track: null });
  assert.deepEqual(store.page().host, { track: null });
  const error = console.error;
  console.error = () => {};
  const off = store.on('host', () => { throw new Error('a listener that throws'); });
  const after = [];
  store.on('host', (host) => after.push(host));
  store.emit('host', null);
  console.error = error;
  off();
  assert.deepEqual(after, [null], 'the listeners after one that throws still hear');
  assert.equal(store.page().host, null);
});

await ok('a_device_is_set_whole_and_a_listener_can_leave', () => {
  const store = createStore(page(), { fetch: answers(200, {}) });
  const heard = [];
  const off = store.on('device', (d) => heard.push(d.device));
  store.setDevice('Amp', { ...amp(), meters: null });
  assert.equal(store.part('meters', 'Amp'), null);
  off();
  store.setDevice('Amp', amp());
  assert.deepEqual(heard, ['Amp']);
  store.forget('Amp');
  assert.equal(store.device('Amp'), null);
});

await ok('refresh_reads_the_model_and_republishes_what_changed_and_what_a_refusal_unsettled', async () => {
  const calls = [];
  const fresh = { ...gungnir(), volume: volume(41), tone: tone('flat') };
  const store = createStore(page(), {
    fetch: async (path, init) => {
      calls.push([path, init?.method ?? 'GET']);
      return { status: 200, json: async () => ({ device: fresh, strip: null }) };
    },
  });
  const parts = [];
  store.on('part', (p) => parts.push(p.name));
  const answer = await store.refresh();
  assert.deepEqual(calls, [['api/v1/devices/Gungnir%202/model', 'GET']]);
  assert.equal(answer.ok, true);
  assert.equal(store.part('volume').raw, 41);
  assert.ok(parts.includes('volume'), 'a changed part');
  assert.ok(parts.includes('controls') && parts.includes('tone') && parts.includes('power')
    && parts.includes('device_presets'), 'the parts a refused write leaves on show');
  assert.ok(!parts.includes('meters'), 'an unchanged part the page did not touch stays');
  const gone = createStore(page(), { fetch: async () => { throw new Error('down'); } });
  assert.deepEqual(await gone.refresh(), { ok: false, status: 0, error: 'svolder did not answer.' });
});

console.log(`${n} core-store checks passed`);
