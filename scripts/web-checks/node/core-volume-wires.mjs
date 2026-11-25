/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the volume ask as each theme wires it: the theme's REAL volume wire (Paper's
// volume-wire.js, Signal's volume-wire.js, Night drive's dash-wire.js, Ember's knob.js) over that
// theme's real view shape, one ask (the Home key) and one nudge (an arrow key, the `−` button) each
// reaching the core's `setVolume`. Paper's view has a getter-only `now`: the asker must not write
// into a view. Run: node core-volume-wires.mjs
import assert from 'node:assert/strict';
import { page as domPage } from './dom.mjs';
import { DAC, computerRig } from './nightdrive-rig.mjs';

const dom = domPage('<div id="root"></div>');
Object.assign(globalThis, {
  window: dom.window,
  document: dom.document,
  HTMLElement: dom.window.HTMLElement,
  customElements: dom.window.customElements,
  Node: dom.window.Node,
  AbortController: dom.window.AbortController,
});
const { wireVolume: wirePaper } = await import('../../../bins/svolderd/web/themes/paper/volume-wire.js');
const { wireVolume: wireSignal } = await import('../../../bins/svolderd/web/themes/signal/volume-wire.js');
const { wireDash } = await import('../../../bins/svolderd/web/themes/nightdrive/dash-wire.js');
const { ember } = await import('../../../bins/svolderd/web/themes/ember/bind.js');
await import('../../../bins/svolderd/web/themes/ember/knob.js');
const { ASK_MS } = await import('../../../bins/svolderd/web/core/volume-ask.js');

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
const tick = () => new Promise((resolve) => { setTimeout(resolve, 0); });
const level = (raw, more = {}) => ({
  raw, lo: 0, hi: 100, cap: null, target: null, muted: false, ...more,
});

/** A fake `listeners` box: the handlers the wire registered, called with an event whose target
 * answers `closest` with `el`. */
function fakeBox(el) {
  const listeners = {};
  const box = {
    addEventListener(type, fn) { (listeners[type] ??= []).push(fn); },
    querySelector: () => null,
    closest: () => null,
    focus() {},
  };
  const fire = (type, init = {}) => {
    for (const fn of listeners[type] ?? []) {
      fn({
        target: { closest: () => el, matches: () => true }, altKey: false, ctrlKey: false, metaKey: false,
        preventDefault() {}, detail: 0, ...init,
      });
    }
  };
  return { box, fire };
}

/** The core a Paper/Signal wire reads: a device at `vol`, and the writes that reached it. */
function stubCore(vol) {
  const sent = [];
  const core = {
    model: { part: () => vol, device: () => ({ inert: false }), shown: () => 'dac' },
    computer: { side: () => 'device' },
    actions: {
      setVolume: (step, o) => { sent.push([step, o.device]); return Promise.resolve({ ok: true }); },
      stopClimb() {},
    },
    repeat: () => ({ end() {} }),
  };
  const act = { act: (_name, _step, fn) => fn(core.actions) };
  const live = { onReading() {}, onSwitch() {}, draw() {}, freeze() {} };
  return { core, act, live, sent };
}

await ok('paper_a_getter_only_now_view_takes_an_ask_and_a_nudge', async () => {
  const { core, act, live, sent } = stubCore(level(50));
  const { box, fire } = fakeBox({ dataset: {} });
  // Paper's view (paper.js): `now` is a getter, nothing may write it
  const view = { asked: null, get now() { return Date.now(); } };
  const control = new AbortController();
  wirePaper({
    root: { querySelector: () => box }, core, live, act, view, signal: control.signal,
  });
  fire('keydown', { key: 'Home' });
  await tick();
  assert.deepEqual(sent, [[0, 'dac']], 'Home asks the lowest step');
  fire('keydown', { key: 'ArrowUp' });
  await tick();
  assert.deepEqual(sent.at(-1), [1, 'dac'], 'an arrow nudges, from the step asked');
  control.abort();
});

await ok('signal_one_ask_and_one_nudge_reach_the_core', async () => {
  const { core, act, live, sent } = stubCore(level(50));
  const { box, fire } = fakeBox({ dataset: {} });
  const view = { asked: null, now: Date.now(), state: 'open' };
  const control = new AbortController();
  wireSignal({
    root: { querySelector: () => box }, core, live, act, view, keep: { take() {}, drop() {} },
    signal: control.signal,
  });
  fire('keydown', { key: 'Home' });
  await tick();
  assert.deepEqual(sent, [[0, 'dac']]);
  fire('keydown', { key: 'ArrowUp' });
  await tick();
  assert.deepEqual(sent.at(-1), [1, 'dac']);
  control.abort();
});

/** Night drive's dash over the rig's real store; `clock` is performance.now. */
function dashOn(raw) {
  const { window: w, document: d } = domPage(
    '<div id="nightdrive"><span id="nd-switch"></span><div id="nd-dash"></div></div>',
  );
  Object.assign(globalThis, { document: d, window: w });
  const volume = { ...level(raw), text: `${raw}`, unit: 'dB', mute: 'mute' };
  const rig = computerRig({ device: { ...DAC, volume }, side: 'device' });
  const sent = [];
  const control = new w.AbortController();
  const view = { state: 'open', phone: () => false, asked: null };
  const wired = wireDash({
    root: d.getElementById('nightdrive').parentNode,
    core: rig.core,
    live: { freeze() {}, thaw() {}, draw() {} },
    act: { act: async (_name, step) => { sent.push(step); return new Promise(() => {}); } },
    view,
    signal: control.signal,
  });
  wired.draw(rig.store.page());
  const press = (act) => d.querySelector(`[data-act="${act}"]`).click();
  return { sent, view, press, stop: () => control.abort(), core: rig.core };
}

await ok('night_drive_one_ask_and_one_nudge_reach_the_core', async () => {
  const r = dashOn(50);
  r.press('volume-up');
  await tick();
  assert.deepEqual(r.sent, [51]);
  r.press('volume-up');
  await tick();
  assert.deepEqual(r.sent, [51, 52], 'a raise goes on from the step asked');
  r.stop();
});

await ok('night_drive_a_press_at_the_floor_behind_a_stale_raise_is_sent', async () => {
  const real = globalThis.performance.now;
  let at = 1000;
  globalThis.performance.now = () => at;
  try {
    const r = dashOn(0);
    r.press('volume-up');
    await tick();
    assert.deepEqual(r.sent, [1], 'the raise is in flight, never answered');
    at += ASK_MS + 1;
    r.press('volume-down');
    await tick();
    assert.deepEqual(r.sent, [1, 0], 'the floor queues behind the raise: the device ends at the floor');
    r.stop();
  } finally {
    globalThis.performance.now = real;
  }
});

await ok('ember_one_ask_and_one_nudge_reach_the_core', async () => {
  const sent = [];
  const mount = new AbortController();
  ember.signal = mount.signal;
  ember.core = {
    actions: {
      setVolume: (step) => { sent.push(step); return Promise.resolve({ ok: true }); },
      canWrite: () => true,
    },
    model: { shown: () => 'dac', part: () => null },
    climb: { cancel() {}, paint() {} },
  };
  const knob = dom.document.createElement('sv-knob');
  Object.assign(knob.dataset, {
    lo: '0', hi: '100', raw: '50', target: '', cap: '', risk: '', inert: 'false', computer: 'false',
  });
  knob.innerHTML = '<svg class="dial"></svg>';
  dom.document.body.append(knob);
  const dial = knob.querySelector('svg.dial');
  const key = (name) => dial.dispatchEvent(new dom.window.KeyboardEvent('keydown', { key: name, bubbles: true }));
  key('Home');
  await tick();
  assert.deepEqual(sent, [0]);
  key('ArrowUp');
  await tick();
  assert.deepEqual(sent, [0, 1]);
  ember.signal = null;
  mount.abort();
});

console.log(`${n} core-volume-wires checks passed`);
