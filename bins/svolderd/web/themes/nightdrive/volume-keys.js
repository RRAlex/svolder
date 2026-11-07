/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The volume from the scene: the wheel over the big volume number and the arrow keys `↑` and `↓`. Both step the volume
// as `−` and `+` do (`nudge`, dash-wire.js: the device's own step, or 1 % of the computer's volume), so the cap, the
// pacing, the refusals and the glide stay where they are.
//
// The wheel steps on a notch over the number; anywhere else on the scene it is the mountains' (tune-wire.js), and the
// first wheel over a number that has faded only wakes the HUD, and a control with the focus does not stop it. The keys
// step when nothing has the keyboard focus, never with the sheet, a question or a menu open; a key held repeats as a held `+` does (the core's pause and pace); a key
// wakes a faded dash like any key. `←` and `→` are not read here, and a control that has the focus keeps its own keys.

import { HOLD_AFTER_MS, HOLD_EVERY_MS } from '../../core/hold.js';

const KEYS = { ArrowUp: 1, ArrowDown: -1 };

/**
 * Wire the wheel and the arrow keys of `page` (`#nightdrive`) to `nudge(delta)` until `signal` aborts. `writable()` says
 * whether a held key may repeat; `wheelSteps` is the core's (`core.drag`: the rolled notch, up positive). Returns `{ end() }`,
 * which ends a key held down.
 */
export function wireVolumeKeys({
  page, nudge, writable, wheelSteps, signal,
}) {
  const doc = page.ownerDocument;
  const win = doc.defaultView;
  let held = null;
  let first = 0;
  let every = 0;
  let rest = 0;

  /** The sheet, a question or the frame's menu is open. */
  const covered = () => Boolean(page.dataset.sheet)
    || doc.querySelector('[aria-modal="true"], #nd-dialog, .fxpop:not([hidden])') !== null;

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

  win.addEventListener('keydown', (event) => {
    const delta = KEYS[event.key];
    if (delta === undefined || event.ctrlKey || event.altKey || event.metaKey || event.shiftKey || event.defaultPrevented) return;
    if (busy()) return;
    event.preventDefault();
    // the system's own repeat is not a second press: the page repeats at its own pace
    if (event.repeat || held === event.key) return;
    end();
    held = event.key;
    nudge(delta);
    first = setTimeout(() => {
      every = setInterval(() => {
        if (!writable() || doc.hidden || busy()) end();
        else nudge(delta);
      }, HOLD_EVERY_MS);
    }, HOLD_AFTER_MS);
  }, { signal });
  win.addEventListener('keyup', (event) => { if (event.key === held) end(); }, { signal, capture: true });
  win.addEventListener('blur', end, { signal });
  doc.addEventListener('visibilitychange', () => { if (doc.hidden) end(); }, { signal });
  signal.addEventListener('abort', end, { once: true });

  // The wheel over the volume's number: up is louder. It is not the keyboard's: a control that has the focus does not
  // stop it. It is marked handled (`preventDefault`, which keeps the mountains' wheel out) but not stopped, so the HUD's
  // and the dash's wake still hear it; this runs first, so a faded number is seen as it was and that wheel only wakes.
  win.addEventListener('wheel', (event) => {
    const figure = page.querySelector('.nd-vol .nd-odb');
    if (!figure || event.ctrlKey || covered()) return;
    const box = figure.getBoundingClientRect();
    const over = event.clientX >= box.left && event.clientX <= box.right && event.clientY >= box.top && event.clientY <= box.bottom;
    if (!over || !page.querySelector('.nd-scene')?.contains(event.target)) return;
    event.preventDefault();
    if (page.classList.contains('nd-faded')) return;
    const got = wheelSteps(rest, event.deltaY, event.deltaMode);
    rest = got.rest;
    if (got.steps !== 0) nudge(got.steps > 0 ? 1 : -1);
  }, { signal, capture: true, passive: false });

  return { end };
}
