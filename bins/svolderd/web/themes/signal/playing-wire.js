/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the Now playing bar and the player's open area: the transport buttons press the computer's player through
// `core.actions.press` (a viewer who may not write taps them and the core says why nothing
// happens), and the bar's progress and time move once a second while the player plays and the page
// is on show.

import { moveProgress } from './draw/playing.js';

/** How often the progress moves (ms). */
const TICK_MS = 1000;

/** Wire the Now playing bar in `root` until `signal` aborts. */
export function wirePlaying({ root, core, act, signal }) {
  const bar = root.querySelector('#sg-now');
  const open = root.querySelector('#sg-open');

  // The transport is in the bar, and in the open area while the player is chosen.
  for (const where of [bar, open]) {
    where.addEventListener('click', (event) => {
      const el = event.target.closest?.('[data-act="transport"]');
      if (!el) return;
      act.act('transport', el.dataset.button, (actions) => actions.press(el.dataset.button))
        .catch(() => {});
    }, { signal });
  }

  // A cover the daemon no longer holds is not shown, and leaves no gap.
  for (const where of [bar, open]) {
    where.addEventListener('error', (event) => {
      if (event.target.matches?.('.sg-cover img')) event.target.closest('.sg-cover').hidden = true;
    }, { capture: true, signal });
  }

  const timer = setInterval(() => {
    if (globalThis.document?.hidden) return;
    for (const at of [bar.querySelector('#sg-np'), open.querySelector('#sg-pp')]) {
      if (at) moveProgress(at, core.model.page(), Date.now());
    }
  }, TICK_MS);
  signal.addEventListener('abort', () => clearInterval(timer), { once: true });
}
