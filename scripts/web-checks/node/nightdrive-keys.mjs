/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// Node harness for the theme Night drive's one key of the whole page, `↑` and `↓`
// (themes/nightdrive/volume-keys.js), bound from its key table (themes/nightdrive/key-table.js) by
// the core's real binding (core/keys.js), as nightdrive.js does: a press steps the volume once and
// keeps the page from scrolling, the system's own repeat is no second press, nothing steps while a
// control has the focus, while something stands over the page, with a modifier or once another
// listener has taken the key, and the release ends the held key. A jsdom page (apt `node-jsdom`).
// Run: node nightdrive-keys.mjs
import assert from 'node:assert/strict';
import { page } from './dom.mjs';
import { createKeys } from '../../../bins/svolderd/web/core/keys.js';
import { KEYS } from '../../../bins/svolderd/web/themes/nightdrive/key-table.js';
import { wireVolumeKeys } from '../../../bins/svolderd/web/themes/nightdrive/volume-keys.js';

let n = 0;
const ok = (name, fn) => { fn(); n++; console.log('ok', name); };

/** Night drive's page with its volume keys wired and its table bound; `steps` holds the nudges. */
function rig() {
  const p = page('<div id="nightdrive"><button id="minus">−</button><input id="field"></div>');
  const { window, document } = p;
  globalThis.AbortController = window.AbortController;
  const steps = [];
  const over = { page: false };
  const mount = new window.AbortController();
  const volume = wireVolumeKeys({
    page: document.getElementById('nightdrive'), nudge: (delta) => steps.push(delta),
    writable: () => true, wheelSteps: () => ({ steps: 0, rest: 0 }), covered: () => over.page,
    signal: mount.signal,
  });
  const keys = createKeys({ document, base: () => ({ canWrite: true }) });
  const table = KEYS.map((entry) => (
    entry.id === 'volume' ? { ...entry, run: (event) => volume.key(event) } : entry));
  keys.bind(table, { signal: mount.signal });
  const press = (name, init = {}, on = document.body) => p.key(name, { on, ...init });
  const release = (name) => window.dispatchEvent(new window.KeyboardEvent('keyup', { key: name }));
  return {
    ...p, steps, over, press, release, volume,
    stop() { mount.abort(); window.close(); },
  };
}

ok('up_and_down_step_the_volume_once_a_press_and_keep_the_page_from_scrolling', () => {
  const r = rig();
  const up = r.press('ArrowUp');
  r.release('ArrowUp');
  const down = r.press('ArrowDown');
  r.release('ArrowDown');
  assert.deepEqual(r.steps, [1, -1]);
  assert.equal(up.defaultPrevented && down.defaultPrevented, true);
  r.press('ArrowLeft');
  r.press('ArrowRight');
  assert.equal(r.steps.length, 2, 'left and right are not read');
  r.stop();
});

ok('the_systems_own_repeat_is_no_second_press_and_the_release_ends_the_key', () => {
  const r = rig();
  r.press('ArrowUp');
  const again = r.press('ArrowUp', { repeat: true });
  const held = r.press('ArrowUp');
  assert.deepEqual(r.steps, [1], 'the page repeats at its own pace');
  assert.equal(again.defaultPrevented && held.defaultPrevented, true, 'a held arrow still does not scroll');
  r.release('ArrowUp');
  r.press('ArrowUp');
  assert.deepEqual(r.steps, [1, 1]);
  r.release('ArrowUp');
  r.stop();
});

ok('nothing_steps_while_a_control_has_the_focus_or_something_stands_over_the_page', () => {
  const r = rig();
  r.document.getElementById('minus').focus();
  const onButton = r.press('ArrowUp');
  r.document.getElementById('field').focus();
  r.press('ArrowUp');
  assert.deepEqual(r.steps, []);
  assert.equal(onButton.defaultPrevented, false, 'the key is the control\'s');
  r.document.activeElement.blur();
  r.over.page = true;
  const covered = r.press('ArrowUp');
  assert.deepEqual(r.steps, []);
  assert.equal(covered.defaultPrevented, false);
  r.over.page = false;
  r.press('ArrowUp');
  assert.deepEqual(r.steps, [1]);
  r.release('ArrowUp');
  r.stop();
});

ok('nothing_steps_with_a_modifier_or_once_another_listener_took_the_key', () => {
  const r = rig();
  for (const mods of [{ ctrlKey: true }, { altKey: true }, { metaKey: true }, { shiftKey: true }]) {
    const event = r.press('ArrowUp', mods);
    assert.equal(event.defaultPrevented, false, JSON.stringify(mods));
  }
  assert.deepEqual(r.steps, []);
  r.document.body.addEventListener('keydown', (event) => event.preventDefault(), { once: true });
  r.press('ArrowDown');
  assert.deepEqual(r.steps, [], 'a key already handled is left alone');
  r.stop();
});

ok('the_keys_go_with_the_mount', () => {
  const r = rig();
  r.press('ArrowUp');
  r.release('ArrowUp');
  const { document } = r;
  r.stop();
  document.body.dispatchEvent(new r.window.KeyboardEvent('keydown', { key: 'ArrowUp', bubbles: true }));
  assert.deepEqual(r.steps, [1]);
});

console.log(`nightdrive-keys.mjs: ${n} passed`);
