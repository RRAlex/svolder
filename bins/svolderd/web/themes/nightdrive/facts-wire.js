/* SPDX-FileCopyrightText: 2026 the svolder authors */
/* SPDX-License-Identifier: GPL-3.0-or-later */
// The hands on the sheet's facts: the title that folds them (the choice kept per browser, fold.js) and
// Identify; and the clock that keeps the readings' ages and the Bluetooth strength's age current while the
// sheet is open and the page is on show. Every write goes through `core.actions`; a viewer who may not
// write taps Identify and the core says why nothing happens.

import { SHEET } from './words.js';

/** How often the ages are written again (ms). */
const TICK_MS = 1000;

/** Wire the facts of the sheet (`sheet.wrap`) until `signal` aborts; `live` redraws the sheet when the fold moves. */
export function wireFacts({
  core, live, view, sheet, signal,
}) {
  const { wrap } = sheet;

  wrap.addEventListener('click', (event) => {
    const el = event.target.closest?.('#nd-facts [data-act]');
    if (!el) return;
    if (el.dataset.act === 'facts-fold') {
      view.facts.toggle();
      live.draw('sheet');
    } else if (el.dataset.act === 'identify') {
      core.actions.identify({ device: core.model.shown() });
    }
  }, { signal });

  // ---- the clock ----
  const { clock } = core;
  /** When this page first saw each drawn table, by this browser's clock. */
  const arrived = new WeakMap();
  const seen = (el) => {
    if (!arrived.has(el)) arrived.set(el, Date.now());
    return arrived.get(el);
  };
  const put = (el, text) => {
    if (el.textContent !== text) el.textContent = text;
  };

  function tick() {
    const spec = wrap.querySelector('#nd-spec');
    if (!spec) return;
    const offset = clock.clockOffset(Number(spec.dataset.now), seen(spec));
    const secs = (stamp) => (clock.daemonClock(offset, Date.now()) - Number(stamp)) / 1000;
    for (const cell of wrap.querySelectorAll('#nd-readings td.nd-age[data-heard-at]')) {
      put(cell, SHEET.ago(core.words.ageWords(secs(cell.dataset.heardAt))));
    }
    const strength = spec.querySelector('td[data-fact=Signal][data-heard-at]');
    if (strength) {
      put(strength, core.words.signalText(strength.dataset.head, 'live' in strength.dataset, secs(strength.dataset.heardAt)));
    }
  }

  let timer = 0;
  const watch = core.lifecycle.watchVisible({
    onVisible(shown) {
      clearInterval(timer);
      timer = 0;
      if (shown) {
        tick();
        timer = setInterval(tick, TICK_MS);
      }
    },
  });
  signal.addEventListener('abort', () => {
    clearInterval(timer);
    watch.stop();
  }, { once: true });
}
