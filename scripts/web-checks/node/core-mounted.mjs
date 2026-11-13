/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the mounted instance (core/core.js, core/registry.js, core/menu.js): what a
// theme's `mount()` returns, `{ unmount, menuRows?, keys? }`, is `core.mounted` while the theme is
// on show, and the frame reads its fields before the theme object's. The instance's rows win over
// the theme's, a theme with none keeps the theme object's, after a switch the old instance's rows
// and keys are gone, and a theme that returns a bare function mounts and unmounts. The
// frame records the last focus inside `#theme` (`core.frame.lastFocus()`): the menu, its button
// and a modal do not overwrite it, an element that left the page is none, a theme switch clears
// it, and its zone is found in the key table of the theme on show. The
// real core over a jsdom page (apt `node-jsdom`) with the real switch of themes.
// Run: node core-mounted.mjs
import assert from 'node:assert/strict';
import { page } from './dom.mjs';
import { createCore } from '../../../bins/svolderd/web/core/core.js';
import { mountTheme } from '../../../bins/svolderd/web/core/registry.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
console.warn = () => {};

const MODEL = {
  v: 1, viewer: { guest: false, writes: true }, fleet: { devices: [], hops: [] }, shown: 'A',
  device: null, followed: {}, card: null, host: null, strip: null,
};

/** The real core over a page, with the themes `themes` registered; `mount(id)` puts one on show
 * as the page's switch does and `rows()` reads the menu's rows as drawn. */
function rig(themes, { popups } = {}) {
  const p = page('<div id="theme"></div><div id="toasts"></div>');
  globalThis.window = p.window;
  globalThis.document = p.document;
  p.window.EventSource = class { addEventListener() {} close() {} };
  // jsdom has no media queries: the page is dark and stays so
  p.window.matchMedia = () => ({ matches: false, addEventListener() {}, removeEventListener() {} });
  p.window.fetch = async () => ({ status: 200, ok: true, json: async () => ({}) });
  const core = createCore({
    page: structuredClone(MODEL), themes: [], window: p.window, ...(popups ? { popups } : {}),
  });
  for (const theme of themes) core.registry.add(theme);
  const root = p.document.getElementById('theme');
  const mount = (id) => mountTheme({
    registry: core.registry, id, root, core, toast() {}, fallback: themes[0].id,
  });
  const rows = () => {
    core.frame.openMenu();
    const texts = [...p.document.querySelectorAll('#fx-menu .fxh')].map((el) => el.textContent);
    core.frame.closeMenu(false);
    return texts;
  };
  return { ...p, core, mount, rows };
}

const heading = (text) => () => [{ kind: 'heading', text }];
const base = (id, over) => ({ id, label: id, needs: [], schemes: ['dark'], ...over });

await ok('the_instances_menu_rows_win_over_the_theme_objects', async () => {
  const keys = [{ key: 'm', zone: 'page', words: 'Mute' }];
  const r = rig([base('one', {
    menuRows: heading('of the theme'),
    keys: [{ key: 'x', zone: 'page', words: 'Of the theme' }],
    mount: () => ({ unmount() {}, menuRows: heading('of the instance'), keys }),
  })]);
  assert.equal(r.core.mounted, null, 'nothing is mounted yet');
  const mounted = await r.mount('one');
  assert.deepEqual(r.rows(), ['of the instance']);
  assert.equal(r.core.mounted.keys, keys);
  assert.equal(r.core.frame.keyTable(), keys, "the frame gives the instance's table");
  await mounted.unmount();
  r.window.close();
});

await ok('the_instances_rows_are_given_the_core', async () => {
  let given = null;
  const r = rig([base('one', {
    mount: () => ({ unmount() {}, menuRows(core) { given = core; return []; } }),
  })]);
  await r.mount('one');
  r.rows();
  assert.equal(given, r.core);
  r.window.close();
});

await ok('a_theme_without_instance_fields_keeps_the_theme_objects', async () => {
  const keys = [{ key: 'x', zone: 'page', words: 'Of the theme' }];
  const r = rig([
    base('one', { menuRows: heading('of the theme'), keys, mount: () => ({ unmount() {} }) }),
    base('bare', { menuRows: heading('bare'), mount: () => () => {} }),
  ]);
  const one = await r.mount('one');
  assert.deepEqual(r.rows(), ['of the theme']);
  assert.equal(r.core.frame.keyTable(), keys);
  await one.unmount();
  let left = 0;
  r.core.registry.add(base('bare', { menuRows: heading('bare'), mount: () => () => { left++; } }));
  const bare = await r.mount('bare');
  assert.equal(r.core.mounted, null, 'a bare function is no instance');
  assert.deepEqual(r.rows(), ['bare']);
  assert.deepEqual(r.core.frame.keyTable(), [], 'no table: an empty one');
  await bare.unmount();
  assert.equal(left, 1, 'and it unmounts');
  r.window.close();
});

await ok('after_a_switch_the_old_instances_rows_and_keys_are_gone', async () => {
  const keys = [{ key: 'm', zone: 'page', words: 'Mute' }];
  const r = rig([
    base('one', { mount: () => ({ unmount() {}, menuRows: heading('of one'), keys }) }),
    base('two', { mount: () => ({ unmount() {} }) }),
  ]);
  const one = await r.mount('one');
  assert.deepEqual(r.rows(), ['of one']);
  // what the switch does: the theme on show unmounts, then the next one mounts
  await one.unmount();
  assert.equal(r.core.mounted, null, 'unmounted: no instance');
  assert.deepEqual(r.core.frame.keyTable(), []);
  const two = await r.mount('two');
  assert.deepEqual(r.rows(), [], "two has no rows, and one's are not drawn");
  assert.deepEqual(r.core.frame.keyTable(), []);
  assert.equal(r.core.mounted, two.instance);
  await two.unmount();
  r.window.close();
});

await ok('a_theme_that_throws_as_it_mounts_leaves_no_instance_behind', async () => {
  const r = rig([
    base('one', { mount: () => ({ unmount() {}, menuRows: heading('of one') }) }),
    base('bad', { menuRows: heading('bad'), mount() { throw new Error('no canvas'); } }),
  ]);
  const mounted = await r.mount('bad');
  assert.equal(mounted.id, 'one', 'the fallback is on show');
  assert.deepEqual(r.rows(), ['of one']);
  r.window.close();
});

/** A theme that draws a dial with a scale in it, a plain button and a field, with a key table. */
const DIAL_KEYS = [
  { key: 'Home', zone: '.dial .scale', words: 'To the lowest' },
  { key: '↑ ↓', zone: '.dial', words: 'One step' },
];
const dialTheme = (id) => base(id, {
  mount(root) {
    root.innerHTML = '<div class="dial"><div class="scale"><button id="deep">deep</button></div>'
      + '<button id="rim">rim</button></div><button id="plain">plain</button>';
    return { unmount() { root.replaceChildren(); }, keys: DIAL_KEYS };
  },
});

await ok('the_frame_records_the_last_focus_inside_the_theme', async () => {
  const r = rig([dialTheme('one')]);
  await r.mount('one');
  assert.equal(r.core.frame.lastFocus(), null, 'nothing had the focus yet');
  r.document.getElementById('plain').focus();
  assert.equal(r.core.frame.lastFocus(), r.document.getElementById('plain'));
  r.document.getElementById('deep').focus();
  assert.equal(r.core.frame.lastFocus(), r.document.getElementById('deep'));
  assert.equal(r.core.keys.zoneOf(r.core.frame.keyTable(), r.core.frame.lastFocus()), '.dial .scale',
    'and its zone is found in the table of the theme on show, by an ancestor');
  r.document.getElementById('rim').focus();
  assert.equal(r.core.keys.zoneOf(r.core.frame.keyTable(), r.core.frame.lastFocus()), '.dial');
  r.document.getElementById('plain').focus();
  assert.equal(r.core.keys.zoneOf(r.core.frame.keyTable(), r.core.frame.lastFocus()), 'page');
  r.window.close();
});

await ok('focus_moving_into_the_menu_or_a_modal_does_not_overwrite_it', async () => {
  const r = rig([dialTheme('one')]);
  await r.mount('one');
  const deep = r.document.getElementById('deep');
  deep.focus();
  r.core.frame.openMenu();
  assert.ok(r.document.activeElement.closest('#fx-menu'), 'the focus is on a row of the menu');
  assert.equal(r.core.frame.lastFocus(), deep);
  r.core.frame.closeMenu();
  assert.equal(r.document.activeElement.id, 'fx-button');
  assert.equal(r.core.frame.lastFocus(), deep, 'nor does the menu button');
  const pop = r.document.createElement('div');
  pop.innerHTML = '<button id="in">in</button>';
  const modal = r.core.modal.open({ el: pop });
  assert.equal(r.document.activeElement.id, 'in');
  assert.equal(r.core.frame.lastFocus(), deep, 'a modal is not the theme');
  modal.close();
  assert.equal(r.core.frame.lastFocus(), deep);
  r.window.close();
});

await ok('a_focus_that_leaves_to_nothing_clears_the_last_focus', async () => {
  const r = rig([dialTheme('one')]);
  await r.mount('one');
  const deep = r.document.getElementById('deep');
  deep.focus();
  // a click on empty ground: the focus leaves the element and goes to nothing
  deep.blur();
  assert.equal(r.document.activeElement, r.document.body);
  assert.equal(r.core.frame.lastFocus(), null, 'the person has left the widget');
  assert.deepEqual(r.core.keys.zonesOf(r.core.frame.keyTable(), r.core.frame.lastFocus()), []);
  deep.focus();
  assert.equal(r.core.frame.lastFocus(), deep, 'and the next focus is recorded again');
  // the window losing the focus tells the element so, and the element keeps the focus
  deep.dispatchEvent(new r.window.FocusEvent('focusout', { bubbles: true, relatedTarget: null }));
  assert.equal(r.document.activeElement, deep);
  assert.equal(r.core.frame.lastFocus(), deep, 'another window in front is not the widget left');
  // to another control of the theme, and on to the menu's button: the last one of the theme stays
  const rim = r.document.getElementById('rim');
  rim.focus();
  assert.equal(r.core.frame.lastFocus(), rim);
  r.document.getElementById('fx-button').focus();
  assert.equal(r.core.frame.lastFocus(), rim, 'the focus went somewhere, not to nothing');
  r.window.close();
});

await ok('an_element_that_left_the_page_is_no_last_focus_and_a_theme_switch_clears_it', async () => {
  const r = rig([dialTheme('one'), dialTheme('two')]);
  const one = await r.mount('one');
  r.document.getElementById('deep').focus();
  r.document.getElementById('deep').remove();
  assert.equal(r.core.frame.lastFocus(), null, 'drawn away');
  const plain = r.document.getElementById('plain');
  plain.focus();
  assert.equal(r.core.frame.lastFocus(), plain);
  // a switch that keeps the old theme's element in the page a moment longer still forgets it
  const kept = r.document.body.appendChild(r.document.createElement('div'));
  await one.unmount();
  kept.append(plain);
  await r.mount('two');
  assert.equal(plain.isConnected, true);
  assert.equal(r.core.frame.lastFocus(), null, 'the theme on show has had no focus yet');
  r.window.close();
});

await ok('the_help_key_opens_help_only_while_no_sheet_stands_over_the_page', async () => {
  const r = rig([dialTheme('one')]);
  await r.mount('one');
  const press = (key = '?') => r.key(key, { on: r.document.getElementById('plain') });
  const open = () => r.core.router.state().popup;
  const close = async () => {
    r.core.router.close();
    await r.settle();
    await r.settle();
  };
  press('h');
  assert.equal(open(), null, 'another key');
  // a sheet the theme draws itself and marks modal
  const sheet = r.document.getElementById('theme').appendChild(r.document.createElement('div'));
  sheet.setAttribute('aria-modal', 'true');
  assert.equal(press().defaultPrevented, false, 'the key goes on to the sheet');
  assert.equal(open(), null, 'under a theme\'s own sheet the key is the sheet\'s');
  sheet.remove();
  // a modal of the stack
  const modal = r.core.modal.open({ el: r.document.createElement('div') });
  press();
  assert.equal(open(), null, 'nor over a modal of the stack');
  modal.close();
  assert.equal(press().defaultPrevented, true);
  assert.equal(open(), 'help', 'with nothing over the page it opens Help');
  await close();
  // the open menu is no sheet: `?` is still the frame's there, as `T` is
  r.core.frame.openMenu();
  press();
  assert.equal(open(), 'help');
  await close();
  r.window.close();
});

await ok('an_element_that_is_hidden_is_no_last_focus', async () => {
  const r = rig([dialTheme('one')]);
  await r.mount('one');
  const deep = r.document.getElementById('deep');
  const dial = r.document.querySelector('.dial');
  deep.focus();
  assert.equal(r.core.frame.lastFocus(), deep);
  // a sheet closed over it: the focus went to the menu's button, which is not recorded
  dial.hidden = true;
  r.document.getElementById('fx-button').focus();
  assert.equal(r.core.frame.lastFocus(), null, 'inside a closed sheet (`hidden` on an ancestor)');
  dial.hidden = false;
  assert.equal(r.core.frame.lastFocus(), deep, 'shown again, it is still the last focus');
  deep.hidden = true;
  assert.equal(r.core.frame.lastFocus(), null, '`hidden` on the element itself');
  deep.hidden = false;
  dial.style.display = 'none';
  assert.equal(r.core.frame.lastFocus(), null, '`display: none` on an ancestor');
  dial.style.display = '';
  // where the browser can say whether an element is drawn, its answer is the one
  let asked = null;
  deep.checkVisibility = (options) => { asked = options; return false; };
  assert.equal(r.core.frame.lastFocus(), null, 'the browser says it is not drawn (a style sheet\'s rule)');
  assert.deepEqual(asked, { visibilityProperty: true });
  deep.checkVisibility = () => true;
  assert.equal(r.core.frame.lastFocus(), deep);
  r.window.close();
});

console.log(`core-mounted.mjs: ${n} passed`);
