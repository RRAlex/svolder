/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for core/stream.js, the page's one event stream: its address, the reopen after an
// error, the session question, the rest of a hidden page and the wait after it, a device switched
// while it rests, and a device no longer followed. A fake EventSource, a fake clock and a fake
// watch of the page's visibility stand in for the browser. Run: node core-stream.mjs
import assert from 'node:assert/strict';
import { createActions } from '../../../bins/svolderd/web/core/actions.js';
import { createStore } from '../../../bins/svolderd/web/core/store.js';
import {
  REOPEN_MS, WAIT_MS, createStream, streamUrl,
} from '../../../bins/svolderd/web/core/stream.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };
const flush = () => new Promise((resolve) => setImmediate(resolve));

// A clock that moves when the test says.
let now = 0;
let seq = 0;
let timers = [];
globalThis.setTimeout = (fn, ms) => { seq++; timers.push({ id: seq, at: now + ms, fn }); return seq; };
globalThis.clearTimeout = (id) => { timers = timers.filter((t) => t.id !== id); };
const advance = (ms) => {
  const end = now + ms;
  for (;;) {
    const due = timers.filter((t) => t.at <= end).sort((a, b) => a.at - b.at)[0];
    if (!due) break;
    timers = timers.filter((t) => t !== due);
    now = due.at;
    due.fn();
  }
  now = end;
};

class FakeSource {
  static all = [];

  constructor(url) {
    this.url = url;
    this.closed = false;
    this.listeners = {};
    FakeSource.all.push(this);
  }

  addEventListener(name, fn) { (this.listeners[name] ??= []).push(fn); }

  close() { this.closed = true; }

  // A closed source still dispatches, as a browser's can with an event already queued: what the
  // stream does with it is the stream's to guard.
  send(name, data) {
    const event = { data: data === undefined ? undefined : JSON.stringify(data) };
    for (const fn of this.listeners[name] ?? []) fn(event);
  }
}

const volume = (raw) => ({ key: 'volume', lo: 0, hi: 100, raw, text: `${raw}`, target: null });
const dac = (over = {}) => ({
  name: 'Gungnir 2', url: 'Gungnir%202', volume: volume(40), controls: [], meters: null, ...over,
});
const amp = () => ({
  name: 'Amp', url: 'Amp', volume: null, controls: [], meters: { live: true, meters: [] },
  tone: null,
});
const pageOf = (shown, device, over = {}) => ({
  v: 1, viewer: { guest: false, writes: true }, fleet: { devices: [], hops: [] }, shown, device,
  followed: {}, card: null, host: null, strip: null, ...over,
});

// A stream over a store, with a visibility watch the test drives and spies for the page's exits.
function rig({ page = pageOf('Gungnir%202', dac()), needs = ['meters', 'host'], session,
  visible = 'visible', read = () => ({}), sessionFetch } = {}) {
  FakeSource.all = [];
  timers = [];
  const store = createStore(page, {
    fetch: async (path) => ({ status: 200, json: async () => read(path) }),
  });
  const actions = createActions({
    store,
    post: async () => ({ status: 200 }),
    toast() {},
    history: { pushState() {} },
    location: { assign() {} },
  });
  const exits = { out: 0, reload: 0 };
  const classes = new Set();
  const lost = [];
  const doc = {
    visibilityState: visible,
    body: {
      classList: {
        toggle: (name, on) => { if (on) classes.add(name); else classes.delete(name); },
        contains: (name) => classes.has(name),
      },
    },
    dispatchEvent: (event) => lost.push(event.detail),
  };
  let watcher = null;
  const stream = createStream({
    store,
    needs,
    EventSource: FakeSource,
    fetch: sessionFetch ?? (async () => ({
      json: async () => session ?? { signed_in: true, guest: false },
    })),
    watchVisible: (options) => { watcher = options; return { stop() {}, observe() {} }; },
    signedOut: () => { exits.out++; },
    reload: () => { exits.reload++; },
    show: (url) => actions.showDevice(url, { push: false }),
    doc,
  });
  // A real watch says it is on show at once when the page is.
  if (visible === 'visible') watcher.onShow();
  return { store, stream, actions, exits, classes, lost, watcher: () => watcher, source: () => FakeSource.all.at(-1) };
}

await ok('the_url_says_the_device_its_needs_and_follows', () => {
  assert.equal(
    streamUrl({ device: 'Gungnir%202', needs: ['meters', 'host'], follow: ['Amp'] }),
    'api/v1/events?device=Gungnir%202&needs=meters,host&follow=Amp',
  );
  assert.equal(streamUrl({ device: 'A', needs: [], follow: ['B%201', 'C'] }),
    'api/v1/events?device=A&follow=B%201,C');
  assert.equal(streamUrl({ device: null, needs: ['host'], follow: [] }),
    'api/v1/events?needs=host');
  assert.equal(streamUrl({}), 'api/v1/events');
  const { stream, source } = rig();
  stream.follow(['Amp']);
  stream.open();
  assert.equal(source().url, 'api/v1/events?device=Gungnir%202&needs=meters,host&follow=Amp');
});

await ok('an_error_marks_lost_closes_and_reopens_after_2000_ms', async () => {
  assert.equal(REOPEN_MS, 2000);
  const { store, stream, source, classes, lost } = rig();
  stream.open();
  const first = source();
  first.send('open');
  assert.equal(store.state(), 'open');
  first.send('error');
  await flush();
  assert.equal(store.state(), 'lost');
  assert.equal(first.closed, true, 'never left to the browser\'s own retry');
  assert.equal(classes.has('lost'), true);
  assert.deepEqual(lost, [true]);
  advance(REOPEN_MS - 1);
  assert.equal(FakeSource.all.length, 1, 'not yet');
  advance(1);
  assert.equal(FakeSource.all.length, 2, 'the reopen');
  assert.equal(source().closed, false);
  source().send('open');
  assert.equal(store.state(), 'open');
  assert.equal(classes.has('lost'), false);
  assert.deepEqual(lost, [true, false]);
});

await ok('a_stream_that_opens_tells_the_store_even_when_the_link_was_not_lost', async () => {
  const { store, stream, source } = rig();
  const opened = [];
  store.on('opened', () => opened.push(source().url));
  stream.open();
  source().send('open');
  assert.equal(store.state(), 'open', 'nothing was lost');
  assert.equal(opened.length, 1, 'a theme forgets what it last read at every open');
});

await ok('a_session_that_ended_signs_out_and_one_turned_guest_reloads', async () => {
  const ended = rig({ session: { signed_in: false, guest: false } });
  ended.stream.open();
  ended.source().send('error');
  await flush();
  assert.deepEqual(ended.exits, { out: 1, reload: 0 });
  const turned = rig({ session: { signed_in: false, guest: true } });
  turned.stream.open();
  turned.source().send('error');
  await flush();
  assert.deepEqual(turned.exits, { out: 0, reload: 1 }, 'a signed-in page is now a guest\'s');
  const guest = rig({
    page: pageOf('Gungnir%202', dac(), { viewer: { guest: true, writes: false } }),
    session: { signed_in: false, guest: true },
  });
  guest.stream.open();
  guest.source().send('error');
  await flush();
  assert.deepEqual(guest.exits, { out: 0, reload: 0 }, 'a guest stays a guest');
  const fine = rig();
  fine.stream.open();
  fine.source().send('error');
  await flush();
  assert.deepEqual(fine.exits, { out: 0, reload: 0 });
});

await ok('a_page_hidden_past_the_grace_rests_and_waits_on_waking', () => {
  const { store, stream, source, watcher, classes } = rig();
  stream.open();
  const awake = source();
  awake.send('open');
  watcher().onHide();
  assert.equal(awake.closed, true);
  assert.equal(store.state(), 'resting');
  assert.equal(classes.has('lost'), false, 'a rest is not a lost link');
  watcher().onShow();
  const woken = source();
  assert.notEqual(woken, awake);
  assert.equal(store.state(), 'waiting');
  assert.equal(classes.has('lost'), true, 'dimmed as a lost link is');
  assert.equal(classes.has('waking'), true, 'but it says it is waking');
  woken.send('model', pageOf('Gungnir%202', dac()));
  assert.equal(store.state(), 'waiting', 'a screen with something to read waits for it');
  woken.send('volume', { device: 'Gungnir%202', raw: 41 });
  assert.equal(store.state(), 'open');
  assert.equal(classes.has('lost') || classes.has('waking'), false);
  assert.equal(store.part('volume').raw, 41);
});

await ok('the_wait_ends_by_itself_after_5000_ms', () => {
  assert.equal(WAIT_MS, 5000);
  const { store, stream, watcher } = rig();
  stream.open();
  watcher().onHide();
  watcher().onShow();
  assert.equal(store.state(), 'waiting');
  advance(WAIT_MS - 1);
  assert.equal(store.state(), 'waiting');
  advance(1);
  assert.equal(store.state(), 'open');
});

await ok('a_screen_with_nothing_to_read_is_ready_at_its_model', () => {
  const nothing = { ...dac(), volume: null, controls: [], meters: null };
  const { store, stream, source, watcher } = rig({ page: pageOf('Gungnir%202', nothing) });
  stream.open();
  watcher().onHide();
  watcher().onShow();
  assert.equal(store.state(), 'waiting');
  source().send('model', pageOf('Gungnir%202', nothing));
  assert.equal(store.state(), 'open');
  // An amp's meters count as something to read only for a page that asked for them.
  const meterless = rig({ page: pageOf('Amp', amp()), needs: ['host'] });
  meterless.stream.open();
  meterless.watcher().onHide();
  meterless.watcher().onShow();
  meterless.source().send('model', pageOf('Amp', amp()));
  assert.equal(meterless.store.state(), 'open');
  const metered = rig({ page: pageOf('Amp', amp()) });
  metered.stream.open();
  metered.watcher().onHide();
  metered.watcher().onShow();
  metered.source().send('model', pageOf('Amp', amp()));
  assert.equal(metered.store.state(), 'waiting');
  metered.source().send('meters', { device: 'Amp', live: true, meters: [] });
  assert.equal(metered.store.state(), 'open');
});

await ok('a_switch_while_resting_opens_the_new_device_on_wake', async () => {
  const { store, stream, actions, source, watcher } = rig({
    read: () => ({ device: amp(), strip: null }),
  });
  stream.open();
  watcher().onHide();
  const before = FakeSource.all.length;
  assert.equal((await actions.showDevice('Amp')).ok, true);
  assert.equal(store.shown(), 'Amp');
  assert.equal(FakeSource.all.length, before, 'a resting page opens nothing');
  watcher().onShow();
  assert.equal(FakeSource.all.length, before + 1, 'one stream on waking');
  assert.equal(source().url, 'api/v1/events?device=Amp&needs=meters,host');
  assert.equal(FakeSource.all.filter((s) => !s.closed).length, 1);
  assert.equal(store.state(), 'waiting');
  source().send('meters', { device: 'Amp', live: true, meters: [{ key: 'l' }] });
  assert.equal(store.state(), 'open', 'the wait ends on the new device\'s first reading');
});

await ok('a_switch_while_waking_waits_for_the_new_devices_first_reading', async () => {
  const { store, stream, actions, source, watcher } = rig({
    read: () => ({ device: amp(), strip: null }),
  });
  stream.open();
  watcher().onHide();
  watcher().onShow();
  const woken = source();
  advance(3000);
  assert.equal(store.state(), 'waiting');
  await actions.showDevice('Amp');
  assert.equal(woken.closed, true);
  assert.equal(source().url, 'api/v1/events?device=Amp&needs=meters,host');
  assert.equal(store.state(), 'waiting', 'the switch does not end the wait');
  advance(WAIT_MS - 1);
  assert.equal(store.state(), 'waiting', 'and the 5 s start again with the new stream');
  woken.send('volume', { device: 'Gungnir%202', raw: 50 });
  assert.equal(store.state(), 'waiting', 'a reading of the device left is not the new one\'s');
  source().send('meters', { device: 'Amp', live: true, meters: [{ key: 'l' }] });
  assert.equal(store.state(), 'open');
  // Following another device while waking keeps waiting too.
  stream.open();
  watcher().onHide();
  watcher().onShow();
  assert.equal(store.state(), 'waiting');
  const before = source();
  stream.follow(['Other']);
  assert.notEqual(source(), before);
  assert.equal(store.state(), 'waiting');
  advance(WAIT_MS);
  assert.equal(store.state(), 'open');
});

await ok('a_switch_while_lost_opens_on_the_new_device_at_the_reopen', async () => {
  const { store, stream, source } = rig();
  stream.open();
  const first = source();
  first.send('error');
  await flush();
  assert.equal(store.state(), 'lost');
  store.replace(pageOf('Amp', amp()));
  stream.follow(['Other']);
  assert.equal(FakeSource.all.length, 1, 'nothing opens before the reopen');
  advance(REOPEN_MS - 1);
  assert.equal(FakeSource.all.length, 1);
  advance(1);
  assert.equal(FakeSource.all.length, 2);
  assert.equal(source().url, 'api/v1/events?device=Amp&needs=meters,host&follow=Other');
  first.send('volume', { device: 'Gungnir%202', raw: 5 });
  assert.equal(store.device('Gungnir%202'), null, 'the stream that failed carries nothing more');
});

await ok('a_switch_on_a_live_stream_reopens_it_on_the_new_device_only', () => {
  const { store, stream, source } = rig();
  stream.open();
  const old = source();
  store.replace(pageOf('Gungnir%202', dac({ volume: volume(9) })));
  assert.equal(source(), old, 'the stream\'s own model is not a switch');
  store.replace(pageOf('Amp', amp()));
  assert.equal(old.closed, true);
  assert.equal(source().url, 'api/v1/events?device=Amp&needs=meters,host');
  old.send('volume', { device: 'Gungnir%202', raw: 99 });
  assert.equal(store.device('Gungnir%202'), null);
  assert.equal(store.part('volume'), null, 'no event of the old stream paints the new screen');
});

await ok('an_event_for_a_device_no_longer_followed_is_dropped', () => {
  const page = pageOf('Gungnir%202', dac(), { followed: { Amp: amp() } });
  const { store, stream, source } = rig({ page });
  stream.follow(['Amp']);
  stream.open();
  const first = source();
  first.send('part', { device: 'Amp', name: 'power', data: { on: true } });
  assert.deepEqual(store.part('power', 'Amp'), { on: true });
  const parts = [];
  store.on('part', (p) => parts.push(p));
  stream.follow([]);
  assert.deepEqual(stream.followed(), []);
  assert.equal(first.closed, true, 'a new address, a new stream');
  assert.equal(source().url, 'api/v1/events?device=Gungnir%202&needs=meters,host');
  first.send('part', { device: 'Amp', name: 'power', data: { on: false } });
  source().send('part', { device: 'Amp', name: 'power', data: { on: false } });
  source().send('meters', { device: 'Amp', live: false, meters: [] });
  assert.equal(parts.length, 0);
  assert.equal(store.part('power', 'Amp'), null);
});

await ok('a_device_the_page_holds_but_the_stream_does_not_follow_is_not_changed', () => {
  const page = pageOf('Gungnir%202', dac(), { followed: { Amp: amp() } });
  const { store, stream, source } = rig({ page });
  stream.open();
  const heard = [];
  for (const name of ['part', 'device', 'meters', 'volume']) store.on(name, (d) => heard.push(name));
  source().send('part', { device: 'Amp', name: 'meters', data: { live: false, meters: [1] } });
  source().send('device', { device: 'Amp', data: { ...amp(), meters: null } });
  source().send('meters', { device: 'Amp', live: false, meters: [1, 2] });
  assert.deepEqual(store.part('meters', 'Amp'), amp().meters);
  assert.deepEqual(heard, [], 'nobody was told');
});

await ok('a_fleet_that_no_longer_lists_a_followed_device_drops_it', () => {
  const page = pageOf('Gungnir%202', dac(), { followed: { Amp: amp() } });
  const { store, stream, source } = rig({ page });
  stream.follow(['Amp']);
  stream.open();
  const opened = FakeSource.all.length;
  source().send('fleet', { devices: [{ url: 'Gungnir%202' }, { url: 'Amp' }], hops: [] });
  assert.deepEqual(stream.followed(), ['Amp']);
  source().send('fleet', { devices: [{ url: 'Gungnir%202' }], hops: [] });
  assert.deepEqual(stream.followed(), []);
  assert.equal(store.device('Amp'), null);
  assert.equal(FakeSource.all.length, opened, 'the stream stays open');
  assert.equal(store.page().fleet.devices.length, 1);
});

await ok('a_fleet_that_no_longer_lists_the_shown_device_shows_the_first_it_lists_or_none', async () => {
  const fleet = { devices: [{ url: 'Gungnir%202' }, { url: 'Amp' }], hops: [] };
  const page = pageOf('Gungnir%202', dac(), { fleet });
  const { store, stream, source } = rig({ page, read: () => ({ device: amp(), strip: null }) });
  stream.open();
  source().send('fleet', { devices: [{ url: 'Gungnir%202' }, { url: 'Amp' }], hops: [] });
  assert.equal(store.shown(), 'Gungnir%202', 'a fleet that lists it leaves it shown');
  source().send('fleet', { devices: [{ url: 'Amp' }], hops: [] });
  await flush();
  assert.equal(store.shown(), 'Amp', 'the first listed is shown');
  assert.equal(store.device().name, 'Amp');
  source().send('fleet', { devices: [], hops: [] });
  assert.equal(store.shown(), null, 'none listed: the empty page');
  assert.equal(store.device(), null);
  assert.equal(store.page().fleet.devices.length, 0);
});

await ok('a_followed_device_sends_its_shape_and_readings_tagged', () => {
  const page = pageOf('Gungnir%202', dac(), { followed: { Amp: amp() } });
  const { store, stream, source } = rig({ page });
  stream.follow(['Amp']);
  stream.open();
  source().send('device', { device: 'Amp', data: { ...amp(), meters: null } });
  assert.equal(store.part('meters', 'Amp'), null);
  source().send('meters', { device: 'Amp', live: true, meters: [{ key: 'r', fraction: 0.5 }] });
  assert.equal(store.part('meters', 'Amp').meters[0].key, 'r');
  source().send('volume', { device: 'Gungnir%202', raw: 60 });
  assert.equal(store.part('volume').raw, 60);
  source().send('host', { track: { headline: 'x' } });
  assert.equal(store.page().host.track.headline, 'x');
  source().send('strip', { device: 'Gungnir%202', bars: [] });
});

await ok('closing_ends_the_stream_its_reopening_and_what_it_still_asked', async () => {
  const { stream, source } = rig();
  stream.open();
  source().send('error');
  await flush();
  stream.close();
  advance(REOPEN_MS * 2);
  assert.equal(FakeSource.all.filter((s) => !s.closed).length, 0);
  // A session answer that arrives after the close changes nothing, and neither does a switch.
  let answer;
  const late = rig({
    sessionFetch: () => new Promise((resolve) => { answer = resolve; }),
  });
  late.stream.open();
  late.source().send('error');
  late.stream.close();
  answer({ json: async () => ({ signed_in: false, guest: false }) });
  await flush();
  assert.deepEqual(late.exits, { out: 0, reload: 0 });
  late.store.replace(pageOf('Amp', amp()));
  assert.equal(FakeSource.all.length, 1, 'a closed stream does not follow a switch');
});

console.log(`${n} core-stream checks passed`);
