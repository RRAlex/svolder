/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The steppers under the computer's table: a - and a + for the frequency, the gain and the Q of the
// point in hand, in the DAC table's own look. A press steps once and, held, repeats at the core's
// pace; each step is the hand's own (`hand.step`: a live write, the value the run reached written
// once as the final one when the hold ends). While it runs the tone part's swap waits
// (`computer.js` holds it for the hand's burst).

import { ember } from './bind.js';
import { off, usable } from './computer-page.js';

/** The hand of the computer's curve, which does the steps (given when the page is installed). */
let hand = null;

/** Whether a run is under way (the page moves what it draws as it does for a drag). */
export const stepping = () => hand?.burst != null;

/** One step of the stepper `button` on the point in hand; a step against its key does nothing. A
 * held stepper's steps are `hold`: its burst ends when the hold does, not on the burst's timer. */
function step(button, hold) {
  const { computer } = ember.core;
  const n = computer.picked();
  if (n === null) return;
  const { param, dir } = button.dataset;
  hand.step(n, param, Number(dir), { hold });
}

/** A press on a stepper steps once and, held, repeats; `true` when the press was one. */
export function pressed(event) {
  const button = event.target.closest?.('#part-tone [data-comp="step"]');
  if (!button) return false;
  if (event.button !== 0 || off(button) || !usable()) return true;
  const { param, dir } = button.dataset;
  const same = `#part-tone [data-comp="step"][data-param="${param}"][data-dir="${dir}"]`;
  ember.core.repeat(event, {
    root: document.body,
    el: button,
    step: (el) => step(el, true),
    find: () => document.querySelector(same),
    live: usable,
    onEnd: () => hand.finishBurst(),
    signal: ember.signal ?? undefined,
  });
  if (!usable()) hand.finishBurst();
  return true;
}

/** A press from the keyboard on a focused stepper: one step, written at once. */
export function pressedByKey(button) {
  step(button);
  hand.finishBurst();
}

/** The hand that does the steps. */
export function installSteps(given) {
  hand = given.hand;
}
