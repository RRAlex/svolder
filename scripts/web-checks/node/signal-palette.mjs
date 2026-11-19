/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for Signal's palettes (themes/signal/palette.js and the root tokens of style.css):
// Blueprint is the default (the first of the list, no attribute on the root, the bare root's
// tokens), a person who never chose gets it, and a kept choice of any palette, Warm amber
// included, stays. Storage is a fake. Run: node signal-palette.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createPrefs } from '../../../bins/svolderd/web/core/prefs.js';
import {
  DEFAULT_PALETTE, PALETTES, applyPalette, clearPalette, paletteOf,
} from '../../../bins/svolderd/web/themes/signal/palette.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const memory = () => {
  const map = new Map();
  return {
    map,
    getItem: (key) => (map.has(key) ? map.get(key) : null),
    setItem: (key, text) => { map.set(key, String(text)); },
    removeItem: (key) => { map.delete(key); },
  };
};
const page = () => ({ documentElement: { dataset: {} } });
const css = readFileSync(
  new URL('../../../bins/svolderd/web/themes/signal/style.css', import.meta.url), 'utf8');
const groundOf = (selector) => {
  const at = css.indexOf(`${selector} {`);
  assert.ok(at >= 0, selector);
  return /--sg-ground: (#[0-9a-f]{6})/.exec(css.slice(at))[1];
};

ok('the_default_palette_is_blueprint_and_leads_the_menu', () => {
  assert.equal(DEFAULT_PALETTE, 'blueprint');
  assert.equal(PALETTES[0].id, 'blueprint');
  assert.deepEqual(PALETTES.map((one) => one.id).sort(), ['amber', 'blueprint', 'oxblood', 'phosphor']);
});

ok('a_person_who_never_chose_gets_blueprint', () => {
  const prefs = createPrefs('signal', memory());
  assert.equal(paletteOf(prefs.get('palette', null)), 'blueprint');
  assert.equal(paletteOf(undefined), 'blueprint');
  assert.equal(paletteOf('nope'), 'blueprint', 'a kept value that is no palette');
});

ok('a_kept_choice_stays_warm_amber_included', () => {
  for (const { id } of PALETTES) {
    const storage = memory();
    const prefs = createPrefs('signal', storage);
    prefs.set('palette', id);
    assert.equal(paletteOf(createPrefs('signal', storage).get('palette', null)), id);
  }
});

ok('the_root_wears_no_attribute_for_the_default_and_one_for_the_others', () => {
  const doc = page();
  applyPalette(doc, null);
  assert.equal(doc.documentElement.dataset.palette, undefined);
  applyPalette(doc, 'amber');
  assert.equal(doc.documentElement.dataset.palette, 'amber');
  applyPalette(doc, 'blueprint');
  assert.equal(doc.documentElement.dataset.palette, undefined);
  applyPalette(doc, 'phosphor');
  clearPalette(doc);
  assert.equal(doc.documentElement.dataset.palette, undefined);
});

ok('the_bare_root_carries_the_default_palettes_tokens', () => {
  assert.equal(groundOf(':root[data-theme="signal"]'), '#08121f');
  assert.equal(groundOf(':root[data-theme="signal"][data-palette="amber"]'), '#0f0d0b');
  assert.ok(!css.includes('[data-palette="blueprint"]'), 'Blueprint is the bare root, no block');
});

console.log(`${n} cases`);
