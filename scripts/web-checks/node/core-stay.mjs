/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for a popup that stays through a change of the shown device (`stays: true` in
// popups/listed.js), on the real core over a jsdom page (dom.mjs): the store, the stack, the
// router, the actions and the stream together, since the switch crosses all five. A staying popup
// stands through `showDevice`, with and without a push: its page and its focus are kept, no entry
// is pushed, the entry the page is on names the new device and its address, and Back still steps
// page → list → closed without showing the device before and without opening a closed popup. A
// popup that does not stay closes as before, and so does a question asked over a staying popup.
// The two other ways a switch reaches a popup: the fleet drops the shown device (it was removed,
// or renamed to another address) and the page moves to the first device left.
// Run: node core-stay.mjs
import assert from 'node:assert/strict';
import { page } from './dom.mjs';
import { createCore } from '../../../bins/svolderd/web/core/core.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
console.warn = () => {};

const BODY = '<div id="frame"><button id="fx-button">Menu</button></div>'
  + '<div id="theme"><button id="knob">Volume</button></div><div id="toasts"></div>';
const device = (name) => ({
  name, url: name, volume: { raw: 40, lo: 0, hi: 100 }, controls: [], meters: null,
});
const fleet = (...names) => ({ devices: names.map((name) => ({ url: name, name })), hops: [] });
const MODEL = {
  v: 1, viewer: { guest: false, writes: true }, fleet: fleet('A', 'B', 'C'), shown: 'A',
  device: device('A'), followed: {}, card: null, host: null, strip: null,
};

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

/** A popup's wire that notes what it is told and draws one button per page. */
const part = (told) => ({
  wire: ({ frame }) => ({
    show(at) {
      told.push(`show ${at}`);
      frame.draw({ page: at, body: `<button id="in-${at ?? 'list'}">Act</button>` });
    },
    hide() { told.push('hide'); },
    onKey: () => false,
  }),
});

/** The real core on device A with two popups: `stay` stays through a device switch, `go` does
 * not. */
function rig() {
  const p = page(BODY, { url: 'http://svolder.test/?device=A' });
  globalThis.window = p.window;
  globalThis.document = p.document;
  globalThis.AbortController = p.window.AbortController;
  Object.defineProperty(p.document, 'visibilityState', { value: 'visible' });
  FakeSource.all = [];
  p.window.EventSource = FakeSource;
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  const reads = [];
  p.window.fetch = async (path) => {
    reads.push(path);
    const url = /^api\/v1\/devices\/([^/]+)\/model/.exec(path)?.[1];
    return { status: 200, ok: true, json: async () => (url ? { device: device(url) } : {}) };
  };
  const told = { stay: [], go: [] };
  const entries = [{ id: 'one', label: 'One' }, { id: 'two', label: 'Two' }];
  const core = createCore({
    page: structuredClone(MODEL), themes: [], window: p.window,
    popups: [
      {
        name: 'stay', title: 'Stay', entries, stays: true, load: async () => part(told.stay),
        row: { label: 'Stay…', sees: 'everyone' },
      },
      {
        name: 'go', title: 'Go', entries, load: async () => part(told.go),
        row: { label: 'Go…', sees: 'everyone' },
      },
    ],
  });
  core.stream.open();
  const $ = (id) => p.document.getElementById(id);
  const landed = (act) => new Promise((resolve) => {
    p.window.addEventListener('popstate', () => p.window.setTimeout(resolve, 0), { once: true });
    act();
  });
  const open = async (name, to = null) => {
    $('knob').focus();
    core.router.open(name, to);
    await p.settle();
    await p.settle();
  };
  return {
    ...p, core, told, reads, $, landed, open,
    back: () => landed(() => p.window.history.back()),
    forward: () => landed(() => p.window.history.forward()),
    source: () => FakeSource.all.at(-1),
    at: () => p.document.activeElement?.id ?? null,
    length: () => p.window.history.length,
  };
}

await ok('a_staying_popup_stands_through_a_switch_and_no_entry_is_pushed', async () => {
  for (const push of [false, true]) {
    const r = rig();
    await r.open('stay');
    r.core.router.step('two');
    await r.settle();
    r.$('in-two').focus();
    const before = r.length();
    const told = r.told.stay.length;
    assert.equal((await r.core.actions.showDevice('B', { push })).ok, true);
    await r.settle();
    assert.equal(r.core.model.shown(), 'B', 'the page behind shows the device');
    assert.equal(r.$('popup-stay').isConnected, true, `the popup stands (push: ${push})`);
    assert.deepEqual(r.core.router.state(), { device: 'B', popup: 'stay', page: 'two' });
    assert.equal(r.length(), before, 'no entry is pushed');
    assert.equal(r.window.location.search, '?device=B', 'the address names the device shown');
    assert.equal(r.window.history.state.device, 'B');
    assert.equal(r.window.history.state.popup, 'stay');
    assert.equal(r.at(), 'in-two', 'the focus stays where it was');
    assert.equal(r.told.stay.length, told, 'the popup is neither hidden nor shown again');
    r.window.close();
  }
});

await ok('back_steps_page_list_closed_on_the_new_device_and_forward_opens_nothing_closed', async () => {
  const r = rig();
  await r.open('stay');
  r.core.router.step('two');
  await r.settle();
  await r.core.actions.showDevice('B', { push: false });
  await r.settle();
  r.reads.length = 0;
  await r.back();
  assert.deepEqual(r.core.router.state(), { device: 'B', popup: 'stay', page: null }, 'the list');
  assert.equal(r.core.model.shown(), 'B', 'Back never shows the device before');
  assert.equal(r.window.location.search, '?device=B');
  await r.back();
  await r.settle();
  assert.deepEqual(r.core.router.state(), { device: 'B', popup: null, page: null }, 'closed');
  assert.equal(r.$('popup-stay'), null);
  assert.equal(r.core.model.shown(), 'B');
  assert.equal(r.window.location.search, '?device=B', 'the entry it was opened from names B too');
  assert.deepEqual(r.reads, [], 'no device was read on the way');
  // Forward after Back shows the popup again, still on B
  await r.forward();
  await r.settle();
  assert.deepEqual(r.core.router.state(), { device: 'B', popup: 'stay', page: null });
  assert.equal(r.core.model.shown(), 'B');
  // closed with Escape: Forward opens nothing
  await r.landed(() => r.key('Escape'));
  assert.equal(r.$('popup-stay'), null);
  assert.deepEqual(r.core.router.state(), { device: 'B', popup: null, page: null });
  await r.forward();
  await r.settle();
  assert.equal(r.$('popup-stay'), null, 'a closed popup is never opened by the history');
  assert.equal(r.core.model.shown(), 'B');
  assert.deepEqual(r.reads, []);
  r.window.close();
});

await ok('the_close_button_after_a_switch_lands_on_the_new_device', async () => {
  const r = rig();
  await r.open('stay', 'two');
  await r.core.actions.showDevice('C', { push: false });
  await r.settle();
  await r.landed(() => r.core.router.close());
  assert.deepEqual(r.core.router.state(), { device: 'C', popup: null, page: null });
  assert.equal(r.core.model.shown(), 'C');
  assert.equal(r.window.location.search, '?device=C');
  assert.equal(r.window.history.state.device, 'C');
  // the device before is still a step back
  r.core.actions.showDevice('B');
  await r.settle();
  await r.settle();
  assert.equal(r.window.location.search, '?device=B');
  await r.back();
  await r.settle();
  await r.settle();
  assert.equal(r.core.model.shown(), 'C');
  r.window.close();
});

await ok('a_popup_that_does_not_stay_closes_as_before', async () => {
  const r = rig();
  await r.open('go', 'two');
  const before = r.length();
  await r.landed(() => { r.core.actions.showDevice('B'); });
  await r.settle();
  await r.settle();
  assert.equal(r.$('popup-go'), null, 'another device on show closes it');
  assert.equal(r.core.model.shown(), 'B');
  assert.deepEqual(r.core.router.state(), { device: 'B', popup: null, page: null });
  assert.equal(r.window.location.search, '?device=B');
  assert.equal(r.length(), before, "the device's entry takes the place of the popup's");
  assert.equal(r.window.history.state.popup, null);
  assert.deepEqual(r.told.go.slice(-1), ['hide']);
  r.window.close();
});

await ok('a_question_over_a_staying_popup_is_answered_no_and_the_popup_stands', async () => {
  const r = rig();
  await r.open('stay');
  const asked = r.core.ask('Remove B?', { yes: 'Remove', no: 'Keep' });
  await r.settle();
  assert.equal(r.core.ask.opened(), true);
  await r.core.actions.showDevice('B', { push: false });
  assert.equal(await asked, false, 'a question about one device is not left over another');
  assert.equal(r.core.ask.opened(), false);
  assert.equal(r.$('popup-stay').isConnected, true);
  assert.equal(r.$('popup-stay').inert, false, 'and is the top again');
  r.window.close();
});

await ok('the_shown_device_removed_moves_the_page_and_the_popup_stands', async () => {
  const r = rig();
  await r.open('stay');
  r.core.router.step('two');
  await r.settle();
  const before = r.length();
  r.source().send('fleet', fleet('B', 'C'));
  await r.settle();
  await r.settle();
  assert.equal(r.core.model.shown(), 'B', 'the page moves to the first device left');
  assert.equal(r.$('popup-stay').isConnected, true, 'the popup stands');
  assert.deepEqual(r.core.router.state(), { device: 'B', popup: 'stay', page: 'two' });
  assert.equal(r.length(), before);
  assert.equal(r.window.location.search, '?device=B', 'the address no longer names the one removed');
  await r.back();
  await r.back();
  await r.settle();
  assert.equal(r.core.model.shown(), 'B', 'Back does not go looking for the removed device');
  assert.deepEqual(r.reads.filter((path) => path.includes('/A/')), []);
  r.window.close();
});

await ok('a_rename_that_changes_the_address_moves_the_page_and_the_popup_stands', async () => {
  const r = rig();
  await r.open('stay');
  // A is now called A2: the fleet lists it under its new address, first as before
  r.source().send('fleet', fleet('A2', 'B', 'C'));
  await r.settle();
  await r.settle();
  assert.equal(r.core.model.shown(), 'A2');
  assert.equal(r.$('popup-stay').isConnected, true);
  assert.deepEqual(r.core.router.state(), { device: 'A2', popup: 'stay', page: null });
  assert.equal(r.window.location.search, '?device=A2');
  r.window.close();
});

await ok('with_no_popup_a_device_shown_without_a_push_still_names_itself_in_the_entry', async () => {
  const r = rig();
  const before = r.length();
  r.source().send('fleet', fleet('B', 'C'));
  await r.settle();
  await r.settle();
  assert.equal(r.core.model.shown(), 'B');
  assert.deepEqual(r.core.router.state(), { device: 'B', popup: null, page: null });
  assert.equal(r.window.location.search, '?device=B');
  assert.equal(r.length(), before);
  r.window.close();
});

console.log(`core-stay: ${n} cases passed`);
