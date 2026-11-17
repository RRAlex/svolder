/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/old-readings.js: the frame's badge on readings from before a link dropped.
// Run: node core-old-readings.mjs
import assert from 'node:assert/strict';
import { createStore } from '../../../bins/svolderd/web/core/store.js';
import {
  OLD_SHORT, OLD_WORDS, createOldBadge, deviceOld, readingsOld,
} from '../../../bins/svolderd/web/core/old-readings.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const WARN = { kind: 'warn', title: 'out of reach', detail: '', faults: [], retry_in: 5, try_now: true };
const LINK_ON = { word: 'connected', state: 'on', old: false };
const LINK_OLD = { word: 'not connected', state: 'off', old: true };
const model = (card = null, extra = {}, link = LINK_ON) => ({
  v: 1,
  viewer: { signed_in: true, guest: false, writes: true },
  fleet: { devices: [{ url: 'Bifrost', name: 'Bifrost 3' }], hops: [] },
  shown: 'Bifrost',
  device: { card, link },
  followed: {},
  card: null,
  host: null,
  strip: null,
  ...extra,
});
const element = () => {
  const el = {
    hidden: false, attrs: {}, title: '', textContent: '', children: [], className: '',
  };
  el.setAttribute = (k, v) => { el.attrs[k] = v; };
  el.append = (...nodes) => { el.children.push(...nodes); };
  return el;
};
const make = (page = model()) => {
  const store = createStore(page, { fetch: async () => ({}) });
  const made = [];
  const root = { append: () => {} };
  const document = {
    createElement: () => { const el = element(); made.push(el); return el; },
    documentElement: { dataset: {} },
  };
  createOldBadge({ document, root, store });
  const [badge, say] = [made[0], made.find((el) => el.attrs.role === 'status')];
  return { store, badge, say, document };
};

const down = (store, url = 'Bifrost') => store.applyPart(url, 'link', LINK_OLD);
const up = (store, url = 'Bifrost') => store.applyPart(url, 'link', LINK_ON);

ok('a_device_that_is_up_has_no_old_readings', () => {
  const { store, badge, document } = make();
  assert.equal(readingsOld(store), false);
  assert.equal(badge.hidden, true);
  assert.equal(document.documentElement.dataset.readingsOld, undefined);
});
ok('a_link_down_marks_the_readings_until_the_device_says_it_is_fresh', () => {
  const { store, badge, document } = make();
  down(store);
  assert.equal(badge.hidden, false);
  assert.equal(document.documentElement.dataset.readingsOld, 'true');
  // the link back with no fresh reading yet keeps the daemon's `old` flag: still marked
  store.applyPart('Bifrost', 'link', { word: 'connected', state: 'on', old: true });
  assert.equal(badge.hidden, false);
  up(store);
  assert.equal(badge.hidden, true);
  assert.equal(document.documentElement.dataset.readingsOld, undefined);
});
ok('the_badge_says_its_words_in_its_title_and_a_live_region', () => {
  const { store, badge, say } = make();
  assert.equal(badge.title, OLD_WORDS);
  assert.equal(badge.children[0].textContent, OLD_SHORT);
  assert.equal(say.attrs['aria-live'], 'polite');
  assert.equal(say.textContent, '');
  down(store);
  assert.equal(say.textContent, OLD_WORDS);
  up(store);
  assert.equal(say.textContent, '');
  assert.equal(OLD_WORDS, 'readings from before the link dropped');
});
ok('a_lost_stream_marks_them_until_the_stream_opens', () => {
  const { store, badge } = make();
  store.setState('lost');
  assert.equal(badge.hidden, false);
  store.setState('open');
  assert.equal(badge.hidden, true);
});
ok('a_resting_or_waiting_page_is_not_a_dropped_link', () => {
  const { store, badge } = make();
  store.setState('resting');
  assert.equal(badge.hidden, true);
  store.setState('waiting');
  assert.equal(badge.hidden, true);
});
ok('a_device_with_no_readings_has_nothing_old', () => {
  for (const kind of ['dim']) {
    const { store, badge } = make(model({ ...WARN, kind }, {}, LINK_OLD));
    assert.equal(readingsOld(store), false, kind);
    store.setState('lost');
    assert.equal(readingsOld(store), false, `${kind} lost`);
    assert.equal(badge.hidden, true, kind);
  }
});
ok('no_device_shown_has_nothing_old', () => {
  const { store, badge } = make(model(null, { shown: null, device: null }));
  store.setState('lost');
  assert.equal(readingsOld(store), false);
  assert.equal(badge.hidden, true);
});
ok('a_followed_device_is_read_by_its_own_link', () => {
  const page = model(null, { followed: { Amp: { card: null, link: LINK_OLD } } });
  const { store, badge } = make(page);
  assert.equal(badge.hidden, true, 'the shown device is fine');
  assert.equal(readingsOld(store, 'Amp'), true);
  assert.equal(deviceOld(store.device('Amp')), true);
  assert.equal(deviceOld({ card: { kind: 'dim' }, link: LINK_OLD }), false);
  assert.equal(deviceOld(null), false);
});
ok('a_replaced_model_is_read_again', () => {
  const { store, badge } = make();
  store.replace(model(WARN, {}, LINK_OLD));
  assert.equal(badge.hidden, false);
  store.replace(model(null));
  assert.equal(badge.hidden, true);
});

console.log(`core-old-readings: ${n} passed`);
