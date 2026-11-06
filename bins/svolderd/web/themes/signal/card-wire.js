/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the card of a device out of reach: `Try now` asks the daemon to connect to the shown
// device (a viewer who may not write taps it and the core says why nothing happens), and the seconds
// to the next try count down once a second while the page is on show.

import { STATES } from './words.js';

/** How often the seconds are counted down (ms). */
const TICK_MS = 1000;

/** Wire the card in `root` until `signal` aborts. */
export function wireCard({ root, core, act, signal }) {
  const open = root.querySelector('#sg-open');

  open.addEventListener('click', (event) => {
    if (!event.target.closest?.('[data-act="try"]')) return;
    const device = core.model.shown();
    act.act('connect', true, (actions) => actions.connect({ device })).catch(() => {});
  }, { signal });

  const timer = setInterval(() => {
    if (globalThis.document?.hidden) return;
    for (const el of open.querySelectorAll('.sg-retry[data-secs]')) {
      const left = Math.max(0, Math.ceil(Number(el.dataset.secs) - (Date.now() - Number(el.dataset.drawn)) / 1000));
      const text = STATES.retryIn(left);
      if (el.textContent !== text) el.textContent = text;
    }
  }, TICK_MS);
  signal.addEventListener('abort', () => clearInterval(timer), { once: true });
}
