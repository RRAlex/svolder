/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// What the core's store says, put on the theme Ember's page: a model that changes the screen's
// shape draws the screen anew, any other model draws every part again, a part that changed is
// drawn again by itself and the readings (volume, levels, meters, strip) go to the components.
// The drawing is draw/'s; the swap is htmx's: a part is swapped by its id out of band, and the
// knob, the sliders, the meters, the band editor and Try now are kept across it.
//
// A part is not swapped while a slider in it is dragged, a stepper held, a curve point dragged or
// its writes unanswered (keep.js), while a form of the presets' lists is open, or while a request
// of a device's own presets is unanswered; it is drawn from the newest copy the store holds once
// that ends. The first volume after a load, a screen or a reconnect is not a step, and a link
// that comes back says so once.

import * as effects from './effects.js';
import { PARTS } from './draw/parts.js';
import { screen as drawScreen } from './draw/screen.js';
import { effective } from './draw/computer.js';
import { computerView } from './computer-view.js';
import {
  clear, drawNewest, holds, refocus, release, remember, requested,
} from './keep.js';
import { once } from './tone-motion.js';
import { strip } from './strip.js';
import { mine, orderOf, shown } from './bind.js';

/** The parts a stream snapshot draws again, by the model's names: those a swap by id reaches (the
 * volume and the meters are components, kept in place and read by their events). */
const SWAPPED = ['link', 'card', 'controls', 'power', 'tone', 'presets', 'device_presets', 'facts',
  'playing'];

/** The parts the computer's side draws from its own model, not the device's. */
const COMPUTER_PARTS = ['tone', 'presets'];

/** The preset lists, whose swap waits while one of their forms is open. */
const PRESET_LISTS = ['part-presets', 'part-device-presets'];

/** How long after the link comes back a freshly drawn link dot still rings, in milliseconds. */
const RING_MS = 1500;

/** The id of the part the model calls `name`. */
const idOf = (name) => `part-${name.replace(/_/g, '-')}`;

/** The swap engine, loaded by the page's head. */
const htmx = () => globalThis.htmx;

let lastVolume = null;
const burst = new effects.VolumeBurst();
let reconnectedAt = -Infinity;
let refocusing = null;
let presetFocus = null;

/** Whether the link came back a moment ago (the dot rings once for it, see `linkBack`). */
export const reconnectedLately = () => performance.now() - reconnectedAt < RING_MS;

/** Whether a use of the page's own holds the swap of the part `id`: a form of a preset list is
 * open (and not being sent), or a request of the device's own presets has no answer yet. */
function ownHold(id) {
  if (PRESET_LISTS.includes(id)
    && document.getElementById(id)?.querySelector('.pform:not([hidden]):not([data-sending])')) {
    return true;
  }
  // a value typed or a type being chosen in the computer's table is not swapped away
  if (id === 'part-tone') {
    return Boolean(document.querySelector('#part-tone.comp .pts-table :is(input, select):focus'));
  }
  return id === 'part-device-presets' && requested(id);
}

/** The device whose link is lost, by its URL segment, while its out-of-reach card is shown (no
 * other card is one: a first link, a reload and a fault are not a link coming back), else
 * nothing. */
function lostLink() {
  const view = document.querySelector('#screen .view');
  return view?.querySelector('.card.warn') ? view.dataset.device : undefined;
}

/** After a screen came in: if `lost` (from [`lostLink`] before it) is the same device and its
 * card is gone, with no "Looking for" or passphrase card in its place, the link is back: its dot
 * rings once and a toast says which device it reached. */
function linkBack(core, lost) {
  if (lost === undefined || lost !== shown() || lostLink() !== undefined) return;
  if (document.querySelector('#screen .view')?.querySelector('.card.warn, .card.dim')) return;
  reconnectedAt = performance.now();
  once(document.querySelector('#part-link i'), 'ring');
  const name = document.querySelector('#screen [data-name]')?.dataset.name ?? 'svolder';
  core.frame.toast(`Reconnected to ${name}`);
}

/** The model's parts `names` as one string, out of band, for the elements that are on the page;
 * `given` (`{ name: data }`) says what a part says where it is not what the model holds. */
function partsHtml(core, names, given = {}) {
  const page = core.model.page();
  if (!page.device) return '';
  const device = { ...page.device, ...given };
  const computer = computerView(core);
  return names
    .filter((name) => document.getElementById(idOf(name)))
    .map((name) => PARTS[idOf(name).slice(5)]?.(device, page, { oob: true, computer }) ?? '')
    .join('');
}

/** Swap the parts `names` in out of band; the ones the page lacks are left. */
function swapParts(core, names, given) {
  const html = partsHtml(core, names, given);
  if (html !== '') htmx().swap(document.body, html, { swapStyle: 'none' });
}

/** Draw the part with the id `id` again from `data`, what the model holds of it (`drawNewest`
 * gives the newest copy). */
export const redraw = (core) => (id, data) => {
  const name = id.slice(5).replace(/-/g, '_');
  swapParts(core, [name], { [name]: data });
};

/** What a part says, without the daemon's clock it carries: a part that says the same is not
 * drawn again for a snapshot. */
const stable = (data) => JSON.stringify(data, (key, value) => (key === 'now_ms' ? undefined : value));

/**
 * Put the store's events on the page. `hooks.beforeScreen` runs before a whole screen is swapped
 * in and `hooks.screenIn` after it, where the page follows what the new screen shows. Returns
 * `{ readWaiting, stop }`: `readWaiting` draws the parts whose swap waited for a
 * use that has ended.
 */
export function installLive(core, hooks, signal) {
  const store = core.model;
  const screenEl = () => document.getElementById('screen');
  /** What each swapped part said when it was last drawn, by the model's name. */
  const drawn = new Map();

  /** Note what the shown device's parts `names` say now as what the page shows. */
  function note(names) {
    const device = store.page().device;
    for (const name of names) drawn.set(name, stable(device?.[name]));
  }

  /** Swap the parts `names` in; with `changed`, only those that say something else than what the
   * page shows. */
  function swap(names, changed = false) {
    const device = store.page().device;
    const wanted = changed ? names.filter((name) => stable(device?.[name]) !== drawn.get(name))
      : names;
    swapParts(core, wanted);
    note(wanted);
  }
  note(SWAPPED);

  /** Whether the computer is the side shown. */
  const onComputer = () => computerView(core)?.side === 'computer';

  /** Draw the screen anew. */
  function swapScreen() {
    hooks.beforeScreen();
    const lost = lostLink();
    const page = store.page();
    const view = computerView(core);
    htmx().swap(screenEl(), drawScreen(page, orderOf(effective(page.device, view)), view),
      { swapStyle: 'innerHTML' });
    // The link and the switch live in the header, outside the screen: they are drawn again with it.
    swapParts(core, ['link', 'switch']);
    note(SWAPPED);
    noteComputer();
    linkBack(core, lost);
    hooks.screenIn();
  }

  /** The link says the stream to svolder is being found again while the stream is lost. */
  function paintLost() {
    const pill = document.getElementById('part-link');
    if (!pill) return;
    if (store.state() === 'lost') pill.dataset.said = 'Reconnecting to svolder…';
    else delete pill.dataset.said;
  }

  /** A stream opened or a model came: the first volume reading after it is not a step. */
  function forgetVolume() {
    lastVolume = null;
    burst.end();
  }

  /** What the page was swapped to: the screen when its shape or its device changed, else every
   * part (a stream's snapshot). */
  function modelChanged() {
    forgetVolume();
    const page = store.page();
    const view = document.querySelector('#screen .view');
    const device = page.device;
    const same = view && device && view.dataset.device === page.shown
      && view.dataset.shape === device.shape
      && (view.dataset.side === 'computer') === onComputer();
    if (same) {
      swap(onComputer() ? SWAPPED.filter((name) => !COMPUTER_PARTS.includes(name)) : SWAPPED, true);
      computerChanged();
    } else swapScreen();
    paintLost();
  }

  /** What the computer's parts said when they were last drawn. */
  let drawnComputer = '';
  const computerState = () => {
    const view = computerView(core);
    return stable(view ? { sw: view.sw, tone: view.tone, picked: view.picked, writes: view.writes }
      : null);
  };
  const noteComputer = () => {
    drawnComputer = computerState();
  };

  /** The computer's side changed (a host reading, a value held or settled, a flip): the screen is
   * drawn anew where the side changed, else the knob reads the volume and the parts that say
   * something else are drawn again. */
  function computerChanged() {
    const view = computerView(core);
    const shownView = document.querySelector('#screen .view');
    if (!shownView || !store.page().device) return;
    if ((shownView.dataset.side === 'computer') !== (view?.side === 'computer')) {
      swapScreen();
      return;
    }
    const now = computerState();
    if (now !== drawnComputer) {
      drawnComputer = now;
      swapParts(core, view?.side === 'computer' ? ['switch', 'tone', 'presets'] : ['switch']);
    }
    if (view?.side === 'computer' && view.volume) computerVolume(view.volume);
  }

  /** The knob reads the computer's volume. */
  function computerVolume(volume) {
    const knob = document.querySelector('sv-knob[data-computer]');
    if (!knob) return;
    const data = {
      raw: volume.pct === null ? null : Math.min(volume.pct, 100),
      target: null,
      risk: null,
      cap: null,
      text: volume.pct === null ? '—' : `${volume.pct} %`,
      muted: volume.muted,
      aria: volume.aria,
      unread: volume.pct === null,
      notes: volume.boosted ? [{ text: 'Set above 100 % elsewhere', red: false }] : [],
    };
    knob.update(data);
    effects.level(data.raw === null || data.muted ? 0.06 : knob.fraction(data.raw));
  }

  /** The stream's state: lost or waiting is dimmed and inert; a lost link says so and greys the
   * effects. `wasLost` starts unknown, so the state the mount finds is applied once. */
  let wasLost = null;
  function stateChanged(state) {
    const lost = state === 'lost';
    screenEl()?.toggleAttribute('inert', lost || state === 'waiting');
    // A lost stream is closed: the volume it reads again comes after its `opened` and its `model`,
    // which forget the last one.
    if (lost !== wasLost) {
      wasLost = lost;
      effects.lost(lost);
    }
    paintLost();
  }

  /** A part of the shown device changed. */
  function partChanged({ device, name }) {
    if (device !== shown()) return;
    if (COMPUTER_PARTS.includes(name) && onComputer()) return;
    swap([name]);
    if (name === 'link') paintLost();
  }

  /** A volume reading of the shown device: the knob repaints; a change between two readings is a
   * step, from any front end, and the effects hear it. */
  function volumeRead(data) {
    if ((data.device ?? shown()) !== shown() || onComputer()) return;
    const knob = document.querySelector('sv-knob');
    if (!knob) return;
    knob.update(data);
    effects.level(data.raw === null || data.muted ? 0.06 : knob.fraction(data.raw));
    // A change between two readings is a step, from any front end; the first reading is not.
    const now = { raw: data.raw, lo: knob.lo, hi: knob.hi };
    const moved = effects.stepOf(lastVolume, now);
    const before = lastVolume;
    lastVolume = now;
    if (!moved) return;
    // a rise over one burst of steps, or into the red zone, makes a note more likely
    const rise = burst.add(
      knob.dbOf(before.raw) ?? knob.lowDb, knob.dbOf(data.raw) ?? knob.lowDb,
      moved.direction * moved.size, performance.now() / 1000,
    );
    const enteredRed = knob.risk !== null && before.raw < knob.risk && data.raw >= knob.risk;
    effects.step(moved.direction, moved.size, knob.fraction(data.raw),
      effects.bigRise({ ...rise, enteredRed }));
  }

  function levelRead(data) {
    if ((data.device ?? shown()) !== shown()) return;
    document.querySelector(`sv-slider[data-key="${CSS.escape(data.key)}"]`)?.update(data);
  }

  function metersRead(data) {
    if ((data.device ?? shown()) !== shown()) return;
    const meters = document.querySelector('sv-meters');
    if (!meters) return;
    meters.update(data);
    if (!document.querySelector('sv-knob')) effects.level(meters.louder() ?? 0.06);
    effects.meters(meters.readings());
  }

  function stripRead(data) {
    if ((data.device ?? shown()) === shown()) strip.add(data);
  }

  noteComputer();
  stateChanged(store.state());

  const off = [
    store.on('model', modelChanged),
    store.on('opened', forgetVolume),
    store.on('state', stateChanged),
    store.on('part', partChanged),
    store.on('volume', volumeRead),
    store.on('level', levelRead),
    store.on('meters', metersRead),
    store.on('strip', stripRead),
    store.on('host', () => computerChanged()),
    core.computer.on(() => computerChanged()),
  ];

  // --------------------------------------------------------------------- what a swap keeps

  /** The preset button that has the focus is noted before its list is swapped, to be found again by
   * its name in the new list. */
  function notePresetFocus(event) {
    const target = event.detail.target;
    if (!PRESET_LISTS.includes(target?.id)) return;
    const active = document.activeElement;
    presetFocus = active?.matches?.(`#${target.id} .pre`) ? active.dataset.name : null;
  }

  /** Give the focus back to the preset noted, when the swap left the page without one. */
  function givePresetFocus(event) {
    const id = event.detail.target?.id;
    if (!PRESET_LISTS.includes(id) || presetFocus === null) return;
    const active = document.activeElement;
    if (!active || active === document.body) {
      [...document.querySelectorAll(`#${id} .pre`)]
        .find((button) => button.dataset.name === presetFocus)?.focus({ preventScroll: true });
    }
    presetFocus = null;
  }

  // Without Element.moveBefore a swap of a part that holds a slider, the band editor or Try now
  // would drop that element's pointer capture or focus while it is used: the swap waits, and the
  // part is drawn again when the use ends. A swap that is cancelled puts nothing back, so no focus
  // is owed.
  function beforeOob(event) {
    notePresetFocus(event);
    const id = event.detail.target?.id;
    if (!id) return;
    if (!event.detail.shouldSwap || holds(id, undefined, ownHold)) {
      event.detail.shouldSwap = false;
      refocusing = null;
    } else refocusing = remember(document.activeElement) ?? refocusing;
  }

  /** The parts whose swap waited for a use that has ended are drawn again, from the newest copy. */
  const draw = drawNewest(store, redraw(core));
  function readWaiting() {
    setTimeout(() => {
      if (signal.aborted) return;
      release((id) => {
        refocusing = remember(document.activeElement) ?? refocusing;
        draw(id);
      }, undefined, ownHold);
    }, 0);
  }

  const body = document.body;
  body.addEventListener('htmx:oobBeforeSwap', beforeOob, mine());
  for (const done of ['htmx:oobAfterSwap', 'htmx:afterSwap']) {
    body.addEventListener(done, (event) => {
      givePresetFocus(event);
      // A kept element put back by a swap in a browser without moveBefore has lost the focus.
      refocus(refocusing);
      refocusing = null;
    }, mine());
  }
  for (const end of ['pointerup', 'pointercancel', 'lostpointercapture', 'focusout']) {
    document.addEventListener(end, readWaiting, mine());
  }

  // What a mount keeps between its readings goes with it, so a mount again starts clean.
  signal.addEventListener('abort', () => {
    forgetVolume();
    reconnectedAt = -Infinity;
    refocusing = null;
    presetFocus = null;
  });

  return {
    readWaiting,
    stop() {
      for (const fn of off) fn();
      clear();
    },
  };
}
