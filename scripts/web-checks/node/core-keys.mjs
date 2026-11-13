/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/keys.js, a theme's key table and its binding: an entry with `run` is bound
// and fires once for its key, one whose `when` is false is neither bound nor listed, nothing fires
// while a modal of the real stack is open, in a field that takes typing, with Ctrl, Alt or Meta, or
// for a key the menu owns; a held key fires only for an entry that repeats; the mount's signal
// unbinds; two tables do not hear each other's keys across a theme switch; a table that binds a
// key the frame reserves, or holds a malformed entry, is refused at bind time; `listed` gives the
// entries that apply in table order and `zoneOf` the zone an element falls in. A jsdom page (apt
// `node-jsdom`) with the real stack of modals in front, as the frame puts it.
// Run: node core-keys.mjs
import assert from 'node:assert/strict';
import { page } from './dom.mjs';
import { createModals } from '../../../bins/svolderd/web/core/modal.js';
import {
  capsOf, createKeys, FRAME_KEYS, keysOf, listed, RESERVED, zoneOf, zonesOf,
} from '../../../bins/svolderd/web/core/keys.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
const warned = [];
console.warn = (...args) => warned.push(args);

const BODY = '<div id="frame"><span class="fxmenu"><button id="fx-button" class="fxbtn">Menu</button>'
  + '<span class="fxpop" id="fx-menu"><button id="row">Row</button></span></span></div>'
  + '<div id="theme"><button id="knob">Volume</button><input id="name"><textarea id="note"></textarea>'
  + '<select id="pick"><option>a</option></select><div id="edit" contenteditable="true">x</div>'
  + '<div class="dial"><div class="scale"><button id="deep">deep</button></div></div></div>'
  + '<div id="toasts"></div>';

/** A page with the real stack in front of every key, as the frame's listener puts it, and the keys
 * of a core whose viewer may write (`can.write`) and whose menu owns the keys of `#fx-menu`. */
function rig() {
  const p = page(BODY);
  const { window, document } = p;
  globalThis.AbortController = window.AbortController;
  const modals = createModals({ document, fallback: () => document.getElementById('fx-button') });
  window.addEventListener('keydown', (event) => { modals.key(event); }, { capture: true });
  const can = { write: true };
  const keys = createKeys({
    document,
    menuOwns: (event) => Boolean(event.target.closest?.('.fxpop')),
    base: () => ({ canWrite: can.write }),
  });
  const el = (id) => document.getElementById(id);
  /** Press `name` on `on` (the body by default). */
  const press = (name, on = document.body, init = {}) => p.key(name, { on, ...init });
  return { ...p, modals, keys, can, el, press };
}

/** A table whose run entries count their calls in `ran`. */
function table(ran = {}) {
  const count = (id) => (event) => { ran[id] = (ran[id] ?? 0) + 1; ran.last = event; };
  return [
    { id: 'm', key: 'm', zone: 'page', words: 'Mute', when: (ctx) => ctx.canWrite, run: count('m') },
    { id: 'arrows', key: ['←', '→'], keys: ['ArrowLeft', 'ArrowRight'], zone: 'page',
      words: 'Volume one step', repeat: true, run: count('arrows') },
    { id: 'a', key: 'A', keys: ['A', 'a'], zone: 'page', words: 'Arrange sections', run: count('a') },
    { id: 'v', key: 'v', zone: 'page', words: 'The look', when: (ctx) => ctx.viz === true,
      run: count('v') },
    { id: 'home', key: 'Home', zone: '.dial .scale', words: 'To the lowest' },
    { id: 'step', key: ['↑', '↓'], keys: ['ArrowUp', 'ArrowDown'], zone: '.dial', words: 'One step' },
  ];
}

await ok('an_entry_with_run_fires_once_for_its_key_and_is_given_the_event', async () => {
  const r = rig();
  const ran = {};
  const stop = new AbortController();
  r.keys.bind(table(ran), { signal: stop.signal });
  const event = r.press('m');
  assert.equal(ran.m, 1);
  assert.equal(ran.last, event, 'run is handed the event');
  r.press('m', r.el('knob'));
  assert.equal(ran.m, 2, 'a key on a button of the page is the page\'s too');
  r.press('x');
  r.press('M');
  assert.deepEqual([ran.m, ran.a, ran.arrows], [2, undefined, undefined], 'only its own key');
  stop.abort();
});

await ok('an_entry_answers_to_each_key_of_its_keys_list', async () => {
  const r = rig();
  const ran = {};
  const stop = new AbortController();
  r.keys.bind(table(ran), { signal: stop.signal });
  r.press('ArrowLeft');
  r.press('ArrowRight');
  r.press('a');
  r.press('A', r.document.body, { shiftKey: true });
  assert.deepEqual([ran.arrows, ran.a], [2, 2]);
  r.press('←');
  assert.equal(ran.arrows, 2, 'the label is not a key once the keys are named');
  assert.deepEqual(keysOf({ key: 'm' }), ['m']);
  assert.deepEqual(keysOf({ key: 'A', keys: ['A', 'a'] }), ['A', 'a']);
  assert.deepEqual(keysOf({ key: ['Home', 'End'] }), ['Home', 'End'], 'each keycap is its own key');
  assert.deepEqual(keysOf({ key: ['←', '→'], keys: ['ArrowLeft', 'ArrowRight'] }),
    ['ArrowLeft', 'ArrowRight']);
  assert.deepEqual(capsOf({ key: 'm' }), ['m']);
  assert.deepEqual(capsOf({ key: ['Page Up', 'Page Down'] }), ['Page Up', 'Page Down']);
  stop.abort();
});

await ok('an_entry_whose_when_is_false_is_neither_bound_nor_listed', async () => {
  const r = rig();
  const ran = {};
  const stop = new AbortController();
  const t = table(ran);
  const facts = { viz: false };
  r.keys.bind(t, { signal: stop.signal, ctx: () => facts });
  r.press('v');
  assert.equal(ran.v, undefined, 'not bound');
  assert.deepEqual(r.keys.listed(t).map((e) => e.id), ['m', 'arrows', 'a', 'home', 'step'], 'not listed');
  facts.viz = true;
  r.press('v');
  assert.equal(ran.v, 1, 'bound as soon as it applies: when is asked at each key');
  assert.deepEqual(r.keys.listed(t).map((e) => e.id), ['m', 'arrows', 'a', 'v', 'home', 'step']);
  // the core's own facts are under the theme's: a guest has no `m`
  r.can.write = false;
  r.press('m');
  assert.equal(ran.m, undefined);
  assert.deepEqual(r.keys.listed(t).map((e) => e.id), ['arrows', 'a', 'v', 'home', 'step']);
  stop.abort();
});

await ok('listed_gives_the_entries_that_apply_in_table_order_as_they_are', async () => {
  const t = table();
  const all = listed(t, { canWrite: true, viz: true });
  assert.deepEqual(all, t, 'every entry, the very objects, in order');
  assert.deepEqual(listed(t, {}).map((e) => e.id), ['arrows', 'a', 'home', 'step']);
  assert.deepEqual(listed(t).map((e) => e.id), ['arrows', 'a', 'home', 'step'], 'no ctx is an empty one');
  assert.deepEqual(listed([]), []);
  // a `when` that throws is not listed and is said in the console
  const before = warned.length;
  assert.deepEqual(listed([{ id: 'x', key: 'x', zone: 'page', words: 'X', when() { throw new Error('no'); } }]), []);
  assert.equal(warned.length, before + 1);
});

await ok('listed_with_a_ctx_given_uses_it_and_without_one_the_bound_tables_own', async () => {
  const r = rig();
  const stop = new AbortController();
  const t = table();
  assert.deepEqual(r.keys.listed(t).map((e) => e.id), ['m', 'arrows', 'a', 'home', 'step'],
    'a table that is not bound is listed with the core\'s facts');
  r.keys.bind(t, { signal: stop.signal, ctx: () => ({ viz: true }) });
  assert.equal(r.keys.listed(t).length, 6);
  assert.deepEqual(r.keys.listed(t, { canWrite: false }).map((e) => e.id), ['arrows', 'a', 'home', 'step'],
    'a ctx given is the whole ctx');
  stop.abort();
  assert.equal(r.keys.listed(t).length, 5, 'unbound: the theme\'s facts went with the mount');
});

await ok('nothing_fires_while_a_modal_is_open', async () => {
  const r = rig();
  const ran = {};
  const stop = new AbortController();
  r.keys.bind(table(ran), { signal: stop.signal });
  const pop = r.document.createElement('div');
  pop.innerHTML = '<button id="in">in</button>';
  const modal = r.modals.open({ el: pop });
  r.press('m', r.el('in'));
  r.press('m');
  r.press('ArrowLeft', r.el('in'));
  assert.deepEqual(ran, {}, 'the top modal owns every key');
  modal.close();
  r.press('m');
  assert.equal(ran.m, 1, 'and the page has its keys again once it is closed');
  stop.abort();
});

await ok('nothing_fires_in_a_field_that_takes_typing', async () => {
  const r = rig();
  const ran = {};
  const stop = new AbortController();
  r.keys.bind(table(ran), { signal: stop.signal });
  for (const id of ['name', 'note', 'pick', 'edit']) {
    r.press('m', r.el(id));
    r.press('ArrowLeft', r.el(id));
    r.press('a', r.el(id));
  }
  assert.deepEqual(ran, {}, 'an input, a text area, a select and an editable element type');
  stop.abort();
});

await ok('nothing_fires_with_ctrl_alt_or_meta_and_shift_is_left_to_the_key', async () => {
  const r = rig();
  const ran = {};
  const stop = new AbortController();
  r.keys.bind(table(ran), { signal: stop.signal });
  for (const mods of [{ ctrlKey: true }, { altKey: true }, { metaKey: true }]) {
    r.press('m', r.document.body, mods);
    r.press('ArrowRight', r.document.body, mods);
  }
  assert.deepEqual(ran, {});
  r.press('m', r.document.body, { shiftKey: true });
  assert.equal(ran.m, 1);
  stop.abort();
});

await ok('a_key_the_menu_owns_is_not_the_pages', async () => {
  const r = rig();
  const ran = {};
  const stop = new AbortController();
  r.keys.bind(table(ran), { signal: stop.signal });
  r.press('m', r.el('row'));
  r.press('ArrowLeft', r.el('row'));
  assert.deepEqual(ran, {});
  r.press('m', r.el('fx-button'));
  assert.equal(ran.m, 1, 'the closed button\'s other keys reach the page');
  stop.abort();
});

await ok('a_held_key_fires_again_only_for_an_entry_that_repeats', async () => {
  const r = rig();
  const ran = {};
  const stop = new AbortController();
  r.keys.bind(table(ran), { signal: stop.signal });
  r.press('m', r.document.body, { repeat: true });
  r.press('a', r.document.body, { repeat: true });
  r.press('ArrowLeft', r.document.body, { repeat: true });
  assert.deepEqual([ran.m, ran.a, ran.arrows], [undefined, undefined, 1], 'a held m would flap the mute');
  stop.abort();
});

await ok('a_key_a_widget_stops_does_not_reach_the_table', async () => {
  const r = rig();
  const ran = {};
  const stop = new AbortController();
  r.keys.bind(table(ran), { signal: stop.signal });
  r.el('knob').addEventListener('keydown', (event) => event.stopPropagation());
  r.press('ArrowLeft', r.el('knob'));
  assert.equal(ran.arrows, undefined, 'the dial took its arrow');
  r.press('ArrowLeft', r.el('deep'));
  assert.equal(ran.arrows, 1);
  stop.abort();
});

await ok('the_first_entry_that_applies_takes_the_key_and_a_run_that_throws_stops_nothing', async () => {
  const r = rig();
  const ran = [];
  const stop = new AbortController();
  r.keys.bind([
    { id: 'one', key: 'x', zone: 'page', words: 'One', when: (ctx) => ctx.one, run: () => ran.push('one') },
    { id: 'two', key: 'x', zone: 'page', words: 'Two', run: () => ran.push('two') },
    { id: 'three', key: 'x', zone: 'page', words: 'Three', run: () => ran.push('three') },
    { id: 'throws', key: 'y', zone: 'page', words: 'Throws', run() { throw new Error('boom'); } },
  ], { signal: stop.signal, ctx: () => ({ one: false }) });
  r.press('x');
  assert.deepEqual(ran, ['two']);
  const before = warned.length;
  r.press('y');
  assert.equal(warned.length, before + 1, 'said in the console');
  r.press('x');
  assert.deepEqual(ran, ['two', 'two'], 'and the keys still work');
  stop.abort();
});

await ok('the_signal_unbinds', async () => {
  const r = rig();
  const ran = {};
  const stop = new AbortController();
  r.keys.bind(table(ran), { signal: stop.signal });
  r.press('m');
  stop.abort();
  r.press('m');
  r.press('ArrowLeft');
  assert.deepEqual([ran.m, ran.arrows], [1, undefined]);
  assert.throws(() => r.keys.bind(table(), {}), /signal/, 'a table is bound for a mount: no signal, no binding');
});

await ok('two_tables_do_not_leak_into_each_other_across_a_theme_switch', async () => {
  const r = rig();
  const one = {};
  const two = {};
  const first = new AbortController();
  r.keys.bind(table(one), { signal: first.signal, ctx: () => ({ viz: true }) });
  r.press('v');
  // the switch: the theme on show unmounts (its signal aborts), the next one mounts
  first.abort();
  const second = new AbortController();
  const next = [{ id: 'v', key: 'v', zone: 'page', words: 'Another v', run: () => { two.v = (two.v ?? 0) + 1; } }];
  r.keys.bind(next, { signal: second.signal });
  r.press('v');
  r.press('m');
  assert.deepEqual(one, { v: 1, last: one.last }, 'the old table hears nothing more');
  assert.deepEqual(two, { v: 1 }, 'and the new one only its own keys');
  assert.deepEqual(r.keys.listed(next).map((e) => e.words), ['Another v']);
  second.abort();
  r.press('v');
  assert.equal(two.v, 1);
});

await ok('a_table_that_binds_a_key_the_frame_reserves_is_refused_at_bind_time', async () => {
  const r = rig();
  const stop = new AbortController();
  assert.deepEqual(RESERVED, ['T', 't', '?', ' '], 'what the frame keeps for itself');
  for (const key of RESERVED) {
    assert.throws(
      () => r.keys.bind([{ id: 'mine', key, zone: 'page', words: 'Mine', run() {} }], { signal: stop.signal }),
      /reserves/, JSON.stringify(key),
    );
  }
  assert.throws(
    () => r.keys.bind([{ id: 'mine', key: 'Themes', keys: ['x', 't'], zone: 'page', words: 'Mine', run() {} }],
      { signal: stop.signal }),
    /reserves/, 'by any of its keys',
  );
  // nothing of a refused table is bound
  let ran = 0;
  assert.throws(() => r.keys.bind([
    { id: 'fine', key: 'x', zone: 'page', words: 'Fine', run: () => { ran++; } },
    { id: 'mine', key: 'T', zone: 'page', words: 'Mine', run() {} },
  ], { signal: stop.signal }), /reserves/);
  r.press('x');
  assert.equal(ran, 0);
  // an entry the theme's own listener answers (`own`) may name a reserved key: `bind` does not
  // dispatch it
  r.keys.bind([{ id: 'card', key: '?', zone: 'page', words: 'This list', own: true }],
    { signal: stop.signal });
  stop.abort();
});

await ok('a_key_of_the_page_is_bound_by_its_run_or_owned_by_the_themes_listener', async () => {
  const r = rig();
  const stop = new AbortController();
  // listed under the page's keys and bound by nothing: refused, and the error says the two ways
  assert.throws(
    () => r.keys.bind([{ id: 'ghost', key: 'g', zone: 'page', words: 'Nothing' }], { signal: stop.signal }),
    /entry 0 \(ghost\).*`run`.*`own: true`/,
  );
  assert.throws(
    () => r.keys.bind([{ id: 'themes', key: 'T', zone: 'page', words: 'Themes' }], { signal: stop.signal }),
    /`own: true`/, 'a reserved key too',
  );
  // `own: true` says a listener of the theme's own handles the key: listed like any other, never
  // dispatched by `bind`
  const t = [
    { id: 'leave', key: 'Esc', keys: ['Escape'], zone: 'page', words: 'Leave', own: true },
    { id: 'back', key: 'Esc', keys: ['Escape'], zone: '.dial', words: 'Put it back', own: true },
    { id: 'm', key: 'm', zone: 'page', words: 'Mute', run() {} },
  ];
  r.keys.bind(t, { signal: stop.signal });
  r.press('Escape');
  assert.deepEqual(r.keys.listed(t).map((e) => e.id), ['leave', 'back', 'm']);
  assert.deepEqual(listed(t, {}).filter((e) => e.own).map((e) => e.id), ['leave', 'back']);
  // one or the other, and `own` is a boolean
  for (const [entry, why] of [
    [{ id: 'x', key: 'x', zone: 'page', words: 'Both', own: true, run() {} }, /`own`/],
    [{ id: 'x', key: 'x', zone: 'page', words: 'A word', own: 'yes' }, /`own`/],
    [{ id: 'x', key: 'Curve', zone: '.dial', words: 'A gesture', gesture: true, own: true }, /gesture/],
  ]) {
    assert.throws(() => r.keys.bind([entry], { signal: stop.signal }), why, JSON.stringify(entry));
  }
  stop.abort();
});

await ok('a_malformed_entry_is_refused_at_bind_time', async () => {
  const r = rig();
  const stop = new AbortController();
  const bad = [
    [{ id: 'x', zone: 'page', words: 'No key' }, /`key`/],
    [{ id: 'x', key: '', zone: 'page', words: 'Empty key' }, /`key`/],
    [{ id: 'x', key: [], zone: 'page', words: 'No keycap' }, /`key`/],
    [{ id: 'x', key: ['Home', ''], zone: 'page', words: 'An empty keycap' }, /`key`/],
    [{ id: 'x', key: ['Home', 7], zone: 'page', words: 'A keycap that is no name' }, /`key`/],
    [{ id: 'x', key: { cap: 'x' }, zone: 'page', words: 'Neither a keycap nor a list' }, /`key`/],
    [{ id: 'x', key: '← →', keys: ['ArrowLeft', 'ArrowRight'], zone: 'page', words: 'Two in one' },
      /one keycap/],
    [{ id: 'x', key: ['↑ ↓', 'Home'], keys: ['Home'], zone: 'page', words: 'Two in one of a list' },
      /one keycap/],
    // several keycaps in one string of names: only `Page Up` and `Page Down` are two words
    [{ id: 'x', key: 'Home End', zone: '.dial', words: 'Two names in one' }, /one keycap/],
    [{ id: 'x', key: 'Enter Space', zone: '.dial', words: 'Two names in one' }, /one keycap/],
    [{ id: 'x', key: 'PgUp PgDn', zone: '.dial', words: 'Two short names in one' }, /one keycap/],
    [{ id: 'x', key: ['Page Up', 'Home End'], zone: '.dial', words: 'One of a list' },
      /one keycap/],
    [{ id: 'x', key: 'Page  Up', zone: '.dial', words: 'Not the keycap\'s name' }, /one keycap/],
    // a list of keys that names none never fires
    [{ id: 'x', key: 'x', keys: [], zone: 'page', words: 'No key at all', run() {} }, /`keys`/],
    [{ id: 'x', key: 'x', keys: [], zone: '.dial', words: 'No key at all' }, /`keys`/],
    // a widget's zone that is the whole page under another name
    [{ id: 'x', key: 'x', zone: 'body', words: 'The page as a widget' }, /`zone`/],
    [{ id: 'x', key: 'x', zone: 'html', words: 'The page as a widget' }, /`zone`/],
    [{ id: 'x', key: 'x', zone: ':root', words: 'The page as a widget' }, /`zone`/],
    [{ id: 'x', key: 'x', zone: '*', words: 'Everything' }, /`zone`/],
    [{ id: 'x', key: 'x', zone: ' page', words: 'The page, misspelt' }, /`zone`/],
    [{ id: 'x', key: 'x', zone: '.dial ', words: 'A space after' }, /`zone`/],
    [{ key: 'x', zone: 'page', words: 'No id' }, /`id`/],
    [{ id: '', key: 'x', zone: 'page', words: 'Empty id' }, /`id`/],
    [{ id: 7, key: 'x', zone: 'page', words: 'An id that is no name' }, /`id`/],
    [{ id: 'x', key: 'x', words: 'No zone' }, /zone/],
    [{ id: 'x', key: 'x', zone: 'page' }, /words/],
    [{ id: 'x', key: 'x', zone: 'page', words: 'X', run: 'no' }, /run/],
    [{ id: 'x', key: 'x', zone: 'page', words: 'X', when: true }, /when/],
    [{ id: 'x', key: 'x', zone: 'page', words: 'X', keys: 'x' }, /keys/],
    [{ id: 'x', key: 'x', zone: '.dial', words: 'A widget key with run', run() {} }, /page/],
    [{ id: 'x', key: 'Curve', zone: '.dial', words: 'X', gesture: 'yes' }, /gesture/],
    [{ id: 'x', key: 'Curve', zone: 'page', words: 'X', gesture: true, run() {} }, /gesture/],
    [{ id: 'x', key: 'Curve', zone: '.dial', words: 'X', gesture: true, keys: ['c'] }, /gesture/],
  ];
  for (const [entry, why] of bad) {
    assert.throws(() => r.keys.bind([entry], { signal: stop.signal }), why, JSON.stringify(entry));
  }
  assert.throws(() => r.keys.bind({}, { signal: stop.signal }), /table/);
  // a key of the page that could never fire: its keycap is no `event.key` and no `keys` say which
  for (const entry of [
    { id: 'x', key: 'Space', zone: 'page', words: 'A keycap\'s name', run() {} },
    { id: 'x', key: 'Esc', zone: 'page', words: 'A keycap\'s name', run() {} },
    { id: 'x', key: ['←', '→'], zone: 'page', words: 'Arrows as drawn', run() {} },
    { id: 'x', key: 'Page Up', zone: 'page', words: 'Two words', run() {} },
    { id: 'x', key: 'A', keys: ['A', 'Left'], zone: 'page', words: 'One of its keys', run() {} },
  ]) {
    assert.throws(() => r.keys.bind([entry], { signal: stop.signal }), /no `event\.key`/,
      JSON.stringify(entry));
  }
  // every key a `run` may name: one character, or the browser's name for a key
  r.keys.bind(['x', '0', '/', 'Enter', 'Escape', 'Home', 'End', 'PageUp', 'PageDown', 'ArrowUp',
    'ArrowDown', 'ArrowLeft', 'ArrowRight', 'Backspace', 'Delete', 'Insert', 'F2', 'F12',
  ].map((key) => ({ id: `k-${key}`, key: 'x', keys: [key], zone: 'page', words: 'X', run() {} })),
  { signal: stop.signal });
  // two entries of one table with the same id, and the error names it
  assert.throws(() => r.keys.bind([
    { id: 'step', key: 'x', zone: '.dial', words: 'One' },
    { id: 'other', key: 'y', zone: '.dial', words: 'Two' },
    { id: 'step', key: 'z', zone: '.dial', words: 'Three' },
  ], { signal: stop.signal }), /entry 2 \(step\).*another entry has it/);
  // a keycap of two words, a list of keycaps and a gesture's name are all fine
  let ran = 0;
  r.keys.bind([
    { id: 'pages', key: ['Page Up', 'Page Down'], keys: ['PageUp', 'PageDown'], zone: '.dial',
      words: 'Ten steps' },
    { id: 'ends', key: ['Home', 'End'], zone: 'page', words: 'The ends', run: () => { ran++; } },
    { id: 'curve', key: 'Curve', gesture: true, zone: '.dial', words: 'Drag a point' },
    // a gesture's name is free text, and a gesture of the whole page needs no `run`
    { id: 'swipe', key: 'Swipe a page', gesture: true, zone: 'page', words: 'The next page' },
  ], { signal: stop.signal });
  r.press('Home');
  r.press('End');
  assert.equal(ran, 2, 'a list of keycaps without `keys` answers to each');
  stop.abort();
});

await ok('zone_of_is_the_first_entrys_zone_that_holds_the_element_else_the_page', async () => {
  const r = rig();
  const t = table();
  assert.equal(zoneOf(t, r.el('deep')), '.dial .scale', 'a nested element, by an ancestor');
  assert.equal(zoneOf(t, r.document.querySelector('.scale')), '.dial .scale', 'the widget itself');
  assert.equal(zoneOf(t, r.document.querySelector('.dial')), '.dial');
  assert.equal(zoneOf(t, r.el('knob')), 'page');
  assert.equal(zoneOf(t, null), 'page');
  assert.equal(zoneOf([], r.el('deep')), 'page');
  // the innermost zone, whatever order the table names them in
  const wide = [
    { id: 'a', key: 'a', zone: '.dial', words: 'A' },
    { id: 'b', key: 'b', zone: '.dial .scale', words: 'B' },
  ];
  assert.equal(zoneOf(wide, r.el('deep')), '.dial .scale');
  assert.equal(r.keys.zoneOf, zoneOf, 'the core gives the same function');
});

await ok('zones_of_is_every_zone_that_holds_the_element_outer_to_inner', async () => {
  const r = rig();
  const t = [
    ...table(),
    { id: 'theme-close', key: 'Esc', keys: ['Escape'], zone: '#theme', words: 'Close the sheet' },
    { id: 'deep-enter', key: 'Enter', zone: '#deep, #knob', words: 'Choose' },
    { id: 'again', key: 'End', zone: '.dial', words: 'A second key of the dial' },
  ];
  assert.deepEqual(zonesOf(t, r.el('deep')), ['#theme', '.dial', '.dial .scale', '#deep, #knob'],
    'a scale in a sheet: the sheet\'s keys, the dial\'s, the scale\'s and the button\'s own');
  assert.deepEqual(zonesOf(t, r.document.querySelector('.scale')), ['#theme', '.dial', '.dial .scale']);
  assert.deepEqual(zonesOf(t, r.document.querySelector('.dial')), ['#theme', '.dial'], 'each zone once');
  assert.deepEqual(zonesOf(t, r.el('knob')), ['#theme', '#deep, #knob']);
  assert.deepEqual(zonesOf(t, r.el('row')), [], 'outside every zone: the page alone');
  assert.deepEqual(zonesOf(t, null), []);
  assert.deepEqual(zonesOf([], r.el('deep')), []);
  // two zones on one element: the table's first is the inner one, as `zoneOf` has it
  const same = [
    { id: 'a', key: 'a', zone: '.scale', words: 'A' },
    { id: 'b', key: 'b', zone: '.dial .scale', words: 'B' },
  ];
  assert.deepEqual(zonesOf(same, r.el('deep')), ['.dial .scale', '.scale']);
  assert.equal(zoneOf(same, r.el('deep')), '.scale');
  assert.equal(zoneOf(t, r.el('deep')), '#deep, #knob', 'zoneOf is the last of them');
  // what the Help popup lists for the element: the entries of those zones
  const here = zonesOf(t, r.el('deep')).flatMap((zone) => t.filter((e) => e.zone === zone));
  assert.deepEqual(here.map((e) => e.id), ['theme-close', 'step', 'again', 'home', 'deep-enter']);
  assert.equal(r.keys.zonesOf, zonesOf, 'the core gives the same function');
});

await ok('the_cores_keys_hold_the_frames_own_list_beside_the_tables_hands', async () => {
  const r = rig();
  assert.ok(!Array.isArray(r.keys), 'the hands are an object: no array method drops or shadows one');
  assert.deepEqual(Object.keys(r.keys).sort(), ['bind', 'frame', 'listed', 'zoneOf', 'zonesOf']);
  assert.equal(r.keys.frame, FRAME_KEYS, "the frame's own list");
  assert.deepEqual(FRAME_KEYS.map((k) => [k.id, k.key, k.words, k.zone]),
    [['themes', 'T', 'Themes', 'page'], ['play', 'Space', 'Play or pause', 'page'],
      ['help', '?', 'Help', 'page']]);
  assert.deepEqual(listed(r.keys.frame, { canWrite: false }).map((k) => k.key), ['T'],
    'Space plays and pauses for a viewer who may write');
  assert.deepEqual(listed(r.keys.frame, { canWrite: true }).map((k) => k.key), ['T', 'Space']);
  assert.deepEqual(listed(r.keys.frame, { canWrite: true, help: true }).map((k) => k.key),
    ['T', 'Space', '?'], '`?` is listed once the page has a Help popup to open');
  assert.deepEqual(r.keys.listed(r.keys.frame).map((k) => k.id), ['themes', 'play'],
    "the core's hand reads the list with the core's facts: a writer, on a page with no Help");
});

console.log(`core-keys.mjs: ${n} passed`);
