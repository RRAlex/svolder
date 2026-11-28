/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme Void Garden: a night garden that grows with the music. This module registers the
// theme and mounts its page: a canvas for the scene and the layers over it (the HUD, the audio
// panel, the dash and the card of a device out of reach), each worked by its own wire. It is
// drawn in the browser from the core's model and changes a device only through the core's
// actions. Evaluating this module touches nothing in the page; it imports nothing from vendor/.
//
//   scene-wire.js  the loop that feeds the scene and sends a `step` on `vg.steps` after each step
//   hud-wire.js, panel-wire.js, dash-wire.js   the HUD, the audio panel and the dash
//   driver.js      what the checks reach into; the parts' drivers are bound there while mounted
//   key-table.js   the keys the theme answers to (`↑` `↓` the volume, `A` the audio panel, `S` `N` `E`
//                  the explore keys)
//   states.js      how the page stands and the values the menu's rows keep; debug.js, debug-wire.js
//                  the readout and the bisect switches the address asks for (`?vg-debug`)
//   #vg-slot       where the frame's menu button sits: the page's top right
//   preview.js     the picture of the theme in the frame's menu and picker

import { REPEAT_AFTER_MS, REPEAT_CAP_MS, REPEAT_EVERY_MS } from '../../core/repeat.js';
import { registerTheme } from '../../core/registry.js';
import { wireDash } from './dash-wire.js';
import { ASKED, withSwitches } from './debug.js';
import { wireDebug } from './debug-wire.js';
import { bindDriver, driver } from './driver.js';
import { wireHud } from './hud-wire.js';
import { KEYS, ZONES } from './key-table.js';
import { createLayers } from './layers.js';
import { motionOf } from './motion.js';
import { wirePanel } from './panel-wire.js';
import { preview } from './preview.js';
import { wireScene } from './scene-wire.js';
import {
  CHOICES, CHOICE_WORDS, ROW_WORDS, choiceOf, nextPanel, panelFades, withMemory,
} from './states.js';

/** The kinds of the explore keys, by the key table's ids. */
const EXPLORE_KEYS = {
  'explore-seed': 'seed', 'explore-scene': 'scene', 'explore-effect': 'effect',
};

const SKELETON = '<div id="voidgarden"><canvas id="vg-scene" aria-hidden="true"></canvas>'
  + '<div id="vg-hud"></div><div id="vg-panel"></div><div id="vg-dash"></div>'
  + '<div id="vg-card" hidden></div><span id="vg-slot"></span></div>';

const ARROWS = { ArrowUp: 1, ArrowDown: -1 };

/**
 * The page's volume keys `↑` and `↓` on `doc`: a press steps once through `nudge(delta)` and a key
 * held steps again after the core's pause at the core's pace, for the core's longest. `writable()`
 * says whether a held key may go on; `covered()` is the frame's (something stands over the page).
 * Nothing steps while a control has the focus or the page is covered. Returns
 * `{ key(event), end() }`:
 * `key` is what a press of the table's `volume` entry does, `end` ends a key held down.
 */
export function wireVolumeKeys({
  doc, nudge, writable, covered, now, signal,
}) {
  let held = null;
  let first = 0;
  let every = 0;

  /** Nothing to step the volume for from the keys: a control has the focus, or `covered`. */
  const busy = () => {
    const at = doc.activeElement;
    if (at && at !== doc.body && at !== doc.documentElement) return true;
    return covered();
  };

  function end() {
    clearTimeout(first);
    clearInterval(every);
    held = null;
  }

  function key(event) {
    const delta = ARROWS[event.key];
    if (delta === undefined || event.shiftKey || event.defaultPrevented) return;
    if (busy()) return;
    event.preventDefault();
    // the system's own repeat is not a second press: the page repeats at its own pace
    if (event.repeat || held === event.key) return;
    end();
    held = event.key;
    nudge(delta);
    first = setTimeout(() => {
      const began = now();
      every = setInterval(() => {
        if (!writable() || doc.hidden || busy() || now() - began > REPEAT_CAP_MS) end();
        else nudge(delta);
      }, REPEAT_EVERY_MS);
    }, REPEAT_AFTER_MS);
  }
  const win = doc.defaultView;
  win.addEventListener('keyup', (event) => {
    if (event.key === held) end();
  }, { signal, capture: true });
  win.addEventListener('blur', end, { signal });
  doc.addEventListener('visibilitychange', () => { if (doc.hidden) end(); }, { signal });
  signal.addEventListener('abort', end, { once: true });
  return { key, end };
}

/**
 * The rows a mount of Void Garden gives the frame's menu: the Motion levels, the Audio panel's
 * modes, whether the panel is pinned, the Detail levels and the Renderer, after `Device controls…`.
 * `prefs` is the mount's (kept
 * in the browser and, where it refuses, for the session), `reduced()` whether the system asks for
 * less motion (the Motion level then starts on Calm), `panel` the mount's panel setting
 * (`{ mode(), set(mode), fades(), setFades(on) }`) and `signal` the mount's: a row does nothing
 * once the mount has gone.
 */
export function menuRows({
  prefs, reduced = () => false, panel, signal, controls = () => {},
}) {
  const radios = (group, current, choose) => CHOICES[group].map((id) => ({
    kind: 'radio',
    id: `vg-${group}-${id}`,
    label: CHOICE_WORDS[group][id],
    dataset: { [group]: id },
    checked: () => !signal.aborted && current() === id,
    disabled: () => null,
    choose() {
      if (!signal.aborted) choose(id);
    },
  }));
  const keep = (group) => (id) => prefs.set(group, id);
  return [
    {
      kind: 'action',
      id: 'vg-controls',
      label: ROW_WORDS.controls,
      elementId: 'menu-vg-controls',
      available: () => !signal.aborted,
      run: () => {
        if (!signal.aborted) controls();
      },
    },
    { kind: 'sep' },
    { kind: 'heading', text: ROW_WORDS.motion },
    ...radios('motion', () => motionOf(prefs, reduced()), keep('motion')),
    { kind: 'sep' },
    { kind: 'heading', text: ROW_WORDS.panel },
    ...radios('panel', panel.mode, (id) => panel.set(id)),
    {
      kind: 'check',
      id: 'vg-panel-pin',
      label: ROW_WORDS.fades,
      dataset: { switch: 'panel-pin' },
      checked: () => !signal.aborted && !panel.fades(),
      available: () => !signal.aborted,
      flip: () => {
        if (!signal.aborted) panel.setFades(!panel.fades());
      },
    },
    { kind: 'sep' },
    { kind: 'heading', text: ROW_WORDS.detail },
    ...radios('detail', () => choiceOf(prefs, 'detail'), keep('detail')),
    { kind: 'sep' },
    { kind: 'heading', text: ROW_WORDS.renderer },
    ...radios('renderer', () => choiceOf(prefs, 'renderer'), keep('renderer')),
  ];
}

/** Mount Void Garden on `root`; returns the mounted instance (the way to unmount, the rows this
 * mount gives the frame's menu and the theme's key table). */
function mount(root, core, { signal: outer } = {}) {
  root.innerHTML = SKELETON;
  const control = new AbortController();
  outer?.addEventListener('abort', () => control.abort(), { once: true });
  // what a row chose stands for the session even where the browser keeps nothing
  const kept = core.prefs;
  const prefs = withMemory(kept);
  core.prefs = prefs;
  const vg = {
    page: root.querySelector('#voidgarden'),
    core,
    signal: control.signal,
    prefs,
    steps: new EventTarget(),
  };
  // the page's own clock, handed to the hands that count real time (held keys, the idle fade)
  const clock = () => root.ownerDocument.defaultView.performance.now();
  // the address's bisect switches turn layers of the scene off through the scene's own seam
  const makeLayers = withSwitches(createLayers, ASKED);
  const parts = [
    wireScene(vg, { makeLayers }), wireHud(vg), wirePanel(vg), wireDash(vg, { now: clock }),
  ];
  const [scene, , panelPart, dash] = parts;
  const volumeKeys = wireVolumeKeys({
    doc: root.ownerDocument,
    nudge: dash.nudge,
    writable: dash.writable,
    covered: () => core.frame.covered(),
    now: clock,
    signal: vg.signal,
  });
  dash.onEnd(volumeKeys.end);
  vg.perf = () => scene.driver.perf();
  bindDriver(parts);
  const panel = {
    mode: () => choiceOf(prefs, 'panel'),
    set(mode) {
      prefs.set('panel', mode);
      panelPart.setMode?.(mode);
    },
    fades: () => panelFades(prefs),
    setFades(on) {
      prefs.set('panel-fades', on);
      panelPart.setFades?.(on);
    },
  };
  panelPart.setMode?.(panel.mode());
  panelPart.setFades?.(panel.fades());
  const readout = ASKED && wireDebug({
    page: vg.page, asked: ASKED, perf: () => scene.driver.perf(),
    canvas: vg.page.querySelector('#vg-scene'),
    clock,
  });
  core.frame.anchor(root.querySelector('#vg-slot'), { fixed: true });
  const win = root.ownerDocument.defaultView;
  const reduced = () => Boolean(win?.matchMedia?.('(prefers-reduced-motion: reduce)').matches);
  const keys = KEYS.map((entry) => {
    if (entry.id === 'volume') return { ...entry, run: (event) => volumeKeys.key(event) };
    if (entry.id === 'panel') return { ...entry, run: () => panel.set(nextPanel(panel.mode())) };
    if (EXPLORE_KEYS[entry.id]) {
      return {
        ...entry,
        run: () => vg.steps.dispatchEvent(new globalThis.CustomEvent('explore', {
          detail: { kind: EXPLORE_KEYS[entry.id] },
        })),
      };
    }
    return entry;
  });
  core.keys.bind(keys, { signal: vg.signal });
  return {
    menuRows: () => menuRows({
      prefs, reduced, panel, signal: vg.signal, controls: () => core.router.open('controls'),
    }),
    keys,
    zones: ZONES,
    unmount() {
      bindDriver();
      control.abort();
      readout?.stop();
      core.frame.anchor(null);
      if (core.prefs === prefs) core.prefs = kept;
      root.replaceChildren();
    },
  };
}

/** The theme as the registry lists it: it asks the daemon for the meters, the computer's player and
 * the music's analysis; dark only; its menu button is the frame's own at the top right. */
export const theme = {
  id: 'voidgarden',
  label: 'Void Garden',
  needs: ['meters', 'host', 'analysis'],
  schemes: ['dark'],
  menuButton: { icon: 'waves', label: 'Menu' },
  preview,
  driver,
  mount,
};

registerTheme(theme);
