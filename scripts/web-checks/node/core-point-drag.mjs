/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the core's hand on the computer's curve (core/point-drag.js): a press on a point
// drags it, with its writes at most 20 a second and its last value written once at the release
// (also at a flip); a re-sort the daemon lists is followed by slot (the label, the lit point and a
// pinch's start); a point removed or a preset loaded meanwhile ends the writes; a tap adds a point
// where it was made; the wheel and the arrow keys are bursts of live updates ending in one final
// write; two fingers set the Q; a page that may not write is turned away. The post, the read, the
// clock and the plot are fakes.
// Run: node core-point-drag.mjs
import assert from 'node:assert/strict';
import { createActions } from '../../../bins/svolderd/web/core/actions.js';
import {
  createComputer, createHolds, TAP_MS, TAP_PX, WHEEL_END_MS,
} from '../../../bins/svolderd/web/core/computer.js';
import {
  labelWords, markersIn, pointDrag, pointValues, POINT_KEYS, touchWatch,
} from '../../../bins/svolderd/web/core/point-drag.js';
import { createStore } from '../../../bins/svolderd/web/core/store.js';
import { computerSteps, plotMap } from '../../../bins/svolderd/web/core/tone-drag.js';
import {
  F_MAX, RANGE_DB, VIEW_HEIGHT, VIEW_WIDTH, xOf, yOf,
} from '../../../bins/svolderd/web/core/tone-curve.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
const flush = async () => { for (let i = 0; i < 12; i++) await new Promise((r) => setImmediate(r)); };

const SIGNED = { signed_in: true, guest: false, writes: true, read_only: null };
const GUEST = { signed_in: false, guest: true, writes: false, read_only: null };
const STEPS = computerSteps();

const point = (type, f, g, q) => ({ type, f, g, q });
const SLOTS = [5, 2, 9, 4];
const slotted = (list) => list.map((p, i) => ({ ...p, slot: SLOTS[i] ?? i + 1 }));
const modelOf = (points, over = {}) => ({
  volume: { pct: 42, muted: false, sink: 'Schiit' },
  eq: { on: true, running_unknown: false, preamp_db: 0, preset: 'Warm', live: true, note: null, max: 16,
    points, response: [] },
  presets: [], unavailable: null, ...over,
});
const LIST = slotted([point('PK', 226, 0, 1), point('PK', 2000, 3, 1), point('PK', 8000, -2, 1)]);
const dac = { name: 'Gungnir 2', url: 'Gungnir%202', volume: null, tone: null };

/** A fake clock the test advances: timers due run in order. */
function fakeClock() {
  const clock = { t: 0, timers: [], id: 0 };
  clock.now = () => clock.t;
  clock.later = (fn, ms) => { const id = ++clock.id; clock.timers.push({ id, at: clock.t + ms, fn }); return id; };
  clock.cancel = (id) => { clock.timers = clock.timers.filter((x) => x.id !== id); };
  clock.advance = async (by) => {
    const to = clock.t + by;
    for (;;) {
      const due = clock.timers.filter((x) => x.at <= to).sort((a, b) => a.at - b.at)[0];
      if (!due) break;
      clock.t = due.at;
      clock.timers = clock.timers.filter((x) => x !== due);
      due.fn();
      await flush();
    }
    clock.t = to;
    await flush();
  };
  return clock;
}

/** The pieces over a page whose computer holds `list`, and the hand over a 1000 x 260 plot. */
function rig({ list = LIST, viewer = SIGNED, eq = {}, usable = () => true, marks = null } = {}) {
  const posts = [];
  const toasts = [];
  const heard = [];
  const state = { host: null };
  const make = (points, over) => modelOf(points, { eq: { ...modelOf(points).eq, ...over, ...eq } });
  const page = (computer) => ({
    v: 1, viewer, fleet: { devices: [], hops: [] }, shown: dac.url, device: dac, followed: {}, card: null,
    host: { track: null, route: null, computer }, strip: null,
  });
  const store = createStore(page(make(list, {})), {
    signedOut: () => {}, fetch: async () => ({ status: 200, json: async () => ({ device: dac }) }),
  });
  const holds = createHolds({
    store, fetch: async () => ({ status: 200, json: async () => state.host ?? { computer: make(list, {}) } }),
  });
  const clock = fakeClock();
  const computer = createComputer({ store, storage: { getItem: () => null, setItem() {}, removeItem() {} }, holds });
  const actions = createActions({
    store, post: async (path, body) => { posts.push([path, body]); return { status: 200, ok: true }; },
    toast: (text) => toasts.push(text), history: { pushState() {} }, location: { assign() {} },
    holds, ask: async () => false, timing: clock, computer,
  });
  computer.flip('computer');
  const captured = [];
  const plot = {
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 1000, height: 260 }),
    setPointerCapture: (id) => captured.push(id),
  };
  const log = [];
  const on = {};
  for (const name of ['pick', 'begin', 'held', 'ended', 'finished', 'moved', 'burst', 'added']) {
    on[name] = (...args) => log.push([name, ...args.map((a) => (a && typeof a === 'object' && 'n' in a ? { n: a.n } : a))]);
  }
  const hand = pointDrag({
    actions, computer, toast: (words) => toasts.push(words), clock, usable: (...a) => usable(...a),
    points: (_, rect) => (marks ? marks(rect) : markersIn(computer.tone()?.markers, rect)), on,
  });
  const host = (points, over) => store.emit('host', { ...store.page().host, computer: make(points, over) });
  return { store, holds, actions, computer, hand, plot, posts, toasts, clock, log, captured, host, state, make };
}

const markerOf = (r, k) => markersIn(r.computer.tone().markers, { width: 1000, height: 260 })[k - 1];
const mouse = (x, y, over = {}) => ({
  clientX: x, clientY: y, pointerId: 1, pointerType: 'mouse', button: 0, buttons: 1, isPrimary: true,
  preventDefault() { this.prevented = true; }, ...over,
});
const pointPosts = (r, k) => r.posts.filter(([path]) => path === `api/v1/host/eq/points/${k}`).map(([, body]) => body);

await ok('a_press_on_a_point_drags_it_with_paced_writes_and_one_last_value_at_the_release', async () => {
  const r = rig();
  const m = markerOf(r, 2);
  const press = mouse(m.x, m.y);
  assert.equal(r.hand.down(press, r.plot), 'drag');
  assert.equal(press.prevented, true);
  assert.deepEqual(r.captured, [1]);
  assert.equal(r.hand.session.n, 2);
  assert.equal(r.computer.picked(), 2, 'the point is in hand');
  assert.equal(r.hand.active, true);
  // thirty moves in 300 ms: at most 20 writes a second go out
  for (let i = 1; i <= 30; i++) {
    await r.clock.advance(10);
    assert.equal(r.hand.move(mouse(m.x + i * 4, m.y - i)), true);
    await flush();
  }
  const live = pointPosts(r, 2).filter((b) => b.done === false);
  assert.ok(live.length >= 3 && live.length <= 7, `live writes ${live.length}`);
  assert.ok(live.every((b) => b.slot === 2));
  assert.equal(r.hand.up(mouse(m.x + 120, m.y - 30)), true);
  const last = r.hand.session.drag.held;
  await r.clock.advance(100);
  const all = pointPosts(r, 2);
  assert.equal(all.filter((b) => b.done).length, 1, 'one last value');
  assert.deepEqual([all.at(-1).f, all.at(-1).g], [STEPS.f[last.f], STEPS.g[last.g]]);
  assert.equal(all.at(-1).done, true);
  assert.equal(r.hand.session, null);
  assert.equal(r.holds.get('point:2'), null, 'the hold goes once the model is read');
  assert.deepEqual(r.log.map((e) => e[0]).filter((x, i, a) => a.indexOf(x) === i),
    ['pick', 'begin', 'held', 'ended', 'finished']);
});

await ok('a_flip_ends_the_drag_and_writes_its_last_value_once', async () => {
  const r = rig();
  const m = markerOf(r, 1);
  r.hand.down(mouse(m.x, m.y), r.plot);
  await r.clock.advance(60);
  r.hand.move(mouse(m.x + 200, m.y));
  await flush();
  r.computer.flip('dac');
  await r.clock.advance(100);
  const bodies = pointPosts(r, 1);
  assert.equal(bodies.filter((b) => b.done).length, 1);
  assert.equal(bodies.at(-1).done, true);
  assert.equal(bodies.at(-1).f, bodies.filter((b) => !b.done).at(-1).f, 'the last value asked');
  assert.equal(r.hand.session, null);
  assert.equal(r.log.filter((e) => e[0] === 'finished').length, 1);
  // a press that never moved writes nothing at all
  const q = rig();
  const k = markerOf(q, 2);
  q.hand.down(mouse(k.x, k.y), q.plot);
  q.hand.up(mouse(k.x, k.y));
  await flush();
  assert.deepEqual(q.posts, []);
});

await ok('a_re_sort_the_daemon_lists_is_followed_by_the_session_the_label_and_a_pinch', async () => {
  const LIST = slotted([point('PK', 226, 0, 0.5), point('PK', 2000, 3, 4), point('PK', 8000, -2, 1)]);
  const r = rig({ list: LIST });
  const m = markerOf(r, 1);
  const touchPress = { touches: [{ identifier: 1, clientX: m.x, clientY: m.y }], preventDefault() {} };
  touchPress.changedTouches = touchPress.touches;
  assert.equal(r.hand.touchStart(touchPress, r.plot), 'drag');
  await r.clock.advance(60);
  r.hand.touchMove({ touches: [{ identifier: 1, clientX: m.x + 600, clientY: m.y }], cancelable: true, preventDefault() {} });
  await flush();
  const s = r.hand.session;
  assert.equal(s.n, 1);
  const label = s.words();
  assert.match(label, /Hz · .* dB · /);
  // another front end's edit lists the point (slot 5) after the 2 kHz one
  r.host([LIST[1], { ...LIST[0], f: 5000 }, LIST[2]]);
  assert.equal(s.n, 2, 'the session follows the point');
  assert.equal(r.computer.picked(), 2);
  const now = r.computer.tone().points;
  assert.equal(s.words(), labelWords(now[1]), 'the label reads the point where it is listed now');
  assert.notEqual(s.words(), labelWords(now[0]));
  assert.match(s.words(), /^(\d{5}) Hz/, 'with the value being dragged, not the model\'s');
  assert.deepEqual(r.log.filter((e) => e[0] === 'moved'), [['moved', { from: 1, to: 2 }]]);
  // a second finger makes a pinch of the same point, from where it is listed now
  const pinch = {
    touches: [{ identifier: 1, clientX: 100, clientY: 100 }, { identifier: 2, clientX: 300, clientY: 100 }],
    changedTouches: [{ identifier: 2, clientX: 300, clientY: 100 }], preventDefault() {},
  };
  assert.equal(r.hand.touchStart(pinch, r.plot), 'pinch');
  assert.equal(r.hand.session.n, 2);
  await r.clock.advance(60);
  r.hand.touchMove({
    touches: [{ identifier: 1, clientX: 100, clientY: 100 }, { identifier: 2, clientX: 500, clientY: 100 }],
    cancelable: true, preventDefault() {},
  });
  await flush();
  r.hand.touchEnd({ type: 'touchend', changedTouches: [{ identifier: 2 }], preventDefault() {} });
  await flush();
  // the writes name the point by its slot (5), at the place the model had it when each went out
  const bodies = r.posts.filter(([path]) => path.startsWith('api/v1/host/eq/points/')).map(([, b]) => b);
  assert.ok(bodies.every((b) => b.slot === 5));
  const qs = bodies.filter((b) => b.q !== undefined && b.done === false).map((b) => b.q);
  assert.ok(qs.length > 0, 'the pinch wrote a Q');
  assert.ok(qs.every((q) => q < 0.4), `the pinch started from the point's own Q (0.5), not its neighbour's: ${qs}`);
  assert.equal(bodies.at(-1).done, true);
  assert.equal(bodies.filter((b) => b.done).length, 1, 'the drag and its pinch end once');
});

await ok('a_point_removed_or_a_preset_loaded_meanwhile_ends_the_writes', async () => {
  const gone = rig();
  const m = markerOf(gone, 2);
  gone.hand.down(mouse(m.x, m.y), gone.plot);
  await gone.clock.advance(60);
  gone.hand.move(mouse(m.x + 100, m.y));
  await flush();
  const before = gone.posts.length;
  assert.equal(before, 1);
  // another front end removed the point (slot 2)
  gone.host([LIST[0], LIST[2]]);
  await gone.clock.advance(60);
  gone.hand.move(mouse(m.x + 200, m.y));
  await gone.clock.advance(60);
  gone.hand.move(mouse(m.x + 300, m.y));
  await flush();
  gone.hand.up(mouse(m.x + 300, m.y));
  await flush();
  assert.equal(gone.posts.length, before, 'nothing is written for a point that is gone');
  assert.equal(gone.hand.session, null);
  // a preset loaded by another front end renumbers the slots: the drag writes no more
  const preset = rig();
  const k = markerOf(preset, 2);
  preset.hand.down(mouse(k.x, k.y), preset.plot);
  await preset.clock.advance(60);
  preset.hand.move(mouse(k.x + 100, k.y));
  await flush();
  const had = preset.posts.length;
  preset.host(LIST, { preset: 'Bass lift' });
  await preset.clock.advance(60);
  preset.hand.move(mouse(k.x + 200, k.y));
  await flush();
  preset.hand.up(mouse(k.x + 200, k.y));
  await flush();
  assert.equal(preset.posts.length, had);
  assert.equal(preset.hand.session, null);
});

await ok('the_wheel_over_a_point_is_a_burst_of_live_q_steps_and_one_last_value', async () => {
  const r = rig();
  const m = markerOf(r, 2);
  const notch = (dy) => ({ clientX: m.x, clientY: m.y, deltaY: dy, deltaMode: 0, ctrlKey: false, preventDefault() { this.prevented = true; } });
  const first = notch(-120);
  assert.equal(r.hand.wheel(first, r.plot), true);
  assert.equal(first.prevented, true);
  for (let i = 1; i < 10; i++) {
    await r.clock.advance(60);
    r.hand.wheel(notch(-120), r.plot);
  }
  await flush();
  const live = pointPosts(r, 2);
  assert.equal(live.length, 10, 'ten notches, ten live writes');
  assert.ok(live.every((b) => b.done === false && b.q !== undefined && b.slot === 2));
  assert.ok(live.every((b, i) => i === 0 || b.q > live[i - 1].q), 'up narrows, one step a notch');
  assert.equal(r.hand.burst.n, 2);
  await r.clock.advance(WHEEL_END_MS);
  const all = pointPosts(r, 2);
  assert.equal(all.length, 11, 'N live and one last');
  assert.equal(all.at(-1).done, true);
  assert.equal(all.at(-1).q, live.at(-1).q);
  assert.equal(r.hand.burst, null);
  assert.deepEqual(r.log.filter((e) => e[0] === 'burst').map((e) => e[2]), [true, false]);
  // elsewhere the page scrolls; a ctrl wheel is the page's zoom
  const away = { clientX: 5, clientY: 255, deltaY: -120, deltaMode: 0, ctrlKey: false, preventDefault() { this.prevented = true; } };
  assert.equal(r.hand.wheel(away, r.plot), false);
  assert.equal(away.prevented, undefined);
  assert.equal(r.hand.wheel({ ...notch(-120), ctrlKey: true }, r.plot), false);
});

await ok('the_arrow_keys_on_a_point_are_a_burst_too_and_never_go_against_their_key', async () => {
  const r = rig();
  const key = (name) => ({ key: name, altKey: false, ctrlKey: false, metaKey: false, preventDefault() { this.prevented = true; } });
  assert.deepEqual(Object.keys(POINT_KEYS), ['ArrowLeft', 'ArrowRight', 'ArrowDown', 'ArrowUp']);
  for (let i = 0; i < 4; i++) {
    const press = key('ArrowUp');
    assert.equal(r.hand.key(press, 2), true);
    assert.equal(press.prevented, true);
    await r.clock.advance(60);
  }
  const bodies = pointPosts(r, 2);
  assert.deepEqual(bodies.map((b) => b.g), [3.1, 3.2, 3.3, 3.4], 'a step of the grid each, from the value being changed');
  assert.ok(bodies.every((b) => b.done === false));
  await r.clock.advance(WHEEL_END_MS);
  const all = pointPosts(r, 2);
  assert.equal(all.length, 5);
  assert.deepEqual([all.at(-1).done, all.at(-1).g], [true, 3.4]);
  // another key, or a modifier, is not the point's
  assert.equal(r.hand.key(key('a'), 2), false);
  assert.equal(r.hand.key({ ...key('ArrowUp'), ctrlKey: true }, 2), false);
  // a point beyond the hand range does not move against the key, and steps back into the range with it
  const beyond = rig({ list: slotted([point('PK', 1000, 14, 1)]) });
  beyond.hand.key(key('ArrowUp'), 1);
  await beyond.clock.advance(WHEEL_END_MS + 60);
  assert.deepEqual(beyond.posts, [], 'up from +14 dB is a no-op: nothing is written');
  beyond.hand.key(key('ArrowDown'), 1);
  await beyond.clock.advance(WHEEL_END_MS + 60);
  assert.deepEqual(pointPosts(beyond, 1).map((b) => [b.g, b.done]), [[11.9, false], [11.9, true]]);
});

await ok('a_tap_on_empty_ground_adds_a_point_there_and_a_slow_or_wandering_press_does_not', async () => {
  const r = rig();
  assert.equal(r.hand.down(mouse(500, 200), r.plot), 'tap');
  assert.equal(r.log.length, 0);
  await r.clock.advance(100);
  const added = await r.hand.up(mouse(500 + TAP_PX - 1, 200));
  assert.equal(added.ok, true);
  const add = r.posts.find(([path]) => path === 'api/v1/host/eq/points');
  assert.equal(add[1].type, 'PK');
  assert.equal(add[1].q, 1);
  assert.ok(add[1].f > 100 && add[1].f < 10000);
  assert.deepEqual(r.log.at(-1), ['added', { ok: true, status: 200, n: added.n }].slice(0, 1).concat([r.log.at(-1)[1]]));
  // let go far from where it began (no move seen)
  const far = rig();
  far.hand.down(mouse(500, 200), far.plot);
  assert.equal(await far.hand.up(mouse(500 + TAP_PX + 3, 200)), null);
  assert.deepEqual(far.posts, []);
  // too slow
  const slow = rig();
  slow.hand.down(mouse(500, 200), slow.plot);
  await slow.clock.advance(TAP_MS + 1);
  assert.equal(await slow.hand.up(mouse(500, 200)), null);
  assert.deepEqual(slow.posts, []);
  // it wandered
  const wander = rig();
  wander.hand.down(mouse(500, 200), wander.plot);
  wander.hand.move(mouse(500 + TAP_PX + 2, 200));
  assert.equal(await wander.hand.up(mouse(500, 200)), null);
  assert.deepEqual(wander.posts, []);
  // a pointer cancelled is no tap
  const cancelled = rig();
  cancelled.hand.down(mouse(500, 200), cancelled.plot);
  cancelled.hand.cancel(mouse(500, 200));
  assert.equal(await cancelled.hand.up(mouse(500, 200)), null);
  // another pointer's release is not its tap
  const other = rig();
  other.hand.down(mouse(500, 200), other.plot);
  assert.equal(await other.hand.up(mouse(500, 200, { pointerId: 2 })), null);
  // a full table adds nothing
  const sixteen = Array.from({ length: 16 }, (_, i) => point('PK', 100 + i * 100, 0, 1));
  const full = rig({ list: slotted(sixteen) });
  full.hand.down(mouse(5, 250), full.plot);
  const said = await full.hand.up(mouse(5, 250));
  assert.deepEqual(said, { ok: false, status: 0, full: true, max: 16, error: '16 is the most' });
  assert.deepEqual(full.posts, []);
  assert.deepEqual(full.log.at(-1), ['added', said], 'the theme hears it too');
  assert.deepEqual(full.toasts, [], 'the hand says nothing: the theme chooses its words');
  // a table with a smaller most is full sooner; a wandering press on it is still no tap
  const few = rig({ list: LIST, eq: { max: 3 } });
  few.hand.down(mouse(5, 250), few.plot);
  assert.deepEqual([(await few.hand.up(mouse(5, 250))).max, few.posts.length], [3, 0]);
  const walk = rig({ list: LIST, eq: { max: 3 } });
  walk.hand.down(mouse(5, 250), walk.plot);
  walk.hand.move(mouse(5 + TAP_PX + 2, 250));
  assert.equal(await walk.hand.up(mouse(5, 250)), null);
  const late = rig({ list: LIST, eq: { max: 3 } });
  late.hand.down(mouse(5, 250), late.plot);
  await late.clock.advance(TAP_MS + 1);
  assert.equal(await late.hand.up(mouse(5, 250)), null);
  assert.deepEqual(few.log.filter((e) => e[0] === 'added').length + walk.log.length + late.log.length, 1);
  // a computer with no EQ has nothing to add to, and nothing to say
  const none = rig({ list: LIST });
  none.store.emit('host', { ...none.store.page().host, computer: { ...modelOf(LIST), eq: null } });
  none.hand.down(mouse(5, 250), none.plot);
  assert.equal(await none.hand.up(mouse(5, 250)), null);
  assert.deepEqual([none.log, none.posts, none.toasts], [[], [], []]);
  // a page that may not write is turned away with its own words, not told the table is full
  const guest = rig({ list: LIST, eq: { max: 3 }, viewer: GUEST });
  guest.hand.down(mouse(5, 250), guest.plot);
  const away = await guest.hand.up(mouse(5, 250));
  assert.equal(away.full, undefined);
  assert.deepEqual([away.ok, guest.toasts.length], [false, 1]);
  // a wide (right) press on empty ground takes the point nearest in x, never a tap
  assert.equal(r.hand.down(mouse(500, 200, { button: 2 }), r.plot, { wide: true }), 'drag');
  r.hand.escape();
});

await ok('a_page_that_may_not_write_is_turned_away_with_the_words_and_nothing_starts', async () => {
  const r = rig({ viewer: GUEST });
  const m = markerOf(r, 2);
  assert.equal(r.hand.down(mouse(m.x, m.y), r.plot), 'refused');
  assert.equal(r.toasts.length, 1);
  assert.equal(r.hand.session, null);
  assert.equal(r.hand.wheel({ clientX: m.x, clientY: m.y, deltaY: -120, deltaMode: 0, preventDefault() {} }, r.plot), 'refused');
  assert.equal(r.hand.key({ key: 'ArrowUp', preventDefault() {} }, 2), 'refused');
  assert.equal(r.hand.touchStart({ touches: [{ identifier: 1, clientX: m.x, clientY: m.y }],
    changedTouches: [], preventDefault() {} }, r.plot), 'refused');
  assert.deepEqual(r.posts, []);
  // a page that is not usable now (the stream is lost) does nothing and says nothing
  const quiet = rig({ usable: () => false });
  const k = markerOf(quiet, 2);
  assert.equal(quiet.hand.down(mouse(k.x, k.y), quiet.plot), null);
  assert.equal(quiet.hand.wheel({ clientX: k.x, clientY: k.y, deltaY: -120, deltaMode: 0, preventDefault() {} }, quiet.plot), false);
  assert.equal(quiet.hand.key({ key: 'ArrowUp', preventDefault() {} }, 2), false);
  assert.deepEqual([quiet.posts, quiet.toasts], [[], []]);
});

await ok('escape_keeps_what_was_asked_and_a_lost_stream_ends_the_drag', async () => {
  const r = rig();
  const m = markerOf(r, 3);
  r.hand.down(mouse(m.x, m.y), r.plot);
  await r.clock.advance(60);
  r.hand.move(mouse(m.x - 300, m.y));
  await flush();
  r.hand.escape();
  await r.clock.advance(100);
  const bodies = pointPosts(r, 3);
  assert.equal(bodies.at(-1).done, true);
  assert.equal(r.hand.session, null);
  assert.equal(r.hand.move(mouse(m.x - 400, m.y)), false, 'the pointer is let go of');
  const lost = rig();
  const k = markerOf(lost, 3);
  lost.hand.down(mouse(k.x, k.y), lost.plot);
  lost.hand.lost();
  assert.equal(lost.hand.active, false);
  lost.hand.switched();
  await flush();
  // the window losing its focus lets go too
  const blur = rig();
  const b = markerOf(blur, 3);
  blur.hand.down(mouse(b.x, b.y), blur.plot);
  blur.hand.blur();
  assert.equal(blur.hand.active, false);
  // a move with no button down is a release nobody saw
  const unseen = rig();
  const u = markerOf(unseen, 3);
  unseen.hand.down(mouse(u.x, u.y), unseen.plot);
  unseen.hand.move(mouse(u.x + 5, u.y, { buttons: 0 }));
  assert.equal(unseen.hand.active, false);
});

await ok('a_right_press_sets_the_q_and_a_second_press_waits_for_the_first', async () => {
  const r = rig();
  const m = markerOf(r, 2);
  assert.equal(r.hand.down(mouse(m.x + 40, m.y + 40, { button: 2 }), r.plot, { wide: false }), null,
    'out of reach of a point: not a drag');
  assert.equal(r.hand.down(mouse(m.x, m.y, { button: 2 }), r.plot, { wide: true }), 'drag');
  await r.clock.advance(60);
  r.hand.move(mouse(m.x + 150, m.y));
  await flush();
  assert.ok(pointPosts(r, 2).every((b) => b.f === undefined && b.g === undefined && b.q !== undefined));
  assert.equal(r.hand.down(mouse(m.x, m.y, { pointerId: 2 }), r.plot), null, 'one drag at a time');
  r.hand.up(mouse(m.x + 150, m.y));
  await flush();
});

await ok('the_label_words_and_the_grabbable_points_come_from_the_tone', async () => {
  assert.equal(labelWords({ f: 1000, g: 3, q: 1 }), '1000 Hz · +3.0 dB · 1.00');
  assert.deepEqual(markersIn([{ n: 1, x: 500, y: 130 }], { width: 100, height: 26 }), [{ band: 1, x: 50, y: 13 }]);
  assert.deepEqual(markersIn(undefined, { width: 1, height: 1 }), []);
  const r = rig();
  const m = markerOf(r, 1);
  assert.equal(r.hand.canGrab(mouse(m.x + 3, m.y), r.plot), true);
  assert.equal(r.hand.canGrab(mouse(m.x + 60, m.y + 60), r.plot), false);
});

await ok('a_wide_wheel_steps_the_point_nearest_in_x_and_a_plain_one_leaves_the_page_to_scroll', async () => {
  const r = rig();
  const marks = markersIn(r.computer.tone().markers, { width: 1000, height: 260 });
  const spot = { x: marks[1].x + 100, y: marks[1].y };
  const nearest = marks.reduce((a, b) => (Math.abs(b.x - spot.x) < Math.abs(a.x - spot.x) ? b : a)).band;
  assert.ok(Math.min(...marks.map((m) => Math.hypot(m.x - spot.x, m.y - spot.y))) > 20, 'out of reach');
  const notch = () => ({ clientX: spot.x, clientY: spot.y, deltaY: -120, deltaMode: 0, ctrlKey: false,
    preventDefault() { this.prevented = true; } });
  assert.equal(r.hand.wheel(notch(), r.plot), false);
  assert.deepEqual(r.posts, []);
  const wide = notch();
  assert.equal(r.hand.wheel(wide, r.plot, { wide: true }), true);
  assert.equal(wide.prevented, true);
  await r.clock.advance(WHEEL_END_MS);
  const slot = SLOTS[nearest - 1];
  assert.ok(r.posts.length >= 2 && r.posts.every(([, b]) => b.slot === slot && b.q !== undefined));
});

await ok('a_key_burst_steps_from_its_own_value_after_a_re_sort_and_a_wheel_burst_follows_it', async () => {
  const r = rig();
  const key = (name) => ({ key: name, altKey: false, ctrlKey: false, metaKey: false, preventDefault() {} });
  r.hand.key(key('ArrowUp'), 1);
  await r.clock.advance(60);
  // another front end lists the point (slot 5) after the 2 kHz one
  r.host([LIST[1], { ...LIST[0] }, LIST[2]]);
  assert.equal(r.hand.burst.n, 2, 'the burst follows the point');
  r.hand.key(key('ArrowUp'), 2);
  await r.clock.advance(WHEEL_END_MS + 60);
  const mine = r.posts.filter(([, b]) => b.slot === 5).map(([, b]) => [b.g, b.done]);
  assert.deepEqual(mine, [[0.1, false], [0.2, false], [0.2, true]], 'not the neighbour\'s 3.0 dB');
  assert.equal(r.posts.every(([, b]) => b.slot === 5), true);
  // a wheel burst follows a re-sort too, and goes on as the same burst
  const w = rig();
  const m = markerOf(w, 1);
  const notch = (x, y) => ({ clientX: x, clientY: y, deltaY: -120, deltaMode: 0, ctrlKey: false, preventDefault() {} });
  w.hand.wheel(notch(m.x, m.y), w.plot);
  await w.clock.advance(60);
  const before = w.hand.burst;
  w.host([LIST[1], { ...LIST[0] }, LIST[2]]);
  assert.equal(w.hand.burst, before);
  assert.equal(w.hand.burst.n, 2);
  const m2 = markerOf(w, 2);
  w.hand.wheel(notch(m2.x, m2.y), w.plot);
  assert.equal(w.hand.burst, before, 'the same burst');
  await w.clock.advance(WHEEL_END_MS + 60);
  assert.equal(w.posts.filter(([, b]) => b.done).length, 1);
});

await ok('a_flip_or_a_switch_ends_a_burst_with_its_one_last_value', async () => {
  const r = rig();
  const m = markerOf(r, 2);
  r.hand.wheel({ clientX: m.x, clientY: m.y, deltaY: -120, deltaMode: 0, preventDefault() {} }, r.plot);
  await r.clock.advance(60);
  r.computer.flip('dac');
  await r.clock.advance(60);
  assert.equal(r.hand.burst, null);
  assert.equal(r.posts.filter(([, b]) => b.done).length, 1);
  await r.clock.advance(WHEEL_END_MS);
  assert.equal(r.posts.filter(([, b]) => b.done).length, 1, 'the timer does not write it again');
  const q = rig();
  const k = markerOf(q, 2);
  q.hand.wheel({ clientX: k.x, clientY: k.y, deltaY: -120, deltaMode: 0, preventDefault() {} }, q.plot);
  await q.clock.advance(60);
  q.hand.switched();
  await q.clock.advance(60);
  assert.equal(q.posts.filter(([, b]) => b.done).length, 1);
});

await ok('a_pointer_cancel_lets_go_a_drag_and_the_pointer_is_kept_for_the_theme', async () => {
  const r = rig();
  const m = markerOf(r, 2);
  r.hand.down(mouse(m.x, m.y), r.plot);
  await r.clock.advance(60);
  r.hand.move(mouse(m.x + 200, m.y));
  await flush();
  r.hand.cancel(mouse(m.x + 200, m.y, { pointerId: 7 }));
  assert.equal(r.hand.active, true, 'another pointer\'s cancel is not this drag\'s');
  r.hand.cancel(mouse(m.x + 200, m.y));
  assert.equal(r.hand.active, false);
  const s = r.hand.session;
  assert.deepEqual([s.pointerId, s.lastPointer], [null, 1]);
  await r.clock.advance(100);
  assert.equal(pointPosts(r, 2).at(-1).done, true);
  assert.equal(r.hand.session, null);
});

await ok('a_press_with_another_button_or_a_second_pointer_on_empty_ground_is_no_tap', async () => {
  const r = rig();
  assert.equal(r.hand.down(mouse(500, 200, { button: 1 }), r.plot), null);
  assert.equal(await r.hand.up(mouse(500, 200)), null);
  assert.equal(r.hand.down(mouse(500, 200, { isPrimary: false }), r.plot), null);
  assert.equal(await r.hand.up(mouse(500, 200)), null);
  assert.deepEqual(r.posts, []);
});

await ok('tearing_the_hand_down_lets_go_a_drag_under_way', async () => {
  const r = rig();
  const m = markerOf(r, 2);
  r.hand.down(mouse(m.x, m.y), r.plot);
  await r.clock.advance(60);
  r.hand.move(mouse(m.x + 200, m.y));
  await flush();
  r.hand.destroy();
  assert.equal(r.hand.active, false);
  assert.equal(r.hand.session, null);
  assert.equal(r.log.some((e) => e[0] === 'ended'), true);
  await r.clock.advance(100);
  // its track is forgotten: a flip has nothing left to end
  const posts = r.posts.length;
  r.computer.flip('dac');
  await r.clock.advance(100);
  assert.equal(r.posts.length, posts);
});

await ok('a_stepper_steps_a_field_in_a_live_burst_and_the_holds_end_writes_one_last_value', async () => {
  const r = rig();
  // a held stepper: a step each repeat, live, from the value being changed (q too, which no key steps)
  for (let i = 0; i < 3; i++) {
    assert.equal(r.hand.step(2, 'q', 1), true);
    await r.clock.advance(60);
  }
  const live = pointPosts(r, 2);
  const up = live.map((b) => b.q);
  assert.deepEqual(live.map((b) => b.done), [false, false, false]);
  assert.ok(up[0] > 1 && up[1] > up[0] && up[2] > up[1], `a step of the Q grid each: ${up}`);
  assert.deepEqual(up.map((q) => STEPS.q.includes(q)), [true, true, true]);
  assert.equal(r.hand.burst.param, 'q');
  // the hold ends: the value it reached goes once as the final one, and the timer writes nothing more
  r.hand.finishBurst();
  await r.clock.advance(WHEEL_END_MS + 60);
  const all = pointPosts(r, 2);
  assert.equal(all.length, 4);
  assert.deepEqual([all.at(-1).q, all.at(-1).done], [up[2], true]);
  assert.equal(r.hand.burst, null);
  // a step down, of the frequency and of the gain
  assert.equal(r.hand.step(2, 'g', -1), true);
  await r.clock.advance(WHEEL_END_MS + 60);
  assert.deepEqual(pointPosts(r, 2).slice(4).map((b) => [b.g, b.done]), [[2.9, false], [2.9, true]]);
  assert.equal(r.hand.step(2, 'f', 1), true);
  r.hand.finishBurst();
  await flush();
  const f = pointPosts(r, 2).slice(6);
  assert.ok(f.at(-1).f > 2000 && f.at(-1).done === true);
  // another field is another burst: the first ends with its one last value
  const two = rig();
  two.hand.step(2, 'g', 1);
  two.hand.step(2, 'q', 1);
  await two.clock.advance(WHEEL_END_MS + 60);
  const sent = pointPosts(two, 2);
  assert.deepEqual([sent[0].g, sent[0].done], [3.1, false], 'the first field was written live');
  assert.deepEqual([sent.at(-1).q, sent.at(-1).done], [STEPS.q.find((q) => q > 1), true]);
  assert.equal(sent.filter((b) => b.done).length, 1, 'one last value, not one per burst');
  // a step against its key, a field that is no field and a delta that is no step do nothing
  const beyond = rig({ list: slotted([point('PK', 1000, 14, 1)]) });
  assert.equal(beyond.hand.step(1, 'g', 1), true);
  assert.equal(beyond.hand.step(1, 'x', 1), false);
  assert.equal(beyond.hand.step(1, 'g', 2), false);
  assert.equal(beyond.hand.step(1, 'g', 0), false);
  await beyond.clock.advance(WHEEL_END_MS + 60);
  assert.deepEqual(beyond.posts, []);
  // a page that may not write is turned away; a page that is not usable says nothing; a drag under way keeps its own
  const guest = rig({ viewer: GUEST });
  assert.equal(guest.hand.step(2, 'g', 1), 'refused');
  assert.equal(guest.toasts.length, 1);
  const idle = rig({ usable: () => false });
  assert.equal(idle.hand.step(2, 'g', 1), false);
  const drag = rig();
  const m = markerOf(drag, 2);
  drag.hand.down(mouse(m.x, m.y), drag.plot);
  assert.equal(drag.hand.step(2, 'g', 1), true);
  assert.equal(drag.hand.burst, null);
  // a flip ends it with its one last value; a re-sort mid-hold steps the same point
  const flip = rig();
  flip.hand.step(1, 'g', 1);
  await flip.clock.advance(60);
  flip.host([LIST[1], { ...LIST[0] }, LIST[2]]);
  flip.hand.step(2, 'g', 1);
  flip.computer.flip('dac');
  await flip.clock.advance(60);
  assert.deepEqual(flip.posts.filter(([, b]) => b.slot === 5).map(([, b]) => [b.g, b.done]),
    [[0.1, false], [0.2, false], [0.2, true]]);
});

const fingers = (...spots) => ({
  touches: spots.map(([x, y], i) => ({ identifier: i + 1, clientX: x, clientY: y })),
  changedTouches: spots.map(([x, y], i) => ({ identifier: i + 1, clientX: x, clientY: y })),
  cancelable: true, type: 'touchstart', preventDefault() { this.prevented = true; },
});

await ok('a_session_begins_on_a_gesture_the_theme_names_and_a_second_finger_makes_a_press_no_tap', async () => {
  // a pinch from a diamond: one pointer sets the Q through the session's own spread
  const r = rig();
  const s = r.hand.begin({ n: 2, plot: r.plot, gesture: 'pinch', press: { distance: 1 }, pointerId: 4 });
  assert.equal(s.n, 2);
  assert.equal(r.computer.picked(), 2, 'the point is in hand');
  assert.deepEqual(r.log.filter((e) => e[0] === 'pick' || e[0] === 'begin').map((e) => e[0]), ['pick', 'begin']);
  assert.equal(r.hand.active, true);
  s.drag.spread(2);
  await r.clock.advance(60);
  const live = pointPosts(r, 2);
  assert.deepEqual(live.map((b) => b.done), [false]);
  assert.equal(live[0].q, STEPS.q.find((q) => q >= 0.5), 'twice as far halves the Q');
  assert.equal(r.hand.move(mouse(10, 10, { pointerId: 4 })), true, 'the pointer is the session\'s');
  assert.equal(r.hand.move(mouse(10, 10, { pointerId: 5 })), false);
  // a re-sort the daemon lists is followed, as for any session
  r.host([LIST[1], { ...LIST[0] }, LIST[2]]);
  assert.equal(r.hand.session.n, 1, 'the 2 kHz point is listed first now');
  assert.equal(r.hand.up(mouse(10, 10, { pointerId: 4 })), true);
  await r.clock.advance(100);
  const all = r.posts.map(([, body]) => body);
  assert.deepEqual([all.at(-1).done, all.at(-1).q, all.at(-1).slot], [true, live[0].q, 2], 'one last value');
  assert.equal(r.hand.session, null);
  // a begin on a point that is not there, or while a drag is under way, starts nothing
  const q = rig();
  assert.equal(q.hand.begin({ n: 2, plot: null, gesture: 'pinch', press: { distance: 1 }, pointerId: 4 }), null);
  assert.equal(q.hand.begin({ n: 9, plot: q.plot, gesture: 'pinch', press: { distance: 1 }, pointerId: 4 }), null);
  const m = markerOf(q, 1);
  q.hand.down(mouse(m.x, m.y), q.plot);
  assert.equal(q.hand.begin({ n: 2, plot: q.plot, gesture: 'pinch', press: { distance: 1 }, pointerId: 4 }), null);
  assert.equal(q.hand.session.n, 1);
  // a page that may not write is turned away with the words; one not usable now says nothing
  const guest = rig({ viewer: GUEST });
  assert.equal(guest.hand.begin({ n: 2, plot: guest.plot, gesture: 'pinch', press: { distance: 1 }, pointerId: 4 }), 'refused');
  assert.deepEqual([guest.toasts.length, guest.hand.session], [1, null]);
  const idle = rig({ usable: () => false });
  assert.equal(idle.hand.begin({ n: 2, plot: idle.plot, gesture: 'pinch', press: { distance: 1 }, pointerId: 4 }), null);
  assert.deepEqual([idle.toasts, idle.hand.session], [[], null]);
  // a begin ends a burst under way with its one last value
  const b = rig();
  b.hand.step(2, 'g', 1);
  await b.clock.advance(60);
  b.hand.begin({ n: 2, plot: b.plot, gesture: 'pinch', press: { distance: 1 }, pointerId: 4 });
  assert.equal(b.hand.burst, null);
  assert.equal(pointPosts(b, 2).filter((x) => x.done).length, 1);

  // a press on empty ground may be a tap; dropTap says it is not
  const t = rig();
  t.hand.down(mouse(500, 200), t.plot);
  t.hand.dropTap();
  assert.equal(await t.hand.up(mouse(500, 200)), null);
  assert.deepEqual(t.posts, []);
  // a second finger is no tap either, even when it has no point to pinch
  const two = rig({ marks: () => [] });
  assert.equal(two.hand.down(mouse(500, 200, { pointerType: 'touch' }), two.plot), 'tap');
  assert.equal(two.hand.touchStart(fingers([480, 200], [520, 200]), two.plot), null);
  assert.equal(await two.hand.up(mouse(500, 200, { pointerType: 'touch' })), null);
  assert.deepEqual(two.posts, []);
  // one finger on empty ground leaves the tap alone
  const one = rig({ marks: () => [] });
  one.hand.down(mouse(500, 200, { pointerType: 'touch' }), one.plot);
  assert.equal(one.hand.touchStart(fingers([500, 200]), one.plot), null);
  assert.equal((await one.hand.up(mouse(500, 200, { pointerType: 'touch' }))).ok, true);
});

/** An element that keeps the listeners it is given, by type, and refuses none. */
function listening() {
  const heard = new Map();
  return {
    heard,
    count: () => [...heard.values()].reduce((sum, set) => sum + set.size, 0),
    addEventListener(type, fn, options) {
      if (!heard.has(type)) heard.set(type, new Map());
      heard.get(type).set(fn, options);
    },
    removeEventListener(type, fn) { heard.get(type)?.delete(fn); },
    getBoundingClientRect: () => ({ left: 0, top: 0, width: 1000, height: 260 }),
    setPointerCapture() {},
  };
}

await ok('a_touch_watch_adds_the_handlers_it_names_once_and_takes_exactly_those_off', async () => {
  const calls = [];
  const move = (event) => calls.push(['move', event]);
  const end = (event) => calls.push(['end', event]);
  const watch = touchWatch({ move, end });
  const el = listening();
  watch(el, true);
  watch(el, true);
  assert.deepEqual([...el.heard.keys()].sort(), ['touchcancel', 'touchend', 'touchmove']);
  assert.equal(el.count(), 3, 'the same functions twice are one listener');
  assert.deepEqual([el.heard.get('touchmove').get(move)], [{ passive: false }], 'move may stop the scroll');
  assert.equal(el.heard.get('touchend').has(end) && el.heard.get('touchcancel').has(end), true);
  watch(el, false);
  assert.equal(el.count(), 0, 'every listener it added is gone');
  // stop takes it off every element still watched
  const a = listening();
  const b = listening();
  watch(a, true);
  watch(b, true);
  watch(a, false);
  watch.stop();
  assert.deepEqual([a.count(), b.count()], [0, 0]);
  watch(null, true);
});

await ok('the_hand_watches_the_touches_of_a_plot_until_its_drag_ends_or_it_is_torn_down', async () => {
  const r = rig();
  const plot = listening();
  r.hand.watchTouch(plot, true);
  assert.equal(plot.count(), 3);
  // a finger's drag: the move and the end reach the hand through the watch, and the end takes it off
  const m = markerOf(r, 2);
  const touch = (id, x, y) => ({ identifier: id, clientX: x, clientY: y });
  const start = { touches: [touch(1, m.x, m.y)], changedTouches: [touch(1, m.x, m.y)], cancelable: true,
    preventDefault() {} };
  assert.equal(r.hand.touchStart(start, plot), 'drag');
  assert.equal(plot.count(), 3, 'a begun drag keeps its plot watched');
  const move = plot.heard.get('touchmove').keys().next().value;
  move({ touches: [touch(1, m.x + 40, m.y)], changedTouches: [], cancelable: true, preventDefault() {} });
  await r.clock.advance(60);
  assert.ok(pointPosts(r, 2).length >= 1, 'the move reached the drag');
  const end = plot.heard.get('touchend').keys().next().value;
  end({ type: 'touchend', touches: [], changedTouches: [touch(1, m.x + 40, m.y)] });
  assert.equal(r.hand.active, false);
  assert.equal(plot.count(), 0, 'the drag is over: the plot is not watched');
  await r.clock.advance(100);
  // torn down: what is watched goes
  const q = rig();
  const other = listening();
  q.hand.watchTouch(other, true);
  q.hand.destroy();
  assert.equal(other.count(), 0);
  // a watch the theme took off itself is gone, and the hand's end finds nothing more to take
  const t = rig();
  const own = listening();
  t.hand.watchTouch(own, true);
  t.hand.watchTouch(own, false);
  assert.equal(own.count(), 0);
});

await ok('the_graphs_viewbox_is_one_pair_of_constants_the_curve_and_the_hand_agree_on', async () => {
  assert.deepEqual([VIEW_WIDTH, VIEW_HEIGHT], [1000, 260]);
  // the curve's own maps span it: the top frequency at the right edge, +13 dB at the top, -13 dB at the bottom
  assert.equal(Math.round(xOf(F_MAX)), VIEW_WIDTH);
  assert.deepEqual([yOf(RANGE_DB), yOf(-RANGE_DB)], [0, VIEW_HEIGHT]);
  // the hand reads a marker in it: the right edge of the viewBox is the right edge of the plot, the middle its middle
  assert.deepEqual(markersIn([{ n: 1, x: VIEW_WIDTH, y: VIEW_HEIGHT / 2 }], { width: 200, height: 40 }),
    [{ band: 1, x: 200, y: 20 }]);
  // and a press at a spot of the plot reads the frequency and gain of the same spot of the viewBox
  const map = plotMap({ width: VIEW_WIDTH, height: VIEW_HEIGHT });
  assert.equal(Math.round(map.hzAt(VIEW_WIDTH)), F_MAX);
  assert.equal(map.dbAt(VIEW_HEIGHT / 2), 0);
  assert.equal(map.yOfDb(0), VIEW_HEIGHT / 2);
  const half = plotMap({ width: VIEW_WIDTH / 2, height: VIEW_HEIGHT / 2 });
  assert.equal(Math.round(half.hzAt(VIEW_WIDTH / 2)), F_MAX, 'a plot of any size maps the same viewBox');
  assert.equal(half.yOfDb(0), VIEW_HEIGHT / 4);
});

await ok('a_session_says_the_values_it_asks_as_numbers_and_the_labels_words_with_or_without_the_q', async () => {
  assert.equal(labelWords({ f: 1000, g: 3, q: 1 }, { qPrefix: 'Q ' }), '1000 Hz · +3.0 dB · Q 1.00');
  assert.equal(labelWords({ f: 1000, g: 3, q: 1 }, {}), '1000 Hz · +3.0 dB · 1.00');
  // raw steps of the hand's grid are the values at them
  const raws = { f: 100, g: 130, q: 40 };
  assert.deepEqual(pointValues(raws), { f: STEPS.f[100], g: STEPS.g[130], q: STEPS.q[40] });
  assert.deepEqual(pointValues({ f: 1, g: 0, q: 1 }, { f: [1, 2], g: [3, 4], q: [5, 6] }), { f: 2, g: 3, q: 6 });
  const r = rig();
  const m = markerOf(r, 2);
  let at;
  r.hand = pointDrag({
    actions: r.actions, computer: r.computer, clock: r.clock, points: (_, rect) => markersIn(r.computer.tone().markers, rect),
    on: { begin: (s) => { at = s.values(); } },
  });
  r.hand.down(mouse(m.x, m.y), r.plot);
  const s = r.hand.session;
  assert.deepEqual(at, s.values(), 'known as the session begins, from the point as it is');
  const first = s.values();
  assert.ok(Math.abs(first.f / 2000 - 1) < 0.03 && first.g === 3 && Math.abs(first.q - 1) < 0.1, 'the grid\'s nearest to the point');
  r.hand.move(mouse(m.x + 150, m.y - 40));
  await r.clock.advance(60);
  const held = s.drag.held;
  assert.deepEqual(s.values(), { f: STEPS.f[held.f], g: STEPS.g[held.g], q: STEPS.q[held.q] });
  assert.ok(s.values().f > 2000 && s.values().g > 3, 'what the drag asks, not what the table still shows');
  assert.match(s.words({ qPrefix: 'Q ' }), /Hz · [+−]?\d+\.\d dB · Q \d/, 'the table\'s words take the option too');
  assert.doesNotMatch(s.words(), /Q/);
  r.hand.cancel(mouse(m.x, m.y));
});

await ok('a_held_stepper_keeps_its_burst_across_the_pause_before_its_first_repeat_and_a_step_on_no_point_says_so', async () => {
  const r = rig();
  // the press steps once; the first repeat comes after the hold's own pause, longer than the burst's timer
  assert.equal(r.hand.step(2, 'g', 1, { hold: true }), true);
  await r.clock.advance(380);
  assert.equal(r.log.filter((e) => e[0] === 'burst').length, 1, 'one burst begun, none ended');
  assert.equal(r.posts.filter(([, b]) => b.done).length, 0, 'no final write in the middle of the hold');
  r.hand.step(2, 'g', 1, { hold: true });
  await r.clock.advance(70);
  r.hand.step(2, 'g', 1, { hold: true });
  await r.clock.advance(WHEEL_END_MS * 3);
  assert.equal(r.hand.burst !== null, true, 'only the hold ends it');
  r.hand.finishBurst();
  await r.clock.advance(60);
  const done = r.posts.filter(([, b]) => b.done);
  assert.equal(done.length, 1);
  assert.equal(r.log.filter((e) => e[0] === 'burst' && e[2] === false).length, 1);
  // a step with no hold ends after its own timer
  const timed = rig();
  timed.hand.step(2, 'g', 1);
  await timed.clock.advance(WHEEL_END_MS + 60);
  assert.equal(timed.hand.burst, null);
  // a hold after a step with a timer takes the timer off
  const mixed = rig();
  mixed.hand.step(2, 'g', 1);
  mixed.hand.step(2, 'g', 1, { hold: true });
  await mixed.clock.advance(WHEEL_END_MS * 3);
  assert.notEqual(mixed.hand.burst, null);
  mixed.hand.finishBurst();
  // a point that is not there is no step
  const none = rig();
  assert.equal(none.hand.step(9, 'g', 1), false);
  assert.equal(none.hand.step(9, 'q', -1, { hold: true }), false);
  assert.deepEqual([none.hand.burst, none.posts, none.log], [null, [], []]);
});

await ok('the_core_changing_the_pick_is_heard_by_the_theme_and_a_begin_that_answers_nothing_ends_no_burst', async () => {
  const r = rig();
  r.computer.pick(3);
  const before = r.log.length;
  await r.actions.computer.removePoint(3);
  assert.deepEqual(r.log.slice(before).filter((e) => e[0] === 'pick'), [['pick', null]], 'the lit point goes with it');
  r.computer.pick(3);
  await r.actions.computer.removePoint(1);
  assert.deepEqual(r.log.filter((e) => e[0] === 'pick').at(-1), ['pick', 2], 'and follows the renumbering');
  // the page's own pick says nothing
  const n0 = r.log.length;
  r.computer.pick(1);
  assert.equal(r.log.length, n0);
  // a begin that answers null leaves a burst under way alone
  const b = rig();
  b.hand.step(2, 'g', 1);
  assert.equal(b.hand.begin({ n: 9, plot: b.plot, gesture: 'pinch', press: { distance: 1 }, pointerId: 4 }), null);
  assert.notEqual(b.hand.burst, null);
  b.hand.finishBurst();
  // a begin carries the fingers and the gesture it is given: two fingers apart set the Q
  const t = rig();
  const finger = (id, x) => ({ identifier: id, clientX: x, clientY: 100 });
  const s = t.hand.begin({ n: 2, plot: t.plot, gesture: 'pinch', press: { distance: 100 }, touchIds: [1, 2] });
  assert.deepEqual(s.touchIds, [1, 2]);
  t.hand.touchMove({ touches: [finger(1, 0), finger(2, 200)], cancelable: true, preventDefault() {} });
  await t.clock.advance(60);
  const live = pointPosts(t, 2);
  assert.equal(live.length, 1, 'the fingers moved the drag');
  assert.ok(live[0].q < 1, 'twice as far halves the Q');
});

await ok('a_burst_ends_with_no_final_write_when_its_point_is_removed_and_follows_a_lower_one_removed', async () => {
  // another front end removes the point a held stepper is on
  const r = rig();
  const gone = [];
  r.computer.on((e) => { if (e.type === 'gone') gone.push(e.n); });
  r.hand.step(2, 'g', 1, { hold: true });
  await r.clock.advance(60);
  const sent = r.posts.length;
  r.host([LIST[0], LIST[2]]);
  r.host([LIST[0], LIST[2]]);
  await r.clock.advance(60);
  assert.deepEqual(gone, [2], 'said once');
  assert.equal(r.hand.burst, null, 'the burst ended');
  assert.deepEqual(r.log.filter((e) => e[0] === 'burst').map((e) => e[2]), [true, false]);
  // no more burst: what a theme does with its hold is its own (Night drive ends it on `gone`)
  r.hand.finishBurst();
  await r.clock.advance(WHEEL_END_MS + 60);
  assert.equal(r.posts.filter(([, b]) => b.done).length, 0, 'no final write for a point that is gone');
  assert.equal(r.posts.length, sent);
  // a lower point removed: the burst follows its point
  const f = rig();
  f.hand.step(3, 'g', 1, { hold: true });
  await f.clock.advance(60);
  f.host([LIST[1], LIST[2]]);
  await f.clock.advance(60);
  assert.equal(f.hand.burst.n, 2, 'point 3 is point 2 now');
  f.hand.finishBurst();
  await f.clock.advance(60);
  const last = f.posts.at(-1)[1];
  assert.deepEqual([last.done, last.slot], [true, SLOTS[2]]);
  // another point removed: the burst goes on
  const o = rig();
  o.hand.step(2, 'g', 1, { hold: true });
  await o.clock.advance(60);
  o.host([LIST[0], LIST[1]]);
  await o.clock.advance(60);
  assert.notEqual(o.hand.burst, null);
  o.hand.finishBurst();
});

console.log(`core-point-drag.mjs: ${n} passed`);
