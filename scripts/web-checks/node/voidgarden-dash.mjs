/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for Void Garden's dash, rows, prefs, keys and states (themes/voidgarden/
// dash-wire.js, draw/dash.js, draw/card.js, states.js, voidgarden.js menuRows, key-table.js,
// debug.js): the `−` `MUTE` `+` order and the figure (muted reads `−∞ dB`), the core's repeat of a
// held key (380 ms, then every 70 ms, at most 10 s) for the pointer and for `↑` `↓`, the fade
// (opacity only, wakes, trouble and the focus keep it up, a first tap only wakes), the page's
// states and the card, the menu's rows with their prefs across a reload and without storage,
// reduced motion starting on Calm, `A` cycling the panel, the bisect switches, and the driver's
// delegation of a target the dash does not draw. The real wire over a jsdom page and a fake core
// with a clock moved by hand. A jsdom page (apt `node-jsdom`). Run: node voidgarden-dash.mjs
import assert from 'node:assert/strict';
import { page as domPage } from './dom.mjs';
import * as freeze from '../../../bins/svolderd/web/core/freeze.js';
import { wireDebug } from '../../../bins/svolderd/web/themes/voidgarden/debug-wire.js';
import { readFileSync } from 'node:fs';
import { createKeys } from '../../../bins/svolderd/web/core/keys.js';
import { createPrefs } from '../../../bins/svolderd/web/core/prefs.js';
import { repeat } from '../../../bins/svolderd/web/core/repeat.js';
import { KEYS } from '../../../bins/svolderd/web/themes/voidgarden/key-table.js';
import { drawCard } from '../../../bins/svolderd/web/themes/voidgarden/draw/card.js';
import { drawDash, volumeWords } from '../../../bins/svolderd/web/themes/voidgarden/draw/dash.js';
import { wireDash } from '../../../bins/svolderd/web/themes/voidgarden/dash-wire.js';
import { debugOf, readoutLines, withSwitches } from '../../../bins/svolderd/web/themes/voidgarden/debug.js';
import { motionOf } from '../../../bins/svolderd/web/themes/voidgarden/motion.js';
import {
  CHOICES, createDashFade, fadeAfter, moodOf, nextPanel, panelFades, quietOf, withMemory,
} from '../../../bins/svolderd/web/themes/voidgarden/states.js';
import { menuRows, theme, wireVolumeKeys } from '../../../bins/svolderd/web/themes/voidgarden/voidgarden.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

// A clock the test moves by hand: timers and Date.now are the fake's.
let now = 0;
let timers = [];
let next = 1;
globalThis.setTimeout = (fn, ms) => { timers.push({ id: next, at: now + ms, fn, every: 0 }); return next++; };
globalThis.setInterval = (fn, ms) => { timers.push({ id: next, at: now + ms, fn, every: ms }); return next++; };
globalThis.clearTimeout = (id) => { timers = timers.filter((t) => t.id !== id); };
globalThis.clearInterval = globalThis.clearTimeout;
Date.now = () => now;
const advance = (ms) => {
  const to = now + ms;
  for (;;) {
    const due = timers.filter((t) => t.at <= to).sort((a, b) => a.at - b.at || a.id - b.id)[0];
    if (!due) break;
    now = due.at;
    if (due.every) due.at += due.every;
    else timers = timers.filter((t) => t !== due);
    due.fn();
  }
  now = to;
};
globalThis.CSS = { escape: (text) => String(text).replace(/[^\w-]/g, '\\$&') };

const HTML = '<div id="voidgarden"><canvas id="vg-scene"></canvas><div id="vg-hud"></div><div id="vg-panel"></div>'
  + '<div id="vg-dash"></div><div id="vg-card" hidden></div><span id="vg-slot"></span></div>';

const volume = (more = {}) => ({
  key: 'volume', mute: 'mute', text: '−23.5 dB', unit: 'dB', muted: false, raw: 40, lo: 0, hi: 300, cap: null, ...more,
});
const input = (on = 'USB') => ({
  key: 'input', kind: 'choice', label: 'Input',
  options: ['USB', 'Optical', 'Coax'].map((label) => ({ label, on: label === on, json: JSON.stringify(label) })),
});
const device = (more = {}) => ({
  name: 'D', url: 'D', family: 'dac', inert: false, card: null, volume: volume(), controls: [input()],
  playing: null, ...more,
});
const player = (status = 'playing') => ({
  now_ms: 1, buttons: true, track: { status, headline: 'Song', artist: 'Band', album: 'Disc', cover: 'abc' },
});
const pageOf = (dev, more = {}) => ({
  v: 1, viewer: { signed_in: true, guest: false, writes: true }, shown: 'D', device: dev, host: dev?.playing ?? null, ...more,
});

/** A mounted dash over a fake core; `did` notes what was asked of the core. */
function stage({
  page: first = pageOf(device({ playing: player() })), side = 'device', prefs = null, phone = false,
} = {}) {
  const p = domPage(HTML);
  const { window, document } = p;
  window.matchMedia = () => ({ matches: phone, addEventListener() {}, removeEventListener() {} });
  globalThis.window = window;
  globalThis.document = document;
  globalThis.AbortController = window.AbortController;
  Object.defineProperty(document, 'hidden', { value: false, configurable: true });
  const did = [];
  const state = {
    page: first, side, stream: 'open', covered: false, view: null, popup: null,
  };
  const listeners = new Map();
  const on = (event, fn) => {
    if (!listeners.has(event)) listeners.set(event, new Set());
    listeners.get(event).add(fn);
    return () => listeners.get(event).delete(fn);
  };
  const emit = (event, data = {}) => [...(listeners.get(event) ?? [])].forEach((fn) => fn(data));
  const volumeOfDevice = () => state.page.device?.volume;
  const watchers = [];
  let opened = 0;
  let closed = 0;
  const held = [];
  const fades = [];
  const core = {
    model: {
      page: () => state.page,
      device: () => state.page.device,
      shown: () => 'D',
      part: (name) => (name === 'volume' ? volumeOfDevice() : null),
      state: () => state.stream,
      on,
    },
    prefs,
    computer: {
      side: () => state.side,
      view: () => state.view,
      on: (fn) => on('computer', fn),
      volumeOf: () => ({ pct: 42, muted: false, inert: false }),
      flip: (to) => did.push(['flip', to]),
    },
    actions: {
      canWrite: () => state.page.viewer.writes === true,
      setVolume: async (value, options) => {
        did.push(['setVolume', value, options.device]);
        if (volumeOfDevice()) Object.assign(volumeOfDevice(), { raw: value, text: `−${100 - value} dB` });
        emit('volume', { device: 'D', raw: value });
        return { ok: true };
      },
      setControl: async (key, value) => { did.push(['setControl', key, value]); return { ok: true }; },
      press: async (button) => { did.push(['press', button]); return { ok: true }; },
      connect: async (options) => { did.push(['connect', options.device]); return { ok: true }; },
      computer: {
        step: async (delta) => { did.push(['computer.step', delta]); },
        setMute: async (on) => { did.push(['computer.setMute', on]); return { ok: true }; },
      },
    },
    freeze,
    repeat,
    lifecycle: { watchVisible: (options) => { watchers.push(options); return { stop() {} }; } },
    analysis: { open: (l) => { opened++; held.push(l); return { close() { closed++; } }; } },
    frame: { covered: () => state.covered, fade: (on, ms) => { fades.push([on, ms]); } },
    router: {
      open: (name) => did.push(['open', name]),
      state: () => ({ popup: state.popup }),
      close: () => { state.popup = null; },
    },
    driver: {
      controls: {
        reach: async (t) => { did.push(['controls.reach', t]); return true; },
        operate: async (t, h) => { did.push(['controls.operate', t, h]); return true; },
        shown: (t) => { did.push(['controls.shown', t]); return 'words'; },
      },
    },
  };
  const control = new window.AbortController();
  const vg = {
    page: document.getElementById('voidgarden'), core, signal: control.signal, prefs, steps: new EventTarget(),
  };
  const wired = wireDash(vg, { now: () => now });
  return {
    ...p, did, fades, state, core, vg, wired, control, emit, watchers, held, analysis: () => ({ opened, closed }),
    dash: document.getElementById('vg-dash'),
    card: document.getElementById('vg-card'),
    stop: () => control.abort(),
    redraw: () => emit('model'),
  };
}

/** The page's volume keys over a staged dash, as the mount makes them. */
const keysOf = (s) => wireVolumeKeys({
  doc: s.document, nudge: s.wired.nudge, writable: s.wired.writable, covered: () => s.core.frame.covered(), now: () => now, signal: s.vg.signal,
});

/** Whether the device's volume key is out of reach. */
const keysInert = (s) => s.dash.querySelector('#vg-vup').inert;

/** Wait for `promise` while the clock moves on: what a driver call waits on is the fake's timers. */
async function drive(promise) {
  for (let i = 0; i < 8; i++) {
    await new Promise((resolve) => { setImmediate(resolve); });
    advance(50);
  }
  return promise;
}

const pointer = (win, type, target, extra = {}) => {
  const event = new win.MouseEvent(type, { bubbles: true, cancelable: true, button: 0 });
  for (const [key, value] of Object.entries({
    pointerId: 7, isPrimary: true, pointerType: 'mouse', buttons: 1, ...extra,
  })) Object.defineProperty(event, key, { value });
  target.dispatchEvent(event);
  return event;
};

// ---- the markup ----
await ok('the_keys_stand_minus_the_figure_plus_then_mute_and_a_muted_device_reads_minus_infinity_db', () => {
  const html = drawDash(pageOf(device()), null);
  const order = [...html.matchAll(/id="(vg-vdown|vg-mute|vg-vup|vg-vol)"/g)].map((m) => m[1]);
  assert.deepEqual(order, ['vg-vdown', 'vg-vol', 'vg-vup', 'vg-mute']);
  assert.match(html, /id="vg-vol"[^>]*>−23\.5 dB</);
  const muted = drawDash(pageOf(device({ volume: volume({ muted: true }) })), null);
  assert.match(muted, /id="vg-vol"[^>]*aria-label="muted, minus infinity dB"[^>]*>−∞ dB</);
  assert.match(muted, /id="vg-mute"[^>]*aria-pressed="true"/);
});

await ok('the_dash_is_two_capsules_with_the_input_as_one_chip_the_transport_and_the_explore_group', () => {
  const html = drawDash(pageOf(device({ playing: player() })), null);
  assert.equal([...html.matchAll(/class="vg-cap /g)].length, 2, 'two capsules');
  assert.equal([...html.matchAll(/data-act="input"/g)].length, 1, 'the input is one chip');
  assert.match(html, /data-list="1"[^>]*>USB ▾</);
  assert.deepEqual([...html.matchAll(/data-button="(\w+)"/g)].map((m) => m[1]), ['previous', 'pause', 'next']);
  assert.doesNotMatch(html, /vg-controls/, 'Controls… is the menu\'s');
  assert.match(html, /vg-cover/);
  const bare = drawDash(pageOf(device({ volume: null, controls: [], playing: null })), null);
  assert.doesNotMatch(bare, /vg-vdown|data-act="input"|data-button/);
  assert.match(bare, /data-act="explore"/, 'the explore keys need no device');
  const long = drawDash(pageOf(device({
    controls: [{ ...input(), options: ['A', 'B', 'C', 'D'].map((label) => ({ label, on: label === 'B', json: '"x"' })) }],
  })), null);
  assert.match(long, /data-list="1"[^>]*>B ▾</);
});

await ok('the_computers_side_reads_its_percent_and_a_guests_keys_are_drawn_inert', () => {
  const comp = { side: 'computer', sw: { side: 'computer', dac: 'D', computer: 'This computer' }, volume: { pct: 42, text: '42', unit: '%', muted: false, aria: 'Volume 42 %', inert: false } };
  assert.equal(volumeWords(pageOf(device()), comp).text, '42 %');
  const html = drawDash(pageOf(device()), comp);
  assert.match(html, /id="vg-side-computer"[^>]*aria-pressed="true"/);
  assert.match(html, /id="vg-vol"[^>]*>42 %</);
  const guest = drawDash(pageOf(device(), { viewer: { guest: true, writes: false } }), null);
  assert.match(guest, /id="vg-vup"[^>]*aria-disabled="true"/);
});

await ok('the_card_escapes_the_words_and_says_try_now_and_the_seconds', () => {
  const html = drawCard({ title: 'D <b>', detail: 'x & y', try_now: true, retry_in: 5 }, { now: 1000 });
  assert.match(html, /D &lt;b&gt;/);
  assert.match(html, /x &amp; y/);
  assert.match(html, /id="vg-try"/);
  assert.match(html, /data-secs="5" data-drawn="1000">next try in 5 s</);
  assert.match(drawCard({ title: 't', try_now: true }, { quiet: true }), /vg-k cy quiet" data-act="try" aria-disabled="true"/);
  assert.equal(drawCard(null), '');
});

// ---- the states ----
await ok('the_states_name_their_words_and_the_lost_stream_outranks_a_card', () => {
  const card = { kind: 'link', title: 'x' };
  assert.equal(moodOf(pageOf(device()), 'open', null), 'ok');
  assert.equal(moodOf(pageOf(device({ card })), 'open', null), 'unreachable');
  assert.equal(moodOf(pageOf(device({ card })), 'lost', null), 'lost');
  assert.equal(moodOf(pageOf(device({ card: { kind: 'fault' } })), 'open', null), 'ok', 'a fault stays live');
  assert.equal(moodOf(pageOf(device()), 'waiting', null), 'wait');
  assert.equal(moodOf(pageOf(device()), 'open', { state: 'unavailable', why: 'no sink' }), 'nocap');
  assert.equal(quietOf('lost'), 'Reconnecting to svolder…');
  assert.equal(quietOf('wait', 'waiting'), 'reading the device again…');
  assert.match(quietOf('wait', 'resting'), /^Paused while the page is hidden/);
  assert.equal(quietOf('nocap', 'open', { why: 'no sink' }), 'no music analysis: no sink');
  assert.equal(quietOf('ok'), '');
});

await ok('the_wire_draws_the_mood_the_line_and_the_card_and_puts_the_dash_out_of_reach', () => {
  const s = stage();
  const root = s.vg.page;
  assert.equal(root.dataset.mood, 'ok');
  assert.equal(s.card.hidden, true);
  assert.equal(keysInert(s), false);
  s.state.stream = 'lost';
  s.emit('state', 'lost');
  assert.equal(root.dataset.mood, 'lost');
  assert.equal(s.card.querySelector('.vg-line').textContent, 'Reconnecting to svolder…');
  assert.equal(keysInert(s), true);
  s.state.stream = 'open';
  s.state.page = pageOf(device({ card: { kind: 'link', title: 'Out of reach', detail: 'gone', try_now: true, retry_in: 9 } }));
  s.emit('state', 'open');
  assert.equal(root.dataset.mood, 'unreachable');
  assert.equal(s.card.hidden, false);
  assert.equal(s.card.querySelector('h2').textContent, 'Out of reach');
  assert.equal(keysInert(s), true);
  assert.equal(s.wired.driver.state().lost, true, 'a card counts as lost for the checks');
  assert.equal(s.dash.querySelector('#vg-ex-seed').inert, false, 'the explore keys stay reachable');
  assert.equal(Boolean(s.dash.inert), false, 'the dash is not one inert box');
  s.card.querySelector('[data-act="try"]').click();
  assert.deepEqual(s.did.at(-1), ['connect', 'D']);
  advance(3000);
  assert.match(s.card.querySelector('.vg-retry').textContent, /next try in 6 s/, 'the seconds count down');
  s.state.page = pageOf(device());
  s.emit('state', 'open');
  assert.equal(root.dataset.mood, 'ok');
  assert.equal(s.card.hidden, true);
  assert.equal(keysInert(s), false);
  s.stop();
});

await ok('the_dash_holds_no_analysis_and_hears_the_scenes_capture_and_release', () => {
  const s = stage();
  s.watchers.forEach((w) => w.onShow?.());
  assert.equal(s.analysis().opened, 0, 'the scene is the page\'s one holder: the dash holds none');
  const tell = (name, detail) => s.vg.steps.dispatchEvent(new globalThis.CustomEvent(name, { detail }));
  tell('capture', { capture: 'unavailable', why: 'no sink' });
  assert.equal(s.vg.page.dataset.mood, 'nocap');
  assert.equal(s.card.querySelector('.vg-line').textContent, 'no music analysis: no sink');
  assert.equal(s.card.hidden, false);
  assert.equal(keysInert(s), false, 'the dash stays live');
  tell('capture', { capture: 'running' });
  assert.equal(s.vg.page.dataset.mood, 'ok');
  assert.equal(s.card.hidden, true);
  tell('capture', { capture: 'unavailable', why: 'no sink' });
  s.vg.steps.dispatchEvent(new globalThis.Event('release'));
  assert.equal(s.vg.page.dataset.mood, 'ok', 'let go: the words are gone');
  s.stop();
});

// ---- the volume and the core's repeat ----
await ok('a_press_of_the_pointer_on_plus_steps_once_and_a_held_one_repeats_after_380_ms_every_70_ms', () => {
  const s = stage();
  const plus = s.dash.querySelector('#vg-vup');
  pointer(s.window, 'pointerdown', plus);
  assert.equal(s.did.filter((d) => d[0] === 'setVolume').length, 1, 'one step at the press');
  advance(379);
  assert.equal(s.did.filter((d) => d[0] === 'setVolume').length, 1);
  advance(1 + 70);
  assert.equal(s.did.filter((d) => d[0] === 'setVolume').length, 2, 'the first repeat 70 ms after the pause');
  advance(70 * 4);
  assert.equal(s.did.filter((d) => d[0] === 'setVolume').length, 6);
  pointer(s.window, 'pointerup', s.window.document);
  advance(1000);
  assert.equal(s.did.filter((d) => d[0] === 'setVolume').length, 6, 'the release ends it');
  assert.deepEqual(s.did.filter((d) => d[0] === 'setVolume').map((d) => d[1]), [41, 42, 43, 44, 45, 46]);
  s.stop();
});

await ok('a_held_key_stops_at_the_cores_cap_of_10_s', () => {
  const s = stage();
  const plus = s.dash.querySelector('#vg-vup');
  pointer(s.window, 'pointerdown', plus);
  advance(60_000);
  const sent = s.did.filter((d) => d[0] === 'setVolume').length;
  // one at the press, then 70 ms steps from 450 ms to just past 10 s after the pause
  assert.equal(sent, 1 + Math.floor(10_000 / 70), 'the repeat ran for the cap and no longer');
  pointer(s.window, 'pointerup', s.window.document);
  s.stop();
});

await ok('arrow_up_steps_once_and_repeats_at_the_same_pace_and_up_to_the_same_cap', () => {
  const s = stage();
  const keys = keysOf(s);
  const event = new s.window.KeyboardEvent('keydown', { key: 'ArrowUp', cancelable: true });
  keys.key(event);
  assert.equal(event.defaultPrevented, true);
  advance(379);
  assert.equal(s.did.length, 1);
  advance(1 + 70 * 3);
  assert.equal(s.did.length, 4, 'steps every 70 ms after the 380 ms pause');
  keys.key(new s.window.KeyboardEvent('keydown', { key: 'ArrowUp', repeat: true, cancelable: true }));
  assert.equal(s.did.length, 4, 'the system\'s own repeat is not a press');
  s.window.dispatchEvent(new s.window.KeyboardEvent('keyup', { key: 'ArrowUp' }));
  advance(1000);
  assert.equal(s.did.length, 4, 'the key up ends it');
  keys.key(new s.window.KeyboardEvent('keydown', { key: 'ArrowUp', cancelable: true }));
  advance(60_000);
  assert.equal(s.did.length - 4, 1 + Math.floor(10_000 / 70), 'a key held past 10 s stops');
  s.stop();
});

await ok('mute_steps_the_computer_and_the_computers_mute_when_it_is_the_side_shown', () => {
  const s = stage();
  s.dash.querySelector('#vg-mute').click();
  assert.deepEqual(s.did.at(-1), ['setControl', 'mute', { value: true }]);
  s.dash.querySelector('#vg-vup').click();
  assert.deepEqual(s.did.at(-1).slice(0, 2), ['setVolume', 41], 'a click with no pointer is one key step');
  s.state.side = 'computer';
  s.state.view = { side: 'computer', sw: { side: 'computer', dac: 'D', computer: 'This computer' }, volume: { pct: 42, text: '42', unit: '%', muted: false, aria: 'v', inert: false } };
  s.redraw();
  s.dash.querySelector('#vg-vdown').click();
  assert.deepEqual(s.did.at(-1), ['computer.step', -1]);
  s.dash.querySelector('#vg-mute').click();
  assert.deepEqual(s.did.at(-1), ['computer.setMute', true]);
  s.dash.querySelector('#vg-side-dac').click();
  assert.deepEqual(s.did.at(-1), ['flip', 'dac']);
  s.stop();
});

await ok('the_input_list_transport_and_explore_keys_go_to_the_core_and_the_scene', () => {
  const s = stage();
  s.dash.querySelector('[data-act="input"]').click();
  s.document.querySelector('.fi-list .fi-opt[data-value="Optical"]').click();
  assert.deepEqual(s.did.at(-1), ['setControl', 'input', 'Optical']);
  s.dash.querySelector('[data-button="pause"]').click();
  assert.deepEqual(s.did.at(-1), ['press', 'pause']);
  const asked = [];
  s.vg.steps.addEventListener('explore', (e) => asked.push(e.detail.kind));
  for (const kind of ['seed', 'scene', 'effect']) s.dash.querySelector(`#vg-ex-${kind}`).click();
  assert.deepEqual(asked, ['seed', 'scene', 'effect'], 'the explore keys tell the scene');
  s.stop();
});

await ok('a_viewer_who_may_not_write_taps_the_same_keys_and_the_core_is_asked_so_it_can_say_why', () => {
  const s = stage({ page: pageOf(device(), { viewer: { guest: false, writes: false } }) });
  s.dash.querySelector('#vg-vup').click();
  assert.deepEqual(s.did.at(-1).slice(0, 2), ['setVolume', 0], 'the core turns it away with its words');
  s.stop();
});

await ok('up_and_down_do_nothing_while_a_control_has_the_focus_or_the_page_is_covered', () => {
  const s = stage();
  const keys = keysOf(s);
  s.dash.querySelector('#vg-mute').focus();
  const event = new s.window.KeyboardEvent('keydown', { key: 'ArrowUp', cancelable: true });
  keys.key(event);
  assert.equal(event.defaultPrevented, false);
  s.document.activeElement.blur();
  s.state.covered = true;
  keys.key(new s.window.KeyboardEvent('keydown', { key: 'ArrowDown', cancelable: true }));
  assert.equal(s.did.length, 0);
  s.stop();
});

// ---- the fade ----
function fadeRig({ after = 3, kept = () => false } = {}) {
  const p = domPage('<div id="root"></div>');
  const root = p.document.getElementById('root');
  const fade = createDashFade({ after, root, doc: p.document, now: () => now, kept });
  return { ...p, root, fade };
}

await ok('the_dash_fades_after_3_s_with_no_input_by_a_class_alone_and_any_input_wakes_it', () => {
  const r = fadeRig();
  advance(2_999);
  assert.equal(r.fade.faded(), false);
  advance(1);
  assert.equal(r.fade.faded(), true);
  assert.equal(r.root.className, 'vg-dim', 'only a class: the markup is untouched');
  r.document.dispatchEvent(new r.window.Event('pointermove', { bubbles: true }));
  assert.equal(r.fade.faded(), false);
  assert.equal(r.root.className, '');
  advance(2_999);
  assert.equal(r.fade.faded(), false, 'the idle time counts from the input');
  advance(1);
  assert.equal(r.fade.faded(), true);
  r.fade.stop();
  // an input part-way through moves the fade out: the tick already planned finds the dash not idle
  const q = fadeRig();
  advance(2_000);
  q.document.dispatchEvent(new q.window.Event('keydown', { bubbles: true }));
  advance(1_000);
  assert.equal(q.fade.faded(), false, 'the planned tick sees an input 1 s ago');
  advance(2_000);
  assert.equal(q.fade.faded(), true);
  q.fade.stop();
});

await ok('the_focus_a_hold_and_trouble_keep_it_up_and_trouble_while_faded_brings_it_back', () => {
  let kept = true;
  const r = fadeRig({ kept: () => kept });
  advance(40_000);
  assert.equal(r.fade.faded(), false, 'kept: it stays');
  kept = false;
  advance(500);
  assert.equal(r.fade.faded(), true, 'let go: it fades at the next look');
  r.fade.hold(true);
  assert.equal(r.fade.faded(), false, 'trouble shows it');
  advance(60_000);
  assert.equal(r.fade.faded(), false, 'trouble keeps it');
  r.fade.hold(false);
  advance(3_000);
  assert.equal(r.fade.faded(), true);
  r.fade.stop();
});

await ok('the_press_that_wakes_a_faded_dash_only_wakes_it_and_its_click_goes_nowhere', () => {
  const s = stage();
  const plus = s.dash.querySelector('#vg-vup');
  advance(3_000);
  assert.equal(s.vg.page.classList.contains('vg-dim'), true);
  pointer(s.window, 'pointerdown', plus);
  assert.equal(s.vg.page.classList.contains('vg-dim'), false, 'it woke');
  assert.equal(s.did.length, 0, 'the first tap only woke it');
  pointer(s.window, 'pointerup', plus);
  plus.dispatchEvent(new s.window.MouseEvent('click', { bubbles: true, cancelable: true, detail: 1 }));
  assert.equal(s.did.length, 0, 'and neither did its click');
  advance(200);
  pointer(s.window, 'pointerdown', plus);
  assert.equal(s.did.filter((d) => d[0] === 'setVolume').length, 1, 'the next tap works');
  pointer(s.window, 'pointerup', s.window.document);
  s.stop();
});

await ok('trouble_holds_the_dash_up_and_it_fades_again_once_the_page_is_well', () => {
  const s = stage();
  s.state.stream = 'lost';
  s.emit('state', 'lost');
  advance(60_000);
  assert.equal(s.vg.page.classList.contains('vg-dim'), false);
  s.state.stream = 'open';
  s.emit('state', 'open');
  advance(3_000);
  assert.equal(s.vg.page.classList.contains('vg-dim'), true);
  s.stop();
});

await ok('the_fade_time_is_a_pref_of_1_to_600_s_and_3_s_or_4_on_a_phone_when_it_is_not_a_number', () => {
  const at = (value, phone = false) => fadeAfter({ get: () => value }, phone);
  assert.equal(at(undefined), 3);
  assert.equal(at('x'), 3);
  assert.equal(at('x', true), 4);
  assert.equal(at(0), 1);
  assert.equal(at(9000), 600);
  assert.equal(at(30), 30);
});

// ---- the rows and the prefs ----
/** A browser's storage that works, or one that refuses. */
const store = (refuse = false) => {
  const map = new Map();
  return {
    getItem: (k) => { if (refuse) throw new Error('no'); return map.get(k) ?? null; },
    setItem: (k, v) => { if (refuse) throw new Error('no'); map.set(k, v); },
    removeItem: (k) => map.delete(k),
  };
};
const rowsOf = (prefs, { reduced = false, panel = null } = {}) => {
  const set = new AbortController();
  const calls = [];
  const fake = panel ?? {
    mode: () => (prefs.get('panel', 'panel')),
    set(mode) { calls.push(['setMode', mode]); prefs.set('panel', mode); },
    fades: () => panelFades(prefs),
    setFades(on) { calls.push(['setFades', on]); prefs.set('panel-fades', on); },
  };
  return { rows: menuRows({ prefs, reduced: () => reduced, panel: fake, signal: set.signal }), calls, set };
};
const row = (rows, id) => rows.find((r) => r.id === id);
const checked = (rows) => rows.filter((r) => (r.kind === 'radio' || r.kind === 'check') && r.checked()).map((r) => r.id);

await ok('the_menu_lists_motion_audio_panel_pin_detail_renderer_and_device_controls_in_that_order', () => {
  const { rows } = rowsOf(withMemory(createPrefs('voidgarden', store())));
  assert.deepEqual(rows.filter((r) => r.kind === 'heading').map((r) => r.text),
    ['Motion', 'Audio panel', 'Detail', 'Renderer']);
  assert.deepEqual(rows.filter((r) => r.kind === 'action').map((r) => r.label), ['Device controls…'],
    'Controls… moved to the menu');
  const opened = [];
  menuRows({ prefs: withMemory(null), panel: { mode: () => 'panel', fades: () => true }, signal: new AbortController().signal, controls: () => opened.push('controls') })
    .find((r) => r.kind === 'action').run();
  assert.deepEqual(opened, ['controls']);
  assert.deepEqual(rows.filter((r) => r.kind === 'radio' || r.kind === 'check').map((r) => r.label),
    ['Full', 'Calm', 'Still', 'Panel', 'Spread', 'Off', 'Pin features panel', 'Auto', 'High', 'Low', 'Auto', 'WebGPU', 'WebGL2']);
  assert.deepEqual(checked(rows), ['vg-motion-full', 'vg-panel-panel', 'vg-detail-auto', 'vg-renderer-auto'],
    'the defaults');
});

await ok('each_row_keeps_its_choice_across_a_reload_through_the_browsers_storage', () => {
  const storage = store();
  const first = withMemory(createPrefs('voidgarden', storage));
  const a = rowsOf(first);
  for (const id of ['vg-motion-still', 'vg-panel-spread', 'vg-detail-low', 'vg-renderer-webgl2']) row(a.rows, id).choose();
  row(a.rows, 'vg-panel-pin').flip();
  assert.deepEqual(a.calls, [['setMode', 'spread'], ['setFades', false]]);
  const reload = withMemory(createPrefs('voidgarden', storage));
  const b = rowsOf(reload);
  assert.deepEqual(checked(b.rows), ['vg-motion-still', 'vg-panel-spread', 'vg-panel-pin', 'vg-detail-low', 'vg-renderer-webgl2']);
  assert.equal(panelFades(reload), false);
  assert.equal(storage.getItem('svolder.theme.voidgarden.motion'), '"still"');
});

await ok('a_browser_that_keeps_nothing_still_shows_and_uses_the_choice_for_the_session', () => {
  const prefs = withMemory(createPrefs('voidgarden', store(true)));
  const { rows } = rowsOf(prefs);
  row(rows, 'vg-motion-calm').choose();
  row(rows, 'vg-detail-high').choose();
  row(rows, 'vg-renderer-webgl2').choose();
  assert.deepEqual(checked(rows).filter((id) => /motion|detail|renderer/.test(id)),
    ['vg-motion-calm', 'vg-detail-high', 'vg-renderer-webgl2']);
  assert.equal(prefs.get('detail', 'auto'), 'high', 'what the scene reads');
  // and a storage that is not there at all
  const none = withMemory(null);
  none.set('motion', 'still');
  assert.equal(motionOf(none, false), 'still');
});

await ok('reduced_motion_starts_on_calm_and_a_choice_of_full_is_kept', () => {
  const prefs = withMemory(createPrefs('voidgarden', store()));
  const { rows } = rowsOf(prefs, { reduced: true });
  assert.deepEqual(checked(rows)[0], 'vg-motion-calm');
  row(rows, 'vg-motion-full').choose();
  assert.deepEqual(checked(rows)[0], 'vg-motion-full');
  assert.equal(motionOf(prefs, true), 'full');
});

await ok('a_row_does_nothing_once_the_mount_has_gone', () => {
  const prefs = withMemory(createPrefs('voidgarden', store()));
  const r = rowsOf(prefs);
  r.set.abort();
  row(r.rows, 'vg-motion-still').choose();
  row(r.rows, 'vg-panel-pin').flip();
  assert.equal(prefs.get('motion', 'full'), 'full');
  assert.deepEqual(r.calls, []);
  assert.deepEqual(checked(r.rows), []);
});

await ok('a_value_a_pref_does_not_name_is_the_default', () => {
  const prefs = withMemory(createPrefs('voidgarden', store()));
  prefs.set('detail', 'ultra');
  prefs.set('renderer', 3);
  const { rows } = rowsOf(prefs);
  assert.deepEqual(checked(rows).filter((id) => /detail|renderer/.test(id)), ['vg-detail-auto', 'vg-renderer-auto']);
});

// ---- A cycles the panel ----
await ok('the_panel_cycles_panel_spread_off_panel_and_the_key_is_one_a_reader_has_too', () => {
  assert.equal(nextPanel('panel'), 'spread');
  assert.equal(nextPanel('spread'), 'off');
  assert.equal(nextPanel('off'), 'panel');
  assert.deepEqual(CHOICES.panel, ['panel', 'spread', 'off']);
  const key = KEYS.find((e) => e.id === 'panel');
  assert.deepEqual([key.key, key.keys, key.zone, key.words], ['A', ['A', 'a'], 'page', 'Cycle the audio panel']);
  assert.equal(key.when, undefined, 'it changes the view, not a device: no write is needed');
});

await ok('the_mount_binds_a_and_each_press_moves_the_panel_one_mode_and_keeps_it', () => {
  const p = domPage('<div id="root"></div>');
  const { window, document } = p;
  globalThis.window = window;
  globalThis.document = document;
  globalThis.AbortController = window.AbortController;
  const storage = store(true);
  const did = [];
  const strict = (core) => core;
  const core = strict({
    prefs: createPrefs('voidgarden', storage),
    computer: { side: () => 'device', view: () => null, on: () => () => {} },
    model: {
      part: () => null, shown: () => 'D', page: () => ({}), state: () => 'open', on: () => () => {}, device: () => null,
    },
    actions: { canWrite: () => true },
    freeze,
    lifecycle: { canvasLoop: () => ({ start() {}, stop() {} }), watchVisible: () => ({ stop() {} }) },
    analysis: { open: () => ({ close() {} }) },
    frame: { anchor: (el) => did.push(el?.id ?? null), covered: () => false, menuOwns: () => false },
    keys: createKeys({ document, base: () => ({ canWrite: false }) }),
  });
  const mounted = theme.mount(document.getElementById('root'), core);
  const press = (name) => p.key(name, { on: document.body });
  press('A');
  assert.equal(core.prefs.get('panel', null), 'spread');
  press('a');
  assert.equal(core.prefs.get('panel', null), 'off');
  press('A');
  assert.equal(core.prefs.get('panel', null), 'panel');
  const rows = mounted.menuRows();
  assert.deepEqual(checked(rows).filter((id) => id.startsWith('vg-panel-') && id !== 'vg-panel-pin'), ['vg-panel-panel']);
  mounted.unmount();
  let sets = 0;
  const give = core.prefs.set;
  core.prefs.set = (...args) => { sets++; return give(...args); };
  press('A');
  assert.equal(sets, 0, 'unbound with the mount, which gave the core its own prefs back');
});

// ---- debug ----
await ok('the_debug_query_shows_the_readout_and_the_switches_turn_the_named_parts_off', () => {
  assert.equal(debugOf(''), null);
  assert.equal(debugOf('?other=1'), null);
  const bare = debugOf('?vg-debug');
  assert.deepEqual([bare.off.size, bare.parts.size, bare.switches, bare.unknown], [0, 0, [], []]);
  const some = debugOf('?vg-debug=nosky,NOLAND,nodash,nopost,bogus');
  assert.deepEqual([...some.off].sort(), ['land', 'post', 'sky']);
  assert.deepEqual([...some.parts], ['dash']);
  assert.deepEqual(some.unknown, ['bogus']);
  const lines = readoutLines({
    perf: { words: 'WebGL2 · chosen', ms: 3.21, gpuMs: null, asked: 0, scale: 1.5, capped: false, step: 2, q: 0.78, grit: false, bloom2: true, motion: 'calm', detail: 'low' },
    fps: 58.7, canvas: [800, 600], dpr: '2.00 -> 1.50', switches: ['nosky'],
  });
  assert.equal(lines[0], 'WebGL2 · chosen', 'the first line names the tier');
  assert.match(lines.join('\n'), /draw 3\.2 ms {2}gpu n\/a/);
  assert.match(lines.join('\n'), /grit off {2}bloom2 on/);
  assert.match(lines.join('\n'), /motion calm {2}detail low/);
  assert.equal(lines.at(-1), 'off: nosky');
  assert.equal(readoutLines({ perf: null, fps: 0, canvas: [1, 1], dpr: '1', switches: [] })[0], 'starting');
  assert.equal(readoutLines({ perf: { tier: 'none' }, fps: 0, canvas: [1, 1], dpr: '1', switches: [] })[0], 'no GPU scene');
});

// ---- the driver ----
await ok('a_target_the_dash_draws_is_its_own_and_every_other_goes_to_the_cores_controls_driver', async () => {
  const s = stage({ page: pageOf(device({ playing: player() })) });
  const d = s.wired.driver;
  assert.equal(d.shown({ control: 'volume' }), '−23.5 dB');
  assert.equal(d.shown({ transport: 'pause' }), 'Pause');
  assert.equal(d.shown('Cover'), 'api/v1/host/cover?v=abc');
  assert.equal(d.shown({ control: 'input' }), 'USB');
  assert.deepEqual(s.did, [], 'those were the dash\'s');
  assert.equal(d.shown({ mode: 'eq' }), 'words');
  assert.equal(await drive(d.reach({ preset: 'Warm' })), true);
  assert.equal(await drive(d.operate({ band: 2, param: 'g' }, { step: 1 })), true);
  assert.deepEqual(s.did, [
    ['controls.shown', { mode: 'eq' }],
    ['controls.reach', { preset: 'Warm' }],
    ['controls.operate', { band: 2, param: 'g' }, { step: 1 }],
  ]);
  s.did.length = 0;
  assert.equal(d.shown({ computer: 'bypass' }), 'words', 'the computer\'s bypass is the popup\'s');
  assert.equal(await drive(d.operate({ control: 'volume' }, { step: 1 })), true);
  assert.deepEqual(s.did.at(-1).slice(0, 2), ['setVolume', 41]);
  s.stop();
});

await ok('the_figure_follows_every_step_of_a_held_key_while_the_keys_are_kept_as_they_are', () => {
  const s = stage();
  const plus = s.dash.querySelector('#vg-vup');
  pointer(s.window, 'pointerdown', plus);
  const held = s.dash.querySelector('#vg-vup');
  advance(380 + 70 * 5);
  const said = () => s.dash.querySelector('#vg-vol').textContent;
  assert.equal(said(), '−54 dB', 'the figure shows the sixth step while the key is down');
  assert.equal(s.dash.querySelector('#vg-vup'), held, 'the key under the finger is the same element');
  advance(70);
  assert.equal(said(), '−53 dB');
  pointer(s.window, 'pointerup', s.window.document);
  s.stop();
});

await ok('the_fade_is_opacity_and_transition_alone_and_the_faded_keys_stay_in_reach', () => {
  const css = readFileSync(new URL('../../../bins/svolderd/web/themes/voidgarden/style.css', import.meta.url), 'utf8');
  const rules = [...css.matchAll(/([^{}]*\.vg-dim[^{}]*)\{([^}]*)\}/g)];
  assert.ok(rules.length >= 2, 'the fade has its rules');
  for (const [, selector, body] of rules) {
    const props = body.split(';').map((d) => d.split(':')[0].trim()).filter(Boolean);
    assert.deepEqual(props.filter((p) => !['opacity', 'transition'].includes(p)), [], selector);
  }
  const s = stage();
  advance(3_000);
  assert.equal(s.vg.page.classList.contains('vg-dim'), true);
  assert.equal(Boolean(s.dash.hidden || s.dash.inert), false);
  const bar = s.dash.querySelector('.vg-bar');
  assert.equal(Boolean(bar.hidden || bar.inert || bar.hasAttribute('style')), false);
  for (const el of s.dash.querySelectorAll('[data-act]')) {
    assert.equal(Boolean(el.hidden || el.inert), false, el.id);
  }
  s.stop();
});

await ok('a_step_that_changes_nothing_at_the_cap_is_not_sent', () => {
  const s = stage({ page: pageOf(device({ volume: volume({ raw: 41, cap: 41 }) })) });
  s.dash.querySelector('#vg-vup').click();
  assert.equal(s.did.length, 0, 'at the cap a step up sends nothing');
  s.dash.querySelector('#vg-vdown').click();
  assert.equal(s.did.length, 1, 'a step down does');
  s.stop();
});

await ok('a_key_press_ends_the_deaf_time_of_a_press_whose_release_was_lost', () => {
  const s = stage();
  const plus = s.dash.querySelector('#vg-vup');
  advance(3_000);
  pointer(s.window, 'pointerdown', plus);
  assert.equal(s.did.length, 0);
  s.window.document.dispatchEvent(new s.window.KeyboardEvent('keydown', { key: 'Enter', bubbles: true }));
  plus.dispatchEvent(new s.window.MouseEvent('click', { bubbles: true, cancelable: true, detail: 0 }));
  assert.equal(s.did.filter((d) => d[0] === 'setVolume').length, 1, 'the keyboard press is not swallowed');
  s.stop();
});

await ok('the_debug_wiring_marks_the_parts_turned_off_puts_its_box_before_the_slot_and_hands_off_layers', () => {
  const p = domPage(HTML);
  const { document } = p;
  const page = document.getElementById('voidgarden');
  const asked = debugOf('?vg-debug=nodash,nosky,nopost');
  const box = wireDebug({
    page, asked, perf: () => ({ words: 'WebGL2 · chosen', draws: 1 }), canvas: { width: 8, height: 6 }, clock: () => now,
  });
  assert.equal(page.classList.contains('vg-no-dash'), true);
  assert.equal(page.lastElementChild.id, 'vg-slot', 'the slot stays last');
  assert.equal(page.lastElementChild.previousElementSibling.id, 'vg-debug');
  assert.match(document.getElementById('vg-debug').textContent, /^WebGL2 · chosen/);
  box.stop();
  assert.equal(document.getElementById('vg-debug'), null);
  assert.equal(page.classList.contains('vg-no-dash'), false);
  let got = null;
  withSwitches((gpu, options) => { got = options; return 'layers'; }, asked)('gpu', { seed: 3 });
  assert.deepEqual([...got.off].sort(), ['post', 'sky']);
  assert.equal(got.seed, 3);
  withSwitches((gpu, options) => { got = options; }, null)('gpu', {});
  assert.equal(got.off.size, 0);
});

await ok('the_page_carries_the_dashs_height_and_follows_a_wrap_and_a_resize', () => {
  const s = stage();
  let top = 700;
  Object.defineProperty(s.window, 'innerHeight', { value: 800, configurable: true });
  s.dash.getBoundingClientRect = () => ({ top });
  s.redraw();
  const read = () => s.vg.page.style.getPropertyValue('--vg-dash-h');
  s.emit('volume', { device: 'D', raw: 1 });
  s.window.dispatchEvent(new s.window.Event('resize'));
  assert.equal(read(), '100px', 'the dash and its offset from the foot');
  top = 640;
  s.window.dispatchEvent(new s.window.Event('resize'));
  assert.equal(read(), '160px', 'a wrapped bar is taller');
  s.state.page = pageOf(device({ volume: null, controls: [] }));
  top = 730;
  s.redraw();
  s.window.dispatchEvent(new s.window.Event('resize'));
  assert.equal(read(), '70px', 'and shorter again');
  s.stop();
});

await ok('the_dash_rules_are_scoped_under_the_dash_and_the_card', () => {
  const css = readFileSync(new URL('../../../bins/svolderd/web/themes/voidgarden/style.css', import.meta.url), 'utf8');
  const region = css.slice(css.indexOf('/* scene, dash, card, states */'), css.indexOf('/* HUD and panel */'));
  for (const [, selector] of region.matchAll(/^([^\s@}/][^{]*)\{/gm)) {
    for (const one of selector.replaceAll(':is(#vg-dash, #vg-card)', ':is(DASH)').split(',').map((x) => x.trim())) {
      if (!/\.vg-/.test(one) || /\.vg-(no-|debug)/.test(one)) continue;
      assert.match(one, /^(?::is\(DASH\)|#vg-dash(?:\[[^\]]*\])?|#vg-card|#voidgarden\.vg-dim #vg-dash|#voidgarden\.vg-dim) /, one);
    }
  }
});

// ---- 0.40.0 task 3.10: the chrome as approved ----
const sheet = readFileSync(new URL('../../../bins/svolderd/web/themes/voidgarden/style.css', import.meta.url), 'utf8');

await ok('the_explore_keys_have_two_line_tooltips_on_hover_and_focus_and_their_keys_are_listed_and_free', () => {
  const html = drawDash(pageOf(device()), null);
  for (const [kind, name, key, words] of [
    ['seed', 'New seed', 'S', 'A new world for this track'],
    ['scene', 'Next scene', 'N', 'Another world on the same track'],
    ['effect', 'Effect', 'E', 'Fire one of the scene’s effects'],
  ]) {
    const tip = new RegExp(`<span class="vg-tip" role="tooltip" id="vg-tip-${kind}"><b>${name} <kbd>${key}</kbd></b><span>${words}</span></span>`);
    assert.match(html, tip, `${kind}: line 1 the name and its key, line 2 what it does`);
    assert.match(html, new RegExp(`id="vg-ex-${kind}"[^>]*aria-describedby="vg-tip-${kind}"`));
  }
  assert.equal([...html.matchAll(/class="vg-tip"/g)].length, 3, 'two lines each, no more');
  assert.match(sheet, /#vg-dash \.vg-k:hover > \.vg-tip, #vg-dash \.vg-k:focus-visible > \.vg-tip \{ opacity: 1; \}/, 'on hover and on focus');
  assert.match(sheet, /#vg-dash \.vg-tip \{[^}]*background: rgba\(6, 9, 14, \.70\)/, 'see-through');
  const mine = KEYS.filter((k) => k.id.startsWith('explore-'));
  assert.deepEqual(mine.map((k) => k.key), ['S', 'N', 'E']);
  const taken = ['T', 't', '?', ' ', 'A', 'a', 'ArrowUp', 'ArrowDown'];
  for (const entry of mine) {
    for (const k of entry.keys) assert.ok(!taken.includes(k), `${k} is free of the frame\u2019s and the theme\u2019s other keys`);
    assert.ok(entry.words.length > 0, 'the help lists it');
  }
});

await ok('the_chrome_fades_to_nothing_over_0_6_s_and_the_menu_button_follows_for_3_s_or_4_on_a_phone', () => {
  for (const sel of ['.vg-cap', '.vg-fchip', '.vg-sheet']) {
    assert.match(sheet, new RegExp(`#voidgarden\\.vg-dim #vg-dash \\${sel.slice(0, 1)}${sel.slice(1)}[^{]*\\{\\s*opacity: 0; transition: opacity \\.6s ease;`), `${sel} fades to 0`);
  }
  assert.match(sheet, /#voidgarden\.vg-dim #vg-hud \{ opacity: 0; transition: opacity \.6s ease; \}/);
  const d = stage();
  assert.deepEqual(d.fades.at(-1), [true, 3000], 'the frame\u2019s button: 3 s on a desktop');
  advance(2_999);
  assert.equal(d.vg.page.classList.contains('vg-dim'), false);
  advance(1);
  assert.equal(d.vg.page.classList.contains('vg-dim'), true, 'the dash fades at 3 s');
  d.document.dispatchEvent(new d.window.Event('pointermove', { bubbles: true }));
  assert.equal(d.vg.page.classList.contains('vg-dim'), false, 'any pointer move brings it back');
  d.stop();
  const p = stage({ phone: true });
  assert.deepEqual(p.fades.at(-1), [true, 4000], '4 s on a phone');
  advance(3_999);
  assert.equal(p.vg.page.classList.contains('vg-dim'), false);
  advance(1);
  assert.equal(p.vg.page.classList.contains('vg-dim'), true);
  p.stop();
});

await ok('a_phone_has_the_features_chip_one_capsule_and_a_sheet_with_four_lines_and_nothing_is_lost', () => {
  const html = drawDash(pageOf(device({ playing: player() })), {
    side: 'device', sw: { side: 'device', dac: 'D', computer: 'This computer' }, volume: null,
  }, { phone: true });
  assert.match(html, /id="vg-fchip"[^>]*aria-expanded="false"/);
  const cap = html.match(/<div class="vg-cap vg-pcap">(.*?)<\/div><div class="vg-sheet-back/s)[1];
  assert.deepEqual([...cap.matchAll(/data-act="([\w-]+)"/g)].map((m) => m[1]), ['volume-down', 'volume-up', 'play', 'more'], 'volume, play and ⋯');
  const sheetHtml = html.match(/<div class="vg-sheet".*$/s)[0];
  const rows = sheetHtml.split('vg-sh-row').slice(1);
  assert.equal(rows.length, 4, 'four lines');
  assert.match(rows[0], /id="vg-side-dac"/);
  assert.match(rows[0], /id="vg-side-computer"/);
  assert.match(rows[0], /data-act="input"[^>]*data-list="1"/, 'line 1: device, computer and the input chip');
  assert.match(rows[1], /vg-cover/);
  assert.match(rows[1], /Song/, 'line 2: the cover and the title');
  assert.deepEqual([...rows[2].matchAll(/data-act="([\w-]+)"/g)].map((m) => m[1]), ['prev', 'play', 'next', 'mute'], 'line 3: transport and mute');
  assert.deepEqual([...rows[3].matchAll(/<span class="vg-xn" aria-hidden="true">([^<]*)</g)].map((m) => m[1]), ['\u23e9\ufe0e Seed', '\u23e9\ufe0e Scene', 'Effect'], 'line 4: the three explore actions, one line each with the fast-forward glyph');
  assert.deepEqual([...rows[3].matchAll(/aria-label="([^"]*)"/g)].map((m) => m[1]), ['New seed (S)', 'Next scene (N)', 'Effect (E)'], 'line 4: their accessible names stay whole');
  // the whole title wraps and is never cut; the desktop and the phone's capsule cut theirs
  assert.match(sheet, /#vg-dash \.vg-sheet \.vg-title \{ white-space: normal; overflow: visible; text-overflow: clip;/);
  // 44 px targets
  assert.match(sheet, /#vg-dash \.vg-pcap \.vg-round, #vg-dash \.vg-sheet \.vg-round \{ width: 44px; height: 44px; \}/);
  assert.match(sheet, /#vg-dash \.vg-pcap \.vg-k, #vg-dash \.vg-sheet \.vg-k \{ min-height: 44px; \}/);
  assert.match(sheet, /#vg-dash \.vg-sheet \.vg-chip \{ height: 44px;/);
  assert.match(sheet, /#vg-dash \.vg-fchip \{[^}]*height: 44px; min-width: 44px;/);
  // nothing lost: every key the dash had is still drawn, on a desktop or a phone, or the menu's
  const desk = drawDash(pageOf(device({ playing: player() })), null);
  for (const act of ['volume-down', 'volume-up', 'mute', 'input', 'prev', 'play', 'next', 'explore']) {
    assert.match(desk, new RegExp(`data-act="${act}"`), `${act} on the desktop`);
    assert.match(html, new RegExp(`data-act="${act}"`), `${act} on the phone`);
  }
});

await ok('on_a_phone_the_dots_open_the_sheet_a_swipe_down_a_tap_above_or_escape_close_it_and_the_chip_opens_the_panel', () => {
  const s = stage({ phone: true });
  const told = [];
  s.vg.steps.addEventListener('feat', (e) => told.push(e.detail.open));
  const more = () => s.dash.querySelector('#vg-more');
  assert.equal(s.dash.dataset.sheet, undefined);
  more().click();
  assert.equal(s.dash.dataset.sheet, 'open');
  s.dash.querySelector('.vg-sheet-back').click();
  assert.equal(s.dash.dataset.sheet, undefined, 'a tap above closes it');
  more().click();
  pointer(s.window, 'pointerdown', s.dash.querySelector('#vg-sheet'), { clientY: 100 });
  pointer(s.window, 'pointermove', s.dash.querySelector('#vg-sheet'), { clientY: 120 });
  assert.equal(s.dash.dataset.sheet, 'open', 'a short drag is not a swipe');
  pointer(s.window, 'pointermove', s.dash.querySelector('#vg-sheet'), { clientY: 150 });
  assert.equal(s.dash.dataset.sheet, undefined, 'a swipe down closes it');
  more().click();
  more().focus();
  more().dispatchEvent(new s.window.KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
  assert.equal(s.dash.dataset.sheet, undefined, 'Escape closes it');
  more().blur();
  // the chip opens the panel and closes it again
  s.dash.querySelector('#vg-fchip').click();
  assert.equal(s.vg.page.dataset.feat, 'open');
  assert.equal(s.dash.querySelector('#vg-fchip').getAttribute('aria-expanded'), 'true');
  s.dash.querySelector('#vg-fchip').click();
  assert.equal(s.vg.page.dataset.feat, undefined);
  assert.deepEqual(told, [true, false]);
  // a tap on the faded chrome only brings it back
  advance(4_000);
  assert.equal(s.vg.page.classList.contains('vg-dim'), true);
  const plus = s.dash.querySelector('#vg-vup');
  pointer(s.window, 'pointerdown', plus);
  assert.equal(s.did.filter((d) => d[0] === 'setVolume').length, 0, 'the tap that woke it did nothing');
  s.stop();
});

await ok('the_phone_sheets_keys_are_reached_by_the_driver_through_the_sheet', async () => {
  const s = stage({ phone: true });
  assert.equal(s.dash.dataset.sheet, undefined);
  await drive(s.wired.driver.reach({ transport: 'previous' }));
  assert.equal(s.dash.dataset.sheet, 'open', 'the sheet opened for it');
  s.stop();
});

await ok('explore_works_whatever_the_device_a_page_out_of_reach_keeps_the_scenes_keys', () => {
  const s = stage();
  s.state.page = pageOf(device({ card: { kind: 'unreachable', title: 'x', try_now: true } }));
  s.redraw();
  for (const kind of ['seed', 'scene', 'effect']) assert.equal(s.dash.querySelector(`#vg-ex-${kind}`).inert, false, kind);
  s.stop();
});

console.log(`voidgarden-dash: ${n} ok`);
process.exit(0);
