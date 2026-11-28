/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// How the page stands (ok, out of reach, lost, reading again, no analysis) and the words of each
// state, the dash's idle fade (`createDashFade`: a class on the page after the idle time, woken by
// any input), and the values the menu's rows keep (Motion, Audio panel, Panel fades, Detail,
// Renderer). The rest is pure: the model, the stream's state, the capture's state and the prefs in;
// words and values out.

import { mayWrite } from '../../core/write.js';

/** The fixed words of the states. */
export const STATES = Object.freeze({
  reconnecting: 'Reconnecting to svolder…',
  waitingLine: 'reading the device again…',
  restingLine: 'Paused while the page is hidden; it reads the device again when shown.',
  tryNow: 'Try now',
  retryIn: (secs) => (secs > 0 ? `next try in ${secs} s` : 'trying again now'),
  noCapture: (why) => `no music analysis: ${why || 'the computer’s audio cannot be read'}`,
});

/** The card the shown device has, or the page's when no device is shown; `null` when none. */
export const heldCard = (page) => page?.device?.card ?? page?.card ?? null;

/**
 * How the page stands: `lost` the stream to the daemon is lost; `wait` the page reads the device
 * again or rests; `unreachable` the device (or the page) has a card that is not a fault; `nocap`
 * the music cannot be analysed (`capture.state` is `unavailable`); else `ok`. A fault on a
 * connected device is `ok` (the controls stay live), and so is a guest (drawn quiet). `capture` is
 * `{ state, why }` as the analysis said it, or `null` before it has said anything.
 */
export function moodOf(page, state, capture) {
  if (state === 'lost') return 'lost';
  if (state === 'waiting' || state === 'resting') return 'wait';
  const card = heldCard(page);
  if (card && card.kind !== 'fault') return 'unreachable';
  if (capture?.state === 'unavailable') return 'nocap';
  return 'ok';
}

/** The quiet line of a mood: what is not as usual, in a few words; empty when nothing is. */
export function quietOf(mood, state, capture) {
  if (mood === 'lost') return STATES.reconnecting;
  if (mood === 'wait') return state === 'resting' ? STATES.restingLine : STATES.waitingLine;
  if (mood === 'nocap') return STATES.noCapture(capture?.why);
  return '';
}

/** Whether the viewer may change nothing: a guest, a page whose writes are off, or a device the
 * model marks inert. */
export const inertOf = (page) => !mayWrite(page) || Boolean(page?.device?.inert);

/** How long the chrome (the dash, the HUD, the panel and the frame's button) waits with no input
 * before it fades away, seconds: 3, and 4 on a phone. */
export const FADE_AFTER_S = 3;
export const FADE_AFTER_PHONE_S = 4;

/** The idle time the person kept, in seconds, 1 to 600; the default (`phone`'s or the desktop's)
 * when none or not a number. */
export function fadeAfter(prefs, phone = false) {
  const base = phone ? FADE_AFTER_PHONE_S : FADE_AFTER_S;
  const kept = prefs?.get?.('fade-after', base);
  return typeof kept === 'number' && Number.isFinite(kept)
    ? Math.min(600, Math.max(1, kept)) : base;
}

/** The rows' values, in the order the menu lists them, and the first of each is the default. */
export const CHOICES = Object.freeze({
  motion: ['full', 'calm', 'still'],
  panel: ['panel', 'spread', 'off'],
  detail: ['auto', 'high', 'low'],
  renderer: ['auto', 'webgpu', 'webgl2'],
});

/** The words of each value in the menu. */
export const CHOICE_WORDS = Object.freeze({
  motion: { full: 'Full', calm: 'Calm', still: 'Still' },
  panel: { panel: 'Panel', spread: 'Spread', off: 'Off' },
  detail: { auto: 'Auto', high: 'High', low: 'Low' },
  renderer: { auto: 'Auto', webgpu: 'WebGPU', webgl2: 'WebGL2' },
});

/** The words of the menu's rows: each group's heading and the one switch. */
export const ROW_WORDS = Object.freeze({
  motion: 'Motion',
  panel: 'Audio panel',
  fades: 'Pin features panel',
  detail: 'Detail',
  renderer: 'Renderer',
  controls: 'Device controls…',
});

/** The value of the pref `name` when it is one of `name`'s values, else `fallback` (the first). */
export function choiceOf(prefs, name, fallback = CHOICES[name][0]) {
  const kept = prefs?.get?.(name, null);
  return CHOICES[name].includes(kept) ? kept : fallback;
}

/** Whether the panel fades with the rest of the chrome: on unless the person pinned it (the menu's
 * `Pin features panel` row keeps `false` here). */
export const panelFades = (prefs) => prefs?.get?.('panel-fades', true) !== false;

/** The audio panel's mode after `mode`, as `A` cycles it: Panel, Spread, Off, Panel. */
export function nextPanel(mode) {
  const list = CHOICES.panel;
  return list[(list.indexOf(mode) + 1) % list.length];
}

/**
 * The prefs of a mount held in memory as well as in storage: a read gives what was set this
 * session even where the browser refused to keep it (a private window, blocked site data), and
 * what the storage holds otherwise. `kept` is the core's prefs (or nothing).
 */
export function withMemory(kept) {
  const memory = new Map();
  return {
    get: (name, fallback) => (memory.has(name)
      ? memory.get(name) : kept?.get?.(name, fallback) ?? fallback),
    set(name, value) {
      memory.set(name, value);
      return kept?.set?.(name, value) ?? false;
    },
    remove(name) {
      memory.delete(name);
      kept?.remove?.(name);
    },
  };
}

/** The input that wakes the dash. */
const WAKERS = ['pointermove', 'pointerdown', 'keydown', 'wheel', 'touchstart', 'focusin'];

/** How often a faded-to-be dash looks again at what keeps it up, ms. */
const LOOK_MS = 500;

/**
 * The fade of `root` (its class `vg-dim`) after `after` seconds with none of `WAKERS` on `doc`.
 * `after` is a number or a function of the moment (the phone's differs). `now()` is the clock in ms and `schedule(fn, ms)` runs `fn` later and gives the way to cancel
 * it. `kept()` says whether something keeps the dash up (a held key, an open list, the focus, a
 * mouse over it): it is asked when the idle time is over. Returns `{ wake, hold, faded, woke,
 * watch, stop }`: `hold(true)` keeps it up (and shows it) until `hold(false)`; `woke(event)` says
 * whether `event` is the input that woke it just now; `watch(fn)` calls `fn('fade')` when it
 * fades and `fn('wake', event)` at each input that wakes it.
 */
export function createDashFade({
  after,
  root,
  doc = globalThis.document,
  now,
  schedule = (fn, ms) => {
    const id = setTimeout(fn, ms);
    return () => clearTimeout(id);
  },
  kept = () => false,
}) {
  const idle = () => (typeof after === 'function' ? after() : after) * 1000;
  let last = now();
  let held = false;
  let faded = false;
  let cancel = null;
  let by = null;
  const watchers = [];
  const tell = (...args) => watchers.forEach((fn) => fn(...args));

  function arm(ms = Math.max(0, idle() - (now() - last))) {
    if (cancel || held || faded) return;
    cancel = schedule(tick, ms);
  }

  function tick() {
    cancel = null;
    if (held || faded) return;
    if (now() - last < idle()) {
      arm();
      return;
    }
    if (kept()) {
      arm(LOOK_MS);
      return;
    }
    faded = true;
    root.classList.add('vg-dim');
    tell('fade');
  }

  function wake(event) {
    last = now();
    if (faded) {
      faded = false;
      by = event ?? null;
      root.classList.remove('vg-dim');
    }
    tell('wake', event);
    arm();
  }

  const listening = WAKERS.map((type) => {
    const fn = (event) => wake(event);
    doc?.addEventListener(type, fn, { capture: true, passive: true });
    return [type, fn];
  });
  arm();

  return {
    wake,
    faded: () => faded,
    woke: (event) => by === event,
    watch: (fn) => watchers.push(fn),
    hold(on) {
      if (on === held) return;
      held = on;
      if (on) {
        cancel?.();
        cancel = null;
        wake();
      } else {
        last = now();
        arm();
      }
    },
    stop() {
      for (const [type, fn] of listening) doc?.removeEventListener(type, fn, { capture: true });
      cancel?.();
      cancel = null;
      held = false;
      faded = false;
      root.classList.remove('vg-dim');
    },
  };
}

