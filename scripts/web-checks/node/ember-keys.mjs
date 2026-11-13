/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Ember's keys (themes/ember/keys.js over its key table,
// themes/ember/key-table.js) with the core's real binding (core/keys.js): what each key of the
// page does and when it does nothing (a guest, a lost link, a text field, a modifier, the menu,
// a control that has arrows of its own, a held key), that `?` is none of Ember's keys (the frame
// opens the Help popup on it: Ember has no key card and no listener for it), and the table
// itself (every key of the page has a `run`, each widget's keys name a zone). A jsdom page (apt
// `node-jsdom`) with stand-ins for the dial, the balance slider, the Flat option and the presets.
// Run: node ember-keys.mjs
import assert from 'node:assert/strict';
import { page } from './dom.mjs';
import { createKeys, keysOf, listed } from '../../../bins/svolderd/web/core/keys.js';
import { ember } from '../../../bins/svolderd/web/themes/ember/bind.js';
import { KEYS } from '../../../bins/svolderd/web/themes/ember/key-table.js';
import { installKeys, TABLE } from '../../../bins/svolderd/web/themes/ember/keys.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const BODY = '<span class="fxmenu"><button class="fxbtn" id="fx-button">Menu</button>'
  + '<span class="fxpop"><button id="row">Row</button></span></span>'
  + '<div id="theme"><main id="screen"><div class="view">'
  + '<sv-knob data-mute="1"><button id="dial" class="dial">dial</button></sv-knob>'
  + '<sv-slider data-key="balance"><button id="slide">slide</button></sv-slider>'
  + '<div class="seg"><button id="seg">seg</button></div>'
  + '<div id="part-tone"><button class="opt" data-mode="flat" aria-pressed="false">Flat</button></div>'
  + '<div class="presets"><button class="pre" id="first">1</button>'
  + '<button class="pre" id="second" aria-pressed="true">2</button></div>'
  + '<div class="bands"><button id="band">band</button></div><div class="bedit"><button id="edit">v</button></div>'
  + '<input id="name"><textarea id="note"></textarea><select id="pick"><option>a</option></select>'
  + '<button id="plain">plain</button><section class="p-viz"></section>'
  + '</div></main></div>';

/** Ember's keys installed on a page whose viewer may write (`can.write`), with what the keys
 * reach noted in `did`. `stop()` ends the mount. */
function rig() {
  const p = page(BODY);
  const { window, document } = p;
  globalThis.window = window;
  globalThis.document = document;
  globalThis.AbortController = window.AbortController;
  const did = [];
  const can = { write: true };
  const knob = document.querySelector('sv-knob');
  knob.step = (by) => did.push(`step ${by}`);
  knob.toggleMute = () => did.push('mute');
  const menuOwns = (event) => Boolean(event.target.closest?.('.fxpop'));
  const mount = new window.AbortController();
  ember.signal = mount.signal;
  ember.core = {
    actions: {
      canWrite: () => can.write,
      setControl: (key, body) => did.push(`control ${key} ${JSON.stringify(body)}`),
      setMode: (mode) => did.push(`mode ${mode}`),
    },
    frame: { menuOwns, inMenu: () => false },
    keys: createKeys({ document, menuOwns, base: () => ({ canWrite: can.write }) }),
  };
  installKeys();
  const el = (id) => document.getElementById(id);
  const press = (name, on = document.body, init = {}) => p.key(name, { on, ...init });
  return {
    ...p, did, can, el, press, knob,
    stop() {
      mount.abort();
      ember.core = null;
      ember.signal = null;
      window.close();
    },
  };
}

await ok('the_arrows_step_the_volume_one_step_and_a_held_arrow_goes_on', async () => {
  const r = rig();
  const right = r.press('ArrowRight');
  const left = r.press('ArrowLeft', r.el('plain'));
  assert.deepEqual(r.did, ['step 1', 'step -1']);
  assert.equal(right.defaultPrevented && left.defaultPrevented, true, 'the page does not scroll');
  r.press('ArrowRight', r.document.body, { repeat: true });
  assert.deepEqual(r.did, ['step 1', 'step -1', 'step 1'], 'a held arrow repeats');
  r.press('ArrowUp');
  r.press('ArrowDown');
  assert.equal(r.did.length, 3, 'up and down are not the page\'s');
  r.stop();
});

await ok('an_arrow_on_a_control_that_has_arrows_of_its_own_is_that_controls', async () => {
  const r = rig();
  for (const id of ['dial', 'slide', 'seg', 'first', 'band', 'edit']) {
    const event = r.press('ArrowRight', r.el(id));
    assert.equal(event.defaultPrevented, false, id);
  }
  assert.deepEqual(r.did, []);
  r.stop();
});

await ok('m_mutes_0_centres_the_balance_f_goes_flat_and_p_goes_to_the_preset_that_is_on', async () => {
  const r = rig();
  r.press('m');
  r.press('0');
  r.press('f');
  r.press('p');
  assert.deepEqual(r.did, ['mute', 'control balance {"value":0}', 'mode flat']);
  assert.equal(r.document.activeElement, r.el('second'), 'the preset that is on has the focus');
  r.el('second').removeAttribute('aria-pressed');
  r.press('p');
  assert.equal(r.document.activeElement, r.el('first'), 'with none on, the first');
  r.stop();
});

await ok('a_key_whose_control_the_screen_lacks_does_nothing', async () => {
  const r = rig();
  delete r.knob.dataset.mute;
  r.document.querySelector('sv-slider').remove();
  r.document.querySelector('#part-tone .opt').setAttribute('aria-pressed', 'true');
  r.press('m');
  r.press('0');
  r.press('f');
  assert.deepEqual(r.did, [], 'no mute, no balance, Flat already on');
  r.document.querySelector('#part-tone .opt').remove();
  r.knob.remove();
  r.press('f');
  r.press('ArrowRight');
  r.press('m');
  assert.deepEqual(r.did, []);
  r.stop();
});

await ok('a_held_key_acts_once_but_for_the_arrows', async () => {
  const r = rig();
  for (const key of ['m', '0', 'f', 'p']) r.press(key, r.document.body, { repeat: true });
  assert.deepEqual(r.did, [], 'a held m would flap the mute');
  assert.notEqual(r.document.activeElement, r.el('second'));
  r.stop();
});

await ok('no_key_acts_in_a_text_field_with_a_modifier_or_in_the_menu', async () => {
  const r = rig();
  for (const id of ['name', 'note', 'pick', 'row']) {
    for (const key of ['ArrowRight', 'm', '0', 'f', 'p', '?']) r.press(key, r.el(id));
  }
  for (const mods of [{ ctrlKey: true }, { metaKey: true }, { altKey: true }]) {
    for (const key of ['ArrowRight', 'm', '0', 'f', 'p', '?']) r.press(key, r.document.body, mods);
  }
  assert.deepEqual(r.did, []);
  r.press('m', r.el('fx-button'));
  assert.deepEqual(r.did, ['mute'], 'the closed menu button\'s other keys reach the page');
  r.stop();
});

await ok('a_viewer_who_may_not_write_and_a_page_whose_link_is_down_have_no_key_that_writes', async () => {
  const r = rig();
  r.can.write = false;
  for (const key of ['ArrowRight', 'm', '0', 'f', 'p']) r.press(key);
  assert.deepEqual(r.did, [], 'a guest');
  assert.notEqual(r.document.activeElement, r.el('second'));
  r.can.write = true;
  r.document.body.classList.add('lost');
  for (const key of ['ArrowRight', 'm', '0', 'f', 'p']) r.press(key);
  assert.deepEqual(r.did, [], 'the link is down');
  r.document.body.classList.remove('lost');
  r.press('m');
  assert.deepEqual(r.did, ['mute']);
  r.stop();
});

await ok('the_question_mark_is_none_of_embers_keys_and_there_is_no_key_card', async () => {
  const r = rig();
  assert.equal(r.el('keys'), null, 'the page has no key card');
  for (const on of [r.document.body, r.el('plain'), r.el('name')]) {
    const event = r.press('?', on, { shiftKey: true });
    assert.equal(event.defaultPrevented, false);
  }
  assert.deepEqual(r.did, [], 'Ember does nothing on it: the frame opens Help');
  assert.ok(!KEYS.some((e) => [].concat(e.key).includes('?')), 'no entry of the table names it');
  assert.ok(!KEYS.some((e) => 'card' in e), 'and no entry carries a field for a card');
  r.stop();
});

await ok('v_is_listed_only_while_the_screen_has_a_visualizer_card', async () => {
  const r = rig();
  assert.deepEqual(ember.core.keys.listed(TABLE).filter((e) => e.id === 'look').length, 1);
  r.document.querySelector('.p-viz').remove();
  assert.deepEqual(ember.core.keys.listed(TABLE).filter((e) => e.id === 'look'), [], 'no card: no look');
  r.stop();
});

await ok('every_key_of_the_page_has_a_run_and_every_widgets_key_names_its_zone', async () => {
  const page_ = TABLE.filter((e) => e.zone === 'page');
  assert.deepEqual(page_.map((e) => [e.key, typeof e.run]), [
    [['←', '→'], 'function'], ['m', 'function'], ['0', 'function'], ['v', 'function'], ['f', 'function'],
    ['p', 'function'], ['A', 'function'],
  ], 'every key of the page is bound');
  const widgets = TABLE.filter((e) => e.zone !== 'page');
  assert.ok(widgets.length >= 9);
  for (const entry of widgets) {
    assert.equal(entry.run, undefined, `${entry.id}: its widget handles it`);
    assert.ok(entry.zone.length > 0 && entry.words.length > 0, entry.id);
  }
  assert.deepEqual(TABLE.map((e) => e.id), KEYS.map((e) => e.id), 'the bound table is the table, in order');
  assert.equal(new Set(KEYS.map((e) => e.id)).size, KEYS.length, 'each entry has an id of its own');
  assert.deepEqual(keysOf(TABLE.find((e) => e.id === 'arrange')), ['A', 'a']);
});

await ok('the_escape_of_a_drag_applies_only_while_a_point_is_held', async () => {
  const has = (ctx) => listed(KEYS, ctx).some((e) => e.id === 'curve-end');
  assert.equal(has({ canWrite: true, held: true }), true);
  assert.equal(has({ canWrite: true, held: false }), false, 'no point held: Help does not list it');
  assert.equal(has({ canWrite: false, held: true }), false);
  const r = rig();
  assert.equal(ember.core.keys.listed(TABLE).some((e) => e.id === 'curve-end'), false,
    'on the page as it stands no point is held');
  r.stop();
});

await ok('what_applies_follows_the_viewer_and_the_page', async () => {
  const ids = (ctx) => listed(KEYS, ctx).filter((e) => e.zone === 'page').map((e) => e.id);
  assert.deepEqual(ids({ canWrite: true }), ['volume', 'mute', 'centre', 'look', 'flat', 'preset', 'arrange']);
  assert.deepEqual(ids({ canWrite: false }), ['look', 'arrange']);
  assert.deepEqual(ids({ canWrite: true, lost: true }), ['look', 'arrange']);
  assert.deepEqual(ids({ canWrite: true, arranging: true }), ['look', 'arrange']);
  assert.deepEqual(ids({ canWrite: true, viz: false, mute: false, balance: false }),
    ['volume', 'flat', 'preset', 'arrange']);
  assert.deepEqual(ids({ canWrite: true, knob: false, flat: false, presets: false }),
    ['mute', 'centre', 'look', 'arrange']);
});

console.log(`ember-keys.mjs: ${n} passed`);
