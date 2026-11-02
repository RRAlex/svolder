/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/climb.js and core/tone-note.js, the two notes the core says for every
// theme: the climb's popup (once a wording, for six seconds, with the cursor class on the page)
// and the tone mode's note (on a change into Flat or Loudness, or on the person's own click
// within eight seconds). Run: node core-climb.mjs
import assert from 'node:assert/strict';
import { CLIMB_POPUP_MS, createClimb } from '../../../bins/svolderd/web/core/climb.js';
import { MODE_CLICK_MS, createToneNote } from '../../../bins/svolderd/web/core/tone-note.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const body = () => {
  const classes = new Set();
  return {
    classes,
    classList: {
      toggle: (name, on) => { if (on) classes.add(name); else classes.delete(name); },
      remove: (name) => classes.delete(name),
      contains: (name) => classes.has(name),
    },
  };
};
const climbing = (text) => ({
  raw: 30, target: 60, notes: [{ text: 'Another', popup: false }, { text, popup: true }],
});

ok('the_climb_is_said_once_per_wording_for_6000_ms', () => {
  assert.equal(CLIMB_POPUP_MS, 6000);
  const said = [];
  const page = body();
  const climb = createClimb({
    toast: (...args) => said.push(args), dismissToast() {}, body: page,
  });
  climb.paint(climbing('Climbing to -30 dB'));
  assert.deepEqual(said, [['Climbing to -30 dB', null, { key: 'climb', ms: 6000 }]]);
  assert.equal(page.classes.has('climbing'), true);
  climb.paint(climbing('Climbing to -30 dB'));
  assert.equal(said.length, 1, 'the same words are said once');
  climb.paint(climbing('Climbing to -20 dB'));
  assert.equal(said.length, 2, 'a new wording is said');
  // The climb ends: the cursor goes, the popup leaves with its time, and the same words said
  // again by a later climb are a new popup.
  climb.paint({ raw: 60, target: null, notes: [] });
  assert.equal(page.classes.has('climbing'), false);
  assert.equal(said.length, 2);
  climb.paint(climbing('Climbing to -20 dB'));
  assert.equal(said.length, 3);
  climb.paint(null);
  assert.equal(page.classes.has('climbing'), false);
  climb.paint({ raw: null, target: 60, notes: [{ text: 'x', popup: true }] });
  assert.equal(page.classes.has('climbing'), false, 'no reading is no climb');
  assert.equal(said.length, 3);
});

ok('a_climb_a_touch_ended_stays_ended_until_a_reading_says_another_target', () => {
  const said = [];
  const dismissed = [];
  const page = body();
  const climb = createClimb({
    toast: (...args) => said.push(args), dismissToast: (key) => dismissed.push(key), body: page,
  });
  climb.paint(climbing('Climbing to -30 dB'));
  assert.equal(said.length, 1);
  climb.cancel(60);
  assert.deepEqual(dismissed, ['climb'], 'the popup goes at once');
  assert.equal(page.classes.has('climbing'), false, 'and the cursor');
  climb.paint(climbing('Climbing to -30 dB'));
  assert.equal(page.classes.has('climbing'), false, 'a reading on its way does not bring it back');
  assert.equal(said.length, 1);
  climb.paint({ raw: 30, target: null, notes: [] });
  climb.paint(climbing('Climbing to -30 dB'));
  assert.equal(page.classes.has('climbing'), true, 'a reading that dropped the target ends it');
  assert.equal(said.length, 2, 'a climb after that is a new climb');
  // A stop that did not land: the climb goes on and says so again.
  climb.cancel(60);
  climb.cancel(null);
  climb.paint(climbing('Climbing to -30 dB'));
  assert.equal(page.classes.has('climbing'), true);
  assert.equal(said.length, 3);
});

ok('dropping_the_climb_takes_the_popup_away', () => {
  const dismissed = [];
  const said = [];
  const page = body();
  const climb = createClimb({
    toast: (...args) => said.push(args), dismissToast: (key) => dismissed.push(key), body: page,
  });
  climb.paint(climbing('Climbing'));
  climb.drop();
  assert.deepEqual(dismissed, ['climb']);
  assert.equal(page.classes.has('climbing'), false);
  climb.paint(climbing('Climbing'));
  assert.equal(said.length, 2, 'what was dropped is said afresh');
});

const tone = (mode, note) => ({ mode, note });
const FLAT = 'Flat bypasses the bands';
const LOUD = 'Loudness follows the volume';

ok('the_tone_note_shows_on_a_change_into_flat_or_loudness_not_on_first_sight', () => {
  const said = [];
  let t = 0;
  const note = createToneNote({ toast: (text) => said.push(text), now: () => t });
  note.seen(tone('flat', FLAT));
  assert.deepEqual(said, [], 'first sight shows nothing');
  note.seen(tone('flat', FLAT));
  assert.deepEqual(said, [], 'no change, no note');
  note.seen(tone('loudness', LOUD));
  assert.deepEqual(said, [LOUD]);
  note.seen(tone('eq', null));
  assert.deepEqual(said, [LOUD], 'a change into EQ says nothing');
  note.seen(tone('flat', FLAT));
  assert.deepEqual(said, [LOUD, FLAT]);
  note.seen(null);
  note.seen(tone('loudness', LOUD));
  assert.equal(said.length, 2, 'a device with no tone control forgets, so the next is a first sight');
});

ok('a_click_within_8000_ms_counts_after_a_forget', () => {
  assert.equal(MODE_CLICK_MS, 8000);
  const said = [];
  let t = 0;
  const note = createToneNote({ toast: (text) => said.push(text), now: () => t });
  note.seen(tone('eq', null));
  note.forget();
  note.clicked('loudness');
  t = 7999;
  note.seen(tone('loudness', LOUD));
  assert.deepEqual(said, [LOUD], 'the person\'s own click, though the reconnect forgot what was seen');
  note.seen(tone('eq', null));
  note.forget();
  t = 10000;
  note.clicked('flat');
  t = 18000;
  note.seen(tone('flat', FLAT));
  assert.deepEqual(said, [LOUD], 'eight seconds on it is no longer theirs, and a first sight says nothing');
  note.clicked('loudness');
  note.seen(tone('flat', FLAT));
  assert.deepEqual(said, [LOUD], 'a click on another mode is not this one\'s');
});

console.log(`${n} core-climb checks passed`);
