/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the frame's reserved key (core/frame.js `reservedKey`) and the two helpers the
// theme Ember's keys ask the menu (core/menu.js `menuOwns`, `inMenu`): `T` is the frame's unless a
// field it would type into has the focus or a modifier is held. Fake nodes stand in for the
// browser's. Run: node frame-keys.mjs
import assert from 'node:assert/strict';
import { reservedKey } from '../../../bins/svolderd/web/core/frame.js';
import { inMenu, menuOwns } from '../../../bins/svolderd/web/core/menu.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

/** A node that is `tag` and sits inside the classes `within`: `closest` answers a selector list. */
const node = (tag, { within = [], editable = false } = {}) => ({
  tag,
  closest(selector) {
    const wanted = selector.split(',').map((part) => part.trim());
    const hit = wanted.some((part) => part === tag
      || (part === '[contenteditable]' && editable)
      || (part.startsWith('.') && within.includes(part.slice(1))));
    return hit ? this : null;
  },
});
const key = (name, target, mods = {}) => ({ key: name, target, ...mods });

ok('t_in_a_text_field_types_and_opens_nothing', () => {
  assert.equal(reservedKey(key('t', node('input'))), false, 'an input types the letter');
  assert.equal(reservedKey(key('T', node('input'))), false);
  assert.equal(reservedKey(key('t', node('textarea'))), false);
  assert.equal(reservedKey(key('t', node('select'))), false);
  assert.equal(reservedKey(key('t', node('div', { editable: true }))), false, 'contenteditable');
  assert.equal(reservedKey(key('t', node('button'))), true, 'a button leaves the letter to the frame');
  assert.equal(reservedKey(key('T', node('button'))), true);
  assert.equal(reservedKey(key('t', node('body'))), true);
});

ok('the_reserved_key_is_the_letter_alone', () => {
  const button = node('button');
  for (const mods of [{ ctrlKey: true }, { altKey: true }, { metaKey: true }]) {
    assert.equal(reservedKey(key('t', button, mods)), false, JSON.stringify(mods));
  }
  assert.equal(reservedKey(key('t', button, { shiftKey: true })), true);
  for (const other of ['a', 'm', 'Tab', 'Escape', '?', 'tt']) {
    assert.equal(reservedKey(key(other, button)), false, other);
  }
  assert.equal(reservedKey({ key: 't' }), true, 'no target at all');
});

ok('the_menu_owns_the_keys_of_its_open_popover_and_the_four_that_open_it', () => {
  const open = node('button', { within: ['fxpop'] });
  open.closest = (sel) => (sel === '.fxpop' ? { hidden: false } : null);
  const shut = node('button');
  shut.closest = (sel) => (sel === '.fxpop' ? { hidden: true } : null);
  const button = node('button');
  button.closest = (sel) => (sel === '.fxbtn' ? button : null);
  assert.equal(menuOwns(key('ArrowRight', open)), true, 'any key in the open popover');
  assert.equal(menuOwns(key('ArrowRight', shut)), false, 'a hidden popover owns none');
  for (const k of ['Enter', ' ', 'ArrowDown', 'ArrowUp']) assert.equal(menuOwns(key(k, button)), true, k);
  for (const k of ['m', 'ArrowLeft', '?', 'Escape']) assert.equal(menuOwns(key(k, button)), false, k);
  assert.equal(menuOwns(key('m', node('body'))), false);
  assert.equal(inMenu(null), false);
  assert.equal(inMenu({ closest: (sel) => (sel === '.fxmenu' ? {} : null) }), true);
});

console.log(`${n} frame-keys checks passed`);
