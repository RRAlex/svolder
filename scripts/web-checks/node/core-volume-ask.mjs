/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the step a page asked the volume for (core/volume-ask.js), which Signal, Paper,
// Night drive and Ember keep alike: the step shown until the device answers or the ask is old, the
// step a change starts from, the timer that ends a stale ask; and Night drive's dash starting from
// the reading, not the ask, once the device has answered and another hand moved it.
// Run: node core-volume-ask.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  ASK_MS, ackAsk, askExpiry, baseOf, createVolumeAsker, noteReading, shownRaw,
} from '../../../bins/svolderd/web/core/volume-ask.js';
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

await ok('a_reading_before_the_answer_settles_the_ask_when_the_answer_comes', () => {
  const view = { asked: { raw: 41, at: 0 } };
  assert.equal(noteReading(view, 41), false, 'before the answer a reading changes nothing yet');
  ackAsk(view, view.asked);
  assert.deepEqual([view.asked.acked, view.asked.answered], [true, true]);
  const other = { asked: { raw: 41, at: 0 } };
  noteReading(other, 30);
  ackAsk(other, other.asked);
  assert.equal(other.asked.answered, undefined, 'a reading of another step does not answer the ask');
  const late = { asked: { raw: 41, at: 0 } };
  ackAsk(late, late.asked);
  assert.equal(late.asked.answered, undefined);
  assert.equal(noteReading(late, 41), true, 'after the answer the reading answers the ask');
  assert.equal(late.asked.answered, true);
  assert.equal(noteReading({ asked: null }, 41), false);
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
function dashRig(raw, landed) {
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
      return landed === undefined ? { ok: true } : { ok: true, landed: { raw: landed } };
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
  assert.equal(r.view.asked.answered, true, 'the reading of the step came first: the answer settles it');
  r.store.applyPart(r.store.shown(), 'volume', r.volume(30));
  r.up();
  await r.tick();
  assert.equal(r.sent.at(-1), 31, 'from the reading, as when the answer comes first');
  r.stop();
});

await ok('night_drive_order_the_streams_reading_another_hands_reading_then_the_answer', async () => {
  const r = dashRig(40);
  r.up();
  await r.tick();
  r.store.emit('volume', { raw: 41, text: '41' });
  // another hand turns it down after the device read the step asked; both readings precede the answer
  r.store.applyPart(r.store.shown(), 'volume', r.volume(30));
  r.store.emit('volume', { raw: 30, text: '30' });
  r.release();
  await r.tick();
  r.up();
  await r.tick();
  assert.equal(r.sent.at(-1), 31, 'the newer reading is another hand\'s: from it, not from the ask');
  r.stop();
});

await ok('night_drive_the_reading_of_the_landed_step_before_the_answer_settles_the_ask', async () => {
  const r = dashRig(40, 42);
  r.up();
  await r.tick();
  r.store.emit('volume', { raw: 42, text: '42' });
  r.release();
  await r.tick();
  assert.equal(r.view.asked.raw, 42, 'the daemon landed the write on another step');
  assert.equal(r.view.asked.answered, true, 'the device had read the step it landed on');
  r.stop();
});

await ok('a_reading_of_another_step_before_the_answer_keeps_the_ask_and_a_later_one_after_the_step_changes_nothing', () => {
  const view = { asked: { raw: 41, at: 0 } };
  noteReading(view, 30);
  assert.equal(ackAsk(view, view.asked), false, 'our write came after that reading');
  assert.equal(view.asked.answered, undefined);
  const seen = { asked: { raw: 41, at: 0 } };
  noteReading(seen, 41);
  noteReading(seen, 30);
  assert.equal(ackAsk(seen, seen.asked), true, 'the step was read once, a later reading changes nothing');
  assert.equal(seen.asked.answered, true);
});

/** The core's asker over a volume reading `vol` (mutable), a clock `t.at`, and writes that wait for
 * `release(answer)`. */
function askerRig(vol, { guest = false } = {}) {
  const t = { at: 1000 };
  const view = { asked: null };
  const sent = [];
  const refused = [];
  const shows = [];
  const releases = [];
  const asker = createVolumeAsker({
    view,
    signal: new AbortController().signal,
    volume: () => vol,
    inert: () => guest,
    refuse: (step, device) => refused.push([step, device]),
    write: (step, device) => {
      sent.push([step, device]);
      return new Promise((resolve) => { releases.push(resolve); });
    },
    show: () => shows.push(view.asked?.raw ?? null),
    now: () => t.at,
    shown: () => 'dac',
  });
  return {
    asker, view, sent, refused, shows, t, answer: (a, i = releases.length - 1) => releases[i](a),
  };
}
const tick = () => new Promise((resolve) => { setTimeout(resolve, 0); });
const reading = (raw, more = {}) => ({
  raw, target: null, lo: 0, hi: 100, cap: null, ...more,
});

await ok('asker_same_ask_union_equal_to_shown_equal_to_fresh_equal_to_stale_different', () => {
  const r = askerRig(reading(50));
  r.asker.ask(50);
  assert.equal(r.sent.length, 0, 'equal to the shown level: nothing to change');
  r.asker.ask(60);
  assert.deepEqual(r.sent, [[60, 'dac']], 'a different step is sent, to the shown device');
  r.t.at += ASK_MS - 1;
  r.asker.ask(60);
  assert.equal(r.sent.length, 1, 'equal to a fresh ask in flight: no flood');
  r.t.at += 1;
  r.asker.ask(60);
  assert.equal(r.sent.length, 2, 'a lost ask older than ASK_MS is sent again');
  assert.equal(r.view.asked.at, r.t.at, 'the new ask is stamped by the handed clock');
  r.asker.ask(50);
  assert.equal(r.sent.length, 3, 'back to the level while another ask waits is a new ask');
});

await ok('asker_a_climb_may_be_stopped_by_asking_its_level_and_another_hands_step_is_new', () => {
  const r = askerRig(reading(50, { target: 70 }));
  r.asker.ask(50);
  assert.equal(r.sent.length, 1, 'the level during a glide is an ask');
  const q = askerRig(reading(50));
  q.asker.ask(60);
  q.view.asked.acked = true;
  q.view.asked.answered = true;
  q.asker.ask(60);
  assert.equal(q.sent.length, 2, 'answered, and the device reads another step: the same ask is new');
});

await ok('asker_clamps_to_range_and_cap_at_and_above_the_cap', () => {
  const r = askerRig(reading(50, { cap: 60 }));
  r.asker.ask(61);
  r.asker.ask(60);
  r.asker.ask(99.6);
  r.asker.ask(-5);
  assert.deepEqual(r.sent.map(([s]) => s), [60, 0], 'above the cap sends the cap; at the cap is the same ask');
  const wide = askerRig(reading(50, { hi: 80 }));
  wide.asker.ask(200);
  assert.equal(wide.sent[0][0], 80, 'above the range sends the top');
  const open = askerRig(reading(50, { cap: undefined }));
  open.asker.ask(99);
  assert.equal(open.sent[0][0], 99, 'no cap: no clamp');
});

await ok('asker_landed_fold_and_an_older_answer_does_not_move_the_shown_value_back', async () => {
  const r = askerRig(reading(50));
  r.asker.ask(60);
  const first = r.view.asked;
  r.answer({ ok: true, landed: { raw: 55 } });
  await tick();
  assert.equal(r.view.asked.raw, 55, 'the daemon landed on another step: that is the ask');
  assert.notEqual(r.view.asked, first);
  assert.equal(r.view.asked.acked, true);
  // a newer ask replaces it; the answer of the older one does not put it back
  const old = askerRig(reading(50));
  old.asker.ask(60);
  old.asker.ask(70);
  old.answer({ ok: true, landed: { raw: 40 } }, 0);
  await tick();
  assert.equal(old.view.asked.raw, 70, 'the newer ask stays shown');
  assert.equal(old.view.asked.acked, undefined, 'and is not acked by the older answer');
  // a refusal clears the ask and shows
  const bad = askerRig(reading(50));
  bad.asker.ask(60);
  bad.answer({ ok: false });
  await tick();
  assert.equal(bad.view.asked, null);
  assert.equal(bad.shows.at(-1), null);
  // queued/dropped answers change nothing
  const q = askerRig(reading(50));
  q.asker.ask(60);
  q.answer({ ok: true, queued: true });
  await tick();
  assert.equal(q.view.asked.raw, 60);
  assert.equal(q.view.asked.acked, undefined);
});

await ok('asker_a_guest_sends_nothing_and_the_core_says_its_words', () => {
  const r = askerRig(reading(50), { guest: true });
  r.asker.ask(60, 'amp');
  r.asker.nudge(1);
  assert.equal(r.sent.length, 0);
  assert.equal(r.view.asked, null, 'nothing is shown either');
  assert.deepEqual(r.refused, [[60, 'amp'], [51, 'dac']], 'the core is asked, which says why');
});

await ok('asker_nudge_starts_from_the_ask_and_the_clock_is_handed_in', () => {
  const src = readFileSync(new URL('../../../bins/svolderd/web/core/volume-ask.js', import.meta.url), 'utf8');
  assert.ok(!/Date\.now|performance\.now/.test(src.replace(/\/\/.*|\/\*[\s\S]*?\*\//g, '')),
    'the module reads no clock of its own');
  const r = askerRig(reading(50));
  r.asker.nudge(1);
  r.asker.nudge(1);
  assert.deepEqual(r.sent.map(([s]) => s), [51, 52], 'a raise goes on from the step asked');
  r.asker.nudge(-1);
  assert.equal(r.sent.at(-1)[0], 49, 'a lowering goes from the level');
  const none = askerRig({ raw: null });
  none.asker.nudge(1);
  none.asker.ask(5);
  assert.equal(none.sent.length, 0, 'an unread volume asks nothing');
});

await ok('asker_never_writes_a_theme_view_but_its_asked_a_getter_only_now_is_fine', async () => {
  const view = { asked: null, get now() { return 0; } };
  const asker = createVolumeAsker({
    view,
    signal: new AbortController().signal,
    volume: () => reading(50),
    inert: () => false,
    refuse() {},
    write: async () => ({ ok: true }),
    now: () => 5,
  });
  asker.nudge(1);
  assert.equal(view.asked.raw, 51);
  assert.equal(asker.base(reading(50), 1), 51);
});

await ok('asker_skips_only_a_no_op_or_the_fresh_ask_in_flight', () => {
  // (a) nothing in flight and the level asked: a no-op
  const idle = askerRig(reading(50));
  idle.asker.ask(50);
  assert.equal(idle.sent.length, 0);
  // (a) an answered ask is not in flight: the level is still a no-op
  const done = askerRig(reading(50));
  done.view.asked = { raw: 60, at: 1000, acked: true, answered: true };
  done.asker.ask(50);
  assert.equal(done.sent.length, 0);
  // (b) the ask in flight, while fresh
  const fresh = askerRig(reading(50));
  fresh.asker.ask(60);
  fresh.t.at += ASK_MS - 1;
  fresh.asker.ask(60);
  assert.equal(fresh.sent.length, 1);
  // a stale raise still unanswered: the floor press is sent behind it (the device ends at the floor)
  const stale = askerRig(reading(0));
  stale.asker.ask(1);
  stale.t.at += ASK_MS + 1;
  stale.asker.ask(0);
  assert.deepEqual(stale.sent.map(([s]) => s), [1, 0]);
  // a different step while one is fresh and in flight is sent too
  const other = askerRig(reading(0));
  other.asker.ask(1);
  other.asker.ask(0);
  assert.deepEqual(other.sent.map(([s]) => s), [1, 0]);
});

console.log(`${n} core-volume-ask checks passed`);
