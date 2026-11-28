/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Void Garden's mount and its test driver (themes/voidgarden/
// voidgarden.js, driver.js) with the core's real key binding (core/keys.js): the skeleton the
// mount builds and its order (#vg-slot last), what `↑` and `↓` write on the device's side and
// the computer's, that they do nothing while a control has the focus or something covers the
// page, that the driver refuses before a mount, after it, and while no bound part answers,
// and what unmount undoes (the signal, the anchor, the driver). The core is a fake that is
// strict: reading a property it does not have throws, so a misspelt `core.computer.side` fails
// here and is not swallowed as the key table's warning. A jsdom page (apt `node-jsdom`).
// Run: node voidgarden-mount.mjs
import assert from 'node:assert/strict';
import { page } from './dom.mjs';
import * as freeze from '../../../bins/svolderd/web/core/freeze.js';
import { createKeys } from '../../../bins/svolderd/web/core/keys.js';
import { bindDriver } from '../../../bins/svolderd/web/themes/voidgarden/driver.js';
import { theme } from '../../../bins/svolderd/web/themes/voidgarden/voidgarden.js';

let n = 0;
const ok = async (name, fn) => { await fn(); n++; console.log('ok', name); };

/** `target` whose every read of a name it lacks throws (nested objects are strict too). */
function strict(label, target) {
  return new Proxy(target, {
    get(obj, name, receiver) {
      if (typeof name === 'symbol' || name === 'then' || name === 'toJSON') return Reflect.get(obj, name, receiver);
      if (!(name in obj)) throw new Error(`the fake core has no ${label}.${String(name)}`);
      return obj[name];
    },
  });
}

/** A fake core over a page, with what the mount did noted in `did`. */
function rig({ side = 'device', raw = 40 } = {}) {
  const p = page('<button id="menu">Menu</button><div id="root"></div>');
  const { window, document } = p;
  globalThis.window = window;
  globalThis.document = document;
  globalThis.AbortController = window.AbortController;
  const did = [];
  const state = { side, raw, covered: false };
  const volume = { raw: 0, lo: 0, hi: 100 };
  const core = strict('core', {
    prefs: {},
    computer: strict('computer', {
      side: () => state.side, view: () => null, on: () => () => {}, toneOf: () => null,
    }),
    model: strict('model', {
      part: (name) => (name === 'volume' ? { ...volume, raw: state.raw } : null),
      shown: () => 'dac',
      page: () => ({}),
      state: () => 'open',
      on: () => () => {},
      device: () => null,
    }),
    freeze,
    actions: strict('actions', {
      canWrite: () => true,
      setVolume: (value, options) => { did.push(['setVolume', value, options.device]); return Promise.resolve(); },
      computer: strict('computer', {
        step: (delta) => { did.push(['computer.step', delta]); return Promise.resolve(); },
      }),
    }),
    lifecycle: strict('lifecycle', {
      canvasLoop: () => ({ start() {}, stop() {} }),
      watchVisible: () => ({ stop() {} }),
    }),
    analysis: strict('analysis', { open: () => ({ close() {} }) }),
    levels: strict('levels', { open: () => ({ close() {} }) }),
    router: strict('router', { open: () => {} }),
    frame: strict('frame', {
      anchor: (el, options) => did.push(['anchor', el?.id ?? null, options?.fixed ?? null]),
      covered: () => state.covered,
      menuOwns: () => false,
      fade: (on, ms) => did.push(['fade', on, ms]),
    }),
    keys: createKeys({ document, base: () => ({ canWrite: true }) }),
  });
  const root = document.getElementById('root');
  return { ...p, did, state, core, root, press: (name, init = {}) => p.key(name, { on: document.body, ...init }) };
}

await ok('the_mount_builds_the_skeleton_with_the_slot_last', async () => {
  const r = rig();
  const mounted = theme.mount(r.root, r.core);
  const page = r.root.firstElementChild;
  assert.equal(page.id, 'voidgarden');
  assert.deepEqual([...page.children].map((c) => c.id),
    ['vg-scene', 'vg-hud', 'vg-panel', 'vg-dash', 'vg-card', 'vg-slot']);
  assert.equal(page.querySelector('#vg-card').hidden, true);
  assert.deepEqual(r.did, [['fade', true, 3000], ['anchor', 'vg-slot', true]], 'the frame\u2019s button fades with the chrome, 3 s');
  mounted.unmount();
});

await ok('the_page_holds_the_analysis_once_and_its_capture_reaches_the_dash', async () => {
  const r = rig();
  let opened = 0;
  const held = [];
  const watchers = [];
  r.core.analysis = strict('analysis', { open: (l) => { opened++; held.push(l); return { close() {} }; } });
  r.core.lifecycle = strict('lifecycle', {
    canvasLoop: (o) => ({ start() { o.watchVisible({}); }, stop() {} }),
    watchVisible: (o) => { watchers.push(o); return { stop() {} }; },
  });
  const mounted = theme.mount(r.root, r.core);
  watchers.forEach((w) => w.onShow?.());
  assert.equal(opened, 1, 'the scene is the page\'s one holder');
  held[0].capture({ capture: 'unavailable', why: 'no sink' });
  assert.equal(r.root.querySelector('.vg-line').textContent, 'no music analysis: no sink');
  mounted.unmount();
});

await ok('up_and_down_step_the_dac_by_one_from_its_reading', async () => {
  const r = rig();
  const mounted = theme.mount(r.root, r.core);
  const up = r.press('ArrowUp');
  r.press('ArrowDown');
  assert.deepEqual(r.did.slice(2), [['setVolume', 41, 'dac'], ['setVolume', 39, 'dac']]);
  assert.equal(up.defaultPrevented, true, 'the page does not scroll');
  mounted.unmount();
});

await ok('on_the_computers_side_down_steps_the_computer', async () => {
  const r = rig({ side: 'computer' });
  const mounted = theme.mount(r.root, r.core);
  r.press('ArrowDown');
  r.press('ArrowUp');
  assert.deepEqual(r.did.slice(2), [['computer.step', -1], ['computer.step', 1]]);
  mounted.unmount();
});

await ok('the_dac_step_is_held_to_its_range_and_a_step_that_changes_nothing_is_not_sent', async () => {
  const r = rig({ raw: 100 });
  const mounted = theme.mount(r.root, r.core);
  r.press('ArrowUp');
  r.state.raw = 0;
  r.press('ArrowDown');
  assert.deepEqual(r.did.slice(2), [], 'at the ends of the range nothing goes out');
  r.state.raw = 99;
  r.press('ArrowUp');
  assert.deepEqual(r.did.slice(2), [['setVolume', 100, 'dac']]);
  mounted.unmount();
});

await ok('the_arrows_do_nothing_while_a_control_has_the_focus_or_the_page_is_covered', async () => {
  const r = rig();
  const mounted = theme.mount(r.root, r.core);
  r.document.getElementById('menu').focus();
  const event = r.press('ArrowUp', { on: r.document.activeElement });
  assert.equal(event.defaultPrevented, false);
  r.document.activeElement.blur();
  r.state.covered = true;
  r.press('ArrowDown');
  assert.deepEqual(r.did.slice(2), [], 'nothing was written');
  r.state.covered = false;
  r.press('ArrowDown');
  assert.equal(r.did.length, 3, 'free again');
  mounted.unmount();
});

await ok('the_driver_refuses_before_a_mount_after_it_and_while_no_part_answers', async () => {
  const refuses = async () => {
    const d = theme.driver;
    assert.equal(await d.reach('x'), false);
    assert.equal(await d.operate('x', {}), false);
    for (const call of [() => d.shown('x'), () => d.state(), () => d.linkWords(), () => d.tier(),
      () => d.frame(1, 0), () => d.pixels(), () => d.perf()]) assert.equal(call(), null);
    assert.equal(d.forceDrawMs(5), false);
  };
  await refuses();
  const r = rig();
  const mounted = theme.mount(r.root, r.core);
  bindDriver([{ driver: {} }, { driver: {} }]);
  await refuses(); // parts are bound and answer nothing
  mounted.unmount();
  await refuses();
});

await ok('unmount_aborts_the_signal_unbinds_the_driver_and_gives_back_the_anchor', async () => {
  const r = rig();
  const mounted = theme.mount(r.root, r.core);
  mounted.unmount();
  assert.deepEqual(r.did, [['fade', true, 3000], ['anchor', 'vg-slot', true], ['fade', false, undefined], ['anchor', null, null]]);
  assert.equal(r.root.children.length, 0);
  r.press('ArrowUp');
  assert.equal(r.did.length, 4, 'the key is unbound with the signal');
});

await ok('the_driver_forwards_the_scenes_calls_to_the_bound_part_and_unmount_unbinds_it', async () => {
  const r = rig();
  const mounted = theme.mount(r.root, r.core);
  const seen = [];
  bindDriver([{ driver: {} }, { driver: {
    tier: () => 'webgpu',
    frame: (seed, t) => { seen.push([seed, t]); return 'frame'; },
    pixels: () => [1, 2],
    perf: () => ({ ms: 3 }),
    forceDrawMs: (ms) => { seen.push(ms); return true; },
  } }]);
  const d = theme.driver;
  assert.equal(d.tier(), 'webgpu');
  assert.equal(d.frame(7, 0.5), 'frame');
  assert.deepEqual(d.pixels(), [1, 2]);
  assert.deepEqual(d.perf(), { ms: 3 });
  assert.equal(d.forceDrawMs(40), true);
  assert.deepEqual(seen, [[7, 0.5], 40]);
  mounted.unmount();
  assert.equal(d.tier(), null, 'unmount unbinds');
  assert.equal(d.forceDrawMs(40), false);
});

console.log(`voidgarden-mount: ${n} ok`);
process.exit(0);
