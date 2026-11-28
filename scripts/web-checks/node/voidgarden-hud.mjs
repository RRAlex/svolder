/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for Void Garden's HUD and audio panel (themes/voidgarden/hud.js, hud-wire.js,
// panel-wire.js, draw/hud.js, draw/panel.js): the modules' words from page fixtures (every row, the
// rows a page does not carry left out, the path in amber when it changes the sound, the VU rows'
// dBFS through core/vu-model.js), the moments that bring a module in (the same at any step rate),
// the link's 8 s history, an amp's meters put in the rows already drawn, the PipeWire config chip,
// the levels stream held only while the Host module shows, the panel's modes, its fade at 8 s
// and its wakes on a clock the harness hands in, and what the canvas painters draw (into a
// recording context). A jsdom page (apt `node-jsdom`).
// Run: node voidgarden-hud.mjs
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { page as makePage } from './dom.mjs';
import { createStore } from '../../../bins/svolderd/web/core/store.js';
import { SETUP } from '../../../bins/svolderd/web/core/popups/listed.js';
import { KEYS } from '../../../bins/svolderd/web/themes/voidgarden/features.js';
import { History } from '../../../bins/svolderd/web/themes/voidgarden/history.js';
import {
  FADE, Levels, LinkHistory, Moments, SPARK_POINTS, ampOf, centroidHz, dbfsWords, featureWords, modulesOf,
  rowText, sparkOf, topLine, vuBar,
} from '../../../bins/svolderd/web/themes/voidgarden/hud.js';
import { hudMarkup, metersInPlace, paintHud } from '../../../bins/svolderd/web/themes/voidgarden/draw/hud.js';
import { panelInk, panelMarkup, paintPanel } from '../../../bins/svolderd/web/themes/voidgarden/draw/panel.js';
import { wireHud } from '../../../bins/svolderd/web/themes/voidgarden/hud-wire.js';
import { MODES, wirePanel } from '../../../bins/svolderd/web/themes/voidgarden/panel-wire.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

// ------------------------------------------------------------------ the pages
const DAC = { name: 'Bifrost 3', url: 'Bifrost%203', family: 'dac' };
const AMP = { name: 'Asgard 3', url: 'Asgard%203', family: 'amp' };
const NOW = 1800000000000;

const hops = (changes = false, detail = 'USB 1, 96 kHz') => [
  { label: 'player', detail: '44.1 kHz S16LE', changes: false },
  { label: 'computer', detail: '96 kHz S32LE', changes },
  { label: 'DAC', detail, changes: false },
];
const flat = () => Array.from({ length: 181 }, () => 0);
const curve = () => Array.from({ length: 181 }, (_, i) => 6 * Math.sin((i / 180) * Math.PI * 2));
const meters = (fractions = [0.4, 0.55]) => ({
  live: true,
  state: null,
  meters: [
    { key: 'out_l', channel: 'L', fraction: fractions[0], text: '1.2 W', class_a: 0.3 },
    { key: 'out_r', channel: 'R', fraction: fractions[1], text: '1.4 W', class_a: 0.3 },
  ],
});
const ampModel = () => ({
  name: AMP.name,
  url: AMP.url,
  family: 'amp',
  power: { key: 'power', on: true, word: 'On', warming: false, toggles: [{ kind: 'toggle', key: 'gain', label: 'Gain', on: true }], readouts: [] },
  controls: [],
  tone: { mode: 'eq', strength: null, bands: { rows: [{ g: '+3.0 dB' }, { g: '−1.5 dB' }] } },
  meters: meters(),
});

/** A page with everything the HUD can say, each part a function of what a test changes. */
function full(more = {}) {
  return {
    v: 1,
    viewer: { signed_in: true, guest: false, writes: true },
    shown: DAC.url,
    fleet: { devices: [DAC, AMP], hops: [{ id: 'a', kind: 'amp', name: AMP.name, after: DAC.url, device: AMP.url, live: true }] },
    followed: { [AMP.url]: ampModel() },
    device: {
      name: DAC.name,
      url: DAC.url,
      family: 'dac',
      link: { word: 'connected', state: 'on', old: false },
      volume: { lo: 0, hi: 200, raw: 100, text: '-23.5 dB', unit: 'dB', muted: false, unread: false },
      controls: [
        { key: 'input', kind: 'choice', label: 'Input', options: [{ label: 'Optical', on: false }, { label: 'USB', on: true }] },
        { key: 'sample_rate', kind: 'readout', label: 'Rate', text: '96 kHz', tone: '' },
        { key: 'filter', kind: 'choice', label: 'Filter', options: [{ label: 'Fast', on: false }, { label: 'NOS', on: true }] },
      ],
      facts: { now_ms: NOW, signal: '−58 dBm live', signal_age: { head: '−58 dBm live', heard_at_ms: NOW - 2000, live: true } },
      playing: {
        track: { status: 'playing' },
        path: {
          verdict: 'untouched', end_to_end: 'untouched', words: ['host EQ on'], hops: hops(),
          config: { chip: 'config ✓', outcome: 'untouched', summary: 'the PipeWire config is fine' },
        },
      },
    },
    host: {
      track: { headline: 'Song', artist: 'Artist', status: 'playing' },
      route: {
        sink: { words: 'Schiit Bifrost 3', rate_hz: 96000, format: 'S32LE', muted: false },
        player_rate_hz: 44100,
        host_eq: true,
      },
      computer: {
        volume: { pct: 25, muted: false },
        eq: {
          on: true,
          preset: 'warm',
          running_unknown: false,
          points: [{ f: 100 }, { f: 1000 }, { f: 8000 }],
          response: curve(),
          crossfeed: { on: true, model: 'bs2b', amount_pct: 45 },
          speakers: { on: true, label: '2.0 studio 45°' },
          loudness: { on: true, strength_pct: 40, depth_db: -3.2 },
        },
      },
    },
    ...more,
  };
}

const ids = (modules, id) => modules.find((m) => m.id === id)?.rows.map((r) => r.id) ?? null;
const rowOf = (modules, mod, id) => modules.find((m) => m.id === mod)?.rows.find((r) => r.id === id);

// ------------------------------------------------------------------ words from the model
await ok('every_row_of_the_host_the_dac_and_the_amp_reads_its_fact', () => {
  const modules = modulesOf(full());
  assert.deepEqual(modules.map((m) => m.id), ['host', 'dac', 'amp']);
  assert.deepEqual(ids(modules, 'host'), ['output', 'format', 'eq', 'xfeed', 'speakers', 'loudness', 'vu-l', 'vu-r', 'path']);
  const say = (label) => rowText(modules, label);
  assert.equal(say('Output'), 'Schiit Bifrost 3 · playing');
  assert.equal(say('Format'), '96 kHz · S32LE · from 44.1 kHz');
  assert.equal(say('EQ'), 'on · warm · 3 points');
  assert.equal(say('XFeed'), 'on · bs2b · 45 %');
  assert.equal(say('V.Spkr'), '2.0 studio 45°');
  assert.equal(say('Loudness'), '40 % · depth −3.2 dB');
  assert.equal(say('Path'), 'player → computer → host EQ on → DAC');
  assert.deepEqual(ids(modules, 'dac'), ['volume', 'input', 'control:filter', 'link', 'state']);
  assert.equal(say('Volume'), '−23.5 dB');
  assert.equal(say('Input'), 'USB · 96 kHz');
  assert.equal(say('Filter'), 'NOS');
  assert.equal(say('Ctrl'), '−58 dBm live');
  assert.equal(say('State'), 'connected');
  assert.deepEqual(ids(modules, 'amp'), ['power', 'control:gain', 'tone', 'meter:out_l', 'meter:out_r']);
  assert.equal(say('Power'), 'On');
  assert.equal(say('Gain'), 'on');
  assert.equal(say('Tone'), 'eq · +3.0 −1.5 dB');
  assert.equal(say('L'), '1.2 W');
  assert.equal(rowOf(modules, 'amp', 'meter:out_l').slot.value, 0.4);
  assert.equal(rowOf(modules, 'host', 'eq').slot.values.length, SPARK_POINTS, 'the EQ line draws 64 values');
});

await ok('a_row_the_model_does_not_carry_is_left_out', () => {
  const p = full();
  delete p.host.computer.eq.crossfeed;
  delete p.host.computer.eq.speakers;
  delete p.host.computer.eq.loudness;
  p.host.route.sink = { words: 'Out', rate_hz: null, format: null, muted: false };
  p.device.controls = [];
  delete p.device.volume;
  delete p.device.link;
  delete p.device.facts;
  delete p.fleet.hops;
  const modules = modulesOf(p);
  assert.deepEqual(ids(modules, 'host'), ['output', 'eq', 'vu-l', 'vu-r', 'path']);
  assert.equal(ids(modules, 'dac'), null, 'a DAC with none of the rows has no module');
  assert.equal(ids(modules, 'amp'), null, 'no followed amp: no Amp module');
  const bare = modulesOf(full({ host: null, followed: {} }));
  assert.deepEqual(bare.map((m) => m.id), ['dac'], 'no host: no Host module');
  const noEq = full();
  delete noEq.host.computer.eq;
  assert.ok(!ids(modulesOf(noEq), 'host').includes('eq'));
  assert.ok(!ids(modulesOf(noEq), 'host').includes('xfeed'));
  const off = full();
  off.host.computer.eq.on = false;
  off.host.computer.eq.crossfeed.on = false;
  assert.equal(rowText(modulesOf(off), 'EQ'), 'off');
  assert.equal(rowOf(modulesOf(off), 'host', 'eq').slot, undefined, 'an EQ that is off draws no curve');
  assert.equal(rowText(modulesOf(off), 'XFeed'), 'off');
  const meterless = full();
  delete meterless.followed[AMP.url].meters;
  assert.deepEqual(ids(modulesOf(meterless), 'amp'), ['power', 'control:gain', 'tone']);
});

await ok('the_amp_is_the_shown_one_else_a_followed_one', () => {
  const asAmp = full({ shown: AMP.url, device: ampModel(), followed: {} });
  assert.equal(ampOf(asAmp).url, AMP.url);
  assert.deepEqual(modulesOf(asAmp).map((m) => m.id), ['host', 'amp'], 'an amp shown has no DAC module');
  assert.equal(ampOf(full()).url, AMP.url);
  assert.equal(ampOf(full({ fleet: { devices: [DAC, AMP], hops: [] } })), null);
  const other = full();
  other.fleet.hops[0].kind = 'other';
  assert.equal(ampOf(other), null, 'only an amp hop is read as one');
});

await ok('the_path_is_amber_when_it_changes_the_sound_and_says_so', () => {
  const plain = rowOf(modulesOf(full()), 'host', 'path');
  assert.equal(plain.tone, '');
  assert.equal(plain.note, undefined);
  const changed = full();
  changed.device.playing.path.hops = hops(true);
  const row = rowOf(modulesOf(changed), 'host', 'path');
  assert.equal(row.tone, 'warn');
  assert.equal(row.note, 'changes the sound');
  assert.match(hudMarkup(modulesOf(changed)), /data-row="path" data-tone="warn"/);
  assert.equal(rowText(modulesOf(changed), 'Path'), 'player → computer → host EQ on → DAC · changes the sound');
  const own = full();
  own.device.playing.path.hops = hops(false, 'USB 1, 96 kHz, volume −3.3 dB, tone control');
  const dac = rowOf(modulesOf(own), 'host', 'path');
  assert.equal(dac.tone, 'warn', 'the DAC’s own stage changes the sound too');
  assert.match(dac.text, /DAC → volume −3\.3 dB → tone control$/);
  assert.equal(rowOf(modulesOf(full()), 'host', 'path').chip.text, 'config ✓');
  const noPath = full();
  noPath.device.playing.path.hops = [];
  assert.ok(!ids(modulesOf(noPath), 'host').includes('path'));
});

await ok('the_link_row_names_dbm_bars_and_the_state_in_words', () => {
  const weak = full();
  weak.device.facts.signal_age = { head: '−85 dBm live', heard_at_ms: NOW - 1000, live: true };
  const link = rowOf(modulesOf(weak), 'dac', 'link');
  assert.equal(link.text, '−85 dBm live');
  assert.equal(link.slot.bars, 1);
  assert.equal(link.tone, 'warn');
  assert.equal(rowText(modulesOf(weak), 'State'), 'connected · weak link');
  const strong = rowOf(modulesOf(full()), 'dac', 'link');
  assert.equal(strong.slot.bars, 3);
  const lost = full();
  lost.device.link = { word: 'lost', state: 'off', old: false };
  const gone = rowOf(modulesOf(lost), 'dac', 'link');
  assert.equal(gone.text, 'lost');
  assert.equal(gone.slot.bars, 0);
  assert.equal(gone.tone, 'bad');
  const muted = full();
  muted.device.volume = { ...muted.device.volume, muted: true };
  assert.equal(rowText(modulesOf(muted), 'Volume'), '−∞ dB muted');
  assert.equal(rowText(modulesOf(muted), 'State'), 'connected · muted');
});

/** The byte of an RMS level in dBFS (the wire's: -72 .. 0 over 0 .. 255). */
const byteOf = (db) => Math.round(((db + 72) / 72) * 255);

await ok('the_vu_rows_read_the_rms_in_dbfs_from_the_byte_not_a_needle', () => {
  // a steady level reads as itself, whatever the reference: the byte's own step is 0.28 dB
  for (const db of [-14, -10, -6, -3]) {
    const levels = new Levels();
    levels.push([byteOf(db), byteOf(db), 0, 0], 0);
    const r = levels.step(3, 3000).L;
    assert.ok(Math.abs(r.db - db) < 0.15, `${db} dBFS read as ${r.db}`);
    assert.equal(r.text, dbfsWords(r.db));
    assert.ok(Math.abs(r.bar - (db + 60) / 60) < 0.01, 'the bar runs -60 to 0 dBFS');
  }
  const eight = new Levels();
  eight.push([byteOf(-8), byteOf(-8), 0, 0], 0);
  assert.equal(eight.step(3, 3000).L.text, '\u22127.9 dBFS', 'a steady -8 reads -7.9, its byte (a needle stuck at -14.8)');
  assert.equal(dbfsWords(-Infinity), '\u2212\u221e dBFS');
  assert.equal(dbfsWords(-89), '\u2212\u221e dBFS');
  assert.equal(dbfsWords(-3.04), '\u22123.0 dBFS');
  assert.equal(dbfsWords(1.26), '+1.3 dBFS');
  assert.equal(new Levels().step(1, 0).L.text, '\u2212\u221e dBFS');
  assert.deepEqual([vuBar(-60), vuBar(0), vuBar(-Infinity), vuBar(6)], [0, 1, 0, 1]);
});

await ok('a_level_follows_its_byte_by_a_time_constant_the_same_at_any_step_rate', () => {
  const after = (rate) => {
    const levels = new Levels();
    levels.push([byteOf(-10), byteOf(-10), 0, 0], 0);
    let r;
    for (let i = 0; i < rate / 10; i++) r = levels.step(1 / rate, i * 1000 / rate).L;
    return r.db;
  };
  const [a, b, c] = [30, 60, 120].map(after);
  assert.ok(Math.abs(a - b) < 0.05 && Math.abs(b - c) < 0.05, `${a} ${b} ${c}`);
  assert.ok(a < -10.5 && a > -60, 'a tenth of a second in: on its way');
});

await ok('the_peak_mark_is_held_then_falls_and_a_peak_byte_of_255_lights_the_over_lamp', () => {
  const levels = new Levels();
  levels.push([byteOf(-30), byteOf(-30), byteOf(-6), 255], 0);
  const first = levels.step(0.5, 500);
  assert.ok(Math.abs(first.L.peak - 0.9) < 0.02, 'the mark is at the peak byte');
  assert.equal(first.R.lamp, true);
  assert.equal(first.L.lamp, false);
  const held = levels.step(0.5, 1000);
  assert.ok(Math.abs(held.L.peak - 0.9) < 0.02, 'held for 1.2 s');
  const fallen = levels.step(2, 3000);
  assert.ok(fallen.L.peak < 0.9 - 0.3, 'then it falls');
  assert.equal(levels.step(0.1, 3000).R.lamp, false, 'the lamp lets go after 1.2 s');
  levels.clear();
  assert.equal(levels.step(1, 0).L.text, '\u2212\u221e dBFS');
});

await ok('the_sparkline_is_the_curve_over_plus_minus_12_dB_in_64_values', () => {
  const s = sparkOf(flat());
  assert.equal(s.length, 64);
  assert.ok(s.every((v) => v === 0.5));
  const peak = sparkOf(Array.from({ length: 181 }, (_, i) => (i === 180 ? 18 : -18)));
  assert.equal(peak[0], 0);
  assert.equal(peak[63], 1, 'held to the ends');
  assert.equal(sparkOf(null), null);
  assert.equal(sparkOf([1]), null);
});

await ok('the_links_history_keeps_8_seconds_of_new_readings', () => {
  const h = new LinkHistory();
  h.push(-60, 1, 0, 'a');
  h.push(-60, 1, 1000, 'a');
  assert.equal(h.points(1000).length, 1, 'a reading seen again is one sample');
  h.push(-70, 2, 4000, 'a');
  const at = h.points(4000);
  assert.deepEqual(at.map((p) => Math.round(p.x * 1000) / 1000), [0.5, 1]);
  assert.ok(at[1].y < at[0].y, 'weaker is lower');
  h.push(-70, 3, 9000, 'a');
  assert.equal(h.points(9000).length, 2, 'the first is past 8 s');
  h.push(-50, 9, 9500, 'b');
  assert.equal(h.points(9500).length, 1, 'another device starts afresh');
});

// ------------------------------------------------------------------ the director's moments
/** `moments` stepped over `seconds` at `rate` steps a second with `at` as the event at second `at`. */
function run(seconds, rate, { event = null, at = 1, has = ['host', 'dac', 'amp'], narrow = false } = {}) {
  const m = new Moments();
  // the start shows every module for 5 s: let it pass
  m.step(0.1, { has, narrow, rate: 'r' });
  m.step(6, { has, narrow, rate: 'r' });
  const dt = 1 / rate;
  const steps = Math.round(seconds * rate);
  let fired = false;
  for (let i = 0; i < steps; i++) {
    const t = (i + 1) * dt;
    const events = event && !fired && t >= at - 1e-9 ? [event] : [];
    if (events.length) fired = true;
    m.step(dt, { events, has, narrow, changed: false, rate: 'r' });
  }
  return m;
}

await ok('a_moment_brings_its_module_in_for_a_few_seconds_the_same_at_any_step_rate', () => {
  const alphas = [30, 60, 144].map((rate) => {
    const m = run(3, rate, { event: { type: 'section', section: 'build' }, at: 1 });
    return ['host', 'dac', 'amp'].map((id) => m.alpha(id));
  });
  for (const a of alphas) {
    assert.equal(a[0], 1, 'the section shows the Host');
    assert.equal(a[2], 0, 'and not the amp (not a drop)');
  }
  const late = [30, 60, 144].map((rate) => run(7, rate, { event: { type: 'section', section: 'build' }, at: 1 }).alpha('host'));
  assert.deepEqual(late, [0, 0, 0], 'gone after its 4 s');
  const edge = [30, 60, 144].map((rate) => Math.round(run(1.1, rate, { event: { type: 'section', section: 'build' }, at: 1 }).alpha('host') * 100) / 100);
  assert.ok(new Set(edge).size <= 2 && Math.abs(edge[0] - edge[2]) < 0.2, `coming in alike at every rate: ${edge}`);
  const drop = run(3, 60, { event: { type: 'section', section: 'drop' }, at: 1 });
  assert.equal(drop.alpha('amp'), 1, 'a drop shows the Amp');
  const track = run(3, 60, { event: { type: 'reseed' }, at: 1 });
  assert.equal(track.alpha('dac'), 1, 'a new track shows the DAC');
});

await ok('one_module_to_a_slot_and_on_a_narrow_screen_one_at_a_time', () => {
  const m = new Moments();
  m.on('host', 4, true);
  m.step(0.1, { has: ['host', 'dac'], narrow: true });
  m.on('dac', 4, true);
  m.step(1, { has: ['host', 'dac'], narrow: true });
  assert.equal(m.alpha('host'), 0, 'the DAC sent the Host out');
  assert.equal(m.alpha('dac'), 1);
  const wide = new Moments();
  wide.step(0.1, { has: ['host', 'dac'] });
  wide.step(6, { has: ['host', 'dac'] });
  wide.on('host', 4);
  wide.on('dac', 4);
  wide.step(1, { has: ['host', 'dac'] });
  assert.ok(wide.alpha('host') > 0.9 && wide.alpha('dac') > 0.9, 'two slots show together');
});

await ok('the_path_changing_the_sound_and_a_new_rate_are_moments', () => {
  const m = new Moments();
  m.step(0.1, { has: ['host'], changed: false, rate: '96' });
  m.step(6, { has: ['host'], changed: false, rate: '96' });
  assert.equal(m.alpha('host'), 0);
  m.step(0.5, { has: ['host'], changed: true, rate: '96' });
  m.step(0.5, { has: ['host'], changed: true, rate: '96' });
  assert.equal(m.alpha('host'), 1, 'the path started to change the sound');
  m.step(8, { has: ['host'], changed: true, rate: '96' });
  assert.equal(m.alpha('host'), 0);
  m.step(0.5, { has: ['host'], changed: true, rate: '44.1' });
  m.step(0.5, { has: ['host'], changed: true, rate: '44.1' });
  assert.equal(m.alpha('host'), 1, 'a new rate');
});

await ok('the_features_say_their_values_in_words', () => {
  assert.equal(featureWords('bass', 0.624), '062', 'three digits, the way the panel says them');
  assert.equal(featureWords('bass', 3), '100');
  assert.equal(featureWords('bass', 0), '000');
  assert.equal(featureWords('flat', 0.31), '0.31 T');
  assert.equal(featureWords('flat', 0.62), '0.62 N');
  assert.equal(featureWords('cent', 0), '30Hz');
  assert.equal(featureWords('cent', 1), '14kHz');
  assert.ok(Math.abs(centroidHz(0.5) - Math.sqrt(30 * 14000)) < 1e-6);
});

// ------------------------------------------------------------------ the wires over a page
/** A recording 2D context. */
function recorder() {
  const calls = [];
  const state = {};
  const ctx = new Proxy(state, {
    get: (t, k) => (k === 'calls' ? calls : (k in t ? t[k] : (...a) => { calls.push([k, ...a]); })),
    set: (t, k, v) => { t[k] = v; return true; },
  });
  return ctx;
}

/** A page with the HUD and the panel layers and a core over a real store. */
function rig(model = full()) {
  const p = makePage('<div id="voidgarden"><canvas id="vg-scene"></canvas><div id="vg-hud"></div><div id="vg-panel"></div></div>');
  const { window, document } = p;
  globalThis.window = window;
  globalThis.document = document;
  globalThis.AbortController = window.AbortController;
  globalThis.CustomEvent = window.CustomEvent;
  // jsdom's page is never visible unless it pretends to be
  Object.defineProperty(document, 'hidden', { configurable: true, get: () => false });
  const ctx = recorder();
  window.HTMLCanvasElement.prototype.getContext = () => ctx;
  const store = createStore(model, { fetch: () => Promise.reject(new Error('no network')) });
  const did = { opened: [], levels: [], closed: 0, prefs: [] };
  const core = {
    model: store,
    computer: { toneOf: () => null },
    levels: { open: (listeners) => { did.levels.push(listeners); return { close: () => { did.closed++; } }; } },
    router: { open: (name) => did.opened.push(name) },
  };
  const control = new AbortController();
  const state = { now: 0 };
  const vg = {
    page: document.getElementById('voidgarden'), core, signal: control.signal, prefs: { set: (k, v) => did.prefs.push([k, v]), get: () => undefined }, steps: new window.EventTarget(),
  };
  const clock = () => state.now;
  return { ...p, ctx, store, core, did, control, vg, state, clock };
}

const featureValues = (v = 0.5) => Object.fromEntries(KEYS.map((k) => [k, v]));
/** One `step` of `dt` s: the page time moves by `dt`. */
function step(r, history, extra = {}) {
  const dt = extra.dt ?? 0.25;
  r.state.now += dt * 1000;
  history.push(featureValues(), dt);
  r.vg.steps.dispatchEvent(new CustomEvent('step', {
    detail: {
      dt, events: [], features: featureValues(), raw: { peak: 0 }, cues: { rateHz: 96000, inputRateHz: 96000, mute: false }, history, ...extra,
    },
  }));
}

/** The opacity of the panel layer. */
const opacity = (r) => Number(r.vg.page.querySelector('#vg-panel').style.opacity);

await ok('the_hud_draws_the_modules_and_the_driver_reads_what_is_shown', () => {
  const r = rig();
  const hud = wireHud(r.vg);
  const mods = [...r.vg.page.querySelectorAll('#vg-hud [data-mod]')].map((e) => e.dataset.mod);
  assert.deepEqual(mods, ['host', 'dac', 'amp']);
  assert.equal(hud.driver.shown('output'), 'Schiit Bifrost 3 · playing');
  assert.equal(hud.driver.shown('Path'), 'player → computer → host EQ on → DAC');
  assert.equal(hud.driver.shown('nonsense'), null);
  assert.equal(r.vg.page.querySelector('.vg-h-mod').getAttribute('aria-label'), 'Host');
  assert.equal(r.vg.page.querySelector('.vg-h-slot').getAttribute('aria-hidden'), 'true', 'the canvas parts are hidden from a reader');
  assert.equal(r.vg.page.querySelector('.vg-h-cv').getAttribute('aria-hidden'), 'true');
  r.control.abort();
});

await ok('a_module_is_out_until_a_moment_and_only_its_opacity_changes', () => {
  const r = rig();
  wireHud(r.vg);
  const host = r.vg.page.querySelector('[data-mod="host"]');
  const history = new History();
  assert.equal(host.dataset.on, '0');
  step(r, history, { dt: 0.5 });
  step(r, history, { dt: 0.5 });
  assert.equal(host.dataset.on, '1', 'the start shows the modules');
  assert.equal(Number(host.style.opacity), 1);
  assert.ok(host.querySelector('.vg-h-v').textContent.length > 0, 'its words stay in the page');
  for (let i = 0; i < 12; i++) step(r, history, { dt: 0.5 });
  assert.equal(host.dataset.on, '0');
  assert.equal(Number(host.style.opacity), 0);
  assert.notEqual(host.querySelector('.vg-h-v').textContent, '', 'out is opacity only: the words are still there');
  step(r, history, { dt: 0.1, events: [{ type: 'section', section: 'build' }] });
  step(r, history, { dt: 0.5 });
  assert.equal(host.dataset.on, '1', 'a section change brings it back');
  r.control.abort();
});

await ok('the_panel_spread_keeps_every_module_in', () => {
  const r = rig();
  wireHud(r.vg);
  const panel = wirePanel(r.vg);
  const history = new History();
  for (let i = 0; i < 16; i++) step(r, history, { dt: 0.5 });
  const mods = [...r.vg.page.querySelectorAll('[data-mod]')];
  assert.ok(mods.every((m) => m.dataset.on === '0'));
  panel.setMode('spread');
  assert.ok(mods.every((m) => m.dataset.on === '1' && Number(m.style.opacity) === 1), 'always while spread');
  panel.setMode('panel');
  assert.ok(mods.every((m) => m.dataset.on === '0'));
  r.control.abort();
});

await ok('an_amps_meters_move_in_the_rows_already_drawn_on_the_meters_event', () => {
  const r = rig();
  wireHud(r.vg);
  const row = r.vg.page.querySelector('[data-row="meter:out_l"]');
  const bar = row.querySelector('.vg-h-slot');
  assert.equal(bar.dataset.v, '0.4000');
  assert.equal(row.querySelector('.vg-h-v').textContent, '1.2 W');
  r.store.emit('meters', { device: AMP.url, ...meters([0.9, 0.1]) });
  assert.equal(r.vg.page.querySelector('[data-row="meter:out_l"]'), row, 'the same element');
  assert.equal(row.querySelector('.vg-h-slot'), bar);
  assert.equal(bar.dataset.v, '0.9000');
  assert.equal(r.vg.page.querySelector('[data-row="meter:out_r"] .vg-h-slot').dataset.v, '0.1000');
  r.store.emit('meters', { device: AMP.url, meters: meters().meters.map((m) => ({ ...m, text: '9.9 W' })) });
  assert.equal(row.querySelector('.vg-h-v').textContent, '9.9 W');
  // another reading of the model that says what is drawn does not draw it again
  r.store.emit('host', r.store.page().host);
  assert.equal(r.vg.page.querySelector('[data-row="meter:out_l"]'), row, 'a later draw keeps the rows');
  r.control.abort();
});

await ok('meters_that_change_in_number_or_state_draw_the_amp_again', () => {
  const p = full();
  const modules = modulesOf(p);
  const root = makePage(hudMarkup(modules)).document.body;
  assert.equal(metersInPlace(root, ampOf(p)), true);
  const gone = ampModel();
  gone.meters = { live: false, state: 'the amp is asleep', meters: [] };
  assert.equal(metersInPlace(root, gone), false, 'the rows differ');
  const fewer = ampModel();
  fewer.meters.meters.pop();
  assert.equal(metersInPlace(root, fewer), false);
  const asleep = full();
  asleep.followed[AMP.url].meters = { live: false, state: 'the amp is asleep', meters: meters().meters.map((m) => ({ ...m, fraction: null })) };
  assert.equal(rowText(modulesOf(asleep), '—'), 'the amp is asleep');
  const root2 = makePage(hudMarkup(modulesOf(asleep))).document.body;
  assert.equal(metersInPlace(root2, asleep.followed[AMP.url]), true);
  assert.equal(root2.querySelector('[data-row="meter:out_l"] .vg-h-slot').dataset.off, '1', 'an unread meter is drawn as off');
});

await ok('the_config_chip_opens_the_pipewire_config_popup', () => {
  const r = rig();
  wireHud(r.vg);
  const chip = r.vg.page.querySelector('.vg-h-chip');
  assert.equal(chip.textContent, 'config ✓');
  assert.equal(chip.dataset.act, 'config');
  chip.click();
  assert.deepEqual(r.did.opened, [SETUP]);
  const fail = full();
  fail.device.playing.path.config = { chip: 'config 2', outcome: 'changes', summary: '2 lines differ' };
  const r2 = rig(fail);
  wireHud(r2.vg, { clock: r2.clock });
  assert.equal(r2.vg.page.querySelector('.vg-h-chip').dataset.outcome, 'changes');
  r2.vg.page.querySelector('.vg-h-chip').click();
  assert.deepEqual(r2.did.opened, [SETUP]);
  r.control.abort();
  r2.control.abort();
});

await ok('the_vu_rows_follow_the_levels_stream_held_only_while_the_host_shows', () => {
  const r = rig();
  const hud = wireHud(r.vg);
  const history = new History();
  assert.equal(r.did.levels.length, 0, 'not held before the Host module shows');
  step(r, history, { dt: 0.25 });
  assert.equal(r.did.levels.length, 1, 'held while it shows');
  const lv = (db) => Math.round(((db + 72) / 72) * 255);
  for (let i = 0; i < 20; i++) {
    r.state.now += 100;
    r.did.levels[0].frame(JSON.stringify({ t: r.state.now, lv: [lv(-8), lv(-30), 0, 0] }));
  }
  step(r, history, { dt: 2 });
  assert.equal(hud.driver.shown('VU L'), '−7.9 dBFS', 'a level above a needle\u2019s stop');
  assert.equal(hud.driver.shown('VU R'), '−29.9 dBFS');
  // the stream's own capture event (`capture` names the state): running keeps the levels
  r.did.levels[0].capture({ capture: 'running', sink: 'hdmi', why: null });
  step(r, history, { dt: 0.01 });
  assert.equal(hud.driver.shown('VU L'), '−7.9 dBFS', 'a running capture keeps the levels');
  r.did.levels[0].frame(JSON.stringify({ t: r.state.now + 10, lv: [lv(-8), lv(-30), 255, 0] }));
  step(r, history, { dt: 0.01 });
  assert.match(hud.driver.shown('VU L'), /· over$/, 'the over lamp');
  for (let i = 0; i < 20; i++) step(r, history, { dt: 0.5 });
  assert.equal(r.did.closed, 1, 'let go once the Host module is out');
  r.control.abort();
});

await ok('the_levels_stream_is_let_go_while_the_page_is_hidden', () => {
  const r = rig();
  wireHud(r.vg);
  const history = new History();
  step(r, history, { dt: 0.25 });
  assert.equal(r.did.levels.length, 1);
  Object.defineProperty(r.document, 'hidden', { configurable: true, get: () => true });
  r.document.dispatchEvent(new r.window.Event('visibilitychange'));
  assert.equal(r.did.closed, 1);
  r.control.abort();
});

await ok('the_top_line_names_the_device_its_link_words_and_the_verdict_or_the_trouble', () => {
  const plain = topLine(full(), 'open');
  assert.equal(plain.left, 'Bifrost 3 \u00b7 USB \u00b7 96 kHz');
  assert.equal(plain.link, null);
  assert.equal(plain.right, 'host EQ on');
  assert.equal(plain.tone, '');
  const changed = full();
  changed.device.playing.path.hops = hops(true);
  assert.equal(topLine(changed, 'open').right, 'host EQ on \u00b7 changes the sound');
  assert.equal(topLine(changed, 'open').tone, 'warn');
  const own = full();
  own.device.playing.path.hops = hops(false, 'USB 1, 96 kHz, volume \u22123.3 dB');
  assert.match(topLine(own, 'open').right, /DAC volume \u22123\.3 dB$/);
  const lost = full();
  lost.device.link = { word: 'lost', state: 'off', old: false };
  const t = topLine(lost, 'open');
  assert.equal(t.left, 'Bifrost 3 \u00b7 lost \u00b7 USB \u00b7 96 kHz');
  assert.equal(t.link, 'lost');
  const trouble = topLine(full(), 'lost');
  assert.equal(trouble.right, 'reconnecting');
  assert.equal(trouble.tone, 'bad');
  assert.equal(topLine(full(), 'waiting').tone, 'warn');
  const card = full();
  card.device.card = { kind: 'out', title: 'Out of reach' };
  assert.equal(topLine(card, 'open').right, 'out of reach');
});

await ok('the_hud_part_answers_link_words_and_the_top_line_is_drawn_always', () => {
  const r = rig();
  const hud = wireHud(r.vg);
  assert.equal(r.vg.page.querySelector('.vg-h-top .vg-h-l').textContent, 'Bifrost 3 \u00b7 USB \u00b7 96 kHz');
  assert.equal(hud.driver.shown('Top line'), 'Bifrost 3 \u00b7 USB \u00b7 96 kHzhost EQ on');
  assert.equal(hud.driver.linkWords(), null, 'none while the link is on');
  r.store.page().device.link = { word: 'lost', state: 'off', old: false };
  r.store.emit('device', { device: DAC.url });
  assert.equal(hud.driver.linkWords(), 'Bifrost 3 \u00b7 lost \u00b7 USB \u00b7 96 kHz');
  r.store.setState('lost');
  assert.match(r.vg.page.querySelector('.vg-h-r').textContent, /reconnecting/);
  r.control.abort();
});

await ok('a_volume_event_puts_the_volume_in_the_row_drawn_and_draws_nothing_again', () => {
  const r = rig();
  wireHud(r.vg);
  const row = r.vg.page.querySelector('[data-row="volume"]');
  const mod = r.vg.page.querySelector('[data-mod="dac"]');
  r.store.emit('volume', { device: DAC.url, raw: 150, text: '-12.0 dB' });
  assert.equal(r.vg.page.querySelector('[data-row="volume"]'), row, 'the same row element');
  assert.equal(r.vg.page.querySelector('[data-mod="dac"]'), mod);
  assert.equal(row.querySelector('.vg-h-v').textContent, '\u221212.0 dB');
  assert.equal(row.querySelector('.vg-h-slot').dataset.v, '0.7500');
  r.store.emit('volume', { device: DAC.url, muted: true });
  assert.equal(row.querySelector('.vg-h-v').textContent, '\u2212\u221e dB muted');
  assert.equal(row.dataset.tone, 'warn');
  r.control.abort();
});

await ok('the_panel_and_the_hud_unbind_on_the_signal_and_the_panel_reads_its_pin_pref', () => {
  const r = rig();
  r.vg.prefs.get = (name, fallback) => (name === 'panel-fades' ? false : fallback);
  // every listener either wire puts on the document carries the mount's signal
  const unsigned = [];
  const add = r.document.addEventListener.bind(r.document);
  r.document.addEventListener = (type, fn, opt) => {
    if (opt?.signal !== r.vg.signal) unsigned.push(type);
    return add(type, fn, opt);
  };
  const panel = wirePanel(r.vg);
  wireHud(r.vg);
  const history = new History();
  for (let i = 0; i < 24; i++) step(r, history, { dt: 0.5 });
  assert.equal(r.vg.page.dataset.pinned, '1', 'the pref has it pinned from the start');
  panel.setFades(true);
  assert.equal(r.vg.page.dataset.pinned, '0');
  assert.equal(opacity(r), 1, 'the chrome\u2019s fade is the style sheet\u2019s: the layer keeps its strength');
  assert.deepEqual(unsigned, [], 'no document listener is left without the signal');
  step(r, history, { dt: 0.1, events: [{ type: 'section', section: 'drop' }] });
  step(r, history, { dt: 0.5 });
  assert.equal(r.did.levels.length, r.did.closed + 1, 'the Host module is on: the levels are held');
  const frozen = opacity(r);
  r.control.abort();
  // after the abort: no step, no pointer and no visibility change reaches either
  const el = r.vg.page.querySelector('#vg-panel');
  const mods = [...r.vg.page.querySelectorAll('[data-mod]')];
  const before = mods.map((m) => m.style.opacity).join();
  r.document.dispatchEvent(new r.window.Event('pointermove'));
  step(r, history, { dt: 20 });
  assert.equal(opacity(r), frozen, 'the panel hears no step');
  step(r, history, { dt: 0.25, events: [{ type: 'section', section: 'drop' }] });
  assert.equal(mods.map((m) => m.style.opacity).join(), before, 'the HUD hears no step');
  const opened = r.did.levels.length;
  assert.equal(r.did.closed, opened, 'the hold is let go with the signal');
  r.document.dispatchEvent(new r.window.Event('visibilitychange'));
  assert.equal(r.did.levels.length, opened, 'no visibility change reaches the HUD');
  assert.equal(r.vg.page.querySelector('[data-mod="host"]').dataset.on, mods[0].dataset.on);
});

await ok('the_hud_listens_to_nothing_after_the_signal_aborts', () => {
  const r = rig();
  wireHud(r.vg);
  const history = new History();
  step(r, history, { dt: 0.25 });
  r.control.abort();
  assert.equal(r.did.closed, 1, 'the levels hold is let go');
  const before = r.vg.page.querySelector('[data-mod="host"]').style.opacity;
  step(r, history, { dt: 5 });
  assert.equal(r.vg.page.querySelector('[data-mod="host"]').style.opacity, before, 'no step reaches it');
  r.store.emit('volume', { device: DAC.url, raw: 10, text: '-90 dB' });
  assert.notEqual(r.vg.page.querySelector('[data-row="volume"] .vg-h-v').textContent, '−90 dB', 'no model event either');
});

// ------------------------------------------------------------------ the panel
await ok('the_panel_has_three_modes_and_tells_the_hud', () => {
  const r = rig();
  const panel = wirePanel(r.vg);
  assert.deepEqual([...MODES], ['panel', 'spread', 'off']);
  assert.equal(panel.mode(), 'panel');
  const heard = [];
  r.vg.steps.addEventListener('panel', (e) => heard.push(e.detail.mode));
  const el = r.vg.page.querySelector('#vg-panel');
  assert.equal(el.querySelectorAll('.vg-h-feat').length, 10);
  assert.equal(panel.driver.shown('audio panel'), 'Panel');
  assert.equal(panel.setMode('spread'), true);
  assert.equal(el.querySelector('.vg-h-pn').dataset.mode, 'spread');
  assert.equal(el.querySelector('[data-key="bass"]').dataset.form, 'col');
  assert.equal(el.querySelector('[data-key="loud"]').dataset.form, 'strip');
  assert.equal(el.querySelector('[data-key="cent"]').dataset.form, 'dial');
  assert.equal(panel.driver.shown('Audio panel'), 'Spread');
  assert.equal(panel.setMode('off'), true);
  assert.equal(el.hidden, true, 'Off draws nothing and leaves nothing for a reader');
  assert.equal(panel.setMode('sideways'), false);
  assert.equal(panel.driver.operate('audio panel', { mode: 'panel' }), true);
  assert.equal(el.hidden, false);
  assert.equal(panel.driver.operate('something else', { mode: 'panel' }), false);
  assert.deepEqual(heard, ['spread', 'off', 'panel']);
  r.control.abort();
});

await ok('the_panels_words_follow_the_features_and_its_lamps_the_cues', () => {
  const r = rig();
  const panel = wirePanel(r.vg);
  const history = new History();
  step(r, history, { features: { ...featureValues(0.2), bass: 0.624, cent: 1, flat: 0.62 }, raw: { peak: 1 }, cues: { rateHz: 44100, weak: true } });
  assert.equal(panel.driver.shown('Bass'), '062');
  assert.equal(panel.driver.shown('centroid'), '14kHz');
  assert.equal(panel.driver.shown('Flatness'), '0.62 N');
  const lamp = (name) => r.vg.page.querySelector(`[data-lamp="${name}"]`).dataset.on;
  assert.equal(lamp('over'), '1');
  assert.equal(lamp('link'), '1');
  assert.equal(r.vg.page.querySelector('.vg-h-rate').textContent, '44.1 kHz  SEED 0.00  Q —', 'the rate, the seed and the quality');
  for (let i = 0; i < 6; i++) step(r, history, { dt: 0.5, cues: { rateHz: 44100 } });
  assert.equal(lamp('over'), '0', 'the over lamp lets go');
  assert.equal(lamp('link'), '0');
  r.control.abort();
});

await ok('the_panel_keeps_its_strength_and_leaves_the_fade_to_the_pin_state_the_style_sheet_reads', () => {
  const r = rig();
  const panel = wirePanel(r.vg);
  const history = new History();
  step(r, history, { dt: 0.5 });
  assert.equal(r.vg.page.dataset.pinned, '0', 'unpinned: the chrome\u2019s fade takes it');
  for (let i = 0; i < 40; i++) step(r, history, { dt: 0.5 });
  assert.equal(opacity(r), 1, 'no clock of its own: 20 s of steps change nothing');
  panel.setFades(false);
  assert.equal(r.vg.page.dataset.pinned, '1', 'pinned from the menu');
  assert.equal(r.vg.page.querySelector('#vg-panel').dataset.pinned, '1');
  assert.ok(r.vg.page.querySelector('.vg-h-feat .vg-h-v').textContent !== '', 'the words stay in the page');
  panel.setFades(true);
  assert.equal(r.vg.page.dataset.pinned, '0');
  r.control.abort();
});

await ok('the_style_sheet_fades_the_panel_with_the_chrome_unless_pinned_and_only_by_opacity', () => {
  const css = readFileSync(new URL('../../../bins/svolderd/web/themes/voidgarden/style.css', import.meta.url), 'utf8');
  const rule = css.match(/#voidgarden\.vg-dim:not\(\[data-pinned="1"\]\) #vg-panel > \* \{([^}]*)\}/);
  assert.ok(rule, 'the panel has a fade rule that a pin turns off');
  assert.match(rule[1], /opacity: 0/);
  assert.match(rule[1], /transition: opacity \.6s/);
  assert.deepEqual(rule[1].split(';').map((d) => d.split(':')[0].trim()).filter((x) => x && x !== 'opacity' && x !== 'transition'), []);
});

await ok('the_drivers_mode_change_writes_the_pref_the_menu_row_reads', () => {
  const r = rig();
  const panel = wirePanel(r.vg);
  assert.equal(panel.driver.operate('Audio panel', { mode: 'spread' }), true);
  assert.deepEqual(r.did.prefs, [['panel', 'spread']]);
  assert.equal(panel.driver.operate('Audio panel', { mode: 'sideways' }), false);
  assert.equal(r.did.prefs.length, 1, 'a refused mode writes nothing');
  panel.setMode('off');
  assert.equal(r.did.prefs.length, 1, 'the theme\u2019s own setMode leaves the pref to its caller');
  r.control.abort();
});

await ok('a_muted_scene_holds_the_panel_lower', () => {
  const r = rig();
  wirePanel(r.vg);
  const history = new History();
  step(r, history, { dt: 0.25, cues: { rateHz: 96000, inputRateHz: 96000, mute: true } });
  assert.equal(opacity(r), 0.45);
  r.control.abort();
});

// ------------------------------------------------------------------ the painters
const ink = {
  fill: 'F', line: 'L', hair: 'H', gap: 'G', dim: 'D', warn: 'W', mid: 'M', ink: 'I', c0: 'C0', c1: 'C1', c2: 'C2', c3: 'C3',
};
const boxes = (root, sizes) => (el) => {
  if (el === root) return { left: 0, top: 0, width: 400, height: 300 };
  const s = sizes[el.dataset.id ?? el.dataset.trace] ?? sizes.default;
  return { left: s[0], top: s[1], width: s[2], height: s[3] };
};

await ok('the_hud_painter_fills_a_bar_to_its_value_and_skips_a_module_that_is_out', () => {
  const modules = modulesOf(full());
  const root = makePage(hudMarkup(modules)).document.body;
  root.querySelector('[data-id="volume"]').dataset.v = '0.5000';
  const rect = boxes(root, { default: [10, 20, 100, 5] });
  const c = recorder();
  paintHud(c, root, { sparks: { eq: sparkOf(curve()) }, link: { bars: 3, warn: false, points: [{ x: 0, y: 0.5 }, { x: 1, y: 0.7 }] }, alpha: { host: 1, dac: 1, amp: 0 }, ink }, { rect });
  const filled = c.calls.filter((k) => k[0] === 'fillRect' && k[1] === 10 && k[2] === 20 && k[4] === 5);
  assert.ok(filled.some((k) => k[3] === 50), 'the volume bar is half full');
  assert.ok(filled.some((k) => k[3] === 100), 'a track the full width');
  assert.equal(c.calls.filter((k) => k[0] === 'stroke').length > 0, true, 'the sparkline is a stroked path');
  const lines = c.calls.filter((k) => k[0] === 'lineTo');
  assert.ok(lines.length >= SPARK_POINTS - 1, 'the EQ line has its 64 values');
  const c2 = recorder();
  paintHud(c2, root, { sparks: {}, link: null, alpha: { host: 0, dac: 0, amp: 0 }, ink }, { rect });
  assert.equal(c2.calls.filter((k) => k[0] === 'fillRect').length, 0, 'every module out draws nothing');
  assert.equal(c2.calls[0][0], 'clearRect');
});

await ok('the_panel_painter_draws_each_feature_in_its_format', () => {
  const history = new History();
  for (let i = 0; i < 200; i++) history.push(featureValues(i % 7 / 7), 0.1);
  const data = {
    values: featureValues(0.5), peaks: Object.fromEntries(KEYS.map((k) => [k, 0.9])), series: Object.fromEntries(KEYS.map((k) => [k, history.series(k)])), seconds: 18, ink,
  };
  const draw = (mode) => {
    const root = makePage(panelMarkup(mode)).document.body;
    for (const t of root.querySelectorAll('.vg-h-tr')) t.getClientRects = () => [1];
    const c = recorder();
    paintPanel(c, root, data, { rect: boxes(root, { default: [10, 10, 160, 40] }) });
    return c;
  };
  const row = draw('panel');
  assert.equal(row.calls.filter((k) => k[0] === 'beginPath').length, 10, 'a history line for each of the ten');
  assert.ok(row.calls.filter((k) => k[0] === 'lineTo').length >= 10 * 143);
  const spread = draw('spread');
  assert.equal(spread.calls.filter((k) => k[0] === 'arc').length, 4, 'two dials: a track and a value arc each');
  assert.equal(spread.calls.filter((k) => k[0] === 'fill').length, 2, 'the two strips are filled');
  assert.ok(panelMarkup('panel').includes('aria-hidden="true"'), 'the traces are hidden from a reader');
  assert.ok(!panelMarkup('panel').includes('<canvas'), 'the canvas is the wire’s');
});

/** A recorder that notes every colour set too: `['fillStyle', v]` and `['strokeStyle', v]`. */
function styled() {
  const calls = [];
  const state = {};
  return new Proxy(state, {
    get: (t, k) => (k === 'calls' ? calls : (k in t ? t[k] : (...a) => { calls.push([k, ...a]); })),
    set: (t, k, v) => { if (k === 'fillStyle' || k === 'strokeStyle') calls.push([k, v]); t[k] = v; return true; },
  });
}

const barsink = { hair: 'H', gap: 'G', dim: 'D', mid: 'M', ink: 'I', c0: 'C0', c1: 'C1', c2: 'C2', c3: 'C3' };
const barsdata = (more = {}) => {
  const history = new History();
  for (let i = 0; i < 200; i++) history.push(featureValues(i % 7 / 7), 0.1);
  return {
    values: featureValues(0.5), peaks: Object.fromEntries(KEYS.map((k) => [k, 0.9])),
    series: Object.fromEntries(KEYS.map((k) => [k, history.series(k)])), seconds: 18, window: 18, ticks: [15, 10, 5], ink: barsink, ...more,
  };
};
const barspaint = (data, width = 310) => {
  const root = makePage(panelMarkup('panel')).document.body;
  for (const t of root.querySelectorAll('.vg-h-tr')) t.getClientRects = () => [1];
  const c = styled();
  paintPanel(c, root, data, { rect: boxes(root, { default: [0, 0, width, 15] }) });
  return c.calls;
};

await ok('the_panels_bar_is_24_segments_100_by_5_with_a_peak_hold_and_five_ticks', () => {
  const calls = barspaint(barsdata());
  const bar = (x, w, h) => calls.filter((k) => k[0] === 'fillRect' && k[1] === x && k[3] === w && k[4] === h);
  assert.equal(bar(0, 100, 5).length, 10, 'a track 100 by 5 for each of the ten');
  assert.equal(bar(0, 50, 5).length, 10, 'filled to the value');
  assert.equal(calls.filter((k) => k[0] === 'fillRect' && k[4] === 5 && k[3] === 1).length, 10 * 23, '23 gaps cut a bar into 24 segments');
  const peaks = calls.filter((k) => k[0] === 'fillRect' && k[3] === 2 && k[4] === 9);
  assert.equal(peaks.length, 10, 'a peak mark, 2 by 9, for each');
  assert.ok(peaks.every((k) => k[1] === Math.round(98 * 0.9)), 'where the hold is');
  const ticks = calls.filter((k) => k[0] === 'fillRect' && k[3] === 1 && k[2] === 8 && k[1] <= 99);
  assert.equal(ticks.length, 10 * 5, 'five ticks under each bar');
});

await ok('the_panels_traces_are_18_s_with_ticks_at_15_10_and_5_and_9_s_with_6_and_3_on_a_phone', () => {
  const lines = (calls) => calls.filter((k) => k[0] === 'lineTo').length;
  const desk = barspaint(barsdata());
  assert.equal(lines(desk), 10 * 143, 'the 144 values of 18 s');
  const phone = barspaint(barsdata({ window: 9, ticks: [6, 3] }), 306 - 100 - 50 + 150);
  assert.equal(lines(phone), 10 * 71, 'half of them for 9 s');
  const tickAt = (calls, w, t) => calls.filter((k) => k[0] === 'fillRect' && k[2] === 9 && k[3] === 1 && k[4] === 2
    && k[1] === 150 + Math.round((w - 150) * (1 - t / (calls === desk ? 18 : 9)))).length;
  for (const t of [15, 10, 5]) assert.equal(tickAt(desk, 310, t), 10, `-${t} s ticks on every row`);
  const wide = 306 - 100 - 50 + 150;
  for (const t of [6, 3]) assert.equal(tickAt(phone, wide, t), 10, `-${t} s ticks on every row of a phone`);
  const axis = (win) => panelMarkup('panel', {}, { phone: win }).match(/>-\d+S</g);
  assert.deepEqual(axis(false), ['>-15S<', '>-10S<', '>-5S<']);
  assert.deepEqual(axis(true), ['>-6S<', '>-3S<']);
  assert.match(panelMarkup('panel', {}, { phone: true }), /Audio features · 9 s</);
  assert.match(panelMarkup('panel', {}), /Audio features · live · 18 s history</);
  assert.ok(panelMarkup('panel').includes('>NOW<'));
});

await ok('the_panels_colours_are_the_scenes_live_palette_and_follow_it', () => {
  const base = { hair: 'H', dim: 'D', mid: 'M', ink: 'I' };
  const ink1 = panelInk(['#c6f25a', '#e86fb8'], base);
  for (const c of [ink1.c0, ink1.c1, ink1.c2]) assert.match(c, /^hsl\(\d+,\d+%,\d+%\)$/, 'the hsl accents');
  assert.equal(ink1.frame, '#e86fb8', 'the box\u2019s outline is the second colour');
  const ink2 = panelInk(['#7fe0ff', '#ff7a4d'], base);
  assert.notDeepEqual([ink1.c0, ink1.c1], [ink2.c0, ink2.c1], 'another palette, other colours');
  assert.equal(panelInk(['#7fe0ff', '#ff7a4d'], base).c0, ink2.c0, 'the same palette, the same colours');
  assert.equal(panelInk(null, base), base, 'no palette: the theme\u2019s own');
  const calls = barspaint(barsdata({ ink: { ...barsink, c0: '#aa0001', c1: '#aa0002', c2: '#aa0003', c3: 'I' } }));
  const used = new Set(calls.filter((k) => k[0] === 'fillStyle' || k[0] === 'strokeStyle').map((k) => k[1]));
  for (const c of ['#aa0001', '#aa0002', '#aa0003']) assert.ok(used.has(c), `${c} draws bars and traces`);
});

await ok('the_panels_three_accents_are_bright_and_differ_in_hue_as_r9_builds_them', () => {
  const base = { hair: 'H', dim: 'D', mid: 'M', ink: 'I' };
  // dark palette colours, as the scene's can be: the accents still come out 55, 63 and 71 % light
  for (const pair of [['#1b3a2a', '#3a1b2e'], ['#c6f25a', '#e86fb8'], ['#102040', '#402010']]) {
    const k = panelInk(pair, base);
    const parts = [k.c0, k.c1, k.c2].map((c) => /^hsl\((\d+),(\d+)%,(\d+)%\)$/.exec(c).slice(1).map(Number));
    assert.deepEqual(parts.map((p) => p[2]), [55, 63, 71]);
    assert.ok(parts.every((p) => p[1] >= 70), 'a high saturation');
    assert.equal(new Set(parts.map((p) => p[0])).size, 3, `three hues for ${pair}`);
  }
});

await ok('the_panels_bar_lights_its_cells_to_the_value_and_its_canvas_is_above_the_box_fill', () => {
  for (const v of [0.6, 0.25, 1, 0]) {
    const calls = barspaint(barsdata({ values: featureValues(v) }));
    // the first bar's track (100 wide) and then its fill, at the bar's row (y 2, 5 high)
    const row = calls.filter((k) => k[0] === 'fillRect' && k[1] === 0 && k[2] === 2 && k[4] === 5);
    assert.equal(row[0][3], 100);
    assert.equal(row[1][3], Math.round(100 * v), `${v} fills ${Math.round(100 * v)} % of the bar (${(24 * v).toFixed(1)} of 24 cells)`);
  }
  // the lit cells are drawn with the accent, whole, then the 1 px gaps: no alpha between
  const calls = barspaint(barsdata({ ink: { ...barsink, c0: '#ff8800' } }));
  assert.ok(calls.some((k, i) => k[0] === 'fillStyle' && k[1] === '#ff8800' && calls[i + 1]?.[0] === 'fillRect'));
  // the box's translucent fill is its markup's, and the traces' canvas stands above it
  const css = readFileSync(new URL('../../../bins/svolderd/web/themes/voidgarden/style.css', import.meta.url), 'utf8');
  const z = (sel) => Number(new RegExp(`${sel.replace(/[.]/g, '\\.')}[^{]*\\{[^}]*z-index: (\\d+)`).exec(css)?.[1]);
  assert.ok(z('#vg-panel .vg-h-pcv') > z('#vg-panel .vg-h-pbox'), 'the panel canvas is above the box');
  assert.ok(z('#vg-hud .vg-h-cv') > z('#vg-hud .vg-h-mods'), 'the HUD canvas is above its modules');
});

await ok('the_panels_figures_hold_4_5_to_1_over_a_white_sky_with_its_box_alpha', () => {
  const css = readFileSync(new URL('../../../bins/svolderd/web/themes/voidgarden/style.css', import.meta.url), 'utf8');
  const box = /#vg-panel \.vg-h-pn\[data-mode="panel"\] \{[^}]*background: rgba\((\d+), (\d+), (\d+), ([.\d]+)\)/.exec(css);
  const fig = /\.vg-h-v \{[^}]*color: rgba\((\d+), (\d+), (\d+), ([.\d]+)\)/.exec(css);
  assert.ok(box && fig, 'the box and the figure colours are in the sheet');
  const lin = (v) => { const c = v / 255; return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4; };
  const lum = (rgb) => 0.2126 * lin(rgb[0]) + 0.7152 * lin(rgb[1]) + 0.0722 * lin(rgb[2]);
  const over = (fg, a, bg) => fg.map((v, i) => v * a + bg[i] * (1 - a));
  const sky = over([3, 6, 10].map((_, i) => Number(box[i + 1])), Number(box[4]), [255, 255, 255]);
  const figure = over([fig[1], fig[2], fig[3]].map(Number), Number(fig[4]), sky);
  const ratio = (lum(figure) + 0.05) / (lum(sky) + 0.05);
  assert.ok(ratio >= 4.5, `figures over a white sky: ${ratio.toFixed(2)}`);
  assert.ok(Number(box[4]) <= 0.7, 'no darker than needed');
});

await ok('a_phones_panel_wire_draws_9_s_with_the_foot_in_full_and_a_wide_window_18_s', () => {
  const drawn = (phone) => {
    const r = rig();
    r.window.matchMedia = () => ({ matches: phone, addEventListener() {} });
    const proto = r.window.Element.prototype;
    const was = proto.getClientRects;
    proto.getClientRects = () => [1];
    try {
      wirePanel(r.vg);
      step(r, new History(), { seed: 59.12, cues: { rateHz: 44100, inputRateHz: 44100, mute: false } });
    } finally { proto.getClientRects = was; }
    const out = {
      lines: r.ctx.calls.filter((k) => k[0] === 'lineTo').length,
      title: r.vg.page.querySelector('.vg-h-title').textContent,
      foot: r.vg.page.querySelector('.vg-h-rate').textContent,
      lamps: [...r.vg.page.querySelectorAll('.vg-h-lamp')].map((l) => l.textContent),
    };
    r.control.abort();
    return out;
  };
  const phone = drawn(true);
  const wide = drawn(false);
  assert.ok(phone.lines < wide.lines, `fewer points for 9 s: ${phone.lines} against ${wide.lines}`);
  assert.equal(phone.title, 'Audio features · 9 s');
  assert.equal(phone.foot, '44.1kHz SEED 59.12 Q —', 'a phone keeps the whole foot, tighter');
  assert.equal(wide.foot, '44.1 kHz  SEED 59.12  Q —');
  assert.deepEqual(phone.lamps, ['Over', 'Link']);
});

await ok('on_a_phone_spread_leaves_the_module_above_the_dash_clear_of_the_width_row_and_never_below_96_px', async () => {
  const { readFileSync } = await import('node:fs');
  const css = readFileSync(new URL('../../../bins/svolderd/web/themes/voidgarden/style.css', import.meta.url), 'utf8');
  const phone = css.slice(css.indexOf('@media (max-width: 719px), (pointer: coarse) and (max-height: 500px) {\n  /* no top line on a phone'));
  const bound = /#vg-hud\[data-spread="1"\] \.vg-h-mod\[data-mod\] \{\s*max-height: max\((\d+)px, calc\(100% - var\(--vg-above\) - (\d+)px/.exec(phone);
  assert.ok(bound, 'the phone bounds the module in Spread, with a floor');
  const [floor, under] = [Number(bound[1]), Number(bound[2])];
  const width = /\[data-key="width"\] \{ top: calc\((\d+)px/.exec(phone);
  const ROW = 28; // the Width row's height, as the CSS comment states
  assert.ok(under >= Number(width[1]) + ROW, 'the bound is under the Width row');
  assert.ok(floor >= 96, 'a short window keeps a usable module');
  const box = (viewport, dash, content) => Math.min(content, Math.max(floor, viewport - (dash + 10) - under));
  // the module is bottom-anchored above the dash: its top is the viewport less the dash's offset and its box
  for (const dash of [72, 190, 202, 260]) {
    const top = 844 - (dash + 10) - box(844, dash, 286);
    assert.ok(top >= Number(width[1]) + ROW, `a dash of ${dash} px on 390x844 leaves the module under the Width row (top ${top})`);
  }
  assert.ok(box(844, 72, 286) < 286 || 844 - 82 - 286 >= under, 'a tall module is cut to the room, not the room to it');
  for (const [view, dash] of [[375, 72], [667, 200], [375, 190]]) {
    assert.ok(box(view, dash, 286) >= floor, `${view} px high with a ${dash} px dash keeps ${floor} px`);
  }
});

console.log(`voidgarden-hud: ${n} ok`);
process.exit(0);
