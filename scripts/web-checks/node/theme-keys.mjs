/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the five themes' key tables (themes/<id>/key-table.js): each is one the core
// can read and bind (core/keys.js refuses a malformed entry and a reserved key), every entry has
// an id of its own, a widget's entry has no `run` and names a selector the browser can parse, a
// zone's entries stand together, every keycap is one a keyboard has (one name a
// keycap, several keycaps a list) and only a gesture is named otherwise, the words are sentence
// case with no full stop,
// and the keys, zones and words are the ones written here: the list a person reads in the Help
// popup, so a change of wording shows in this file. The pure `KEYS` are read; what a mount adds
// (a `run` for each key of the page) is held by `ember-keys.mjs` and, in a browser, `keys.py`.
// A jsdom page (apt `node-jsdom`). Run: node theme-keys.mjs
import assert from 'node:assert/strict';
import { page } from './dom.mjs';
import { capsOf, createKeys, keysOf, listed } from '../../../bins/svolderd/web/core/keys.js';
import { KEYS as ember } from '../../../bins/svolderd/web/themes/ember/key-table.js';
import { KEYS as nightdrive } from '../../../bins/svolderd/web/themes/nightdrive/key-table.js';
import { KEYS as paper } from '../../../bins/svolderd/web/themes/paper/key-table.js';
import { KEYS as signal } from '../../../bins/svolderd/web/themes/signal/key-table.js';
import { KEYS as voidgarden } from '../../../bins/svolderd/web/themes/voidgarden/key-table.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const TABLES = { ember, paper, signal, nightdrive, voidgarden };

/** Each theme's table as a person reads it: the key, the zone and the words, in table order. */
const WORDS = {
  ember: [
    [['←', '→'], 'page', 'Volume one step'],
    ['m', 'page', 'Mute'],
    ['0', 'page', 'Centre the balance'],
    ['v', 'page', 'Change the visualizer\'s look'],
    ['f', 'page', 'Tone control to Flat'],
    ['p', 'page', 'Go to the preset that is on'],
    ['Curve', '.eqg .plot', 'Drag a point for frequency and gain; wheel, right drag or pinch for Q'],
    ['A', 'page', 'Arrange sections'],
    [['↑', '↓', '←', '→'], 'sv-knob', 'Volume one step'],
    [['Page Up', 'Page Down'], 'sv-knob', 'Volume eight steps'],
    ['Home', 'sv-knob', 'Volume to its lowest'],
    [['↑', '↓', '←', '→'], 'sv-slider', 'The slider one step'],
    ['Home', 'sv-slider[data-centre]:not([data-centre=""])', 'Back to the centre'],
    [['↑', '↓', '←', '→'], '.arr-handle', 'Move the section'],
    ['Esc', '.view.arranging', 'Leave Arrange mode and put the order back'],
    ['Enter', '#part-bands .bedit .v', 'Apply the typed value'],
    ['Esc', '#part-bands .bedit .v', 'Put the old value back'],
    ['Enter', '.pts-table .cell', 'Apply the typed value'],
    ['Esc', '.pts-table .cell', 'Put the old value back'],
    ['Esc', '.eqg .plot', 'Let go of the point where it is'],
  ],
  paper: [
    [['↑', '↓', '←', '→'], '#p-needle', 'Volume one step'],
    [['Page Up', 'Page Down'], '#p-needle', 'Volume ten steps'],
    ['Home', '#p-needle', 'Volume to its lowest'],
    [['↑', '↓', '←', '→'], '.p-scale', 'The scale one step'],
    [['Page Up', 'Page Down'], '.p-scale', 'The scale ten steps'],
    ['Home', '.p-scale', 'The scale to its lowest'],
    [['←', '→'], '.pt', 'Frequency one step'],
    [['↑', '↓'], '.pt', 'Gain one step'],
    ['Enter', '.p-cell', 'Apply the typed value'],
    ['Esc', '.p-cell', 'Put the old value back'],
    [['↑', '↓'], '.p-cell', 'The value one step'],
    [['↑', '↓'], '.plist', 'The option above or below'],
    [['Home', 'End'], '.plist', 'The first or the last option'],
    ['Enter', '.plist', 'Choose the option'],
    ['Esc', '.plist', 'Close the list'],
    ['Esc', '.p-askbox', 'Cancel and change nothing'],
  ],
  signal: [
    [['←', '→'], '#sg-line', 'The hop before or after'],
    [['Home', 'End'], '#sg-line', 'The first or the last hop'],
    [['Enter', 'Space'], '#sg-line', 'Choose the hop'],
    [['↑', '↓', '←', '→'], '#sg-vtrack, #sg-cvtrack', 'Volume one step'],
    [['Page Up', 'Page Down'], '#sg-vtrack, #sg-cvtrack', 'Volume ten steps'],
    ['Home', '#sg-vtrack, #sg-cvtrack', 'Volume to its lowest'],
    [['Enter', 'Space'], '.sg-vface', 'Change the meter glow'],
    [['↑', '↓', '←', '→'], '.sg-scale', 'The scale one step'],
    [['Page Up', 'Page Down'], '.sg-scale', 'The scale ten steps'],
    ['Home', '.sg-scale', 'The scale to its lowest'],
    [['←', '→'], '.sg-pt', 'Frequency one step'],
    [['↑', '↓'], '.sg-pt', 'Gain one step'],
    ['Esc', '.sg-pt[data-n]', 'Let go of the point where it is'],
    ['Enter', 'input.sg-val, input.sg-pval', 'Apply the typed value'],
    ['Esc', 'input.sg-val, input.sg-pval', 'Put the old value back'],
    [['↑', '↓'], '.sg-list', 'The option above or below'],
    [['Home', 'End'], '.sg-list', 'The first or the last option'],
    ['Enter', '.sg-list', 'Choose the option'],
    ['Esc', '.sg-list', 'Close the list'],
    ['Esc', '.sg-askbox', 'Cancel and change nothing'],
    ['Esc', '.sg-editform', 'Cancel and change nothing'],
    ['Esc', '#sg-sheet-form', 'Close the sheet and send nothing'],
  ],
  nightdrive: [
    [['↑', '↓'], 'page', 'Volume one step'],
    [['↑', '↓', '←', '→'], '#nd-controls .nd-scale', 'The scale one step'],
    [['Page Up', 'Page Down'], '#nd-controls .nd-scale', 'The scale ten steps'],
    ['Home', '#nd-controls .nd-scale', 'The scale to its lowest'],
    ['Enter', 'input.nd-v', 'Apply the typed value'],
    ['Esc', 'input.nd-v', 'Put the old value back'],
    [['↑', '↓'], '.nd-list', 'The option above or below'],
    [['Home', 'End'], '.nd-list', 'The first or the last option'],
    ['Enter', '.nd-list', 'Choose the option'],
    ['Esc', '.nd-list', 'Close the list'],
    ['Esc', '.nd-askbox', 'Cancel and change nothing'],
    ['Esc', '#nd-dialog', 'Close the sheet'],
  ],
  voidgarden: [
    [['↑', '↓'], 'page', 'Volume one step'],
    ['A', 'page', 'Cycle the audio panel'],
    ['S', 'page', 'New seed: a new world for this track'],
    ['N', 'page', 'Next scene: another world on the same track'],
    ['E', 'page', 'Effect: fire one of the scene’s effects'],
    ['Esc', '#vg-dash', 'Close the phone\u2019s sheet'],
  ],
};

/** The keys of the whole page each theme binds (the entries a mount gives a `run`), by id. */
const PAGE_KEYS = {
  ember: ['volume', 'mute', 'centre', 'look', 'flat', 'preset', 'arrange'],
  paper: [],
  signal: [],
  nightdrive: ['volume'],
  voidgarden: ['volume', 'panel', 'explore-seed', 'explore-scene', 'explore-effect'],
};

/** The keycaps a table may name: a letter or a digit, or one of these. A name of two words is
 * one keycap (`Page Up`); two keys are two keycaps, never one string. */
const CAPS = ['↑', '↓', '←', '→', 'Home', 'End', 'Page Up', 'Page Down', 'Enter', 'Esc', 'Space'];

/** The keys a listener of the theme's own answers in place of the binding (`own`), by id. */
const OWN = {
  ember: ['arrange-leave', 'curve-end'],
  paper: [],
  signal: [],
  nightdrive: [],
  voidgarden: [],
};

const p = page('<div id="theme"></div>');
globalThis.AbortController = p.window.AbortController;

for (const [id, table] of Object.entries(TABLES)) {
  ok(`${id}_the_keys_zones_and_words_are_the_ones_written_here`, () => {
    assert.deepEqual(table.map((e) => [e.key, e.zone, e.words]), WORDS[id]);
  });

  ok(`${id}_the_core_can_read_and_bind_the_table`, () => {
    const keys = createKeys({ document: p.document });
    const stop = new p.window.AbortController();
    // a run on every key of the page that the theme's own listener does not answer, as the
    // theme's mount gives them
    const bound = table.map((e) => (e.zone === 'page' && !e.own ? { ...e, run() {} } : e));
    // the table as data is not bound as it stands where the page has a key that needs its run
    if (table.some((e) => e.zone === 'page' && !e.own)) {
      assert.throws(() => keys.bind(table, { signal: stop.signal }), /`own: true`/);
    }
    keys.bind(bound, { signal: stop.signal });
    stop.abort();
    assert.equal(new Set(table.map((e) => e.id)).size, table.length, 'each entry has an id of its own');
    assert.deepEqual(table.filter((e) => e.zone === 'page').map((e) => e.id), PAGE_KEYS[id]);
    for (const e of table) assert.equal(e.run, undefined, `${e.id}: the table is data`);
    assert.deepEqual(table.filter((e) => e.own).map((e) => e.id), OWN[id]);
  });

  ok(`${id}_every_zone_is_a_selector_and_a_zones_entries_stand_together`, () => {
    const zones = [...new Set(table.filter((e) => e.zone !== 'page').map((e) => e.zone))];
    // Void Garden's keys are the page's plus its phone sheet's Escape: one widget zone, by design
    const vgZones = id === 'voidgarden' && zones.every((z) => z === '#vg-dash');
    assert.ok(zones.length >= 4 || vgZones, `${zones.length} widgets`);
    for (const zone of zones) p.document.querySelector(zone);
    // the entries of one zone stand together (a gesture's row stands among the page's keys)
    const order = table.filter((e) => e.zone !== 'page' && !e.gesture).map((e) => e.zone);
    assert.deepEqual(order.filter((zone, at) => zone !== order[at - 1]), [...new Set(order)]);
  });

  ok(`${id}_the_words_are_sentence_case_with_no_full_stop_and_each_key_has_a_name`, () => {
    for (const e of table) {
      assert.match(e.words, /^[A-Z][^.]*$/, `${e.id}: ${e.words}`);
      assert.ok(e.words.length <= 72, `${e.id}: short`);
      assert.ok(keysOf(e).every((key) => key.length > 0), e.id);
    }
  });

  ok(`${id}_every_keycap_is_one_a_keyboard_has_and_a_gesture_alone_is_named_otherwise`, () => {
    for (const e of table) {
      assert.ok(typeof e.key === 'string' || (Array.isArray(e.key) && e.key.length >= 2),
        `${e.id}: one keycap is a string, several are a list`);
      if (e.gesture) {
        assert.equal(typeof e.key, 'string', `${e.id}: a gesture has a name`);
        assert.notEqual(e.zone, 'page', `${e.id}: a gesture is a widget's`);
        continue;
      }
      for (const cap of capsOf(e)) {
        assert.ok(CAPS.includes(cap) || /^[A-Za-z0-9]$/.test(cap), `${e.id}: the keycap ${cap}`);
      }
      // a keycap is the key itself unless the entry says which keys answer
      if (!e.keys) for (const cap of capsOf(e)) assert.doesNotMatch(cap, /[ ←→↑↓]|^Esc$/, e.id);
    }
    assert.deepEqual(table.filter((e) => e.gesture).map((e) => e.id), id === 'ember' ? ['curve'] : []);
  });

  ok(`${id}_a_viewer_who_may_not_write_has_fewer_keys_and_none_that_writes`, () => {
    const writer = listed(table, { canWrite: true });
    const reader = listed(table, { canWrite: false });
    assert.deepEqual(writer, table, 'a writer has every key');
    assert.ok(reader.length < writer.length, `${reader.length} of ${writer.length}`);
    // the keys of the page a viewer who may not write still has
    assert.deepEqual(reader.filter((e) => e.zone === 'page').map((e) => e.id), {
      ember: ['look', 'arrange'], paper: [], signal: [], nightdrive: [], voidgarden: ['panel', 'explore-seed', 'explore-scene', 'explore-effect'],
    }[id]);
    for (const e of reader) {
      assert.doesNotMatch(e.words, /^(Volume|The scale|The slider|Back to|Apply|Put the old|Frequency|Gain|Mute|Centre)/,
        `${e.id}: ${e.words}`);
    }
  });
}

p.window.close();
console.log(`theme-keys.mjs: ${n} passed`);
