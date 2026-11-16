/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for what Signal draws of the computer's level meters (themes/signal/draw/vu.js and the
// words of words.js): the two dials, the glow class and the face button's label, the reference
// control's Reset (drawn hidden at the default), the Meters | Spectrum panes, the needle's angle at the
// stops and at 0 VU, the reading's text, the key table's entry; no `style` attribute and no script.
// Run: node signal-vu.mjs
import assert from 'node:assert/strict';
import { GLOWS, needleAngle, vuArea } from '../../../bins/svolderd/web/themes/signal/draw/vu.js';
import { KEYS, ZONES } from '../../../bins/svolderd/web/themes/signal/key-table.js';
import { VU, vuText } from '../../../bins/svolderd/web/themes/signal/words.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };
const state = (over = {}) => ({
  glow: 'w3', view: 'meters', ref: -18, defaults: -18, min: -30, max: -6, note: '', ...over,
});

ok('the section has two dial buttons, each naming the glow', () => {
  const html = vuArea(state());
  assert.equal((html.match(/<button[^>]*class="sg-vface"/g) ?? []).length, 2);
  assert.equal((html.match(/aria-label="Meter glow: filament\. Change"/g) ?? []).length, 2);
  assert.match(html, /class="sg-vu sg-w3"/);
});

ok('each reading row is a named group and a dark lamp is spoken with its state', () => {
  const html = vuArea(state());
  assert.match(html, /<div class="sg-vrd" role="group" aria-label="Left level">/);
  assert.match(html, /<div class="sg-vrd" role="group" aria-label="Right level">/);
  assert.equal((html.match(/class="sg-vlamp"[^>]*>Peak<span class="sg-sr">, off<\/span>/g) ?? []).length, 2);
  assert.equal(VU.peakState(true), ', over');
  assert.equal(VU.peakState(false), ', off');
});

ok('the glows go W1, W2, W3 and each has its words', () => {
  assert.deepEqual([...GLOWS], ['w1', 'w2', 'w3']);
  assert.deepEqual(GLOWS.map((g) => VU.glowWords(g)), [
    'Meter glow: subtle. Change', 'Meter glow: warm backlight. Change', 'Meter glow: filament. Change']);
});

ok('Reset is drawn hidden at the default reference and shown off it', () => {
  assert.match(vuArea(state()), /id="sg-vref-reset" hidden>Reset to −18/);
  assert.doesNotMatch(vuArea(state({ ref: -17 })), /id="sg-vref-reset" hidden/);
  assert.match(vuArea(state({ ref: -6 })), /id="sg-vref-up"[^>]*aria-disabled="true"/);
  assert.match(vuArea(state({ ref: -30 })), /id="sg-vref-down"[^>]*aria-disabled="true"/);
  assert.match(vuArea(state({ ref: -17 })), /0 VU = −17 dBFS/);
});

ok('one pane is hidden: Spectrum in the Meters view and the reverse', () => {
  const meters = vuArea(state());
  assert.match(meters, /data-pane="spectrum" hidden/);
  assert.match(meters, /data-view="meters" id="sg-vview-meters" aria-pressed="true"/);
  const spectrum = vuArea(state({ view: 'spectrum' }));
  assert.match(spectrum, /data-pane="meters" hidden/);
  assert.match(spectrum, /id="sg-vview-spectrum" aria-pressed="true"/);
});

ok('the markup has no style attribute and no script', () => {
  const html = vuArea(state({ note: 'a note' }));
  assert.doesNotMatch(html, /\sstyle=|<script/i);
  assert.match(html, /a note/);
});

ok('the needle rests at the left stop, 0 VU is right of the middle and the pin is the right stop', () => {
  assert.equal(needleAngle(0), -42);
  assert.ok(Math.abs(needleAngle(0.6857) - 15.6) < 0.1);
  assert.ok(Math.abs(needleAngle(1) - 42) < 1e-9);
  assert.ok(needleAngle(5) <= -42 + 84 * 1.03 + 1e-9);
  assert.equal(needleAngle(-1), -42);
});

ok('the reading is its tenth of a VU, or `< −20` below the scale', () => {
  assert.equal(vuText(0), '+0.0');
  assert.equal(vuText(-0.04), '+0.0');
  assert.equal(vuText(2.62), '+2.6');
  assert.equal(vuText(-4.26), '−4.3');
  assert.equal(vuText(-20), '−20.0');
  assert.equal(vuText(-20.1), '< −20');
  assert.equal(vuText(-Infinity), '< −20');
  assert.equal(vuText(Number.NaN), '< −20');
});

ok('the key table lists the dial face as a widget key', () => {
  const entry = KEYS.find((k) => k.id === 'meter-glow');
  assert.ok(entry);
  assert.equal(entry.zone, '.sg-vface');
  assert.deepEqual(entry.keys, ['Enter', ' ']);
  assert.ok(ZONES['.sg-vface']);
});

console.log(`signal-vu.mjs: ${n} passed`);
