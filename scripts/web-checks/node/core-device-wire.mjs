/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the settings of a device's page in the Devices popup (core/popups/devices/
// wire.js with settings.js), on the real core over a jsdom page, every case with another device on
// show (A) than the one the page is about (B): the settings stand above the key graphs on a
// device's page and not on the Overview; each control sends its write naming the page's device;
// the place says it is saving while the answer is on its way; every refusal reads its words at its
// control; the focus returns to the control; what is typed (the name, the passphrase's code) stays
// through a put and a stream update; a rename moves the page to the new address, in place, and the
// shown device's rename moves the page behind with it; a setting changed elsewhere reaches the
// page; a passphrase on a playing device is asked once; no passphrase is in the page after it is
// sent; Remove… asks once (the safe answer has the focus), sends the removal naming the page's
// device, and ends on the Overview with a line that says what was removed, the shown device's
// removal and a rename after it included. Run: node core-device-wire.mjs
process.env.TZ = 'UTC';
const { default: assert } = await import('node:assert/strict');
const { page } = await import('./dom.mjs');
const { createCore } = await import('../../../bins/svolderd/web/core/core.js');
const { DEVICES } = await import('../../../bins/svolderd/web/core/popups/listed.js');
const { settingId, ASK_ID, REMOVE_ASK_ID } = await import('../../../bins/svolderd/web/core/popups/devices/settings.js');
const { FACTS_ID, IDENTIFY_ID } = await import('../../../bins/svolderd/web/core/popups/devices/facts.js');
const { READINGS_ID, READINGS_BODY_ID } = await import('../../../bins/svolderd/web/core/popups/devices/readings.js');
const { REREAD_MS } = await import('../../../bins/svolderd/web/core/popups/devices/wire.js');
const { DEVICE_REFUSALS } = await import('../../../bins/svolderd/web/core/device-config-actions.js');

let n = 0;
const wanted = process.env.WEB_CHECK_CASE ?? '';
const ok = async (name, fn) => {
  if (!name.includes(wanted)) return;
  await fn();
  n++;
  console.log('ok', name);
};
console.warn = () => {};

const NOW = Date.UTC(2026, 10, 11, 14, 30, 0);
const SECRET = '8642';
const BODY = '<div id="frame"><button id="fx-button">Menu</button></div>'
  + '<div id="theme"><button id="knob">Volume</button></div><div id="toasts"></div>';
const link = (on) => (on ? { word: 'connected', state: 'on' } : { word: 'not connected', state: 'off' });
// the daemon's address of a name: everything but letters, digits and -._~ is percent-encoded
const seg = (name) => encodeURIComponent(name).replace(/[!'()*]/g, (c) => `%${c.charCodeAt(0).toString(16).toUpperCase()}`);
const fleet = (list) => ({
  devices: list.map((name) => ({ url: seg(name), name, link: link(true) })), hops: [],
});
const model = (name, over = {}) => ({
  name, url: name, link: link(true), volume: null, controls: [], meters: null, ...over,
});
const PROFILES = [
  { name: 'lyr-plus', display: 'Lyr+' }, { name: 'amp-common', base: true }, { name: 'bifrost' },
];
const start = () => [
  { unit: 'unit-A', name: 'A', passphrase: 'auto', connected: true, default: true },
  { unit: 'unit-B', name: 'B', passphrase: 'set', profile: 'bifrost', connected: true },
  { unit: 'unit-C', name: 'C', passphrase: 'auto', connected: true },
];
const factsOf = (name, over = {}) => ({
  product: 'Bifrost 3', product_id: 5, unit: `unit-${name}`, firmware: '7', dsp: 3, profile: 'bifrost',
  link: 'connected', signal: '−60 dBm',
  signal_age: { head: '−60 dBm live', heard_at_ms: NOW - 5000, live: true }, passphrase_note: null,
  rows: [{ id: '1.1', label: 'Input', value: '3 (Optical)', heard_at_ms: NOW - 10000 }],
  now_ms: NOW, identify: true, identify_offered: true, identify_gap_secs: 2, ...over,
});
const empty = { now_ms: NOW, bin_ms: 1000, secs: 1800, refused: [], link: [], series: [] };

class FakeSource {
  static all = [];

  constructor(url) {
    this.url = url;
    this.listeners = {};
    FakeSource.all.push(this);
  }

  addEventListener(name, fn) { (this.listeners[name] ??= []).push(fn); }

  close() {}

  send(name, data) {
    for (const fn of this.listeners[name] ?? []) fn({ data: JSON.stringify(data) });
  }
}

/** The real core with A on show. The daemon is a small fake: `rows` is its configuration, a POST
 * to `api/v1/devices/{url}/edit|default|remove` is kept in `posts` and applied unless `refuse`
 * (a function of the post) answers `{ status, body }`; `hold` is a promise a write waits for. */
function rig({
  playing = false, path = null, viewer = { signed_in: true, guest: false, writes: true, read_only: null },
} = {}) {
  const p = page(BODY, { url: 'http://svolder.test/?device=A' });
  globalThis.window = p.window;
  globalThis.document = p.document;
  globalThis.AbortController = p.window.AbortController;
  Object.defineProperty(p.document, 'visibilityState', { value: 'visible' });
  globalThis.requestAnimationFrame = (fn) => p.window.setTimeout(fn, 0);
  FakeSource.all = [];
  p.window.EventSource = FakeSource;
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  const daemon = {
    rows: start(), posts: [], refuse: null, hold: null, modelHold: null, model404: [], facts: {}, reads: [],
    changed: null, listHold: null, asked: [],
  };
  p.window.fetch = async (path, init = {}) => {
    daemon.asked.push(`${init.method ?? 'GET'} ${path}`);
    const reply = (status, body) => ({ status, ok: status === 200, json: async () => body });
    const post = /^api\/v1\/devices\/([^/]+)\/(edit|default|remove|identify)$/.exec(path);
    if (post) {
      const sent = { device: post[1], route: post[2], body: JSON.parse(init.body ?? '{}') };
      daemon.posts.push(sent);
      if (daemon.hold) await daemon.hold;
      const no = daemon.refuse?.(sent);
      if (no?.down) throw new TypeError('the daemon is down');
      if (no) return reply(no.status, no.body);
      if (post[2] === 'identify') return reply(200, { sent: true });
      const row = daemon.rows.find((one) => seg(one.name) === post[1]);
      if (row && post[2] === 'edit') {
        if ('name' in sent.body) row.name = sent.body.name;
        if ('profile' in sent.body) row.profile = sent.body.profile ?? undefined;
        if ('passphrase' in sent.body) {
          row.passphrase = ['auto', 'none'].includes(sent.body.passphrase) ? sent.body.passphrase : 'set';
        }
      }
      if (row && post[2] === 'remove') daemon.rows.splice(daemon.rows.indexOf(row), 1);
      if (row && post[2] === 'default') {
        for (const one of daemon.rows) one.default = one === row;
      }
      // the daemon's own write is read back: its answer says no change
      daemon.changed = null;
      return reply(200, {
        devices: structuredClone(daemon.rows), profiles: PROFILES, config_changed: null,
      });
    }
    if (path === 'api/v1/devices') {
      // a read says what stood when it was asked, however late its answer comes
      const said = {
        devices: structuredClone(daemon.rows), profiles: PROFILES, config_changed: daemon.changed,
      };
      if (daemon.listHold) await daemon.listHold;
      return reply(200, said);
    }
    const one = /^api\/v1\/devices\/([^/]+)\/(model|series)/.exec(path);
    if (one?.[2] === 'model') {
      daemon.reads.push(one[1]);
      if (daemon.modelHold) await daemon.modelHold;
      if (daemon.model404.includes(one[1])) return reply(404, { error: 'not listed yet' });
      return reply(200, { device: model(one[1], daemon.facts[one[1]] ? { facts: daemon.facts[one[1]] } : {}) });
    }
    if (one?.[2] === 'series') return reply(200, empty);
    if (path === 'api/v1/walk') return reply(200, { job: 7 });
    return reply(200, {});
  };
  globalThis.fetch = (...args) => p.window.fetch(...args);
  globalThis.location = { assign: () => {} };
  const ticks = [];
  // the popup's reread timer is the one the cases step; the core's ticker of the ages is kept apart
  const agers = [];
  p.window.setInterval = (fn, ms) => {
    const id = ticks.length + agers.length + 1;
    (ms === REREAD_MS ? ticks : agers).push({ fn, ms, id });
    return id;
  };
  p.window.clearInterval = () => {};
  const keyListeners = [];
  for (const target of [p.window, p.document]) {
    const add = target.addEventListener.bind(target);
    target.addEventListener = (kind, ...rest) => {
      if (/^key/.test(kind)) keyListeners.push(kind);
      return add(kind, ...rest);
    };
  }
  const core = createCore({
    page: {
      v: 1, viewer, fleet: fleet(['A', 'B', 'C']), shown: 'A',
      device: model('A'), followed: {}, card: null, host: null, strip: null,
    },
    themes: [],
    window: p.window,
  });
  core.stream.open();
  keyListeners.length = 0;
  const $ = (id) => p.document.getElementById(id);
  const calm = async () => { for (let i = 0; i < 8; i++) await p.settle(); };
  const open = async (to) => {
    $('knob').focus();
    core.router.open(DEVICES, to);
    await calm();
    if (playing) {
      FakeSource.all.at(-1).send('device', {
        device: 'B', data: model('B', { playing: { track: { status: 'playing' }, ...(path ? { path } : {}) } }),
      });
      await calm();
    }
  };
  const type = (id, text) => {
    const field = $(id);
    field.focus();
    field.value = text;
    field.dispatchEvent(new p.window.Event('input', { bubbles: true }));
  };
  const click = async (id) => { $(id).click(); await calm(); };
  const choose = async (kind) => {
    const radio = $(settingId('passphrase', `kind-${kind}`));
    radio.checked = true;
    radio.dispatchEvent(new p.window.Event('change', { bubbles: true }));
    await calm();
  };
  const submit = async (setting) => {
    $(settingId(setting, 'save')).click();
    await calm();
  };
  const words = (setting) => $(settingId(setting, 'words')).textContent;
  return {
    ...p, core, daemon, ticks, agers, keyListeners, $, calm, open, type, click, choose, submit, words,
    source: () => FakeSource.all.at(-1),
    at: () => p.document.activeElement?.id ?? null,
  };
}

await ok('a_devices_page_has_its_settings_above_the_graphs_and_the_overview_has_none', async () => {
  const r = rig();
  await r.open('d-B');
  const block = r.$('popup-devices-device-42');
  const kids = [...block.children].map((el) => el.id || el.className);
  assert.deepEqual(kids.slice(0, 4), ['fpd-head', 'popup-devices-state-42', 'popup-devices-settings',
    'popup-devices-graphs-42'], 'name, state, settings, graphs');
  assert.equal(r.$(settingId('name', 'field')).value, 'B');
  assert.match(r.$(settingId('profile')).textContent, /bifrost/);
  assert.match(r.$(settingId('passphrase')).textContent, /Set/);
  assert.ok(r.$(settingId('default', 'make')), 'B is not the default: Make default');
  assert.equal(r.core.model.shown(), 'A');
  assert.deepEqual(r.keyListeners, [], 'no key listener of its own');
  r.core.router.step('d-A');
  await r.calm();
  assert.equal(r.$(settingId('default', 'make')), null, 'A is the default');
  assert.match(r.$(settingId('default')).textContent, /Default/);
  r.core.router.step(null);
  await r.calm();
  r.core.router.step('overview');
  await r.calm();
  assert.equal(r.$('popup-devices-settings'), null, 'the Overview has no settings');
  r.window.close();
});

await ok('the_name_is_saved_by_enter_or_save_naming_the_pages_device_and_the_page_follows', async () => {
  const r = rig();
  await r.open('d-B');
  const before = r.window.history.length;
  r.type(settingId('name', 'field'), 'Desk amp');
  await r.submit('name');
  assert.deepEqual(r.daemon.posts, [{ device: 'B', route: 'edit', body: { name: 'Desk amp' } }],
    'the page\'s device, not the shown one');
  assert.deepEqual(r.core.router.state(), { device: 'A', popup: 'devices', page: 'd-Desk%20amp' },
    'the page followed the device to its new address, A still on show');
  assert.equal(r.window.history.length, before, 'in place: no entry pushed, so Back has no dead one');
  assert.equal(r.$(settingId('name', 'field')).value, 'Desk amp');
  assert.equal(r.at(), settingId('name', 'field'), 'the focus is on the name field again');
  assert.match(r.document.title, /Devices · Desk amp/, 'the tab\'s title follows');
  assert.ok([...r.document.querySelectorAll('#popup-devices .fp-entry')].some(
    (el) => el.textContent === 'Desk amp'), 'the list follows');
  r.window.close();
});

await ok('the_form_is_sent_by_enter_too_and_an_unchanged_or_empty_name_sends_nothing', async () => {
  const r = rig();
  await r.open('d-B');
  r.$(settingId('name', 'field')).form.dispatchEvent(new r.window.Event('submit', { cancelable: true, bubbles: true }));
  await r.calm();
  assert.deepEqual(r.daemon.posts, [], 'the same name: nothing is sent');
  r.type(settingId('name', 'field'), '   ');
  await r.submit('name');
  assert.deepEqual(r.daemon.posts, []);
  assert.match(r.words('name'), /cannot be empty/);
  assert.equal(r.$(settingId('name', 'field')).getAttribute('aria-invalid'), 'true');
  r.type(settingId('name', 'field'), 'Desk');
  r.$(settingId('name', 'field')).form.dispatchEvent(new r.window.Event('submit', { cancelable: true, bubbles: true }));
  await r.calm();
  assert.equal(r.daemon.posts.length, 1, 'Enter sends');
  r.window.close();
});

await ok('a_submit_that_falls_through_never_navigates', async () => {
  const r = rig();
  await r.open('overview');
  // a form of the popup on a page that has no device: nothing is sent, yet the browser must not
  // take the submit as its own (a native GET would put the secret in the address)
  const form = r.document.createElement('form');
  form.dataset.setting = 'passphrase';
  r.document.querySelector('.fp').append(form);
  const event = new r.window.Event('submit', { cancelable: true, bubbles: true });
  form.dispatchEvent(event);
  await r.calm();
  assert.equal(event.defaultPrevented, true, 'the submit is stopped even with no device on the page');
  assert.deepEqual(r.daemon.posts, []);
  r.window.close();
});

await ok('while_a_write_is_on_its_way_the_place_says_so_and_a_refusal_reads_at_the_control', async () => {
  const r = rig();
  await r.open('d-B');
  let release;
  r.daemon.hold = new Promise((resolve) => { release = resolve; });
  r.type(settingId('name', 'field'), 'A2');
  const sent = r.submit('name');
  await r.calm();
  assert.equal(r.$(settingId('name', 'field')).disabled, true);
  assert.equal(r.words('name'), 'Saving…');
  assert.equal(r.$(settingId('name', 'field')).value, 'A2', 'the typed name is kept while it is saved');
  r.daemon.refuse = () => ({ status: 422, body: { error: 'the name A2 is not free', reason: 'taken' } });
  release();
  await sent;
  await r.calm();
  assert.equal(r.words('name'), 'the name A2 is not free', 'the daemon\'s own words for a name');
  const field = r.$(settingId('name', 'field'));
  assert.equal(field.getAttribute('aria-invalid'), 'true');
  assert.equal(field.value, 'A2', 'what was typed stays');
  assert.equal(r.at(), field.id, 'the focus is on the field');
  r.window.close();
});

await ok('every_refusal_reason_reads_its_words_at_its_control', async () => {
  const reasons = DEVICE_REFUSALS.filter((one) => !['no_device', 'failed'].includes(one));
  const status = { taken: 422, missing: 404, invalid: 422, read_only: 403, file_busy: 409,
    stopping: 503, signed_out: 401, forbidden: 403, no_answer: 0, busy: 409, not_waiting: 409,
    no_such_job: 404 };
  for (const reason of reasons) {
    const r = rig();
    await r.open('d-B');
    r.daemon.refuse = () => (reason === 'no_answer' ? { down: true } : {
      status: status[reason], body: { error: `daemon said ${reason}`, reason },
    });
    await r.click(settingId('default', 'make'));
    const said = r.words('default');
    assert.ok(said.length > 0 && !said.startsWith('daemon said'),
      `${reason}: the page's words at Make default, got "${said}"`);
    assert.equal(r.at(), settingId('default', 'make'), `${reason}: the focus is on the control`);
    r.window.close();
  }
  const r = rig();
  await r.open('d-B');
  r.daemon.refuse = () => ({ status: 500, body: { error: 'the daemon broke' } });
  await r.click(settingId('default', 'make'));
  assert.equal(r.words('default'), 'the daemon broke', 'any other: the daemon\'s words');
  r.window.close();
});

await ok('the_profile_is_picked_from_a_list_in_the_popup_and_no_pin_sends_null', async () => {
  const r = rig();
  await r.open('d-B');
  await r.click(settingId('profile', 'open'));
  const list = r.document.querySelector('#popup-devices .fi-list');
  assert.ok(list, 'the list is inside the popup');
  assert.deepEqual([...list.querySelectorAll('.fi-opt')].map((el) => el.textContent),
    ['Automatic', 'Lyr+', 'bifrost'], 'the bases are not offered; the pinned one stands');
  // Escape: the list's, the popup stays
  r.document.dispatchEvent(new r.window.KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  await r.calm();
  assert.equal(r.document.querySelector('#popup-devices .fi-list'), null);
  assert.equal(r.core.router.state().popup, 'devices', 'Escape closed the list only');
  await r.click(settingId('profile', 'open'));
  r.document.querySelector('#popup-devices .fi-opt[data-value="lyr-plus"]').click();
  await r.calm();
  assert.deepEqual(r.daemon.posts, [{ device: 'B', route: 'edit', body: { profile: 'lyr-plus' } }]);
  assert.match(r.$(settingId('profile')).textContent, /Lyr\+/);
  assert.equal(r.at(), settingId('profile', 'open'), 'the focus is on the opener again');
  await r.click(settingId('profile', 'open'));
  r.document.querySelector('#popup-devices .fi-opt[data-value=""]').click();
  await r.calm();
  assert.deepEqual(r.daemon.posts.at(-1).body, { profile: null });
  await r.click(settingId('profile', 'open'));
  r.document.querySelector('#popup-devices .fi-opt.on').click();
  await r.calm();
  assert.equal(r.daemon.posts.length, 2, 'the profile it already has sends nothing');
  r.window.close();
});

await ok('the_passphrase_form_sends_each_of_its_three_choices_and_the_code_in_the_body_only', async () => {
  const r = rig();
  await r.open('d-B');
  await r.click(settingId('passphrase', 'change'));
  assert.equal(r.at(), settingId('passphrase', 'kind-auto'));
  await r.choose('none');
  await r.submit('passphrase');
  assert.deepEqual(r.daemon.posts.at(-1), { device: 'B', route: 'edit', body: { passphrase: 'none' } });
  assert.match(r.$(settingId('passphrase')).textContent, /None/, 'the form closed on the new kind');
  assert.equal(r.at(), settingId('passphrase', 'change'));
  await r.click(settingId('passphrase', 'change'));
  await r.choose('auto');
  await r.submit('passphrase');
  assert.deepEqual(r.daemon.posts.at(-1).body, { passphrase: 'auto' });
  await r.click(settingId('passphrase', 'change'));
  await r.choose('code');
  r.type(settingId('passphrase', 'field'), SECRET);
  await r.click(settingId('passphrase', 'reveal'));
  assert.equal(r.$(settingId('passphrase', 'field')).type, 'text', 'the code is shown');
  assert.equal(r.$(settingId('passphrase', 'field')).value, SECRET, 'and typed text survives the put');
  await r.click(settingId('passphrase', 'reveal'));
  assert.equal(r.$(settingId('passphrase', 'field')).type, 'password');
  r.source().send('fleet', fleet(['A', 'B']));
  await r.calm();
  assert.equal(r.$(settingId('passphrase', 'field')).value, SECRET, 'and the stream\'s update');
  await r.submit('passphrase');
  assert.deepEqual(r.daemon.posts.at(-1), { device: 'B', route: 'edit', body: { passphrase: SECRET } });
  const page = r.document.documentElement.outerHTML + r.document.title + r.window.location.href
    + JSON.stringify(r.window.history.state);
  assert.ok(!page.includes(SECRET), 'the code is nowhere in the page, its title, address or history');
  r.window.close();
});

await ok('a_bad_code_or_a_refused_one_stays_in_the_form_with_its_words_and_the_focus', async () => {
  const r = rig();
  await r.open('d-B');
  await r.click(settingId('passphrase', 'change'));
  await r.choose('code');
  r.type(settingId('passphrase', 'field'), '12');
  await r.submit('passphrase');
  assert.deepEqual(r.daemon.posts, [], 'not four digits: the form is not sent (the browser says why)');
  assert.equal(r.at(), settingId('passphrase', 'field'));
  r.daemon.refuse = () => ({ status: 422, body: { error: `bad code ${SECRET}`, reason: 'invalid' } });
  r.type(settingId('passphrase', 'field'), SECRET);
  await r.submit('passphrase');
  assert.ok(!r.words('passphrase').includes(SECRET));
  assert.equal(r.$(settingId('passphrase', 'field')).value, SECRET, 'the typed code stays');
  assert.equal(r.at(), settingId('passphrase', 'field'));
  await r.click(settingId('passphrase', 'cancel'));
  assert.equal(r.$(settingId('passphrase', 'field')), null, 'Cancel closes the form');
  assert.equal(r.at(), settingId('passphrase', 'change'));
  r.window.close();
});

await ok('a_passphrase_change_on_the_device_that_is_playing_asks_once_and_keep_has_the_focus', async () => {
  const r = rig({ playing: true });
  await r.open('d-B');
  await r.click(settingId('passphrase', 'change'));
  await r.choose('none');
  await r.submit('passphrase');
  assert.deepEqual(r.daemon.posts, [], 'nothing is sent before the answer');
  const ask = r.$(ASK_ID);
  assert.match(ask.textContent, /^B is playing: /);
  assert.equal(r.document.activeElement.dataset.answer, 'keep', 'the safe answer has the focus');
  r.document.activeElement.click();
  await r.calm();
  assert.deepEqual(r.daemon.posts, []);
  assert.equal(ask.textContent, '', 'Keep closes it and sends nothing');
  await r.submit('passphrase');
  r.$(ASK_ID).querySelector('[data-answer="change"]').click();
  await r.calm();
  assert.deepEqual(r.daemon.posts, [{ device: 'B', route: 'edit', body: { passphrase: 'none' } }]);
  assert.equal(r.$(ASK_ID).textContent, '');
  r.window.close();
});

await ok('a_card_whose_sound_goes_elsewhere_is_no_device_that_is_playing', async () => {
  const r = rig({ playing: true, path: { verdict: 'not_this_dac', words: ['→ Gungnir 2'], title: 'the computer plays to Gungnir 2' } });
  await r.open('d-B');
  await r.click(settingId('passphrase', 'change'));
  await r.choose('none');
  await r.submit('passphrase');
  assert.deepEqual(r.daemon.posts, [{ device: 'B', route: 'edit', body: { passphrase: 'none' } }],
    'sent at once: the card plays to another sink');
  assert.equal(r.$(ASK_ID).textContent, '');
  r.window.close();
});

await ok('a_name_the_daemon_encodes_beyond_encodeURIComponent_keeps_the_page', async () => {
  const r = rig();
  await r.open('d-B');
  r.type(settingId('name', 'field'), 'Desk (left)');
  await r.submit('name');
  assert.deepEqual(r.daemon.posts, [{ device: 'B', route: 'edit', body: { name: 'Desk (left)' } }]);
  r.source().send('fleet', fleet(['A', 'Desk (left)']));
  await r.calm();
  assert.equal(r.core.router.state().page, 'd-Desk%20%28left%29', 'the page has the daemon\'s address');
  assert.equal(r.$(settingId('name', 'field')).value, 'Desk (left)');
  assert.ok(!r.document.querySelector('#popup-devices-body').textContent.includes('no longer configured'));
  r.window.close();
});

await ok('the_shown_devices_rename_moves_the_page_behind_when_the_fleet_comes_before_the_answer', async () => {
  const r = rig();
  await r.open('d-A');
  let release;
  r.daemon.hold = new Promise((resolve) => { release = resolve; });
  r.type(settingId('name', 'field'), 'Main');
  const sent = r.submit('name');
  await r.calm();
  r.daemon.rows[0].name = 'Main';
  r.source().send('fleet', fleet(['B', 'Main']));
  await r.calm();
  assert.equal(r.core.model.shown(), 'B', 'the fleet dropped A and the page went to the first device');
  release();
  await sent;
  await r.calm();
  assert.equal(r.core.model.shown(), 'Main', 'the answer then moves it to the renamed device');
  assert.equal(r.core.router.state().popup, 'devices');
  r.window.close();
});

await ok('a_stale_drop_does_not_wipe_a_rename_of_another_shown_device', async () => {
  const r = rig();
  await r.open('d-A');
  // A is removed elsewhere: the fleet drops it and the page goes to B
  r.source().send('fleet', fleet(['B', 'C']));
  await r.calm();
  assert.equal(r.core.model.shown(), 'B');
  // the person shows C and renames it; the fleet comes before the page's own read of Main
  await r.core.actions.showDevice('C', { push: false });
  r.core.router.step('d-C');
  await r.calm();
  let release;
  r.daemon.modelHold = new Promise((resolve) => { release = resolve; });
  r.type(settingId('name', 'field'), 'Main');
  await r.submit('name');
  r.source().send('fleet', fleet(['B', 'Main']));
  await r.calm();
  release();
  await r.calm();
  assert.equal(r.core.model.shown(), 'Main', 'the page behind ends on the renamed device, not the first');
  r.window.close();
});

await ok('a_device_shown_while_a_rename_is_in_flight_is_not_pulled_back', async () => {
  const r = rig();
  await r.open('d-A');
  let release;
  r.daemon.hold = new Promise((resolve) => { release = resolve; });
  r.type(settingId('name', 'field'), 'Main');
  const sent = r.submit('name');
  await r.calm();
  await r.core.actions.showDevice('B', { push: false });
  release();
  await sent;
  await r.calm();
  assert.equal(r.core.model.shown(), 'B', 'the person\'s own switch stands');
  r.window.close();
});

await ok('a_reload_that_has_not_listed_the_new_name_yet_makes_no_toast', async () => {
  const r = rig();
  await r.open('d-A');
  r.daemon.model404 = ['Main'];
  r.type(settingId('name', 'field'), 'Main');
  await r.submit('name');
  assert.equal(r.$('toasts').textContent, '', 'the follow is quiet when the daemon does not list it yet');
  assert.equal(r.core.router.state().page, 'd-Main');
  r.window.close();
});

await ok('make_default_names_the_pages_device_and_the_mark_moves', async () => {
  const r = rig();
  await r.open('d-B');
  await r.click(settingId('default', 'make'));
  assert.deepEqual(r.daemon.posts, [{ device: 'B', route: 'default', body: {} }]);
  assert.equal(r.$(settingId('default', 'make')), null);
  assert.match(r.$(settingId('default')).textContent, /Default/);
  assert.equal(r.at(), settingId('default', 'mark'), 'the Default mark has the focus');
  r.window.close();
});

await ok('the_shown_devices_rename_moves_the_page_behind_with_it', async () => {
  const r = rig();
  await r.open('d-A');
  r.type(settingId('name', 'field'), 'Main');
  await r.submit('name');
  assert.deepEqual(r.daemon.posts, [{ device: 'A', route: 'edit', body: { name: 'Main' } }]);
  assert.equal(r.core.router.state().page, 'd-Main', 'the popup\'s page follows');
  // the daemon's fleet now lists the new name and no longer the old one
  r.source().send('fleet', fleet(['B', 'Main']));
  await r.calm();
  assert.equal(r.core.model.shown(), 'Main', 'the page behind shows the renamed device, not the first');
  assert.equal(r.core.router.state().popup, 'devices');
  r.window.close();
});

await ok('what_is_typed_survives_another_setting_saved_and_a_profile_changed_elsewhere_reaches_the_page', async () => {
  const r = rig();
  await r.open('d-B');
  r.type(settingId('name', 'field'), 'Half typed');
  await r.click(settingId('default', 'make'));
  assert.equal(r.$(settingId('name', 'field')).value, 'Half typed', 'another setting\'s save keeps it');
  // the profile is changed in the TUI: the next read brings it, the typed name stays
  r.daemon.rows[1].profile = 'lyr-plus';
  r.ticks[0].fn();
  await r.calm();
  assert.match(r.$(settingId('profile')).textContent, /Lyr\+/, 'a change elsewhere reaches the page');
  assert.equal(r.$(settingId('name', 'field')).value, 'Half typed');
  assert.equal(r.ticks[0].ms, REREAD_MS);
  r.window.close();
});

await ok('a_code_typed_on_one_devices_page_is_not_carried_to_another_devices_page', async () => {
  const r = rig();
  await r.open('d-B');
  await r.click(settingId('passphrase', 'change'));
  await r.choose('code');
  r.type(settingId('passphrase', 'field'), SECRET);
  r.core.router.step('d-A');
  await r.calm();
  r.core.router.step('d-B');
  await r.calm();
  assert.equal(r.$(settingId('name', 'field')).value, 'B');
  assert.equal(r.$(settingId('passphrase', 'field'))?.value ?? '', '',
    'the code typed on B is not carried into another page (it is a new draw)');
  r.window.close();
});


const REMOVE = settingId('remove', 'open');
const answer = (r, kind) => r.$(REMOVE_ASK_ID).querySelector(`[data-answer="${kind}"]`);

await ok('remove_asks_once_naming_the_device_and_keep_sends_nothing_and_has_the_focus', async () => {
  const r = rig();
  await r.open('d-B');
  assert.equal(r.$(REMOVE).textContent, 'Remove…');
  await r.click(REMOVE);
  assert.deepEqual(r.daemon.posts, [], 'nothing is sent before the answer');
  assert.match(r.$(REMOVE_ASK_ID).textContent, /^Remove B\? svolder forgets its name/);
  assert.equal(r.document.activeElement.dataset.answer, 'keep', 'the safe answer has the focus');
  assert.ok(answer(r, 'remove').classList.contains('danger'));
  r.document.activeElement.click();
  await r.calm();
  assert.deepEqual(r.daemon.posts, []);
  assert.equal(r.$(REMOVE_ASK_ID).textContent, '', 'Keep closes it');
  assert.equal(r.at(), REMOVE, 'and the focus returns to Remove…');
  await r.click(REMOVE);
  r.core.router.step('d-C');
  await r.calm();
  assert.equal(r.$(REMOVE_ASK_ID).textContent, '', 'the question does not follow to another device');
  assert.equal(r.core.router.state().page, 'd-C');
  r.window.close();
});

await ok('a_removal_names_the_pages_device_and_ends_on_the_overview_with_a_line_that_has_the_focus', async () => {
  const r = rig();
  await r.open('d-B');
  await r.click(REMOVE);
  answer(r, 'remove').click();
  await r.calm();
  assert.deepEqual(r.daemon.posts, [{ device: 'B', route: 'remove', body: {} }]);
  assert.equal(r.core.model.shown(), 'A', 'the device on show is not the one removed');
  assert.equal(r.core.router.state().page, 'overview');
  assert.equal(r.$('popup-devices-notice').textContent, 'B was removed.');
  assert.equal(r.at(), 'popup-devices-notice', 'the line is read');
  assert.equal(r.$(settingId('name', 'field')), null, 'no settings on the Overview');
  assert.deepEqual([...r.document.querySelectorAll('[data-device]')].map((e) => e.dataset.device), ['A', 'C']);
  const entries = [...r.document.querySelectorAll('[data-page]')].map((e) => e.dataset.page);
  assert.ok(!entries.includes('d-B'), 'the list lost the entry');
  r.core.router.step('d-C');
  await r.calm();
  r.core.router.step('overview');
  await r.calm();
  assert.equal(r.$('popup-devices-notice'), null, 'the line goes with the page that showed it');
  r.window.close();
});

await ok('removing_a_device_that_is_playing_says_so_in_the_question', async () => {
  const r = rig({ playing: true });
  await r.open('d-B');
  await r.click(REMOVE);
  assert.match(r.$(REMOVE_ASK_ID).textContent, /^B is playing: removing it stops the sound/);
  assert.equal(r.document.activeElement.dataset.answer, 'keep');
  r.window.close();
});

await ok('every_refusal_of_a_removal_reads_at_its_control_and_the_question_stays', async () => {
  const reasons = {
    missing: [404, /no longer configured/], read_only: [409, /read-only/],
    file_busy: [409, /being written/], stopping: [503, /stopping/],
  };
  for (const [reason, [status, words]] of Object.entries(reasons)) {
    const r = rig();
    await r.open('d-B');
    r.daemon.refuse = () => ({ status, body: { error: 'daemon words', reason } });
    await r.click(REMOVE);
    answer(r, 'remove').click();
    await r.calm();
    assert.match(r.words('remove'), words, reason);
    assert.ok(r.$(REMOVE_ASK_ID).textContent.includes('Remove B?'), `${reason}: the question stays`);
    assert.equal(r.document.activeElement.dataset.answer, 'keep');
    assert.equal(r.core.router.state().page, 'd-B', 'the page is where it was');
    // trying again after the refusal works
    r.daemon.refuse = null;
    answer(r, 'remove').click();
    await r.calm();
    assert.equal(r.core.router.state().page, 'overview', reason);
    r.window.close();
  }
  for (const [status, words, body] of [[401, /signed out/, { error: 'no' }], [403, /turned off/, { error: 'no' }]]) {
    const r = rig();
    await r.open('d-B');
    r.daemon.refuse = () => ({ status, body });
    await r.click(REMOVE);
    answer(r, 'remove').click();
    await r.calm();
    assert.match(r.words('remove'), words, String(status));
    r.window.close();
  }
  const r = rig();
  await r.open('d-B');
  r.daemon.refuse = () => ({ down: true });
  await r.click(REMOVE);
  answer(r, 'remove').click();
  await r.calm();
  assert.match(r.words('remove'), /did not answer/);
  r.window.close();
});

await ok('while_a_removal_is_on_its_way_the_place_says_so_and_nothing_else_can_be_pressed', async () => {
  const r = rig();
  await r.open('d-B');
  let release;
  r.daemon.hold = new Promise((resolve) => { release = resolve; });
  await r.click(REMOVE);
  answer(r, 'remove').click();
  await r.calm();
  assert.equal(r.words('remove'), 'Removing…');
  assert.ok(r.$(REMOVE).disabled);
  assert.ok(answer(r, 'remove').disabled && answer(r, 'keep').disabled);
  release();
  await r.calm();
  assert.equal(r.core.router.state().page, 'overview');
  r.window.close();
});

await ok('the_shown_devices_removal_moves_the_page_behind_and_a_rename_after_it_ends_on_the_right_device', async () => {
  const r = rig();
  await r.open('d-A');
  await r.click(REMOVE);
  answer(r, 'remove').click();
  await r.calm();
  assert.deepEqual(r.daemon.posts, [{ device: 'A', route: 'remove', body: {} }]);
  r.source().send('fleet', fleet(['B', 'C']));
  await r.calm();
  assert.equal(r.core.model.shown(), 'B', 'the page behind moved to the first device left');
  assert.equal(r.core.router.state().popup, 'devices', 'the popup stands');
  assert.equal(r.core.router.state().page, 'overview');
  // B is on show: its page, a rename
  r.core.router.step('d-B');
  await r.calm();
  r.type(settingId('name', 'field'), 'Main');
  await r.submit('name');
  r.source().send('fleet', fleet(['C', 'Main']));
  await r.calm();
  assert.equal(r.core.model.shown(), 'Main', 'the page behind ends on the renamed device, not the first');
  assert.equal(r.core.router.state().page, 'd-Main');
  r.window.close();
});

await ok('the_last_device_removed_leaves_the_overview_with_the_line_and_none_configured', async () => {
  const r = rig();
  await r.open('d-C');
  r.daemon.rows.splice(0, 2);
  r.source().send('fleet', fleet(['C']));
  await r.calm();
  await r.click(REMOVE);
  answer(r, 'remove').click();
  await r.calm();
  r.source().send('fleet', fleet([]));
  await r.calm();
  assert.equal(r.core.router.state().page, 'overview');
  assert.equal(r.$('popup-devices-notice').textContent, 'C was removed.');
  assert.match(r.$('popup-devices-body').textContent, /No device is configured\./);
  assert.equal(r.core.model.page().card.title, 'No devices', 'the page behind shows the daemon\'s card for no device');
  assert.deepEqual(r.core.model.page().card.spans.map((one) => one.text).join(''), 'No devices yet. svolder setup adds one.');
  assert.equal(r.core.router.state().popup, 'devices', 'the popup stands');
  r.window.close();
});

/** B's page with the facts B's model holds (`over` laid over them), A on show. */
async function withFacts(over = {}, options = {}) {
  const r = rig(options);
  await r.open('d-B');
  r.source().send('device', { device: 'B', data: model('B', { facts: factsOf('B', over) }) });
  await r.calm();
  return r;
}
const fact = (r, label) => r.document.querySelector(`#${FACTS_ID} dd[data-fact="${label}"]`)?.textContent;

await ok('a_devices_page_draws_the_facts_of_that_device_and_the_overview_none', async () => {
  const r = await withFacts({ passphrase_note: 'The passphrase is not set.' });
  assert.equal(fact(r, 'Product'), 'Bifrost 3 (product 5)');
  assert.equal(fact(r, 'Unit'), 'unit-B', 'the page\'s device, not the one shown');
  assert.equal(fact(r, 'Firmware'), '7');
  assert.equal(fact(r, 'DSP'), '3');
  assert.equal(fact(r, 'Link'), 'connected');
  assert.equal(fact(r, 'Passphrase'), 'The passphrase is not set.');
  assert.match(fact(r, 'Signal'), /^−60 dBm live/);
  const block = r.$('popup-devices-device-42');
  const kids = [...block.children].map((el) => el.id || el.className);
  assert.deepEqual(kids.slice(0, 5), ['fpd-head', 'popup-devices-state-42', 'popup-devices-settings',
    'popup-devices-graphs-42', FACTS_ID], 'the facts stand under the graphs');
  r.core.router.step('overview');
  await r.calm();
  assert.equal(r.$(FACTS_ID), null, 'none on the Overview');
  r.core.router.step('d-A');
  await r.calm();
  assert.equal(r.$(FACTS_ID).textContent, '', 'a device whose model holds no facts yet draws none');
  r.window.close();
});

await ok('a_facts_part_that_arrives_is_put_in_its_place_and_the_unit_is_absent_for_a_guest', async () => {
  const r = await withFacts();
  r.source().send('part', { device: 'B', name: 'facts', data: factsOf('B', { firmware: '8' }) });
  await r.calm();
  assert.equal(fact(r, 'Firmware'), '8');
  r.source().send('part', { device: 'B', name: 'facts', data: factsOf('B', { firmware: '9', unit: undefined }) });
  await r.calm();
  assert.equal(fact(r, 'Unit'), undefined, 'no unit for a viewer who may not see it');
  assert.equal(fact(r, 'Firmware'), '9');
  r.window.close();
});

await ok('identify_names_the_pages_device_and_is_disabled_with_its_reason_on_one_that_is_away', async () => {
  const r = await withFacts();
  const button = r.$(IDENTIFY_ID);
  assert.ok(button && button.getAttribute('aria-disabled') !== 'true');
  await r.click(IDENTIFY_ID);
  assert.deepEqual(r.daemon.posts, [{ device: 'B', route: 'identify', body: {} }], 'B, and not A on show');
  // not connected: kept, disabled, with the reason, and it sends nothing
  r.source().send('part', { device: 'B', name: 'link', data: link(false) });
  await r.calm();
  assert.equal(r.$(IDENTIFY_ID).getAttribute('aria-disabled'), 'true');
  assert.match(r.$('popup-devices-identify-note').textContent, /not connected/);
  assert.equal(r.$(IDENTIFY_ID).getAttribute('aria-describedby'), 'popup-devices-identify-note');
  r.daemon.posts.length = 0;
  await r.click(IDENTIFY_ID);
  assert.deepEqual(r.daemon.posts, [], 'a disabled Identify sends nothing');
  // a device the profile offers no Identify for has none
  r.source().send('part', { device: 'B', name: 'facts', data: factsOf('B', { identify_offered: false }) });
  await r.calm();
  assert.equal(r.$(IDENTIFY_ID), null);
  r.window.close();
});

await ok('the_facts_are_read_afresh_when_a_page_opens_and_an_older_answer_is_not_taken', async () => {
  const r = rig();
  r.daemon.facts.B = factsOf('B', { firmware: '10', now_ms: NOW + 600000 });
  await r.open('d-B');
  assert.deepEqual(r.daemon.reads, [], 'nothing is read before the stream brings B\'s model');
  r.source().send('device', { device: 'B', data: model('B', { facts: factsOf('B') }) });
  await r.calm();
  assert.ok(r.daemon.reads.includes('B'), 'B\'s model was read');
  assert.equal(fact(r, 'Firmware'), '10', 'the part that was read is on the page');
  assert.equal(r.$(FACTS_ID).querySelector('section').dataset.now, String(NOW + 600000));
  // an answer older than what the store holds is dropped
  r.source().send('part', { device: 'B', name: 'facts', data: factsOf('B', { firmware: '11', now_ms: NOW + 900000 }) });
  await r.calm();
  r.daemon.facts.B = factsOf('B', { firmware: '10', now_ms: NOW + 600000 });
  r.core.router.step('d-C');
  await r.calm();
  r.core.router.step('d-B');
  await r.calm();
  assert.equal(fact(r, 'Firmware'), '11', 'the older read did not replace the newer part');
  r.window.close();
});

await ok('the_ages_are_kept_by_the_cores_ticker_with_the_selectors_of_the_page', async () => {
  const r = await withFacts();
  assert.ok(r.agers.length >= 1, 'one ticker of the ages');
  const signal = () => r.document.querySelector(`#${FACTS_ID} dd[data-signal]`).textContent;
  assert.equal(signal(), '−60 dBm live, 5 s ago'.replace(', 5 s ago', ''), 'a live reading under half a minute');
  const real = Date.now;
  try {
    Date.now = () => real() + 120000;
    for (const ager of r.agers) ager.fn();
    assert.equal(signal(), '−60 dBm live, 2 min ago', 'two minutes on, by the daemon\'s clock');
  } finally {
    Date.now = real;
  }
  r.window.close();
});


/** A person opens or closes the All readings disclosure: the state, then the event a browser sends. */
async function toggle(r, open) {
  const details = r.$(READINGS_ID);
  details.open = open;
  details.dispatchEvent(new r.window.Event('toggle'));
  await r.calm();
}
const readingRows = (r) => [...r.document.querySelectorAll(`#${READINGS_BODY_ID} tr[data-reading]`)]
  .map((tr) => [tr.dataset.reading, tr.querySelector('td').textContent]);

await ok('all_readings_is_closed_and_empty_until_opened_and_empty_again_once_closed', async () => {
  const r = await withFacts();
  const details = r.$(READINGS_ID);
  assert.ok(details && details.open === false, 'a closed disclosure under the facts');
  assert.equal(r.$(READINGS_BODY_ID).innerHTML, '', 'nothing is drawn while it is closed');
  assert.equal(r.document.querySelector('#popup-devices-device-42').lastElementChild, details);
  await toggle(r, true);
  assert.deepEqual(readingRows(r), [['1.1', '3 (Optical)']], 'the page\'s device\'s rows, in the daemon\'s words');
  assert.match(r.$(READINGS_BODY_ID).querySelector('caption').textContent, /^Readings of B$/);
  await toggle(r, false);
  assert.equal(r.$(READINGS_BODY_ID).innerHTML, '', 'closed, it is empty again');
  r.window.close();
});

await ok('an_open_disclosure_follows_the_facts_parts_and_is_never_drawn_again_and_a_page_for_another_device_starts_closed', async () => {
  const r = await withFacts();
  await toggle(r, true);
  const details = r.$(READINGS_ID);
  r.source().send('part', { device: 'B', name: 'facts', data: factsOf('B', {
    rows: [{ id: '1.1', label: 'Input', value: '2 (Coax)', heard_at_ms: NOW }, { id: '1.2', label: 'Rate', value: '96 kHz', heard_at_ms: null }],
  }) });
  await r.calm();
  assert.deepEqual(readingRows(r), [['1.1', '2 (Coax)'], ['1.2', '96 kHz']], 'a part reaches the open table');
  assert.equal(r.$(READINGS_ID), details, 'the details element stays');
  assert.equal(details.open, true, 'as the person left it');
  assert.equal(r.document.querySelector('td.fpd-reading-age:not([data-heard-at])').textContent, '—', 'a reading with no stamp');
  // the body is put again (a rename elsewhere changes the pane): it comes back open
  r.core.router.step('d-C');
  await r.calm();
  assert.equal(r.$(READINGS_ID).open, false, 'another device\'s page starts closed');
  assert.equal(r.$(READINGS_BODY_ID).innerHTML, '');
  r.window.close();
});

await ok('a_device_that_is_not_connected_shows_its_last_known_readings_as_such', async () => {
  const r = await withFacts();
  r.source().send('part', { device: 'B', name: 'link', data: link(false) });
  await r.calm();
  await toggle(r, true);
  assert.match(r.$(READINGS_BODY_ID).querySelector('caption').textContent, /^Last known readings of B \(not connected\)$/);
  assert.ok(r.$(READINGS_BODY_ID).querySelector('table[data-last-known]'));
  r.window.close();
});

await ok('the_readings_ages_are_kept_by_the_same_ticker_as_the_signals', async () => {
  const r = await withFacts();
  await toggle(r, true);
  const age = () => r.document.querySelector('td.fpd-reading-age[data-heard-at]').textContent;
  assert.equal(age(), '10 s ago');
  const real = Date.now;
  try {
    Date.now = () => real() + 180000;
    for (const ager of r.agers) ager.fn();
    assert.equal(age(), '3 min ago');
  } finally {
    Date.now = real;
  }
  assert.equal(r.agers.length, 1, 'one ticker for the page, not one for the facts and one for the readings');
  r.window.close();
});


await ok('the_shown_devices_removal_answered_before_the_fleet_never_reopens_the_stream_on_it', async () => {
  const r = rig();
  await r.open('d-A');
  await r.click(REMOVE);
  answer(r, 'remove').click();
  await r.calm();
  // no fleet event: the answer came first
  const named = FakeSource.all.map((one) => one.url);
  assert.ok(named.slice(2).every((url) => !/device=A(&|$)/.test(url)), `no stream is opened on the removed device: ${named}`);
  assert.equal(r.core.model.shown(), 'B', 'the page behind moved to the first device the answer lists');
  assert.ok(!/device=A(&|$)/.test(r.source().url), r.source().url);
  r.source().send('fleet', fleet(['B', 'C']));
  await r.calm();
  assert.equal(r.core.model.shown(), 'B');
  r.window.close();
});


await ok('another_devices_event_does_not_start_the_pages_ages_again', async () => {
  const r = await withFacts();
  r.source().send('device', { device: 'C', data: model('C', { facts: factsOf('C') }) });
  await r.calm();
  const age = () => r.document.querySelector(`#${FACTS_ID} dd[data-signal]`).textContent;
  const real = Date.now;
  try {
    Date.now = () => real() + 180000;
    for (const ager of r.agers) ager.fn();
    assert.equal(age(), '−60 dBm live, 3 min ago');
    r.source().send('part', { device: 'C', name: 'facts', data: factsOf('C', { firmware: '5' }) });
    r.source().send('part', { device: 'C', name: 'link', data: link(true) });
    await r.calm();
    for (const ager of r.agers) ager.fn();
    assert.equal(age(), '−60 dBm live, 3 min ago', 'the page\'s ages count from the readings, not from a put');
    r.source().send('part', { device: 'B', name: 'facts', data: factsOf('B', { firmware: '9', now_ms: NOW + 180000 }) });
    await r.calm();
    assert.equal(fact(r, 'Firmware'), '9', 'its own device\'s facts are still put');
  } finally {
    Date.now = real;
  }
  r.window.close();
});


await ok('a_late_toggle_of_a_disclosure_drawn_away_does_not_open_another_devices_readings', async () => {
  const r = await withFacts();
  const old = r.$(READINGS_ID);
  r.core.router.step('d-C');
  await r.calm();
  old.open = true;
  old.dispatchEvent(new r.window.Event('toggle'));
  await r.calm();
  assert.equal(r.$(READINGS_ID).open, false);
  assert.equal(r.$(READINGS_BODY_ID).innerHTML, '', 'C\'s readings are still closed and empty');
  r.window.close();
});


await ok('show_on_the_page_stands_on_the_names_line_and_the_shown_device_says_so_there', async () => {
  const r = rig();
  await r.open('d-B');
  const head = r.$('popup-devices-device-42').firstElementChild;
  assert.equal(head.className, 'fpd-head');
  assert.deepEqual([...head.children].map((el) => el.className), ['fpd-name', 'fpd-show']);
  assert.equal(r.$('popup-devices-show').textContent, 'Show on the page', 'the full label');
  r.core.router.step('d-A');
  await r.calm();
  assert.equal(r.document.querySelector('.fpd-head .fpd-show').textContent, 'This is the device the page shows.');
  r.window.close();
});


await ok('a_fleet_event_during_the_quiet_show_releases_the_hold_and_a_device_added_later_is_followed', async () => {
  const r = rig();
  await r.open('d-A');
  let release;
  r.daemon.modelHold = new Promise((resolve) => { release = resolve; });
  await r.click(REMOVE);
  answer(r, 'remove').click();
  await r.calm();
  r.source().send('fleet', fleet(['B', 'C']));
  await r.calm();
  release();
  await r.calm();
  assert.equal(r.core.model.shown(), 'B');
  r.daemon.rows.push({ unit: 'unit-D', name: 'D', passphrase: 'auto', connected: true });
  r.source().send('fleet', fleet(['B', 'C', 'D']));
  await r.calm();
  assert.match(r.source().url, /follow=B,C,D/, `the stream follows the device added later: ${r.source().url}`);
  r.window.close();
});

await ok('a_second_removal_during_the_quiet_show_keeps_the_hold', async () => {
  const r = rig();
  await r.open('d-A');
  let release;
  r.daemon.modelHold = new Promise((resolve) => { release = resolve; });
  await r.click(REMOVE);
  answer(r, 'remove').click();
  await r.calm();
  r.core.router.step('d-C');
  await r.calm();
  await r.click(REMOVE);
  answer(r, 'remove').click();
  await r.calm();
  const after = FakeSource.all.slice(2).map((one) => one.url);
  assert.ok(after.every((url) => !/device=A(&|$)/.test(url)), `the stream never goes back to the removed A: ${after}`);
  release();
  await r.calm();
  r.window.close();
});


await ok('a_redraw_of_the_same_page_does_not_start_the_ages_again', async () => {
  const r = await withFacts();
  const age = () => r.document.querySelector(`#${FACTS_ID} dd[data-signal]`).textContent;
  const real = Date.now;
  try {
    Date.now = () => real() + 180000;
    for (const ager of r.agers) ager.fn();
    assert.equal(age(), '−60 dBm live, 3 min ago');
    r.core.router.step('d-C');
    await r.calm();
    r.core.router.step('d-B');
    await r.calm();
    r.source().send('device', { device: 'B', data: model('B', { facts: factsOf('B') }) });
    await r.calm();
    for (const ager of r.agers) ager.fn();
    assert.equal(age(), '−60 dBm live, 3 min ago', 'the same stamp is aged from its first sight');
  } finally {
    Date.now = real;
  }
  r.window.close();
});

const CHANGED = 'config: /home/u/config.toml changed since the last reload: svolder daemon reload (or SIGHUP)';

await ok('a_writes_answer_clears_the_config_line_whichever_comes_first_it_or_the_fleet_events_read', async () => {
  for (const order of ['the read answers first', 'the write answers first']) {
    const r = rig();
    r.daemon.changed = CHANGED;
    await r.open('overview');
    assert.equal(r.$('popup-devices-config').textContent, CHANGED);
    r.core.router.step('d-B');
    await r.calm();
    let release;
    let free;
    if (order === 'the read answers first') {
      // the write waits; the stream's fleet event makes the page read, and that read answers
      // while the write is still out: the line stands, then the write's answer clears it
      r.daemon.hold = new Promise((resolve) => { release = resolve; });
      r.type(settingId('name', 'field'), 'Desk amp');
      const sent = r.submit('name');
      await r.calm();
      r.source().send('fleet', fleet(['A', 'B', 'C']));
      await r.calm();
      release();
      await sent;
      await r.calm();
    } else {
      // the page's read is asked first and answers last, with what stood when it was asked
      r.daemon.listHold = new Promise((resolve) => { free = resolve; });
      r.source().send('fleet', fleet(['A', 'B', 'C']));
      await r.calm();
      r.type(settingId('name', 'field'), 'Desk amp');
      await r.submit('name');
      await r.calm();
      free();
      await r.calm();
    }
    r.core.router.step('overview');
    await r.calm();
    assert.equal(r.$('popup-devices-config').textContent, '', `${order}: the daemon's write is no change`);
    r.window.close();
  }
});

await ok('a_popup_that_closes_forgets_the_line_and_the_next_opening_reads_it_afresh', async () => {
  const r = rig();
  r.daemon.changed = CHANGED;
  await r.open('overview');
  assert.equal(r.$('popup-devices-config').textContent, CHANGED);
  await new Promise((resolve) => {
    r.window.addEventListener('popstate', () => r.window.setTimeout(resolve, 0), { once: true });
    r.core.router.close();
  });
  await r.calm();
  assert.equal(r.$('popup-devices-config'), null, 'the popup is closed');
  r.daemon.changed = null;
  await r.open('overview');
  assert.equal(r.$('popup-devices-config').textContent, '');
  r.window.close();
});

// ---- the read-only popup: a guest, and a signed-in viewer of a page with writes off ----

const GUEST = { signed_in: false, guest: true, writes: false, read_only: null };
const WRITES_OFF = { signed_in: true, guest: false, writes: false, read_only: 'Read-only here' };
const READERS = [['a guest', GUEST], ['a page with writes off', WRITES_OFF]];
/** What a reader's page must never hold: a setting, Remove…, Identify, the config line, the add entry. */
const writes = (r) => [
  ...r.document.querySelectorAll('#popup-devices [id^="popup-devices-set-"], #popup-devices-settings'),
  ...[IDENTIFY_ID, 'popup-devices-config'].map((id) => r.$(id)).filter(Boolean),
  ...r.document.querySelectorAll('#popup-devices [data-page="add"], #popup-devices [data-popup-page="add"]'),
];
/** Every request that is a write or the writer's list. */
const forbidden = (r) => r.daemon.asked.filter((one) => !one.startsWith('GET ') || one === 'GET api/v1/devices');
const readerFacts = (name, viewer) => factsOf(name, viewer.guest ? { unit: '' } : {});

await ok('a_reader_opens_the_popup_and_every_gesture_makes_no_post_and_no_read_of_the_list', async () => {
  for (const [who, viewer] of READERS) {
    const r = rig({ viewer });
    r.daemon.facts = { A: readerFacts('A', viewer), B: readerFacts('B', viewer), C: readerFacts('C', viewer) };
    await r.open();
    const side = () => [...r.document.querySelectorAll('#popup-devices .fp-entry')].map((el) => el.dataset.popupPage);
    assert.deepEqual(side(), ['overview', 'd-A', 'd-B', 'd-C'], `${who}: the fleet's list and no add entry`);
    assert.equal(r.document.querySelectorAll('#popup-devices .fpd-device').length, 3, who);
    assert.deepEqual(writes(r), [], who);
    r.core.router.step('d-B');
    await r.calm();
    r.source().send('device', { device: 'B', data: model('B', { facts: readerFacts('B', viewer) }) });
    await r.calm();
    assert.equal(r.document.querySelector('#popup-devices .fpd-name').textContent, 'B', who);
    assert.ok(fact(r, 'Product'), `${who}: the facts are drawn`);
    assert.equal(fact(r, 'Unit'), viewer.guest ? undefined : 'unit-B', `${who}: the Unit is the daemon's`);
    assert.deepEqual(writes(r), [], who);
    assert.ok(r.$('popup-devices-show').querySelector('button'), `${who}: Show on the page stays`);
    // All readings opens
    const details = r.$(READINGS_ID);
    details.open = true;
    details.dispatchEvent(new r.window.Event('toggle'));
    await r.calm();
    assert.ok(r.$(READINGS_BODY_ID).textContent.includes('Input'), `${who}: the readings table`);
    // Show on the page: navigation only
    await r.click('popup-devices-show-button');
    assert.equal(r.core.model.shown(), 'B');
    // a rename and a drop come from the stream
    r.source().send('fleet', { devices: [
      { url: 'A', name: 'A2', link: { word: 'connected', state: 'on' } },
      { url: 'B', name: 'B', link: { word: 'connected', state: 'on' } },
    ], hops: [] });
    await r.calm();
    assert.deepEqual(side(), ['overview', 'd-A', 'd-B'].map((x) => x), `${who}: the drop follows the stream`);
    r.core.router.step('overview');
    await r.calm();
    r.core.router.step(null);
    await r.calm();
    assert.deepEqual(forbidden(r), [], `${who}: no write and no read of the configured list`);
    assert.deepEqual(r.keyListeners, [], `${who}: no key listener`);
    r.window.close();
  }
});

await ok('a_reader_reads_the_facts_whichever_comes_first_the_model_or_the_fleet_event', async () => {
  for (const order of ['the model answers first', 'the fleet event comes first']) {
    const r = rig({ viewer: WRITES_OFF });
    r.daemon.facts = { B: readerFacts('B', WRITES_OFF) };
    let release;
    r.daemon.modelHold = new Promise((resolve) => { release = resolve; });
    await r.open('d-B');
    r.source().send('device', { device: 'B', data: model('B', { facts: readerFacts('B', WRITES_OFF) }) });
    const two = { devices: [{ url: 'A', name: 'A', link: { word: 'connected', state: 'on' } },
      { url: 'B', name: 'B', link: { word: 'connected', state: 'on' } }], hops: [] };
    if (order === 'the fleet event comes first') {
      r.source().send('fleet', two);
      await r.calm();
      release();
    } else {
      release();
      await r.calm();
      r.source().send('fleet', two);
    }
    await r.calm();
    assert.equal(fact(r, 'Unit'), 'unit-B', order);
    assert.deepEqual([...r.document.querySelectorAll('#popup-devices .fp-entry')].map((el) => el.dataset.popupPage),
      ['overview', 'd-A', 'd-B'], order);
    assert.deepEqual(forbidden(r), [], order);
    r.window.close();
  }
});

await ok('a_device_that_left_the_fleet_is_said_gone_once_and_a_reader_never_asks_for_it_again', async () => {
  const r = rig({ viewer: GUEST });
  r.daemon.facts = { B: readerFacts('B', GUEST) };
  await r.open('d-B');
  r.source().send('fleet', { devices: [{ url: 'A', name: 'A', link: { word: 'connected', state: 'on' } }], hops: [] });
  await r.calm();
  assert.ok(r.$('popup-devices').textContent.includes('This device is no longer configured.'));
  const before = r.daemon.asked.length;
  r.daemon.model404 = ['B'];
  for (let i = 0; i < 4; i++) {
    for (const tick of r.ticks) tick.fn();
    await r.calm();
  }
  assert.deepEqual(r.daemon.asked.slice(before).filter((one) => one.includes('/B/')), [],
    'no retry loop on the gone device');
  assert.equal(r.$('popup-devices').textContent.split('no longer configured').length, 2, 'said once');
  assert.deepEqual(forbidden(r), []);
  r.window.close();
});

await ok('a_readers_open_device_page_follows_a_renamed_device_by_its_fleet_key', async () => {
  const r = rig({ viewer: WRITES_OFF });
  const row = (name, key) => ({ url: name, name, key, link: { word: 'connected', state: 'on' } });
  r.source().send('fleet', { devices: [row('A', 'ka'), row('B', 'kb')], hops: [] });
  await r.calm();
  await r.open('d-B');
  assert.equal(r.core.router.state().page, 'd-B');
  // B is renamed to Bee: the fleet lists the same key under the new address
  r.source().send('fleet', { devices: [row('A', 'ka'), row('Bee', 'kb')], hops: [] });
  await r.calm();
  assert.equal(r.core.router.state().page, 'd-Bee', 'the reader\'s page follows the key');
  assert.ok(!r.$('popup-devices').textContent.includes('no longer configured'));
  // a device that is removed (no row under its key) is still said gone
  r.source().send('fleet', { devices: [row('A', 'ka')], hops: [] });
  await r.calm();
  assert.ok(r.$('popup-devices').textContent.includes('no longer configured'));
  r.window.close();
});

await ok('a_reader_page_put_never_starts_the_ages_again', async () => {
  const r = rig({ viewer: WRITES_OFF });
  await r.open('d-B');
  r.source().send('device', { device: 'B', data: model('B', { facts: readerFacts('B', WRITES_OFF) }) });
  await r.calm();
  const age = () => r.document.querySelector(`#${FACTS_ID} dd[data-signal]`).textContent;
  const real = Date.now;
  try {
    Date.now = () => real() + 180000;
    for (const ager of r.agers) ager.fn();
    assert.equal(age(), '−60 dBm live, 3 min ago');
    r.source().send('fleet', { devices: [{ url: 'A', name: 'A', link: { word: 'connected', state: 'on' } },
      { url: 'B', name: 'B', link: { word: 'connected', state: 'on' } }], hops: [] });
    await r.calm();
    r.source().send('device', { device: 'A', data: model('A') });
    await r.calm();
    for (const ager of r.agers) ager.fn();
    assert.equal(age(), '−60 dBm live, 3 min ago', 'another device\'s event leaves the age alone');
  } finally {
    Date.now = real;
  }
  r.window.close();
});

await ok('a_viewer_who_changes_under_the_popup_draws_it_again_in_the_new_mode_and_drops_what_was_typed', async () => {
  const r = rig();
  r.daemon.facts = { B: factsOf('B') };
  await r.open('d-B');
  r.source().send('device', { device: 'B', data: model('B', { facts: factsOf('B') }) });
  await r.calm();
  assert.ok(r.$(settingId('name', 'field')), 'a writer has the settings');
  r.type(settingId('name', 'field'), 'Typed');
  r.core.model.replace({ ...r.core.model.page(), viewer: WRITES_OFF });
  await r.calm();
  assert.ok(r.$('popup-devices'), 'the popup stands');
  assert.deepEqual(writes(r), [], 'no setting stays on a guest\'s screen');
  assert.equal(r.document.querySelectorAll('#popup-devices input').length, 0, 'and nothing typed');
  assert.equal(r.document.querySelector('#popup-devices .fp-entry[data-popup-page="add"]'), null);
  assert.equal(fact(r, 'Unit'), 'unit-B');
  const asked = r.daemon.asked.length;
  for (const tick of r.ticks) tick.fn();
  r.source().send('fleet', { devices: [{ url: 'A', name: 'A', link: { word: 'connected', state: 'on' } },
    { url: 'B', name: 'B', link: { word: 'connected', state: 'on' } }], hops: [] });
  await r.calm();
  assert.deepEqual(r.daemon.asked.slice(asked).filter((one) => !one.startsWith('GET ') || one === 'GET api/v1/devices'), [],
    'after the change nothing writes and the list is not asked');
  // and back to a writer: the settings, the add entry and the config line return, the list is read once
  const lists = () => r.daemon.asked.filter((one) => one === 'GET api/v1/devices').length;
  const before = lists();
  r.core.model.replace({ ...r.core.model.page(), viewer: { signed_in: true, guest: false, writes: true, read_only: null } });
  await r.calm();
  assert.ok(r.$(settingId('name', 'field')), 'the settings are back');
  assert.ok(r.document.querySelector('#popup-devices .fp-entry[data-popup-page="add"]'), 'and the add entry');
  assert.equal(lists() - before, 1, 'the configured list is read once');
  r.window.close();
});

await ok('a_write_answered_after_the_viewer_lost_the_right_leaves_the_reader_the_fleets_list', async () => {
  const r = rig();
  await r.open('d-B');
  let release;
  r.daemon.hold = new Promise((resolve) => { release = resolve; });
  r.$(settingId('default', 'make')).click();
  await r.calm();
  r.core.model.replace({ ...r.core.model.page(), viewer: WRITES_OFF });
  await r.calm();
  release();
  await r.calm();
  r.source().send('fleet', { devices: ['A', 'B', 'D'].map((name) => ({
    url: name, name, link: { word: 'connected', state: 'on' } })), hops: [] });
  await r.calm();
  assert.deepEqual([...r.document.querySelectorAll('#popup-devices .fp-entry')].map((el) => el.dataset.popupPage),
    ['overview', 'd-A', 'd-B', 'd-D'], 'the reader\'s list follows the stream, not the write\'s answer');
  assert.deepEqual(writes(r), []);
  r.window.close();
});

await ok('a_flip_to_read_only_on_the_add_page_says_nothing_and_the_router_is_on_the_overview', async () => {
  const r = rig();
  await r.open('add');
  r.core.model.replace({ ...r.core.model.page(), viewer: WRITES_OFF });
  await r.calm();
  assert.equal(r.$('toasts').textContent, '', 'no read-only toast from a cancel the viewer cannot make');
  assert.equal(r.core.router.state().page, 'overview', 'the router names the page that is drawn');
  assert.deepEqual(writes(r), []);
  assert.equal(r.document.querySelectorAll('#popup-devices .fpd-device').length, 3, 'the Overview stands');
  r.window.close();
});

await ok('a_reader_whose_address_names_the_add_page_has_the_overview', async () => {
  const r = rig({ viewer: GUEST });
  await r.open('add');
  assert.deepEqual(writes(r), []);
  assert.equal(r.document.querySelectorAll('#popup-devices .fpd-device').length, 3, 'the Overview stands');
  assert.deepEqual(forbidden(r), []);
  r.window.close();
});

console.log(`core-device-wire: ${n} cases passed`);
