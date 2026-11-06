/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/title.js: the browser tab's title every theme gets from the core.
// Run: node core-title.mjs
import assert from 'node:assert/strict';
import { createStore } from '../../../bins/svolderd/web/core/store.js';
import { TITLE, createTitle, statusOf, titleOf } from '../../../bins/svolderd/web/core/title.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const model = (extra = {}) => ({
  v: 1,
  viewer: { signed_in: true, guest: false, writes: true },
  fleet: { devices: [{ url: 'Bifrost', name: 'Bifrost 3' }, { url: 'Amp', name: 'Amp' }], hops: [] },
  shown: 'Bifrost',
  device: { card: null },
  followed: {},
  card: null,
  host: null,
  strip: null,
  ...extra,
});
const make = (page = model()) => {
  const store = createStore(page, { fetch: async () => ({}) });
  const document = { title: 'svolder' };
  const title = createTitle({ store, document });
  return { store, document, title };
};

ok('the_title_names_the_device_and_connected', () => {
  const { document } = make();
  assert.equal(document.title, 'Bifrost 3 · connected — svolder');
});
ok('a_lost_stream_says_reconnecting', () => {
  const { store, document } = make();
  store.setState('lost');
  assert.equal(document.title, 'Bifrost 3 · reconnecting — svolder');
  store.setState('open');
  assert.equal(document.title, 'Bifrost 3 · connected — svolder');
});
ok('waiting_says_reading_again', () => {
  const { store, document } = make();
  store.setState('waiting');
  assert.equal(document.title, 'Bifrost 3 · reading again — svolder');
});
ok('resting_says_resting', () => {
  const { store, document } = make();
  store.setState('resting');
  assert.equal(document.title, 'Bifrost 3 · resting — svolder');
});
ok('an_unreachable_card_says_its_title_in_lower_case', () => {
  const { store, document } = make();
  store.applyPart('Bifrost', 'card', { title: 'Out of reach', kind: 'bad' });
  assert.equal(document.title, 'Bifrost 3 · out of reach — svolder');
  store.applyPart('Bifrost', 'card', null);
  assert.equal(document.title, 'Bifrost 3 · connected — svolder');
});
ok('a_fault_card_says_fault', () => {
  const { store, document } = make();
  store.applyPart('Bifrost', 'card', { title: 'Overheated', kind: 'fault' });
  assert.equal(document.title, 'Bifrost 3 · fault — svolder');
});
ok('the_lost_stream_outranks_a_card_and_a_page_card_counts_too', () => {
  assert.equal(statusOf(model({ device: { card: { title: 'Looking', kind: 'wait' } } }), 'lost'),
    TITLE.reconnecting);
  assert.equal(statusOf(model({ device: null, card: { title: 'Looking', kind: 'wait' } }), 'open'),
    'looking');
  assert.equal(statusOf(model({ device: { card: { title: '', kind: 'bad' } } }), 'open'), TITLE.fault);
});
ok('no_device_says_svolder', () => {
  const { store, document } = make();
  store.replace(model({ shown: null, device: null, fleet: { devices: [], hops: [] } }));
  assert.equal(document.title, 'svolder');
  assert.equal(titleOf({ name: undefined, status: 'connected', more: ['x'] }), 'svolder');
});
ok('a_device_switch_names_the_new_device', () => {
  const { store, document } = make();
  store.replace(model({ shown: 'Amp' }));
  assert.equal(document.title, 'Amp · connected — svolder');
});
ok('an_added_part_follows_the_status_and_goes_when_removed', () => {
  const { store, document, title } = make();
  const remove = title.add((page) => `${page.fleet.devices.length} devices`);
  assert.equal(document.title, 'Bifrost 3 · connected · 2 devices — svolder');
  store.setState('lost');
  assert.equal(document.title, 'Bifrost 3 · reconnecting · 2 devices — svolder');
  const none = title.add(() => null);
  assert.equal(document.title, 'Bifrost 3 · reconnecting · 2 devices — svolder');
  none();
  remove();
  assert.equal(document.title, 'Bifrost 3 · reconnecting — svolder');
});
ok('a_part_that_throws_is_left_out', () => {
  const { document, title } = make();
  const error = console.error;
  console.error = () => {};
  try {
    title.add(() => { throw new Error('no'); });
    title.add(() => 'fine');
  } finally {
    console.error = error;
  }
  assert.equal(document.title, 'Bifrost 3 · connected · fine — svolder');
});
ok('stop_lets_the_store_go_and_resets_the_title', () => {
  const { store, document, title } = make();
  title.stop();
  assert.equal(document.title, 'svolder');
  store.setState('lost');
  assert.equal(document.title, 'svolder');
});

console.log(`core-title.mjs: ${n} passed`);
