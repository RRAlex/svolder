/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The theme Ember: warm, with smoke and glow, drawn in the browser from the core's model. Its
// mount draws the shell and the screen (draw/), installs what makes them work and follows the
// store (live.js); its unmount takes them away. No inline script, no eval: htmx is the swap engine
// for the screen and its parts, configured from a meta element in the head (the theme's data in
// themes.rs); every handler is attached here or in a module, and goes with the mount's signal:
//   press.js       a press on a control, a preset or a slot, and the presets' forms, as core actions
//   live.js        the store's events on the page: the swaps, the readings, what a swap keeps
//   strip.js       the strip along the bottom
//   tone-motion.js the tone curve's easing and flashes
//   tone-editor.js the steppers that repeat while held, and the band editor
//   tone-graph.js  the graph's points under the mouse: drag, right drag and wheel
//   keys.js        the desktop keys and the key list
//   facts.js       the Device fold-out: its open state, its ages and the Identify button
//   playing.js     the Now playing card: its time and bar, the transport and the phone's line
//   visualizer.js  the visualizer card: the canvas, the looks, the frames stream
//   effects.js     the layer behind it all, and the rows it gives the frame's menu; knob.js,
//                  slider.js, meters.js the components
//   keep.js        what is kept across a swap while a part is in use
//   arrange.js     Arrange mode: the sections moved between the band and the columns; order.js
//                  and sections.js hold the order and where the sections stand
//   preview.js     the picture of the theme in the frame's menu and picker
//   driver.js      the test driver the contract and completeness checks drive

import './knob.js';
import './slider.js';
import './meters.js';
import { registerTheme } from '../../core/registry.js';
import { ember, mine, orderOf, shown } from './bind.js';
import { screen } from './draw/screen.js';
import { shell } from './draw/shell.js';
import { SECTIONS, canArrange, endArrange, openArrange } from './arrange.js';
import { installLive, reconnectedLately } from './live.js';
import { installPress } from './press.js';
import { startStrip, stopStrip, strip } from './strip.js';
import { installToneMotion, once } from './tone-motion.js';
import { installToneEditor, syncEditor } from './tone-editor.js';
import { endDrag, installToneGraph } from './tone-graph.js';
import { installKeys } from './keys.js';
import { installFacts, syncFacts } from './facts.js';
import { installPlaying, syncPlaying } from './playing.js';
import {
  hasBand, installVisualizer, isOn, setOn, syncVisualizer,
} from './visualizer.js';
import * as effects from './effects.js';
import { clear, reset } from './keep.js';
import { preview } from './preview.js';
import { driver } from './driver.js';

const clamp = (v, lo, hi) => Math.min(hi, Math.max(lo, v));

/** A screen is in place: the effects take its glow level, its away look and where its dial or
 * its meters sit. */
function effectsFor() {
  const view = document.querySelector('#screen .view');
  effects.level(Number(view?.dataset.glow));
  effects.away(view?.classList.contains('away'));
  effects.layout();
}

let retryTotal = 1;
let retryLeft = 0;

/**
 * The card's countdown bar fills as the seconds to the next connect attempt run out. The stream
 * sends the card again each second with the seconds left; a number above the last starts a
 * new wait, whose first number is its length.
 */
function countdowns() {
  const bar = document.querySelector('.countdown[data-secs]');
  if (!bar) {
    retryLeft = 0;
    return;
  }
  const left = Number(bar.dataset.secs);
  if (left > retryLeft || left > retryTotal) retryTotal = Math.max(1, left);
  retryLeft = left;
  const done = clamp((retryTotal - left) / retryTotal, 0, 1);
  bar.style.setProperty('--rp', `${(done * 100).toFixed(1)}%`);
}

/** The device whose bars the strip holds, by its URL segment. */
let stripOf = null;

/** A screen is in place (first paint, a switch, a history step): follow the device it shows. A
 * redrawn screen of the same device keeps the history the strip has gathered. */
function follow() {
  const device = shown();
  for (const choice of document.querySelectorAll('.devs .dev[data-device]')) {
    if (choice.dataset.device === device) choice.setAttribute('aria-current', 'page');
    else choice.removeAttribute('aria-current');
  }
  effectsFor();
  countdowns();
  syncEditor();
  syncFacts();
  syncPlaying();
  syncVisualizer();
  ember.core?.frame.realign();
  if (stripOf !== device) {
    strip.load();
    stripOf = device;
  }
}

/**
 * The knob, the meters, the sliders and the band editor are kept across a swap by their ids, and
 * the knob's and the meters' ids are the same on every device. Before any swap that brings in a
 * whole screen (a device switch, a history step, a new shape) the ones in place lose their ids, so
 * the incoming screen's come in whole and no device's parts stand in another's.
 */
function dropPreserved() {
  if (endArrange()) ember.core?.frame.toast('The screen changed, so arranging stopped.');
  endDrag();
  clear();
  for (const kept of document.querySelectorAll('[hx-preserve]')) kept.removeAttribute('id');
}

/** Mount Ember on `root`: draw it, wire it, follow the model; returns the way to unmount. */
function mount(root, core, { signal }) {
  ember.core = core;
  ember.signal = signal;
  const page = core.model.page();
  root.innerHTML = shell(page, document.getElementById('svolder-wordmark')?.innerHTML ?? '');
  document.getElementById('screen').innerHTML = screen(page, orderOf(page.device));
  stripOf = null;

  const live = installLive(core, { beforeScreen: dropPreserved, screenIn: follow }, signal);
  const body = document.body;
  body.addEventListener('htmx:oobAfterSwap', (event) => {
    if (event.detail.target?.id === 'part-card') countdowns();
    if (event.detail.target?.id === 'part-link' && reconnectedLately()) {
      once(document.querySelector('#part-link i'), 'ring');
    }
  }, mine());
  installToneMotion();
  installPress({ readWaiting: live.readWaiting });
  installToneEditor({ readWaiting: live.readWaiting });
  installToneGraph({ readWaiting: live.readWaiting });
  installKeys();
  installFacts();
  installPlaying();
  installVisualizer();
  effects.switchRow('viz', { label: 'Visualizer', get: isOn, set: setOn, available: hasBand });

  // A device chosen in the switcher is shown without loading the page again.
  document.addEventListener('click', (event) => {
    const choice = event.target.closest?.('.devs a.dev[data-device]');
    if (!choice || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey
      || event.altKey) return;
    event.preventDefault();
    core.actions.showDevice(choice.dataset.device);
  }, mine());

  // The effects are decoration: whatever they throw, the stream, the knob and the keys still
  // start; the layer then shows the glow.
  try {
    effects.init({ say: core.frame.toast, changed: () => core.frame.realign(), signal });
  } catch (error) {
    console.warn('svolder: the effects threw while setting up, so the page shows Glow', error);
    const layer = document.querySelector('.fx');
    if (layer && !layer.dataset.level) layer.dataset.level = 'glow';
  }
  // The frame's button sits over the empty slot beside the link pill.
  core.frame.anchor(document.querySelector('.fxslot'));
  follow();
  startStrip();
  // What the mount registered and holds goes with it, once every listener of its signal has run.
  signal.addEventListener('abort', () => {
    effects.removeSwitchRow('viz');
    ember.core = null;
    ember.signal = null;
  });

  return function unmount() {
    live.stop();
    stopStrip();
    reset();
    root.replaceChildren();
  };
}

/** The rows Ember gives the frame's menu: the effects and the visualizer's switch, then the way
 * into Arrange mode. */
function menuRows() {
  return [...effects.menuRows(), { kind: 'sep' }, {
    kind: 'action',
    id: 'arrange',
    label: 'Arrange sections…',
    key: 'A',
    elementId: 'menu-arrange',
    available: canArrange,
    run: () => {
      openArrange();
    },
  }];
}

/** The theme as the registry lists it: it asks the daemon for the meters and the computer's player,
 * and draws the analysis of the music; its menu button is the waves, and its rows in the frame's
 * menu are the effects, the visualizer's switch and the way into Arrange mode; its `sections` are
 * the ones Arrange moves; its `driver` is the test driver of the contract checks. */
export const theme = {
  id: 'ember',
  label: 'Ember',
  needs: ['meters', 'host', 'analysis'],
  schemes: ['dark'],
  menuButton: { icon: 'waves', label: 'Effects' },
  menuRows,
  menuOpening: effects.opening,
  sections: SECTIONS,
  preview,
  driver,
  mount,
};

registerTheme(theme);
