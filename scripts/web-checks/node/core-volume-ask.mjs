/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the step a page asked the volume for (core/volume-ask.js), which Signal, Paper
// and Night drive keep alike: the step shown until the device answers or the ask is old, the step
// a change starts from, the timer that ends a stale ask; and Night drive's dash starting from the
// reading, not the ask, once the device has answered and another hand moved it.
// Run: node core-volume-ask.mjs
import assert from 'node:assert/strict';
import { ASK_MS, askExpiry, baseOf, shownRaw } from '../../../bins/svolderd/web/core/volume-ask.js';
import { page as domPage } from './dom.mjs';
import { DAC, computerRig } from './nightdrive-rig.mjs';
import { wireDash } from '../../../bins/svolderd/web/themes/nightdrive/dash-wire.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const v = (raw, target = null) => ({ raw, target });
const asked = (raw, at, answered = false) => ({ raw, at, answered });

await ok('shown_raw_is_the_ask_until_the_device_answers_or_the_ask_is_old', () => {
  assert.equal(shownRaw(v(50), { asked: null, now: 0 }), 50, 'no ask: the reading');
  assert.equal(shownRaw(v(50), { asked: asked(50, 0), now: 10 }), 50, 'reading equals the ask');
  assert.equal(shownRaw(v(50), { asked: asked(60, 0), now: ASK_MS }), 50, 'an ask older than ASK_MS');
  assert.equal(shownRaw(v(50), { asked: asked(60, 0, true), now: 10 }), 50,
    'answered and not gliding shows the reading');
  assert.equal(shownRaw(v(50, 70), { asked: asked(60, 0, true), now: 10 }), 60,
    'gliding keeps the ask');
  assert.equal(shownRaw(v(50), { asked: asked(60, 0), now: 10 }), 60, 'unanswered keeps the ask');
  assert.equal(shownRaw(v(null), { asked: asked(60, 0), now: 10 }), null, 'unread stays unread');
});

await ok('a_change_starts_from_the_ask_while_it_is_shown_else_from_the_reading', () => {
  const view = (a, now = 10) => ({ asked: a, now });
  assert.equal(baseOf(v(50), view(asked(60, 0)), 1), 60, 'a raise from the ask');
  assert.equal(baseOf(v(50), view(asked(60, 0)), -1), 50, 'a lowering from the level');
  assert.equal(baseOf(v(50), view(asked(40, 0)), -1), 40, 'a lowering below the ask goes on from it');
  assert.equal(baseOf(v(50), view(asked(60, 0, true)), 1), 50, 'another hand moved it after the answer');
  assert.equal(baseOf(v(50), view(asked(60, 0), ASK_MS), 1), 50, 'an old ask is forgotten');
  assert.equal(baseOf(v(50, 70), view(null), 1), 70, 'a raise in a climb goes on from its target');
  assert.equal(baseOf(v(50, 70), view(null), -1), 50);
});

await ok('the_expiry_clears_a_stale_ask_once_and_redraws', () => {
  const real = { setTimeout: globalThis.setTimeout, clearTimeout: globalThis.clearTimeout };
  const pending = new Map();
  let next = 1;
  globalThis.setTimeout = (fn, ms) => { pending.set(next, { fn, ms }); return next++; };
  globalThis.clearTimeout = (id) => { pending.delete(id); };
  try {
    const view = { asked: asked(60, 0) };
    let now = 0;
    let redraws = 0;
    const control = new AbortController();
    const expiry = askExpiry({ view, now: () => now, redraw: () => { redraws++; }, signal: control.signal });
    expiry.arm();
    expiry.arm();
    assert.equal(pending.size, 1, 'arming again replaces the timer');
    const [{ fn, ms }] = [...pending.values()];
    assert.ok(ms > ASK_MS);
    now = ASK_MS - 1;
    fn();
    assert.deepEqual([view.asked?.raw, redraws], [60, 0], 'not yet old');
    now = ASK_MS + 50;
    fn();
    assert.deepEqual([view.asked, redraws], [null, 1]);
    expiry.arm();
    control.abort();
    assert.equal(pending.size, 0, 'an abort stops it');
  } finally {
    Object.assign(globalThis, real);
  }
});

/** Night drive's dash over the real store and listener, the device's volume at `raw`; the act's answer is
 * held until `release()` so a case can put the stream's reading before or after it. */
function dashRig(raw) {
  const { window, document } = domPage('<div id="nightdrive"><span id="nd-switch"></span><div id="nd-dash"></div></div>');
  Object.assign(globalThis, { document, window });
  const volume = (at) => ({ text: `${at}`, unit: 'dB', muted: false, raw: at, lo: 0, hi: 100, mute: 'mute', target: null });
  const rig = computerRig({ device: { ...DAC, volume: volume(raw) }, side: 'device' });
  const { core, store } = rig;
  const sent = [];
  let release = () => {};
  const control = new window.AbortController();
  const view = { state: 'open', phone: () => false, asked: null };
  const act = {
    act: async (_name, step) => {
      sent.push(step);
      await new Promise((resolve) => { release = resolve; });
      return { ok: true };
    },
  };
  const wired = wireDash({
    root: document.getElementById('nightdrive').parentNode, core, live: { freeze() {}, thaw() {}, draw() {} }, act, view,
    signal: control.signal,
  });
  wired.draw(store.page());
  const up = () => document.querySelector('[data-act="volume-up"]').click();
  const tick = () => new Promise((resolve) => { window.setTimeout(resolve, 0); });
  return {
    store, view, sent, volume, up, tick, stop: () => control.abort(), release: () => release(),
  };
}

await ok('night_drive_order_answer_then_the_streams_reading_then_another_hand', async () => {
  const r = dashRig(40);
  r.up();
  await r.tick();
  assert.deepEqual(r.sent, [41]);
  r.release();
  await r.tick();
  assert.equal(r.view.asked.acked, true);
  // the stream reads the step asked: the ask is answered
  r.store.emit('volume', { raw: 41, text: '41' });
  assert.equal(r.view.asked.answered, true);
  // another hand turns it down, folded as a part (no listener runs): the next step goes on from it
  r.store.applyPart(r.store.shown(), 'volume', r.volume(30));
  r.up();
  await r.tick();
  assert.equal(r.sent.at(-1), 31, 'from the reading, not from the ask of 41');
  r.stop();
});

await ok('night_drive_order_the_streams_reading_then_the_answer', async () => {
  const r = dashRig(40);
  r.up();
  await r.tick();
  // the stream's reading of the step asked comes before the write's answer: nothing is answered yet
  r.store.emit('volume', { raw: 41, text: '41' });
  assert.equal(r.view.asked.answered, undefined);
  r.release();
  await r.tick();
  assert.equal(r.view.asked.acked, true);
  assert.equal(r.view.asked.answered, undefined, 'the reading came first: the ask stands until ASK_MS');
  r.store.applyPart(r.store.shown(), 'volume', r.volume(30));
  r.up();
  await r.tick();
  assert.equal(r.sent.at(-1), 42, 'still from the ask until it is old');
  r.stop();
});

console.log(`${n} core-volume-ask checks passed`);
