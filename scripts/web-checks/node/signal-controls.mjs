/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Signal's controls (themes/signal/controls-wire.js) with a stand-in
// page: a scale's writes are one run in flight per device and control, so a step asked for after another
// device is shown never goes to the device shown before.
// Run: node signal-controls.mjs
import assert from 'node:assert/strict';
import { wireControls } from '../../../bins/svolderd/web/themes/signal/controls-wire.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

/** A page with one scale, `balance`, on the device shown (`shown.name`); the calls the core was asked for and the
 * answers still to be given. */
function stage() {
  const shown = { name: 'A' };
  const calls = [];
  const answers = [];
  const handlers = { switch: [] };
  const listeners = {};
  const scale = {
    dataset: { key: 'balance' },
    attrs: { 'aria-valuenow': '0' },
    getAttribute(name) { return this.attrs[name] ?? null; },
    setAttribute(name, value) { this.attrs[name] = value; },
    closest() { return this; },
    querySelector() { return this.knob; },
    querySelectorAll() { return []; },
    knob: { dataset: {} },
    appendChild(el) { return el; },
    focus() {},
  };
  const wrap = {
    addEventListener(type, fn) { (listeners[type] ??= []).push(fn); },
    setPointerCapture() {},
    querySelector() { return null; },
  };
  const control = { key: 'balance', raw: 0, lo: -10, hi: 10, centre: 0 };
  const core = {
    drag: { BAND_PARAM: {} },
    hold: { repeat() { return { end() {} }; } },
    actions: {
      setControl(key, body, opts) {
        calls.push({ key, raw: body.raw, device: opts.device });
        return new Promise((resolve) => answers.push(resolve));
      },
    },
  };
  const view = { device: () => ({ controls: [control] }), target: () => shown.name, state: 'open' };
  const hand = { grip() {}, letGo() {}, forget() {}, stop() {} };
  const keep = { hand: () => hand, reset() {}, around: (fn) => fn(), dropLater() {}, take() {} };
  const live = { onSwitch: (fn) => handlers.switch.push(fn), onChoice() {}, onReading() {}, draw() {} };
  const act = { act: (name, value, fn) => fn(core.actions) };
  const controller = new AbortController();
  wireControls({
    root: { querySelector: () => wrap }, core, live, act, view, keep, release() {}, signal: controller.signal,
  });
  const press = (key) => {
    for (const fn of listeners.keydown) {
      fn({ key, target: scale, altKey: false, ctrlKey: false, metaKey: false, preventDefault() {} });
    }
  };
  const switchTo = (name) => {
    shown.name = name;
    for (const fn of handlers.switch) fn();
  };
  return {
    calls, answers, press, switchTo, scale, controller,
  };
}

const settle = () => new Promise((resolve) => setTimeout(resolve, 0));

await ok('a_scale_step_asked_after_a_switch_is_written_to_the_new_device_only', async () => {
  const s = stage();
  s.press('ArrowRight');
  assert.deepEqual(s.calls, [{ key: 'balance', raw: 1, device: 'A' }]);
  s.switchTo('B');
  s.press('ArrowRight');
  assert.deepEqual(s.calls.at(-1), { key: 'balance', raw: 2, device: 'B' }, 'the new device has its own run');
  s.answers.forEach((answer) => answer({ ok: true }));
  await settle();
  await settle();
  assert.equal(s.calls.filter((c) => c.device === 'A').length, 1, 'nothing more reaches the old device');
  s.controller.abort();
});

await ok('a_scale_keeps_one_write_in_flight_and_the_latest_waiting', async () => {
  const s = stage();
  s.press('ArrowRight');
  s.press('ArrowRight');
  s.press('ArrowRight');
  assert.equal(s.calls.length, 1, 'the second and third wait');
  s.answers[0]({ ok: true });
  await settle();
  await settle();
  assert.deepEqual(s.calls.map((c) => c.raw), [1, 3], 'the newest is written, the one before it is not');
  s.controller.abort();
});

console.log(`signal-controls.mjs: ${n} passed`);
