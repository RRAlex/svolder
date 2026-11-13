/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the core's router (core/router.js) with the core's stack of modals
// (core/modal.js): the history state `{ device, popup, page }` and a popup the router puts on the
// stack. Back steps page → the popup's list → closed and never shows a device when only the popup
// part changed; a step past the popup shows a device only when the entry names another; an entry
// without a device keeps the one shown; Escape, the stack's `closeAll` and the close button step
// back past the popup, and Forward after them opens nothing, while Forward after Back shows the
// popup again; a popup closed any way can be opened again; a push asked for while such a step is
// on its way waits for it; after the page is loaded again no entry of the load before opens a
// popup. The history is jsdom's (dom.mjs): its steps land later, as a browser's do, but a second
// step asked for before the first has landed takes its place there (a browser takes both), so two
// of the router's own steps in a row are proved in the browser (modal.py).
// Run: node core-router.mjs
import assert from 'node:assert/strict';
import { createModals } from '../../../bins/svolderd/web/core/modal.js';
import { askForm, closeAsk, openAsks } from '../../../bins/svolderd/web/core/inline-ask.js';
import { closeList, openList, openLists } from '../../../bins/svolderd/web/core/list.js';
import { createRouter } from '../../../bins/svolderd/web/core/router.js';
import { page } from './dom.mjs';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const BODY = '<div id="frame"><button id="fx-button">Menu</button></div>'
  + '<div id="theme"><button id="knob">Volume</button></div><div id="toasts"></div>';

/**
 * One load of the page `p` on device `device`: the stack, the router and a popup `devices` that
 * notes what it is told. `unload()` ends it as a reload does: its popup leaves the page and it
 * hears no more of the history, which stays.
 */
function load(p, {
  device = 'A', slow = false, activation = undefined, stays = false,
} = {}) {
  const { window, document } = p;
  let live = true;
  const view = {
    history: window.history,
    location: window.location,
    sessionStorage: window.sessionStorage,
    // what the browser says of the person's last tap or key; jsdom has none
    navigator: activation === undefined ? {} : { userActivation: activation },
    addEventListener: (kind, fn) => window.addEventListener(kind, (event) => { if (live) fn(event); }),
  };
  const modal = createModals({ document, fallback: () => document.getElementById('fx-button') });
  const keys = (event) => { if (live) modal.key(event); };
  window.addEventListener('keydown', keys, { capture: true });
  /** The devices `show` was asked for, and the one on show. */
  const shows = [];
  let shown = device;
  /** Lets a slow `show` finish. */
  let finish = () => {};
  const router = createRouter({
    window: view,
    modal,
    device,
    show: async (url) => {
      shows.push(url);
      if (slow) await new Promise((resolve) => { finish = resolve; });
      shown = url;
    },
  });
  const el = document.createElement('div');
  el.id = 'devices';
  el.innerHTML = '<h2 id="devices-title">Devices</h2><button id="devices-x">Close</button>';
  const popup = {
    page: null,
    told: [],
    keys: [],
    get open() { return el.isConnected; },
  };
  router.register('devices', {
    el,
    labelledBy: 'devices-title',
    stays,
    onKey(event) { popup.keys.push(event.key); return false; },
    show(at) { popup.page = at; popup.told.push(`show ${at}`); },
    hide() { popup.told.push('hide'); },
  });
  /** What a theme does to show a device: the core's action pushes through the router. */
  const show = (url) => {
    shown = url;
    router.pushState({ device: url }, '', `./?device=${url}`);
  };
  return {
    router, modal, popup, shows, el,
    device: show,
    shown: () => shown,
    finish: () => finish(),
    unload() {
      live = false;
      el.remove();
      for (const child of document.body.children) child.inert = false;
    },
  };
}

/** A page on device A, loaded once. */
function rig(options = {}) {
  const p = page(BODY, { url: 'http://svolder.test/?device=A' });
  const { window } = p;
  /** Take a step of the history and wait until it has landed and been acted on. */
  const go = (by) => new Promise((resolve) => {
    window.addEventListener('popstate', () => window.setTimeout(resolve, 0), { once: true });
    window.history.go(by);
  });
  /** Wait until `count` steps of the router's own have landed. */
  const landed = (count = 1) => new Promise((resolve) => {
    let left = count;
    const one = () => {
      left -= 1;
      if (left > 0) return;
      window.removeEventListener('popstate', one);
      window.setTimeout(resolve, 0);
    };
    window.addEventListener('popstate', one);
  });
  return {
    ...p, ...load(p, options), go, landed,
    again: (more = {}) => load(p, more),
    entry: () => window.history.state,
    back: () => go(-1),
    forward: () => go(1),
  };
}

await ok('the_entry_the_page_opened_on_knows_its_device_and_a_device_shown_pushes_one', async () => {
  const r = rig();
  assert.equal(r.entry().device, 'A');
  assert.equal(r.entry().popup, null);
  assert.deepEqual(r.router.state(), { device: 'A', popup: null, page: null });
  const before = r.window.history.length;
  r.device('B');
  assert.equal(r.window.history.length, before + 1);
  assert.equal(r.window.location.search, '?device=B');
  assert.deepEqual(r.router.state(), { device: 'B', popup: null, page: null });
  await r.back();
  assert.deepEqual(r.shows, ['A'], 'a step back shows the device the entry names');
  await r.forward();
  assert.deepEqual(r.shows, ['A', 'B']);
  assert.deepEqual(r.popup.told, [], 'no popup was ever told anything');
});

await ok('back_from_a_popup_page_never_calls_showdevice', async () => {
  const r = rig();
  const before = r.window.history.length;
  r.router.open('devices');
  assert.deepEqual(r.popup.told, ['show null'], 'the popup opens at its list');
  assert.deepEqual(r.router.state(), { device: 'A', popup: 'devices', page: null });
  assert.equal(r.window.history.length, before + 1, 'opening pushes');
  assert.equal(r.window.location.search, '?device=A', 'the address stays the device\'s');
  r.router.step('amp');
  assert.equal(r.window.history.length, before + 2, 'entering a page pushes');
  assert.deepEqual(r.router.state(), { device: 'A', popup: 'devices', page: 'amp' });
  await r.back();
  assert.deepEqual(r.router.state(), { device: 'A', popup: 'devices', page: null }, 'page → list');
  assert.equal(r.popup.open, true);
  await r.back();
  assert.deepEqual(r.router.state(), { device: 'A', popup: null, page: null }, 'list → closed');
  assert.deepEqual(r.popup.told, ['show null', 'show amp', 'show null', 'hide']);
  assert.deepEqual(r.shows, [], 'no device was shown on the way');
  // and forward again: the popup, then its page, still no device
  await r.forward();
  await r.forward();
  assert.deepEqual(r.router.state(), { device: 'A', popup: 'devices', page: 'amp' });
  assert.deepEqual(r.shows, []);
});

await ok('back_past_the_popup_changes_the_device_only_when_the_device_part_changed', async () => {
  const r = rig();
  r.device('B');
  r.router.open('devices');
  r.router.step('amp');
  await r.back();
  await r.back();
  assert.equal(r.popup.open, false);
  assert.deepEqual(r.shows, [], 'the popup is closed and B is still shown');
  assert.equal(r.shown(), 'B');
  await r.back();
  assert.deepEqual(r.shows, ['A'], 'the step past the popup is the one that names another device');
  assert.equal(r.popup.open, false);
});

await ok('a_popup_page_may_name_another_device_than_the_one_shown', async () => {
  const r = rig();
  r.router.open('devices');
  r.router.step('B');
  assert.deepEqual(r.router.state(), { device: 'A', popup: 'devices', page: 'B' });
  await r.back();
  await r.back();
  assert.deepEqual(r.shows, []);
  assert.equal(r.shown(), 'A');
});

await ok('an_entry_without_a_device_keeps_the_shown_device', async () => {
  const r = rig();
  r.device('B');
  // entries that are not the router's: no state at all, and a popup pushed without a device
  r.window.history.pushState(null, '', './?device=A#elsewhere');
  r.window.history.pushState({ popup: 'devices' }, '', './');
  r.window.history.pushState({ other: true }, '', './');
  await r.back();
  assert.equal(r.popup.open, false, 'a popup this load did not open is not shown');
  assert.equal(r.entry().popup, null, 'its entry is a plain one from here on');
  await r.back();
  assert.equal(r.popup.open, false);
  assert.deepEqual(r.shows, [], 'no step showed a device, whatever the address or the fleet says');
  assert.deepEqual(r.router.state(), { device: 'B', popup: null, page: null });
  await r.back();
  assert.deepEqual(r.shows, ['B'], 'the router\'s own entry for B reads B again, as any step between two entries does');
  await r.back();
  assert.deepEqual(r.shows, ['B', 'A'], 'and the one under it names A');
});

await ok('escape_or_the_close_button_steps_back_past_the_popup_and_forward_opens_nothing', async () => {
  for (const deep of [false, true]) {
    const r = rig();
    r.device('B');
    const base = r.window.history.length;
    r.router.open('devices');
    if (deep) r.router.step('amp');
    r.router.close();
    assert.equal(r.popup.open, false, 'the popup is gone at once');
    assert.deepEqual(r.router.state(), { device: 'B', popup: null, page: null });
    r.router.close();
    await r.landed();
    assert.equal(r.entry().device, 'B');
    assert.equal(r.entry().popup, null, 'the entry is the one the popup was opened from');
    assert.equal(r.window.history.length, base + (deep ? 2 : 1), 'the popup\'s entries are ahead');
    const told = r.popup.told.length;
    for (let ahead = 0; ahead < (deep ? 2 : 1); ahead++) {
      await r.forward();
      assert.equal(r.popup.open, false, 'Forward after the close opens nothing');
      assert.equal(r.entry().popup, null, 'and the entry is a plain one from here on');
      assert.deepEqual(r.router.state(), { device: 'B', popup: null, page: null });
    }
    assert.equal(r.popup.told.length, told, 'the popup was told nothing');
    assert.deepEqual(r.shows, [], 'and no device was shown');
    // back over the entries that were the popup's: still nothing opens
    for (let behind = 0; behind < (deep ? 2 : 1); behind++) await r.back();
    assert.equal(r.popup.open, false);
    // a popup opened afresh is its own: it opens, and Back closes it
    r.router.open('devices');
    assert.equal(r.popup.open, true);
    await r.back();
    assert.equal(r.popup.open, false);
    await r.forward();
    assert.equal(r.popup.open, true, 'Forward after Back shows the popup again');
  }
});

await ok('a_page_changed_from_a_page_replaces_and_null_steps_back_to_the_list', async () => {
  const r = rig();
  r.router.open('devices');
  r.router.step('amp');
  const length = r.window.history.length;
  r.router.step('dac');
  r.router.step('dac');
  assert.equal(r.window.history.length, length, 'a page from a page is the same entry');
  assert.deepEqual(r.router.state(), { device: 'A', popup: 'devices', page: 'dac' });
  r.router.step(null);
  assert.deepEqual(r.router.state(), { device: 'A', popup: 'devices', page: null }, 'the list at once');
  await r.landed();
  assert.equal(r.entry().page, null, 'the history is on the list\'s entry');
  assert.equal(r.popup.open, true);
  await r.back();
  assert.equal(r.popup.open, false, 'one more step back closes');
  assert.deepEqual(r.shows, []);

  // opened straight at a page: Back closes; the list is the same entry
  r.router.open('devices', 'amp');
  r.router.step(null);
  assert.deepEqual(r.router.state(), { device: 'A', popup: 'devices', page: null });
  r.router.open('devices', 'dac');
  assert.deepEqual(r.router.state(), { device: 'A', popup: 'devices', page: 'dac' }, 'open on the open popup is a step');
  r.router.close();
  await r.landed();
  assert.equal(r.entry().popup, null);
  assert.deepEqual(r.shows, []);
});

await ok('a_push_asked_for_while_the_step_back_is_on_its_way_waits_for_it', async () => {
  const r = rig();
  const base = r.window.history.length;
  r.router.open('devices');
  r.router.step('amp');
  // a device switch closes the popup (its modal tells the router) and then pushes
  r.router.close();
  r.device('B');
  assert.equal(r.window.history.length, base + 2, 'not pushed over the popup\'s entries');
  await r.landed();
  assert.equal(r.window.history.length, base + 1, 'the device\'s entry took the place of the popup\'s');
  assert.deepEqual(r.router.state(), { device: 'B', popup: null, page: null });
  assert.equal(r.entry().device, 'B');
  assert.equal(r.window.location.search, '?device=B');
  await r.back();
  assert.deepEqual(r.shows, ['A']);
  assert.equal(r.entry().popup, null, 'and under it is the entry the popup was opened from');
  // a popup asked for meanwhile waits too
  const s = rig();
  s.router.open('devices');
  s.router.close();
  s.router.open('devices', 'amp');
  assert.equal(s.popup.open, false);
  await s.landed();
  assert.deepEqual(s.router.state(), { device: 'A', popup: 'devices', page: 'amp' });
});

await ok('a_device_shown_hides_the_popup_and_a_step_back_into_it_shows_the_device_then_the_popup', async () => {
  const r = rig({ slow: true });
  r.router.open('devices');
  r.device('B');
  assert.equal(r.popup.open, false, 'a device shown is a plain entry');
  assert.equal(r.entry().popup, null);
  const stepped = r.back();
  await r.settle();
  await r.settle();
  assert.deepEqual(r.shows, ['A']);
  assert.equal(r.popup.open, false, 'not before the device is on show');
  r.finish();
  await stepped;
  await r.settle();
  assert.equal(r.popup.open, true);
  assert.deepEqual(r.router.state(), { device: 'A', popup: 'devices', page: null });
  // a newer step while the device is still being read: the older one's popup is dropped
  await r.forward();
  r.finish();
  await r.settle();
  assert.equal(r.popup.open, false);
  const first = r.back();
  await r.settle();
  await r.settle();
  r.device('C');
  r.finish();
  await first;
  await r.settle();
  assert.equal(r.popup.open, false);
  assert.deepEqual(r.router.state(), { device: 'C', popup: null, page: null });
});

await ok('a_popup_registered_without_its_element_is_refused_by_name', async () => {
  const r = rig();
  assert.throws(() => r.router.register('bare', { show() {} }), /the popup bare is registered with its element/);
  assert.throws(() => r.router.register('none'), /the popup none/);
});

await ok('a_popup_nothing_registered_is_a_plain_entry', async () => {
  const r = rig();
  r.router.open('nothing');
  assert.deepEqual(r.router.state(), { device: 'A', popup: null, page: null });
  r.router.step('amp');
  r.router.close();
  assert.equal(r.entry().popup, null);
  r.window.history.pushState({ device: 'A', popup: 'nothing', page: null, run: 'x.1' }, '', './');
  r.window.history.pushState({ device: 'A' }, '', './');
  await r.back();
  assert.equal(r.entry().popup, null, 'rewritten as a plain entry');
  assert.deepEqual(r.router.state(), { device: 'A', popup: null, page: null });
  assert.deepEqual(r.shows, []);
});

await ok('a_popup_is_on_the_stack_and_off_it_with_the_history_whoever_closes_it', async () => {
  const r = rig();
  const { document } = r;
  const theme = document.getElementById('theme');
  document.getElementById('knob').focus();
  const base = r.window.history.length;
  r.router.open('devices');
  assert.equal(r.el.parentNode, document.body, 'the router put the popup on the stack');
  assert.equal(r.el.getAttribute('aria-modal'), 'true');
  assert.equal(r.el.getAttribute('aria-labelledby'), 'devices-title');
  assert.equal(r.modal.opened(), true);
  assert.equal(theme.inert, true);
  assert.equal(document.activeElement.id, 'devices-x', 'the focus is in it');
  r.key('ArrowLeft');
  assert.deepEqual(r.popup.keys, ['ArrowLeft'], 'its keys come through the stack');
  // the stack emptied (another theme, another device): the history follows, and the popup opens again
  r.modal.closeAll();
  assert.equal(r.popup.open, false);
  assert.deepEqual(r.router.state(), { device: 'A', popup: null, page: null });
  await r.landed();
  assert.equal(r.entry().popup, null, 'the history is on the entry the popup was opened from');
  assert.equal(r.window.history.length, base + 1);
  assert.equal(document.activeElement.id, 'knob');
  r.router.open('devices');
  assert.equal(r.popup.open, true, 'it opens again after a closeAll');
  assert.deepEqual(r.router.state(), { device: 'A', popup: 'devices', page: null });
  // asked for before the step back has landed, it opens once it has
  r.modal.closeAll();
  r.router.open('devices', 'amp');
  assert.equal(r.popup.open, false);
  await r.landed();
  assert.equal(r.popup.open, true);
  assert.deepEqual(r.router.state(), { device: 'A', popup: 'devices', page: 'amp' });
  // Escape: the stack closes it, the history steps back past its page and its list
  r.router.step(null);
  r.router.step('amp');
  r.key('Escape');
  assert.equal(r.popup.open, false);
  await r.landed();
  assert.equal(r.entry().popup, null);
  assert.equal(theme.inert, false);
  await r.forward();
  assert.equal(r.popup.open, false, 'Forward after Escape opens nothing');
  assert.equal(r.modal.opened(), false);
  await r.back();
  // Back: the router takes it off the stack, once
  r.router.open('devices');
  const told = r.popup.told.length;
  await r.back();
  assert.equal(r.popup.open, false);
  assert.equal(r.modal.opened(), false);
  assert.deepEqual(r.popup.told.slice(told), ['hide']);
  assert.equal(r.entry().popup, null);
  await r.forward();
  assert.equal(r.popup.open, true, 'Forward after Back puts it back on the stack');
  assert.equal(theme.inert, true);
  // the close button
  r.router.close();
  assert.equal(r.modal.opened(), false);
  await r.landed();
  // taken out of the page by something else: off the stack at the next key, the history follows
  r.router.open('devices');
  r.el.remove();
  r.key('m', { on: document.body });
  assert.deepEqual(r.router.state(), { device: 'A', popup: null, page: null });
  await r.landed();
  assert.equal(r.entry().popup, null);
  // (a step between two plain entries of one device reads that device again, as it always did)
  assert.deepEqual(r.shows.filter((url) => url !== 'A'), [], 'no other device was shown on the way');
  assert.equal(r.shown(), 'A');
});

await ok('a_page_loaded_again_opens_no_popup_of_the_load_before', async () => {
  // a reload in the middle of a popup's page, then Back, Back, Forward, Forward
  const r = rig();
  r.router.open('devices');
  r.router.step('amp');
  r.unload();
  const b = r.again();
  assert.deepEqual(b.router.state(), { device: 'A', popup: null, page: null });
  assert.equal(r.entry().popup, null, 'the entry the page was loaded on is a plain one');
  await r.back();
  assert.equal(b.popup.open, false, 'Back onto the old popup\'s list opens nothing');
  assert.equal(r.entry().popup, null, 'and makes it a plain entry');
  await r.back();
  await r.forward();
  await r.forward();
  assert.deepEqual(b.popup.told, [], 'the popup was told nothing on the way');
  assert.equal(b.modal.opened(), false);
  // a popup this load opens is its own: Back closes it, Forward shows it again
  b.router.open('devices');
  await r.back();
  await r.forward();
  assert.equal(b.popup.open, true);
  assert.deepEqual(b.popup.told, ['show null', 'hide', 'show null']);

  // Escape, a reload, then Forward: no ghost
  const s = rig();
  s.router.open('devices');
  s.router.close();
  await s.landed();
  s.unload();
  const c = s.again();
  await s.forward();
  assert.equal(c.popup.open, false);
  assert.deepEqual(c.popup.told, []);
  assert.deepEqual(c.router.state(), { device: 'A', popup: null, page: null });
  assert.equal(s.entry().popup, null);
  assert.deepEqual([...b.shows, ...c.shows].filter((url) => url !== 'A'), [], 'and no other device was read');
});

await ok('a_popup_that_draws_in_show_has_its_control_when_the_stack_gives_the_focus', async () => {
  const r = rig();
  const el = r.document.createElement('div');
  r.router.register('drawn', {
    el,
    labelledBy: null,
    show(at) { el.innerHTML = `<button id="drawn-${at ?? 'list'}">Go</button>`; },
  });
  r.router.open('drawn');
  assert.equal(r.document.activeElement?.id, 'drawn-list', 'the focus is on the control show drew');
  r.router.close();
});

await ok('an_ask_and_a_list_a_popup_opens_in_show_are_the_popups_and_go_with_it', async () => {
  const r = rig();
  const { document } = r;
  // jsdom's listeners take only its own signals
  globalThis.AbortController = r.window.AbortController;
  const WORDS = { presetName: 'Name', newName: 'Name', answers: { save: 'Save', cancel: 'Cancel' } };
  const form = (within, note) => askForm({
    within, kind: 'save', words: WORDS, submit: async () => ({ ok: true }), onClose: note,
  });
  const closed = { pageAsk: 0, popAsk: 0, pageList: 0, popList: 0 };
  document.getElementById('theme').insertAdjacentHTML('beforeend', '<div id="place"></div>');
  const knob = document.getElementById('knob');
  form(document.getElementById('place'), () => { closed.pageAsk++; });
  openList({ opener: knob, label: 'x', options: [], choose() {}, onClose: () => { closed.pageList++; } });
  const el = document.createElement('div');
  el.innerHTML = '<div id="in"></div><button id="chip">c</button>';
  r.router.register('form', {
    el,
    // straight at a page that is a form: drawn before the stack has `el`
    show() {
      form(el.querySelector('#in'), () => { closed.popAsk++; });
      openList({
        opener: el.querySelector('#chip'), label: 'y', options: [], choose() {}, within: el,
        onClose: () => { closed.popList++; },
      });
    },
    hide() {
      // the stack took `el` out of the page before this runs
      closeAsk(el);
      closeList(el);
    },
  });
  r.router.open('form', 'new');
  assert.equal(el.isConnected, true);
  assert.deepEqual(closed, { pageAsk: 0, popAsk: 0, pageList: 0, popList: 0 },
    "the popup's ask and list left the page's open");
  assert.equal(openAsks(), 2);
  assert.equal(openLists(), 2);
  const landed = r.landed();
  r.router.close();
  await landed;
  assert.deepEqual(closed, { pageAsk: 0, popAsk: 1, pageList: 0, popList: 1 },
    "closing the popup closed its own and hide()'s closeAsk(el) left the page's alone");
  assert.equal(openAsks(), 1);
  assert.equal(openLists(), 1);
  closeAsk();
  closeList();
  assert.equal(openAsks(), 0);
  assert.equal(openLists(), 0);
});

await ok('a_popup_its_viewer_may_not_open_opens_by_no_path_and_says_so_once_per_ask', async () => {
  const r = rig();
  const el = r.document.createElement('div');
  let may = true;
  const told = [];
  r.router.register('gear', {
    el, allowed: () => may, refused: () => told.push('refused'),
    show(at) { told.push(`show ${at}`); },
  });
  may = false;
  r.router.open('gear');
  r.router.open('gear', 'two');
  assert.equal(el.isConnected, false);
  assert.deepEqual(told, ['refused', 'refused'], 'an open is turned away and says so; nothing is drawn');
  assert.equal(r.entry().popup, null, 'and no entry is pushed');
  // an entry of the history that names it (opened while the viewer still could)
  may = true;
  r.router.open('gear', 'two');
  assert.equal(el.isConnected, true);
  await r.back();
  assert.equal(el.isConnected, false);
  may = false;
  told.length = 0;
  await r.forward();
  assert.equal(el.isConnected, false, 'Forward onto its entry opens nothing');
  assert.equal(r.entry().popup, null, 'the entry is made a plain one');
  assert.deepEqual(told, [], 'a step of the history says nothing');
  // a popup with no `allowed` is anyone's
  r.router.open('devices');
  assert.equal(r.popup.open, true);
});

await ok('a_right_lost_while_a_step_reads_another_device_leaves_no_popup_standing', async () => {
  const r = rig({ slow: true });
  const el = r.document.createElement('div');
  let may = true;
  r.router.register('gear', { el, allowed: () => may, show() {} });
  r.router.open('gear');
  assert.equal(el.isConnected, true);
  r.device('B');
  assert.equal(el.isConnected, false, 'another device shown closes it');
  // Back onto the popup's entry of device A: the device is read first, and the right goes meanwhile
  await r.back();
  assert.deepEqual(r.shows, ['A']);
  may = false;
  r.finish();
  await new Promise((resolve) => { r.window.setTimeout(resolve, 0); });
  assert.equal(r.shown(), 'A');
  assert.equal(el.isConnected, false, 'the popup does not open for a viewer who may no longer have it');
  assert.equal(r.router.state().popup, null);
  assert.equal(r.entry().popup, null, 'and its entry is a plain one');
  assert.equal(r.entry().device, 'A');
});

await ok('a_popup_entry_pushed_with_no_tap_or_key_is_said_in_the_console', async () => {
  const warned = [];
  const warn = console.warn;
  console.warn = (...args) => { warned.push(String(args[0])); };
  try {
    const activation = { isActive: false };
    const r = rig({ activation });
    r.router.open('devices');
    assert.equal(r.popup.open, true, 'the popup opens all the same');
    assert.equal(warned.length, 1);
    assert.match(warned[0], /^svolder: the popup devices was opened with no tap or key/);
    r.router.step('two');
    assert.equal(warned.length, 2, 'the step from the list to a page pushes too');
    assert.match(warned[1], /^svolder: the popup devices was stepped to a page with no tap or key/);
    r.router.step('three');
    assert.equal(warned.length, 2, 'a page changed in place pushes nothing');
    let landed = r.landed();
    r.router.close();
    await landed;
    activation.isActive = true;
    r.router.open('devices');
    r.router.step('two');
    assert.equal(warned.length, 2, 'from a tap or a key: nothing is said');
    landed = r.landed();
    r.router.close();
    await landed;
    r.router.open('nothing');
    activation.isActive = false;
    r.device('B');
    assert.equal(warned.length, 2, 'a popup nothing registered and a device shown are no popup entry');
    // a browser that does not say (jsdom): nothing is said
    const quiet = rig();
    quiet.router.open('devices');
    assert.equal(warned.length, 2);
  } finally {
    console.warn = warn;
  }
});

await ok('after_a_switch_under_a_staying_popup_and_a_reload_back_keeps_the_device_shown', async () => {
  const r = rig({ stays: true });
  r.router.open('devices');
  await r.settle();
  r.router.step('amp');
  await r.settle();
  // Show on the page: B is shown, with no entry of its own
  r.router.shown('B', './?device=B');
  assert.deepEqual(r.router.state(), { device: 'B', popup: 'devices', page: 'amp' });
  r.unload();
  // the page is loaded again on B: its popup entries are another load's and open nothing
  const s = r.again({ device: 'B', stays: true });
  await r.back();
  assert.deepEqual(s.shows, [], 'Back onto the list entry of the load before shows no device');
  assert.deepEqual(s.router.state(), { device: 'B', popup: null, page: null });
  assert.equal(r.window.location.search, '?device=B');
  await r.back();
  // a step between two plain entries reads the entry's device again, as always: it is B's
  assert.deepEqual(s.shows, ['B'], 'the entry the popup was opened from names B, never the device before');
  assert.deepEqual(s.router.state(), { device: 'B', popup: null, page: null });
  assert.equal(r.entry().device, 'B');
  assert.equal(r.window.location.search, '?device=B');
});

await ok('a_new_chain_of_loads_takes_no_device_a_staying_popup_carried_in_the_one_before', async () => {
  const r = rig({ stays: true });
  r.router.open('devices');
  await r.settle();
  // Show on the page: B is carried under the popup, with the place of the entry it was opened from
  r.router.shown('B', './?device=B');
  r.unload();
  // the person goes to another address in the same tab (a bookmark, a typed address): a new
  // entry with no state of the router's, and a page loaded on C that counts its places from 0
  r.window.history.pushState(null, '', './?device=C');
  const s = r.again({ device: 'C', stays: true });
  assert.equal(r.entry().device, 'C');
  s.router.open('devices');
  await r.settle();
  const done = r.landed();
  s.router.close();
  await done;
  assert.deepEqual(s.router.state(), { device: 'C', popup: null, page: null },
    'the entry the popup was opened from is this chain\'s, and names C');
  assert.equal(r.entry().device, 'C');
  assert.equal(r.window.location.search, '?device=C');
  assert.deepEqual(s.shows, [], 'and nothing was shown');
});

console.log(`core-router.mjs: ${n} passed`);
