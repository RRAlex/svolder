/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the keys of the frame's menu (core/menu.js) and of the modal stack over it
// (core/modal.js, handed its keys by core/frame.js), on the real core over a jsdom page
// (dom.mjs). These cases need what only a real DOM gives: a key that travels from the focused
// row up to the window, where the frame's listener hears it in the capture phase before the
// menu's own and can stop it; the focus moving from row to row and back to the button; a
// `focusout` that names where the focus went. The rows themselves are data and are pinned
// without a page in core-menu-rows.mjs.
// Run: node core-menu.mjs
import assert from 'node:assert/strict';
import { page } from './dom.mjs';
import { createCore } from '../../../bins/svolderd/web/core/core.js';
import { mountTheme } from '../../../bins/svolderd/web/core/registry.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
console.warn = () => {};

const DEVICES = [{ url: 'A', name: 'Bifrost' }, { url: 'B', name: 'Kara' }];
const model = (devices) => ({
  v: 1, viewer: { guest: false, writes: true, sign_out: true }, fleet: { devices, hops: [] },
  shown: 'A', device: null, followed: {}, card: null, host: null, strip: null,
});

/** A theme with two radios (the second checked, the first disabled), a check that is not
 * available and an action. */
const THEME = {
  id: 'one', label: 'One', needs: [], schemes: ['dark'],
  preview: (box) => { box.textContent = 'drawn'; },
  mount: (root) => {
    root.innerHTML = '<button id="knob">Volume</button>';
    return {
      unmount() {},
      menuRows: () => [
        { kind: 'heading', text: 'Look' },
        {
          kind: 'radio', id: 'off', label: 'Off', checked: () => false,
          disabled: () => 'Not here', choose() {},
        },
        {
          kind: 'radio', id: 'on', label: 'On', checked: () => true, disabled: () => null,
          choose() {},
        },
        {
          kind: 'check', id: 'gone', label: 'Gone', checked: () => false, available: () => false,
          flip() {},
        },
        { kind: 'action', id: 'act', label: 'Act', elementId: 'row-act', run() {} },
      ],
    };
  },
};

/** The real core with the theme on show; `devices` is the fleet. */
async function rig(devices = DEVICES, popups = undefined) {
  const p = page('<div id="theme"></div><div id="toasts"></div>');
  globalThis.window = p.window;
  globalThis.document = p.document;
  p.window.EventSource = class { addEventListener() {} close() {} };
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  p.window.fetch = async () => ({ status: 200, ok: true, json: async () => ({}) });
  // jsdom lays nothing out and scrolls nothing
  p.window.Element.prototype.scrollIntoView = () => {};
  const core = createCore({
    page: model(devices), themes: [], window: p.window, ...(popups ? { popups } : {}),
  });
  core.registry.add(THEME);
  await mountTheme({
    registry: core.registry, id: 'one', root: p.document.getElementById('theme'), core,
    toast() {}, fallback: 'one',
  });
  const button = p.document.getElementById('fx-button');
  const pop = p.document.getElementById('fx-menu');
  /** The words of the row that has the focus (its key hint and arrow left off). */
  const at = () => {
    const el = p.document.activeElement;
    return el?.id || el?.firstChild?.textContent || el?.tagName;
  };
  const press = (name, init = {}) => p.key(name, init);
  return { ...p, core, button, pop, at, press };
}

await ok('arrow_down_on_the_closed_button_opens_the_menu_on_the_checked_radio', async () => {
  const r = await rig();
  r.button.focus();
  const other = r.press('a');
  assert.equal(r.pop.hidden, true, 'another key on the button opens nothing');
  assert.equal(other.defaultPrevented, false);
  const event = r.press('ArrowDown');
  assert.equal(r.pop.hidden, false);
  assert.equal(event.defaultPrevented, true, 'the page does not scroll');
  assert.equal(r.button.getAttribute('aria-expanded'), 'true');
  assert.equal(r.at(), 'On', 'the focus opens on the checked radio');
  const theme = r.document.getElementById('menu-theme');
  assert.equal(theme.firstElementChild.className, 'mthumb', 'the Theme row opens with its picture');
  assert.equal(theme.firstElementChild.textContent, 'drawn', 'which the theme drew');
  assert.equal(r.pop.querySelectorAll('.mthumb').length, 1, 'and no other row has one');
  r.window.close();
});

await ok('the_arrows_go_round_the_ends_past_a_disabled_and_a_hidden_row', async () => {
  const r = await rig();
  r.core.frame.openMenu();
  const seen = [r.at()];
  for (let i = 0; i < 7; i++) {
    r.press('ArrowDown');
    seen.push(r.at());
  }
  // Off is disabled, Gone is hidden: neither is reached; Devices, Sound and Help are the
  // page's popup rows
  assert.deepEqual(seen, ['On', 'row-act', 'menu-popup-devices', 'menu-popup-sound',
    'menu-popup-help',
    'menu-sign-out', 'menu-theme', 'On']);
  r.press('ArrowUp');
  assert.equal(r.at(), 'menu-theme');
  r.press('ArrowUp');
  assert.equal(r.at(), 'menu-sign-out', 'and round the top');
  r.press('ArrowLeft');
  assert.equal(r.at(), 'menu-popup-help', 'Left is Up');
  r.press('ArrowRight');
  assert.equal(r.at(), 'menu-sign-out', 'Right is Down');
  r.press('Home');
  assert.equal(r.at(), 'menu-theme');
  r.press('End');
  assert.equal(r.at(), 'menu-sign-out');
  r.window.close();
});

await ok('escape_and_q_close_the_menu_and_the_focus_is_back_on_the_button', async () => {
  const r = await rig();
  r.core.frame.openMenu();
  const event = r.press('Escape');
  assert.equal(r.pop.hidden, true);
  assert.equal(event.defaultPrevented, true);
  assert.equal(r.at(), 'fx-button');
  r.core.frame.openMenu();
  r.press('q', { ctrlKey: true });
  assert.equal(r.pop.hidden, false, 'Ctrl+q is the browser\'s');
  r.press('Q');
  assert.equal(r.pop.hidden, true);
  assert.equal(r.at(), 'fx-button');
  r.window.close();
});

await ok('a_tab_out_of_the_menu_closes_it_and_leaves_the_focus_where_it_went', async () => {
  const r = await rig();
  r.core.frame.openMenu();
  r.press('End');
  const tab = r.press('Tab');
  assert.equal(tab.defaultPrevented, false, 'Tab is the browser\'s');
  // what the browser then does: the focus goes to the next control of the page
  r.document.getElementById('knob').focus();
  assert.equal(r.pop.hidden, true, 'the focus left the menu: it closes');
  assert.equal(r.at(), 'knob', 'and is not pulled back to the button');
  r.window.close();
});

await ok('the_menu_has_no_device_row_with_one_device_or_with_two', async () => {
  for (const devices of [DEVICES.slice(0, 1), DEVICES]) {
    const r = await rig(devices);
    r.core.frame.openMenu();
    assert.equal(r.document.getElementById('menu-device'), null);
    assert.equal(r.pop.querySelectorAll('[data-device], [role=menuitemradio][data-device]').length, 0);
    r.press('End');
    r.press('ArrowUp');
    assert.equal(r.at(), 'menu-popup-help', 'from Sign out to the row before it');
    r.window.close();
  }
});

await ok('a_listed_popups_row_is_in_the_menu_and_opens_its_popup', async () => {
  const r = await rig(DEVICES, [{
    name: 'guide', title: 'Guide', row: { label: 'Guide', sees: 'everyone' },
    load: async () => ({ wire: () => ({ show() {} }) }),
  }]);
  r.core.frame.openMenu();
  const ids = [...r.pop.querySelectorAll('[id^="menu-"]')].map((el) => el.id);
  assert.deepEqual(ids, ['menu-theme', 'menu-popup-guide', 'menu-sign-out']);
  const row = r.document.getElementById('menu-popup-guide');
  assert.equal(row.firstChild.textContent, 'Guide');
  row.click();
  await r.settle();
  assert.equal(r.core.router.state().popup, 'guide', 'the row opens the popup it was listed with');
  assert.equal(r.pop.hidden, true, 'and the menu has closed');
  r.window.close();
  const plain = await rig();
  plain.core.frame.openMenu();
  assert.deepEqual([...plain.pop.querySelectorAll('[id^="menu-popup-"]')].map((el) => el.id),
    ['menu-popup-devices', 'menu-popup-sound', 'menu-popup-help'],
    'the page as shipped has the rows of Devices, Sound and Help and no other');
  plain.window.close();
});

await ok('tab_goes_round_inside_a_modal_and_between_its_controls_it_is_the_browsers', async () => {
  const r = await rig();
  const el = r.document.createElement('div');
  el.innerHTML = '<button id="in-a">a</button><button id="in-b">b</button>';
  r.core.modal.open({ el });
  r.document.getElementById('in-b').focus();
  const tab = r.press('Tab');
  assert.equal(tab.defaultPrevented, true, 'from the last control the stack moves the focus');
  assert.equal(r.at(), 'in-a');
  const back = r.press('Tab', { shiftKey: true });
  assert.equal(back.defaultPrevented, true);
  assert.equal(r.at(), 'in-b');
  const inside = r.press('Tab', { shiftKey: true });
  assert.equal(inside.defaultPrevented, false, 'between two of its controls Tab is the browser\'s');
  r.window.close();
});

console.log(`core-menu.mjs: ${n} passed`);
