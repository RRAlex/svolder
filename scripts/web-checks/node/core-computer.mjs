/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the core's computer side (core/computer.js, actions.computer, the shelves of
// core/tone-curve.js, computerSteps and pointWriter of core/tone-drag.js, computerTargetsOf of
// core/contract.js): the side kept per viewer, no switch without a shown device and the
// computer's controls, a flip that writes nothing, the curve with its shelves, the steps of the
// hand ranges, the point a tap or `+ Add point` makes, the line of points and preamp, a drag sent
// at most 20 a second with its last value at release, a value this viewer is dragging that a host
// update does not move, a tap on a point beyond the hand range that writes nothing, a guest who
// may flip and write nothing, every route and body, the both-sides question, the volume's
// first paint and its cap. The post, the read, the clock and the question are fakes.
// Run: node core-computer.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createActions } from '../../../bins/svolderd/web/core/actions.js';
import {
  addedAt, createComputer, createHolds, FLAT_POINT, HAND, idAt, indexOfId, POINT_TYPES,
} from '../../../bins/svolderd/web/core/computer.js';
import { createAsk } from '../../../bins/svolderd/web/core/ask.js';
import { computerTargetsOf } from '../../../bins/svolderd/web/core/contract.js';
import { SIDE_KEY, readSide, writeSide } from '../../../bins/svolderd/web/core/prefs.js';
import { createStore } from '../../../bins/svolderd/web/core/store.js';
import { responseDb, yOf } from '../../../bins/svolderd/web/core/tone-curve.js';
import {
  computerSteps, createDrag, pointWriter, startOf,
} from '../../../bins/svolderd/web/core/tone-drag.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
const flush = async () => { for (let i = 0; i < 12; i++) await new Promise((r) => setImmediate(r)); };

const fixture = JSON.parse(readFileSync(new URL('./fixtures/computer-response.json', import.meta.url), 'utf8'));

const SIGNED = { signed_in: true, guest: false, writes: true, read_only: null };
const GUEST = { signed_in: false, guest: true, writes: false, read_only: null };

const point = (type, f, g, q) => ({ type, f, g, q });
const modelOf = (over = {}) => ({
  volume: { pct: 42, muted: false, sink: 'Schiit' },
  eq: {
    on: true, running_unknown: false, preamp_db: -6, preset: 'Warm', live: true, note: null, max: 16,
    points: [point('PK', 1000, 3, 1), point('LSC', 100, -2, 0.7)], response: [],
  },
  presets: [
    { name: 'Warm', built_in: true, on: true, playable: true, why: null },
    { name: 'Huge', built_in: false, on: false, playable: false, why: '17 filters' },
  ],
  unavailable: null,
  ...over,
});
const dacDevice = { name: 'Gungnir 2', url: 'Gungnir%202', volume: null, tone: null };
const pageOf = (computer, { viewer = SIGNED, device = dacDevice } = {}) => ({
  v: 1, viewer, fleet: { devices: [], hops: [] }, shown: device?.url ?? null, device, followed: {},
  card: null, host: computer === undefined ? null : { track: null, route: null, computer },
  strip: null,
});

const memory = () => {
  const map = new Map();
  return { map, getItem: (k) => map.get(k) ?? null, setItem: (k, v) => map.set(k, String(v)),
    removeItem: (k) => map.delete(k) };
};
const refusing = () => ({
  getItem() { throw new Error('no'); }, setItem() { throw new Error('no'); },
  removeItem() { throw new Error('no'); },
});

/** A fake clock the test advances: timers due run in order. */
function fakeClock() {
  const clock = { t: 0, timers: [], id: 0 };
  clock.now = () => clock.t;
  clock.later = (fn, ms) => { const id = ++clock.id; clock.timers.push({ id, at: clock.t + ms, fn }); return id; };
  clock.cancel = (id) => { clock.timers = clock.timers.filter((x) => x.id !== id); };
  clock.advance = (to) => {
    clock.t = to;
    for (;;) {
      const due = clock.timers.filter((x) => x.at <= to).sort((a, b) => a.at - b.at)[0];
      if (!due) return;
      clock.timers = clock.timers.filter((x) => x !== due);
      due.fn();
    }
  };
  return clock;
}

/** The page's pieces over `page`, with fakes: `reply(path, body)` answers a post (default 200),
 * `host` is what a read of the host gives. */
function rig({ computer = modelOf(), viewer = SIGNED, device, storage = memory(), reply, asked } = {}) {
  const posts = [];
  const gets = [];
  const toasts = [];
  const state = { host: { computer } };
  const store = createStore(pageOf(computer, { viewer, ...(device !== undefined ? { device } : {}) }), {
    signedOut: () => {},
    fetch: async (path) => { gets.push(path); return { status: 200, json: async () => ({ device: dacDevice }) }; },
  });
  const holds = createHolds({
    store,
    fetch: async (path) => { gets.push(path); return { status: 200, json: async () => state.host }; },
  });
  const clock = fakeClock();
  const computerCore = createComputer({ store, storage, holds });
  const actions = createActions({
    store,
    post: async (path, body) => { posts.push([path, body]); return reply ? reply(path, body) : { status: 200, ok: true }; },
    toast: (text) => toasts.push(text),
    history: { pushState: () => {} },
    location: { assign: () => {} },
    holds,
    ask: async (question) => { asked?.questions.push(question); return asked?.answer ?? false; },
    timing: clock,
    computer: computerCore,
  });
  return { store, holds, actions, computer: computerCore, posts, gets, toasts, storage, clock, state };
}

await ok('the_side_is_kept_per_viewer_and_defaults_to_the_dac', async () => {
  const storage = memory();
  assert.equal(readSide(storage), 'dac');
  const first = rig({ storage });
  assert.equal(first.computer.side(), 'dac');
  first.computer.flip('computer');
  assert.equal(storage.map.get(SIDE_KEY), 'computer');
  assert.equal(first.computer.side(), 'computer');
  const heard = [];
  first.computer.on((event) => heard.push(event));
  first.computer.flip('dac');
  assert.deepEqual(heard, [{ type: 'flip', side: 'dac' }]);
  assert.equal(storage.map.get(SIDE_KEY), 'dac');
  first.computer.flip('computer');
  // another load of the page in this browser keeps it; another browser's storage does not
  assert.equal(rig({ storage }).computer.side(), 'computer');
  assert.equal(rig({ storage: memory() }).computer.side(), 'dac');
  // anything else in storage reads as the DAC; a refusing storage works in memory
  storage.map.set(SIDE_KEY, 'both');
  assert.equal(readSide(storage), 'dac');
  assert.equal(writeSide('computer', refusing()), false);
  const shut = rig({ storage: refusing() });
  shut.computer.flip('computer');
  assert.equal(shut.computer.side(), 'computer');
  shut.computer.flip('elsewhere');
  assert.equal(shut.computer.side(), 'computer', 'a word that is no side is ignored');
});

await ok('no_switch_without_a_dac_or_without_the_computer', async () => {
  const none = rig({ computer: null });
  assert.equal(none.computer.drawable(none.store.page()), false);
  assert.equal(none.computer.switchOf(none.store.page()), null);
  assert.equal(none.computer.volumeOf(none.store.page()), null);
  assert.equal(none.computer.toneOf(none.store.page()), null);
  const noHost = rig();
  noHost.store.replace({ ...noHost.store.page(), host: null });
  assert.equal(noHost.computer.switchOf(noHost.store.page()), null);
  const noDevice = rig({ device: null });
  assert.equal(noDevice.computer.switchOf(noDevice.store.page()), null, 'no device shown');
  const dac = rig();
  assert.deepEqual(dac.computer.switchOf(dac.store.page()),
    { dac: 'Gungnir 2', computer: 'This computer', side: 'dac', unavailable: null });
  // an amp page has the switch too, with the same volume and tone parts
  const amp = rig({ device: { name: 'Amp', url: 'Amp', volume: null, tone: null } });
  assert.equal(amp.computer.switchOf(amp.store.page()).dac, 'Amp');
  assert.ok(amp.computer.toneOf(amp.store.page()).points.length === 2);
  assert.equal(amp.computer.volumeOf(amp.store.page()).pct, 42);
  // unavailable: the switch says why and the DAC side is shown, whatever was kept
  const off = rig({ computer: modelOf({ unavailable: 'computer controls unavailable: no pw-cli' }) });
  off.computer.flip('computer');
  assert.equal(off.computer.side(), 'dac');
  assert.equal(off.computer.kept(), 'computer');
  assert.deepEqual(off.computer.switchOf(off.store.page()), { dac: 'Gungnir 2',
    computer: 'This computer', side: 'dac', unavailable: 'computer controls unavailable: no pw-cli' });
  assert.deepEqual(computerTargetsOf(off.store.page()), [{ computer: 'flip' }]);
});

await ok('flipping_writes_nothing', async () => {
  const storage = memory();
  storage.map.set(SIDE_KEY, 'computer');
  const loaded = rig({ storage });
  assert.equal(loaded.computer.side(), 'computer');
  loaded.computer.flip('dac');
  loaded.computer.flip('computer');
  loaded.computer.flip('dac');
  await flush();
  assert.deepEqual(loaded.posts, [], 'no write after a load with the side kept and three flips');
  assert.deepEqual(loaded.gets, [], 'and no read either');
  // a flip ends what a theme tracked
  const ended = [];
  const off = loaded.computer.track({ cancel: (why) => ended.push(['drag', why]) });
  loaded.computer.track({ end: () => ended.push(['hold']) });
  const forgotten = loaded.computer.track({ cancel: () => ended.push(['forgotten']) });
  forgotten();
  loaded.computer.flip('computer');
  assert.deepEqual(ended, [['drag', 'flip'], ['hold']]);
  off();
  loaded.computer.flip('dac');
  assert.equal(ended.length, 2, 'what ended is not ended again');
  assert.deepEqual(loaded.posts, []);
});

await ok('the_curve_reads_shelves', async () => {
  const bands = fixture.points.map((p) => ({ f0: p.f, gainDb: p.g, q: p.q, type: p.type }));
  const got = responseDb(bands, fixture.freqs);
  assert.equal(got.length, fixture.response.length);
  got.forEach((db, i) => assert.ok(Math.abs(db - fixture.response[i]) <= 0.01,
    `${fixture.freqs[i]} Hz: ${db} against ${fixture.response[i]}`));
  // a band with no type is a peak, as before; a shelf is not a peak
  const peak = responseDb([{ f0: 1000, gainDb: 6, q: 1.4 }], fixture.freqs);
  const asPk = responseDb([{ f0: 1000, gainDb: 6, q: 1.4, type: 'PK' }], fixture.freqs);
  assert.deepEqual(peak, asPk);
  const low = responseDb([{ f0: 100, gainDb: 6, q: 0.7, type: 'LSC' }], [20, 20000]);
  assert.ok(low[0] > 5.5 && Math.abs(low[1]) < 0.5, low);
  const high = responseDb([{ f0: 8000, gainDb: 6, q: 0.7, type: 'HSC' }], [20, 20000]);
  assert.ok(high[1] > 5.5 && Math.abs(high[0]) < 0.5, high);
  // the page's own curve is that of the points it holds, shelves included
  const { computer, store } = rig({ computer: modelOf({ eq: { ...modelOf().eq,
    points: fixture.points } }) });
  const tone = computer.toneOf(store.page());
  assert.equal(tone.response.length, 181);
  tone.response.forEach((db, i) => assert.ok(Math.abs(db - fixture.response[i]) <= 0.01, `i ${i}`));
});

await ok('the_steps_span_the_hand_ranges', async () => {
  const { f, g, q } = computerSteps();
  assert.equal(f[0], 20);
  assert.equal(f.at(-1), 20000);
  for (let i = 1; i < f.length - 1; i++) {
    assert.ok(Math.abs(f[i] / f[i - 1] - 2 ** (1 / 24)) < 1e-3, `f step ${i}`);
  }
  assert.ok(f.at(-1) / f.at(-2) <= 2 ** (1 / 24) + 1e-3);
  assert.ok(f.includes(1000) || f.some((v) => Math.abs(v / 1000 - 1) < 0.03));
  assert.equal(g[0], -12);
  assert.equal(g.at(-1), 12);
  g.slice(1).forEach((v, i) => assert.equal(Math.round((v - g[i]) * 10), 1));
  assert.ok(g.includes(0) && !g.some(Object.is.bind(null, -0)));
  assert.equal(q[0], 0.1);
  assert.equal(q.at(-1), 10);
  assert.ok(q.includes(1));
  for (let i = 1; i < q.length; i++) {
    assert.ok(Math.abs(q[i] / q[i - 1] - 10 ** (1 / 24)) < 1e-2, `q step ${i}`);
  }
  assert.equal(computerSteps(), computerSteps());
  // a preset's value outside the ranges sits at the nearest step
  assert.deepEqual(startOf(computerSteps(), point('PK', 25000, 15, 20)),
    { f: f.length - 1, g: g.length - 1, q: q.length - 1 });
});

await ok('a_tap_adds_a_pk_at_the_spot_clamped', async () => {
  const { computer } = rig();
  assert.deepEqual(computer.addAt(1234.567, -3.04), { type: 'PK', f: 1234.57, g: -3, q: 1 });
  assert.deepEqual(computer.addAt(5, 30), { type: 'PK', f: 20, g: 12, q: 1 });
  assert.deepEqual(computer.addAt(90000, -30), { type: 'PK', f: 20000, g: -12, q: 1 });
  // a full table adds nothing
  const sixteen = Array.from({ length: 16 }, (_, i) => point('PK', 100 + i * 10, 0, 1));
  const full = rig({ computer: modelOf({ eq: { ...modelOf().eq, points: sixteen } }) });
  assert.equal(full.computer.addAt(500, 1), null);
  assert.equal(full.computer.toneOf(full.store.page()).canAdd, false);
  assert.equal(full.computer.toneOf(full.store.page()).max, 16);
  assert.equal(computerTargetsOf(full.store.page()).some((t) => t.computer === 'add'), false);
});

await ok('add_point_goes_to_the_widest_gap', async () => {
  const { computer } = rig();
  assert.equal(computer.nextFree([]), 632.46);
  assert.equal(computer.nextFree([{ f: 1000 }]), 141.42);
  assert.equal(computer.nextFree([{ f: 100 }, { f: 400 }]), 2828.43);
  // points beyond the range count at its edges; an unsorted list is read sorted
  assert.equal(computer.nextFree([{ f: 30000 }, { f: 5 }]), 632.46);
  assert.equal(computer.nextFree([{ f: 5000 }, { f: 50 }]), 500);
});

await ok('the_lone_point_keeps_its_remove', async () => {
  const lone = rig({ computer: modelOf({ eq: { ...modelOf().eq, points: [point('PK', 1000, 0, 1)] } }) });
  const one = lone.computer.toneOf(lone.store.page());
  assert.deepEqual(one.points.map((p) => p.n), [1]);
  assert.deepEqual(computerTargetsOf(lone.store.page()).filter((t) => t.computer === 'remove'),
    [{ computer: 'remove', n: 1 }]);
  // a flat curve the daemon sends with no point is still point 1
  const none = rig({ computer: modelOf({ eq: { ...modelOf().eq, points: [] } }) });
  const flat = none.computer.toneOf(none.store.page());
  assert.deepEqual(flat.points, [{ n: 1, type: 'PK', f: 1000, g: 0, q: 1 }]);
  const two = rig();
  assert.deepEqual(computerTargetsOf(two.store.page()).filter((t) => t.computer === 'remove'),
    [{ computer: 'remove', n: 1 }, { computer: 'remove', n: 2 }]);
});

await ok('the_preamp_line_reads_points_and_preamp', async () => {
  const seven = Array.from({ length: 7 }, (_, i) => point('PK', 100 * (i + 1), 0, 1));
  const { computer, store } = rig({ computer: modelOf({ eq: { ...modelOf().eq, points: seven,
    preamp_db: -6 } }) });
  assert.equal(computer.toneOf(store.page()).line, '7 of 16 points · preamp −6.0 dB');
  const flat = rig({ computer: modelOf({ eq: { ...modelOf().eq, preamp_db: 0, max: 16 } }) });
  assert.equal(flat.computer.toneOf(flat.store.page()).line, '2 of 16 points · preamp 0.0 dB');
  const small = rig({ computer: modelOf({ eq: { ...modelOf().eq, preamp_db: -0.04 } }) });
  assert.equal(small.computer.toneOf(small.store.page()).line, '2 of 16 points · preamp 0.0 dB');
  const deep = rig({ computer: modelOf({ eq: { ...modelOf().eq, preamp_db: -11.25 } }) });
  assert.match(deep.computer.toneOf(deep.store.page()).line, /preamp −11\.[23] dB$/);
});

await ok('the_tone_reads_mode_markers_and_what_is_not_verified', async () => {
  const { computer, store } = rig({ computer: modelOf({ eq: { ...modelOf().eq, on: false,
    running_unknown: true, points: [point('PK', 30000, 20, 1), point('PK', 1000, -4, 1)] } }) });
  const tone = computer.toneOf(store.page());
  assert.deepEqual(tone.modes, [{ name: 'off', label: 'Off', on: true },
    { name: 'eq', label: 'EQ', on: false }]);
  assert.equal(tone.verified, false, 'running_unknown is never drawn as verified');
  assert.equal(tone.markers[0].x, 1000, 'a point beyond the range sits at the edge');
  assert.ok(Number.isFinite(tone.markers[0].y));
  assert.equal(tone.points[0].f, 30000, 'and plays as written');
  assert.deepEqual(tone.presets.map((p) => [p.name, p.playable]), [['Warm', true], ['Huge', false]]);
  assert.equal(tone.presets[1].why, '17 filters');
});

await ok('every_marker_sits_on_the_summed_curve_a_shelfs_too', async () => {
  const { computer, store } = rig({ computer: modelOf({ eq: { ...modelOf().eq, on: true,
    points: [point('LSC', 105, 5.5, 0.7), point('PK', 1000, 3, 1), point('HSC', 8000, -4, 0.7)] } }) });
  const tone = computer.toneOf(store.page());
  const bands = tone.points.map((p) => ({ f0: p.f, gainDb: p.g, q: p.q, type: p.type }));
  for (const [i, m] of tone.markers.entries()) {
    const [db] = responseDb(bands, [tone.points[i].f]);
    assert.ok(Math.abs(m.y - yOf(db)) < 1, `marker ${i + 1}`);
  }
  assert.ok(Math.abs(tone.markers[0].y - yOf(5.5)) > 3, 'the shelf is not at its written gain');
});

await ok('a_drag_posts_at_most_twenty_a_second_and_the_release_always', async () => {
  const { actions, posts, clock, computer, store } = rig();
  const steps = computerSteps();
  const start = startOf(steps, point('PK', 1000, 3, 1));
  const write = pointWriter({ actions, n: 1 });
  const drag = createDrag({
    band: 1, gesture: 'move', start, press: { x: 500, y: 100 }, plot: { width: 1000, height: 260 },
    steps, write, on: { held() {}, answer() {}, toast() {}, ended() {} },
  });
  let last;
  for (let t = 0; t <= 1000; t += 2) {
    clock.advance(t);
    drag.move({ x: 500 + t / 2, y: 100 });
    await flush();
    if (t === 400) {
      const shown = computer.toneOf(store.page()).points[0].f;
      assert.ok(shown > 1000, 'the page draws the dragged frequency while it is sent');
    }
    last = steps.f[drag.held.f];
  }
  const inSecond = posts.length;
  assert.ok(inSecond <= 21, `${inSecond} live posts in a second`);
  assert.ok(inSecond >= 15, `${inSecond} live posts in a second is a drag that is heard`);
  for (let t = 1100; t <= 1500; t += 100) {
    clock.advance(t);
    await flush();
  }
  const live = posts.length;
  assert.ok(posts.every(([path, body]) => path === 'api/v1/host/eq/points/1' && body.done === false));
  drag.release();
  await drag.settled;
  await write.release();
  assert.equal(posts.length, live + 1, 'the release sends once more');
  assert.deepEqual(posts.at(-1), ['api/v1/host/eq/points/1', { f: last, done: true }]);
  const live1 = new Set(posts.slice(0, -1).map(([, body]) => body.f));
  assert.ok(live1.size > 5, 'values moved');
  // never two requests in flight for one point
  let inflight = 0;
  let worst = 0;
  const slow = rig({ reply: async () => { inflight++; worst = Math.max(worst, inflight);
    await new Promise((r) => setImmediate(r)); inflight--; return { status: 200 }; } });
  const sent = [];
  for (let t = 0; t < 400; t += 5) {
    slow.clock.advance(t);
    sent.push(slow.actions.computer.setPoint(1, { g: (t % 24) / 10 - 1 }, { done: false }));
    await new Promise((r) => setImmediate(r));
  }
  slow.clock.advance(1000);
  await slow.actions.computer.setPoint(1, {}, { done: true });
  assert.equal(worst, 1);
  assert.equal(slow.posts.at(-1)[1].done, true);
});

await ok('a_host_update_mid_drag_keeps_the_dragged_value', async () => {
  const r = rig();
  let release;
  r.actions = createActions({
    store: r.store, holds: r.holds, toast: () => {}, history: { pushState: () => {} },
    location: { assign: () => {} }, timing: r.clock,
    post: async (path, body) => { r.posts.push([path, body]); await new Promise((res) => { release = res; }); return { status: 200 }; },
  });
  const first = r.actions.computer.setPoint(1, { f: 2500, g: 7 }, { done: false });
  await flush();
  assert.equal(r.computer.toneOf(r.store.page()).points[0].f, 2500);
  // the host says another thing about point 1 while the drag goes on: the dragged fields stay,
  // the others follow
  r.store.emit('host', { ...r.store.page().host, computer: modelOf({ eq: { ...modelOf().eq,
    points: [point('PK', 1100, 4, 2.5), point('LSC', 90, -1, 0.7)] } }) });
  const tone = r.computer.toneOf(r.store.page());
  assert.equal(tone.points[0].f, 2500, 'the dragged frequency');
  assert.equal(tone.points[0].g, 7, 'the dragged gain');
  assert.equal(tone.points[0].q, 2.5, 'what the hand has not moved follows the host');
  assert.equal(tone.points[1].f, 90, 'another point follows the host');
  release();
  await first;
  const done = r.actions.computer.setPoint(1, {}, { done: true });
  await flush();
  release();
  await done;
  await flush();
  assert.deepEqual(r.posts.at(-1)[1], { f: 2500, g: 7, done: true }, 'the release sends the drag\'s values, not the model\'s');
  assert.equal(r.holds.get('point:1'), null, 'the held value goes once the model is read again');
  assert.ok(r.gets.some((g) => g.startsWith('api/v1/host')), 'the computer\'s model was read');
});

await ok('a_tap_on_a_point_beyond_the_hand_range_writes_nothing', async () => {
  const far = point('PK', 25000, 15, 20);
  const r = rig({ computer: modelOf({ eq: { ...modelOf().eq, points: [far] } }) });
  const steps = computerSteps();
  const start = startOf(steps, far);
  const write = pointWriter({ actions: r.actions, n: 1 });
  const press = { x: 1000, y: 0 };
  const tapped = createDrag({
    band: 1, gesture: 'move', start, press, plot: { width: 1000, height: 260 }, steps, write,
    on: { held() {}, answer() {}, toast() {}, ended() {} },
  });
  tapped.move({ x: 1000, y: 3 });
  tapped.release();
  await tapped.settled;
  await write.release();
  r.clock.advance(1000);
  await flush();
  assert.deepEqual(r.posts, [], 'a tap, or a hair of movement, writes nothing');
  assert.deepEqual(await r.actions.computer.setPoint(1, {}, { done: true }), { ok: true, status: 0, nothing: true });
  // a hand that moves the gain moves the gain only: the frequency and Q play as written
  const moved = createDrag({
    band: 1, gesture: 'move', start, press, plot: { width: 1000, height: 260 }, steps, write,
    on: { held() {}, answer() {}, toast() {}, ended() {} },
  });
  moved.move({ x: 1000, y: 80 });
  await moved.settled;
  moved.release();
  await write.release();
  assert.equal(r.posts.length >= 1, true);
  for (const [, body] of r.posts) {
    assert.deepEqual(Object.keys(body).sort(), ['done', 'g'], JSON.stringify(body));
  }
});

await ok('a_guest_flips_and_writes_nothing', async () => {
  const guest = rig({ viewer: GUEST });
  guest.computer.flip('computer');
  assert.equal(guest.computer.side(), 'computer');
  const answers = [
    await guest.actions.computer.setVolume(10),
    await guest.actions.computer.step(1),
    await guest.actions.computer.setMute(true),
    await guest.actions.computer.setMode('eq'),
    await guest.actions.computer.recallPreset('Warm'),
    await guest.actions.computer.addPoint({ type: 'PK', f: 100, g: 0, q: 1 }),
    await guest.actions.computer.removePoint(1),
    await guest.actions.computer.setPoint(1, { f: 200 }, { done: false }),
    await guest.actions.computer.setPoint(1, {}, { done: true }),
    await guest.actions.computer.savePreset('Mine'),
  ];
  assert.deepEqual(guest.posts, []);
  assert.deepEqual(guest.gets, []);
  assert.equal(answers.filter((a) => a.ok === false && a.error === 'Sign in to change this').length, 10,
    JSON.stringify(answers));
  assert.equal(guest.holds.get('point:1'), null, 'a guest holds nothing');
  const readOnly = rig({ viewer: { signed_in: true, guest: false, writes: false, read_only: 'Read-only: [web] writes is off' } });
  const refused = await readOnly.actions.computer.setVolume(10);
  assert.deepEqual(readOnly.posts, []);
  assert.equal(refused.error, 'Read-only: [web] writes is off');
});

await ok('each_action_reaches_its_route_with_one_based_points', async () => {
  const table = [
    [(a) => a.setVolume(40.4), 'api/v1/host/volume', { pct: 40 }],
    [(a) => a.setMute(true), 'api/v1/host/mute', { muted: true }],
    [(a) => a.setMode('eq'), 'api/v1/host/eq', { on: true }],
    [(a) => a.setMode('off'), 'api/v1/host/eq', { on: false }],
    [(a) => a.recallPreset('Warm'), 'api/v1/host/eq/preset', { name: 'Warm', device: 'Gungnir 2' }],
    [(a) => a.addPoint({ type: 'pk', f: 100, g: 1.5, q: 1 }), 'api/v1/host/eq/points',
      { type: 'PK', f: 100, g: 1.5, q: 1 }],
    [(a) => a.removePoint(3), 'api/v1/host/eq/points/3/remove', {}],
    [(a) => a.setPoint(2, { f: 300, g: -1 }, { done: true }), 'api/v1/host/eq/points/2',
      { f: 300, g: -1, done: true }],
    [(a) => a.savePreset('Mine'), 'api/v1/devices/Gungnir%202/presets', { name: 'Mine', from: 'computer' }],
  ];
  for (const [run, path, body] of table) {
    const r = rig();
    const answer = await run(r.actions.computer);
    assert.deepEqual(r.posts, [[path, body]], path);
    assert.equal(answer.ok, true, path);
    assert.equal(JSON.stringify(r.posts[0][1]), JSON.stringify(body), `${path}: the body's order`);
  }
  const bad = rig();
  assert.equal((await bad.actions.computer.removePoint(0)).ok, false);
  assert.equal((await bad.actions.computer.setPoint(0, { f: 1 }, {})).ok, false);
  assert.deepEqual(bad.posts, [], 'point 0 is no point');
});

await ok('the_volume_is_capped_stepped_and_inert_before_its_first_read', async () => {
  const first = rig({ computer: modelOf({ volume: null }) });
  const paint = first.computer.volumeOf(first.store.page());
  assert.equal(paint.text, '—');
  assert.equal(paint.pct, null);
  assert.equal(paint.inert, true, 'the first paint is not a live 0 %');
  assert.deepEqual(await first.actions.computer.step(1), { ok: false, status: 0, inert: true });
  assert.deepEqual(first.posts, []);
  const read = rig();
  const live = read.computer.volumeOf(read.store.page());
  assert.deepEqual(live, { pct: 42, muted: false, text: '42', unit: '%', aria: 'Volume 42 %',
    inert: false, boosted: false });
  await read.actions.computer.step(1);
  assert.deepEqual(read.posts.at(-1), ['api/v1/host/volume', { pct: 43 }]);
  await read.actions.computer.setVolume(250);
  assert.deepEqual(read.posts.at(-1)[1], { pct: 100 });
  await read.actions.computer.setVolume(-7);
  assert.deepEqual(read.posts.at(-1)[1], { pct: 0 });
  // a sink another program boosted: shown as read, + does nothing, - writes 100
  const boosted = rig({ computer: modelOf({ volume: { pct: 130, muted: true, sink: 's' } }) });
  const shown = boosted.computer.volumeOf(boosted.store.page());
  assert.equal(shown.text, '130');
  assert.equal(shown.boosted, true);
  assert.equal(shown.aria, 'Volume 130 %, muted');
  assert.equal((await boosted.actions.computer.step(1)).noop, true);
  assert.deepEqual(boosted.posts, []);
  await boosted.actions.computer.step(-1);
  assert.deepEqual(boosted.posts, [['api/v1/host/volume', { pct: 100 }]]);
  // at the ends a step does nothing
  const top = rig({ computer: modelOf({ volume: { pct: 100, muted: false, sink: 's' } }) });
  assert.equal((await top.actions.computer.step(1)).noop, true);
  assert.deepEqual(top.posts, []);
});

await ok('the_volume_goes_one_at_a_time_with_the_newest_waiting', async () => {
  const gates = [];
  const r = rig({ reply: () => new Promise((res) => gates.push(() => res({ status: 200 }))) });
  const first = r.actions.computer.setVolume(10);
  await flush();
  assert.equal((await r.actions.computer.setVolume(20)).queued, true);
  assert.equal((await r.actions.computer.setVolume(30)).queued, true);
  assert.equal(r.posts.length, 1, 'one in flight');
  assert.equal(r.computer.volumeOf(r.store.page()).pct, 30, 'the page shows what was asked');
  gates.shift()();
  await flush();
  assert.deepEqual(r.posts.map(([, body]) => body.pct), [10, 30], 'the newest, not the middle one');
  gates.shift()();
  await first;
  await flush();
  assert.equal(r.holds.get('volume'), null, 'then the model is read again and the value goes');
});

await ok('a_refusal_toasts_the_daemons_words_and_reads_again', async () => {
  const r = rig({ reply: () => ({ status: 503, error: 'computer controls unavailable: no pw-cli' }) });
  const answer = await r.actions.computer.setMode('eq');
  assert.equal(answer.ok, false);
  assert.deepEqual(r.toasts, ['computer controls unavailable: no pw-cli']);
  await flush();
  assert.ok(r.gets.some((g) => g.startsWith('api/v1/host')), 'the computer is read again');
  assert.equal(r.holds.get('mode'), null, 'and what was shown as asked is what the daemon holds');
  const point = rig({ reply: () => ({ status: 409, error: 'too many points' }) });
  const refused = await point.actions.computer.setPoint(1, { f: 100 }, { done: false });
  assert.equal(refused.ok, false);
  assert.deepEqual(point.toasts, ['too many points']);
  assert.equal(point.holds.get('point:1'), null);
});

await ok('the_same_preset_on_both_asks_and_yes_flattens_the_dac', async () => {
  const ask = { kind: 'both_sides', name: 'Warm', devices: ['dac'],
    question: 'Warm is on the dac too: flatten the dac and apply on the computer?' };
  const calls = [];
  const reply = (path, body) => {
    calls.push(body);
    return body.confirm ? { status: 200 } : { status: 409, error: 'Warm is on the dac too', ask };
  };
  const yes = rig({ reply, asked: { questions: [], answer: true } });
  const done = await yes.actions.computer.recallPreset('Warm');
  assert.deepEqual(yes.posts.map(([, body]) => body), [
    { name: 'Warm', device: 'Gungnir 2' },
    { name: 'Warm', device: 'Gungnir 2', confirm: true },
  ]);
  assert.equal(done.ok, true);
  assert.deepEqual(yes.toasts, [], 'a question is not a refusal');
  const no = rig({ reply, asked: { questions: [], answer: false } });
  const declined = await no.actions.computer.recallPreset('Warm');
  assert.equal(no.posts.length, 1, 'no sends nothing');
  assert.equal(declined.declined, true);
  assert.deepEqual(no.toasts, []);
  // the question is the daemon's words, asked
  const seen = { questions: [], answer: false };
  await rig({ reply, asked: seen }).actions.computer.recallPreset('Warm');
  assert.deepEqual(seen.questions, [ask.question]);
  // a refusal that is no question is a toast
  const plain = rig({ reply: () => ({ status: 409, error: '17 filters; the computer EQ holds 16' }) });
  assert.equal((await plain.actions.computer.recallPreset('Huge')).ok, false);
  assert.deepEqual(plain.toasts, ['17 filters; the computer EQ holds 16']);
  assert.equal(calls.length, 4);
});

await ok('the_contract_lists_the_computers_targets', async () => {
  const r = rig();
  const targets = computerTargetsOf(r.store.page());
  assert.deepEqual(targets.slice(0, 5), [{ computer: 'flip' }, { computer: 'volume' },
    { computer: 'mute' }, { computer: 'mode' }, { computer: 'add' }]);
  assert.deepEqual(targets.filter((t) => t.computer === 'point').length, 8);
  assert.deepEqual(targets.filter((t) => t.computer === 'point').slice(0, 4),
    ['type', 'f', 'g', 'q'].map((param) => ({ computer: 'point', n: 1, param })));
  assert.deepEqual(targets.filter((t) => t.computer === 'preset'), [{ computer: 'preset', name: 'Warm' }],
    'a preset the computer cannot play is not a target');
  assert.deepEqual(computerTargetsOf(rig({ computer: null }).store.page()), []);
  assert.deepEqual(computerTargetsOf(rig({ device: null }).store.page()), []);
});

await ok('a_full_model_event_mid_drag_keeps_the_drags_holds_and_another_device_drops_them', async () => {
  const r = rig();
  void r.actions.computer.setPoint(1, { f: 2500 }, { done: false });
  await flush();
  r.store.replace({ ...r.store.page() });
  assert.equal(r.computer.toneOf(r.store.page()).points[0].f, 2500, 'a snapshot of the same device');
  r.store.replace({ ...r.store.page(), shown: 'Amp', device: { name: 'Amp', url: 'Amp' } });
  assert.equal(r.holds.get('point:1'), null, 'another device shown holds nothing of it');
});

await ok('a_newer_read_that_matches_releases_a_hold_and_an_overtaken_read_is_not_shown', async () => {
  const r = rig();
  r.holds.set('volume', 60);
  r.holds.release('volume');
  // the read after a release lands and the hold goes
  r.state.host = { computer: modelOf({ volume: { pct: 10, muted: false, sink: 's' } }) };
  await flush();
  assert.equal(r.holds.get('volume'), null, 'released once the read landed');
  assert.equal(r.store.page().host.computer.volume.pct, 10);
  // a released hold meets a newer read that says what it holds: it goes at once
  r.holds.set('volume', 70);
  r.holds.release('volume');
  r.store.emit('host', { ...r.store.page().host, computer: modelOf({ volume: { pct: 70, muted: false, sink: 's' } }) });
  assert.equal(r.holds.get('volume'), null);
  // a read that does not match leaves a held (unreleased) value in place
  r.holds.set('volume', 80);
  r.store.emit('host', { ...r.store.page().host, computer: modelOf({ volume: { pct: 70, muted: false, sink: 's' } }) });
  assert.equal(r.computer.volumeOf(r.store.page()).pct, 80);
  // the stale read: a stream event arrives while the read is in flight; the read is not shown
  const slow = rig();
  let finish;
  const store = slow.store;
  const holds = createHolds({ store, fetch: () => new Promise((res) => { finish = res; }) });
  holds.set('volume', 55);
  holds.release('volume');
  store.emit('host', { ...store.page().host, computer: modelOf({ volume: { pct: 12, muted: false, sink: 's' } }) });
  finish({ status: 200, json: async () => ({ computer: modelOf({ volume: { pct: 99, muted: false, sink: 's' } }) }) });
  await flush();
  assert.equal(store.page().host.computer.volume.pct, 12, 'the newer stream event stands');
  assert.equal(holds.get('volume'), null);
});

// slots that are not the points' numbers: a point's slot is its own, whatever place it is listed in
const SLOTS = [7, 2, 5];
const slotted = (list) => list.map((p, i) => ({ ...p, slot: SLOTS[i] ?? i + 1 }));
const withPoints = (points) => modelOf({ eq: { ...modelOf().eq, points } });

await ok('a_point_is_found_by_its_slot_and_a_new_one_is_listed_by_frequency', async () => {
  const list = [{ ...point('PK', 226, 0, 1), slot: 5 }, { ...point('PK', 2000, 0, 1), slot: 2 },
    { ...point('PK', 8000, 0, 1), slot: 9 }];
  assert.equal(idAt(list, 2), 2);
  assert.equal(idAt(list, 3), 9);
  assert.equal(idAt(list, 7), 7, 'a point the list does not have is its number');
  assert.equal(indexOfId(list, 9), 3);
  assert.equal(indexOfId(list, 4), null, 'a point that is gone');
  assert.equal(indexOfId([], 1), 1, 'a flat list is the one point 1');
  assert.equal(indexOfId([point('PK', 1000, 0, 1)], 1), 1, 'no slot: the number is the id');
  assert.equal(addedAt([point('PK', 100, 0, 1), point('PK', 1000, 0, 1)], 1000), 3);
  assert.equal(addedAt([point('PK', 100, 0, 1), point('PK', 1000, 0, 1)], 50), 1);
  const r = rig({ computer: withPoints(slotted(list.map(({ slot, ...p }) => p))) });
  const added = await r.actions.computer.addPoint({ type: 'PK', f: 1000, g: 0, q: 1 });
  assert.equal(added.ok, true);
  assert.equal(added.n, 2);
});

await ok('a_hold_follows_its_point_to_its_new_row_and_is_told', async () => {
  const list = slotted([point('PK', 226, 0, 1), point('PK', 2000, 0, 1), point('PK', 8000, 0, 1)]);
  const r = rig({ computer: withPoints(list) });
  const heard = [];
  r.computer.on((event) => heard.push(event));
  await r.actions.computer.setPoint(1, { f: 5000 }, { done: false });
  assert.equal(r.holds.get('point:7').value.f, 5000, 'held by the point\'s slot');
  await r.actions.computer.setPoint(1, {}, { done: true });
  // the daemon lists it second now: the read that says so tells the themes, then frees the hold
  r.state.host = { computer: withPoints([{ ...list[1] }, { ...list[0], f: 5000 }, list[2]]) };
  await flush();
  assert.deepEqual(heard.filter((e) => e.type === 'moved').map(({ from, to }) => [from, to]), [[1, 2]]);
  assert.equal(r.holds.get('point:7'), null);
  assert.equal(r.computer.toneOf(r.store.page()).points[0].f, 2000, 'no held value on the row it left');
});

await ok('a_point_is_followed_a_while_after_its_hold_goes_so_a_late_sort_is_seen', async () => {
  const list = slotted([point('PK', 226, 3, 1), point('PK', 2000, 3, 1)]);
  const r = rig({ computer: withPoints(list) });
  r.holds.set('point:7', { g: 3, f: 300 });
  const heard = [];
  r.computer.on((event) => heard.push(event));
  r.holds.release('point:7');
  // an event listed before the sort says what the hold holds: the hold goes
  r.store.emit('host', { ...r.store.page().host, computer: withPoints([{ ...list[0], f: 300 }, list[1]]) });
  assert.equal(r.holds.get('point:7'), null);
  // and the sort that follows is still seen as the point's move
  r.store.emit('host', { ...r.store.page().host, computer: withPoints([list[1], { ...list[0], f: 300 }]) });
  assert.deepEqual(heard.filter((e) => e.type === 'moved').map(({ from, to }) => [from, to]), [[1, 2]]);
  // another point that merely reads the same values does not free the hold of a point that is
  // not its own
  r.holds.set('point:2', { g: 3, f: 300 });
  r.holds.release('point:2');
  r.store.emit('host', { ...r.store.page().host, computer: withPoints([{ ...list[0], f: 300 }, list[1]]) });
  assert.notEqual(r.holds.get('point:2'), null, 'its own point (slot 2) reads f 2000 on, not 300');
});

await ok('a_drag_goes_on_with_its_own_point_when_another_client_re_sorts_the_table', async () => {
  const list = slotted([point('PK', 226, 0, 1), point('PK', 2000, 0, 1), point('PK', 8000, 0, 1)]);
  const r = rig({ computer: withPoints(list) });
  const routes = () => r.posts.map(([path, body]) => [path, body.done]);
  await r.actions.computer.setPoint(3, { g: 1 }, { done: false });
  assert.deepEqual(routes(), [['api/v1/host/eq/points/3', false]]);
  // another client's release put a new 100 Hz point first: this client's point is point 4 now
  r.store.emit('host', { ...r.store.page().host, computer: withPoints(
    [{ ...point('PK', 100, 0, 1), slot: 4 }, ...list]) });
  r.clock.advance(r.clock.t + 60);
  const next = r.actions.computer.setPoint(3, { g: 2 }, { done: false });
  await flush();
  r.clock.advance(r.clock.t + 60);
  await flush();
  await next;
  const last = r.actions.computer.setPoint(3, {}, { done: true });
  await flush();
  r.clock.advance(r.clock.t + 60);
  await last;
  assert.deepEqual(routes().slice(1), [['api/v1/host/eq/points/4', false], ['api/v1/host/eq/points/4', true]],
    'the live value and the release land on the point that was dragged, not on its neighbour');
  assert.equal(r.holds.get('point:5'), null);
});

await ok('a_drag_of_a_point_another_client_removed_ends_with_no_write', async () => {
  const list = slotted([point('PK', 226, 0, 1), point('PK', 2000, 0, 1), point('PK', 8000, 0, 1)]);
  const r = rig({ computer: withPoints(list) });
  await r.actions.computer.setPoint(3, { g: 1 }, { done: false });
  r.store.emit('host', { ...r.store.page().host, computer: withPoints(list.slice(0, 2)) });
  r.clock.advance(r.clock.t + 60);
  const next = await r.actions.computer.setPoint(3, { g: 2 }, { done: false });
  r.clock.advance(r.clock.t + 60);
  await flush();
  const done = await r.actions.computer.setPoint(3, {}, { done: true });
  await flush();
  assert.equal(r.posts.length, 1, 'nothing was written after the point went');
  assert.ok(next && done);
});

await ok('a_preset_load_ends_a_held_drag_and_nothing_more_is_written_for_it', async () => {
  const list = slotted([point('PK', 226, 0, 1), point('PK', 2000, 0, 1), point('PK', 8000, 0, 1)]);
  const r = rig({ computer: withPoints(list) });
  await r.actions.computer.setPoint(3, { g: 1 }, { done: false });
  assert.equal(r.posts.length, 1);
  // a preset is loaded by another client: the slots are renumbered
  r.store.emit('host', { ...r.store.page().host, computer: { ...withPoints(
    slotted([point('PK', 100, 1, 1), point('PK', 500, 1, 1), point('PK', 900, 1, 1)])),
  eq: { ...modelOf().eq, preset: 'Bass lift', points: slotted([point('PK', 100, 1, 1), point('PK', 500, 1, 1), point('PK', 900, 1, 1)]) } } });
  assert.equal(r.holds.get('point:5'), null, 'the hold ended');
  r.clock.advance(r.clock.t + 60);
  await r.actions.computer.setPoint(3, { g: 2 }, { done: false });
  r.clock.advance(r.clock.t + 60);
  await r.actions.computer.setPoint(3, {}, { done: true });
  await flush();
  assert.equal(r.posts.length, 1, 'no tick and no release landed on the point that has its number now');
  // the next drag is a new one
  await r.actions.computer.setPoint(3, { g: 3 }, { done: false });
  assert.equal(r.posts.length, 2);
});

await ok('every_point_set_and_remove_names_the_point_by_its_slot', async () => {
  const list = slotted([point('PK', 226, 0, 1), point('PK', 2000, 0, 1), point('PK', 8000, 0, 1)]);
  const r = rig({ computer: withPoints(list) });
  await r.actions.computer.setPoint(2, { g: 4 }, { done: false });
  await r.actions.computer.setPoint(2, {}, { done: true });
  await flush();
  assert.ok(r.posts.length >= 1);
  for (const [path, body] of r.posts) {
    assert.equal(path, 'api/v1/host/eq/points/2');
    assert.equal(body.slot, 2, 'the point\'s own slot, not its place');
  }
  const gone = rig({ computer: withPoints(list) });
  await gone.actions.computer.removePoint(3);
  assert.deepEqual(gone.posts, [['api/v1/host/eq/points/3/remove', { slot: 5 }]]);
  const plain = rig({ computer: withPoints([point('PK', 226, 0, 1)]) });
  await plain.actions.computer.removePoint(1);
  assert.deepEqual(plain.posts, [['api/v1/host/eq/points/1/remove', {}]], 'a model with no slots sends none');
});

await ok('a_re_sort_the_page_has_not_read_yet_still_lands_on_the_point_that_was_dragged', async () => {
  const list = slotted([point('PK', 226, 0, 1), point('PK', 2000, 0, 1), point('PK', 8000, 0, 1)]);
  const r = rig({ computer: withPoints(list) });
  await r.actions.computer.setPoint(3, { g: 1 }, { done: false });
  assert.deepEqual(r.posts.map(([path, body]) => [path, body.slot]), [['api/v1/host/eq/points/3', 5]]);
  r.store.emit('host', { ...r.store.page().host, computer: withPoints(
    [{ ...point('PK', 100, 0, 1), slot: 4 }, ...list]) });
  r.clock.advance(r.clock.t + 60);
  const next = r.actions.computer.setPoint(3, { g: 2 }, { done: false });
  await flush();
  r.clock.advance(r.clock.t + 60);
  await flush();
  await next;
  assert.equal(r.posts.at(-1)[1].slot, 5, 'the slot of the dragged point, at its new place');
  assert.equal(r.posts.at(-1)[0], 'api/v1/host/eq/points/4');
});

await ok('a_409_for_a_point_that_moved_ends_the_drag_toasts_once_and_sends_nothing_more', async () => {
  const list = slotted([point('PK', 226, 0, 1), point('PK', 2000, 0, 1), point('PK', 8000, 0, 1)]);
  const r = rig({ computer: withPoints(list),
    reply: () => ({ status: 409, error: 'the point in slot 5 has moved or is gone. Read the points again' }) });
  const first = await r.actions.computer.setPoint(3, { g: 1 }, { done: false });
  assert.equal(first.ok, false);
  assert.deepEqual(r.toasts, ['That point moved. Try again.']);
  await flush();
  assert.ok(r.gets.some((g) => g.startsWith('api/v1/host')), 'the points are read again');
  r.clock.advance(r.clock.t + 60);
  await r.actions.computer.setPoint(3, { g: 2 }, { done: false });
  r.clock.advance(r.clock.t + 60);
  await r.actions.computer.setPoint(3, {}, { done: true });
  await flush();
  assert.equal(r.posts.length, 1, 'no retry and no further write for the ended drag');
  assert.equal(r.toasts.length, 1);
  // the next drag is a new one
  await r.actions.computer.setPoint(3, { g: 3 }, { done: false });
  assert.equal(r.posts.length, 2);
  const removed = rig({ computer: withPoints(list),
    reply: () => ({ status: 409, error: 'the point in slot 5 has moved or is gone. Read the points again' }) });
  assert.equal((await removed.actions.computer.removePoint(3)).ok, false);
  assert.deepEqual(removed.toasts, ['That point moved. Try again.']);
  assert.equal(removed.posts.length, 1);
});

await ok('a_cell_equal_to_its_own_text_or_the_points_value_writes_nothing', async () => {
  const beyond = slotted([point('PK', 1000, 14, 1), point('PK', 100, -2, 0.7)]);
  const r = rig({ computer: withPoints(beyond) });
  const c = r.actions.computer;
  // the cell of a point at +14 dB shows +14.0 dB: typing 14 back is not a write of the hand range's 12
  assert.equal((await c.cell(1, 'g', '+14', '+14.0 dB')).nothing, true);
  assert.equal((await c.cell(1, 'g', '14', '+14.0 dB')).nothing, true);
  // the text of the cell, units and all, and the same number another way
  assert.equal((await c.cell(2, 'f', '100 Hz', '100 Hz')).nothing, true);
  assert.equal((await c.cell(2, 'f', '0,1k', '100 Hz')).nothing, true);
  // the cell rounds: typing what it shows, or the exact value the point holds, is no change
  const fine = rig({ computer: withPoints(slotted([point('PK', 1000, 3.24, 1)])) });
  assert.equal((await fine.actions.computer.cell(1, 'g', '3.2', '+3.2 dB')).nothing, true);
  assert.equal((await fine.actions.computer.cell(1, 'g', '3.24', '+3.2 dB')).nothing, true);
  // the cell showed another value (a host update since): the point's own value is still no change
  const stale = rig({ computer: withPoints(slotted([point('PK', 1000, 3.2, 1)])) });
  assert.equal((await stale.actions.computer.cell(1, 'g', '3.2', '+1.0 dB')).nothing, true);
  assert.deepEqual(stale.posts, []);
  assert.deepEqual(fine.posts, []);
  assert.deepEqual(r.posts, []);
  // a value the point does not hold is written, held to the hand range
  await c.cell(1, 'g', '40', '+14.0 dB');
  assert.deepEqual(r.posts.at(-1)[1], { g: 12, done: true, slot: 7 });
  await c.cell(2, 'f', '2.9k', '100 Hz');
  assert.equal(r.posts.at(-1)[1].f, 2900);
  // a comma is read both ways and the field's range tells which: 1,000 Hz is 1000 (1 is below the range),
  // 20,500 is 20.5 (20500 is above it), 1,5k is 1500; a comma the range cannot settle is asked again
  const typedF = async (text) => { await c.cell(2, 'f', text, '100 Hz'); return r.posts.at(-1)[1].f; };
  assert.equal(await typedF('1,000'), 1000);
  assert.equal(await typedF('1,5k'), 1500);
  assert.equal(await typedF('20,5'), 20.5);
  assert.equal(await typedF('20,500'), 20.5);
  assert.equal(await typedF('2.9k'), 2900);
  // a group that starts with 0 is a decimal, never thousands: 0,500 Hz is 0.5 (held to the range's end), not 500
  assert.equal(await typedF('0,500'), HAND.f[0]);
  await c.cell(2, 'q', '0,010', '0.70');
  assert.equal(r.posts.at(-1)[1].q, HAND.q[0], '0,010 is 0.01, never 10');
  // neither reading in range: the plain out-of-range answer (held to the range), not the way to write it
  const toasted = r.toasts.length;
  assert.equal(await typedF('-1,000'), HAND.f[0]);
  await c.cell(2, 'q', '0,000', '0.70');
  assert.equal(r.posts.at(-1)[1].q, HAND.q[0]);
  assert.equal(r.toasts.length, toasted, 'no hint when neither reading is in range');
  await c.cell(2, 'g', '+3,2', '-2.0 dB');
  assert.equal(r.posts.at(-1)[1].g, 3.2);
  await c.cell(2, 'q', '0,707', '0.70');
  assert.equal(r.posts.at(-1)[1].q, 0.71);
  await c.cell(2, 'q', '1,414', '0.70');
  assert.equal(r.posts.at(-1)[1].q, 1.41);
  // not a number is toasted and writes nothing
  const before = r.posts.length;
  assert.equal((await c.cell(2, 'g', 'loud', '-2.0 dB')).invalid, true);
  assert.deepEqual(r.toasts, ['Not a number.']);
  assert.equal(r.posts.length, before);
  assert.equal((await c.cell(0, 'g', '1', '')).ok, false);
  const asked = r.posts.length;
  const unsure = await c.cell(2, 'f', '20,000', '100 Hz');
  assert.equal(unsure.invalid, true);
  assert.equal(r.toasts.at(-1), 'Write 20 or 20k.');
  assert.equal(r.posts.length, asked, 'a comma the range cannot settle writes nothing');
});

await ok('an_arrow_step_never_goes_against_its_key', async () => {
  const beyond = slotted([point('PK', 1000, 14, 1), point('PK', 100, 12, 0.7)]);
  const r = rig({ computer: withPoints(beyond) });
  const c = r.actions.computer;
  // above the hand range: up is a no-op, down is one step from the range's end
  assert.equal((await c.stepPoint(1, 'g', 1)).nothing, true);
  assert.deepEqual(r.posts, []);
  await c.stepPoint(1, 'g', -1);
  assert.equal(r.posts.at(-1)[1].g, 11.9);
  // at the end of the table up is a no-op as well
  assert.equal((await c.stepPoint(2, 'g', 1)).nothing, true);
  assert.equal(r.posts.length, 1);
  await c.stepPoint(2, 'g', -1);
  assert.equal(r.posts.at(-1)[1].g, 11.9);
  assert.equal((await c.stepPoint(0, 'g', 1)).ok, false);
  assert.equal((await c.stepPoint(1, 'x', 1)).ok, false);
});

await ok('refuse_posts_nothing_for_a_guest_and_a_read_only_viewer_and_says_why', async () => {
  const guest = rig({ viewer: GUEST });
  assert.equal(guest.actions.refuse().ok, false);
  assert.equal(guest.toasts.length, 1);
  const ro = rig({ viewer: { signed_in: true, guest: false, writes: false, read_only: 'No writes here' } });
  assert.equal(ro.actions.refuse().error, 'No writes here');
  assert.deepEqual(ro.toasts, ['No writes here']);
  const open = rig();
  assert.equal(open.actions.refuse(), null);
  assert.deepEqual(open.toasts, []);
  for (const r of [guest, ro, open]) assert.deepEqual([r.posts, r.gets], [[], []]);
  for (const r of [guest, ro]) {
    assert.equal((await r.actions.computer.cell(1, 'g', '1', '')).ok, false);
    assert.equal((await r.actions.computer.stepPoint(1, 'g', 1)).ok, false);
    assert.deepEqual(r.posts, []);
  }
});

await ok('can_write_says_whether_the_page_may_write_and_says_nothing_else', async () => {
  const guest = rig({ viewer: GUEST });
  const ro = rig({ viewer: { signed_in: true, guest: false, writes: false, read_only: 'No writes here' } });
  const open = rig();
  assert.deepEqual([guest, ro, open].map((r) => r.actions.canWrite()), [false, false, true]);
  // the page it is asked about, the store's by default
  assert.equal(open.actions.canWrite(guest.store.page()), false);
  assert.equal(guest.actions.canWrite(open.store.page()), true);
  assert.equal(open.actions.canWrite({}), true, 'a page with no viewer says nothing against it');
  assert.equal(open.actions.canWrite({ viewer: { guest: true, writes: true } }), false, 'a guest never writes');
  for (const r of [guest, ro, open]) assert.deepEqual([r.toasts, r.posts, r.gets], [[], [], []]);
  // it agrees with `refuse`, which turns away exactly the viewers it says may not write
  for (const r of [guest, ro, open]) assert.equal(r.actions.canWrite(), r.actions.refuse() === null);
});

await ok('a_removed_point_leaves_the_hand_and_the_ones_after_it_are_one_place_higher_and_a_failed_remove_changes_nothing', async () => {
  const list = slotted([point('PK', 226, 0, 1), point('PK', 2000, 0, 1), point('PK', 8000, 0, 1)]);
  const heard = [];
  const flipped = (r) => { r.computer.flip('computer'); r.computer.on((e) => { if (e.type === 'picked') { heard.push(e.n); assert.equal(e.side, 'computer'); } }); return r; };
  // a point after the one in hand: it stays
  const r = flipped(rig({ computer: withPoints(list) }));
  r.computer.pick(2);
  await r.actions.computer.removePoint(3);
  assert.equal(r.computer.picked(), 2);
  assert.deepEqual(heard, [], 'nothing changed: nothing said');
  // a point before it: the pick is the one before, and the listeners hear it
  r.computer.pick(3);
  await r.actions.computer.removePoint(1);
  assert.equal(r.computer.picked(), 2);
  assert.deepEqual(heard, [2]);
  // the point in hand itself: none in hand, said
  await r.actions.computer.removePoint(2);
  assert.equal(r.computer.picked(), null);
  assert.deepEqual(heard, [2, null]);
  // nothing in hand: nothing to say
  await r.actions.computer.removePoint(1);
  assert.deepEqual(heard, [2, null]);
  // a refusal, a failure and a point that moved keep the pick
  for (const reply of [() => ({ status: 500, error: 'down' }), () => ({ status: 409, error: 'the point in slot 5 has moved or is gone. Read' })]) {
    const f = flipped(rig({ computer: withPoints(list), reply }));
    f.computer.pick(2);
    const before = heard.length;
    await f.actions.computer.removePoint(2);
    assert.equal(f.computer.picked(), 2);
    assert.equal(heard.length, before);
    await f.actions.computer.removePoint(1);
    assert.equal(f.computer.picked(), 2, 'not renumbered either');
  }
  // a call the page turns away removes nothing and leaves the hand alone
  const guest = flipped(rig({ computer: withPoints(list), viewer: GUEST }));
  guest.computer.pick(2);
  await guest.actions.computer.removePoint(2);
  assert.equal(guest.computer.picked(), 2);
  // actions made with no computer remove all the same
  const bare = createActions({
    store: r.store, post: async () => ({ status: 200, ok: true }), toast() {}, history: {}, location: {}, holds: r.holds,
  });
  assert.equal((await bare.computer.removePoint(1)).ok, true);
});

await ok('a_remove_of_a_point_that_moved_says_so_in_a_flag_and_the_hand_ranges_are_the_rust_ones', async () => {
  const list = slotted([point('PK', 226, 0, 1), point('PK', 2000, 0, 1)]);
  const r = rig({ computer: withPoints(list),
    reply: () => ({ status: 409, error: 'the point in slot 5 has moved or is gone. Read the points again' }) });
  assert.equal((await r.actions.computer.removePoint(2)).moved, true);
  // crates/svolder-host/src/eq/points.rs: HAND
  assert.deepEqual(HAND, { f: [20, 20000, 2], g: [-12, 12, 1], q: [0.1, 10, 2] });
  const points = readFileSync(new URL('../../../crates/svolder-host/src/eq/points.rs', import.meta.url), 'utf8');
  assert.match(points, /f: \(20\.0, 20_?000\.0\)/);
  assert.match(points, /g: \(-12\.0, 12\.0\)/);
  assert.match(points, /q: \(0\.1, 10\.0\)/);
  const t = computerSteps();
  assert.deepEqual([t.f[0], t.f.at(-1), t.g[0], t.g.at(-1), t.q[0], t.q.at(-1)], [20, 20000, -12, 12, 0.1, 10]);
});

await ok('the_point_types_are_the_ones_the_daemon_names', async () => {
  assert.deepEqual(POINT_TYPES, ['PK', 'LSC', 'HSC']);
  // crates/svolder-host/src/eq/control.rs: kind_word
  const control = readFileSync(new URL('../../../crates/svolder-host/src/eq/control.rs', import.meta.url), 'utf8');
  for (const type of POINT_TYPES) assert.match(control, new RegExp(`=> "${type}",`));
  assert.equal(FLAT_POINT.type, POINT_TYPES[0], 'a flat curve is one peaking point');
});

await ok('an_answered_question_gives_the_focus_to_returnTo_found_again_by_id', async () => {
  const focused = [];
  const el = (tag) => {
    const e = { tag, children: [], listeners: {}, attrs: {}, inert: false, id: '', isConnected: true };
    e.append = (...kids) => { e.children.push(...kids); };
    e.setAttribute = (k, v) => { e.attrs[k] = v; };
    e.addEventListener = (type, fn) => { e.listeners[type] = fn; };
    e.contains = () => false;
    e.focus = () => { focused.push(e.id || e.tag); doc.activeElement = e; };
    e.remove = () => { e.isConnected = false; };
    return e;
  };
  const body = el('body');
  const doc = { body, activeElement: null, addEventListener() {}, createElement: el,
    getElementById: (id) => byId.get(id) ?? null };
  const byId = new Map();
  const old = el('button'); old.id = 'chip'; byId.set('chip', old);
  doc.activeElement = old;
  const ask = createAsk({ document: doc });
  const answer = ask('Play on both?', { returnTo: 'chip' });
  // the chip is drawn again while the card is open: a twin has the id now
  const twin = el('button'); twin.id = 'chip'; byId.set('chip', twin);
  const buttons = body.children.at(-1).children[1].children;
  buttons[0].listeners.click();
  assert.equal(await answer, true);
  assert.equal(focused.at(-1), 'chip');
  assert.equal(doc.activeElement, twin);
  // with no returnTo the focus goes back to where it was
  doc.activeElement = old;
  const plain = ask('Again?');
  body.children.at(-1).children[1].children[1].listeners.click();
  assert.equal(await plain, false);
  assert.equal(doc.activeElement, old);
});

await ok('a_typed_value_is_rounded_to_its_fields_places_before_the_check_and_the_write', async () => {
  const r = rig({ computer: withPoints(slotted([point('PK', 1000, 3.1, 1), point('PK', 100, -2, 0.7)])) });
  const c = r.actions.computer;
  await c.cell(1, 'g', '3.14159', '+3.1 dB');
  assert.deepEqual(r.posts, [], 'the rounded value is the point\'s own: nothing is written');
  await c.cell(1, 'g', '5.55', '+3.1 dB');
  assert.equal(r.posts.at(-1)[1].g, 5.6);
  await c.cell(2, 'q', '1.23456', '0.70');
  assert.equal(r.posts.at(-1)[1].q, 1.23);
  await c.cell(2, 'f', '123.4567', '100 Hz');
  assert.equal(r.posts.at(-1)[1].f, 123.46);
  assert.deepEqual(Object.values(HAND).map((range) => range[2]), [2, 1, 2]);
  assert.equal((await c.cell(1, 'type', '5', '')).ok, false, 'a field the hand has no range for');
  assert.equal((await c.stepPoint(1, 'type', 1)).ok, false);
});

await ok('the_most_points_is_the_models_and_a_table_of_that_many_is_full', async () => {
  const eight = Array.from({ length: 8 }, (_, i) => point('PK', 100 + i * 10, 0, 1));
  const r = rig({ computer: modelOf({ eq: { ...modelOf().eq, max: 8, points: eight } }) });
  const tone = r.computer.toneOf(r.store.page());
  assert.equal(tone.max, 8);
  assert.equal(tone.canAdd, false);
  assert.match(tone.line, /^8 of 8 points/);
  const older = rig({ computer: modelOf({ eq: { ...modelOf().eq, max: undefined } }) });
  assert.equal(older.computer.toneOf(older.store.page()).max, 16);
});

await ok('the_focus_lands_on_a_control_drawn_again_after_the_answer_and_after_a_no', async () => {
  const el = (tag) => {
    const e = { tag, children: [], listeners: {}, attrs: {}, inert: false, id: '', isConnected: true };
    e.append = (...kids) => { e.children.push(...kids); };
    e.setAttribute = (k, v) => { e.attrs[k] = v; };
    e.addEventListener = (type, fn) => { e.listeners[type] = fn; };
    e.contains = () => false;
    e.focus = () => { doc.activeElement = e; };
    e.remove = () => { e.isConnected = false; if (doc.activeElement === e) doc.activeElement = body; };
    return e;
  };
  const body = el('body');
  const byId = new Map();
  const doc = { body, activeElement: body, addEventListener() {}, createElement: el,
    getElementById: (id) => byId.get(id) ?? null };
  const draw = () => {
    const old = byId.get('chip');
    old?.remove();
    const chip = el('button'); chip.id = 'chip'; byId.set('chip', chip);
    return chip;
  };
  for (const button of [0, 1]) {
    draw().focus();
    const ask = createAsk({ document: doc });
    const caller = (async () => {
      const answer = await ask('Play on both?', { returnTo: 'chip' });
      // the caller's own work: the chip is drawn again once the answer is in
      draw();
      return answer;
    })();
    body.children.at(-1).children[1].children[button].listeners.click();
    assert.equal(await caller, button === 0);
    await flush();
    assert.equal(doc.activeElement, byId.get('chip'), `answer ${button}: the new chip has the focus`);
    assert.equal(doc.activeElement.isConnected, true);
  }
  // a focus the person moved meanwhile is left where it is
  draw().focus();
  const other = el('input'); other.id = 'other';
  const ask = createAsk({ document: doc });
  const asked = ask('Again?', { returnTo: 'chip' });
  body.children.at(-1).children[1].children[1].listeners.click();
  other.focus();
  await asked;
  await flush();
  assert.equal(doc.activeElement, other);
});

await ok('add_next_adds_a_flat_pk_in_the_widest_gap_and_says_when_the_table_is_full', async () => {
  const r = rig();
  const done = await r.actions.computer.addNext();
  assert.deepEqual(r.posts, [['api/v1/host/eq/points', { type: 'PK', f: 4472.14, g: 0, q: 1 }]]);
  assert.equal(done.ok, true);
  assert.equal(done.n, 3, 'the new point is listed after the 1 kHz one');
  // a point being dragged is counted where it is held, not where the model has it
  const held = rig();
  held.holds.set('point:1', { f: 15000 });
  await held.actions.computer.addNext();
  assert.equal(held.posts[0][1].f, 1224.74);
  const eight = Array.from({ length: 8 }, (_, i) => point('PK', 100 + i * 10, 0, 1));
  const full = rig({ computer: modelOf({ eq: { ...modelOf().eq, max: 8, points: eight } }) });
  const no = await full.actions.computer.addNext();
  assert.deepEqual([no.ok, no.full, no.max, no.error], [false, true, 8, '8 is the most']);
  assert.deepEqual(full.posts, []);
  const none = rig({ computer: modelOf({ eq: null }) });
  assert.equal((await none.actions.computer.addNext()).ok, false);
  assert.deepEqual(none.posts, []);
  const guest = rig({ viewer: GUEST });
  assert.equal((await guest.actions.computer.addNext()).ok, false);
  assert.deepEqual(guest.posts, []);
});

await ok('a_preset_is_renamed_or_deleted_on_the_shown_device_and_the_computer_reads_its_list_again', async () => {
  const r = rig();
  const done = await r.actions.computer.renamePreset('Mine', 'Mine 2');
  assert.deepEqual(r.posts, [['api/v1/devices/Gungnir%202/presets/rename', { from: 'Mine', to: 'Mine 2' }]]);
  assert.equal(done.ok, true);
  await flush();
  assert.equal(r.gets.some((g) => g.startsWith('api/v1/host')), true, 'the list is read again');
  const d = rig();
  await d.actions.computer.deletePreset('Mine');
  assert.deepEqual(d.posts, [['api/v1/devices/Gungnir%202/presets/delete', { name: 'Mine' }]]);
  await flush();
  assert.equal(d.gets.some((g) => g.startsWith('api/v1/host')), true);
  const refused = rig({ reply: () => ({ status: 409, error: 'No such preset' }) });
  assert.equal((await refused.actions.computer.deletePreset('Gone')).ok, false);
  await flush();
  assert.deepEqual(refused.toasts, ['No such preset']);
  assert.equal(refused.gets.some((g) => g.startsWith('api/v1/host')), false, 'a refusal reads nothing');
  const guest = rig({ viewer: GUEST });
  assert.equal((await guest.actions.computer.renamePreset('a', 'b')).ok, false);
  assert.deepEqual([guest.posts, guest.gets], [[], []]);
});

await ok('nudge_steps_the_volume_by_one_or_by_a_larger_step_and_mute_toggles_what_is_drawn', async () => {
  const r = rig();
  const c = r.actions.computer;
  await c.nudge(1);
  assert.deepEqual(r.posts.at(-1), ['api/v1/host/volume', { pct: 43 }]);
  await flush();
  await c.nudge(-10);
  assert.deepEqual(r.posts.at(-1), ['api/v1/host/volume', { pct: 32 }]);
  await flush();
  await c.nudge(500);
  assert.deepEqual(r.posts.at(-1), ['api/v1/host/volume', { pct: 100 }]);
  await flush();
  await c.toggleMute();
  assert.deepEqual(r.posts.at(-1), ['api/v1/host/mute', { muted: true }]);
  await flush();
  const unread = rig({ computer: modelOf({ volume: null }) });
  assert.equal((await unread.actions.computer.nudge(10)).inert, true);
  assert.equal((await unread.actions.computer.toggleMute()).inert, true);
  assert.deepEqual(unread.posts, [], 'nothing is written before the first read');
  const guest = rig({ viewer: GUEST });
  assert.equal((await guest.actions.computer.nudge(10)).ok, false);
  assert.equal((await guest.actions.computer.toggleMute()).error, 'Sign in to change this');
  assert.deepEqual(guest.posts, []);
  assert.equal(guest.toasts.length, 2, 'the guest is told why');
});

await ok('current_lays_the_values_being_changed_over_the_points', async () => {
  const r = rig({ computer: withPoints(slotted([point('PK', 1000, 3, 1), point('LSC', 100, -2, 0.7)])) });
  assert.deepEqual(r.actions.computer.current(2), { type: 'LSC', f: 100, g: -2, q: 0.7, slot: SLOTS[1] });
  r.holds.set(`point:${SLOTS[1]}`, { g: 5 });
  assert.equal(r.actions.computer.current(2).g, 5);
  assert.deepEqual(r.actions.computer.current(9), FLAT_POINT, 'a point the list has not is the flat one');
});

await ok('the_eq_is_a_light_read_of_the_mode_the_preset_and_whether_it_is_confirmed', async () => {
  const r = rig();
  assert.equal(r.computer.eqOf(r.store.page()).preset, 'Warm');
  assert.deepEqual(r.computer.eqOf(r.store.page()), { on: true, preset: 'Warm', verified: true });
  // a mode being changed is laid over the model's, as `toneOf` does
  r.holds.set('mode', false);
  assert.equal(r.computer.eqOf(r.store.page()).on, false);
  assert.equal(r.computer.toneOf(r.store.page()).modes.find((m) => m.name === 'eq').on, false);
  r.holds.drop('mode');
  // a preset the model has none of, a state that is only what the file says
  const unsure = rig({ computer: modelOf({ eq: { ...modelOf().eq, preset: null, running_unknown: true } }) });
  assert.deepEqual(unsure.computer.eqOf(unsure.store.page()), { on: true, preset: null, verified: false });
  const tone = unsure.computer.toneOf(unsure.store.page());
  assert.deepEqual([tone.verified, tone.preset], [false, null], 'the tone says the same');
  // no EQ, or no computer: nothing
  const none = rig({ computer: modelOf({ eq: null }) });
  assert.equal(none.computer.eqOf(none.store.page()), null);
  const bare = rig({ computer: null });
  assert.equal(bare.computer.eqOf(bare.store.page()), null);
  // on the view, lazily and only on the computer's side; it does not make the tone
  let tones = 0;
  const real = r.computer.toneOf;
  r.computer.toneOf = (...args) => { tones++; return real(...args); };
  assert.equal(r.computer.view(r.store.page()).eq, null, 'the device\'s side has none');
  r.computer.flip('computer');
  const view = r.computer.view(r.store.page());
  assert.equal(typeof Object.getOwnPropertyDescriptor(view, 'eq').get, 'function');
  assert.deepEqual(view.eq, { on: true, preset: 'Warm', verified: true });
  assert.equal(view.eq, view.eq, 'made once per view');
  assert.equal(tones, 0, 'the 181 frequencies are not worked out for it');
  r.computer.toneOf = real;
});

await ok('the_view_is_the_switch_and_lazily_the_volume_tone_and_the_point_picked', async () => {
  const r = rig();
  const bare = rig({ computer: null });
  assert.equal(bare.computer.view(bare.store.page()), null, 'no computer in the model: no switch');
  // the device's side: the switch only
  const dac = r.computer.view(r.store.page());
  assert.equal(dac.side, 'dac');
  assert.equal(dac.sw.computer, 'This computer');
  assert.deepEqual([dac.volume, dac.tone, dac.picked, dac.writes], [null, null, null, true]);
  r.computer.flip('computer');
  const view = r.computer.view(r.store.page());
  assert.equal(view.side, 'computer');
  // lazy: what is not read is not made
  const volume = Object.getOwnPropertyDescriptor(view, 'volume');
  const tone = Object.getOwnPropertyDescriptor(view, 'tone');
  assert.equal(typeof volume.get, 'function');
  assert.equal(typeof tone.get, 'function');
  assert.equal(view.volume.pct, 42);
  assert.equal(view.tone.points.length, 2);
  assert.equal(view.tone, view.tone, 'made once per view');
  assert.equal(r.computer.view(rig({ viewer: GUEST }).store.page()).writes, false);
  assert.equal(r.computer.picked(), null);
  r.computer.pick(2);
  assert.equal(r.computer.picked(), 2);
  assert.equal(r.computer.view(r.store.page()).picked, 2);
  // the side flips: nothing is picked on the device's side
  r.computer.flip('dac');
  assert.equal(r.computer.picked(), null);
  assert.equal(r.computer.view(r.store.page()).picked, null);
  r.computer.flip('computer');
  r.computer.pick(2);
  r.computer.flip('dac');
  r.computer.flip('computer');
  assert.equal(r.computer.picked(), null, 'and nothing is picked when it comes back');
  // another device shown: the pick is gone
  r.computer.pick(1);
  r.store.replace({ ...r.store.page(), shown: 'Other', device: { ...dacDevice, name: 'Other', url: 'Other' } });
  assert.equal(r.computer.picked(), null);
  r.computer.pick(null);
});

await ok('the_point_picked_follows_a_re_sort_the_way_a_hold_does', async () => {
  const list = slotted([point('PK', 226, 0, 1), point('PK', 2000, 0, 1), point('PK', 8000, 0, 1)]);
  const r = rig({ computer: withPoints(list) });
  r.computer.flip('computer');
  r.computer.pick(1);
  await r.actions.computer.setPoint(1, { f: 5000 }, { done: false });
  await r.actions.computer.setPoint(1, {}, { done: true });
  r.state.host = { computer: withPoints([{ ...list[1] }, { ...list[0], f: 5000 }, list[2]]) };
  await flush();
  assert.equal(r.computer.picked(), 2);
  // a pick of another row stays
  r.computer.pick(3);
  await r.actions.computer.setPoint(1, { f: 100 }, { done: true });
  await flush();
  assert.equal(r.computer.picked(), 3);
});

await ok('the_focus_lands_on_a_control_drawn_again_long_after_the_answer_and_the_watch_ends', async () => {
  const el = (tag) => {
    const e = { tag, children: [], listeners: {}, attrs: {}, inert: false, id: '', isConnected: true };
    e.append = (...kids) => { e.children.push(...kids); };
    e.setAttribute = (k, v) => { e.attrs[k] = v; };
    e.addEventListener = (type, fn) => { e.listeners[type] = fn; };
    e.contains = () => false;
    e.focus = () => { doc.activeElement = e; };
    e.remove = () => { e.isConnected = false; if (doc.activeElement === e) doc.activeElement = body; };
    return e;
  };
  const body = el('body');
  const byId = new Map();
  const observers = [];
  const timers = [];
  const view = {
    MutationObserver: class {
      constructor(fn) { this.fn = fn; this.on = false; observers.push(this); }
      observe() { this.on = true; }
      disconnect() { this.on = false; }
    },
    setTimeout: (fn, ms) => { timers.push({ fn, ms }); return timers.length; },
    clearTimeout: () => {},
  };
  const doc = { body, activeElement: body, defaultView: view, addEventListener() {}, createElement: el,
    getElementById: (id) => byId.get(id) ?? null };
  const draw = () => {
    byId.get('chip')?.remove();
    const chip = el('button'); chip.id = 'chip'; byId.set('chip', chip);
    return chip;
  };
  const mutate = () => observers.filter((o) => o.on).forEach((o) => o.fn());
  for (const button of [0, 1]) {
    observers.length = 0;
    draw().focus();
    const ask = createAsk({ document: doc });
    const answer = ask('Play on both?', { returnTo: 'chip' });
    body.children.at(-1).children[1].children[button].listeners.click();
    await answer;
    await flush();
    // the write and the model read take their time: the chip is drawn again well after the answer
    const chip = draw();
    assert.notEqual(doc.activeElement, chip);
    mutate();
    assert.equal(doc.activeElement, chip, `answer ${button}: the chip drawn late has the focus`);
    assert.equal(observers.every((o) => !o.on), true, 'the watch ends once the focus has landed');
  }
  // the window is over: a chip drawn after it is not given the focus
  observers.length = 0;
  draw().focus();
  const ask = createAsk({ document: doc });
  const answer = ask('Again?', { returnTo: 'chip' });
  body.children.at(-1).children[1].children[1].listeners.click();
  await answer;
  assert.equal(timers.at(-1).ms, 1500);
  timers.at(-1).fn();
  assert.equal(observers.every((o) => !o.on), true);
  // a new question ends the watch of the old one
  const second = ask('Next?', { returnTo: 'chip' });
  body.children.at(-1).children[1].children[1].listeners.click();
  await second;
});

await ok('a_press_or_a_key_of_the_persons_own_after_the_answer_ends_the_focus_watch', async () => {
  const el = (tag) => {
    const e = { tag, children: [], listeners: {}, attrs: {}, inert: false, id: '', isConnected: true };
    e.append = (...kids) => { e.children.push(...kids); };
    e.setAttribute = (k, v) => { e.attrs[k] = v; };
    e.addEventListener = (type, fn) => { e.listeners[type] = fn; };
    e.contains = () => false;
    e.focus = () => { doc.activeElement = e; };
    e.remove = () => { e.isConnected = false; if (doc.activeElement === e) doc.activeElement = body; };
    return e;
  };
  const body = el('body');
  const byId = new Map();
  const observers = [];
  const view = {
    MutationObserver: class {
      constructor(fn) { this.fn = fn; this.on = false; observers.push(this); }
      observe() { this.on = true; }
      disconnect() { this.on = false; }
    },
    setTimeout: () => 1,
    clearTimeout: () => {},
  };
  const heard = new Map();
  const doc = { body, activeElement: body, defaultView: view, createElement: el,
    addEventListener: (type, fn) => { heard.set(type, fn); },
    removeEventListener: (type, fn) => { if (heard.get(type) === fn) heard.delete(type); },
    getElementById: (id) => byId.get(id) ?? null };
  const draw = () => {
    byId.get('chip')?.remove();
    const chip = el('button'); chip.id = 'chip'; byId.set('chip', chip);
    return chip;
  };
  const mutate = () => observers.filter((o) => o.on).forEach((o) => o.fn());
  for (const type of ['pointerdown', 'keydown']) {
    observers.length = 0;
    draw().focus();
    const ask = createAsk({ document: doc });
    const answer = ask('Play on both?', { returnTo: 'chip' });
    body.children.at(-1).children[1].children[0].listeners.click();
    await answer;
    await flush();
    // the person clicks blank ground (or types) before the chip is drawn again
    doc.activeElement = body;
    assert.equal(typeof heard.get(type), 'function', `${type}: the watch hears the person`);
    heard.get(type)();
    const chip = draw();
    mutate();
    assert.notEqual(doc.activeElement, chip, `${type}: the focus is left where the person put it`);
    assert.equal(observers.every((o) => !o.on), true, `${type}: the watch is over`);
    assert.equal(heard.has('pointerdown') || heard.has('keydown'), false, `${type}: nothing still heard`);
  }
});

console.log(`core-computer.mjs: ${n} passed`);
