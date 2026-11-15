/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the Night drive dash's track time (themes/nightdrive/dash-wire.js): a draw that brings no new
// reading of the player (a tone change, a volume change) counts the time on from where it was, it never
// goes back to the last reading; a new reading re-bases it. The real dash over a jsdom page, the clock a variable.
// Run: node nightdrive-dash-clock.mjs
import assert from 'node:assert/strict';
import { page as domPage } from './dom.mjs';
import { wireDash } from '../../../bins/svolderd/web/themes/nightdrive/dash-wire.js';

const { window, document } = domPage('<div id="nightdrive"><span id="nd-switch"></span><div id="nd-dash"></div></div>');
Object.assign(globalThis, { document, window });
let t = 5000;
Date.now = () => t;

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

const player = (more = {}) => ({
  now_ms: 1_000_000,
  buttons: true,
  track: {
    status: 'playing', headline: 'T', artist: 'A', album: 'B', cover: null, length_us: 600e6,
    progress: { position_us: 0, rate_milli: 1000, read_at_ms: 1_000_000 },
  },
  ...more,
});
const modes = (mode) => ({
  mode, modes: ['flat', 'eq'].map((name) => ({ name, label: name, on: name === mode })),
});
const dev = (mode, pct, playing) => ({
  name: 'D', url: 'D', family: 'dac', inert: false, tone: modes(mode), playing,
  volume: { text: `${pct}`, unit: 'dB', muted: false, raw: pct, mute: 'mute' },
});
const pageOf = (device) => ({
  v: 1, viewer: { signed_in: true, guest: false, writes: true }, fleet: { devices: [{ url: 'D', family: 'dac' }], hops: [] },
  shown: 'D', device, host: null,
});

function stage(first) {
  let current = first;
  const controller = new window.AbortController();
  const core = {
    model: {
      page: () => current, device: () => current.device, shown: () => 'D', part: () => null, on: () => () => {},
    },
    computer: { view: () => null, side: () => 'device', on: () => () => {}, flip() {} },
    actions: { canWrite: () => true },
    eq: { bands: { wheelSteps: () => 0 } },
    frame: { covered: () => false },
    repeat: () => ({ end() {} }),
  };
  const live = { freeze() {}, thaw() {}, draw() {} };
  const view = { state: 'open', phone: () => false, asked: null };
  const wired = wireDash({
    root: document.getElementById('nightdrive').parentNode, core, live, act: { act: async () => ({}) }, view, signal: controller.signal,
  });
  return {
    draw(next) { current = next; wired.draw(next); },
    time: () => document.querySelector('.nd-time')?.textContent,
    stop: () => controller.abort(),
  };
}

await ok('a_tone_or_volume_redraw_keeps_counting_the_track_time', () => {
  t = 5000;
  const s = stage(pageOf(dev('eq', 40, player())));
  s.draw(pageOf(dev('eq', 40, player())));
  assert.match(s.time(), /^0:00/);
  t += 60_000;
  s.draw(pageOf(dev('flat', 40, player())));
  assert.match(s.time(), /^1:00/, 'a tone change does not take the time back to the last reading');
  assert.equal(document.getElementById('nd-eq-key').getAttribute('aria-pressed'), 'false', 'and the EQ key was drawn again');
  t += 5_000;
  s.draw(pageOf(dev('flat', 41, player())));
  assert.match(s.time(), /^1:05/, 'nor does a volume change');
  s.stop();
});

await ok('a_new_reading_of_the_player_re_bases_the_time', () => {
  t = 5000;
  const s = stage(pageOf(dev('eq', 40, player())));
  s.draw(pageOf(dev('eq', 40, player())));
  t += 60_000;
  const fresh = player({ now_ms: 1_060_000 });
  fresh.track.progress = { position_us: 90e6, rate_milli: 1000, read_at_ms: 1_060_000 };
  s.draw(pageOf(dev('eq', 40, fresh)));
  assert.match(s.time(), /^1:30/, 'the player said 1:30 at its own time');
  t += 10_000;
  s.draw(pageOf(dev('flat', 40, fresh)));
  assert.match(s.time(), /^1:40/);
  s.stop();
});

console.log(`${n} ok`);
process.exit(0);
