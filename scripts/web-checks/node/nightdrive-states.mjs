/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Night drive's states (themes/nightdrive/states.js, draw/card.js): the mood
// of the page in each state, the card of a device out of reach with its Try now and its seconds, a
// fault that leaves the controls live, and the quiet line that says why there is no music analysis.
// Run: node nightdrive-states.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { drawCard } from '../../../bins/svolderd/web/themes/nightdrive/draw/card.js';
import { drawFaults } from '../../../bins/svolderd/web/themes/nightdrive/draw/facts.js';
import { drawDash } from '../../../bins/svolderd/web/themes/nightdrive/draw/dash.js';
import { topLine } from '../../../bins/svolderd/web/themes/nightdrive/hud.js';
import {
  dullMood, flatMood, moodOf, quietOf,
} from '../../../bins/svolderd/web/themes/nightdrive/states.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

const fixture = JSON.parse(readFileSync(new URL('./fixtures/ember-parts.json', import.meta.url), 'utf8'));
const clone = (value) => JSON.parse(JSON.stringify(value));
const pageOf = (name) => clone(fixture.cases.find((c) => c.name === name).page);
const view = { phone: () => false };
const RUNNING = { state: 'running' };
const NONE = { state: 'unavailable', why: 'no sound card found' };

ok('the_mood_of_each_state', () => {
  const dac = pageOf('dac');
  assert.equal(moodOf(dac, 'open', RUNNING), 'ok');
  assert.equal(moodOf(dac, 'open', null), 'ok', 'before the analysis has said anything');
  assert.equal(moodOf(dac, 'open', { state: 'starting' }), 'ok');
  assert.equal(moodOf(dac, 'lost', RUNNING), 'lost');
  assert.equal(moodOf(dac, 'waiting', RUNNING), 'wait');
  assert.equal(moodOf(dac, 'resting', RUNNING), 'wait');
  assert.equal(moodOf(dac, 'open', NONE), 'nocap');
  for (const name of ['lost', 'looking', 'refused', 'refused-amp']) {
    assert.equal(moodOf(pageOf(name), 'open', RUNNING), 'unreachable', name);
  }
  for (const name of ['no-devices', 'unavailable']) {
    assert.equal(moodOf(pageOf(name), 'open', RUNNING), 'unreachable', `${name}: the page's own card`);
  }
  // the stream first, then the device's card, then the music
  assert.equal(moodOf(pageOf('lost'), 'lost', NONE), 'lost');
  assert.equal(moodOf(pageOf('lost'), 'waiting', NONE), 'wait');
  assert.equal(moodOf(pageOf('lost'), 'open', NONE), 'unreachable');
  // a fault on a connected device and a guest are both `ok`
  assert.equal(moodOf(pageOf('fault'), 'open', RUNNING), 'ok');
  assert.equal(moodOf(pageOf('dac-guest'), 'open', RUNNING), 'ok');
  assert.equal(moodOf(pageOf('dac-guest'), 'open', NONE), 'nocap');
  assert.equal(moodOf(null, 'open', null), 'ok');
  // which moods grey the scene and flatten the mountains
  assert.deepEqual(['ok', 'nocap', 'wait', 'lost', 'unreachable'].map(dullMood), [false, false, true, true, true]);
  assert.deepEqual(['ok', 'nocap', 'wait', 'lost', 'unreachable'].map(flatMood), [false, false, false, true, true]);
});

ok('an_unreachable_card_has_try_now_and_the_seconds', () => {
  const held = pageOf('lost').device.card;
  const html = drawCard(held, { now: 5000 });
  assert.match(html, /<h2>Gungnir 2 is out of reach<\/h2>/);
  assert.match(html, /<button[^>]*id="nd-try"[^>]*data-act="try"[^>]*>Try now<\/button>/);
  assert.doesNotMatch(html, /quiet|aria-disabled/);
  assert.match(html, /<span class="nd-retry" data-secs="8" data-drawn="5000">next try in 8 s<\/span>/);
  assert.match(html, /Not connected\. Next attempt in 8 s\./);
  // 0 seconds says it is trying now
  assert.match(drawCard({ ...held, retry_in: 0 }), />trying again now</);
  // the card that has no retry and no key: neither is drawn, the command is in the mono face
  const refused = drawCard(pageOf('refused').device.card);
  assert.doesNotMatch(refused, /nd-try|nd-retry|nd-tryline/);
  assert.match(refused, /<code>svolder doctor<\/code>/);
  // a card that is looking has its own seconds
  assert.match(drawCard(pageOf('looking').device.card), /next try in 5 s/);
  // a viewer who may not write has the key drawn quiet
  assert.match(drawCard(held, { quiet: true }), /id="nd-try" class="nd-k cy quiet" data-act="try" aria-disabled="true"/);
  assert.equal(drawCard(null), '');
  // the words of a card are escaped
  const hostile = drawCard({ title: '<b>x</b>', detail: '"><img>', retry_in: null, try_now: false });
  assert.doesNotMatch(hostile, /<b>x|<img/);
  assert.doesNotMatch(drawCard(held), /style=/);
});

ok('a_fault_keeps_the_controls_live', () => {
  const page = pageOf('fault');
  assert.equal(moodOf(page, 'open', RUNNING), 'ok');
  assert.equal(dullMood('ok') || flatMood('ok'), false);
  const top = topLine(page, 'open');
  assert.equal(top.right, 'fault');
  assert.equal(top.tone, 'bad');
  const dash = drawDash(page, view, 0);
  assert.match(dash, /data-act="power"/, "the fixture is an amp: its power key is there");
  assert.doesNotMatch(dash, /quiet|aria-disabled/, 'no key of the dash is drawn quiet');
  // the faults go to the sheet with their advice, never into the middle of the scene
  const faults = drawFaults(page.device);
  assert.match(faults, /<ul class="nd-advice">.*<b>Over-current<\/b> Power-cycle the device\./);
  assert.match(faults, /<b>Over-temperature<\/b>/);
  assert.equal(drawFaults(pageOf('lost').device), '', 'a card that is not a fault has no faults');
  assert.equal(drawFaults(null), '');
});

ok('no_capture_says_why', () => {
  assert.equal(quietOf('nocap', 'open', NONE), 'no music analysis: no sound card found');
  assert.equal(quietOf('nocap', 'open', { state: 'unavailable' }), 'no music analysis: the computer’s audio cannot be read');
  assert.equal(quietOf('nocap', 'open', { state: 'unavailable', why: '' }), 'no music analysis: the computer’s audio cannot be read');
  assert.equal(quietOf('ok', 'open', RUNNING), '');
  assert.equal(quietOf('unreachable', 'open', NONE), '', 'the card speaks, not the line');
  assert.equal(quietOf('lost', 'lost', NONE), 'Reconnecting to svolder…');
  assert.equal(quietOf('wait', 'waiting', RUNNING), 'reading the device again…');
  assert.equal(quietOf('wait', 'resting', RUNNING), 'Paused while the page is hidden; it reads the device again when shown.');
});

console.log(`nightdrive-states.mjs: ${n} passed`);
