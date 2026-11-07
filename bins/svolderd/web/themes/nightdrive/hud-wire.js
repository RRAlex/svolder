/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The HUD over the scene and its fade. The top line, the volume and the EQ line are drawn from the
// model; after a time with no input they fade out (opacity only: they stay in the accessibility
// tree and the tab order) and any input brings them back at once. A volume that moves without this
// page's own ask shows the volume for 5 s. Trouble (the stream lost, a card, a fault, the page
// reading again) holds the fade off, and trouble arriving while faded brings the top line back.

import { TITLE, statusOf } from '../../core/title.js';
import { eqMarkup, noVolumeMarkup } from './draw/hud.js';
import { outrunVolume } from './draw/volume.js';
import { computerView } from './computer-view.js';
import { eqLine, topLine, volumeOf } from './hud.js';

/** The input that wakes the HUD. */
const WAKERS = ['pointermove', 'pointerdown', 'keydown', 'wheel', 'touchstart', 'focusin'];

/** How long a volume that moved elsewhere shows while the HUD is faded, ms. */
export const SHOW_VOLUME_MS = 5000;

/** How long the HUD (and the dash with it) waits with no input before it fades, seconds. */
export const FADE_AFTER_S = 15;

/** The idle time the person kept, in seconds, 1 to 600, 15 when none or not a number. */
export function fadeAfter(prefs) {
  const kept = prefs?.get('fade-after', FADE_AFTER_S);
  return typeof kept === 'number' && Number.isFinite(kept) ? Math.min(600, Math.max(1, kept)) : FADE_AFTER_S;
}

/**
 * The fade of `root` (its class `nd-faded`) after `after` seconds with none of `WAKERS` on `doc`.
 * `now()` is the clock in ms and `schedule(fn, ms)` runs `fn` later and gives the way to cancel it.
 * Returns `{ wake, hold, showVolume, faded, watch, stop }`: `hold(true)` keeps the HUD up (and shows it) until
 * `hold(false)`, `showVolume(ms)` shows the volume alone for `ms` while the rest is faded, `faded()` says
 * whether it is, `watch(fn)` calls `fn('fade')` when it fades and `fn('wake', event)` at every input that wakes it
 * (`event` is the input, absent for a wake from the code) after the state has changed.
 */
export function createFade({
  after,
  root,
  doc = globalThis.document,
  now = () => globalThis.performance.now(),
  schedule = (fn, ms) => {
    const id = setTimeout(fn, ms);
    return () => clearTimeout(id);
  },
}) {
  const idle = after * 1000;
  const classes = root.classList;
  let last = now();
  let held = false;
  let faded = false;
  let cancel = null;
  let cancelVolume = null;
  const watchers = [];
  const tell = (...args) => watchers.forEach((fn) => fn(...args));

  function endVolume(quick) {
    cancelVolume?.();
    cancelVolume = null;
    if (!classes.contains('nd-vol-on')) return;
    classes.remove('nd-vol-on');
    // the volume goes back quicker than the rest, so its 5 s are over by the 6th
    if (quick) classes.add('nd-vol-back');
  }

  function arm() {
    if (cancel || held || faded) return;
    cancel = schedule(tick, Math.max(0, idle - (now() - last)));
  }

  function tick() {
    cancel = null;
    if (held || faded) return;
    if (now() - last < idle) {
      arm();
      return;
    }
    faded = true;
    classes.remove('nd-vol-back');
    classes.add('nd-faded');
    tell('fade');
  }

  function wake(event) {
    last = now();
    if (faded) {
      faded = false;
      classes.remove('nd-faded', 'nd-vol-back');
      endVolume(false);
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

    watch(fn) {
      watchers.push(fn);
    },

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

    showVolume(ms) {
      if (!faded) return;
      cancelVolume?.();
      classes.remove('nd-vol-back');
      classes.add('nd-vol-on');
      cancelVolume = schedule(() => {
        cancelVolume = null;
        endVolume(true);
      }, ms);
    },

    stop() {
      for (const [type, fn] of listening) doc?.removeEventListener(type, fn, { capture: true });
      cancel?.();
      cancel = null;
      endVolume(false);
      held = false;
      faded = false;
      classes.remove('nd-faded', 'nd-vol-back');
    },
  };
}

/** What stands for a reading of the volume: a change in it is what shows the volume again. */
const keyOf = (v) => (v ? `${v.raw ?? v.text ?? ''}|${Boolean(v.muted)}` : '');

/** How long a volume that the page asked for is its own, ms. */
const OWN_MS = 3000;

/**
 * Wire the HUD in `root` (`.nd-top`, `.nd-vol`, `.nd-eq`) to `core` and `view`. `view.asked` is
 * `{ raw, at }` while this page's own ask for a volume step is waiting (`at` in `performance.now()`
 * ms): the reading that answers it is not another hand's. Returns `{ draw(page), fade, stop() }`.
 */
export function wireHud({
  root, core, view, now = () => globalThis.performance.now(), schedule, doc,
}) {
  const left = root.querySelector('.nd-left');
  const right = root.querySelector('.nd-right');
  const volume = root.querySelector('.nd-vol');
  const eq = root.querySelector('.nd-eq');
  const page = root.querySelector('#nightdrive');
  const fade = createFade({
    after: fadeAfter(core.prefs), root: page, now, ...(schedule ? { schedule } : {}), ...(doc ? { doc } : {}),
  });
  let drawn = { volume: null, eq: null };
  let seen = null;
  let seenComputer = null;

  /** What stands for a reading of the computer's volume while it is the side shown, else nothing. */
  const computerKey = () => {
    if (core.computer.side() !== 'computer') return '';
    const v = core.computer.volumeOf(core.model.page());
    return `${v.pct}|${v.muted}`;
  };

  const put = (el, name, html) => {
    if (drawn[name] === html) return;
    drawn[name] = html;
    el.innerHTML = html;
  };

  function draw(model) {
    const top = topLine(model, view.state);
    left.textContent = top.left;
    right.textContent = top.right;
    right.className = `nd-right${top.tone ? ` ${top.tone}` : ''}`;
    // the verdict's line pushes the EQ line down where the keys stand by the menu button (style.css)
    if (top.right) page.classList.add('nd-verdict');
    else page.classList.remove('nd-verdict');
    const comp = computerView(core);
    const line = eqLine(model, comp);
    put(eq, 'eq', eqMarkup(line));
    eq.hidden = !line;
    const v = volumeOf(model, comp);
    if (v.none) put(volume, 'volume', noVolumeMarkup(v.none));
    else put(volume, 'volume', v.figures ? outrunVolume(v) : '');
    seen ??= keyOf(model.device?.volume);
    seenComputer ??= computerKey();
    fade.hold(statusOf(model, view.state) !== TITLE.connected);
  }

  const off = core.model.on('volume', (data) => {
    const store = core.model;
    if ((data.device ?? store.shown()) !== store.shown()) return;
    const key = keyOf(store.page().device?.volume);
    if (key === seen) return;
    seen = key;
    const asked = view.asked;
    if (asked && asked.raw === data.raw && now() - asked.at < OWN_MS) return;
    fade.showVolume(SHOW_VOLUME_MS);
  });

  // The computer's volume moved by another hand shows the volume again, as the device's does.
  const offHost = core.model.on('host', () => {
    const key = computerKey();
    if (key === seenComputer) return;
    const first = seenComputer === null;
    seenComputer = key;
    if (!first && key !== '') fade.showVolume(SHOW_VOLUME_MS);
  });

  return {
    draw,
    fade,
    stop() {
      off();
      offHost();
      fade.stop();
    },
  };
}
